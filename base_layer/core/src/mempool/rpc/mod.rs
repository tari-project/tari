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

mod service;
pub use service::MempoolRpcService;

#[cfg(test)]
mod test;

use crate::{
    mempool::service::MempoolHandle,
    proto::{
        mempool::{StateResponse, StatsResponse, TxStorage},
        types::{Signature, Transaction},
    },
};
use tari_comms::protocol::rpc::{Request, Response, RpcStatus};
use tari_comms_rpc_macros::tari_rpc;

#[tari_rpc(protocol_name = b"t/mempool/1", server_struct = MempoolRpcServer, client_struct = MempoolRpcClient)]
pub trait MempoolService: Send + Sync + 'static {
    #[rpc(method = 1)]
    async fn get_stats(&self, request: Request<()>) -> Result<Response<StatsResponse>, RpcStatus>;

    #[rpc(method = 2)]
    async fn get_state(&self, request: Request<()>) -> Result<Response<StateResponse>, RpcStatus>;

    #[rpc(method = 3)]
    async fn get_transaction_state_by_excess_sig(
        &self,
        request: Request<Signature>,
    ) -> Result<Response<TxStorage>, RpcStatus>;

    #[rpc(method = 4)]
    async fn submit_transaction(&self, request: Request<Transaction>) -> Result<Response<TxStorage>, RpcStatus>;
}

pub fn create_mempool_rpc_service(mempool: MempoolHandle) -> MempoolRpcServer<MempoolRpcService> {
    MempoolRpcServer::new(MempoolRpcService::new(mempool))
}
