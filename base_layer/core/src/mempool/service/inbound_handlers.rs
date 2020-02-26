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
        service::{MempoolRequest, MempoolResponse, MempoolServiceError, OutboundMempoolServiceInterface},
        Mempool,
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
where T: BlockchainBackend
{
    mempool: Mempool<T>,
    outbound_nmi: OutboundMempoolServiceInterface,
}

impl<T> MempoolInboundHandlers<T>
where T: BlockchainBackend
{
    /// Construct the MempoolInboundHandlers.
    pub fn new(mempool: Mempool<T>, outbound_nmi: OutboundMempoolServiceInterface) -> Self {
        Self { mempool, outbound_nmi }
    }

    /// Handle inbound Mempool service requests from remote nodes and local services.
    pub async fn handle_request(&self, request: &MempoolRequest) -> Result<MempoolResponse, MempoolServiceError> {
        // TODO: make mempool calls async
        debug!(target: LOG_TARGET, "request received for mempool: {:?}", request);
        match request {
            MempoolRequest::GetStats => Ok(MempoolResponse::Stats(self.mempool.stats()?)),
            MempoolRequest::GetTxStateWithExcessSig(excess_sig) => Ok(MempoolResponse::TxStorage(
                self.mempool.has_tx_with_excess_sig(excess_sig)?,
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
        self.mempool.insert(Arc::new(tx.clone()))?;
        let exclude_list = if let Some(peer) = source_peer {
            vec![peer]
        } else {
            Vec::new()
        };
        self.outbound_nmi.propagate_tx(tx.clone(), exclude_list).await?;

        Ok(())
    }

    /// Handle inbound block events from the local base node service.
    pub async fn handle_block_event(&mut self, block_event: &BlockEvent) -> Result<(), MempoolServiceError> {
        match block_event {
            BlockEvent::Verified((block, BlockAddResult::Ok)) => {
                self.mempool.process_published_block(block)?;
            },
            BlockEvent::Verified((_, BlockAddResult::ChainReorg((removed_blocks, added_blocks)))) => {
                self.mempool
                    .process_reorg(removed_blocks.to_vec(), added_blocks.to_vec())?;
            },
            BlockEvent::Verified(_) | BlockEvent::Invalid(_) => {},
        }

        Ok(())
    }
}
