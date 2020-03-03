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

use std::fmt::{Display, Error, Formatter};

/// The base node state represents the FSM of the base node synchronisation process.
///
/// ## Starting state
/// The node is in the `Starting`` state when it's first created. After basic internal setup and configuration, it will
/// move to the `InitialSync` state.
///
/// ## Initial Sync State
/// In this state, we need to obtain
/// i. The height of our chain tip,
/// ii. The height of the chain tip from the network.
///
/// Once these two values are obtained, we can move to the next state:
/// If we're between the genesis block and the network chain tip, switch to `BlockSync`.
/// Otherwise switch to `Listening`
///
/// ## BlockSync
///
/// For each `n` from genesis block + 1 to the network chain tip, submit a request for block `n`. In this state, an
/// entire block is received, and the normal block validation and storage process is followed. The only difference
/// between `BlockSync` and `Listening` is that the former state is actively asking for blocks, while the latter is a
/// passive process.
///
/// After we have caught up on the chain, switch to `Listening`.
///
/// If errors occur, re-request the problematic block.
///
/// Give up after n failures and switch back to `Listening` (if a peer gave an erroneous chain tip and cannot provide
/// the blocks it says it has, we can switch back to `Listening` and try receive blocks passively.
///
/// Full blocks received while in this state can be stored in the orphan pool until they are needed.
///
/// ## Listening
///
/// Passively wait for new blocks to arrive, and process them accordingly.
///
/// Periodically poll peers to request the chain tip height. If we are more than one block behind the network chain
/// tip, switch to `BlockSync` mode.
///
/// ## Shutdown
///
/// Reject all new requests with a `Shutdown` message, complete current validations / tasks, flush all state if
/// required, and then shutdown.
#[derive(Clone, Debug, PartialEq)]
pub enum BaseNodeState {
    Starting(Starting),
    InitialSync(InitialSync),
    BlockSync(BlockSyncInfo),
    Listening(ListeningInfo),
    Shutdown(Shutdown),
}

#[derive(Debug, PartialEq)]
pub enum StateEvent {
    Initialized,
    MetadataSynced(SyncStatus),
    BlocksSynchronized,
    MaxRequestAttemptsReached,
    FallenBehind(SyncStatus),
    NetworkSilence,
    FatalError(String),
    UserQuit,
}

/// Some state transition functions must return `SyncStatus`. The sync status indicates how far behind the network's
/// blockchain the local node is. It can either be very far behind (`BehindHorizon`), in which case we will just
/// synchronise against the pruning horizon; we're somewhat behind (`Lagging`) and need to download the missing
/// blocks to catch up, or we are `UpToDate`.
#[derive(Debug, PartialEq)]
pub enum SyncStatus {
    // We are behind the chain tip. The usize parameter gives the network's chain height.
    Lagging(u64),
    UpToDate,
}

impl Display for BaseNodeState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let s = match self {
            Self::Starting(_) => "Initializing",
            Self::InitialSync(_) => "Synchronizing blockchain metadata",
            Self::BlockSync(_) => "Synchronizing blocks",
            Self::Listening(_) => "Listening",
            Self::Shutdown(_) => "Shutting down",
        };
        f.write_str(s)
    }
}

mod block_sync;
mod error;
mod helpers;
mod initial_sync;
mod listening;
mod shutdown_state;
mod starting_state;

pub use block_sync::{BlockSyncConfig, BlockSyncInfo};
pub use initial_sync::InitialSync;
pub use listening::{ListeningConfig, ListeningInfo};
pub use shutdown_state::Shutdown;
pub use starting_state::Starting;
