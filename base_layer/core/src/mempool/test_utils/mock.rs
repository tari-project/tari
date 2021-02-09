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

use crate::mempool::{
    service::{MempoolHandle, MempoolRequest, MempoolResponse},
    MempoolServiceError,
    StateResponse,
    StatsResponse,
    TxStorageResponse,
};
use futures::StreamExt;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tari_service_framework::reply_channel;
use tokio::{sync::Mutex, task};

pub fn create_mempool_service_mock() -> (MempoolHandle, MempoolMockState) {
    let (tx, rx) = reply_channel::unbounded();
    let mock = MempoolServiceMock::new(rx);
    let state = mock.get_shared_state();
    task::spawn(mock.run());
    (MempoolHandle::new(tx), state)
}

#[derive(Debug, Clone)]
pub struct MempoolMockState {
    get_stats: Arc<Mutex<StatsResponse>>,
    get_state: Arc<Mutex<StateResponse>>,
    get_tx_state_by_excess_sig: Arc<Mutex<TxStorageResponse>>,
    submit_transaction: Arc<Mutex<TxStorageResponse>>,
    calls: Arc<AtomicUsize>,
}

impl Default for MempoolMockState {
    fn default() -> Self {
        Self {
            get_stats: Arc::new(Mutex::new(StatsResponse {
                total_txs: 0,
                unconfirmed_txs: 0,
                reorg_txs: 0,
                total_weight: 0,
            })),
            get_state: Arc::new(Mutex::new(StateResponse {
                unconfirmed_pool: vec![],
                reorg_pool: vec![],
            })),
            get_tx_state_by_excess_sig: Arc::new(Mutex::new(TxStorageResponse::NotStored)),
            submit_transaction: Arc::new(Mutex::new(TxStorageResponse::NotStored)),
            calls: Arc::new(Default::default()),
        }
    }
}

impl MempoolMockState {
    pub async fn set_get_stats_response(&self, stats: StatsResponse) {
        *self.get_stats.lock().await = stats;
    }

    pub async fn set_get_state_response(&self, state: StateResponse) {
        *self.get_state.lock().await = state;
    }

    pub async fn set_get_tx_by_excess_sig_stats_response(&self, resp: TxStorageResponse) {
        *self.get_tx_state_by_excess_sig.lock().await = resp;
    }

    pub async fn set_submit_transaction_response(&self, resp: TxStorageResponse) {
        *self.submit_transaction.lock().await = resp;
    }

    fn inc_call_count(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }

    pub fn get_call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

struct MempoolServiceMock {
    receiver: reply_channel::TryReceiver<MempoolRequest, MempoolResponse, MempoolServiceError>,
    state: MempoolMockState,
}

impl MempoolServiceMock {
    pub fn new(receiver: reply_channel::TryReceiver<MempoolRequest, MempoolResponse, MempoolServiceError>) -> Self {
        Self {
            receiver,
            state: MempoolMockState::default(),
        }
    }

    pub fn get_shared_state(&self) -> MempoolMockState {
        self.state.clone()
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.next().await {
            let (req, reply) = req.split();
            reply.send(self.handle_request(req).await).unwrap();
        }
    }

    async fn handle_request(&self, req: MempoolRequest) -> Result<MempoolResponse, MempoolServiceError> {
        use MempoolRequest::*;

        self.state.inc_call_count();
        match req {
            GetStats => Ok(MempoolResponse::Stats(self.state.get_stats.lock().await.clone())),
            GetState => Ok(MempoolResponse::State(self.state.get_state.lock().await.clone())),
            GetTxStateByExcessSig(_) => Ok(MempoolResponse::TxStorage(
                self.state.get_tx_state_by_excess_sig.lock().await.clone(),
            )),
            SubmitTransaction(_) => Ok(MempoolResponse::TxStorage(
                self.state.submit_transaction.lock().await.clone(),
            )),
        }
    }
}
