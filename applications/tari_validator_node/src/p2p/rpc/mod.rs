//  Copyright 2021, The Tari Project
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

mod service_impl;

#[cfg(test)]
mod test;

pub use service_impl::ValidatorNodeRpcServiceImpl;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus, Streaming};
use tari_comms_rpc_macros::tari_rpc;
use tari_dan_core::{
    services::{AssetProcessor, MempoolService},
    storage::DbFactory,
};

use crate::p2p::proto::validator_node as proto;

#[tari_rpc(protocol_name = b"t/vn/1", server_struct = ValidatorNodeRpcServer, client_struct = ValidatorNodeRpcClient)]
pub trait ValidatorNodeRpcService: Send + Sync + 'static {
    #[rpc(method = 1)]
    async fn get_token_data(
        &self,
        request: Request<proto::GetTokenDataRequest>,
    ) -> Result<Response<proto::GetTokenDataResponse>, RpcStatus>;

    #[rpc(method = 2)]
    async fn invoke_read_method(
        &self,
        request: Request<proto::InvokeReadMethodRequest>,
    ) -> Result<Response<proto::InvokeReadMethodResponse>, RpcStatus>;

    #[rpc(method = 3)]
    async fn invoke_method(
        &self,
        request: Request<proto::InvokeMethodRequest>,
    ) -> Result<Response<proto::InvokeMethodResponse>, RpcStatus>;

    #[rpc(method = 4)]
    async fn get_sidechain_blocks(
        &self,
        request: Request<proto::GetSidechainBlocksRequest>,
    ) -> Result<Streaming<proto::GetSidechainBlocksResponse>, RpcStatus>;

    #[rpc(method = 5)]
    async fn get_sidechain_state(
        &self,
        request: Request<proto::GetSidechainStateRequest>,
    ) -> Result<Streaming<proto::GetSidechainStateResponse>, RpcStatus>;

    #[rpc(method = 6)]
    async fn get_op_logs(
        &self,
        request: Request<proto::GetStateOpLogsRequest>,
    ) -> Result<Response<proto::GetStateOpLogsResponse>, RpcStatus>;

    #[rpc(method = 7)]
    async fn get_tip_node(
        &self,
        request: Request<proto::GetTipNodeRequest>,
    ) -> Result<Response<proto::GetTipNodeResponse>, RpcStatus>;
}

pub fn create_validator_node_rpc_service<
    TMempoolService: MempoolService + Clone,
    TDbFactory: DbFactory + Clone,
    TAssetProcessor: AssetProcessor + Clone,
>(
    mempool_service: TMempoolService,
    db_factory: TDbFactory,
    asset_processor: TAssetProcessor,
) -> ValidatorNodeRpcServer<ValidatorNodeRpcServiceImpl<TMempoolService, TDbFactory, TAssetProcessor>> {
    ValidatorNodeRpcServer::new(ValidatorNodeRpcServiceImpl::new(
        mempool_service,
        db_factory,
        asset_processor,
    ))
}
