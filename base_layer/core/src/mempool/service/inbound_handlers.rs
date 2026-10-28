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
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId};
use tari_p2p::{comms_connector::PeerMessage, domain_message::DomainMessage};
use tari_transaction_components::{BanPeriod, BanReason, transaction_components::Transaction};
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
            PeerBanPeriods,
            initializer::{ExtractTransactionError, extract_transaction},
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
    peer_bans: Option<(ConnectivityRequester, PeerBanPeriods)>,
}

impl MempoolInboundHandlers {
    /// Construct the MempoolInboundHandlers.
    pub fn new(mempool: Mempool, outbound_service: OutboundMempoolServiceInterface) -> Self {
        Self {
            mempool,
            outbound_service,
            peer_bans: None,
        }
    }

    /// Ban peers that send transactions which are never valid on the gossip path.
    pub fn with_peer_bans(mut self, connectivity: ConnectivityRequester, periods: PeerBanPeriods) -> Self {
        self.peer_bans = Some((connectivity, periods));
        self
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
                    self.submit_transaction(tx, None, None).await?.0,
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
            self.submit_transaction(tx, None, Some(permit)).await?.0,
        ))
    }

    /// Handle a raw inbound transaction message from a remote peer.
    ///
    /// Encrypted transaction messages are dropped without decoding, validation or a ban: no honest sender encrypts
    /// mempool gossip.
    ///
    /// Otherwise a mempool validation permit is acquired first, and covers both decoding the message (on a blocking
    /// thread) and validating the transaction, so that the number of inbound transactions being decoded or validated
    /// at once is bounded. A message that cannot be decoded is logged and dropped, releasing the permit. The peer is
    /// banned if the transaction fails a final internal consistency check or has a covenant over the token limit.
    pub async fn handle_transaction_message(&mut self, msg: Arc<PeerMessage>) -> Result<(), MempoolServiceError> {
        if msg.dht_header.flags.is_encrypted() {
            debug!(
                target: LOG_TARGET,
                "Dropping encrypted transaction message from peer {}",
                msg.source_peer.node_id.short_str()
            );
            return Ok(());
        }
        let permit = self.mempool.acquire_validation_permit().await?;
        let raw = msg.clone();
        let decoded = task::spawn_blocking(move || extract_transaction(&raw))
            .await
            .map_err(MempoolError::from)?;
        let DomainMessage::<_> { source_peer, inner, .. } = match decoded {
            Ok(decoded) => decoded,
            Err(ExtractTransactionError::CovenantTokenLimit) => {
                drop(permit);
                let reason = BanReason {
                    reason: ExtractTransactionError::CovenantTokenLimit.to_string(),
                    ban_duration: BanPeriod::Long,
                };
                self.ban_peer(&msg.source_peer.node_id, reason).await;
                return Ok(());
            },
            Err(_) => return Ok(()),
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
        let ban_reason = self
            .handle_transaction_inner(inner, Some(source_peer.node_id.clone()), Some(permit))
            .await?;
        let Some(reason) = ban_reason else {
            return Ok(());
        };
        self.ban_peer(&source_peer.node_id, reason).await;
        Ok(())
    }

    /// Handle inbound transactions from remote wallets and local services.
    pub async fn handle_transaction(
        &mut self,
        tx: Transaction,
        source_peer: Option<NodeId>,
    ) -> Result<(), MempoolServiceError> {
        self.handle_transaction_inner(tx, source_peer, None).await?;
        Ok(())
    }

    /// Returns a ban reason if the transaction failed a final internal consistency check
    async fn handle_transaction_inner(
        &mut self,
        tx: Transaction,
        source_peer: Option<NodeId>,
        permit: Option<ValidationPermit>,
    ) -> Result<Option<BanReason>, MempoolServiceError> {
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
        let (_, ban_reason) = self.submit_transaction(tx, source_peer, permit).await?;
        Ok(ban_reason)
    }

    /// Submits a transaction to the mempool and propagate valid transactions. If a validation `permit` is given, it
    /// is used for validation instead of acquiring a new one. Also returns a ban reason if the transaction failed a
    /// final internal consistency check; only the gossip path acts on it.
    async fn submit_transaction(
        &mut self,
        tx: Transaction,
        source_peer: Option<NodeId>,
        permit: Option<ValidationPermit>,
    ) -> Result<(TxStorageResponse, Option<BanReason>), MempoolServiceError> {
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
            return Ok((tx_storage, None));
        }
        let result = self.mempool.insert_with_ban_reason(tx.clone(), permit).await;
        match result {
            Ok((tx_storage, ban_reason)) => {
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
                Ok((tx_storage, ban_reason))
            },
            Err(e) => Err(MempoolServiceError::MempoolError(e)),
        }
    }

    async fn ban_peer(&mut self, node_id: &NodeId, reason: BanReason) {
        let Some((connectivity, periods)) = &mut self.peer_bans else {
            debug!(
                target: LOG_TARGET,
                "Peer bans are not enabled, not banning peer {node_id}: {}", reason.reason
            );
            return;
        };
        let duration = periods.duration(reason.ban_duration);
        warn!(
            target: LOG_TARGET,
            "Banning peer {node_id} for {duration:?}: {}", reason.reason
        );
        if let Err(e) = connectivity
            .ban_peer_until(node_id.clone(), duration, reason.reason)
            .await
        {
            error!(target: LOG_TARGET, "Failed to ban peer: {e:?}");
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

    mod peer_bans {
        use std::time::Duration;

        use tari_common_types::{
            chain_metadata::ChainMetadata,
            types::{CompressedPublicKey, PrivateKey},
        };
        use tari_comms::test_utils::mocks::{ConnectivityManagerMockState, create_connectivity_mock};
        use tari_comms_dht::envelope::DhtMessageFlags;
        use tari_crypto::keys::SecretKey;
        use tari_script::script;
        use tari_transaction_components::{
            crypto_factories::CryptoFactories,
            test_helpers::{create_kernel_only_transaction, create_signature},
            transaction_components::{KernelFeatures, SpentOutput, TransactionError, covenants::CovenantError},
            validation::AggregatedBodyValidationError,
        };

        use super::*;
        use crate::{
            mempool::test_utils::{covenant_over_token_limit, transaction_proto_with_covenant},
            test_helpers::blockchain::{TestBlockchain, create_test_blockchain_db},
            validation::{TransactionValidator, ValidationError, transaction::TransactionFullValidator},
        };

        const LONG_BAN: Duration = Duration::from_secs(2 * 60 * 60);

        struct Bans {
            connectivity: ConnectivityRequester,
            state: ConnectivityManagerMockState,
        }

        impl Bans {
            /// The mock handles requests in order, so once a later request is answered every earlier ban is recorded.
            async fn take(&mut self) -> Vec<(NodeId, Duration, String)> {
                self.connectivity.get_connectivity_status().await.unwrap();
                self.state.take_banned_peers().await
            }
        }

        /// Chain-linked validation passes; internal consistency validation fails with the given error
        struct InternalFailureValidator(fn() -> ValidationError);

        impl TransactionValidator for InternalFailureValidator {
            fn validate_full(&self, _tx: &Transaction) -> Result<(), ValidationError> {
                Err((self.0)())
            }

            fn validate_chain_linked(&self, _tx: &Transaction) -> Result<(), ValidationError> {
                Ok(())
            }

            fn validate_internal_consistency(
                &self,
                _tx: &Transaction,
                _tip: Option<&ChainMetadata>,
            ) -> Result<(), ValidationError> {
                Err((self.0)())
            }
        }

        fn create_handlers_with_bans(
            validator: Box<dyn TransactionValidator>,
        ) -> (MempoolInboundHandlers, Mempool, PropagatedTxs, Bans) {
            let mut config = MempoolConfig::default();
            config.unconfirmed_pool.min_fee = 0;
            let mempool = Mempool::new(config, create_consensus_rules(), validator);
            let (tx_sender, propagated) = mpsc::unbounded_channel();
            let (connectivity, mock) = create_connectivity_mock();
            let state = mock.spawn();
            let handlers =
                MempoolInboundHandlers::new(mempool.clone(), OutboundMempoolServiceInterface::new(tx_sender))
                    .with_peer_bans(connectivity.clone(), PeerBanPeriods::default());
            (handlers, mempool, propagated, Bans { connectivity, state })
        }

        fn chain_validator() -> Box<dyn TransactionValidator> {
            let db = create_test_blockchain_db();
            let rules = db.rules().clone();
            Box::new(TransactionFullValidator::new(
                CryptoFactories::default(),
                true,
                db,
                rules,
            ))
        }

        fn coinbase_kernel_tx() -> Transaction {
            create_kernel_only_transaction(
                &PrivateKey::random(&mut rand::rng()),
                0,
                KernelFeatures::COINBASE_KERNEL,
            )
        }

        fn unbalanced_tx() -> Transaction {
            let mut tx =
                create_kernel_only_transaction(&PrivateKey::random(&mut rand::rng()), 0, KernelFeatures::empty());
            tx.offset = PrivateKey::default();
            tx
        }

        fn transaction_message(tx: Transaction) -> PeerMessage {
            let body = proto::types::Transaction::try_from(tx).unwrap().encode_to_vec();
            Arc::try_unwrap(create_peer_message(TariMessageType::NewTransaction, body)).unwrap()
        }

        #[tokio::test]
        async fn gossiped_transaction_failing_internal_validation_bans_source() {
            let (mut handlers, mempool, mut propagated, mut bans) = create_handlers_with_bans(chain_validator());
            let permits = mempool.available_validation_permits();

            for tx in [coinbase_kernel_tx(), unbalanced_tx()] {
                let msg = transaction_message(tx);
                let source = msg.source_peer.node_id.clone();
                handlers.handle_transaction_message(Arc::new(msg)).await.unwrap();

                let banned = bans.take().await;
                assert_eq!(banned.len(), 1);
                let (node_id, duration, _) = banned.first().unwrap();
                assert_eq!(*node_id, source);
                assert_eq!(*duration, LONG_BAN);
            }
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
            assert!(propagated.try_recv().is_err());
            assert_eq!(mempool.available_validation_permits(), permits);

            // Signed by the sending peer itself
            let mut msg = transaction_message(coinbase_kernel_tx());
            msg.authenticated_origin = Some(msg.source_peer.public_key.clone());
            let source = msg.source_peer.node_id.clone();
            handlers.handle_transaction_message(Arc::new(msg)).await.unwrap();
            let banned = bans.take().await;
            assert_eq!(banned.len(), 1);
            assert_eq!(banned.first().unwrap().0, source);
        }

        /// A transaction message whose output covenant field holds `covenant`, behind its borsh (varint) length prefix
        fn transaction_message_with_covenant(covenant: &[u8]) -> PeerMessage {
            let tx = transaction_proto_with_covenant(covenant);
            let msg = create_peer_message(TariMessageType::NewTransaction, tx.encode_to_vec());
            Arc::try_unwrap(msg).unwrap()
        }

        #[tokio::test]
        async fn gossiped_transaction_with_covenant_over_token_limit_bans_source() {
            let (mut handlers, mempool, mut propagated, mut bans) = create_handlers_with_bans(chain_validator());
            let permits = mempool.available_validation_permits();
            let msg = transaction_message_with_covenant(&covenant_over_token_limit());
            let source = msg.source_peer.node_id.clone();

            handlers.handle_transaction_message(Arc::new(msg)).await.unwrap();

            let banned = bans.take().await;
            assert_eq!(banned.len(), 1);
            let (node_id, duration, reason) = banned.first().unwrap();
            assert_eq!(*node_id, source);
            assert_eq!(*duration, LONG_BAN);
            assert_eq!(reason, "Transaction covenant exceeds token limit");
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
            assert!(propagated.try_recv().is_err());
            assert_eq!(mempool.available_validation_permits(), permits);

            // Mempool sync and RPC `submit_transaction` reject the same transaction without a ban: see
            // `responder_rejects_a_transaction_with_a_covenant_over_the_token_limit_without_a_ban` and
            // `submit_transaction_rejects_a_covenant_over_the_token_limit`
        }

        /// The decoder ignores the covenant field of a compact input, so an over-limit covenant there is not a reason
        /// to ban
        #[tokio::test]
        async fn gossiped_transaction_with_covenant_over_token_limit_on_a_compact_input_does_not_ban() {
            let (mut handlers, mempool, mut propagated, mut bans) = create_handlers_with_bans(chain_validator());
            let permits = mempool.available_validation_permits();
            let mut tx = transaction_proto_with_covenant(&[]);
            let input = tx.body.as_mut().unwrap().inputs.first_mut().unwrap();
            input.commitment = None;
            input.output_hash = vec![1; 32];
            let covenant = covenant_over_token_limit();
            input.covenant.clear();
            prost::encoding::encode_varint(covenant.len() as u64, &mut input.covenant);
            input.covenant.extend_from_slice(&covenant);
            let msg = create_peer_message(TariMessageType::NewTransaction, tx.encode_to_vec());

            handlers.handle_transaction_message(msg).await.unwrap();

            assert!(bans.take().await.is_empty());
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
            assert!(propagated.try_recv().is_err());
            assert_eq!(mempool.available_validation_permits(), permits);
        }

        #[tokio::test]
        async fn gossiped_transaction_with_malformed_covenant_is_dropped_without_ban() {
            let (mut handlers, mempool, mut propagated, mut bans) = create_handlers_with_bans(chain_validator());
            let permits = mempool.available_validation_permits();
            let msg = transaction_message_with_covenant(&[0xff]);

            handlers.handle_transaction_message(Arc::new(msg)).await.unwrap();

            assert!(bans.take().await.is_empty());
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
            assert!(propagated.try_recv().is_err());
            assert_eq!(mempool.available_validation_permits(), permits);
        }

        #[tokio::test]
        async fn gossiped_transaction_signed_by_other_key_bans_source() {
            let (mut handlers, _mempool, _propagated, mut bans) = create_handlers_with_bans(chain_validator());
            let mut msg = transaction_message(coinbase_kernel_tx());
            let (_, other_origin) = CompressedPublicKey::random_keypair(&mut rand::rng());
            assert!(!msg.dht_header.flags.is_encrypted());
            msg.authenticated_origin = Some(other_origin);
            let source = msg.source_peer.node_id.clone();

            handlers.handle_transaction_message(Arc::new(msg)).await.unwrap();

            let banned = bans.take().await;
            assert_eq!(banned.len(), 1);
            assert_eq!(banned.first().unwrap().0, source);
        }

        #[tokio::test]
        async fn encrypted_transaction_is_dropped_without_ban() {
            let (mut handlers, mempool, mut propagated, mut bans) = create_handlers_with_bans(chain_validator());
            let permits = mempool.available_validation_permits();
            let mut msg = transaction_message(coinbase_kernel_tx());
            let (_, other_origin) = CompressedPublicKey::random_keypair(&mut rand::rng());
            assert_ne!(other_origin, msg.source_peer.public_key);
            msg.authenticated_origin = Some(other_origin);
            msg.dht_header.flags = DhtMessageFlags::ENCRYPTED;

            handlers.handle_transaction_message(Arc::new(msg)).await.unwrap();

            assert!(bans.take().await.is_empty());
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
            assert!(propagated.try_recv().is_err());
            assert_eq!(mempool.available_validation_permits(), permits);
        }

        #[tokio::test]
        async fn encrypted_covenant_over_token_limit_is_dropped_without_ban() {
            let (mut handlers, mempool, mut propagated, mut bans) = create_handlers_with_bans(chain_validator());
            let permits = mempool.available_validation_permits();
            let mut msg = transaction_message_with_covenant(&covenant_over_token_limit());
            let (_, other_origin) = CompressedPublicKey::random_keypair(&mut rand::rng());
            assert_ne!(other_origin, msg.source_peer.public_key);
            msg.authenticated_origin = Some(other_origin);
            msg.dht_header.flags = DhtMessageFlags::ENCRYPTED;

            handlers.handle_transaction_message(Arc::new(msg)).await.unwrap();

            assert!(bans.take().await.is_empty());
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
            assert!(propagated.try_recv().is_err());
            assert_eq!(mempool.available_validation_permits(), permits);
        }

        #[tokio::test]
        async fn encrypted_valid_transaction_is_dropped() {
            let (mut handlers, mempool, mut propagated, mut bans) =
                create_handlers_with_bans(Box::new(MockValidator::new(true)));
            let key_manager = KeyManager::new_random().unwrap();
            let tx = tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0;
            let mut msg = transaction_message(tx);
            msg.dht_header.flags = DhtMessageFlags::ENCRYPTED;

            handlers.handle_transaction_message(Arc::new(msg)).await.unwrap();

            assert!(bans.take().await.is_empty());
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
            assert!(propagated.try_recv().is_err());
        }

        #[tokio::test]
        async fn gossiped_transaction_failing_chain_validation_does_not_ban() {
            let mut blockchain = TestBlockchain::create(create_consensus_rules());
            let (block, _) = blockchain.add_next_tip(block_spec!("A")).unwrap();
            let mined_kernel = block.block().body.kernels().first().unwrap().clone();
            let db = blockchain.db().clone();
            let rules = db.rules().clone();
            let validator = TransactionFullValidator::new(CryptoFactories::default(), true, db, rules);
            let (mut handlers, mempool, _propagated, mut bans) = create_handlers_with_bans(Box::new(validator));

            // Spends outputs that are not in the chain
            let key_manager = KeyManager::new_random().unwrap();
            let orphan = tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0;
            // Reuses a kernel excess that is already in the chain
            let mut excess_in_chain = mined_kernel;
            excess_in_chain.excess_sig = create_signature(
                PrivateKey::random(&mut rand::rng()),
                0.into(),
                0,
                KernelFeatures::empty(),
            );
            let excess_in_chain = Transaction::new(
                vec![],
                vec![],
                vec![excess_in_chain],
                PrivateKey::default(),
                PrivateKey::default(),
            );

            let response = mempool.insert(Arc::new(orphan.clone())).await.unwrap();
            assert_eq!(response, TxStorageResponse::NotStoredOrphan);
            let response = mempool.insert(Arc::new(excess_in_chain.clone())).await.unwrap();
            assert!(
                matches!(&response, TxStorageResponse::NotStored(Some(msg)) if msg.contains("already exists in the chain")),
                "{response:?}"
            );

            for tx in [orphan, excess_in_chain] {
                handlers
                    .handle_transaction_message(Arc::new(transaction_message(tx)))
                    .await
                    .unwrap();
                assert!(bans.take().await.is_empty());
            }
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
        }

        #[tokio::test]
        async fn gossiped_transaction_with_tip_dependent_failure_does_not_ban() {
            let errors: [fn() -> ValidationError; 4] = [
                || AggregatedBodyValidationError::MaturityError.into(),
                // Covenants can read the block height
                || AggregatedBodyValidationError::CovenantError(CovenantError::NoMatchingOutputs).into(),
                || AggregatedBodyValidationError::CovenantWorkLimitExceeded { limit: 1 }.into(),
                || AggregatedBodyValidationError::CovenantOutputsClaimedTwice { inputs: 2, matched: 1 }.into(),
            ];
            for error in errors {
                let (mut handlers, mempool, _propagated, mut bans) =
                    create_handlers_with_bans(Box::new(InternalFailureValidator(error)));
                handlers
                    .handle_transaction_message(Arc::new(transaction_message(unbalanced_tx())))
                    .await
                    .unwrap();
                assert!(bans.take().await.is_empty());
                assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
            }
        }

        /// A copy of a fresh transaction whose input script reads the block height
        fn height_reading_tx() -> Transaction {
            let key_manager = KeyManager::new_random().unwrap();
            let tx = tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0;
            let mut inputs = tx.body.inputs().clone();
            match &mut inputs.first_mut().unwrap().spent_output {
                SpentOutput::OutputData { script, .. } => {
                    *script = script!(CheckHeight(1) GtZero IfThen Nop Else Nop EndIf).unwrap()
                },
                SpentOutput::OutputHash(_) => panic!("Expected a full input"),
            }
            Transaction::new(
                inputs,
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                tx.offset.clone(),
                tx.script_offset.clone(),
            )
        }

        #[tokio::test]
        async fn gossiped_transaction_with_height_dependent_script_failure_does_not_ban() {
            let (mut handlers, mempool, _propagated, mut bans) =
                create_handlers_with_bans(Box::new(InternalFailureValidator(|| {
                    AggregatedBodyValidationError::InputScript {
                        error: TransactionError::InvalidSignatureError("Verifying script signature".to_string()),
                        height_dependent: true,
                    }
                    .into()
                })));
            handlers
                .handle_transaction_message(Arc::new(transaction_message(height_reading_tx())))
                .await
                .unwrap();
            assert!(bans.take().await.is_empty());
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);

            // The same failure on an input that does not read the height
            let (mut handlers, _mempool, _propagated, mut bans) =
                create_handlers_with_bans(Box::new(InternalFailureValidator(|| {
                    AggregatedBodyValidationError::InputScript {
                        error: TransactionError::InvalidSignatureError("Verifying script signature".to_string()),
                        height_dependent: false,
                    }
                    .into()
                })));
            handlers
                .handle_transaction_message(Arc::new(transaction_message(height_reading_tx())))
                .await
                .unwrap();
            assert_eq!(bans.take().await.len(), 1);
        }

        #[tokio::test]
        async fn gossiped_transaction_with_height_reading_script_and_unrelated_failure_bans() {
            let (mut handlers, _mempool, _propagated, mut bans) =
                create_handlers_with_bans(Box::new(InternalFailureValidator(|| {
                    AggregatedBodyValidationError::TransactionError(TransactionError::InvalidSignatureError(
                        "Verifying kernel signature".to_string(),
                    ))
                    .into()
                })));
            handlers
                .handle_transaction_message(Arc::new(transaction_message(height_reading_tx())))
                .await
                .unwrap();
            assert_eq!(bans.take().await.len(), 1);
        }

        #[tokio::test]
        async fn submitted_transaction_failing_internal_validation_does_not_ban() {
            let (mut handlers, mempool, _propagated, mut bans) = create_handlers_with_bans(chain_validator());

            handlers
                .handle_request(MempoolRequest::SubmitTransaction(coinbase_kernel_tx()))
                .await
                .unwrap();
            let permit = handlers.acquire_validation_permit().await.unwrap();
            handlers
                .submit_transaction_request(coinbase_kernel_tx(), permit)
                .await
                .unwrap();
            handlers
                .handle_transaction(coinbase_kernel_tx(), Some(NodeId::default()))
                .await
                .unwrap();

            assert!(bans.take().await.is_empty());
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
        }

        #[tokio::test]
        async fn gossiped_valid_transaction_does_not_ban() {
            let (mut handlers, mempool, _propagated, mut bans) =
                create_handlers_with_bans(Box::new(MockValidator::new(true)));
            let key_manager = KeyManager::new_random().unwrap();
            let tx = tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0;

            handlers
                .handle_transaction_message(Arc::new(transaction_message(tx)))
                .await
                .unwrap();

            assert!(bans.take().await.is_empty());
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 1);
        }
    }

    mod add_block_errored {
        use tari_node_components::blocks::{Block, BlockHeader};

        use super::*;
        use crate::mempool::TxStorageResponse;

        fn create_handlers_with_validator(validator: MockValidator) -> (MempoolInboundHandlers, Mempool) {
            let mut config = MempoolConfig::default();
            config.unconfirmed_pool.min_fee = 0;
            let mempool = Mempool::new(config, create_consensus_rules(), Box::new(validator));
            let (tx_sender, _tx_receiver) = mpsc::unbounded_channel();
            let handlers =
                MempoolInboundHandlers::new(mempool.clone(), OutboundMempoolServiceInterface::new(tx_sender));
            (handlers, mempool)
        }

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

        async fn local_block_errored(handlers: &mut MempoolInboundHandlers, block: Arc<Block>) {
            handlers
                .handle_block_event(&BlockEvent::AddBlockErrored {
                    block,
                    source_peer: None,
                })
                .await
                .unwrap();
        }

        #[tokio::test]
        async fn a_local_block_failing_at_commit_evicts_a_transaction_that_fails_revalidation() {
            // A locally built block that fails at commit time (`AddBlockErrored`, no source peer) has each of its
            // transactions removed and individually re-validated, exactly as for a validation failure. One that is
            // no longer valid (here: the validator now rejects it) is evicted.
            let validator = MockValidator::new(true);
            let is_valid = validator.shared_flag();
            let (mut handlers, mempool) = create_handlers_with_validator(validator);
            let (tx, block) = insert_tx_and_build_block(&mempool).await;

            is_valid.set(false);
            local_block_errored(&mut handlers, block).await;

            assert_ne!(
                mempool.has_transaction(tx).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
        }

        #[tokio::test]
        async fn a_local_block_failing_at_commit_reinserts_a_transaction_that_still_passes_revalidation() {
            // Re-validation is individual: a transaction that is valid on its own goes straight back into the pool,
            // even if it was the cause of the failure in combination with another transaction of the same block (e.g.
            // two exits for one validator node). Eviction alone therefore cannot break such a loop - that is done by
            // refusing to store the second transaction in the first place (the unconfirmed pool's validator node
            // conflict check, see `validator_node_conflicts` below).
            let (mut handlers, mempool) = create_handlers_with_validator(MockValidator::new(true));
            let (tx, block) = insert_tx_and_build_block(&mempool).await;

            local_block_errored(&mut handlers, block).await;

            assert_eq!(
                mempool.has_transaction(tx).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
        }

        #[tokio::test]
        async fn a_peer_block_that_fails_at_commit_leaves_the_mempool_alone() {
            let validator = MockValidator::new(true);
            let is_valid = validator.shared_flag();
            let (mut handlers, mempool) = create_handlers_with_validator(validator);
            let (tx, block) = insert_tx_and_build_block(&mempool).await;

            // Even a transaction that would now fail re-validation is not touched for a peer's block
            is_valid.set(false);
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

    mod validator_node_conflicts {
        use tari_common_types::{
            epoch::VnEpoch,
            types::{CompressedPublicKey, PrivateKey},
        };
        use tari_crypto::keys::SecretKey;
        use tari_transaction_components::transaction_components::{OutputFeatures, ValidatorNodeSignature};

        use super::*;
        use crate::mempool::TxStorageResponse;

        const NETWORK: u8 = 0x10;

        fn tx_with(features: OutputFeatures, fee: u64, key_manager: &KeyManager) -> Arc<Transaction> {
            Arc::new(
                tx!(MicroMinotari(100_000), fee: MicroMinotari(fee), inputs: 1, outputs: 1, features: features, key_manager)
                    .expect("Failed to get tx")
                    .0,
            )
        }

        fn exit_features(vn_secret_key: &PrivateKey) -> OutputFeatures {
            let signature =
                ValidatorNodeSignature::sign_for_exit(vn_secret_key, NETWORK, None, VnEpoch(0), VnEpoch(10));
            OutputFeatures::for_validator_node_exit(signature, None, VnEpoch(0), VnEpoch(10))
        }

        fn registration_features(vn_secret_key: &PrivateKey) -> OutputFeatures {
            registration_features_until(vn_secret_key, VnEpoch(10))
        }

        fn registration_features_until(vn_secret_key: &PrivateKey, max_epoch: VnEpoch) -> OutputFeatures {
            let claim = CompressedPublicKey::from_secret_key(vn_secret_key);
            let signature =
                ValidatorNodeSignature::sign_for_registration(vn_secret_key, NETWORK, None, &claim, max_epoch);
            OutputFeatures::for_validator_node_registration(signature, claim, None, max_epoch)
        }

        #[tokio::test]
        async fn a_published_block_re_validates_validator_node_transactions_and_drops_invalid_ones() {
            // A validator node transaction's validity depends on chain state that changes at epoch boundaries and
            // when validator node transactions are mined (e.g. a cancelling spend becomes invalid once the registration
            // activates), so such blocks re-check them. Ordinary transactions, and ordinary blocks, are left alone.
            let validator = crate::validation::mocks::MockValidator::new(true);
            let is_valid = validator.shared_flag();
            let mut config = crate::mempool::MempoolConfig::default();
            config.unconfirmed_pool.min_fee = 0;
            let mempool = Mempool::new(config, create_consensus_rules(), Box::new(validator));
            let key_manager = KeyManager::new_random().unwrap();

            let vn_tx = tx_with(exit_features(&PrivateKey::random(&mut rand::rng())), 5, &key_manager);
            let plain_tx = Arc::new(
                tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                    .expect("Failed to get tx")
                    .0,
            );
            for tx in [&vn_tx, &plain_tx] {
                assert_eq!(
                    mempool.insert(tx.clone()).await.unwrap(),
                    TxStorageResponse::UnconfirmedPool
                );
            }
            let block_with = |body: &tari_transaction_components::aggregated_body::AggregateBody| {
                Arc::new(tari_node_components::blocks::Block::new(
                    tari_node_components::blocks::BlockHeader::new(0),
                    body.clone(),
                ))
            };

            // The chain moves on and the validator node transaction is no longer valid, but a block with no validator
            // node transactions inside an epoch does not re-check anything
            is_valid.set(false);
            let plain_block_tx =
                tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                    .expect("Failed to get tx")
                    .0;
            mempool
                .process_published_block(block_with(&plain_block_tx.body))
                .await
                .unwrap();
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 2);

            // A block that mines a validator node transaction (for another node) re-checks the pool's validator node
            // transactions: the stale one is dropped, the ordinary one stays
            let other_vn_tx = tx_with(exit_features(&PrivateKey::random(&mut rand::rng())), 5, &key_manager);
            mempool
                .process_published_block(block_with(&other_vn_tx.body))
                .await
                .unwrap();
            assert_ne!(
                mempool.has_transaction(vn_tx).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            assert_eq!(
                mempool.has_transaction(plain_tx).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 1);
        }

        #[tokio::test]
        async fn a_validator_node_slot_is_replaced_only_by_a_strictly_higher_fee() {
            let (_handlers, mempool, _propagated) = create_handlers();
            let key_manager = KeyManager::new_random().unwrap();
            let vn_secret_key = PrivateKey::random(&mut rand::rng());

            let exit1 = tx_with(exit_features(&vn_secret_key), 5, &key_manager);
            assert_eq!(
                mempool.insert(exit1.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            // The same transaction again is still reported as stored
            assert_eq!(
                mempool.insert(exit1.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );

            // An equal or lower fee for the same validator node (exit or registration) is refused, with a reason that
            // is not a validation failure
            for (features, fee) in [
                (exit_features(&vn_secret_key), 5),
                (exit_features(&vn_secret_key), 1),
                (registration_features(&vn_secret_key), 5),
            ] {
                assert_eq!(
                    mempool.insert(tx_with(features, fee, &key_manager)).await.unwrap(),
                    TxStorageResponse::NotStoredValidatorNodeSlotTaken
                );
            }

            // A strictly higher fee replaces the holder
            let exit2 = tx_with(exit_features(&vn_secret_key), 10, &key_manager);
            assert_eq!(
                mempool.insert(exit2.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            assert_ne!(
                mempool.has_transaction(exit1.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            // ... and the slot now belongs to the new holder
            assert_eq!(
                mempool
                    .insert(tx_with(exit_features(&vn_secret_key), 10, &key_manager))
                    .await
                    .unwrap(),
                TxStorageResponse::NotStoredValidatorNodeSlotTaken
            );

            // Another validator node is unaffected
            let other = tx_with(exit_features(&PrivateKey::random(&mut rand::rng())), 5, &key_manager);
            assert_eq!(
                mempool.insert(other.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );

            // So a template never carries two transactions for one validator node
            let template = mempool.retrieve(1_000_000).await.unwrap();
            assert_eq!(template.len(), 2);
            assert!(template.contains(&exit2));
            assert!(template.contains(&other));
            assert!(!template.contains(&exit1));
        }

        #[tokio::test]
        async fn a_published_block_evicts_a_pool_transaction_for_the_same_validator_node() {
            let (_handlers, mempool, _propagated) = create_handlers();
            let key_manager = KeyManager::new_random().unwrap();
            let vn_secret_key = PrivateKey::random(&mut rand::rng());
            let t1 = tx_with(exit_features(&vn_secret_key), 5, &key_manager);
            assert_eq!(
                mempool.insert(t1.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );

            // A peer's block exits the same validator node through a different transaction
            let t2 = tx_with(exit_features(&vn_secret_key), 7, &key_manager);
            let block = Arc::new(tari_node_components::blocks::Block::new(
                tari_node_components::blocks::BlockHeader::new(0),
                t2.body.clone(),
            ));
            mempool.process_published_block(block).await.unwrap();

            assert_ne!(
                mempool.has_transaction(t1).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
        }

        #[tokio::test]
        async fn a_published_block_drops_validator_node_transactions_that_expire_before_the_next_epoch() {
            let (_handlers, mempool, _propagated) = create_handlers();
            let key_manager = KeyManager::new_random().unwrap();
            let epoch_length = create_consensus_rules().consensus_constants(0).epoch_length();
            let epoch = VnEpoch(1);

            let expiring = tx_with(
                registration_features_until(&PrivateKey::random(&mut rand::rng()), epoch),
                5,
                &key_manager,
            );
            let still_valid = tx_with(
                registration_features_until(&PrivateKey::random(&mut rand::rng()), epoch.saturating_add(VnEpoch(1))),
                5,
                &key_manager,
            );
            assert_eq!(
                mempool.insert(expiring.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            assert_eq!(
                mempool.insert(still_valid.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );

            // A block that is not the last of the epoch leaves both
            let mut header = tari_node_components::blocks::BlockHeader::new(0);
            header.height = (epoch.as_u64() + 1) * epoch_length - 2;
            let block = |header| {
                Arc::new(tari_node_components::blocks::Block::new(
                    header,
                    tari_transaction_components::aggregated_body::AggregateBody::empty(),
                ))
            };
            mempool.process_published_block(block(header.clone())).await.unwrap();
            assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 2);

            // The last block of the epoch: the next template is in the following epoch, so `expiring` is dropped
            header.height += 1;
            mempool.process_published_block(block(header)).await.unwrap();
            assert_ne!(
                mempool.has_transaction(expiring).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            assert_eq!(
                mempool.has_transaction(still_valid).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
        }
    }
}
