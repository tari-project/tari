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
use serde::{Deserialize, Serialize};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms_dht::{event::DhtEvent, DiscoveryPhase};
use tari_utilities::epoch_time::EpochTime;
use tokio::sync::broadcast;

use crate::{
    base_node::{
        chain_metadata_service::{ChainMetadataEvent, PeerChainMetadata},
        state_machine_service::{
            states::{
                events_and_states,
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
/// This struct contains info that is useful for external viewing of state info
pub struct ListeningInfo {
    synced: bool,
    initial_delay_connected_count: u64,
    initial_sync_peer_wait_count: u64,
}

impl Display for ListeningInfo {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str("Node in listening state\n")
    }
}

impl ListeningInfo {
    /// Creates a new ListeningInfo
    pub const fn new(is_synced: bool, initial_delay_connected_count: u64, initial_sync_peer_wait_count: u64) -> Self {
        Self {
            synced: is_synced,
            initial_delay_connected_count,
            initial_sync_peer_wait_count,
        }
    }

    pub fn is_synced(&self) -> bool {
        self.synced
    }

    pub fn initial_delay_connected_count(&self) -> u64 {
        self.initial_delay_connected_count
    }

    pub fn initial_sync_peer_wait_count(&self) -> u64 {
        self.initial_sync_peer_wait_count
    }
}

/// This state listens for chain metadata events received from the liveness and chain metadata service. Based on the
/// received metadata, if it detects that the current node is lagging behind the network it will switch to block sync
/// state.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Listening {
    is_synced: bool,
    initial_delay_count: u64,
}

impl Listening {
    pub fn new() -> Self {
        Default::default()
    }

    fn set_synced_response<B: BlockchainBackend + 'static>(&mut self, shared: &mut BaseNodeStateMachine<B>) {
        if !self.is_synced {
            self.is_synced = true;
            self.initial_delay_count = 0;
            shared.set_state_info(StateInfo::Listening(events_and_states::ListeningInfo::new(
                true,
                0,
                shared.config.initial_sync_peer_count,
            )));
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        network_silence: bool,
    ) -> StateEvent {
        info!(target: LOG_TARGET, "Listening for chain metadata updates");

        if network_silence {
            self.set_synced_response(shared);
            warn!(
                target: LOG_TARGET,
                "Initial sync achieved based on event 'NetworkSilence'; this may not be true if the entire \
                network in general is slow to respond to pings"
            );
        } else {
            // Update ListeningInfo based on current bootstrap state
            let mut current_listening_info = match shared.info.clone() {
                StateInfo::Listening(li) => li,
                // If not in listening state, but entering it, create a fresh one.
                _ => events_and_states::ListeningInfo {
                    synced: self.is_synced,
                    initial_delay_connected_count: self.initial_delay_count,
                    initial_sync_peer_wait_count: shared.config.initial_sync_peer_count,
                    bootstrap_phase: None,
                },
            };

            // Check for any missed DHT events first (to handle timing issues)
            let mut dht_events_check = shared.dht_event_stream.resubscribe();
            info!(target: LOG_TARGET, "[BN SM LISTENING] Checking for any missed DHT bootstrap events before setting up UI state");

            // Try to receive any recent DHT events that might have been published before we started listening
            let mut events_processed = 0;
            loop {
                match dht_events_check.try_recv() {
                    Ok(event_arc) => {
                        events_processed += 1;
                        let event = event_arc.deref();
                        match event {
                            DhtEvent::BootstrapMethodDetermined(method) => {
                                let method_str = format!("{}", method);
                                info!(target: LOG_TARGET, "[BN SM LISTENING] Found missed BootstrapMethodDetermined event: {}", method_str);
                                if method_str == "ExistingPeers" {
                                    info!(target: LOG_TARGET, "[BN SM LISTENING] Processing missed ExistingPeers event - marking bootstrap complete");
                                    shared.set_primary_bootstrap_complete(true);
                                }
                            },
                            DhtEvent::PrimaryBootstrapComplete => {
                                info!(target: LOG_TARGET, "[BN SM LISTENING] Found missed PrimaryBootstrapComplete event - marking bootstrap complete");
                                shared.set_primary_bootstrap_complete(true);
                            },
                            _ => {},
                        }
                    },
                    Err(broadcast::error::TryRecvError::Empty) => {
                        // No more events to process
                        break;
                    },
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        warn!(target: LOG_TARGET, "[BN SM LISTENING] DHT event stream lagged by {} events during startup check", n);
                        // Continue processing in case there are still events available
                    },
                    Err(broadcast::error::TryRecvError::Closed) => {
                        warn!(target: LOG_TARGET, "[BN SM LISTENING] DHT event stream closed during startup check");
                        break;
                    },
                }
            }

            info!(target: LOG_TARGET, "[BN SM LISTENING] Processed {} missed DHT events. Bootstrap complete: {}", events_processed, shared.is_primary_bootstrap_complete);

            if shared.is_primary_bootstrap_complete {
                current_listening_info.bootstrap_phase = None;
                info!(target: LOG_TARGET, "[BN SM LISTENING] Bootstrap already complete - UI will show Listening state");
            } else {
                // Default to round 0 until first DhtEvent updates it
                current_listening_info.bootstrap_phase = Some(events_and_states::BootstrapPhaseInfo {
                    current_round: 0,
                    total_rounds: shared
                        .config
                        .blockchain_sync_config
                        .num_initial_sync_rounds_seed_bootstrap(),
                });
                info!(target: LOG_TARGET, "[BN SM LISTENING] Bootstrap not complete - setting UI to show bootstrap phase 0/{}", shared.config.blockchain_sync_config.num_initial_sync_rounds_seed_bootstrap());
            };

            // Ensure other fields are also current
            current_listening_info.synced = self.is_synced;
            current_listening_info.initial_delay_connected_count = self.initial_delay_count;
            shared.set_state_info(StateInfo::Listening(current_listening_info));
        }

        let mut chain_metadata_events = shared.metadata_event_stream.resubscribe();
        let mut dht_events = shared.dht_event_stream.resubscribe();
        let mut time_since_better_block = None;
        let mut initial_sync_counter = 0; // This seems to track number of peers heard from for initial sync decision.
        let mut ahead_of_peers_counter = 0;
        let mut initial_sync_peer_list = Vec::new();
        loop {
            tokio::select! {
                result = chain_metadata_events.recv() => {
                    match result {
                        Ok(metadata_event_arc) => {
                            let metadata_event = metadata_event_arc.deref();
                            match metadata_event {
                                ChainMetadataEvent::NetworkSilence => {
                                    // Only consider this if not actively bootstrapping via seeds
                                    if shared.is_primary_bootstrap_complete {
                                        self.set_synced_response(shared);
                                        debug!("NetworkSilence event received");
                                    }
                                },
                                ChainMetadataEvent::PeerChainMetadataReceived(peer_metadata) => {
                                    if !shared.is_primary_bootstrap_complete {
                                        // Still bootstrapping, update initial_delay_connected_count for "Waiting for peer data" if bootstrap_phase becomes None *prematurely*
                                        if let StateInfo::Listening(mut li) = shared.info.clone() {
                                            if li.bootstrap_phase.is_none() { // If bootstrap phase is gone BUT primary_bootstrap_complete is false
                                                initial_sync_counter += 1;
                                                self.initial_delay_count = initial_sync_counter;
                                                li.initial_delay_connected_count = self.initial_delay_count;
                                                shared.set_state_info(StateInfo::Listening(li));
                                            }
                                        }
                                        continue; // Skip sync decision logic while bootstrapping
                                    }

                                    let configured_sync_peers = &shared.config.blockchain_sync_config.forced_sync_peers;
                                    if !configured_sync_peers.is_empty() && !configured_sync_peers.contains(peer_metadata.node_id()) {
                                         continue;
                                    };

                                    let local_metadata = match shared.db.get_chain_metadata().await {
                                        Ok(m) => m,
                                        Err(e) => {
                                            return FatalError(format!("Could not get local blockchain metadata. {}", e));
                                        },
                                    };
                                    let mut sync_mode = determine_sync_mode(
                                        shared.config.blocks_behind_before_considered_lagging,
                                        &local_metadata,
                                        peer_metadata,
                                    );
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
                                            .unwrap_or(false)
                                        {
                                            sync_mode = SyncStatus::Lagging {
                                                local: local.clone(),
                                                network: network.clone(),
                                                sync_peers: sync_peers.clone(),
                                            };
                                        }
                                    } else if sync_mode == SyncStatus::UpToDate {
                                            time_since_better_block = None;

                                    } else {
                                        // here for clippy
                                    }
                                    if !self.is_synced && sync_mode.is_up_to_date() {
                                        ahead_of_peers_counter += 1;
                                        if ahead_of_peers_counter >= shared.config.initial_sync_peer_count {
                                            self.set_synced_response(shared);
                                            info!(target: LOG_TARGET, "Initial sync achieved");
                                        } else {
                                            info!(target: LOG_TARGET, "We are ahead of at least {} peers, waiting for more info", ahead_of_peers_counter);
                                            // This call to set_synced_response if is_synced is false might clear the bootstrap_phase too early.
                                            // Ensure set_synced_response only clears bootstrap_phase if bootstrap is truly done.
                                            if shared.is_primary_bootstrap_complete {
                                               self.set_synced_response(shared);
                                            }
                                        }
                                    }
                                    if sync_mode.is_lagging() && self.is_synced && shared.is_primary_bootstrap_complete {
                                        return StateEvent::FallenBehind(sync_mode);
                                    }
                                    if let SyncStatus::Lagging {
                                        local,
                                        network,
                                        sync_peers,
                                    } = sync_mode
                                    {
                                        if shared.is_primary_bootstrap_complete { // Only try to sync if not bootstrapping with seeds
                                            initial_sync_counter += 1;
                                            self.initial_delay_count = initial_sync_counter;
                                             // update the display of "waiting for peer data X/Y"
                                            if let StateInfo::Listening(mut li) = shared.info.clone(){
                                                 if li.bootstrap_phase.is_none() {
                                                    li.initial_delay_connected_count = self.initial_delay_count;
                                                    shared.set_state_info(StateInfo::Listening(li));
                                                 }
                                            }
                                            for peer in sync_peers {
                                                let mut found = false;
                                                for initial_peer in &mut initial_sync_peer_list {
                                                    if *initial_peer == peer {
                                                        found = true;
                                                        *initial_peer = peer.clone();
                                                        break;
                                                    }
                                                }
                                                if !found {
                                                    initial_sync_peer_list.push(peer.clone());
                                                }
                                            }
                                            if initial_sync_counter >= shared.config.initial_sync_peer_count {
                                                return StateEvent::FallenBehind(SyncStatus::Lagging {
                                                    local,
                                                    network,
                                                    sync_peers: initial_sync_peer_list,
                                                });
                                            }
                                        }
                                    }
                                },
                            }
                        },
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(target: LOG_TARGET, "Metadata event subscriber lagged by {} item(s)", n);
                        },
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!(target: LOG_TARGET, "Chain metadata event stream closed");
                            return StateEvent::UserQuit;
                        },
                    }
                },
                result = dht_events.recv() => {
                    match result {
                        Ok(dht_event_arc) => {
                            let dht_event = dht_event_arc.deref();
                            match dht_event {
                                DhtEvent::BootstrapMethodDetermined(method) => {
                                    let method_str = format!("{}", method);
                                    info!(target: LOG_TARGET, "[BN SM LISTENING] Bootstrap method determined by DHT: {}", method_str);
                                    if method_str == "ExistingPeers" {
                                        info!(target: LOG_TARGET, "[BN SM LISTENING] Bootstrap method is ExistingPeers. Marking primary bootstrap as complete.");
                                        shared.set_primary_bootstrap_complete(true);

                                        // Explicitly clear UI state here as well
                                        if let StateInfo::Listening(mut li) = shared.info.clone() {
                                            if li.bootstrap_phase.is_some() {
                                                li.bootstrap_phase = None;
                                                shared.set_state_info(StateInfo::Listening(li));
                                                info!(
                                                    target: LOG_TARGET,
                                                    "[BN SM LISTENING] Bootstrap (ExistingPeers) determined. UI bootstrap_phase cleared explicitly."
                                                );
                                            }
                                        }
                                    }
                                },
                                DhtEvent::NetworkDiscoveryPeersAdded(round_info) => {
                                    if round_info.phase == DiscoveryPhase::SeedStrap && !shared.is_primary_bootstrap_complete {
                                        if let StateInfo::Listening(mut li) = shared.info.clone() {
                                            let total_rounds = round_info.total_rounds.unwrap_or(1).max(1);
                                            let current_round = round_info.round_number.unwrap_or(0).min(total_rounds);
                                            let new_bootstrap_phase = Some(events_and_states::BootstrapPhaseInfo { current_round, total_rounds });
                                            if li.bootstrap_phase != new_bootstrap_phase {
                                                li.bootstrap_phase = new_bootstrap_phase;
                                                shared.set_state_info(StateInfo::Listening(li));
                                            }

                                            // If this event is the final report from SeedStrap (i.e. current_round >= total_rounds)
                                            // and some seed peers were successfully synced from, ensure primary bootstrap is marked complete.
                                            // This acts as a robust way to ensure completion if the PrimaryBootstrapComplete event was missed.
                                            if current_round >= total_rounds && round_info.num_succeeded > 0 {
                                                info!(
                                                    target: LOG_TARGET,
                                                    "SeedStrap phase reporting as complete via NetworkDiscoveryPeersAdded (round {}/{}, {} successful syncs). Marking primary bootstrap complete.",
                                                    current_round, total_rounds, round_info.num_succeeded
                                                );
                                                shared.set_primary_bootstrap_complete(true);
                                            }
                                        }
                                    }
                                },

                                DhtEvent::PrimaryBootstrapComplete => {
                                    info!(
                                        target: LOG_TARGET,
                                        "[BN SM LISTENING] Received DhtEvent::PrimaryBootstrapComplete. Current is_primary_bootstrap_complete = {}",
                                        shared.is_primary_bootstrap_complete
                                    );
                                    shared.set_primary_bootstrap_complete(true);
                                    info!(
                                        target: LOG_TARGET,
                                        "[BN SM LISTENING] Called set_primary_bootstrap_complete(true). UI should now show 'Listening' instead of 'Bootstrapping'"
                                    );
                                },
                                _ => {}
                            }
                        },
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(target: LOG_TARGET, "DHT event subscriber lagged by {} item(s)", n);
                        },
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!(target: LOG_TARGET, "DHT event stream closed");
                            return StateEvent::UserQuit;
                        },
                    }
                },
            }
        }
    }
}

impl From<Waiting> for Listening {
    fn from(_: Waiting) -> Self {
        debug!(target: LOG_TARGET, "Initial sync set to 'false' (from Waiting)");
        Self {
            is_synced: false,
            initial_delay_count: 0,
        }
    }
}

impl From<HeaderSyncState> for Listening {
    fn from(sync: HeaderSyncState) -> Self {
        Self {
            is_synced: sync.is_synced(),
            initial_delay_count: 0,
        }
    }
}

impl From<BlockSync> for Listening {
    fn from(sync: BlockSync) -> Self {
        Self {
            is_synced: sync.is_synced(),
            initial_delay_count: 0,
        }
    }
}

impl From<DecideNextSync> for Listening {
    fn from(sync: DecideNextSync) -> Self {
        Self {
            is_synced: sync.is_synced(),
            initial_delay_count: 0,
        }
    }
}

/// Given a local and the network chain state respectively, figure out what synchronisation state we should be in.
#[allow(clippy::too_many_lines)]
fn determine_sync_mode(
    blocks_behind_before_considered_lagging: u64,
    local: &ChainMetadata,
    network: &PeerChainMetadata,
) -> SyncStatus {
    let network_tip_accum_difficulty = network.claimed_chain_metadata().accumulated_difficulty();
    let local_tip_accum_difficulty = local.accumulated_difficulty();
    if local_tip_accum_difficulty < network_tip_accum_difficulty {
        let local_tip_height = local.best_block_height();
        let network_tip_height = network.claimed_chain_metadata().best_block_height();
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

        // If both the local and remote are pruned mode, we need to ensure that the remote pruning horizon is
        // greater_equal to ours so that we can sync all the data from it. If the remote is a pruned mode, and
        // we only require some data from it, we need to ensure that they can supply the data we need, as in their
        // effective pruned horizon is greater than our local current chain tip.
        let pruned_mode = local.pruning_horizon() > 0;
        let pruning_horizon_check = network.claimed_chain_metadata().pruning_horizon() > 0 &&
            network.claimed_chain_metadata().pruning_horizon() < local.pruning_horizon();
        let pruning_height_check = network.claimed_chain_metadata().pruned_height() > local.best_block_height();
        let sync_able_peer = match (pruned_mode, pruning_horizon_check, pruning_height_check) {
            (true, true, _) => {
                info!(
                    target: LOG_TARGET,
                    "The remote peer is a pruned node, and it's pruning_horizon is less than ours. Remote pruning horizon # {}, current local pruning horizon #{}",
                    network.claimed_chain_metadata(),
                    local.pruning_horizon(),
                );
                false
            },
            (false, _, true) => {
                info!(
                    target: LOG_TARGET,
                    "The remote peer is a pruned node, and it cannot supply the blocks we need. Remote pruned height # {}, current local tip #{}",
                    network.claimed_chain_metadata().pruned_height(),
                    local.best_block_height(),
                );
                false
            },
            _ => true,
        };

        if !sync_able_peer {
            return SyncStatus::SyncNotPossible {
                peers: vec![network.clone().into()],
            };
        }

        // This is to test the block propagation by delaying lagging.
        // If the config is 0, ignore this set.
        if blocks_behind_before_considered_lagging > 0 {
            // Otherwise, only wait when the tip is above us, otherwise
            // chains with a lower height will never be reorged to.
            if network_tip_height > local_tip_height &&
                local_tip_height.saturating_add(blocks_behind_before_considered_lagging) > network_tip_height
            {
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
        }

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
        SyncStatus::Lagging {
            local: local.clone(),
            network: network.claimed_chain_metadata().clone(),
            sync_peers: vec![network.clone().into()],
        }
    } else {
        if local_tip_accum_difficulty / 2 > network_tip_accum_difficulty {
            // We are ahead of the network, but not by much. We should be in listening mode.
            info!(
                target: LOG_TARGET,
                "Received a metadata update from a peer that is very far behind us. Disregarding. We are at block #{} with an \
                 accumulated difficulty of {} and the network chain tip is at #{} with an accumulated difficulty of {}",
                local.best_block_height(),
                local_tip_accum_difficulty,
                network.claimed_chain_metadata().best_block_height(),
                network_tip_accum_difficulty,
            );
            return SyncStatus::SyncNotPossible {
                peers: vec![network.clone().into()],
            };
        }
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
            local.best_block_height(),
            local_tip_accum_difficulty,
            network.claimed_chain_metadata().best_block_height(),
            network_tip_accum_difficulty,
        );
        SyncStatus::UpToDate
    }
}

#[cfg(test)]
mod test {

    use primitive_types::U512;
    use rand::rngs::OsRng;
    use tari_common_types::types::FixedHash;
    use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};

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
        let accumulated_difficulty = U512::from(10000);

        let archival_node = PeerChainMetadata::new(
            random_node_id(),
            ChainMetadata::new(NETWORK_TIP_HEIGHT, block_hash, 0, 0, accumulated_difficulty, 0).unwrap(),
            None,
        );

        let behind_node = PeerChainMetadata::new(
            random_node_id(),
            ChainMetadata::new(
                NETWORK_TIP_HEIGHT - 1,
                block_hash,
                0,
                0,
                accumulated_difficulty - U512::from(1000),
                0,
            )
            .unwrap(),
            None,
        );

        let sync_mode = determine_sync_mode(0, archival_node.claimed_chain_metadata(), &behind_node);
        assert!(sync_mode.is_up_to_date());

        let sync_mode = determine_sync_mode(1, behind_node.claimed_chain_metadata(), &archival_node);
        assert!(sync_mode.is_lagging());

        let sync_mode = determine_sync_mode(2, behind_node.claimed_chain_metadata(), &archival_node);
        assert!(matches!(sync_mode, SyncStatus::BehindButNotYetLagging { .. }));
    }
}
