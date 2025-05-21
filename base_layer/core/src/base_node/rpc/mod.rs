// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod http;

pub mod models;
#[cfg(feature = "base_node")]
mod service;
#[cfg(feature = "base_node")]
pub mod sync_utxos_by_block_task;

use std::{error::Error, fmt::Debug};

#[cfg(feature = "base_node")]
pub use service::BaseNodeWalletRpcService;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus, Streaming};
use tari_comms_rpc_macros::tari_rpc;
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use url::Url;

#[cfg(feature = "base_node")]
use crate::base_node::StateMachineHandle;
use crate::{
    blocks::BlockHeader,
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
#[cfg(feature = "base_node")]
use crate::{
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    mempool::service::MempoolHandle,
};

/// Trait that a base node wallet query service must implement.
/// Please note that this service is to fetch data, so read-only queries.
#[async_trait::async_trait]
pub trait BaseNodeWalletQueryService: Send + Sync + 'static {
    type Error: Error + 'static;

    async fn get_tip_info(&self) -> Result<models::TipInfoResponse, Self::Error>;

    async fn get_header_by_height(&self, height: u64) -> Result<BlockHeader, Self::Error>;

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, Self::Error>;
}

#[derive(Debug, Error)]
pub enum BaseNodeWalletQueryServiceClientError {
    #[error("Failed to parse http address: {0}")]
    HttpAddressParse(#[from] url::ParseError),
    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),
}

/// Trait that a base node wallet query service client must implement.
/// This is the client side of [`BaseNodeWalletQueryService`].
#[async_trait::async_trait]
pub trait BaseNodeWalletQueryServiceClient: Send + Sync + Clone + 'static {
    async fn get_tip_info(&self) -> Result<models::TipInfoResponse, BaseNodeWalletQueryServiceClientError>;

    async fn get_header_by_height(&self, height: u64) -> Result<BlockHeader, BaseNodeWalletQueryServiceClientError>;

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, BaseNodeWalletQueryServiceClientError>;
}

#[tari_rpc(protocol_name = b"t/bnwallet/1", server_struct = BaseNodeWalletRpcServer, client_struct = BaseNodeWalletRpcClient
)]
pub trait BaseNodeWalletService: Send + Sync + 'static {
    #[rpc(method = 1)]
    async fn submit_transaction(
        &self,
        request: Request<Transaction>,
    ) -> Result<Response<TxSubmissionResponse>, RpcStatus>;

    #[rpc(method = 2)]
    async fn transaction_query(&self, request: Request<Signature>) -> Result<Response<TxQueryResponse>, RpcStatus>;

    #[rpc(method = 3)]
    async fn transaction_batch_query(
        &self,
        request: Request<Signatures>,
    ) -> Result<Response<TxQueryBatchResponses>, RpcStatus>;

    #[rpc(method = 4)]
    async fn fetch_matching_utxos(
        &self,
        request: Request<FetchMatchingUtxos>,
    ) -> Result<Response<FetchUtxosResponse>, RpcStatus>;

    #[rpc(method = 5)]
    async fn get_tip_info(&self, request: Request<()>) -> Result<Response<TipInfoResponse>, RpcStatus>;

    #[rpc(method = 6)]
    async fn get_header(&self, request: Request<u64>) -> Result<Response<proto::core::BlockHeader>, RpcStatus>;

    #[rpc(method = 7)]
    async fn utxo_query(&self, request: Request<UtxoQueryRequest>) -> Result<Response<UtxoQueryResponses>, RpcStatus>;

    #[rpc(method = 8)]
    async fn query_deleted(
        &self,
        request: Request<QueryDeletedRequest>,
    ) -> Result<Response<QueryDeletedResponse>, RpcStatus>;

    #[rpc(method = 9)]
    async fn get_header_by_height(
        &self,
        request: Request<u64>,
    ) -> Result<Response<proto::core::BlockHeader>, RpcStatus>;

    #[rpc(method = 10)]
    async fn get_height_at_time(&self, request: Request<u64>) -> Result<Response<u64>, RpcStatus>;

    #[rpc(method = 11)]
    async fn sync_utxos_by_block(
        &self,
        request: Request<SyncUtxosByBlockRequest>,
    ) -> Result<Streaming<SyncUtxosByBlockResponse>, RpcStatus>;

    #[rpc(method = 12)]
    async fn get_mempool_fee_per_gram_stats(
        &self,
        request: Request<GetMempoolFeePerGramStatsRequest>,
    ) -> Result<Response<GetMempoolFeePerGramStatsResponse>, RpcStatus>;

    #[rpc(method = 13)]
    async fn get_wallet_query_http_service_address(
        &self,
        request: Request<()>,
    ) -> Result<Response<GetWalletQueryHttpServiceAddressResponse>, RpcStatus>;
}

#[cfg(feature = "base_node")]
pub fn create_base_node_wallet_rpc_service<B: BlockchainBackend + 'static>(
    db: AsyncBlockchainDb<B>,
    mempool: MempoolHandle,
    state_machine: StateMachineHandle,
    wallet_query_service_address: Option<Url>,
) -> BaseNodeWalletRpcServer<BaseNodeWalletRpcService<B>> {
    BaseNodeWalletRpcServer::new(BaseNodeWalletRpcService::new(
        db,
        mempool,
        state_machine,
        wallet_query_service_address,
    ))
}

#[cfg(feature = "base_node")]
pub fn create_base_node_wallet_query_http_server<B: BlockchainBackend + 'static>(
    port: u16,
    db: AsyncBlockchainDb<B>,
    state_machine: StateMachineHandle,
    shutdown_signal: ShutdownSignal,
) -> http::server::Server<impl BaseNodeWalletQueryService> {
    http::server::Server::new(
        port,
        http::query_service::Service::new(db, state_machine),
        shutdown_signal,
    )
}
