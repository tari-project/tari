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
        comms_interface::OutboundNodeCommsInterface,
        states,
        states::{BaseNodeState, StateEvent},
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase},
};
use bitflags::_core::sync::atomic::AtomicBool;
use log::*;
use std::sync::{atomic::Ordering, Arc};

const LOG_TARGET: &str = "core::base_node";

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
    pub(super) user_stopped: Arc<AtomicBool>,
}

impl<B: BlockchainBackend> BaseNodeStateMachine<B> {
    /// Instantiate a new Base Node.
    pub fn new(db: &BlockchainDatabase<B>, comms: &OutboundNodeCommsInterface) -> Self {
        Self {
            db: db.clone(),
            comms: comms.clone(),
            user_stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Describe the Finite State Machine for the base node. This function describes _every possible_ state
    /// transition for the node given its current state and an event that gets triggered.
    pub fn transition(state: BaseNodeState, event: StateEvent) -> BaseNodeState {
        use crate::base_node::states::{BaseNodeState::*, StateEvent::*, SyncStatus::*};
        match (state, event) {
            (Starting(s), Initialized) => InitialSync(s.into()),
            (InitialSync(s), MetadataSynced(BehindHorizon)) => FetchingHorizonState(s.into()),
            (InitialSync(s), MetadataSynced(Lagging)) => BlockSync(s.into()),
            (InitialSync(s), MetadataSynced(UpToDate)) => Listening(s.into()),
            (FetchingHorizonState(s), HorizonStateFetched) => BlockSync(s.into()),
            (BlockSync(s), BlocksSynchronized) => Listening(s.into()),
            (Listening(s), FallenBehind(BehindHorizon)) => FetchingHorizonState(s.into()),
            (Listening(s), FallenBehind(Lagging)) => BlockSync(s.into()),
            (_, FatalError(s)) => Shutdown(states::Shutdown::with_reason(s)),
            (s, e) => {
                debug!(
                    target: LOG_TARGET,
                    "No state transition occurs for event {:?} in state {}", e, s
                );
                s
            },
        }
    }

    /// Return a copy of the `user_stopped` flag. Setting this to `true` at any time will signal the node runtime to
    /// shutdown.
    pub fn get_interrupt_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.user_stopped)
    }

    /// Start the base node runtime.
    pub async fn run(self) {
        use crate::base_node::states::BaseNodeState::*;
        let mut state = Starting(states::Starting);
        let mut shared_state = self;
        loop {
            let next_event = match &mut state {
                Starting(s) => s.next_event(&mut shared_state).await,
                InitialSync(s) => s.next_event(&mut shared_state).await,
                FetchingHorizonState(s) => s.next_event().await,
                BlockSync(s) => s.next_event().await,
                Listening(s) => s.next_event().await,
                Shutdown(_) => break,
            };
            debug!(
                target: LOG_TARGET,
                "=== Base Node event in State [{}]:  {:?}", state, next_event
            );
            state = BaseNodeStateMachine::<B>::transition(state, next_event);
        }
    }

    /// Checks the value of the interrupt flag and returns a `FatalError` event if the flag is true. Otherwise it
    /// returns the `default` event.
    pub fn check_interrupt(flag: &AtomicBool, default: StateEvent) -> StateEvent {
        if flag.load(Ordering::SeqCst) {
            StateEvent::FatalError("User interrupted".into())
        } else {
            default
        }
    }
}
