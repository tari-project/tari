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

use std::sync::Arc;

use log::*;
use tari_comms::peer_manager::NodeId;
use tari_crypto::tari_utilities::hex::Hex;

use crate::{
    base_node::comms_interface::BlockEvent,
    chain_storage::BlockAddResult,
    mempool::{
        metrics,
        service::{MempoolRequest, MempoolResponse, MempoolServiceError, OutboundMempoolServiceInterface},
        Mempool,
        TxStorageResponse,
    },
    transactions::transaction_components::Transaction,
};

pub const LOG_TARGET: &str = "c::mp::service::inbound_handlers";

/// The MempoolInboundHandlers is used to handle all received inbound mempool requests and transactions from remote
/// nodes.
#[derive(Clone)]
pub struct MempoolInboundHandlers {
    mempool: Mempool,
    outbound_nmi: OutboundMempoolServiceInterface,
}

impl MempoolInboundHandlers {
    /// Construct the MempoolInboundHandlers.
    pub fn new(mempool: Mempool, outbound_nmi: OutboundMempoolServiceInterface) -> Self {
        Self { mempool, outbound_nmi }
    }

    /// Handle inbound Mempool service requests from remote nodes and local services.
    pub async fn handle_request(&mut self, request: MempoolRequest) -> Result<MempoolResponse, MempoolServiceError> {
        debug!(target: LOG_TARGET, "Handling remote request: {}", request);
        use MempoolRequest::*;
        match request {
            GetStats => Ok(MempoolResponse::Stats(self.mempool.stats().await?)),
            GetState => Ok(MempoolResponse::State(self.mempool.state().await?)),
            GetTxStateByExcessSig(excess_sig) => Ok(MempoolResponse::TxStorage(
                self.mempool.has_tx_with_excess_sig(excess_sig).await?,
            )),
            SubmitTransaction(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Transaction ({}) submitted using request.",
                    tx.body.kernels()[0].excess_sig.get_signature().to_hex(),
                );
                Ok(MempoolResponse::TxStorage(self.submit_transaction(tx, None).await?))
            },
        }
    }

    /// Handle inbound transactions from remote wallets and local services.
    pub async fn handle_transaction(
        &mut self,
        tx: Transaction,
        source_peer: Option<NodeId>,
    ) -> Result<(), MempoolServiceError> {
        debug!(
            target: LOG_TARGET,
            "Transaction ({}) received from {}.",
            tx.body.kernels()[0].excess_sig.get_signature().to_hex(),
            source_peer
                .as_ref()
                .map(|p| format!("remote peer: {}", p))
                .unwrap_or_else(|| "local services".to_string())
        );
        self.submit_transaction(tx, source_peer).await?;
        Ok(())
    }

    /// Submits a transaction to the mempool and propagate valid transactions.
    async fn submit_transaction(
        &mut self,
        tx: Transaction,
        source_peer: Option<NodeId>,
    ) -> Result<TxStorageResponse, MempoolServiceError> {
        trace!(target: LOG_TARGET, "submit_transaction: {}.", tx);

        let tx = Arc::new(tx);
        let tx_storage = self.mempool.has_transaction(tx.clone()).await?;
        let kernel_excess_sig = tx
            .first_kernel_excess_sig()
            .ok_or(MempoolServiceError::TransactionNoKernels)?
            .get_signature()
            .to_hex();
        if tx_storage.is_stored() {
            debug!(
                target: LOG_TARGET,
                "Mempool already has transaction: {}.", kernel_excess_sig
            );
            return Ok(tx_storage);
        }
        match self.mempool.insert(tx.clone()).await {
            Ok(tx_storage) => {
                if tx_storage.is_stored() {
                    metrics::inbound_transactions(source_peer.as_ref()).inc();
                } else {
                    metrics::rejected_inbound_transactions(source_peer.as_ref()).inc();
                }
                self.update_pool_size_metrics().await;

                debug!(
                    target: LOG_TARGET,
                    "Transaction inserted into mempool: {}, pool: {}.", kernel_excess_sig, tx_storage
                );
                // propagate the tx if it was accepted to the unconfirmed pool
                if matches!(tx_storage, TxStorageResponse::UnconfirmedPool) {
                    debug!(
                        target: LOG_TARGET,
                        "Propagate transaction ({}) to network.", kernel_excess_sig,
                    );
                    self.outbound_nmi
                        .propagate_tx(tx, source_peer.into_iter().collect())
                        .await?;
                }
                Ok(tx_storage)
            },
            Err(e) => Err(MempoolServiceError::MempoolError(e)),
        }
    }

    async fn update_pool_size_metrics(&self) {
        if let Ok(stats) = self.mempool.stats().await {
            metrics::unconfirmed_pool_size().set(stats.unconfirmed_txs as i64);
            metrics::reorg_pool_size().set(stats.reorg_txs as i64);
        }
    }

    /// Handle inbound block events from the local base node service.
    pub async fn handle_block_event(&mut self, block_event: &BlockEvent) -> Result<(), MempoolServiceError> {
        use BlockEvent::*;
        match block_event {
            ValidBlockAdded(block, BlockAddResult::Ok(_)) => {
                self.mempool.process_published_block(block.clone()).await?;
            },
            ValidBlockAdded(_, BlockAddResult::ChainReorg { added, removed }) => {
                self.mempool
                    .process_reorg(
                        removed.iter().map(|b| b.to_arc_block()).collect(),
                        added.iter().map(|b| b.to_arc_block()).collect(),
                    )
                    .await?;
            },
            ValidBlockAdded(_, _) => {},
            BlockSyncRewind(removed_blocks) => {
                self.mempool
                    .process_reorg(removed_blocks.iter().map(|b| b.to_arc_block()).collect(), vec![])
                    .await?;
            },
            BlockSyncComplete(tip_block) => {
                self.mempool.process_published_block(tip_block.to_arc_block()).await?;
            },
            AddBlockFailed(_) => {},
        }

        self.update_pool_size_metrics().await;

        Ok(())
    }
}
