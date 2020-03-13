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
    base_node::comms_interface::BlockEvent,
    chain_storage::{BlockAddResult, BlockchainBackend},
    mempool::{
        async_mempool,
        service::{MempoolRequest, MempoolResponse, MempoolServiceError, OutboundMempoolServiceInterface},
        Mempool,
        TxStorageResponse,
    },
    transactions::transaction::Transaction,
};
use log::*;
use std::sync::Arc;
use tari_comms::types::CommsPublicKey;

pub const LOG_TARGET: &str = "c::mp::service::inbound_handlers";

/// The MempoolInboundHandlers is used to handle all received inbound mempool requests and transactions from remote
/// nodes.
pub struct MempoolInboundHandlers<T>
where T: BlockchainBackend + 'static
{
    mempool: Mempool<T>,
    outbound_nmi: OutboundMempoolServiceInterface,
}

impl<T> MempoolInboundHandlers<T>
where T: BlockchainBackend + 'static
{
    /// Construct the MempoolInboundHandlers.
    pub fn new(mempool: Mempool<T>, outbound_nmi: OutboundMempoolServiceInterface) -> Self {
        Self { mempool, outbound_nmi }
    }

    /// Handle inbound Mempool service requests from remote nodes and local services.
    pub async fn handle_request(&self, request: &MempoolRequest) -> Result<MempoolResponse, MempoolServiceError> {
        debug!(target: LOG_TARGET, "request received for mempool: {:?}", request);
        match request {
            MempoolRequest::GetStats => Ok(MempoolResponse::Stats(
                async_mempool::stats(self.mempool.clone()).await?,
            )),
            MempoolRequest::GetTxStateWithExcessSig(excess_sig) => Ok(MempoolResponse::TxStorage(
                async_mempool::has_tx_with_excess_sig(self.mempool.clone(), excess_sig.clone()).await?,
            )),
        }
    }

    /// Handle inbound transactions from remote wallets and local services.
    pub async fn handle_transaction(
        &mut self,
        tx: &Transaction,
        source_peer: Option<CommsPublicKey>,
    ) -> Result<(), MempoolServiceError>
    {
        if async_mempool::has_tx_with_excess_sig(self.mempool.clone(), tx.body.kernels()[0].excess_sig.clone()).await? ==
            TxStorageResponse::NotStored
        {
            async_mempool::insert(self.mempool.clone(), Arc::new(tx.clone())).await?;
            let exclude_peers = source_peer.into_iter().collect();
            self.outbound_nmi.propagate_tx(tx.clone(), exclude_peers).await?;
        }
        Ok(())
    }

    /// Handle inbound block events from the local base node service.
    pub async fn handle_block_event(&mut self, block_event: &BlockEvent) -> Result<(), MempoolServiceError> {
        match block_event {
            BlockEvent::Verified((block, BlockAddResult::Ok)) => {
                async_mempool::process_published_block(self.mempool.clone(), *block.clone()).await?;
            },
            BlockEvent::Verified((_, BlockAddResult::ChainReorg((removed_blocks, added_blocks)))) => {
                async_mempool::process_reorg(self.mempool.clone(), removed_blocks.to_vec(), added_blocks.to_vec())
                    .await?;
            },
            BlockEvent::Verified(_) | BlockEvent::Invalid(_) => {},
        }

        Ok(())
    }
}

impl<T> Clone for MempoolInboundHandlers<T>
where T: BlockchainBackend + 'static
{
    fn clone(&self) -> Self {
        // All members use Arc's internally so calling clone should be cheap.
        Self {
            mempool: self.mempool.clone(),
            outbound_nmi: self.outbound_nmi.clone(),
        }
    }
}
