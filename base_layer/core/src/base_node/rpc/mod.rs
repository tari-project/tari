// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

mod service;

pub mod sync_utxos_by_block_task;

pub use service::BaseNodeWalletRpcService;

pub mod query_service;

use std::{error::Error, fmt::Debug};

use tari_comms::{
    protocol::rpc::{Request, Response, RpcStatus, Streaming},
    types::CompressedSignature,
};
use tari_comms_rpc_macros::tari_rpc;
use tari_transaction_components::{
    rpc::{models, models::GenerateKernelMerkleProofResponse},
    transaction_components::TransactionOutput,
};
use url::Url;

use crate::{
    base_node::StateMachineHandle,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    mempool::service::MempoolHandle,
    proto::{
        self,
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

    async fn get_utxos_mined_info(
        &self,
        request: models::GetUtxosMinedInfoRequest,
    ) -> Result<models::GetUtxosMinedInfoResponse, Self::Error>;

    async fn get_utxos_by_block(
        &self,
        request: models::GetUtxosByBlockRequest,
    ) -> Result<models::GetUtxosByBlockResponse, Self::Error>;

    async fn transaction_query(&self, signature: models::Signature) -> Result<models::TxQueryResponse, Self::Error>;

    async fn sync_utxos_by_block(
        &self,
        request: models::SyncUtxosByBlockRequest,
    ) -> Result<models::SyncUtxosByBlockResponse, Self::Error>;

    async fn get_utxos_deleted_info(
        &self,
        request: models::GetUtxosDeletedInfoRequest,
    ) -> Result<models::GetUtxosDeletedInfoResponse, Self::Error>;

    async fn generate_kernel_merkle_proof(
        &self,
        excess_sig: CompressedSignature,
    ) -> Result<GenerateKernelMerkleProofResponse, Self::Error>;

    async fn get_utxo(&self, request: models::GetUtxoRequest) -> Result<Option<TransactionOutput>, Self::Error>;
}

#[tari_rpc(protocol_name = b"t/bnwallet/1", server_struct = BaseNodeWalletRpcServer, client_struct = BaseNodeWalletRpcClient)]
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
