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

pub mod test_utils;

mod config;

mod error;

#[allow(clippy::module_inception)]
mod mempool;

mod mempool_storage;

mod priority;

mod reorg_pool;

mod rpc;

pub use rpc::{MempoolRpcClient, MempoolRpcServer, MempoolRpcService, MempoolService, create_mempool_rpc_service};
#[cfg(feature = "metrics")]
mod metrics;

mod shrink_hashmap;

mod unconfirmed_pool;

// Public re-exports

pub use error::MempoolError;
pub use mempool::Mempool;
use tari_transaction_components::rpc::models::FeePerGramStat;

pub use self::config::{MempoolConfig, MempoolServiceConfig};
pub mod proto;

pub mod service;

pub use service::{MempoolServiceError, MempoolServiceInitializer, OutboundMempoolServiceInterface};

mod sync_protocol;
use core::fmt::{Display, Error, Formatter};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
pub use sync_protocol::MempoolSyncInitializer;
use tari_common_types::types::CompressedSignature;
use tari_transaction_components::transaction_components::Transaction;

use crate::proto::base_node as base_node_proto;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatsResponse {
    pub unconfirmed_txs: u64,
    pub reorg_txs: u64,
    pub unconfirmed_weight: u64,
}

impl Display for StatsResponse {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            fmt,
            "Mempool stats: Unconfirmed: {}, In Reorg Pool: {}, Total Weight: {}g",
            self.unconfirmed_txs, self.reorg_txs, self.unconfirmed_weight
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateResponse {
    pub unconfirmed_pool: Vec<Arc<Transaction>>,
    pub reorg_pool: Vec<CompressedSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxStorageResponse {
    UnconfirmedPool,
    ReorgPool,
    NotStoredOrphan,
    NotStoredTimeLocked,
    NotStoredAlreadySpent,
    NotStoredConsensus,
    NotStored,
    NotStoredAlreadyMined,
    NotStoredFeeTooLow,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxStorageResponseWithDetails {
    pub response: TxStorageResponse,
    pub rejection_reason: Option<String>,
}

impl TxStorageResponseWithDetails {
    pub fn new(response: TxStorageResponse, rejection_reason: Option<String>) -> Self {
        Self {
            response,
            rejection_reason,
        }
    }

    pub fn is_stored(&self) -> bool {
        self.response.is_stored()
    }

    pub fn into_response(self) -> TxStorageResponse {
        self.response
    }
}

impl From<TxStorageResponse> for TxStorageResponseWithDetails {
    fn from(response: TxStorageResponse) -> Self {
        Self {
            response,
            rejection_reason: None,
        }
    }
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
            TxStorageResponse::NotStoredAlreadyMined => "Not stored tx already mined",
            TxStorageResponse::NotStoredFeeTooLow => "Not stored tx fee is below the minimum accepted by this mempool",
        };
        fmt.write_str(storage)
    }
}
impl From<base_node_proto::MempoolFeePerGramStat> for FeePerGramStat {
    fn from(value: base_node_proto::MempoolFeePerGramStat) -> Self {
        Self {
            order: value.order,
            min_fee_per_gram: value.min_fee_per_gram.into(),
            avg_fee_per_gram: value.avg_fee_per_gram.into(),
            max_fee_per_gram: value.max_fee_per_gram.into(),
        }
    }
}
