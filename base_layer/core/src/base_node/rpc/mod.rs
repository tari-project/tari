// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[cfg(feature = "base_node")]
mod service;
#[cfg(feature = "base_node")]
pub mod sync_utxos_by_block_task;

pub mod models;

#[cfg(feature = "base_node")]
pub mod query_service;

use std::{error::Error, fmt::Debug};

#[cfg(feature = "base_node")]
pub use service::BaseNodeWalletRpcService;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus, Streaming};
use tari_comms_rpc_macros::tari_rpc;
#[cfg(feature = "base_node")]
use url::Url;

#[cfg(feature = "base_node")]
use crate::base_node::StateMachineHandle;
#[cfg(feature = "base_node")]
use crate::{
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    mempool::service::MempoolHandle,
};
use crate::{
    proto,
    proto::{
        base_node::{
            FetchMatchingUtxos,
            FetchUtxosResponse,
            GetMempoolFeePerGramStatsRequest,
            GetMempoolFeePerGramStatsResponse,
            GetWalletQueryHttpServiceAddressResponse,
            QueryDeletedRequest,
            QueryDeletedResponse,
            Signatures,
            SyncUtxosByBlockRequest,
            SyncUtxosByBlockResponse,
            TipInfoResponse,
            TxQueryBatchResponses,
            TxQueryResponse,
            TxSubmissionResponse,
            UtxoQueryRequest,
            UtxoQueryResponses,
        },
        types::{Signature, Transaction},
    },
};

/// Trait that a base node wallet query service must implement.
/// Please note that this service is to fetch data, so read-only queries.
#[async_trait::async_trait]
pub trait BaseNodeWalletQueryService: Send + Sync + 'static {
    type Error: Error + 'static;

    async fn get_tip_info(&self) -> Result<models::TipInfoResponse, Self::Error>;

    async fn get_header_by_height(&self, height: u64) -> Result<models::BlockHeader, Self::Error>;

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, Self::Error>;

    async fn get_utxos_by_block(
        &self,
        request: models::GetUtxosByBlockRequest,
    ) -> Result<models::GetUtxosByBlockResponse, Self::Error>;

    async fn transaction_query(&self, signature: models::Signature) -> Result<models::TxQueryResponse, Self::Error>;

    async fn sync_utxos_by_block(
        &self,
        request: models::SyncUtxosByBlockRequest,
    ) -> Result<models::SyncUtxosByBlockResponse, Self::Error>;
}
