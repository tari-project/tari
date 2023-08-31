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
    convert::TryFrom,
    fmt::{Display, Formatter},
    ops::Deref,
    time::Instant,
};

use log::*;
use num_format::{Locale, ToFormattedString};
use serde::{Deserialize, Serialize};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_utilities::epoch_time::EpochTime;
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

/// This struct contains the info of the peer, and is used to serialised and deserialised.
#[derive(Serialize, Deserialize)]
pub struct PeerMetadata {
    pub metadata: ChainMetadata,
    pub last_updated: EpochTime,
}

impl PeerMetadata {
    pub fn to_bytes(&self) -> Vec<u8> {
        let size = usize::try_from(bincode::serialized_size(self).unwrap())
            .expect("The serialized size is larger than the platform allows");
        let mut buf = Vec::with_capacity(size);
        bincode::serialize_into(&mut buf, self).unwrap(); // this should not fail
        buf
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
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
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Listening {
    is_synced: bool,
}

impl Listening {
    pub fn new() -> Self {
        Default::default()
    }

    #[allow(clippy::too_many_lines)]
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
                Ok(ChainMetadataEvent::PeerChainMetadataReceived(peer_metadata)) => {
                    match shared.peer_manager.is_peer_banned(peer_metadata.node_id()).await {
                        Ok(true) => {
                            warn!(
                                target: LOG_TARGET,
                                "Ignoring chain metadata from banned peer {}",
                                peer_metadata.node_id()
                            );
                            continue;
                        },
                        Ok(false) => {},
                        Err(e) => {
                            return FatalError(format!("Error checking if peer is banned: {}", e));
                        },
                    }
                    let peer_data = PeerMetadata {
                        metadata: peer_metadata.claimed_chain_metadata().clone(),
                        last_updated: EpochTime::now(),
                    };
                    // If this fails, its not the end of the world, we just want to keep record of the stats of
                    // the peer
                    let _old_data = shared
                        .peer_manager
                        .set_peer_metadata(peer_metadata.node_id(), 1, peer_data.to_bytes())
                        .await;
                    log_mdc::extend(mdc.clone());

                    let configured_sync_peers = &shared.config.blockchain_sync_config.forced_sync_peers;
                    if !configured_sync_peers.is_empty() {
                        // If a _forced_ set of sync peers have been specified, ignore other peers when determining if
                        // we're out of sync
                        if !configured_sync_peers.contains(peer_metadata.node_id()) {
                            continue;
                        }
                    };

                    log_mdc::extend(mdc.clone());

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
                        peer_metadata,
                    );

                    // Generally we will receive a block via incoming blocks, but something might have
                    // happened that we have not synced to them, e.g. our network could have been down.
                    // If we know about a stronger chain, but haven't synced to it, because we didn't get
                    // the blocks propagated to us, or we have a high `blocks_before_considered_lagging`
                    // then we will wait at least `time_before_considered_lagging` before we try to sync
                    // to that new chain. If you want to sync to a new chain immediately, then you can
                    // set this value to 1 second or lower.
                    if let SyncStatus::BehindButNotYetLagging {
                        local,
                        network,
                        sync_peers,
                    } = &sync_mode
                    {
                        if time_since_better_block.is_none() {
                            time_since_better_block = Some(Instant::now());
                        }
                        if time_since_better_block
                            .map(|t| t.elapsed() > shared.config.time_before_considered_lagging)
                            .unwrap()
                        {
                            return StateEvent::FallenBehind(SyncStatus::Lagging {
                                local: local.clone(),
                                network: network.clone(),
                                sync_peers: sync_peers.clone(),
                            });
                        }
                    }

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

/// Given a local and the network chain state respectively, figure out what synchronisation state we should be in.
fn determine_sync_mode(
    blocks_behind_before_considered_lagging: u64,
    local: &ChainMetadata,
    network: &PeerChainMetadata,
) -> SyncStatus {
    use SyncStatus::{Lagging, UpToDate};
    let network_tip_accum_difficulty = network.claimed_chain_metadata().accumulated_difficulty();
    let local_tip_accum_difficulty = local.accumulated_difficulty();
    if local_tip_accum_difficulty < network_tip_accum_difficulty {
        let local_tip_height = local.height_of_longest_chain();
        let network_tip_height = network.claimed_chain_metadata().height_of_longest_chain();
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
        if local_tip_height.saturating_add(blocks_behind_before_considered_lagging) > network_tip_height {
            info!(
                target: LOG_TARGET,
                "While we are behind, we are still within {} blocks of them, so we are staying as listening and \
                 waiting for the propagated blocks",
                blocks_behind_before_considered_lagging
            );
            return SyncStatus::BehindButNotYetLagging {
                local: local.clone(),
                network: network.claimed_chain_metadata().clone(),
                sync_peers: vec![network.clone().into()],
            };
        };

        debug!(
            target: LOG_TARGET,
            "Lagging (local height = {}, network height = {}, peer = {} ({}))",
            local_tip_height,
            network_tip_height,
            network.node_id(),
            network
                .latency()
                .map(|l| format!("{:.2?}", l))
                .unwrap_or_else(|| "unknown".to_string())
        );
        Lagging {
            local: local.clone(),
            network: network.claimed_chain_metadata().clone(),
            sync_peers: vec![network.clone().into()],
        }
    } else {
        debug!(
            target: LOG_TARGET,
            "{} We're at block {} with an accumulated difficulty of {} and the network chain tip is at {} with an \
             accumulated difficulty of {}",
            if local_tip_accum_difficulty > network_tip_accum_difficulty {
                "Our blockchain is ahead of the network."
            } else {
                // Equals
                "Our blockchain is up-to-date."
            },
            local.height_of_longest_chain(),
            local_tip_accum_difficulty.to_formatted_string(&Locale::en),
            network.claimed_chain_metadata().height_of_longest_chain(),
            network_tip_accum_difficulty.to_formatted_string(&Locale::en),
        );
        UpToDate
    }
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_common_types::types::FixedHash;
    use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
    use tari_crypto::keys::PublicKey;

    use super::*;

    fn random_node_id() -> NodeId {
        let (_secret_key, public_key) = CommsPublicKey::random_keypair(&mut OsRng);
        NodeId::from_key(&public_key)
    }

    #[test]
    fn test_determine_sync_mode() {
        const NETWORK_TIP_HEIGHT: u64 = 5000;
        let block_hash = FixedHash::from([
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28,
            29, 30, 31,
        ]);
        const ACCUMULATED_DIFFICULTY: u128 = 10000;

        let archival_node = PeerChainMetadata::new(
            random_node_id(),
            ChainMetadata::new(NETWORK_TIP_HEIGHT, block_hash, 0, 0, ACCUMULATED_DIFFICULTY, 0),
            None,
        );

        let behind_node = PeerChainMetadata::new(
            random_node_id(),
            ChainMetadata::new(
                NETWORK_TIP_HEIGHT - 1,
                block_hash,
                0,
                0,
                ACCUMULATED_DIFFICULTY - 1000,
                0,
            ),
            None,
        );

        let sync_mode = determine_sync_mode(0, archival_node.claimed_chain_metadata(), &behind_node);
        assert!(sync_mode.is_up_to_date());

        let sync_mode = determine_sync_mode(1, behind_node.claimed_chain_metadata(), &archival_node);
        assert!(sync_mode.is_lagging());

        let sync_mode = determine_sync_mode(2, behind_node.claimed_chain_metadata(), &archival_node);
        assert!(sync_mode.is_up_to_date());
    }
}
