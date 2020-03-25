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
        comms_interface::OutboundNodeCommsInterface,
        states,
        states::{BaseNodeState, BlockSyncConfig, StateEvent},
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase},
};
use futures::{future, future::Either, SinkExt};
use log::*;
use std::{future::Future, sync::Arc};
use tari_broadcast_channel::{bounded, Publisher, Subscriber};
use tari_comms::{connection_manager::ConnectionManagerRequester, PeerManager};
use tari_shutdown::ShutdownSignal;

const LOG_TARGET: &str = "c::bn::base_node";

/// Configuration for the BaseNodeStateMachine.
#[derive(Clone, Copy)]
pub struct BaseNodeStateMachineConfig {
    pub block_sync_config: BlockSyncConfig,
}

impl Default for BaseNodeStateMachineConfig {
    fn default() -> Self {
        Self {
            block_sync_config: BlockSyncConfig::default(),
        }
    }
}

/// A Tari full node, aka Base Node.
///
/// The Base Node is essentially a finite state machine that synchronises its blockchain state with its peers and
/// then listens for new blocks to add to the blockchain. See the [SynchronizationSate] documentation for more details.
///
/// This struct holds fields that will be used by all the various FSM state instances, including the local blockchain
/// database and hooks to the p2p network
pub struct BaseNodeStateMachine<B: BlockchainBackend> {
    pub(super) db: BlockchainDatabase<B>,
    pub(super) comms: OutboundNodeCommsInterface,
    pub(super) peer_manager: Arc<PeerManager>,
    pub(super) connection_manager: ConnectionManagerRequester,
    pub(super) metadata_event_stream: Subscriber<ChainMetadataEvent>,
    pub(super) config: BaseNodeStateMachineConfig,
    event_sender: Publisher<BaseNodeState>,
    event_receiver: Subscriber<BaseNodeState>,
    interrupt_signal: ShutdownSignal,
}

impl<B: BlockchainBackend + 'static> BaseNodeStateMachine<B> {
    /// Instantiate a new Base Node.
    pub fn new(
        db: &BlockchainDatabase<B>,
        comms: &OutboundNodeCommsInterface,
        peer_manager: Arc<PeerManager>,
        connection_manager: ConnectionManagerRequester,
        metadata_event_stream: Subscriber<ChainMetadataEvent>,
        config: BaseNodeStateMachineConfig,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        let (event_sender, event_receiver): (Publisher<BaseNodeState>, Subscriber<BaseNodeState>) = bounded(1);
        Self {
            db: db.clone(),
            comms: comms.clone(),
            peer_manager,
            connection_manager,
            metadata_event_stream,
            interrupt_signal: shutdown_signal,
            config,
            event_sender,
            event_receiver,
        }
    }

    /// Describe the Finite State Machine for the base node. This function describes _every possible_ state
    /// transition for the node given its current state and an event that gets triggered.
    pub fn transition(state: BaseNodeState, event: StateEvent) -> BaseNodeState {
        use crate::base_node::states::{BaseNodeState::*, StateEvent::*, SyncStatus::*};
        match (state, event) {
            (Starting(s), Initialized) => Listening(s.into()),
            (BlockSync(s, _, _), BlocksSynchronized) => Listening(s.into()),
            (BlockSync(s, _, _), BlockSyncFailure) => Listening(s.into()),
            (Listening(s), FallenBehind(Lagging(network_tip, sync_peers))) => {
                BlockSync(s.into(), network_tip, sync_peers)
            },
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

    /// Start the base node runtime.
    pub async fn run(mut self) {
        use crate::base_node::states::BaseNodeState::*;
        let mut state = Starting(states::Starting);
        loop {
            let _ = self.event_sender.send(state.clone()).await;

            if let Shutdown(reason) = &state {
                debug!(
                    target: LOG_TARGET,
                    "=== Base Node state machine is shutting down because {}", reason
                );
                break;
            }

            let interrupt_signal = self.get_interrupt_signal();
            let next_state_future = self.next_state_event(&mut state);

            // Get the next `StateEvent`, returning a `UserQuit` state event if the interrupt signal is triggered
            let next_event = select_next_state_event(interrupt_signal, next_state_future).await;

            debug!(
                target: LOG_TARGET,
                "=== Base Node event in State [{}]:  {:?}", state, next_event
            );
            state = BaseNodeStateMachine::<B>::transition(state, next_event);
        }
    }

    /// Processes and returns the next `StateEvent`
    async fn next_state_event(&mut self, state: &mut BaseNodeState) -> StateEvent {
        use states::BaseNodeState::*;
        let shared_state = self;
        match state {
            Starting(s) => s.next_event(shared_state).await,
            BlockSync(s, network_tip, sync_peers) => s.next_event(shared_state, network_tip, sync_peers).await,
            Listening(s) => s.next_event(shared_state).await,
            Shutdown(_) => unreachable!("called get_next_state_event while in Shutdown state"),
        }
    }

    /// Return a copy of the `interrupt_signal` for this node. This is a `ShutdownSignal` future that will be ready when
    /// the node will enter a `Shutdown` state.
    pub fn get_interrupt_signal(&self) -> ShutdownSignal {
        self.interrupt_signal.clone()
    }

    /// This clones the receiver end of the channel and gives out a copy to the caller
    /// This allows multiple subscribers to this channel by only keeping one channel and cloning the receiver for every
    /// caller.
    pub fn get_state_change_event_stream(&self) -> Subscriber<BaseNodeState> {
        self.event_receiver.clone()
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
