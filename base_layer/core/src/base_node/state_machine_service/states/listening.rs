// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fmt::{Display, Formatter},
    ops::Deref,
    time::{Duration, Instant},
};

use log::*;
use num_format::{Locale, ToFormattedString};
use serde::{Deserialize, Serialize};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_crypto::tari_utilities::epoch_time::EpochTime;
use tokio::sync::broadcast;

use crate::{
    base_node::{
        chain_metadata_service::{ChainMetadataEvent, PeerChainMetadata},
        state_machine_service::{
            states::{
                BlockSync,
                DecideNextSync,
                HeaderSyncState,
                StateEvent,
                StateEvent::FatalError,
                StateInfo,
                SyncStatus,
                Waiting,
            },
            BaseNodeStateMachine,
        },
    },
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::listening";

/// The length of time to wait for a propagated block when one block behind before proceeding to sync
const ONE_BLOCK_BEHIND_WAIT_PERIOD: Duration = Duration::from_secs(20);

/// This struct contains the info of the peer, and is used to serialised and deserialised.
#[derive(Serialize, Deserialize)]
pub struct PeerMetadata {
    pub metadata: ChainMetadata,
    pub last_updated: EpochTime,
}

impl PeerMetadata {
    pub fn to_bytes(&self) -> Vec<u8> {
        let size = bincode::serialized_size(self).unwrap();
        let mut buf = Vec::with_capacity(size as usize);
        bincode::serialize_into(&mut buf, self).unwrap(); // this should not fail
        buf
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
/// This struct contains info that is use full for external viewing of state info
pub struct ListeningInfo {
    synced: bool,
}

impl Display for ListeningInfo {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str("Node in listening state\n")
    }
}

impl ListeningInfo {
    /// Creates a new ListeningInfo
    pub const fn new(is_synced: bool) -> Self {
        Self { synced: is_synced }
    }

    pub fn is_synced(&self) -> bool {
        self.synced
    }
}

/// This state listens for chain metadata events received from the liveness and chain metadata service. Based on the
/// received metadata, if it detects that the current node is lagging behind the network it will switch to block sync
/// state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Listening {
    is_synced: bool,
}

impl Listening {
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent {
        info!(target: LOG_TARGET, "Listening for chain metadata updates");
        shared.set_state_info(StateInfo::Listening(ListeningInfo::new(self.is_synced)));
        let mut time_since_better_block = None;
        let mut mdc = vec![];
        log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
        loop {
            let metadata_event = shared.metadata_event_stream.recv().await;
            log_mdc::extend(mdc.clone());
            match metadata_event.as_ref().map(|v| v.deref()) {
                Ok(ChainMetadataEvent::NetworkSilence) => {
                    debug!("NetworkSilence event received");
                    if !self.is_synced {
                        self.is_synced = true;
                        shared.set_state_info(StateInfo::Listening(ListeningInfo::new(true)));
                        debug!(target: LOG_TARGET, "Initial sync achieved");
                    }
                },
                Ok(ChainMetadataEvent::PeerChainMetadataReceived(peer_metadata_list)) => {
                    // Convert a &Vec<..> to a Vec<&..> without copying each element
                    let mut peer_metadata_list = peer_metadata_list.iter().collect::<Vec<_>>();

                    // lets update the peer data from the chain meta data
                    for peer in &peer_metadata_list {
                        let peer_data = PeerMetadata {
                            metadata: peer.claimed_chain_metadata().clone(),
                            last_updated: EpochTime::now(),
                        };
                        // If this fails, its not the end of the world, we just want to keep record of the stats of
                        // the peer
                        let _ = shared
                            .peer_manager
                            .set_peer_metadata(peer.node_id(), 1, peer_data.to_bytes())
                            .await;
                        log_mdc::extend(mdc.clone());
                    }

                    let configured_sync_peers = &shared.config.blockchain_sync_config.forced_sync_peers;
                    if !configured_sync_peers.is_empty() {
                        // If a _forced_ set of sync peers have been specified, ignore other peers when determining if
                        // we're out of sync
                        peer_metadata_list.retain(|p| configured_sync_peers.contains(p.node_id()));
                    };

                    // If ther peer metadata list is empty, there is nothing to do except stay in listening
                    if peer_metadata_list.is_empty() {
                        debug!(
                            target: LOG_TARGET,
                            "No peer metadata to check. Continuing in listening state.",
                        );

                        if !self.is_synced {
                            debug!(target: LOG_TARGET, "Initial sync achieved");
                            self.is_synced = true;
                            shared.set_state_info(StateInfo::Listening(ListeningInfo::new(true)));
                        }
                        continue;
                    }

                    // Find the best network metadata and set of sync peers with the best tip.
                    let best_metadata = match best_claimed_metadata(&peer_metadata_list) {
                        Some(m) => m,
                        None => {
                            debug!(
                                target: LOG_TARGET,
                                "No better metadata advertised for {} peer(s)",
                                peer_metadata_list.len()
                            );
                            continue;
                        },
                    };

                    let local = match shared.db.get_chain_metadata().await {
                        Ok(m) => m,
                        Err(e) => {
                            return FatalError(format!("Could not get local blockchain metadata. {}", e));
                        },
                    };
                    log_mdc::extend(mdc.clone());

                    // If this node is just one block behind, wait for block propagation before
                    // rushing to sync mode
                    if self.is_synced &&
                        best_metadata.height_of_longest_chain() == local.height_of_longest_chain() + 1 &&
                        time_since_better_block
                            .map(|ts: Instant| ts.elapsed() < ONE_BLOCK_BEHIND_WAIT_PERIOD)
                            .unwrap_or(true)
                    {
                        if time_since_better_block.is_none() {
                            time_since_better_block = Some(Instant::now());
                        }
                        debug!(
                            target: LOG_TARGET,
                            "This node is one block behind. Best network metadata is at height {}.",
                            best_metadata.height_of_longest_chain()
                        );
                        continue;
                    }
                    time_since_better_block = None;

                    // If we have configured sync peers, they are already filtered at this point
                    let sync_peers = if configured_sync_peers.is_empty() {
                        select_sync_peers(best_metadata, &peer_metadata_list)
                    } else {
                        peer_metadata_list
                    };

                    let local_metadata = match shared.db.get_chain_metadata().await {
                        Ok(m) => m,
                        Err(e) => {
                            return FatalError(format!("Could not get local blockchain metadata. {}", e));
                        },
                    };
                    log_mdc::extend(mdc.clone());

                    let sync_mode = determine_sync_mode(
                        shared.config.blocks_behind_before_considered_lagging,
                        &local_metadata,
                        best_metadata,
                        sync_peers,
                    );

                    if sync_mode.is_lagging() {
                        return StateEvent::FallenBehind(sync_mode);
                    }

                    if !self.is_synced {
                        self.is_synced = true;
                        shared.set_state_info(StateInfo::Listening(ListeningInfo::new(true)));
                        debug!(target: LOG_TARGET, "Initial sync achieved");
                    }
                },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(target: LOG_TARGET, "Metadata event subscriber lagged by {} item(s)", n);
                },
                Err(broadcast::error::RecvError::Closed) => {
                    debug!(target: LOG_TARGET, "Metadata event subscriber closed");
                    break;
                },
            }
        }

        debug!(
            target: LOG_TARGET,
            "Event listener is complete because liveness metadata and timeout streams were closed"
        );
        StateEvent::UserQuit
    }
}

impl From<Waiting> for Listening {
    fn from(_: Waiting) -> Self {
        Self { is_synced: false }
    }
}

impl From<HeaderSyncState> for Listening {
    fn from(sync: HeaderSyncState) -> Self {
        Self {
            is_synced: sync.is_synced(),
        }
    }
}

impl From<BlockSync> for Listening {
    fn from(sync: BlockSync) -> Self {
        Self {
            is_synced: sync.is_synced(),
        }
    }
}

impl From<DecideNextSync> for Listening {
    fn from(_: DecideNextSync) -> Self {
        Self { is_synced: false }
    }
}

// Finds the set of sync peers that have the best tip on their main chain and have all the data required to update the
// local node.
fn select_sync_peers<'a>(
    best_metadata: &ChainMetadata,
    peer_metadata_list: &[&'a PeerChainMetadata],
) -> Vec<&'a PeerChainMetadata> {
    peer_metadata_list
        .iter()
        // Check if the peer can provide blocks higher than the local tip height
        .filter(|peer| {
            peer.claimed_chain_metadata().best_block() == best_metadata.best_block()
        })
        // &T is a Copy type
        .copied()
        .collect()
}

/// Determine the best metadata claimed from a set of metadata received from the network.
fn best_claimed_metadata<'a>(metadata_list: &[&'a PeerChainMetadata]) -> Option<&'a ChainMetadata> {
    metadata_list
        .iter()
        .map(|c| c.claimed_chain_metadata())
        .fold(None, |best, current| {
            if current.accumulated_difficulty() >= best.map(|cm| cm.accumulated_difficulty()).unwrap_or(0) {
                Some(current)
            } else {
                best
            }
        })
}

/// Given a local and the network chain state respectively, figure out what synchronisation state we should be in.
fn determine_sync_mode(
    blocks_behind_before_considered_lagging: u64,
    local: &ChainMetadata,
    network: &ChainMetadata,
    sync_peers: Vec<&PeerChainMetadata>,
) -> SyncStatus {
    use SyncStatus::*;
    let network_tip_accum_difficulty = network.accumulated_difficulty();
    let local_tip_accum_difficulty = local.accumulated_difficulty();
    if local_tip_accum_difficulty < network_tip_accum_difficulty {
        let local_tip_height = local.height_of_longest_chain();
        let network_tip_height = network.height_of_longest_chain();
        info!(
            target: LOG_TARGET,
            "Our local blockchain accumulated difficulty is a little behind that of the network. We're at block #{} \
             with an accumulated difficulty of {}, and the network chain tip is at #{} with an accumulated difficulty \
             of {}",
            local_tip_height,
            local_tip_accum_difficulty.to_formatted_string(&Locale::en),
            network_tip_height,
            network_tip_accum_difficulty.to_formatted_string(&Locale::en),
        );

        // This is to test the block propagation by delaying lagging.
        if local_tip_height + blocks_behind_before_considered_lagging > network_tip_height &&
            local_tip_height < network_tip_height + blocks_behind_before_considered_lagging
        {
            info!(
                target: LOG_TARGET,
                "While we are behind, we are still within {} blocks of them, so we are staying as listening and \
                 waiting for the propagated blocks",
                blocks_behind_before_considered_lagging
            );
            return UpToDate;
        };

        debug!(
            target: LOG_TARGET,
            "Lagging (local height = {}, network height = {})", local_tip_height, network_tip_height
        );
        Lagging {
            local: local.clone(),
            network: network.clone(),
            sync_peers: sync_peers.into_iter().cloned().map(Into::into).collect(),
        }
    } else {
        info!(
            target: LOG_TARGET,
            "Our blockchain is up-to-date. We're at block {} with an accumulated difficulty of {} and the network \
             chain tip is at {} with an accumulated difficulty of {}",
            local.height_of_longest_chain(),
            local_tip_accum_difficulty.to_formatted_string(&Locale::en),
            network.height_of_longest_chain(),
            network_tip_accum_difficulty.to_formatted_string(&Locale::en),
        );
        UpToDate
    }
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
    use tari_crypto::keys::PublicKey;

    use super::*;

    fn random_node_id() -> NodeId {
        let (_secret_key, public_key) = CommsPublicKey::random_keypair(&mut OsRng);
        NodeId::from_key(&public_key)
    }

    #[test]
    fn sync_peer_selection() {
        let network_tip_height = 5000;
        let block_hash1 = vec![0, 1, 2, 3];
        let block_hash2 = vec![4, 5, 6, 7];
        let accumulated_difficulty1 = 200000;
        let accumulated_difficulty2 = 100000;

        let mut peer_metadata_list = Vec::new();
        let best_network_metadata = best_claimed_metadata(&peer_metadata_list);
        assert!(best_network_metadata.is_none());
        let best_network_metadata = ChainMetadata::empty();
        assert_eq!(best_network_metadata, ChainMetadata::new(0, Vec::new(), 0, 0, 0));
        let sync_peers = select_sync_peers(&best_network_metadata, &peer_metadata_list);
        assert_eq!(sync_peers.len(), 0);

        let node_id1 = random_node_id();
        let node_id2 = random_node_id();
        let node_id3 = random_node_id();
        let node_id4 = random_node_id();
        let node_id5 = random_node_id();
        // Archival node
        let peer1 = PeerChainMetadata::new(
            node_id1.clone(),
            ChainMetadata::new(network_tip_height, block_hash1.clone(), 0, 0, accumulated_difficulty1),
            None,
        );

        // Pruning horizon is to short to sync from
        let peer2 = PeerChainMetadata::new(
            node_id2,
            ChainMetadata::new(
                network_tip_height,
                block_hash1.clone(),
                500,
                5000 - 500,
                accumulated_difficulty1,
            ),
            None,
        );

        let peer3 = PeerChainMetadata::new(
            node_id3.clone(),
            ChainMetadata::new(
                network_tip_height,
                block_hash1.clone(),
                1440,
                5000 - 1440,
                accumulated_difficulty1,
            ),
            None,
        );
        let peer4 = PeerChainMetadata::new(
            node_id4,
            ChainMetadata::new(
                network_tip_height,
                block_hash2,
                2880,
                5000 - 2880,
                accumulated_difficulty2,
            ),
            None,
        );
        // Node running a fork
        let peer5 = PeerChainMetadata::new(
            node_id5.clone(),
            ChainMetadata::new(
                network_tip_height,
                block_hash1.clone(),
                2880,
                5000 - 2880,
                accumulated_difficulty1,
            ),
            None,
        );
        peer_metadata_list.push(&peer1);
        peer_metadata_list.push(&peer2);
        peer_metadata_list.push(&peer3);
        peer_metadata_list.push(&peer4);
        peer_metadata_list.push(&peer5);

        let best_network_metadata = best_claimed_metadata(peer_metadata_list.as_slice()).unwrap();
        assert_eq!(best_network_metadata.height_of_longest_chain(), network_tip_height);
        assert_eq!(best_network_metadata.best_block(), &block_hash1);
        assert_eq!(best_network_metadata.accumulated_difficulty(), accumulated_difficulty1);
        let sync_peers = select_sync_peers(best_network_metadata, &peer_metadata_list);
        assert_eq!(sync_peers.len(), 4);
        sync_peers.iter().find(|p| *p.node_id() == node_id1).unwrap();
        sync_peers.iter().find(|p| *p.node_id() == node_id3).unwrap();
        sync_peers.iter().find(|p| *p.node_id() == node_id5).unwrap();
    }

    #[test]
    fn sync_mode_selection() {
        let local = ChainMetadata::new(0, Vec::new(), 0, 0, 500_000);
        match determine_sync_mode(0, &local, &local, vec![]) {
            SyncStatus::UpToDate => {},
            _ => panic!(),
        }

        let network = ChainMetadata::new(0, Vec::new(), 0, 0, 499_000);
        match determine_sync_mode(0, &local, &network, vec![]) {
            SyncStatus::UpToDate => {},
            _ => panic!(),
        }

        let network = ChainMetadata::new(0, Vec::new(), 0, 0, 500_001);
        match determine_sync_mode(0, &local, &network, vec![]) {
            SyncStatus::Lagging { network: n, .. } => assert_eq!(n, network),
            _ => panic!(),
        }

        let local = ChainMetadata::new(100, Vec::new(), 50, 50, 500_000);
        let network = ChainMetadata::new(150, Vec::new(), 0, 0, 500_001);
        match determine_sync_mode(0, &local, &network, vec![]) {
            SyncStatus::Lagging { network: n, .. } => assert_eq!(n, network),
            _ => panic!(),
        }

        let local = ChainMetadata::new(0, Vec::new(), 50, 50, 500_000);
        let network = ChainMetadata::new(100, Vec::new(), 0, 0, 500_001);
        match determine_sync_mode(0, &local, &network, vec![]) {
            SyncStatus::Lagging { network: n, .. } => assert_eq!(n, network),
            _ => panic!(),
        }

        let local = ChainMetadata::new(99, Vec::new(), 50, 50, 500_000);
        let network = ChainMetadata::new(150, Vec::new(), 0, 0, 500_001);
        match determine_sync_mode(0, &local, &network, vec![]) {
            SyncStatus::Lagging { network: n, .. } => assert_eq!(n, network),
            _ => panic!(),
        }
    }
}
