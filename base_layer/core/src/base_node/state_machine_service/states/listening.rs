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
    cmp::Ordering,
    collections::HashMap,
    convert::TryFrom,
    fmt::{Display, Formatter},
    ops::Deref,
    sync::Arc,
    time::{Duration, Instant},
};

use log::*;
use primitive_types::U512;
use serde::{Deserialize, Serialize};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_utilities::epoch_time::EpochTime;
use tokio::{
    sync::broadcast,
    time::{MissedTickBehavior, interval},
};

use crate::{
    base_node::{
        chain_metadata_service::{ChainMetadataEvent, PeerChainMetadata},
        state_machine_service::{
            BaseNodeStateMachine,
            states::{
                BlockSync,
                DecideNextSync,
                HeaderSyncState,
                StateEvent,
                StateEvent::FatalError,
                StateInfo,
                SyncStatus,
                Waiting,
                events_and_states,
            },
        },
        sync::SyncPeer,
    },
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::listening";
const CHAIN_STATUS_LOG_INTERVAL: Duration = Duration::from_secs(30);

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

/// Chain tip data used to aggregate peer metadata logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChainStatusTipLog {
    height: u64,
    accumulated_difficulty: U512,
}

impl ChainStatusTipLog {
    fn new(metadata: &ChainMetadata) -> Self {
        Self {
            height: metadata.best_block_height(),
            accumulated_difficulty: metadata.accumulated_difficulty(),
        }
    }
}

impl Display for ChainStatusTipLog {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(fmt, "height={}, diff={}", self.height, self.accumulated_difficulty)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChainStatusPeerLog {
    node_id: String,
    tip: ChainStatusTipLog,
}

impl ChainStatusPeerLog {
    fn new(peer_metadata: &PeerChainMetadata) -> Self {
        Self {
            node_id: peer_metadata.node_id().to_string(),
            tip: ChainStatusTipLog::new(peer_metadata.claimed_chain_metadata()),
        }
    }
}

impl Display for ChainStatusPeerLog {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(fmt, "({}, {})", self.node_id, self.tip)
    }
}

#[derive(Debug, Default)]
struct ChainStatusLog {
    local_tip: Option<ChainStatusTipLog>,
    peers: HashMap<String, ChainStatusPeerLog>,
}

impl ChainStatusLog {
    fn record(&mut self, local: &ChainMetadata, peer_metadata: &PeerChainMetadata) {
        self.local_tip = Some(ChainStatusTipLog::new(local));
        let peer_log = ChainStatusPeerLog::new(peer_metadata);
        self.peers.insert(peer_log.node_id.clone(), peer_log);
    }

    fn summary_message(&self) -> Option<String> {
        let local_tip = self.local_tip?;
        if self.peers.is_empty() {
            return None;
        }

        let mut in_sync_peers = Vec::new();
        let mut lagging_peers = Vec::new();
        let mut ahead_peers = Vec::new();

        for peer in self.peers.values() {
            match peer.tip.accumulated_difficulty.cmp(&local_tip.accumulated_difficulty) {
                Ordering::Greater => ahead_peers.push(peer),
                Ordering::Equal => in_sync_peers.push(peer),
                Ordering::Less => lagging_peers.push(peer),
            }
        }

        if !ahead_peers.is_empty() {
            return Some(format!(
                "We are behind the network ({local_tip}) with peers: {{{}}}",
                Self::format_peer_tips(&ahead_peers)
            ));
        }

        if in_sync_peers.is_empty() {
            return Some(format!(
                "We are ahead ({local_tip}) with 0 in sync peer(s), we have lagging peers {{{}}}",
                Self::format_peer_tips(&lagging_peers)
            ));
        }

        if lagging_peers.is_empty() {
            return Some(format!(
                "We are in sync with the network ({local_tip}) with peers {{{}}}",
                Self::format_peer_ids(&in_sync_peers)
            ));
        }

        Some(format!(
            "We are in sync with the network ({local_tip}) with {} in sync peer(s), we have lagging peers {{{}}}",
            in_sync_peers.len(),
            Self::format_peer_tips(&lagging_peers)
        ))
    }

    fn log_and_clear(&mut self) {
        if let Some(message) = self.summary_message() {
            debug!(target: LOG_TARGET, "{message}");
        }
        self.clear();
    }

    fn clear(&mut self) {
        self.local_tip = None;
        self.peers.clear();
    }

    fn format_peer_tips(peers: &[&ChainStatusPeerLog]) -> String {
        peers.iter().map(|peer| peer.to_string()).collect::<Vec<_>>().join(", ")
    }

    fn format_peer_ids(peers: &[&ChainStatusPeerLog]) -> String {
        peers
            .iter()
            .map(|peer| peer.node_id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Default)]
struct ListeningLoopState {
    time_since_better_block: Option<Instant>,
    initial_sync_counter: u64,
    ahead_of_peers_counter: u64,
    initial_sync_peer_list: HashMap<String, SyncPeer>,
    chain_status_log: ChainStatusLog,
}

/// This state listens for chain metadata events received from the liveness and chain metadata service. Based on the
/// received metadata, if it detects that the current node is lagging behind the network it will switch to block sync
/// state.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Listening {
    is_synced: bool,
    initial_delay_count: u64,
    network_silence: bool,
}

impl Listening {
    pub fn new() -> Self {
        Default::default()
    }

    fn set_synced_response<B: BlockchainBackend + 'static>(&mut self, shared: &mut BaseNodeStateMachine<B>) {
        if !self.is_synced {
            self.is_synced = true;
            self.initial_delay_count = 0;
            self.publish_status_info(shared);
        }
    }

    fn publish_status_info<B: BlockchainBackend + 'static>(&self, shared: &mut BaseNodeStateMachine<B>) {
        shared.set_state_info(StateInfo::Listening(events_and_states::ListeningInfo::new(
            self.is_synced,
            self.initial_delay_count,
            shared.config.initial_sync_peer_count,
            self.network_silence,
        )));
    }

    async fn handle_metadata_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        metadata_event: Result<Arc<ChainMetadataEvent>, broadcast::error::RecvError>,
        state: &mut ListeningLoopState,
    ) -> Option<StateEvent> {
        match metadata_event.as_ref().map(|v| v.deref()) {
            Ok(ChainMetadataEvent::NetworkSilence) => {
                self.network_silence = true;
                self.set_synced_response(shared);
                debug!("NetworkSilence event received");
            },
            Ok(ChainMetadataEvent::PeerChainMetadataReceived(peer_metadata)) => {
                if let Some(state_event) = self.handle_peer_chain_metadata(shared, peer_metadata, state).await {
                    return Some(state_event);
                }
            },
            Err(broadcast::error::RecvError::Lagged(n)) => {
                debug!(target: LOG_TARGET, "Metadata event subscriber lagged by {n} item(s)");
            },
            Err(broadcast::error::RecvError::Closed) => {
                state.chain_status_log.log_and_clear();
                debug!(target: LOG_TARGET, "Metadata event subscriber closed");
                debug!(
                    target: LOG_TARGET,
                    "Event listener is complete because liveness metadata and timeout streams were closed"
                );
                return Some(StateEvent::UserQuit);
            },
        }

        None
    }

    async fn handle_peer_chain_metadata<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        peer_metadata: &PeerChainMetadata,
        state: &mut ListeningLoopState,
    ) -> Option<StateEvent> {
        self.mark_network_active(shared);

        if !self.accepts_peer_metadata(shared, peer_metadata).await {
            return None;
        }

        self.record_peer_metadata(shared, peer_metadata).await;

        if !Self::is_configured_sync_peer(shared, peer_metadata) {
            return None;
        }

        let local_metadata = match Self::get_local_metadata(shared, state).await {
            Ok(metadata) => metadata,
            Err(state_event) => return Some(state_event),
        };

        let mut sync_mode = determine_sync_mode(
            shared.config.blocks_behind_before_considered_lagging,
            &local_metadata,
            peer_metadata,
        );

        Self::promote_to_lagging_after_delay(&mut sync_mode, state, shared.config.time_before_considered_lagging);

        state.chain_status_log.record(&local_metadata, peer_metadata);

        self.update_initial_sync_status(shared, state, &sync_mode);

        // If we have already reached initial sync before, as indicated by the `is_synced` flagged we can
        // immediately return fallen behind with the peer that has a higher pow than us
        if sync_mode.is_lagging() && self.is_synced {
            state.chain_status_log.log_and_clear();
            return Some(StateEvent::FallenBehind(sync_mode));
        }

        self.collect_initial_lagging_peers(shared, state, sync_mode)
    }

    fn mark_network_active<B: BlockchainBackend + 'static>(&mut self, shared: &mut BaseNodeStateMachine<B>) {
        // We received a valid metadata update, so the network is not silent.
        if self.network_silence {
            self.network_silence = false;
            self.publish_status_info(shared);
        }

        // If we are not yet synced, propagate the updated initial-delay info while waiting for ping/pongs.
        if !self.is_synced {
            self.publish_status_info(shared);
        }
    }

    async fn accepts_peer_metadata<B: BlockchainBackend + 'static>(
        &self,
        shared: &BaseNodeStateMachine<B>,
        peer_metadata: &PeerChainMetadata,
    ) -> bool {
        // We already ban the peer based on some previous logic, but this message was already in the
        // pipeline before the ban went into effect.
        match shared.peer_manager.is_peer_banned(peer_metadata.node_id()).await {
            Ok(true) => {
                warn!(
                    target: LOG_TARGET,
                    "Ignoring chain metadata from banned peer {}",
                    peer_metadata.node_id()
                );
                false
            },
            Ok(false) => true,
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Ignoring chain metadata from peer {} due to error: {}",
                    peer_metadata.node_id(), e
                );
                false
            },
        }
    }

    async fn record_peer_metadata<B: BlockchainBackend + 'static>(
        &self,
        shared: &mut BaseNodeStateMachine<B>,
        peer_metadata: &PeerChainMetadata,
    ) {
        let peer_data = PeerMetadata {
            metadata: peer_metadata.claimed_chain_metadata().clone(),
            last_updated: EpochTime::now(),
        };
        // If this fails, it's not the end of the world; we just want to keep peer stats.
        let _old_data = shared
            .peer_manager
            .set_peer_metadata(peer_metadata.node_id(), 1, peer_data.to_bytes())
            .await;
    }

    fn is_configured_sync_peer<B: BlockchainBackend + 'static>(
        shared: &BaseNodeStateMachine<B>,
        peer_metadata: &PeerChainMetadata,
    ) -> bool {
        let configured_sync_peers = &shared.config.blockchain_sync_config.forced_sync_peers;
        configured_sync_peers.is_empty() || configured_sync_peers.contains(peer_metadata.node_id())
    }

    async fn get_local_metadata<B: BlockchainBackend + 'static>(
        shared: &BaseNodeStateMachine<B>,
        state: &mut ListeningLoopState,
    ) -> Result<ChainMetadata, StateEvent> {
        shared.db.get_chain_metadata().await.map_err(|e| {
            state.chain_status_log.log_and_clear();
            FatalError(format!("Could not get local blockchain metadata. {e}"))
        })
    }

    fn promote_to_lagging_after_delay(
        sync_mode: &mut SyncStatus,
        state: &mut ListeningLoopState,
        time_before_considered_lagging: Duration,
    ) {
        // If a stronger chain is known but blocks have not propagated yet, wait before block sync.
        let lagging_sync_mode = if let SyncStatus::BehindButNotYetLagging {
            local,
            network,
            sync_peers,
        } = sync_mode
        {
            let time_since_better_block = state.time_since_better_block.get_or_insert_with(Instant::now);
            if time_since_better_block.elapsed() > time_before_considered_lagging {
                Some(SyncStatus::Lagging {
                    local: local.clone(),
                    network: network.clone(),
                    sync_peers: sync_peers.clone(),
                })
            } else {
                None
            }
        } else if *sync_mode == SyncStatus::UpToDate {
            // We might have gotten up to date via propagation outside of this state, so reset the timer.
            state.time_since_better_block = None;
            None
        } else {
            None
        };

        if let Some(sync_mode_update) = lagging_sync_mode {
            *sync_mode = sync_mode_update;
        }
    }

    fn update_initial_sync_status<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        state: &mut ListeningLoopState,
        sync_mode: &SyncStatus,
    ) {
        if self.is_synced || !sync_mode.is_up_to_date() {
            return;
        }

        state.ahead_of_peers_counter = state.ahead_of_peers_counter.saturating_add(1);
        if state.ahead_of_peers_counter >= shared.config.initial_sync_peer_count {
            self.set_synced_response(shared);
            info!(target: LOG_TARGET, "Initial sync achieved");
        } else {
            info!(
                target: LOG_TARGET,
                "We are ahead of at least {} peers, waiting for more info",
                state.ahead_of_peers_counter
            );
            self.set_synced_response(shared);
        }
    }

    fn collect_initial_lagging_peers<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        state: &mut ListeningLoopState,
        sync_mode: SyncStatus,
    ) -> Option<StateEvent> {
        // if we are lagging and not yet reached initial sync, we delay a bit till we get
        // INITIAL_SYNC_PEER_COUNT metadata updates from peers to ensure we make a better choice of which
        // peer to sync from in the next stages
        if let SyncStatus::Lagging {
            local,
            network,
            sync_peers,
        } = sync_mode
        {
            state.initial_sync_counter = state.initial_sync_counter.saturating_add(1);
            self.initial_delay_count = state.initial_sync_counter;
            for peer in sync_peers {
                let node_id = peer.node_id().to_string();
                state.initial_sync_peer_list.insert(node_id, peer);
            }
            // We use a map here to ensure that we dont wait for even for INITIAL_SYNC_PEER_COUNT different
            // peers
            if state.initial_sync_counter >= shared.config.initial_sync_peer_count {
                state.chain_status_log.log_and_clear();
                // lets return now that we have enough peers to chose from
                return Some(StateEvent::FallenBehind(SyncStatus::Lagging {
                    local,
                    network,
                    sync_peers: std::mem::take(&mut state.initial_sync_peer_list)
                        .into_values()
                        .collect(),
                }));
            }
        }

        None
    }

    #[allow(clippy::too_many_lines)]
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        network_silence: bool,
    ) -> StateEvent {
        info!(target: LOG_TARGET, "Listening for chain metadata updates");

        self.network_silence = network_silence;

        // If the node was previously bootstrapped (had completed initial sync at least once), restore the is_synced
        // flag. This prevents the node from getting stuck after a failed sync attempt: without this, the node would
        // need to collect `initial_sync_peer_count` metadata events before retrying sync, which can take a very long
        // time on networks with few peers (especially when the failed peer is banned).
        if shared.is_bootstrapped() && !self.is_synced {
            debug!(
                target: LOG_TARGET,
                "Restoring is_synced flag from shared bootstrapped state — node was previously synced"
            );
            self.set_synced_response(shared);
        }

        if network_silence {
            self.set_synced_response(shared);
            warn!(
                target: LOG_TARGET,
                "Initial sync achieved based on event 'NetworkSilence'; this may not be true if the entire \
                network in general is slow to respond to pings"
            );
        } else {
            self.publish_status_info(shared);
        }

        let mut state = ListeningLoopState::default();
        let mut chain_status_log_interval = interval(CHAIN_STATUS_LOG_INTERVAL);
        chain_status_log_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        chain_status_log_interval.tick().await;

        loop {
            tokio::select! {
                _ = chain_status_log_interval.tick() => {
                    state.chain_status_log.log_and_clear();
                },
                metadata_event = shared.metadata_event_stream.recv() => {
                    if let Some(state_event) = self.handle_metadata_event(shared, metadata_event, &mut state).await {
                        return state_event;
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
            network_silence: false,
        }
    }
}

impl From<HeaderSyncState> for Listening {
    fn from(sync: HeaderSyncState) -> Self {
        Self {
            is_synced: sync.is_synced(),
            initial_delay_count: 0,
            network_silence: false,
        }
    }
}

impl From<BlockSync> for Listening {
    fn from(sync: BlockSync) -> Self {
        Self {
            is_synced: sync.is_synced(),
            initial_delay_count: 0,
            network_silence: false,
        }
    }
}

impl From<DecideNextSync> for Listening {
    fn from(sync: DecideNextSync) -> Self {
        Self {
            is_synced: sync.is_synced(),
            initial_delay_count: 0,
            network_silence: false,
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
        trace!(
            target: LOG_TARGET,
            "Our local blockchain accumulated difficulty is a little behind that of the network. We're at block #{local_tip_height} \
             with an accumulated difficulty of {local_tip_accum_difficulty}, and the network chain tip is at #{network_tip_height} with an accumulated difficulty \
             of {network_tip_accum_difficulty}"
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
                trace!(
                    target: LOG_TARGET,
                    "While we are behind, we are still within {blocks_behind_before_considered_lagging} blocks of them, so we are staying as listening and \
                     waiting for the propagated blocks"
                );
                return SyncStatus::BehindButNotYetLagging {
                    local: local.clone(),
                    network: network.claimed_chain_metadata().clone(),
                    sync_peers: vec![network.clone().into()],
                };
            };
        }

        trace!(
            target: LOG_TARGET,
            "Lagging (local height = {}, network height = {}, peer = {} ({}))",
            local_tip_height,
            network_tip_height,
            network.node_id(),
            network
                .latency()
                .map(|l| format!("{l:.2?}"))
                .unwrap_or_else(|| "unknown".to_string())
        );
        SyncStatus::Lagging {
            local: local.clone(),
            network: network.claimed_chain_metadata().clone(),
            sync_peers: vec![network.clone().into()],
        }
    } else {
        if local_tip_accum_difficulty.checked_div(2.into()).unwrap_or_default() > network_tip_accum_difficulty {
            // We are ahead of the network, but not by much. We should be in listening mode.
            trace!(
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
        SyncStatus::UpToDate
    }
}

#[cfg(test)]
mod test {

    use primitive_types::U512;
    use tari_common_types::types::FixedHash;
    use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};

    use super::*;

    fn random_node_id() -> NodeId {
        let (_secret_key, public_key) = CommsPublicKey::random_keypair(&mut rand::rng());
        NodeId::from_key(&public_key)
    }

    fn test_block_hash() -> FixedHash {
        FixedHash::from([
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28,
            29, 30, 31,
        ])
    }

    fn chain_metadata(best_block_height: u64, accumulated_difficulty: U512) -> ChainMetadata {
        ChainMetadata::new(best_block_height, test_block_hash(), 0, 0, accumulated_difficulty, 0).unwrap()
    }

    fn peer_chain_metadata(best_block_height: u64, accumulated_difficulty: U512) -> PeerChainMetadata {
        PeerChainMetadata::new(
            random_node_id(),
            chain_metadata(best_block_height, accumulated_difficulty),
            None,
        )
    }

    #[test]
    fn test_determine_sync_mode() {
        const NETWORK_TIP_HEIGHT: u64 = 5000;
        let accumulated_difficulty = U512::from(10000);

        let archival_node = PeerChainMetadata::new(
            random_node_id(),
            chain_metadata(NETWORK_TIP_HEIGHT, accumulated_difficulty),
            None,
        );

        let behind_node = PeerChainMetadata::new(
            random_node_id(),
            chain_metadata(NETWORK_TIP_HEIGHT - 1, accumulated_difficulty - U512::from(1000)),
            None,
        );

        let sync_mode = determine_sync_mode(0, archival_node.claimed_chain_metadata(), &behind_node);
        assert!(sync_mode.is_up_to_date());

        let sync_mode = determine_sync_mode(1, behind_node.claimed_chain_metadata(), &archival_node);
        assert!(sync_mode.is_lagging());

        let sync_mode = determine_sync_mode(2, behind_node.claimed_chain_metadata(), &archival_node);
        assert!(matches!(sync_mode, SyncStatus::BehindButNotYetLagging { .. }));
    }

    #[test]
    fn test_chain_status_log_summarizes_in_sync_with_lagging_peers() {
        let local = chain_metadata(5000, U512::from(10000));
        let in_sync_peer = peer_chain_metadata(5000, U512::from(10000));
        let lagging_peer = peer_chain_metadata(4999, U512::from(9999));
        let lagging_peer_id = lagging_peer.node_id().to_string();
        let mut chain_status_log = ChainStatusLog::default();

        chain_status_log.record(&local, &in_sync_peer);
        chain_status_log.record(&local, &lagging_peer);

        let message = chain_status_log.summary_message().unwrap();
        assert!(message.contains("We are in sync with the network"));
        assert!(message.contains("with 1 in sync peer(s)"));
        assert!(message.contains("lagging peers"));
        assert!(message.contains(&lagging_peer_id));

        chain_status_log.log_and_clear();
        assert!(chain_status_log.summary_message().is_none());
    }

    #[test]
    fn test_chain_status_log_summarizes_ahead_with_lagging_peer_count() {
        let local = chain_metadata(5000, U512::from(10000));
        let lagging_peer = peer_chain_metadata(4999, U512::from(9999));
        let lagging_peer_id = lagging_peer.node_id().to_string();
        let mut chain_status_log = ChainStatusLog::default();

        chain_status_log.record(&local, &lagging_peer);

        let message = chain_status_log.summary_message().unwrap();
        assert!(message.contains("We are ahead"));
        assert!(message.contains("with 0 in sync peer(s)"));
        assert!(message.contains("lagging peers"));
        assert!(message.contains(&lagging_peer_id));
    }

    #[test]
    fn test_chain_status_log_summarizes_all_in_sync_peers() {
        let local = chain_metadata(5000, U512::from(10000));
        let peer = peer_chain_metadata(5000, U512::from(10000));
        let peer_id = peer.node_id().to_string();
        let mut chain_status_log = ChainStatusLog::default();

        chain_status_log.record(&local, &peer);

        let message = chain_status_log.summary_message().unwrap();
        assert!(message.contains("We are in sync with the network"));
        assert!(message.contains("with peers"));
        assert!(message.contains(&peer_id));
        assert!(!message.contains("lagging peers"));
    }

    #[test]
    fn test_chain_status_log_summarizes_behind_network() {
        let local = chain_metadata(5000, U512::from(10000));
        let ahead_peer = peer_chain_metadata(5001, U512::from(10001));
        let ahead_peer_id = ahead_peer.node_id().to_string();
        let mut chain_status_log = ChainStatusLog::default();

        chain_status_log.record(&local, &ahead_peer);

        let message = chain_status_log.summary_message().unwrap();
        assert!(message.contains("We are behind the network"));
        assert!(message.contains(&ahead_peer_id));
    }
}
