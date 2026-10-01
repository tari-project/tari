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
use tari_p2p::{comms_connector::PeerMessage, domain_message::DomainMessage};
use tari_transaction_components::transaction_components::Transaction;
use tari_utilities::hex::Hex;
use tokio::task;

#[cfg(feature = "metrics")]
use crate::mempool::metrics;
use crate::{
    base_node::comms_interface::{BlockEvent, BlockEvent::AddBlockErrored},
    chain_storage::BlockAddResult,
    mempool::{
        Mempool,
        MempoolError,
        TxStorageResponse,
        ValidationPermit,
        service::{
            MempoolRequest,
            MempoolResponse,
            MempoolServiceError,
            OutboundMempoolServiceInterface,
            initializer::extract_transaction,
        },
    },
};

pub const LOG_TARGET: &str = "c::mp::service::inbound_handlers";

/// The MempoolInboundHandlers is used to handle all received inbound mempool requests and transactions from remote
/// nodes.
#[derive(Clone)]
pub struct MempoolInboundHandlers {
    mempool: Mempool,
    outbound_service: OutboundMempoolServiceInterface,
}

impl MempoolInboundHandlers {
    /// Construct the MempoolInboundHandlers.
    pub fn new(mempool: Mempool, outbound_service: OutboundMempoolServiceInterface) -> Self {
        Self {
            mempool,
            outbound_service,
        }
    }

    /// Handle inbound Mempool service requests from remote nodes and local services.
    pub async fn handle_request(&mut self, request: MempoolRequest) -> Result<MempoolResponse, MempoolServiceError> {
        trace!(target: LOG_TARGET, "Handling remote request: {request}");
        use MempoolRequest::{
            FilterOutputsInMempool,
            GetFeePerGramStats,
            GetState,
            GetStats,
            GetTxStateByExcessSig,
            SubmitTransaction,
        };
        match request {
            GetStats => Ok(MempoolResponse::Stats(self.mempool.stats().await?)),
            GetState => Ok(MempoolResponse::State(self.mempool.state().await?)),
            GetTxStateByExcessSig(excess_sig) => Ok(MempoolResponse::TxStorage(
                self.mempool.has_tx_with_excess_sig(excess_sig).await?,
            )),
            SubmitTransaction(tx) => {
                let first_tx_kernel_excess_sig = tx
                    .first_kernel_excess_sig()
                    .ok_or(MempoolServiceError::TransactionNoKernels)?
                    .get_signature()
                    .to_hex();
                debug!(
                    target: LOG_TARGET,
                    "Transaction ({first_tx_kernel_excess_sig}) submitted using request."
                );
                Ok(MempoolResponse::TxStorage(
                    self.submit_transaction(tx, None, None).await?,
                ))
            },
            GetFeePerGramStats { count, tip_height } => {
                let stats = self.mempool.get_fee_per_gram_stats(count, tip_height).await?;
                Ok(MempoolResponse::FeePerGramStats { response: stats })
            },
            FilterOutputsInMempool(hashes) => Ok(MempoolResponse::FilteredOutputs(
                self.mempool.filter_outputs_in_mempool(hashes).await?,
            )),
        }
    }

    /// Acquire a mempool validation permit, for [MempoolInboundHandlers::submit_transaction_request]
    pub async fn acquire_validation_permit(&self) -> Result<ValidationPermit, MempoolServiceError> {
        Ok(self.mempool.acquire_validation_permit().await?)
    }

    /// Handle a `SubmitTransaction` request (as [MempoolInboundHandlers::handle_request] does), validating with the
    /// given, already acquired, permit
    pub async fn submit_transaction_request(
        &mut self,
        tx: Transaction,
        permit: ValidationPermit,
    ) -> Result<MempoolResponse, MempoolServiceError> {
        let first_tx_kernel_excess_sig = tx
            .first_kernel_excess_sig()
            .ok_or(MempoolServiceError::TransactionNoKernels)?
            .get_signature()
            .to_hex();
        debug!(
            target: LOG_TARGET,
            "Transaction ({first_tx_kernel_excess_sig}) submitted using request."
        );
        Ok(MempoolResponse::TxStorage(
            self.submit_transaction(tx, None, Some(permit)).await?,
        ))
    }

    /// Handle a raw inbound transaction message from a remote peer.
    ///
    /// A mempool validation permit is acquired first, and covers both decoding the message (on a blocking thread) and
    /// validating the transaction, so that the number of inbound transactions being decoded or validated at once is
    /// bounded. A message that cannot be decoded is logged and dropped, releasing the permit.
    pub async fn handle_transaction_message(&mut self, msg: Arc<PeerMessage>) -> Result<(), MempoolServiceError> {
        let permit = self.mempool.acquire_validation_permit().await?;
        let decoded = task::spawn_blocking(move || extract_transaction(&msg))
            .await
            .map_err(MempoolError::from)?;
        let Some(DomainMessage::<_> { source_peer, inner, .. }) = decoded else {
            return Ok(());
        };

        debug!(
            "New transaction received: {}, from: {}",
            inner
                .first_kernel_excess_sig()
                .map(|s| s.get_signature().to_hex())
                .unwrap_or_else(|| "No kernels!".to_string()),
            source_peer.public_key,
        );
        trace!(
            target: LOG_TARGET,
            "New transaction: {}, from: {}",
            inner,
            source_peer.public_key
        );
        self.handle_transaction_inner(inner, Some(source_peer.node_id), Some(permit))
            .await
    }

    /// Handle inbound transactions from remote wallets and local services.
    pub async fn handle_transaction(
        &mut self,
        tx: Transaction,
        source_peer: Option<NodeId>,
    ) -> Result<(), MempoolServiceError> {
        self.handle_transaction_inner(tx, source_peer, None).await
    }

    async fn handle_transaction_inner(
        &mut self,
        tx: Transaction,
        source_peer: Option<NodeId>,
        permit: Option<ValidationPermit>,
    ) -> Result<(), MempoolServiceError> {
        let first_tx_kernel_excess_sig = tx
            .first_kernel_excess_sig()
            .ok_or(MempoolServiceError::TransactionNoKernels)?
            .get_signature()
            .to_hex();
        debug!(
            target: LOG_TARGET,
            "Transaction ({}) received from {}.",
            first_tx_kernel_excess_sig,
            source_peer
                .as_ref()
                .map(|p| format!("remote peer: {p}"))
                .unwrap_or_else(|| "local services".to_string())
        );
        self.submit_transaction(tx, source_peer, permit).await?;
        Ok(())
    }

    /// Submits a transaction to the mempool and propagate valid transactions. If a validation `permit` is given, it
    /// is used for validation instead of acquiring a new one.
    async fn submit_transaction(
        &mut self,
        tx: Transaction,
        source_peer: Option<NodeId>,
        permit: Option<ValidationPermit>,
    ) -> Result<TxStorageResponse, MempoolServiceError> {
        trace!(target: LOG_TARGET, "submit_transaction: {tx}");

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
                "Mempool already has transaction: {kernel_excess_sig}"
            );
            return Ok(tx_storage);
        }
        let result = match permit {
            Some(permit) => self.mempool.insert_with_permit(tx.clone(), permit).await,
            None => self.mempool.insert(tx.clone()).await,
        };
        match result {
            Ok(tx_storage) => {
                #[cfg(feature = "metrics")]
                if tx_storage.is_stored() {
                    metrics::inbound_transactions().inc();
                } else {
                    metrics::rejected_inbound_transactions().inc();
                }
                self.update_pool_size_metrics().await;

                debug!(
                    target: LOG_TARGET,
                    "Transaction inserted into mempool: {kernel_excess_sig}, pool: {tx_storage}"
                );
                // propagate the tx if it was accepted to the unconfirmed pool
                if matches!(tx_storage, TxStorageResponse::UnconfirmedPool) {
                    debug!(
                        target: LOG_TARGET,
                        "Propagate transaction ({kernel_excess_sig}) to network."
                    );
                    self.outbound_service
                        .propagate_tx(tx, source_peer.into_iter().collect())
                        .await?;
                }
                Ok(tx_storage)
            },
            Err(e) => Err(MempoolServiceError::MempoolError(e)),
        }
    }

    #[allow(clippy::cast_possible_wrap)]
    async fn update_pool_size_metrics(&self) {
        #[cfg(feature = "metrics")]
        if let Ok(stats) = self.mempool.stats().await {
            metrics::unconfirmed_pool_size().set(stats.unconfirmed_txs as i64);
            metrics::reorg_pool_size().set(stats.reorg_txs as i64);
        }
    }

    /// Handle inbound block events from the local base node service.
    pub async fn handle_block_event(&mut self, block_event: &BlockEvent) -> Result<(), MempoolServiceError> {
        use BlockEvent::{AddBlockValidationFailed, BlockSyncComplete, BlockSyncRewind, ValidBlockAdded};
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
            BlockSyncRewind(_) => {},
            BlockSyncComplete(_, _) => {
                self.mempool.process_sync().await?;
            },
            AddBlockValidationFailed {
                block: failed_block,
                source_peer,
            } |
            AddBlockErrored {
                block: failed_block,
                source_peer,
            } => {
                // Only clear mempool transactions for locally submitted blocks. A block template is built from the
                // mempool, so if adding it fails - whether at validation or at commit time - one of its transactions
                // is the likely cause; leaving them in place would make every subsequent template fail the same way.
                // Blocks from peers say nothing about our mempool and are left alone.
                if source_peer.is_none() {
                    self.mempool
                        .clear_transactions_for_failed_block(failed_block.clone())
                        .await?;
                }
            },
        }

        self.update_pool_size_metrics().await;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use prost::Message;
    use tari_p2p::tari_message::TariMessageType;
    use tari_transaction_components::{MicroMinotari, key_manager::KeyManager, tx};
    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        mempool::MempoolConfig,
        proto,
        test_helpers::{create_consensus_rules, create_peer_message},
        validation::mocks::MockValidator,
    };

    /// Receives the transactions the handlers propagate, with the peers they are not sent to
    type PropagatedTxs = mpsc::UnboundedReceiver<(Arc<Transaction>, Vec<NodeId>)>;

    fn create_handlers() -> (MempoolInboundHandlers, Mempool, PropagatedTxs) {
        let mut config = MempoolConfig::default();
        config.unconfirmed_pool.min_fee = 0;
        let mempool = Mempool::new(config, create_consensus_rules(), Box::new(MockValidator::new(true)));
        let (tx_sender, tx_receiver) = mpsc::unbounded_channel();
        let handlers = MempoolInboundHandlers::new(mempool.clone(), OutboundMempoolServiceInterface::new(tx_sender));
        (handlers, mempool, tx_receiver)
    }

    #[tokio::test]
    async fn malformed_transaction_messages_are_dropped_and_release_the_permit() {
        let (mut handlers, mempool, mut propagated) = create_handlers();
        let permits = mempool.available_validation_permits();

        // Not a protobuf transaction at all
        let msg = create_peer_message(TariMessageType::NewTransaction, vec![0xff; 64]);
        handlers.handle_transaction_message(msg).await.unwrap();
        assert_eq!(mempool.available_validation_permits(), permits);

        // A valid protobuf message that is not a valid transaction
        let body = proto::types::Transaction::default().encode_to_vec();
        let msg = create_peer_message(TariMessageType::NewTransaction, body);
        handlers.handle_transaction_message(msg).await.unwrap();
        assert_eq!(mempool.available_validation_permits(), permits);

        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
        assert!(propagated.try_recv().is_err());
    }

    #[tokio::test]
    async fn well_formed_transaction_messages_are_decoded_and_inserted() {
        let (mut handlers, mempool, mut propagated) = create_handlers();
        let permits = mempool.available_validation_permits();
        let key_manager = KeyManager::new_random().unwrap();
        let tx = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let body = proto::types::Transaction::try_from(tx.clone()).unwrap().encode_to_vec();
        let msg = create_peer_message(TariMessageType::NewTransaction, body);
        let source = msg.source_peer.node_id.clone();

        handlers.handle_transaction_message(msg).await.unwrap();
        assert_eq!(mempool.available_validation_permits(), permits);
        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 1);
        // Propagated to everyone except the peer it came from
        let (propagated_tx, excluded) = propagated.try_recv().unwrap();
        assert_eq!(propagated_tx, tx);
        assert_eq!(excluded, vec![source]);
    }

    mod add_block_errored {
        use tari_node_components::blocks::{Block, BlockHeader};

        use super::*;
        use crate::mempool::TxStorageResponse;

        /// Inserts a transaction into the mempool and returns it together with a block containing it.
        async fn insert_tx_and_build_block(mempool: &Mempool) -> (Arc<Transaction>, Arc<Block>) {
            let key_manager = KeyManager::new_random().unwrap();
            let tx = Arc::new(
                tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                    .expect("Failed to get tx")
                    .0,
            );
            mempool.insert(tx.clone()).await.unwrap();
            assert_eq!(
                mempool.has_transaction(tx.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            let block = Arc::new(Block::new(BlockHeader::new(0), tx.body.clone()));
            (tx, block)
        }

        #[tokio::test]
        async fn a_local_block_that_fails_at_commit_evicts_its_transactions() {
            // A locally built block template that passes validation but fails at commit time (e.g. a storage error
            // applying a validator node exit) is published as `AddBlockErrored` with no source peer. Its transactions
            // must be evicted, exactly as for a validation failure, otherwise every subsequent template would be
            // built from the same transactions and fail the same way.
            let mut config = MempoolConfig::default();
            config.unconfirmed_pool.min_fee = 0;
            let validator = MockValidator::new(true);
            let is_valid = validator.shared_flag();
            let mempool = Mempool::new(config, create_consensus_rules(), Box::new(validator));
            let (tx_sender, _tx_receiver) = mpsc::unbounded_channel();
            let mut handlers =
                MempoolInboundHandlers::new(mempool.clone(), OutboundMempoolServiceInterface::new(tx_sender));
            let (tx, block) = insert_tx_and_build_block(&mempool).await;

            // The offending transaction no longer passes validation once it is re-checked.
            is_valid.set(false);
            handlers
                .handle_block_event(&BlockEvent::AddBlockErrored {
                    block,
                    source_peer: None,
                })
                .await
                .unwrap();

            assert_ne!(
                mempool.has_transaction(tx).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
        }

        #[tokio::test]
        async fn a_peer_block_that_fails_at_commit_leaves_the_mempool_alone() {
            let (mut handlers, mempool, _propagated) = create_handlers();
            let (tx, block) = insert_tx_and_build_block(&mempool).await;

            handlers
                .handle_block_event(&BlockEvent::AddBlockErrored {
                    block,
                    source_peer: Some(NodeId::default()),
                })
                .await
                .unwrap();

            assert_eq!(
                mempool.has_transaction(tx).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
        }
    }
}
