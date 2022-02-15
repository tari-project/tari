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

use std::convert::{TryFrom, TryInto};

use log::*;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus};

use crate::{
    mempool::{rpc::MempoolService, service::MempoolHandle},
    proto,
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::mempool::rpc";

pub struct MempoolRpcService {
    mempool: MempoolHandle,
}

impl MempoolRpcService {
    pub fn new(mempool: MempoolHandle) -> Self {
        Self { mempool }
    }

    #[inline]
    pub fn mempool(&self) -> MempoolHandle {
        self.mempool.clone()
    }
}

// TODO: Logging the error and returning a general error to the requester is a common requirement. Figure out a clean
//       way to provide this functionality.
fn to_internal_error<T: std::error::Error>(err: T) -> RpcStatus {
    error!(target: LOG_TARGET, "Internal error: {}", err);
    RpcStatus::general(err.to_string())
}

#[tari_comms::async_trait]
impl MempoolService for MempoolRpcService {
    async fn get_stats(&self, _: Request<()>) -> Result<Response<proto::mempool::StatsResponse>, RpcStatus> {
        let stats = self.mempool().get_stats().await.map_err(to_internal_error)?;
        Ok(Response::new(stats.into()))
    }

    async fn get_state(&self, _: Request<()>) -> Result<Response<proto::mempool::StateResponse>, RpcStatus> {
        let state = self.mempool().get_state().await.map_err(to_internal_error)?;
        Ok(Response::new(state.try_into().map_err(|e: String| {
            error!(target: LOG_TARGET, "Internal error: {}", e);
            RpcStatus::general(e)
        })?))
    }

    async fn get_transaction_state_by_excess_sig(
        &self,
        request: Request<proto::types::Signature>,
    ) -> Result<Response<proto::mempool::TxStorage>, RpcStatus> {
        let excess_sig = request
            .into_message()
            .try_into()
            .map_err(|_| RpcStatus::bad_request("Invalid signature received"))?;
        let resp = self
            .mempool()
            .get_tx_state_by_excess_sig(excess_sig)
            .await
            .map_err(to_internal_error)?;
        Ok(Response::new(resp.into()))
    }

    async fn submit_transaction(
        &self,
        request: Request<proto::types::Transaction>,
    ) -> Result<Response<proto::mempool::TxStorage>, RpcStatus> {
        let (context, message) = request.into_parts();
        let tx = match Transaction::try_from(message) {
            Ok(tx) => tx,
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Received invalid message from peer `{}`: {}",
                    context.peer_node_id(),
                    err
                );
                // These error messages are safe to send back to the requester
                return Err(RpcStatus::bad_request(format!("Malformed transaction: {}", err)));
            },
        };
        let tx_storage = self.mempool().submit_transaction(tx).await.map_err(to_internal_error)?;
        Ok(Response::new(tx_storage.into()))
    }
}
