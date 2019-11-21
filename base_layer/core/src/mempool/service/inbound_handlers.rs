// Copyright 2019. The Tari Project
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

use crate::{
    chain_storage::BlockchainBackend,
    mempool::{
        service::{MempoolRequest, MempoolResponse, MempoolServiceError},
        Mempool,
    },
};
use std::sync::Arc;
use tari_transactions::transaction::Transaction;

/// The MempoolInboundHandlers is used to handle all received inbound mempool requests and transactions from remote
/// nodes.
pub struct MempoolInboundHandlers<T>
where T: BlockchainBackend
{
    mempool: Mempool<T>,
}

impl<T> MempoolInboundHandlers<T>
where T: BlockchainBackend
{
    /// Construct the MempoolInboundHandlers.
    pub fn new(mempool: Mempool<T>) -> Self {
        Self { mempool }
    }

    /// Handle inbound Mempool service requests from remote nodes and local services.
    pub async fn handle_request(&self, request: &MempoolRequest) -> Result<MempoolResponse, MempoolServiceError> {
        match request {
            MempoolRequest::GetStats => Ok(MempoolResponse::Stats(self.mempool.stats()?)), /* TODO: make mempool
                                                                                            * calls async */
        }
    }

    /// Handle inbound transactions from remote wallets and local services.
    pub async fn handle_transaction(&self, tx: &Transaction) -> Result<(), MempoolServiceError> {
        // TODO tx must pass through the validation pipeline, with checking of its internal consistency, before adding
        // and propagating it.
        Ok(self.mempool.insert(Arc::new(tx.clone()))?)
    }
}
