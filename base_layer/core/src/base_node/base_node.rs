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
        states,
        states::{BaseNodeState, StateEvent, StateEvent::FatalError},
        BaseNodeConfig,
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase},
};
use log::*;

const TARGET: &str = "core::base_node";

/// A Tari full node, aka Base Node.
///
/// The Base Node is essentially a finite state machine that synchronises its blockchain state with its peers and
/// then listens for new blocks to add to the blockchain
///
/// ## Initial Synchronisation
/// First there's the initial sync from the horizon point. This has multiple steps
/// 1. All block headers to the horizon block are downloaded and validated.
/// 2. All kernels to the horizon are downloaded and validated.
/// 3. Download the UTXO set at the pruning horizon.
///
/// ## Block synchronisation
///
/// Then there's the sequential block sync. Essentially a series of "NewBlock" commands starting from the horizon
/// block + 1 to the current chain tip. This process is identical to the normal block validation process. You'll
/// receive an entire block from a peer and then try to add to the `BlockchainDB` instance.
///
/// See the [SynchronizationSate] documentation for more details.
pub struct BaseNodeStateMachine<B: BlockchainBackend> {
    state: BaseNodeState<B>,
}

impl<B: BlockchainBackend> BaseNodeStateMachine<B> {
    /// Instantiate a new Base Node.
    ///
    /// ```
    /// use tari_core::{
    ///     base_node::{BaseNodeConfig, BaseNodeStateMachine},
    ///     chain_storage::{BlockchainBackend, BlockchainDatabase, MemoryDatabase},
    ///     types::HashDigest,
    /// };
    ///
    /// let db = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    /// let config = BaseNodeConfig;
    /// let mut node = BaseNodeStateMachine::<MemoryDatabase<HashDigest>>::new(db, config);
    /// node.run();
    /// ```
    pub fn new(db: BlockchainDatabase<B>, config: BaseNodeConfig) -> Self {
        Self {
            state: BaseNodeState::Starting(states::Starting::new(config, db)),
        }
    }

    /// Describe the Finite State Machine for the base node. This function describes _every possible_ state
    /// transition for the node given its current state and an event that gets triggered.
    pub fn transition(state: BaseNodeState<B>, event: StateEvent) -> BaseNodeState<B> {
        use crate::base_node::states::{BaseNodeState::*, StateEvent::*};
        match (state, event) {
            (Starting(s), Initialized) => InitialSync(s.into()),
            (InitialSync(_s), MetadataSynced) => FetchingHorizonState,
            (FetchingHorizonState, HorizonStateFetched) => BlockSync,
            (BlockSync, BlocksSynchronized) => Listening,
            (Listening, FallenBehind) => BlockSync,
            (_, FatalError(s)) => Shutdown(states::Shutdown::with_reason(s)),
            (s, e) => {
                debug!(
                    target: TARGET,
                    "No state transition occurs for event {:?} in state {}", e, s
                );
                s
            },
        }
    }

    /// Start the base node runtime.
    pub fn run(&mut self) {
        use crate::base_node::states::BaseNodeState::*;
        loop {
            // Replace the node state with a dummy state
            let next_event = match &mut self.state {
                Starting(s) => s.next_event(),
                InitialSync(s) => s.next_event(),
                FetchingHorizonState => FatalError("Unimplemented".into()),
                BlockSync => FatalError("Unimplemented".into()),
                Listening => FatalError("Unimplemented".into()),
                Shutdown(_) => break,
                None => unreachable!("Node cannot be in a `None` state"),
            };
            let old_state = std::mem::replace(&mut self.state, BaseNodeState::None);
            self.state = BaseNodeStateMachine::transition(old_state, next_event);
        }
        info!(target: TARGET, "Goodbye!");
    }
}
