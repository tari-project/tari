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
mod service;
#[cfg(feature = "base_node")]
pub mod sync_utxos_by_block_task;

#[cfg(feature = "base_node")]
pub use service::BaseNodeWalletRpcService;
use tari_p2p::{
    proto,
    proto::{
        base_node::{
            FetchMatchingUtxos,
            FetchUtxosResponse,
            GetMempoolFeePerGramStatsRequest,
            GetMempoolFeePerGramStatsResponse,
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
use tari_rpc_framework::{Request, Response, RpcStatus, Streaming};
use tari_rpc_macros::tari_rpc;

#[cfg(feature = "base_node")]
use crate::base_node::StateMachineHandle;
#[cfg(feature = "base_node")]
use crate::{
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    mempool::service::MempoolHandle,
};

#[tari_rpc(protocol_name = "t/bnwallet/1", server_struct = BaseNodeWalletRpcServer, client_struct = BaseNodeWalletRpcClient)]
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
}

#[cfg(feature = "base_node")]
pub fn create_base_node_wallet_rpc_service<B: BlockchainBackend + 'static>(
    db: AsyncBlockchainDb<B>,
    mempool: MempoolHandle,
    state_machine: StateMachineHandle,
) -> BaseNodeWalletRpcServer<BaseNodeWalletRpcService<B>> {
    BaseNodeWalletRpcServer::new(BaseNodeWalletRpcService::new(db, mempool, state_machine))
}
