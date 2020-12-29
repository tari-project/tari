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

use crate::{
    base_node::{
        chain_metadata_service::{ChainMetadataEvent, PeerChainMetadata},
        state_machine_service::{
            states::{BlockSync, StateEvent, StateEvent::FatalError, StateInfo, SyncStatus, Waiting},
            BaseNodeStateMachine,
        },
        sync::SyncPeers,
    },
    chain_storage::BlockchainBackend,
};
use futures::StreamExt;
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Formatter},
    ops::Deref,
};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_crypto::tari_utilities::epoch_time::EpochTime;
use tokio::sync::broadcast;

const LOG_TARGET: &str = "c::bn::state_machine_service::states::listening";

/// This struct contains the info of the peer, and is used to serialised and deserialised.
#[derive(Serialize, Deserialize)]
pub struct PeerMetadata {
    pub metadata: ChainMetadata,
    pub last_updated: EpochTime,
}

impl PeerMetadata {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
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
    ) -> StateEvent
    {
        let local = match shared.db.get_chain_metadata().await {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("Could not get local blockchain metadata. {}", e.to_string());
                return FatalError(msg);
            },
        };
        // If we do not have any blocks go straight to initial sync
        if local.height_of_longest_chain() == 0 {
            info!(
                target: LOG_TARGET,
                "Chain height is at 0, proceeding directly to initial sync"
            );
            return StateEvent::InitialSync;
        }

        info!(target: LOG_TARGET, "Listening for chain metadata updates");
        shared.set_state_info(StateInfo::Listening(ListeningInfo::new(self.is_synced)));
        while let Some(metadata_event) = shared.metadata_event_stream.next().await {
            match metadata_event.as_ref().map(|v| v.deref()) {
                Ok(ChainMetadataEvent::PeerChainMetadataReceived(peer_metadata_list)) => {
                    if peer_metadata_list.is_empty() {
                        debug!(target: LOG_TARGET, "No peers in chain metadata round");
                        continue;
                    }
                    let mut peer_metadata_list = peer_metadata_list.clone();

                    // lets update the peer data from the chain meta data
                    for peer in &peer_metadata_list {
                        let peer_data = PeerMetadata {
                            metadata: peer.chain_metadata.clone(),
                            last_updated: EpochTime::now(),
                        };
                        // If this fails, its not the end of the world, we just want to keep record of the stats of
                        // the peer
                        let _ = shared
                            .peer_manager
                            .set_peer_metadata(&peer.node_id, 1, peer_data.to_bytes())
                            .await;
                    }

                    let configured_sync_peers = &shared.config.block_sync_config.sync_peers;
                    if !configured_sync_peers.is_empty() {
                        // If a _forced_ set of sync peers have been specified, ignore other peers when determining if
                        // we're out of sync
                        peer_metadata_list.retain(|p| configured_sync_peers.contains(&p.node_id));
                    };

                    // If ther peer metadata list is empty, there is nothing to do except stay in listening
                    if peer_metadata_list.is_empty() {
                        debug!(
                            target: LOG_TARGET,
                            "No peer metadata to check. Continuing in listening state.",
                        );
                        continue;
                    }

                    // Find the best network metadata and set of sync peers with the best tip.
                    let best_metadata = match best_metadata(&peer_metadata_list) {
                        Some(m) => m.clone(),
                        None => {
                            debug!(
                                target: LOG_TARGET,
                                "No best metadata for {} peer(s)",
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

                    let local_tip_height = local.height_of_longest_chain();
                    // If we have configured sync peers, they are already filtered at this point
                    let sync_peers = if configured_sync_peers.is_empty() {
                        select_sync_peers(local_tip_height, &best_metadata, &peer_metadata_list)
                    } else {
                        peer_metadata_list
                    };
                    let sync_mode = determine_sync_mode(&local, best_metadata, sync_peers);

                    if sync_mode.is_lagging() {
                        return StateEvent::FallenBehind(sync_mode);
                    } else {
                        if !shared.bootstrapped_sync && sync_mode == SyncStatus::UpToDate {
                            shared.bootstrapped_sync = true;
                            debug!(
                                target: LOG_TARGET,
                                "Initial sync achieved, bootstrap done: {}", sync_mode
                            );
                        }
                        self.is_synced = true;
                        shared.set_state_info(StateInfo::Listening(ListeningInfo::new(true)));
                    }
                },
                Err(broadcast::RecvError::Lagged(n)) => {
                    debug!(target: LOG_TARGET, "Metadata event subscriber lagged by {} item(s)", n);
                },
                Err(broadcast::RecvError::Closed) => {
                    // This should never happen because the while loop exits when the stream ends
                    debug!(target: LOG_TARGET, "Metadata event subscriber closed");
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
        Default::default()
    }
}

impl From<BlockSync> for Listening {
    fn from(_: BlockSync) -> Self {
        Default::default()
    }
}

// Finds the set of sync peers that have the best tip on their main chain and have all the data required to update the
// local node.
fn select_sync_peers(
    local_tip_height: u64,
    best_metadata: &ChainMetadata,
    peer_metadata_list: &[PeerChainMetadata],
) -> Vec<PeerChainMetadata>
{
    peer_metadata_list
        .iter()
        // Check if the peer can provide blocks higher than the local tip height
        .filter(|peer| {
                let peer_horizon_height = peer.chain_metadata.effective_pruned_height();
                local_tip_height >= peer_horizon_height && peer.chain_metadata.best_block() == best_metadata.best_block()
        })
        .cloned()
        .collect()
}

/// Determine the best metadata from a set of metadata received from the network.
fn best_metadata(metadata_list: &[PeerChainMetadata]) -> Option<&ChainMetadata> {
    // TODO: Use heuristics to weed out outliers / dishonest nodes.
    metadata_list.iter().fold(None, |best, current| {
        if current.chain_metadata.accumulated_difficulty() >=
            best.as_ref().map(|cm| cm.accumulated_difficulty()).unwrap_or(0)
        {
            Some(&current.chain_metadata)
        } else {
            best
        }
    })
}

/// Given a local and the network chain state respectively, figure out what synchronisation state we should be in.
fn determine_sync_mode(local: &ChainMetadata, network: ChainMetadata, sync_peers: SyncPeers) -> SyncStatus {
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
            local_tip_accum_difficulty,
            network_tip_height,
            network_tip_accum_difficulty,
        );

        let network_horizon_block = local.horizon_block(network_tip_height);
        if local_tip_height < network_horizon_block {
            debug!(
                target: LOG_TARGET,
                "Lagging behind horizon ({} sync peer(s))",
                sync_peers.len()
            );
            LaggingBehindHorizon(network, sync_peers)
        } else {
            debug!(target: LOG_TARGET, "Lagging ({} sync peer(s))", sync_peers.len());
            Lagging(network, sync_peers)
        }
    } else {
        info!(
            target: LOG_TARGET,
            "Our blockchain is up-to-date. We're at block {} with an accumulated difficulty of {} and the network \
             chain tip is at {} with an accumulated difficulty of {}",
            local.height_of_longest_chain(),
            local_tip_accum_difficulty,
            network.height_of_longest_chain(),
            network_tip_accum_difficulty,
        );
        UpToDate
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::rngs::OsRng;
    use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
    use tari_crypto::keys::PublicKey;

    fn random_node_id() -> NodeId {
        let (_secret_key, public_key) = CommsPublicKey::random_keypair(&mut OsRng);
        NodeId::from_key(&public_key).unwrap()
    }

    #[test]
    fn sync_peer_selection() {
        let local_tip_height: u64 = 4000;
        let network_tip_height = 5000;
        let block_hash1 = vec![0, 1, 2, 3];
        let block_hash2 = vec![4, 5, 6, 7];
        let accumulated_difficulty1 = 200000;
        let accumulated_difficulty2 = 100000;

        let mut peer_metadata_list = Vec::<PeerChainMetadata>::new();
        let best_network_metadata = best_metadata(peer_metadata_list.as_slice());
        assert!(best_network_metadata.is_none());
        let best_network_metadata = ChainMetadata::empty();
        assert_eq!(
            best_network_metadata.clone(),
            ChainMetadata::new(0, Vec::new(), 0, 0, 0)
        );
        let sync_peers = select_sync_peers(local_tip_height, &best_network_metadata, &peer_metadata_list);
        assert_eq!(sync_peers.len(), 0);

        let node_id1 = random_node_id();
        let node_id2 = random_node_id();
        let node_id3 = random_node_id();
        let node_id4 = random_node_id();
        let node_id5 = random_node_id();
        let peer1 = PeerChainMetadata::new(
            node_id1.clone(),
            ChainMetadata::new(network_tip_height, block_hash1.clone(), 0, 0, accumulated_difficulty1),
        ); // Archival node
        let peer2 = PeerChainMetadata::new(
            node_id2,
            ChainMetadata::new(
                network_tip_height,
                block_hash1.clone(),
                500,
                5000 - 500,
                accumulated_difficulty1,
            ),
        ); // Pruning horizon is to short to sync from
        let peer3 = PeerChainMetadata::new(
            node_id3.clone(),
            ChainMetadata::new(
                network_tip_height,
                block_hash1.clone(),
                1440,
                5000 - 1440,
                accumulated_difficulty1,
            ),
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
        ); // Node running a fork
        let peer5 = PeerChainMetadata::new(
            node_id5.clone(),
            ChainMetadata::new(
                network_tip_height,
                block_hash1.clone(),
                2880,
                5000 - 2880,
                accumulated_difficulty1,
            ),
        );
        peer_metadata_list.push(peer1);
        peer_metadata_list.push(peer2);
        peer_metadata_list.push(peer3);
        peer_metadata_list.push(peer4);
        peer_metadata_list.push(peer5);

        let best_network_metadata = best_metadata(peer_metadata_list.as_slice()).unwrap();
        assert_eq!(best_network_metadata.height_of_longest_chain(), network_tip_height);
        assert_eq!(best_network_metadata.best_block(), &block_hash1);
        assert_eq!(best_network_metadata.accumulated_difficulty(), accumulated_difficulty1);
        let sync_peers = select_sync_peers(local_tip_height, &best_network_metadata, &peer_metadata_list);
        assert_eq!(sync_peers.len(), 3);
        sync_peers.iter().find(|p| p.node_id == node_id1).unwrap();
        sync_peers.iter().find(|p| p.node_id == node_id3).unwrap();
        sync_peers.iter().find(|p| p.node_id == node_id5).unwrap();
    }

    #[test]
    fn sync_mode_selection() {
        let local = ChainMetadata::new(0, Vec::new(), 0, 0, 500_000);
        match determine_sync_mode(&local, local.clone(), vec![]) {
            SyncStatus::UpToDate => assert!(true),
            _ => assert!(false),
        }

        let network = ChainMetadata::new(0, Vec::new(), 0, 0, 499_000);
        match determine_sync_mode(&local, network, vec![]) {
            SyncStatus::UpToDate => assert!(true),
            _ => assert!(false),
        }

        let network = ChainMetadata::new(0, Vec::new(), 0, 0, 500_001);
        match determine_sync_mode(&local, network.clone(), vec![]) {
            SyncStatus::Lagging(n, _) => assert_eq!(n, network),
            _ => assert!(false),
        }

        let local = ChainMetadata::new(100, Vec::new(), 50, 50, 500_000);
        let network = ChainMetadata::new(150, Vec::new(), 0, 0, 500_001);
        match determine_sync_mode(&local, network.clone(), vec![]) {
            SyncStatus::Lagging(n, _) => assert_eq!(n, network),
            _ => assert!(false),
        }

        let local = ChainMetadata::new(0, Vec::new(), 50, 50, 500_000);
        let network = ChainMetadata::new(100, Vec::new(), 0, 0, 500_001);
        match determine_sync_mode(&local, network.clone(), vec![]) {
            SyncStatus::LaggingBehindHorizon(n, _) => assert_eq!(n, network),
            _ => assert!(false),
        }

        let local = ChainMetadata::new(99, Vec::new(), 50, 50, 500_000);
        let network = ChainMetadata::new(150, Vec::new(), 0, 0, 500_001);
        match determine_sync_mode(&local, network.clone(), vec![]) {
            SyncStatus::LaggingBehindHorizon(n, _) => assert_eq!(n, network),
            _ => assert!(false),
        }
    }
}
