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

//! The base node state represents the FSM of the base node synchronisation process.
//!
//! ## Starting state
//! The node is in the `Starting`` state when it's first created. After basic internal setup and configuration, it will
//! move to the `Listening` state.
//!
//! ## Listening
//!
//! In this state, we listen for chain tip updates from the network.
//!
//! The liveness service will periodically poll peers to request the chain tip height. If we are more than one block
//! behind the network chain tip, switch to `BlockSync` mode.
//!
//! ## BlockSync
//!
//! The BlockSync process first downloads the headers from the chain tip to the fork height on the local chain. The
//! chain of headers are constructed by first downloading the tip header based on the received best chain metadata and
//! then recursively downloading the previous header using the previous header hash recorded in the header until the
//! current local chain is reached. The next step is to download the individual blocks corresponding to the previously
//! downloaded headers, this is performed in a ascending order from the lowest height to the highest block height until
//! the tip is reached.
//!
//! After we have caught up on the chain, switch to `Listening`.
//!
//! If errors occur, re-request the problematic header or block.
//!
//! Give up after n failures and switch back to `Listening` (if a peer gave an erroneous chain tip and cannot provide
//! the blocks it says it has, we can switch back to `Listening` and try receive blocks passively.
//!
//! Full blocks received while in this state can be stored in the orphan pool until they are needed.
//!
//! ## Shutdown
//!
//! Reject all new requests with a `Shutdown` message, complete current validations / tasks, flush all state if
//! required, and then shutdown.

mod events_and_states;
pub use events_and_states::{BaseNodeState, BlockSyncInfo, StateEvent, StateInfo, StatusInfo, SyncStatus};

mod block_sync;
pub use block_sync::BlockSync;

mod header_sync;
pub use header_sync::HeaderSyncState;

mod sync_decide;
pub use sync_decide::DecideNextSync;

mod horizon_state_sync;
pub use horizon_state_sync::HorizonStateSync;

mod listening;
pub use listening::{Listening, ListeningInfo, PeerMetadata};

mod shutdown_state;
pub use shutdown_state::Shutdown;

mod starting_state;
pub use starting_state::Starting;

mod waiting;
pub use waiting::Waiting;
