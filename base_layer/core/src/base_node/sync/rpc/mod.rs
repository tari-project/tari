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
pub use service::BaseNodeSyncRpcService;

// TODO: Tests need to be rewritten
// #[cfg(test)]
// mod tests;

#[cfg(feature = "base_node")]
use crate::chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend};

use crate::{
    proto,
    proto::base_node::{
        FindChainSplitRequest,
        FindChainSplitResponse,
        SyncBlocksRequest,
        SyncHeadersRequest,
        SyncKernelsRequest,
        SyncUtxos2Response,
        SyncUtxosRequest,
        SyncUtxosResponse,
    },
};
use tari_comms::protocol::rpc::{Request, Response, RpcStatus, Streaming};
use tari_comms_rpc_macros::tari_rpc;

#[tari_rpc(protocol_name = b"t/blksync/1", server_struct = BaseNodeSyncRpcServer, client_struct = BaseNodeSyncRpcClient)]
pub trait BaseNodeSyncService: Send + Sync + 'static {
    #[rpc(method = 1)]
    async fn sync_blocks(
        &self,
        request: Request<SyncBlocksRequest>,
    ) -> Result<Streaming<proto::base_node::BlockBodyResponse>, RpcStatus>;

    #[rpc(method = 2)]
    async fn sync_headers(
        &self,
        request: Request<SyncHeadersRequest>,
    ) -> Result<Streaming<proto::core::BlockHeader>, RpcStatus>;

    #[rpc(method = 3)]
    async fn get_header_by_height(
        &self,
        request: Request<u64>,
    ) -> Result<Response<proto::core::BlockHeader>, RpcStatus>;

    #[rpc(method = 4)]
    async fn find_chain_split(
        &self,
        request: Request<FindChainSplitRequest>,
    ) -> Result<Response<FindChainSplitResponse>, RpcStatus>;

    #[rpc(method = 5)]
    async fn get_chain_metadata(
        &self,
        request: Request<()>,
    ) -> Result<Response<proto::base_node::ChainMetadata>, RpcStatus>;

    #[rpc(method = 6)]
    async fn sync_kernels(
        &self,
        request: Request<SyncKernelsRequest>,
    ) -> Result<Streaming<proto::types::TransactionKernel>, RpcStatus>;

    #[rpc(method = 7)]
    async fn sync_utxos(&self, request: Request<SyncUtxosRequest>) -> Result<Streaming<SyncUtxosResponse>, RpcStatus>;

    #[rpc(method = 8)]
    async fn sync_utxos2(&self, request: Request<SyncUtxosRequest>)
        -> Result<Streaming<SyncUtxos2Response>, RpcStatus>;
}

#[cfg(feature = "base_node")]
pub fn create_base_node_sync_rpc_service<B: BlockchainBackend + 'static>(
    db: AsyncBlockchainDb<B>,
) -> BaseNodeSyncRpcServer<BaseNodeSyncRpcService<B>> {
    BaseNodeSyncRpcServer::new(BaseNodeSyncRpcService::new(db))
}
