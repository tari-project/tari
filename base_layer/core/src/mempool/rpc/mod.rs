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

use tari_comms::protocol::rpc::{Request, Response, RpcStatus};
use tari_comms_rpc_macros::tari_rpc;

use crate::{
    mempool::service::MempoolHandle,
    proto::{
        mempool::{StateResponse, StatsResponse, TxStorage},
        types::{Signature, Transaction},
    },
};

// `max_items` is the decode budget for a method: the most embedded message instances (plus repeated bytes/string
// elements and packed scalar bytes) its request, response or each stream item may carry (see
// `tari_comms::decode_budget`). BODY_MAX_DECODE_ITEMS (262,144) covers a max-weight block body or transaction of fully
// populated (hydrated) inputs plus 1,000 coinbases: ~160k instances on mainnet (~1.6x headroom), ~222k on the
// 127,795-weight networks (~1.2x); see `proto::decode_budget_tests::per_network`. sync_blocks serves compact inputs (2
// instances each), so a synced body has ~7x (mainnet) / ~5.7x headroom; hydrated inputs only reach submit_transaction,
// which rejects an over-budget request without a ban. 65_536 covers the batched queries and streams. The default is
// 16_384.
#[tari_rpc(protocol_name = b"t/mempool/1", server_struct = MempoolRpcServer, client_struct = MempoolRpcClient)]
pub trait MempoolService: Send + Sync + 'static {
    #[rpc(method = 1)]
    async fn get_stats(&self, request: Request<()>) -> Result<Response<StatsResponse>, RpcStatus>;

    #[rpc(method = 2, max_items = crate::proto::BODY_MAX_DECODE_ITEMS)]
    async fn get_state(&self, request: Request<()>) -> Result<Response<StateResponse>, RpcStatus>;

    #[rpc(method = 3)]
    async fn get_transaction_state_by_excess_sig(
        &self,
        request: Request<Signature>,
    ) -> Result<Response<TxStorage>, RpcStatus>;

    #[rpc(method = 4, max_items = crate::proto::BODY_MAX_DECODE_ITEMS)]
    async fn submit_transaction(&self, request: Request<Transaction>) -> Result<Response<TxStorage>, RpcStatus>;
}

pub fn create_mempool_rpc_service(mempool: MempoolHandle) -> MempoolRpcServer<MempoolRpcService> {
    MempoolRpcServer::new(MempoolRpcService::new(mempool))
}
