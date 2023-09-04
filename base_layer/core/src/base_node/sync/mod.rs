//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

#[cfg(feature = "base_node")]
pub mod ban;

#[cfg(feature = "base_node")]
mod config;
#[cfg(feature = "base_node")]
pub use self::config::BlockchainSyncConfig;

#[cfg(feature = "base_node")]
mod block_sync;
#[cfg(feature = "base_node")]
pub use block_sync::{BlockSyncError, BlockSynchronizer};

#[cfg(feature = "base_node")]
mod header_sync;
#[cfg(feature = "base_node")]
pub use header_sync::{BlockHeaderSyncError, HeaderSynchronizer};

#[cfg(feature = "base_node")]
mod horizon_state_sync;
#[cfg(feature = "base_node")]
pub use horizon_state_sync::{HorizonStateSynchronization, HorizonSyncError, HorizonSyncInfo, HorizonSyncStatus};

#[cfg(feature = "base_node")]
mod hooks;

#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
pub mod rpc;

#[cfg(feature = "base_node")]
mod sync_peer;
#[cfg(feature = "base_node")]
pub use sync_peer::SyncPeer;

#[cfg(feature = "base_node")]
mod validators;
#[cfg(feature = "base_node")]
pub use validators::SyncValidators;
