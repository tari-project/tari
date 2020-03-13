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

use crate::chain_storage::ChainMetadata;
use std::fmt::{Display, Error, Formatter};

/// The base node state represents the FSM of the base node synchronisation process.
///
/// ## Starting state
/// The node is in the `Starting`` state when it's first created. After basic internal setup and configuration, it will
/// move to the `Listening` state.
///
/// ## Listening
///
/// In this state, we listen for chain tip updates from the network.
///
/// The liveness service will periodically poll peers to request the chain tip height. If we are more than one block
/// behind the network chain tip, switch to `BlockSync` mode.
///
/// ## BlockSync
///
/// The BlockSync process first downloads the headers from the chain tip to the fork height on the local chain. The
/// chain of headers are constructed by first downloading the tip header based on the received best chain metadata and
/// then recursively downloading the previous header using the previous header hash recorded in the header until the
/// current local chain is reached. The next step is to download the individual blocks corresponding to the previously
/// downloaded headers, this is performed in a ascending order from the lowest height to the highest block height until
/// the tip is reached.
///
/// After we have caught up on the chain, switch to `Listening`.
///
/// If errors occur, re-request the problematic header or block.
///
/// Give up after n failures and switch back to `Listening` (if a peer gave an erroneous chain tip and cannot provide
/// the blocks it says it has, we can switch back to `Listening` and try receive blocks passively.
///
/// Full blocks received while in this state can be stored in the orphan pool until they are needed.
///
/// ## Shutdown
///
/// Reject all new requests with a `Shutdown` message, complete current validations / tasks, flush all state if
/// required, and then shutdown.

#[derive(Clone, Debug, PartialEq)]
pub enum BaseNodeState {
    Starting(Starting),
    BlockSync(BlockSyncInfo, ChainMetadata), // The best network chain metadata
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
    // We are behind the chain tip.
    Lagging(ChainMetadata),
    UpToDate,
}

impl Display for BaseNodeState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let s = match self {
            Self::Starting(_) => "Initializing",
            Self::BlockSync(_, _) => "Synchronizing blocks",
            Self::Listening(_) => "Listening",
            Self::Shutdown(_) => "Shutting down",
        };
        f.write_str(s)
    }
}

mod block_sync;
mod error;
mod helpers;
mod listening;
mod shutdown_state;
mod starting_state;

pub use block_sync::{BlockSyncConfig, BlockSyncInfo};
pub use listening::{ListeningConfig, ListeningInfo};
pub use shutdown_state::Shutdown;
pub use starting_state::Starting;
