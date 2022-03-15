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

#[cfg(all(test, feature = "base_node"))]
pub mod test_utils;

#[cfg(feature = "base_node")]
mod config;
#[cfg(feature = "base_node")]
mod consts;
#[cfg(feature = "base_node")]
mod error;
#[cfg(feature = "base_node")]
#[allow(clippy::module_inception)]
mod mempool;
#[cfg(feature = "base_node")]
mod mempool_storage;
#[cfg(feature = "base_node")]
mod priority;
#[cfg(feature = "base_node")]
mod reorg_pool;
#[cfg(feature = "base_node")]
mod rpc;
#[cfg(feature = "base_node")]
pub use rpc::create_mempool_rpc_service;
#[cfg(feature = "base_node")]
pub use rpc::{MempoolRpcClient, MempoolRpcServer, MempoolRpcService, MempoolService};
#[cfg(feature = "base_node")]
mod metrics;
#[cfg(feature = "base_node")]
mod unconfirmed_pool;

// Public re-exports
#[cfg(feature = "base_node")]
pub use error::MempoolError;
#[cfg(feature = "base_node")]
pub use mempool::Mempool;

#[cfg(feature = "base_node")]
pub use self::config::{MempoolConfig, MempoolServiceConfig};

#[cfg(any(feature = "base_node", feature = "mempool_proto"))]
pub mod proto;

#[cfg(any(feature = "base_node", feature = "mempool_proto"))]
pub mod service;
#[cfg(feature = "base_node")]
pub use service::{MempoolServiceError, MempoolServiceInitializer, OutboundMempoolServiceInterface};

#[cfg(feature = "base_node")]
mod sync_protocol;
use core::fmt::{Display, Error, Formatter};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
#[cfg(feature = "base_node")]
pub use sync_protocol::MempoolSyncInitializer;
use tari_common_types::types::Signature;

use crate::transactions::transaction_components::Transaction;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatsResponse {
    pub total_txs: usize,
    pub unconfirmed_txs: usize,
    pub reorg_txs: usize,
    pub total_weight: u64,
}

impl Display for StatsResponse {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            fmt,
            "Mempool stats: Total transactions: {}, Unconfirmed: {}, Published: {}, Total Weight: {}g",
            self.total_txs, self.unconfirmed_txs, self.reorg_txs, self.total_weight
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StateResponse {
    pub unconfirmed_pool: Vec<Arc<Transaction>>,
    pub reorg_pool: Vec<Signature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TxStorageResponse {
    UnconfirmedPool,
    ReorgPool,
    NotStoredOrphan,
    NotStoredTimeLocked,
    NotStoredAlreadySpent,
    NotStoredConsensus,
    NotStored,
}

impl TxStorageResponse {
    pub fn is_stored(&self) -> bool {
        matches!(self, Self::UnconfirmedPool | Self::ReorgPool)
    }
}

impl Display for TxStorageResponse {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        let storage = match self {
            TxStorageResponse::UnconfirmedPool => "Unconfirmed pool",
            TxStorageResponse::ReorgPool => "Reorg pool",
            TxStorageResponse::NotStoredOrphan => "Not stored orphan transaction",
            TxStorageResponse::NotStoredTimeLocked => "Not stored time locked transaction",
            TxStorageResponse::NotStoredAlreadySpent => "Not stored output already spent",
            TxStorageResponse::NotStoredConsensus => "Not stored due to consensus rule",
            TxStorageResponse::NotStored => "Not stored",
        };
        fmt.write_str(storage)
    }
}
