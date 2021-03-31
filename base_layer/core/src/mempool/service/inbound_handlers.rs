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
    chain_storage::BlockAddResult,
    mempool::{
        async_mempool,
        service::{MempoolRequest, MempoolResponse, MempoolServiceError, OutboundMempoolServiceInterface},
        Mempool,
        MempoolStateEvent,
        TxStorageResponse,
    },
    transactions::transaction::Transaction,
};
use log::*;
use std::sync::Arc;
use tari_comms::peer_manager::NodeId;
use tari_crypto::tari_utilities::hex::Hex;
use tokio::sync::broadcast;

pub const LOG_TARGET: &str = "c::mp::service::inbound_handlers";

/// The MempoolInboundHandlers is used to handle all received inbound mempool requests and transactions from remote
/// nodes.
#[derive(Clone)]
pub struct MempoolInboundHandlers {
    event_publisher: broadcast::Sender<MempoolStateEvent>,
    mempool: Mempool,
    outbound_nmi: OutboundMempoolServiceInterface,
}

impl MempoolInboundHandlers {
    /// Construct the MempoolInboundHandlers.
    pub fn new(
        event_publisher: broadcast::Sender<MempoolStateEvent>,
        mempool: Mempool,
        outbound_nmi: OutboundMempoolServiceInterface,
    ) -> Self
    {
        Self {
            event_publisher,
            mempool,
            outbound_nmi,
        }
    }

    /// Handle inbound Mempool service requests from remote nodes and local services.
    pub async fn handle_request(&mut self, request: MempoolRequest) -> Result<MempoolResponse, MempoolServiceError> {
        debug!(target: LOG_TARGET, "Handling remote request: {}", request);
        use MempoolRequest::*;
        match request {
            GetStats => Ok(MempoolResponse::Stats(
                async_mempool::stats(self.mempool.clone()).await?,
            )),
            GetState => Ok(MempoolResponse::State(
                async_mempool::state(self.mempool.clone()).await?,
            )),
            GetTxStateByExcessSig(excess_sig) => Ok(MempoolResponse::TxStorage(
                async_mempool::has_tx_with_excess_sig(self.mempool.clone(), excess_sig).await?,
            )),
            SubmitTransaction(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Transaction ({}) submitted using request.",
                    tx.body.kernels()[0].excess_sig.get_signature().to_hex(),
                );
                Ok(MempoolResponse::TxStorage(self.submit_transaction(tx, vec![]).await?))
            },
        }
    }

    /// Handle inbound transactions from remote wallets and local services.
    pub async fn handle_transaction(
        &mut self,
        tx: Transaction,
        source_peer: Option<NodeId>,
    ) -> Result<(), MempoolServiceError>
    {
        debug!(
            target: LOG_TARGET,
            "Transaction ({}) received from {}.",
            tx.body.kernels()[0].excess_sig.get_signature().to_hex(),
            source_peer
                .as_ref()
                .map(|p| format!("remote peer: {}", p))
                .unwrap_or_else(|| "local services".to_string())
        );
        let exclude_peers = source_peer.into_iter().collect();
        self.submit_transaction(tx, exclude_peers).await.map(|_| ())
    }

    // Submits a transaction to the mempool and propagate valid transactions.
    async fn submit_transaction(
        &mut self,
        tx: Transaction,
        exclude_peers: Vec<NodeId>,
    ) -> Result<TxStorageResponse, MempoolServiceError>
    {
        trace!(target: LOG_TARGET, "submit_transaction: {}.", tx);
        let tx_storage =
            async_mempool::has_tx_with_excess_sig(self.mempool.clone(), tx.body.kernels()[0].excess_sig.clone())
                .await?;

        let kernel_excess_sig = tx.body.kernels()[0].excess_sig.get_signature().to_hex();
        if tx_storage.is_stored() {
            debug!(
                target: LOG_TARGET,
                "Mempool already has transaction: {}", kernel_excess_sig
            );
            return Ok(tx_storage);
        }

        match async_mempool::insert(self.mempool.clone(), Arc::new(tx.clone())).await {
            Ok(tx_storage) => {
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
                    self.outbound_nmi.propagate_tx(tx, exclude_peers).await?;
                }
                Ok(tx_storage)
            },
            Err(e) => Err(MempoolServiceError::MempoolError(e)),
        }
    }

    /// Handle inbound block events from the local base node service.
    pub async fn handle_block_event(&mut self, block_event: &BlockEvent) -> Result<(), MempoolServiceError> {
        use BlockEvent::*;
        match block_event {
            ValidBlockAdded(block, BlockAddResult::Ok(_), broadcast) => {
                async_mempool::process_published_block(self.mempool.clone(), block.clone()).await?;
                if broadcast.is_true() {
                    let _ = self.event_publisher.send(MempoolStateEvent::Updated);
                }
            },
            ValidBlockAdded(_, BlockAddResult::ChainReorg(removed_blocks, added_blocks), broadcast) => {
                async_mempool::process_reorg(
                    self.mempool.clone(),
                    removed_blocks.iter().map(|b| b.block.clone().into()).collect(),
                    added_blocks.iter().map(|b| b.block.clone().into()).collect(),
                )
                .await?;
                if broadcast.is_true() {
                    let _ = self.event_publisher.send(MempoolStateEvent::Updated);
                }
            },
            BlockSyncRewind(removed_blocks) if !removed_blocks.is_empty() => {
                async_mempool::process_reorg(
                    self.mempool.clone(),
                    removed_blocks.iter().map(|b| b.block.clone().into()).collect(),
                    vec![],
                )
                .await?;
                let _ = self.event_publisher.send(MempoolStateEvent::Updated);
            },
            BlockSyncComplete(tip_block) => {
                async_mempool::process_published_block(self.mempool.clone(), tip_block.block.clone().into()).await?;
                let _ = self.event_publisher.send(MempoolStateEvent::Updated);
            },
            _ => {},
        }

        Ok(())
    }
}
