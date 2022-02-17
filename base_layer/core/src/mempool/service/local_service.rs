// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use tari_common_types::types::Signature;
use tari_service_framework::{reply_channel::SenderService, Service};

use crate::{
    mempool::{
        service::{MempoolRequest, MempoolResponse, MempoolServiceError},
        StateResponse,
        StatsResponse,
        TxStorageResponse,
    },
    transactions::transaction_components::Transaction,
};

pub type LocalMempoolRequester = SenderService<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>;

/// A local interface into the mempool service.
///
/// Clients obtain a handle to the request_sender, which sends a `MempoolRequest` through the channel, returning a
/// one-shot channel that will eventually carry the response.
///
/// The `request_stream` (the receiver side of the request channel) will be owned by the MempoolService and make the
/// actual requests to the mempool API before packaging up the response to be sent back out the one-shot channel.
#[derive(Clone)]
pub struct LocalMempoolService {
    request_sender: LocalMempoolRequester,
}

impl LocalMempoolService {
    /// Create a new LocalMempoolService instance. This struct doesn't do anything on its own. It is meant to be used
    /// in the main Mempool service event loop, where Mempool requests from the `request_stream` will be handled and
    /// the responses sent back on the appropriate one-shot channels.
    ///
    /// To make things a little more ergonomic, the channel handling is done for you in the other member functions,
    /// such that the request behaves like a standard future.
    pub fn new(request_sender: LocalMempoolRequester) -> Self {
        LocalMempoolService { request_sender }
    }

    /// Returns a future that resolves to the current mempool statistics
    pub async fn get_mempool_stats(&mut self) -> Result<StatsResponse, MempoolServiceError> {
        match self.request_sender.call(MempoolRequest::GetStats).await?? {
            MempoolResponse::Stats(s) => Ok(s),
            _ => Err(MempoolServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_mempool_state(&mut self) -> Result<StateResponse, MempoolServiceError> {
        match self.request_sender.call(MempoolRequest::GetState).await?? {
            MempoolResponse::State(s) => Ok(s),
            _ => Err(MempoolServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn submit_transaction(
        &mut self,
        transaction: Transaction,
    ) -> Result<TxStorageResponse, MempoolServiceError> {
        match self
            .request_sender
            .call(MempoolRequest::SubmitTransaction(transaction))
            .await??
        {
            MempoolResponse::TxStorage(s) => Ok(s),
            _ => Err(MempoolServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_transaction_state_by_excess_sig(
        &mut self,
        sig: Signature,
    ) -> Result<TxStorageResponse, MempoolServiceError> {
        match self
            .request_sender
            .call(MempoolRequest::GetTxStateByExcessSig(sig))
            .await??
        {
            MempoolResponse::TxStorage(s) => Ok(s),
            _ => Err(MempoolServiceError::UnexpectedApiResponse),
        }
    }
}

#[cfg(test)]
mod test {
    use futures::StreamExt;
    use tari_service_framework::reply_channel::{unbounded, Receiver};
    use tokio::task;

    use crate::mempool::{
        service::{local_service::LocalMempoolService, MempoolRequest, MempoolResponse},
        MempoolServiceError,
        StatsResponse,
    };

    pub type LocalMempoolRequestStream = Receiver<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>;

    fn request_stats() -> StatsResponse {
        StatsResponse {
            total_txs: 10,
            unconfirmed_txs: 3,
            reorg_txs: 4,
            total_weight: 1000,
        }
    }

    async fn mock_handler(mut rx: LocalMempoolRequestStream) {
        while let Some(req) = rx.next().await {
            let (req, reply_channel) = req.split();
            let res = match req {
                MempoolRequest::GetStats => Ok(MempoolResponse::Stats(request_stats())),
                _ => Err(MempoolServiceError::UnexpectedApiResponse),
            };
            reply_channel.send(res).unwrap();
        }
    }

    #[tokio::test]
    async fn mempool_stats() {
        let (tx, rx) = unbounded();
        let mut service = LocalMempoolService::new(tx);
        task::spawn(mock_handler(rx));
        let stats = service.get_mempool_stats().await;
        let stats = stats.expect("get_mempool_stats should have succeeded");
        assert_eq!(stats, request_stats());
    }

    #[tokio::test]
    async fn mempool_stats_from_multiple() {
        let (tx, rx) = unbounded();
        let mut service = LocalMempoolService::new(tx);
        let mut service2 = service.clone();
        task::spawn(mock_handler(rx));
        let stats = service.get_mempool_stats().await;
        let stats = stats.expect("get_mempool_stats should have succeeded");
        assert_eq!(stats, request_stats());
        let stats = service2.get_mempool_stats().await;
        let stats = stats.expect("get_mempool_stats should have succeeded");
        assert_eq!(stats, request_stats());
    }
}
