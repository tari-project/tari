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
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::FutureExt;
use log::*;
use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_common_types::{
    transaction::{LegacyTransactionStatus, TxId},
    types::CompressedSignature,
};
use tari_transaction_components::{
    rpc::models::{TxLocation, TxSubmissionRejectionReason},
    transaction_components::Transaction,
};
use tari_transaction_key_manager::legacy_key_manager::LegacyTransactionKeyManagerInterface;
use tari_utilities::{ByteArray, hex::Hex};
use tokio::{sync::watch, time::sleep};

use crate::{
    connectivity_service::WalletConnectivityInterface,
    transaction_service::{
        error::{TransactionServiceError, TransactionServiceProtocolError},
        handle::TransactionEvent,
        protocols::check_transaction_size,
        service::TransactionServiceResources,
        storage::{
            database::TransactionBackend,
            models::{CompletedTransaction, TxCancellationReason},
        },
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::broadcast_protocol";

/// The total number of times a transaction is submitted to the mempool (the initial submission plus resubmissions)
/// before it is cancelled. This is a hard limit for the lifetime of the protocol; the count is never reset, so a
/// transaction that the mempool keeps dropping is always cancelled after this many attempts. Resubmissions are spaced
/// at least `transaction_mempool_resubmission_window` apart.
const MAX_MEMPOOL_SUBMISSION_ATTEMPTS: usize = 5;

pub struct TransactionBroadcastProtocol<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    tx_id: TxId,
    mode: TxBroadcastMode,
    resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
    timeout_update_receiver: watch::Receiver<Duration>,
    last_resubmission: Option<Instant>,
    resubmission_attempts: usize,
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    TransactionBroadcastProtocol<TBackend, TWalletConnectivity, TKeyManagerInterface>
where
    TBackend: TransactionBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: LegacyTransactionKeyManagerInterface,
{
    pub fn new(
        tx_id: TxId,
        resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
        timeout_update_receiver: watch::Receiver<Duration>,
    ) -> Self {
        Self {
            tx_id,
            mode: TxBroadcastMode::TransactionSubmission,
            resources,
            timeout_update_receiver,
            last_resubmission: None,
            resubmission_attempts: 0,
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<TxId, TransactionServiceProtocolError<TxId>> {
        let mut shutdown = self.resources.shutdown_signal.clone();
        let mut timeout_update_receiver = self.timeout_update_receiver.clone();

        // Main protocol loop
        loop {
            let client = self.resources.connectivity.obtain_base_node_wallet_rpc_client().await;

            let completed_tx = match self.resources.db.get_completed_transaction(self.tx_id) {
                Ok(tx) => tx,
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "Cannot find Completed Transaction (TxId: {}) referred to by this Broadcast protocol: {:?}",
                        self.tx_id,
                        e
                    );
                    return Err(TransactionServiceProtocolError::new(
                        self.tx_id,
                        TransactionServiceError::TransactionDoesNotExistError,
                    ));
                },
            };

            if !(completed_tx.status == LegacyTransactionStatus::Completed ||
                completed_tx.status == LegacyTransactionStatus::Broadcast ||
                completed_tx.status == LegacyTransactionStatus::MinedUnconfirmed)
            {
                debug!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) no longer in Completed state and will stop being broadcast", self.tx_id
                );
                return Ok(self.tx_id);
            }
            if let Err(e) = check_transaction_size(&completed_tx.transaction, self.tx_id) {
                let reason = e.error.to_string();
                self.cancel_and_publish(TxCancellationReason::Oversized, Some(reason))
                    .await;
                return Err(e);
            }

            tokio::select! {
                result = self.query_or_submit_transaction(completed_tx.clone(), &client).fuse() => {
                    match self.mode {
                        TxBroadcastMode::TransactionSubmission => {
                            if result? {
                                self.mode = TxBroadcastMode::TransactionQuery;
                            }
                        },
                        TxBroadcastMode::TransactionQuery => {
                            if result? {
                                debug!(
                                    target: LOG_TARGET,
                                    "Transaction broadcast, transaction validation protocol will continue from here"
                                );
                                return Ok(self.tx_id)
                            }
                        },
                    }
                    // Wait out the remainder of the delay before proceeding with next loop
                    drop(client);
                    let delay = *timeout_update_receiver.borrow();
                    sleep(delay).await;
                },
                _ = timeout_update_receiver.changed() => {
                     info!(
                        target: LOG_TARGET,
                        "Transaction Broadcast protocol (TxId: {}) timeout updated to {:?}", self.tx_id, timeout_update_receiver.borrow()
                    );
                },
                _ = shutdown.wait() => {
                    info!(target: LOG_TARGET, "Transaction Broadcast Protocol (TxId: {}) shutting down because it received the shutdown signal", self.tx_id);
                    return Err(TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::Shutdown))
                },
            }
        }
    }

    /// Attempt to submit the transaction to the base node via RPC.
    /// # Returns:
    /// `Ok(true)` => Transaction was successfully submitted to UnconfirmedPool
    /// `Ok(false)` => There was a problem with the RPC call and this should be retried
    /// `Err(_)` => The transaction was rejected by the base node and the protocol should end.
    #[allow(clippy::too_many_lines)]
    async fn submit_transaction(
        &mut self,
        tx: Transaction,
        client: &TWalletConnectivity::BaseNodeClient,
    ) -> Result<bool, TransactionServiceProtocolError<TxId>> {
        let response = match client.submit_transaction(tx.clone()).await {
            Ok(r) => {
                info!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) submission response from Base Node: {:?}", self.tx_id, r
                );
                r
            },
            Err(e) => {
                info!(
                    target: LOG_TARGET,
                    "Submit Transaction RPC Call to Base Node failed: {e}"
                );
                return Ok(false);
            },
        };

        if !response.is_synced {
            info!(
                target: LOG_TARGET,
                "Base Node reports not being synced, submission will be retried."
            );
            return Ok(false);
        }

        if !response.accepted &&
            matches!(
                response.rejection_reason,
                TxSubmissionRejectionReason::ValidatorNodeSlotTaken | TxSubmissionRejectionReason::MempoolPolicy
            )
        {
            // Another mempool transaction registers or exits the same validator node, or a mempool policy refused this
            // one (e.g. other mempool transactions already spend its inputs). That is not a reason to cancel it
            // straight away: it may be accepted once the mempool changes, so retry later. The retries share the
            // submission attempt budget, so a refusal that persists does not keep the protocol alive forever.
            if self.resubmission_attempts < MAX_MEMPOOL_SUBMISSION_ATTEMPTS - 1 {
                self.resubmission_attempts = self.resubmission_attempts.saturating_add(1);
                info!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) not accepted yet ({}), submission will be retried (submission attempt {} \
                     of {}).",
                    self.tx_id,
                    response.rejection_reason,
                    self.resubmission_attempts.saturating_add(1),
                    MAX_MEMPOOL_SUBMISSION_ATTEMPTS
                );
                return Ok(false);
            }
            let reason = format!(
                "not accepted ({}) on all {MAX_MEMPOOL_SUBMISSION_ATTEMPTS} submission attempts",
                response.rejection_reason
            );
            error!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) has been rejected: {}, cancelling transaction", self.tx_id, reason
            );
            self.cancel_and_publish(TxCancellationReason::InvalidTransaction, Some(reason))
                .await;
            return Err(TransactionServiceProtocolError::new(
                self.tx_id,
                TransactionServiceError::MempoolRejectionInvalidTransaction,
            ));
        }

        if !response.accepted && response.rejection_reason != TxSubmissionRejectionReason::AlreadyMined {
            if let Some(ref details) = response.details {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) rejected by Base Node for reason: {} (details: {})",
                    self.tx_id,
                    response.rejection_reason,
                    details
                );
            } else {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) rejected by Base Node for reason: {}", self.tx_id, response.rejection_reason
                );
            }

            let (reason_error, reason) = match response.rejection_reason {
                TxSubmissionRejectionReason::None | TxSubmissionRejectionReason::ValidationFailed => (
                    TransactionServiceError::MempoolRejectionInvalidTransaction,
                    TxCancellationReason::InvalidTransaction,
                ),
                TxSubmissionRejectionReason::DoubleSpend => (
                    TransactionServiceError::MempoolRejectionDoubleSpend,
                    TxCancellationReason::DoubleSpend,
                ),
                TxSubmissionRejectionReason::Orphan => (
                    TransactionServiceError::MempoolRejectionOrphan,
                    TxCancellationReason::Orphan,
                ),
                TxSubmissionRejectionReason::TimeLocked => (
                    TransactionServiceError::MempoolRejectionTimeLocked,
                    TxCancellationReason::TimeLocked,
                ),
                TxSubmissionRejectionReason::FeeTooLow => (
                    TransactionServiceError::MempoolRejectionFeeTooLow,
                    TxCancellationReason::FeeTooLow,
                ),
                TxSubmissionRejectionReason::AlreadyMined => (
                    TransactionServiceError::MempoolRejectionAlreadyMined,
                    TxCancellationReason::AlreadyMined,
                ),
                // Handled above (retried within the attempt budget, then cancelled)
                TxSubmissionRejectionReason::ValidatorNodeSlotTaken | TxSubmissionRejectionReason::MempoolPolicy => {
                    return Ok(false);
                },
            };

            self.cancel_and_publish(reason, response.details).await;

            return Err(TransactionServiceProtocolError::new(self.tx_id, reason_error));
        } else if response.rejection_reason == TxSubmissionRejectionReason::AlreadyMined {
            info!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) is Already Mined according to Base Node. Will be completed by transaction \
                 validation protocol.",
                self.tx_id
            );
        } else {
            info!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) successfully submitted to UnconfirmedPool", self.tx_id
            );
            trace!(target: LOG_TARGET, "submit_transaction ({}) - {}", self.tx_id, tx,);
            self.resources
                .db
                .broadcast_completed_transaction(self.tx_id)
                .map_err(|e| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::from(e)))?;
            let _size = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionBroadcast(self.tx_id)))
                .inspect_err(|e| {
                    trace!(
                        target: LOG_TARGET,
                        "Error sending event, usually because there are no subscribers: {e:?}",
                    );
                });
        }

        self.last_resubmission = Some(Instant::now());

        Ok(true)
    }

    /// Attempt to query the location of the transaction from the base node via RPC.
    /// # Returns:
    /// `Ok(true)` => Transaction was successfully mined and confirmed
    /// `Ok(false)` => There was a problem with the RPC call or the transaction is not mined but still in the mempool
    /// and this should be retried `Err(_)` => The transaction was rejected by the base node and the protocol should
    /// end.
    async fn transaction_query(
        &mut self,
        signature: CompressedSignature,
        client: &TWalletConnectivity::BaseNodeClient,
    ) -> Result<bool, TransactionServiceProtocolError<TxId>> {
        let response = match client
            .transaction_query(
                signature.get_compressed_public_nonce().as_bytes().to_vec(),
                signature.get_signature().as_bytes().to_vec(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                info!(
                    target: LOG_TARGET,
                    "Transaction Query RPC Call to Base Node failed: {e}"
                );
                return Ok(false);
            },
        };

        // Mined?
        if response.location == TxLocation::Mined {
            info!(
                target: LOG_TARGET,
                "Broadcast transaction detected as mined, will be managed by transaction validation protocol"
            );
            Ok(true)
        } else if response.location != TxLocation::InMempool {
            // Resubmissions are spaced at least a resubmission window apart; until that window has elapsed we keep
            // querying the base node in case the transaction shows up on its own.
            if self
                .last_resubmission
                .is_some_and(|last| last.elapsed() < self.resources.config.transaction_mempool_resubmission_window)
            {
                debug!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) not found in mempool, waiting out the resubmission window before \
                     resubmitting",
                    self.tx_id
                );
                return Ok(false);
            }

            // `TxQueryResponse` does not report sync status; an unsynced base node returns `NotStored` simply because
            // it has not caught up yet, which must neither spend the submission budget nor cancel the transaction.
            if !self.is_base_node_synced(client).await {
                return Ok(false);
            }

            if self.resubmission_attempts < MAX_MEMPOOL_SUBMISSION_ATTEMPTS - 1 {
                self.resubmission_attempts = self.resubmission_attempts.saturating_add(1);
                info!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) not found in mempool, attempting to resubmit transaction (submission \
                     attempt {} of {})",
                    self.tx_id,
                    self.resubmission_attempts.saturating_add(1),
                    MAX_MEMPOOL_SUBMISSION_ATTEMPTS
                );
                self.mode = TxBroadcastMode::TransactionSubmission;
                self.last_resubmission = Some(Instant::now());
                Ok(false)
            } else {
                let reason =
                    format!("rejected by the mempool after {MAX_MEMPOOL_SUBMISSION_ATTEMPTS} submission attempts");
                error!(target: LOG_TARGET,
                    "Transaction (TxId: {}) has been {}, cancelling transaction",
                    self.tx_id, reason,
                );
                self.cancel_and_publish(TxCancellationReason::InvalidTransaction, Some(reason.clone()))
                    .await;
                Err(TransactionServiceProtocolError::new(
                    self.tx_id,
                    TransactionServiceError::MempoolRejection { reason },
                ))
            }
        } else {
            info!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) found in mempool.", self.tx_id
            );
            Ok(true)
        }
    }

    async fn is_base_node_synced(&self, client: &TWalletConnectivity::BaseNodeClient) -> bool {
        match client.get_tip_info().await {
            Ok(tip_info) if tip_info.is_synced => true,
            Ok(_) => {
                info!(
                    target: LOG_TARGET,
                    "Base Node reports not being synced during transaction query for TxId: {}, query will be retried.",
                    self.tx_id
                );
                false
            },
            Err(e) => {
                info!(
                    target: LOG_TARGET,
                    "Get Tip Info RPC Call to Base Node failed during transaction query for TxId: {}: {e}",
                    self.tx_id
                );
                false
            },
        }
    }

    async fn cancel_and_publish(&mut self, reason: TxCancellationReason, details: Option<String>) {
        self.cancel_pending_transaction(reason, details.clone()).await;
        let _size = self
            .resources
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCancelled(
                self.tx_id,
                reason,
                details.unwrap_or_default(),
            )))
            .inspect_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event because there are no subscribers: {e:?}",
                );
            });
    }

    async fn query_or_submit_transaction(
        &mut self,
        completed_transaction: CompletedTransaction,
        client: &TWalletConnectivity::BaseNodeClient,
    ) -> Result<bool, TransactionServiceProtocolError<TxId>> {
        let signature = completed_transaction
            .transaction
            .first_kernel_excess_sig()
            .ok_or_else(|| {
                TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::InvalidTransaction)
            })?;
        if self.mode == TxBroadcastMode::TransactionSubmission {
            info!(
                target: LOG_TARGET,
                "Submitting Transaction (TxId: {}) with signature '{}' to Base Node",
                self.tx_id,
                signature.clone().get_signature().to_hex(),
            );
            self.submit_transaction(completed_transaction.transaction, client).await
        } else {
            info!(
                target: LOG_TARGET,
                "Querying Transaction (TxId: {}) status on Base Node", self.tx_id
            );
            self.transaction_query(signature.clone(), client).await
        }
    }

    async fn cancel_pending_transaction(&mut self, reason: TxCancellationReason, details: Option<String>) {
        if let Err(e) = self
            .resources
            .output_manager_service
            .cancel_pending_transaction(self.tx_id)
            .await
        {
            warn!(
                target: LOG_TARGET,
                "Failed to Cancel pending outputs for TxId: {} after failed sending attempt with error {:?}",
                self.tx_id, e
            );
        }
        if let Err(e) = self
            .resources
            .db
            .reject_completed_transaction(self.tx_id, reason, details)
        {
            warn!(
                target: LOG_TARGET,
                "Failed to Cancel pending TxId: {} after failed sending attempt with error {:?}", self.tx_id, e
            );
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TxBroadcastMode {
    TransactionSubmission,
    TransactionQuery,
}

#[cfg(test)]
mod test {
    use std::{collections::VecDeque, mem::size_of, sync::Arc, time::Duration};

    use anyhow::{Error, anyhow};
    use async_trait::async_trait;
    use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
    use chrono::Utc;
    use futures::StreamExt;
    use minotari_node_wallet_client::BaseNodeWalletClient;
    use rand::Rng;
    use tari_common::configuration::Network;
    use tari_common_types::{
        tari_address::TariAddress,
        transaction::{LegacyTransactionStatus, TransactionDirection, TxId},
        types::{CompressedCommitment, CompressedPublicKey, CompressedSignature, PrivateKey},
    };
    use tari_comms::{peer_manager::PeerFeatures, test_utils::node_identity::build_node_identity};
    use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey as SecretKeyTrait};
    use tari_service_framework::reply_channel;
    use tari_shutdown::{Shutdown, ShutdownSignal};
    use tari_transaction_components::{
        MicroMinotari,
        consensus::ConsensusManager,
        crypto_factories::CryptoFactories,
        rpc::models::{
            self,
            BlockHeader,
            FeePerGramStat,
            GenerateBurnOutputProofResponse,
            GetUtxosDeletedInfoResponse,
            GetUtxosMinedInfoResponse,
            SyncUtxosByBlockResponseV0,
            TipInfoResponse,
            TxLocation,
            TxQueryResponse,
            TxSubmissionRejectionReason,
            TxSubmissionResponse,
        },
        transaction_components::{
            KernelBuilder,
            Transaction,
            TransactionOutput,
            memo_field::{MemoField, TxType},
        },
    };
    use tari_transaction_key_manager::legacy_key_manager::{MemoryKeyManager, create_new_random_key_manager};
    use tokio::sync::{Mutex, broadcast, mpsc, watch};

    use super::{MAX_MEMPOOL_SUBMISSION_ATTEMPTS, TransactionBroadcastProtocol};
    use crate::{
        connectivity_service::{OnlineStatus, WalletConnectivityInterface},
        output_manager_service::handle::{OutputManagerHandle, OutputManagerRequest, OutputManagerResponse},
        storage::sqlite_utilities::run_migration_and_create_sqlite_memory_connection,
        transaction_service::{
            config::TransactionServiceConfig,
            error::TransactionServiceError,
            handle::{TransactionEvent, TransactionEventReceiver},
            service::TransactionServiceResources,
            storage::{
                database::TransactionDatabase,
                models::{CompletedTransaction, TxCancellationReason},
                sqlite_db::TransactionServiceSqliteDatabase,
            },
        },
        util::watch::Watch,
        utxo_scanner_service::handle::UtxoScannerHandle,
    };

    struct MockRpcState {
        submit_responses: VecDeque<Result<TxSubmissionResponse, String>>,
        default_submit_response: TxSubmissionResponse,
        query_responses: VecDeque<Result<TxQueryResponse, String>>,
        default_query_response: TxQueryResponse,
        tip_info_responses: VecDeque<Result<TipInfoResponse, String>>,
        default_tip_info: TipInfoResponse,
        submit_calls: usize,
        query_calls: usize,
        tip_info_calls: usize,
    }

    impl Default for MockRpcState {
        fn default() -> Self {
            Self {
                submit_responses: VecDeque::new(),
                default_submit_response: TxSubmissionResponse {
                    accepted: true,
                    rejection_reason: TxSubmissionRejectionReason::None,
                    is_synced: true,
                    details: None,
                },
                query_responses: VecDeque::new(),
                default_query_response: TxQueryResponse {
                    location: TxLocation::InMempool,
                    mined_height: None,
                    mined_header_hash: None,
                    mined_timestamp: None,
                },
                tip_info_responses: VecDeque::new(),
                default_tip_info: TipInfoResponse {
                    metadata: None,
                    is_synced: true,
                },
                submit_calls: 0,
                query_calls: 0,
                tip_info_calls: 0,
            }
        }
    }

    #[derive(Clone, Default)]
    struct MockBaseNodeClient {
        state: Arc<Mutex<MockRpcState>>,
    }

    #[async_trait]
    impl BaseNodeWalletClient for MockBaseNodeClient {
        async fn get_address(&self) -> String {
            "http://127.0.0.1:18142".to_string()
        }

        async fn is_online(&self) -> bool {
            true
        }

        async fn get_last_request_latency(&self) -> Option<Duration> {
            Some(Duration::from_millis(1))
        }

        async fn get_utxos_mined_info(
            &self,
            _hashes: Vec<Vec<u8>>,
            _version: u32,
        ) -> Result<GetUtxosMinedInfoResponse, Error> {
            Err(anyhow!("unexpected get_utxos_mined_info call"))
        }

        async fn fetch_utxo(&self, _hash: Vec<u8>) -> Result<Option<TransactionOutput>, Error> {
            Err(anyhow!("unexpected fetch_utxo call"))
        }

        async fn query_deleted_utxos(
            &self,
            _hashes: Vec<Vec<u8>>,
            _must_include_header: Vec<u8>,
        ) -> Result<GetUtxosDeletedInfoResponse, Error> {
            Err(anyhow!("unexpected query_deleted_utxos call"))
        }

        async fn submit_transaction(&self, _transaction: Transaction) -> Result<TxSubmissionResponse, Error> {
            let mut state = self.state.lock().await;
            state.submit_calls = state.submit_calls.saturating_add(1);
            if let Some(next) = state.submit_responses.pop_front() {
                next.map_err(Error::msg)
            } else {
                Ok(state.default_submit_response.clone())
            }
        }

        async fn transaction_query(
            &self,
            _excess_sig_nonce: Vec<u8>,
            _excess_sig_sig: Vec<u8>,
        ) -> Result<TxQueryResponse, Error> {
            let mut state = self.state.lock().await;
            state.query_calls = state.query_calls.saturating_add(1);
            if let Some(next) = state.query_responses.pop_front() {
                next.map_err(Error::msg)
            } else {
                Ok(TxQueryResponse {
                    location: state.default_query_response.location.clone(),
                    mined_height: state.default_query_response.mined_height,
                    mined_header_hash: state.default_query_response.mined_header_hash.clone(),
                    mined_timestamp: state.default_query_response.mined_timestamp,
                })
            }
        }

        async fn get_mempool_fee_per_gram_stats(&self, _count: u64) -> Result<FeePerGramStat, Error> {
            Err(anyhow!("unexpected get_mempool_fee_per_gram_stats call"))
        }

        async fn get_tip_info(&self) -> Result<TipInfoResponse, Error> {
            let mut state = self.state.lock().await;
            state.tip_info_calls = state.tip_info_calls.saturating_add(1);
            if let Some(next) = state.tip_info_responses.pop_front() {
                next.map_err(Error::msg)
            } else {
                Ok(state.default_tip_info.clone())
            }
        }

        async fn get_header_by_height(&self, _height: u64) -> Result<Option<BlockHeader>, Error> {
            Err(anyhow!("unexpected get_header_by_height call"))
        }

        async fn get_height_at_time(&self, _epoch_time: u64) -> Result<u64, Error> {
            Err(anyhow!("unexpected get_height_at_time call"))
        }

        async fn get_utxos_by_block(&self, _header_hash: Vec<u8>) -> Result<models::GetUtxosByBlockResponse, Error> {
            Err(anyhow!("unexpected get_utxos_by_block call"))
        }

        async fn sync_utxos_by_block(
            &self,
            _start_header_hash: Vec<u8>,
            _shutdown: ShutdownSignal,
        ) -> Result<mpsc::Receiver<Result<SyncUtxosByBlockResponseV0, Error>>, Error> {
            Err(anyhow!("unexpected sync_utxos_by_block call"))
        }

        async fn get_burn_output_proof(
            &self,
            _commitment: &CompressedCommitment,
        ) -> Result<GenerateBurnOutputProofResponse, Error> {
            Err(anyhow!("unexpected get_burn_output_proof call"))
        }
    }

    #[derive(Clone)]
    struct MockConnectivity {
        client: MockBaseNodeClient,
        status_rx: watch::Receiver<OnlineStatus>,
    }

    impl MockConnectivity {
        fn new(client: MockBaseNodeClient) -> Self {
            let (_tx, status_rx) = watch::channel(OnlineStatus::Online {
                latency_ms: 1,
                url: "http://127.0.0.1:18142".to_string(),
            });
            Self { client, status_rx }
        }
    }

    #[async_trait]
    impl WalletConnectivityInterface for MockConnectivity {
        type BaseNodeClient = MockBaseNodeClient;

        async fn obtain_base_node_wallet_rpc_client(&self) -> Self::BaseNodeClient {
            self.client.clone()
        }

        async fn get_connectivity_status(&self) -> OnlineStatus {
            OnlineStatus::Online {
                latency_ms: 1,
                url: "http://127.0.0.1:18142".to_string(),
            }
        }

        fn get_connectivity_status_watch(&self) -> watch::Receiver<OnlineStatus> {
            self.status_rx.clone()
        }

        async fn get_last_request_latency(&self) -> Option<Duration> {
            Some(Duration::from_millis(1))
        }

        async fn get_address(&self) -> String {
            "http://127.0.0.1:18142".to_string()
        }
    }

    struct BroadcastTestHarness {
        resources: TransactionServiceResources<TransactionServiceSqliteDatabase, MockConnectivity, MemoryKeyManager>,
        client: MockBaseNodeClient,
        event_rx: TransactionEventReceiver,
        cancelled_oms_txs: Arc<Mutex<Vec<TxId>>>,
        timeout_rx: watch::Receiver<Duration>,
        _timeout_tx: watch::Sender<Duration>,
        _shutdown: Shutdown,
    }

    async fn setup_harness(resubmission_window: Duration) -> BroadcastTestHarness {
        let connection = run_migration_and_create_sqlite_memory_connection().unwrap();
        let mut key = [0u8; size_of::<Key>()];
        rand::rng().fill_bytes(&mut key);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
        let db = TransactionDatabase::new(TransactionServiceSqliteDatabase::new(connection, cipher));

        let key_manager = create_new_random_key_manager().await.unwrap();
        let cancelled_oms_txs = Arc::new(Mutex::new(Vec::new()));
        let cancelled_clone = cancelled_oms_txs.clone();
        let (oms_sender, mut oms_receiver) = reply_channel::unbounded();
        tokio::spawn(async move {
            while let Some(req_ctx) = oms_receiver.next().await {
                let (req, reply_tx) = req_ctx.split();
                if let OutputManagerRequest::CancelPendingTransaction(tx_id) = req {
                    cancelled_clone.lock().await.push(tx_id);
                    let _ignore = reply_tx.send(Ok(OutputManagerResponse::TransactionCancelled));
                }
            }
        });
        let (oms_event_tx, _) = broadcast::channel(16);
        let output_manager_service = OutputManagerHandle::new(oms_sender, oms_event_tx);

        let client = MockBaseNodeClient::default();
        let connectivity = MockConnectivity::new(client.clone());
        let (event_publisher, event_rx) = broadcast::channel(64);
        let one_sided_tari_address = random_address();
        let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build();
        let factories = CryptoFactories::default();
        let config = TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_millis(1),
            transaction_mempool_resubmission_window: resubmission_window,
            ..Default::default()
        };
        let shutdown = Shutdown::new();
        let wallet_type = key_manager.get_legacy_wallet_type();
        let (scanner_event_tx, _) = broadcast::channel(16);
        let utxo_scanner_handle = UtxoScannerHandle::new(
            scanner_event_tx,
            Watch::new("unset".to_string()),
            Watch::new("unset".to_string()),
        );
        let (timeout_tx, timeout_rx) = watch::channel(Duration::from_millis(1));

        let resources = TransactionServiceResources {
            db,
            output_manager_service,
            transaction_key_manager_service: key_manager,
            connectivity,
            event_publisher,
            one_sided_tari_address,
            node_identity,
            consensus_manager,
            factories,
            config,
            shutdown_signal: shutdown.to_signal(),
            wallet_type,
            utxo_scanner_handle,
            network: Network::LocalNet,
        };

        BroadcastTestHarness {
            resources,
            client,
            event_rx,
            cancelled_oms_txs,
            timeout_rx,
            _timeout_tx: timeout_tx,
            _shutdown: shutdown,
        }
    }

    fn random_address() -> TariAddress {
        TariAddress::new_dual_address_with_default_features(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
            Network::LocalNet,
        )
        .unwrap()
    }

    fn make_completed_transaction(tx_id: TxId, kernel_count: usize) -> CompletedTransaction {
        let factories = CryptoFactories::default();
        let kernel = KernelBuilder::new()
            .with_excess(&CompressedCommitment::from_commitment(factories.commitment.zero()))
            .with_signature(CompressedSignature::default())
            .build()
            .unwrap();
        let kernels = vec![kernel; kernel_count];
        let tx = Transaction::new(
            vec![],
            vec![],
            kernels,
            PrivateKey::random(&mut rand::rng()),
            PrivateKey::random(&mut rand::rng()),
        );
        let sig = tx.first_kernel_excess_sig().cloned().unwrap_or_default();
        CompletedTransaction {
            tx_id,
            source_address: random_address(),
            destination_address: random_address(),
            amount: MicroMinotari::from(5_000),
            fee: MicroMinotari::from(20),
            transaction: tx,
            status: LegacyTransactionStatus::Completed,
            timestamp: Utc::now(),
            cancelled: None,
            direction: TransactionDirection::Outbound,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: sig,
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
            payment_id: MemoField::new_open_from_string("test", TxType::PaymentToOther).unwrap(),
            change_output_hashes: vec![],
            received_output_hashes: vec![],
            sent_output_hashes: vec![],
            lock_height: 0,
            rejection_reason: None,
            source_address_claimed: false,
        }
    }

    #[tokio::test]
    async fn transaction_query_rpc_error_is_retried_until_confirmed_in_mempool() {
        let harness = setup_harness(Duration::from_secs(60)).await;
        let tx_id = TxId::from(10u64);
        harness
            .resources
            .db
            .insert_completed_transaction(tx_id, make_completed_transaction(tx_id, 1))
            .unwrap();

        {
            let mut state = harness.client.state.lock().await;
            state
                .query_responses
                .push_back(Err("transient HTTP timeout".to_string()));
            state.query_responses.push_back(Ok(TxQueryResponse {
                location: TxLocation::InMempool,
                mined_height: None,
                mined_header_hash: None,
                mined_timestamp: None,
            }));
        }

        let protocol = TransactionBroadcastProtocol::new(tx_id, harness.resources.clone(), harness.timeout_rx.clone());
        let res = protocol.execute().await;
        assert_eq!(res.unwrap(), tx_id);

        let state = harness.client.state.lock().await;
        assert_eq!(state.submit_calls, 1);
        assert_eq!(state.query_calls, 2);
        drop(state);

        assert!(harness.cancelled_oms_txs.lock().await.is_empty());
        let stored = harness.resources.db.get_completed_transaction(tx_id).unwrap();
        assert_eq!(stored.status, LegacyTransactionStatus::Broadcast);
        assert_eq!(stored.cancelled, None);
    }

    #[tokio::test]
    async fn unsynced_base_node_on_query_does_not_spend_budget_or_cancel_broadcast_tx() {
        let harness = setup_harness(Duration::ZERO).await;
        let tx_id = TxId::from(11u64);
        harness
            .resources
            .db
            .insert_completed_transaction(tx_id, make_completed_transaction(tx_id, 1))
            .unwrap();

        {
            let mut state = harness.client.state.lock().await;
            // Spend 4 retries on MempoolPolicy so the 5th attempt is the final submission budget slot.
            for _ in 0..MAX_MEMPOOL_SUBMISSION_ATTEMPTS.saturating_sub(1) {
                state.submit_responses.push_back(Ok(TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::MempoolPolicy,
                    is_synced: true,
                    details: None,
                }));
            }
            state.submit_responses.push_back(Ok(TxSubmissionResponse {
                accepted: true,
                rejection_reason: TxSubmissionRejectionReason::None,
                is_synced: true,
                details: None,
            }));

            // Base node fails over / loses sync and reports NotStored twice before catching up.
            state.query_responses.push_back(Ok(TxQueryResponse {
                location: TxLocation::NotStored,
                mined_height: None,
                mined_header_hash: None,
                mined_timestamp: None,
            }));
            state.tip_info_responses.push_back(Err("tip info timeout".to_string()));

            state.query_responses.push_back(Ok(TxQueryResponse {
                location: TxLocation::NotStored,
                mined_height: None,
                mined_header_hash: None,
                mined_timestamp: None,
            }));
            state.tip_info_responses.push_back(Ok(TipInfoResponse {
                metadata: None,
                is_synced: false,
            }));

            state.query_responses.push_back(Ok(TxQueryResponse {
                location: TxLocation::InMempool,
                mined_height: None,
                mined_header_hash: None,
                mined_timestamp: None,
            }));
        }

        let protocol = TransactionBroadcastProtocol::new(tx_id, harness.resources.clone(), harness.timeout_rx.clone());
        let res = protocol.execute().await;
        assert_eq!(res.unwrap(), tx_id);

        let state = harness.client.state.lock().await;
        assert_eq!(state.submit_calls, MAX_MEMPOOL_SUBMISSION_ATTEMPTS);
        assert_eq!(state.query_calls, 3);
        assert_eq!(state.tip_info_calls, 2);
        drop(state);

        assert!(harness.cancelled_oms_txs.lock().await.is_empty());
        let stored = harness.resources.db.get_completed_transaction(tx_id).unwrap();
        assert_eq!(stored.status, LegacyTransactionStatus::Broadcast);
        assert_eq!(stored.cancelled, None);
    }

    #[tokio::test]
    async fn resubmission_window_protects_newly_accepted_transaction_from_transient_not_stored() {
        let harness = setup_harness(Duration::from_secs(60)).await;
        let tx_id = TxId::from(12u64);
        harness
            .resources
            .db
            .insert_completed_transaction(tx_id, make_completed_transaction(tx_id, 1))
            .unwrap();

        {
            let mut state = harness.client.state.lock().await;
            for _ in 0..MAX_MEMPOOL_SUBMISSION_ATTEMPTS.saturating_sub(1) {
                state.submit_responses.push_back(Ok(TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::MempoolPolicy,
                    is_synced: true,
                    details: None,
                }));
            }
            state.submit_responses.push_back(Ok(TxSubmissionResponse {
                accepted: true,
                rejection_reason: TxSubmissionRejectionReason::None,
                is_synced: true,
                details: None,
            }));

            // Transient ReorgPool / gossip delay returns NotStored inside the 60s resubmission window.
            for _ in 0..2 {
                state.query_responses.push_back(Ok(TxQueryResponse {
                    location: TxLocation::NotStored,
                    mined_height: None,
                    mined_header_hash: None,
                    mined_timestamp: None,
                }));
            }
            state.query_responses.push_back(Ok(TxQueryResponse {
                location: TxLocation::Mined,
                mined_height: Some(42),
                mined_header_hash: Some(vec![7u8; 32]),
                mined_timestamp: Some(1_700_000_000),
            }));
        }

        let protocol = TransactionBroadcastProtocol::new(tx_id, harness.resources.clone(), harness.timeout_rx.clone());
        let res = protocol.execute().await;
        assert_eq!(res.unwrap(), tx_id);

        let state = harness.client.state.lock().await;
        assert_eq!(state.submit_calls, MAX_MEMPOOL_SUBMISSION_ATTEMPTS);
        assert_eq!(state.query_calls, 3);
        assert_eq!(state.tip_info_calls, 0);
        drop(state);

        assert!(harness.cancelled_oms_txs.lock().await.is_empty());
        let stored = harness.resources.db.get_completed_transaction(tx_id).unwrap();
        assert_eq!(stored.status, LegacyTransactionStatus::Broadcast);
    }

    #[tokio::test]
    async fn synced_not_stored_after_resubmission_window_resubmits_until_budget_spent() {
        let mut harness = setup_harness(Duration::ZERO).await;
        let tx_id = TxId::from(13u64);
        harness
            .resources
            .db
            .insert_completed_transaction(tx_id, make_completed_transaction(tx_id, 1))
            .unwrap();

        {
            let mut state = harness.client.state.lock().await;
            state.default_query_response = TxQueryResponse {
                location: TxLocation::NotStored,
                mined_height: None,
                mined_header_hash: None,
                mined_timestamp: None,
            };
            state.default_tip_info = TipInfoResponse {
                metadata: None,
                is_synced: true,
            };
        }

        let protocol = TransactionBroadcastProtocol::new(tx_id, harness.resources.clone(), harness.timeout_rx.clone());
        let err = protocol.execute().await.unwrap_err();
        assert_eq!(err.id, tx_id);
        assert!(matches!(err.error, TransactionServiceError::MempoolRejection { .. }));

        let state = harness.client.state.lock().await;
        assert_eq!(state.submit_calls, MAX_MEMPOOL_SUBMISSION_ATTEMPTS);
        assert_eq!(state.query_calls, MAX_MEMPOOL_SUBMISSION_ATTEMPTS);
        assert_eq!(state.tip_info_calls, MAX_MEMPOOL_SUBMISSION_ATTEMPTS);
        drop(state);

        assert_eq!(*harness.cancelled_oms_txs.lock().await, vec![tx_id]);
        let cancelled_tx = harness.resources.db.get_cancelled_completed_transaction(tx_id).unwrap();
        assert_eq!(cancelled_tx.status, LegacyTransactionStatus::Rejected);
        assert_eq!(cancelled_tx.cancelled, Some(TxCancellationReason::InvalidTransaction));
        assert!(cancelled_tx.rejection_reason.is_some());

        let mut saw_cancelled_event = false;
        while let Ok(ev) = harness.event_rx.try_recv() {
            if let TransactionEvent::TransactionCancelled(id, reason, details) = (*ev).clone() &&
                id == tx_id
            {
                assert_eq!(reason, TxCancellationReason::InvalidTransaction);
                assert!(!details.is_empty());
                saw_cancelled_event = true;
            }
        }
        assert!(saw_cancelled_event);
    }

    #[tokio::test]
    async fn oversized_transaction_emits_cancelled_event_and_records_rejection_reason() {
        let mut harness = setup_harness(Duration::from_secs(60)).await;
        let tx_id = TxId::from(14u64);
        // 40_000 kernels (~5 MiB serialized) exceeds MAX_BROADCAST_TRANSACTION_SIZE (~4 MiB).
        harness
            .resources
            .db
            .insert_completed_transaction(tx_id, make_completed_transaction(tx_id, 40_000))
            .unwrap();

        let protocol = TransactionBroadcastProtocol::new(tx_id, harness.resources.clone(), harness.timeout_rx.clone());
        let err = protocol.execute().await.unwrap_err();
        assert_eq!(err.id, tx_id);
        assert!(matches!(err.error, TransactionServiceError::TransactionTooLarge { .. }));

        assert_eq!(*harness.cancelled_oms_txs.lock().await, vec![tx_id]);
        let cancelled_tx = harness.resources.db.get_cancelled_completed_transaction(tx_id).unwrap();
        assert_eq!(cancelled_tx.status, LegacyTransactionStatus::Rejected);
        assert_eq!(cancelled_tx.cancelled, Some(TxCancellationReason::Oversized));
        assert!(cancelled_tx.rejection_reason.as_deref().is_some_and(|s| !s.is_empty()));

        let ev = harness.event_rx.try_recv().unwrap();
        match (*ev).clone() {
            TransactionEvent::TransactionCancelled(id, reason, details) => {
                assert_eq!(id, tx_id);
                assert_eq!(reason, TxCancellationReason::Oversized);
                assert!(!details.is_empty());
            },
            other => panic!("expected TransactionCancelled event, got {other:?}"),
        }
    }
}
