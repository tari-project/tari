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
        chain_metadata_service::ChainMetadataEvent,
        comms_interface::{LocalNodeCommsInterface, OutboundNodeCommsInterface},
        state_machine_service::{
            states,
            states::{BaseNodeState, HorizonSyncConfig, StateEvent, StateInfo, StatusInfo, SyncPeerConfig, SyncStatus},
        },
        sync::{BlockSyncConfig, SyncValidators},
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    consensus::ConsensusManager,
    proof_of_work::randomx_factory::RandomXFactory,
};
use futures::{future, future::Either};
use log::*;
use std::{future::Future, sync::Arc};
use tari_comms::{connectivity::ConnectivityRequester, PeerManager};
use tari_shutdown::ShutdownSignal;
use tokio::sync::{broadcast, watch};

const LOG_TARGET: &str = "c::bn::base_node";

/// Configuration for the BaseNodeStateMachine.
#[derive(Clone, Default)]
pub struct BaseNodeStateMachineConfig {
    pub block_sync_config: BlockSyncConfig,
    pub horizon_sync_config: HorizonSyncConfig,
    pub sync_peer_config: SyncPeerConfig,
    pub orphan_db_clean_out_threshold: usize,
    pub pruning_horizon: u64,
    pub max_randomx_vms: usize,
    pub blocks_behind_before_considered_lagging: u64,
}

/// A Tari full node, aka Base Node.
///
/// This service is essentially a finite state machine that synchronises its blockchain state with its peers and
/// then listens for new blocks to add to the blockchain. See the [SynchronizationSate] documentation for more details.
///
/// This struct holds fields that will be used by all the various FSM state instances, including the local blockchain
/// database and hooks to the p2p network
pub struct BaseNodeStateMachine<B: BlockchainBackend> {
    pub(super) db: AsyncBlockchainDb<B>,
    pub(super) local_node_interface: LocalNodeCommsInterface,
    pub(super) _outbound_nci: OutboundNodeCommsInterface,
    pub(super) connectivity: ConnectivityRequester,
    pub(super) peer_manager: Arc<PeerManager>,
    pub(super) metadata_event_stream: broadcast::Receiver<Arc<ChainMetadataEvent>>,
    pub(super) config: BaseNodeStateMachineConfig,
    pub(super) info: StateInfo,
    pub(super) sync_validators: SyncValidators<B>,
    pub(super) consensus_rules: ConsensusManager,
    pub(super) status_event_sender: Arc<watch::Sender<StatusInfo>>,
    pub(super) randomx_factory: RandomXFactory,
    is_bootstrapped: bool,
    event_publisher: broadcast::Sender<Arc<StateEvent>>,
    interrupt_signal: ShutdownSignal,
}

impl<B: BlockchainBackend + 'static> BaseNodeStateMachine<B> {
    /// Instantiate a new Base Node.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: AsyncBlockchainDb<B>,
        local_node_interface: LocalNodeCommsInterface,
        outbound_nci: OutboundNodeCommsInterface,
        connectivity: ConnectivityRequester,
        peer_manager: Arc<PeerManager>,
        metadata_event_stream: broadcast::Receiver<Arc<ChainMetadataEvent>>,
        config: BaseNodeStateMachineConfig,
        sync_validators: SyncValidators<B>,
        status_event_sender: watch::Sender<StatusInfo>,
        event_publisher: broadcast::Sender<Arc<StateEvent>>,
        randomx_factory: RandomXFactory,
        consensus_rules: ConsensusManager,
        interrupt_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            db,
            local_node_interface,
            _outbound_nci: outbound_nci,
            connectivity,
            peer_manager,
            metadata_event_stream,
            config,
            info: StateInfo::StartUp,
            event_publisher,
            status_event_sender: Arc::new(status_event_sender),
            sync_validators,
            randomx_factory,
            is_bootstrapped: false,
            consensus_rules,
            interrupt_signal,
        }
    }

    /// Describe the Finite State Machine for the base node. This function describes _every possible_ state
    /// transition for the node given its current state and an event that gets triggered.
    pub fn transition(&self, state: BaseNodeState, event: StateEvent) -> BaseNodeState {
        use self::{BaseNodeState::*, StateEvent::*, SyncStatus::*};
        match (state, event) {
            (Starting(s), Initialized) => Listening(s.into()),
            (Listening(s), InitialSync) => HeaderSync(s.into()),
            (HeaderSync(_), HeadersSynchronized(conn)) => {
                if self.config.pruning_horizon > 0 {
                    HorizonStateSync(states::HorizonStateSync::with_peer(conn))
                } else {
                    BlockSync(states::BlockSync::with_peer(conn))
                }
            },
            (HeaderSync(s), HeaderSyncFailed) => Waiting(s.into()),
            (HeaderSync(s), NetworkSilence) => Listening(s.into()),
            (HorizonStateSync(s), HorizonStateSynchronized) => BlockSync(s.into()),
            (HorizonStateSync(s), HorizonStateSyncFailure) => Waiting(s.into()),
            (BlockSync(s), BlocksSynchronized) => Listening(s.into()),
            (BlockSync(s), BlockSyncFailed) => Waiting(s.into()),
            (Listening(_), FallenBehind(Lagging(_, sync_peers))) => HeaderSync(sync_peers.into()),
            (Listening(_), FallenBehind(LaggingBehindHorizon(_, sync_peers))) => HeaderSync(sync_peers.into()),
            (Waiting(s), Continue) => Listening(s.into()),
            (_, FatalError(s)) => Shutdown(states::Shutdown::with_reason(s)),
            (_, UserQuit) => Shutdown(states::Shutdown::with_reason("Shutdown initiated by user".to_string())),
            (s, e) => {
                warn!(
                    target: LOG_TARGET,
                    "No state transition occurs for event {:?} in state {}", e, s
                );
                s
            },
        }
    }

    /// This function will publish the current StatusInfo to the channel
    pub fn publish_event_info(&self) {
        let status = StatusInfo {
            bootstrapped: self.is_bootstrapped(),
            state_info: self.info.clone(),
        };

        if let Err(e) = self.status_event_sender.broadcast(status) {
            debug!(target: LOG_TARGET, "Error broadcasting a StatusEvent update: {}", e);
        }
    }

    /// Sets the StatusInfo.
    pub fn set_state_info(&mut self, info: StateInfo) {
        self.info = info;
        if self.info.is_synced() && !self.is_bootstrapped {
            debug!(target: LOG_TARGET, "Node has bootstrapped");
            self.is_bootstrapped = true;
        }
        self.publish_event_info();
    }

    pub fn is_bootstrapped(&self) -> bool {
        self.is_bootstrapped
    }

    /// Start the base node runtime.
    pub async fn run(mut self) {
        use BaseNodeState::*;
        let mut state = Starting(states::Starting);
        loop {
            if let Shutdown(reason) = &state {
                debug!(
                    target: LOG_TARGET,
                    "Base Node state machine is shutting down because {}", reason
                );
                break;
            }

            let interrupt_signal = self.get_interrupt_signal();
            let next_state_future = self.next_state_event(&mut state);

            // Get the next `StateEvent`, returning a `UserQuit` state event if the interrupt signal is triggered
            let next_event = select_next_state_event(interrupt_signal, next_state_future).await;
            // Publish the event on the event bus
            let _ = self.event_publisher.send(Arc::new(next_event.clone()));
            trace!(
                target: LOG_TARGET,
                "Base Node event in State [{}]:  {}",
                state,
                next_event
            );
            state = self.transition(state, next_event);
        }
    }

    /// Processes and returns the next `StateEvent`
    async fn next_state_event(&mut self, state: &mut BaseNodeState) -> StateEvent {
        use states::BaseNodeState::*;
        let shared_state = self;
        match state {
            Starting(s) => s.next_event(shared_state).await,
            HeaderSync(s) => s.next_event(shared_state).await,
            HorizonStateSync(s) => s.next_event(shared_state).await,
            BlockSync(s) => s.next_event(shared_state).await,
            Listening(s) => s.next_event(shared_state).await,
            Waiting(s) => s.next_event().await,
            Shutdown(_) => unreachable!("called get_next_state_event while in Shutdown state"),
        }
    }

    /// Return a copy of the `interrupt_signal` for this node. This is a `ShutdownSignal` future that will be ready when
    /// the node will enter a `Shutdown` state.
    pub fn get_interrupt_signal(&self) -> ShutdownSignal {
        self.interrupt_signal.clone()
    }
}

/// Polls both the interrupt signal and the given future. If the given future `state_fut` is ready first it's value is
/// returned, otherwise if the interrupt signal is triggered, `StateEvent::UserQuit` is returned.
async fn select_next_state_event<F>(interrupt_signal: ShutdownSignal, state_fut: F) -> StateEvent
where F: Future<Output = StateEvent> {
    futures::pin_mut!(state_fut);
    // If future A and B are both ready `future::select` will prefer A
    match future::select(interrupt_signal, state_fut).await {
        Either::Left(_) => StateEvent::UserQuit,
        Either::Right((state, _)) => state,
    }
}
