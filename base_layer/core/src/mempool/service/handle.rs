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

use tari_common_types::types::Signature;
use tari_service_framework::{reply_channel::TrySenderService, Service};

use crate::{
    mempool::{
        service::{MempoolRequest, MempoolResponse},
        MempoolServiceError,
        StateResponse,
        StatsResponse,
        TxStorageResponse,
    },
    transactions::transaction_components::Transaction,
};

#[derive(Clone)]
pub struct MempoolHandle {
    inner: TrySenderService<MempoolRequest, MempoolResponse, MempoolServiceError>,
}

impl MempoolHandle {
    pub(crate) fn new(request_sender: TrySenderService<MempoolRequest, MempoolResponse, MempoolServiceError>) -> Self {
        Self { inner: request_sender }
    }

    pub async fn get_stats(&mut self) -> Result<StatsResponse, MempoolServiceError> {
        match self.inner.call(MempoolRequest::GetStats).await?? {
            MempoolResponse::Stats(resp) => Ok(resp),
            _ => panic!("Incorrect response"),
        }
    }

    pub async fn get_state(&mut self) -> Result<StateResponse, MempoolServiceError> {
        match self.inner.call(MempoolRequest::GetState).await?? {
            MempoolResponse::State(resp) => Ok(resp),
            _ => panic!("Incorrect response"),
        }
    }

    pub async fn get_tx_state_by_excess_sig(
        &mut self,
        sig: Signature,
    ) -> Result<TxStorageResponse, MempoolServiceError> {
        match self.inner.call(MempoolRequest::GetTxStateByExcessSig(sig)).await?? {
            MempoolResponse::TxStorage(resp) => Ok(resp),
            _ => panic!("Incorrect response"),
        }
    }

    pub async fn submit_transaction(
        &mut self,
        transaction: Transaction,
    ) -> Result<TxStorageResponse, MempoolServiceError> {
        match self
            .inner
            .call(MempoolRequest::SubmitTransaction(transaction))
            .await??
        {
            MempoolResponse::TxStorage(resp) => Ok(resp),
            _ => panic!("Incorrect response"),
        }
    }
}
