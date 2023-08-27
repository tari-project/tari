//  Copyright 2020, The Taiji Project
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

//! # Connectivity
//! The ConnectivityManager actor is responsible for tracking the state of all peer
//! connections in the system and maintaining a _pool_ of active peer connections.
//!
//! It emits [ConnectivityEvent](crate::connectivity::ConnectivityEvent)s that can keep client components
//! in the loop with the state of the node's connectivity.

mod connection_stats;

mod config;
pub use config::ConnectivityConfig;

mod connection_pool;

mod error;
pub use error::ConnectivityError;

mod manager;
pub(crate) use manager::ConnectivityManager;
pub use manager::ConnectivityStatus;

#[cfg(feature = "metrics")]
mod metrics;

mod requester;
pub(crate) use requester::ConnectivityRequest;
pub use requester::{ConnectivityEvent, ConnectivityEventRx, ConnectivityEventTx, ConnectivityRequester};

mod selection;
pub use selection::ConnectivitySelection;

#[cfg(test)]
mod test;
