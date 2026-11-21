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

use std::{
    convert::{TryFrom, TryInto},
    sync::Arc,
};

use log::*;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus};
use tari_transaction_components::transaction_components::Transaction;
use tokio::{sync::Semaphore, task};

use crate::{
    mempool::{MempoolError, MempoolServiceError, rpc::MempoolService, service::MempoolHandle},
    proto,
};
const LOG_TARGET: &str = "c::mempool::rpc";

/// Maximum number of inbound P2P `submit_transaction` RPC requests being converted or submitted at once.
pub(super) const MAX_CONCURRENT_RPC_SUBMISSIONS: usize = 2;

#[cfg(test)]
type ConversionHook = Arc<dyn Fn() + Send + Sync>;

pub struct MempoolRpcService {
    mempool: MempoolHandle,
    submission_permits: Arc<Semaphore>,
    #[cfg(test)]
    conversion_hook: Option<ConversionHook>,
}

impl MempoolRpcService {
    pub fn new(mempool: MempoolHandle) -> Self {
        Self {
            mempool,
            submission_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_RPC_SUBMISSIONS)),
            #[cfg(test)]
            conversion_hook: None,
        }
    }

    #[inline]
    pub fn mempool(&self) -> MempoolHandle {
        self.mempool.clone()
    }

    #[cfg(test)]
    pub(super) fn submission_permits(&self) -> Arc<Semaphore> {
        self.submission_permits.clone()
    }

    #[cfg(test)]
    pub(super) fn with_conversion_hook(mut self, hook: ConversionHook) -> Self {
        self.conversion_hook = Some(hook);
        self
    }
}

fn to_internal_error<T: std::error::Error>(err: T) -> RpcStatus {
    error!(target: LOG_TARGET, "Internal error: {err}");
    RpcStatus::general_default()
}

fn to_submit_error(err: MempoolServiceError) -> RpcStatus {
    match err {
        MempoolServiceError::TransactionNoKernels |
        MempoolServiceError::MempoolError(MempoolError::TransactionNoKernels) => {
            RpcStatus::bad_request("Malformed transaction: Transaction has no kernels")
        },
        other => to_internal_error(other),
    }
}

#[tari_comms::async_trait]
impl MempoolService for MempoolRpcService {
    async fn get_stats(&self, _: Request<()>) -> Result<Response<proto::mempool::StatsResponse>, RpcStatus> {
        let stats = self.mempool().get_stats().await.map_err(to_internal_error)?;
        Ok(Response::new(stats.into()))
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
        let peer_node_id = context.peer_node_id().clone();
        if message.body.as_ref().is_some_and(|body| body.kernels.is_empty()) {
            debug!(
                target: LOG_TARGET,
                "Received transaction with no kernels from peer `{peer_node_id}`"
            );
            return Err(RpcStatus::bad_request(
                "Malformed transaction: Transaction has no kernels",
            ));
        }
        let permit = self
            .submission_permits
            .clone()
            .acquire_owned()
            .await
            .map_err(to_internal_error)?;
        #[cfg(test)]
        let conversion_hook = self.conversion_hook.clone();
        let (_permit, conversion) = task::spawn_blocking(move || {
            #[cfg(test)]
            if let Some(hook) = conversion_hook {
                hook();
            }
            let result = Transaction::try_from(message);
            (permit, result)
        })
        .await
        .map_err(to_internal_error)?;
        let tx = match conversion {
            Ok(tx) => tx,
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Received invalid message from peer `{peer_node_id}`: {err}"
                );
                // These error messages are safe to send back to the requester
                return Err(RpcStatus::bad_request(&format!("Malformed transaction: {err}")));
            },
        };
        let tx_storage = self
            .mempool()
            .submit_transaction_from_peer(tx, peer_node_id)
            .await
            .map_err(to_submit_error)?;
        Ok(Response::new(tx_storage.into()))
    }
}
