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

use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use digest::Digest;
use futures::{StreamExt, pin_mut, stream::FuturesUnordered};
use log::*;
use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
use minotari_node_wallet_client::BaseNodeWalletClient;
use rand::rngs::OsRng;
use sha2::Sha256;
use tari_common::configuration::Network;
use tari_common_types::{
    burn_proof::PartialBurnClaimProof,
    epoch::VnEpoch,
    payment_reference::generate_payment_reference,
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::{LegacyImportStatus, LegacyTransactionStatus, TransactionDirection, TxId},
    types::{
        ComAndPubSignature,
        CommitmentFactory,
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        FixedHash,
        HashOutput,
        PrivateKey,
        UncompressedPublicKey,
    },
};
use tari_comms::{NodeIdentity, types::CommsPublicKey};
use tari_crypto::{
    keys::{PublicKey as pkt, SecretKey},
    tari_utilities::ByteArray,
};
use tari_max_size::MaxSizeString;
use tari_script::{
    CompressedCheckSigSchnorrSignature,
    ExecutionStack,
    Opcode,
    ScriptContext,
    StackItem,
    TariScript,
    push_pubkey_script,
    script,
};
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_shutdown::ShutdownSignal;
use tari_sidechain::EvictionProof;
use tari_transaction_components::{
    MicroMinotari,
    TransactionBuilder,
    TransactionBuilderError,
    consensus::ConsensusManager,
    crypto_factories::CryptoFactories,
    fee::Fee,
    helpers::borsh::SerializedSize,
    key_manager::{SerializedKeyString, TariKeyId},
    multisig::{script::get_multi_sig_script_components, session::MultisigSession, types::GetMultisigUtxoDataOutput},
    offline_signing::{
        models::{PaymentRecipient, SignedOneSidedTransactionResult},
        offline_signer::{
            prepare_deposit_multisig_transaction,
            prepare_one_sided_transaction_for_signing,
            prepare_withdraw_multisig_transaction,
            sign_locked_deposit_multisig_transaction,
            sign_locked_transaction,
            sign_locked_withdraw_multisig_transaction,
        },
    },
    transaction_components::{
        BuildInfo,
        CodeTemplateRegistration,
        EncryptedData,
        KernelFeatures,
        OutputFeatures,
        TemplateType,
        Transaction,
        TransactionError,
        TransactionOutput,
        ValidatorNodeSignature,
        WalletOutputBuilder,
        covenants::Covenant,
        memo_field::{MemoField, TxType},
        one_sided::{public_key_to_output_encryption_key, public_key_to_output_spending_key},
    },
    tx_outputs_to_tx_id,
};
use tari_transaction_key_manager::legacy_key_manager::{
    LegacyTransactionKeyManagerInterface,
    wallet_types::{FeeType, LegacyWalletType},
};
use tari_utilities::hex::Hex;
use tokio::{
    sync::{Mutex, mpsc::Sender, oneshot},
    task::JoinHandle,
};
use uuid::Uuid;

use crate::{
    OperationId,
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::{
        UtxoSelectionCriteria,
        UtxoSelectionFilter,
        error::OutputManagerError,
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::UseOutput,
        storage::{OutputStatus, database::OutputBackendQuery, models::SpendingPriority},
    },
    transaction_service::{
        config::TransactionServiceConfig,
        error::{TransactionServiceError, TransactionServiceProtocolError, TransactionStorageError},
        handle::{
            PaymentDetails,
            TransactionEvent,
            TransactionEventSender,
            TransactionServiceRequest,
            TransactionServiceResponse,
        },
        protocols::{
            check_transaction_size,
            transaction_broadcast_protocol::TransactionBroadcastProtocol,
            transaction_validation_protocol::TransactionValidationProtocol,
        },
        storage::{
            database::{DbKey, TransactionBackend, TransactionDatabase},
            models::{
                CompletedTransaction,
                TxCancellationReason,
                WalletTransaction::{Completed, PendingInbound, PendingOutbound},
            },
        },
    },
    util::watch::Watch,
    utxo_scanner_service::handle::{UtxoScannerEvent, UtxoScannerHandle},
};

const LOG_TARGET: &str = "wallet::transaction_service::service";

/// TransactionService allows for the management of multiple inbound and outbound transaction protocols
/// which are uniquely identified by a tx_id. The TransactionService generates and accepts the various protocol
/// messages and applies them to the appropriate protocol instances based on the tx_id.
/// The TransactionService allows for the sending of transactions to single receivers, when the appropriate recipient
/// response is handled the transaction is completed and moved to the completed_transaction buffer.
/// The TransactionService will accept inbound transactions and generate a reply. Received transactions will remain
/// in the pending_inbound_transactions buffer.
pub struct TransactionService<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    db: TransactionDatabase<TBackend>,
    request_stream: Option<
        reply_channel::Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    >,
    event_publisher: TransactionEventSender,
    resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
    send_transaction_cancellation_senders: HashMap<TxId, oneshot::Sender<()>>,
    #[allow(clippy::type_complexity)]
    finalized_transaction_senders: HashMap<TxId, (TariAddress, Sender<(TariAddress, TxId, Transaction)>)>,
    receiver_transaction_cancellation_senders: HashMap<TxId, oneshot::Sender<()>>,
    active_transaction_broadcast_protocols: HashSet<TxId>,
    timeout_update_watch: Watch<Duration>,
    base_node_service: BaseNodeServiceHandle,
    validation_in_progress: Arc<Mutex<()>>,
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    TransactionService<TBackend, TWalletConnectivity, TKeyManagerInterface>
where
    TBackend: TransactionBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: LegacyTransactionKeyManagerInterface,
{
    pub async fn new(
        config: TransactionServiceConfig,
        db: TransactionDatabase<TBackend>,
        request_stream: Receiver<
            TransactionServiceRequest,
            Result<TransactionServiceResponse, TransactionServiceError>,
        >,
        output_manager_service: OutputManagerHandle<TKeyManagerInterface>,
        core_key_manager_service: TKeyManagerInterface,
        connectivity: TWalletConnectivity,
        event_publisher: TransactionEventSender,
        node_identity: Arc<NodeIdentity>,
        network: Network,
        consensus_manager: ConsensusManager,
        factories: CryptoFactories,
        shutdown_signal: ShutdownSignal,
        base_node_service: BaseNodeServiceHandle,
        wallet_type: Arc<LegacyWalletType>,
        utxo_scanner_handle: UtxoScannerHandle,
    ) -> Result<Self, TransactionServiceError> {
        // Collect the resources that all protocols will need so that they can be neatly cloned as the protocols are
        // spawned.
        let view_key = core_key_manager_service.get_view_key();
        let spend_key = core_key_manager_service.get_spend_key();
        let one_sided_tari_address = TariAddress::new_dual_address(
            view_key.pub_key.clone(),
            spend_key.pub_key.clone(),
            network,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )?;
        let resources = TransactionServiceResources {
            db: db.clone(),
            output_manager_service,
            transaction_key_manager_service: core_key_manager_service,
            connectivity,
            event_publisher: event_publisher.clone(),
            one_sided_tari_address,
            node_identity: node_identity.clone(),
            factories,
            config: config.clone(),
            shutdown_signal,
            consensus_manager: consensus_manager.clone(),
            wallet_type,
            utxo_scanner_handle,
            network,
        };
        let timeout_update_watch = Watch::new(config.broadcast_monitoring_timeout);

        Ok(Self {
            db,
            request_stream: Some(request_stream),
            event_publisher,
            resources,
            send_transaction_cancellation_senders: HashMap::new(),
            finalized_transaction_senders: HashMap::new(),
            receiver_transaction_cancellation_senders: HashMap::new(),
            active_transaction_broadcast_protocols: HashSet::new(),
            timeout_update_watch,
            base_node_service,
            validation_in_progress: Arc::new(Mutex::new(())),
        })
    }

    #[allow(clippy::too_many_lines)]
    pub async fn start(mut self) -> Result<(), TransactionServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Transaction Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let mut shutdown = self.resources.shutdown_signal.clone();

        let mut send_transaction_protocol_handles: FuturesUnordered<
            JoinHandle<Result<TransactionSendResult, TransactionServiceProtocolError<TxId>>>,
        > = FuturesUnordered::new();

        let mut receive_transaction_protocol_handles: FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        > = FuturesUnordered::new();

        let mut transaction_broadcast_protocol_handles: FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        > = FuturesUnordered::new();

        let mut transaction_validation_protocol_handles: FuturesUnordered<
            JoinHandle<Result<OperationId, TransactionServiceProtocolError<OperationId>>>,
        > = FuturesUnordered::new();

        let mut base_node_service_event_stream = self.base_node_service.get_event_stream();
        let mut utxo_scanner_events = self.resources.utxo_scanner_handle.get_event_receiver();
        let mut output_manager_event_stream = self.resources.output_manager_service.get_event_stream();

        debug!(target: LOG_TARGET, "Transaction Service started");

        // On startup, check if any confirmed transactions should be locked based on the last known tip
        let last_tip = self.db.get_last_scanned_height().unwrap_or(None).unwrap_or(0);
        if let Err(e) = self.db.check_lock_height_status(last_tip) {
            warn!(target: LOG_TARGET, "Failed to check lock height status on startup: {e}");
        }

        loop {
            tokio::select! {
                event = output_manager_event_stream.recv() => {
                    match event {
                        Ok(msg) => self.handle_output_manager_service_event(msg, &mut transaction_validation_protocol_handles).await,
                        Err(e) => debug!(target: LOG_TARGET, "Lagging read on base node event broadcast channel: {e}"),
                    };
                }
               // Base Node Monitoring Service event
                event = base_node_service_event_stream.recv() => {
                    match event {
                        Ok(msg) => self.handle_base_node_service_event(msg).await,
                        Err(e) => debug!(target: LOG_TARGET, "Lagging read on base node event broadcast channel: {e}"),
                    };
                },
                event = utxo_scanner_events.recv() => {
                    match event {
                        Ok(msg) => self.handle_utxo_scanner_service_event(msg, &mut transaction_validation_protocol_handles).await,
                        Err(e) => debug!(target: LOG_TARGET, "Lagging read on utxo scanner event broadcast channel: {e}"),
                    }
                },
                //Incoming request
                Some(request_context) = request_stream.next() => {
                    let start = Instant::now();
                    let (request, reply_tx) = request_context.split();
                    let event = format!("Handling Service API Request ({request})");
                    trace!(target: LOG_TARGET, "{event}");
                    let _result = self.handle_request(request,
                        &mut transaction_broadcast_protocol_handles,
                        &mut transaction_validation_protocol_handles,
                        reply_tx,
                    ).await.inspect_err(|e| {
                        warn!(target: LOG_TARGET, "Error handling request: {e:?}");
                    });
                    trace!(target: LOG_TARGET,
                        "{}, processed in {}ms",
                        event,
                        start.elapsed().as_millis()
                    );
                },
                Some(join_result) = send_transaction_protocol_handles.next() => {
                    trace!(target: LOG_TARGET, "Send Protocol for Transaction has ended with result {join_result:?}");
                    match join_result {
                        Ok(join_result_inner) => self.complete_send_transaction_protocol(
                            join_result_inner,
                            &mut transaction_broadcast_protocol_handles
                        ),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Send Transaction Protocol: {e:?}"),
                    };
                }
                Some(join_result) = receive_transaction_protocol_handles.next() => {
                    trace!(target: LOG_TARGET, "Receive Transaction Protocol has ended with result {join_result:?}");
                    match join_result {
                        Ok(join_result_inner) => self.complete_receive_transaction_protocol(
                            join_result_inner,
                            &mut transaction_broadcast_protocol_handles
                        ),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Send Transaction Protocol: {e:?}"),
                    };
                }
                Some(join_result) = transaction_broadcast_protocol_handles.next() => {
                    trace!(target: LOG_TARGET, "Transaction Broadcast protocol has ended with result {join_result:?}");
                    match join_result {
                        Ok(join_result_inner) => self.complete_transaction_broadcast_protocol(join_result_inner),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Broadcast Protocol: {e:?}"),
                    };
                }
                Some(join_result) = transaction_validation_protocol_handles.next() => {
                    trace!(target: LOG_TARGET, "Transaction Validation protocol has ended with result {join_result:?}");
                    match join_result {
                        Ok(join_result_inner) => self.complete_transaction_validation_protocol(
                            join_result_inner,
                            &mut transaction_broadcast_protocol_handles,
                        ),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Transaction Validation protocol: {e:?}"),
                    };
                }
                 _ = shutdown.wait() => {
                    info!(target: LOG_TARGET, "Transaction service shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
        info!(target: LOG_TARGET, "Transaction service shut down");
        Ok(())
    }

    /// This handler is called when requests arrive from the various streams
    #[allow(clippy::too_many_lines)]
    async fn handle_request(
        &mut self,
        request: TransactionServiceRequest,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        transaction_validation_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<OperationId, TransactionServiceProtocolError<OperationId>>>,
        >,
        reply_channel: oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
    ) -> Result<(), TransactionServiceError> {
        let mut reply_channel = Some(reply_channel);

        trace!(target: LOG_TARGET, "Handling Service Request: {request}");
        let response: Result<TransactionServiceResponse, TransactionServiceError> = match request {
            TransactionServiceRequest::ProcessReorg { height } => {
                self.resources.db.process_reorg(height)?;
                Ok(TransactionServiceResponse::ReorgProcessed)
            },
            TransactionServiceRequest::PrepareOneSidedTransactionForSigning {
                destination,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                mut payment_id,
            } => {
                async {
                    if selection_criteria.range_limit.is_some() {
                        return Err(TransactionServiceError::RangeLimitError {
                            reason: "Range limit coin-join cannot be set for ons-sided signing transactions"
                                .to_string(),
                        });
                    }
                    self.verify_send(&destination, TariAddressFeatures::create_one_sided_only())?;
                    debug!(target: LOG_TARGET, "Locking one sided transaction to {destination} with {amount}");
                    let temp_tx_id = TxId::new_random();

                    // let override the payment_id if the address says we should
                    if destination.features().contains(TariAddressFeatures::PAYMENT_ID) {
                        debug!(
                            target: LOG_TARGET,
                            "Address contains memo, overriding memo {} with {:?}",
                            payment_id, destination.get_memo_field_payment_id_bytes()
                        );
                        payment_id =
                            MemoField::new_open(destination.get_memo_field_payment_id_bytes(), TxType::PaymentToOther)
                                .map_err(OutputManagerError::InvalidPaymentIdFormat)?;
                    }

                    // Prepare sender part of the transaction
                    let script = push_pubkey_script(&Default::default());
                    let covenant = Covenant::default();
                    let mut payment_id = payment_id
                        .clone()
                        .add_sender_address(
                            self.resources.one_sided_tari_address.clone(),
                            true,
                            0.into(),
                            if destination == self.resources.one_sided_tari_address {
                                Some(TxType::PaymentToSelf)
                            } else {
                                Some(TxType::PaymentToOther)
                            },
                        )
                        .unwrap_or(payment_id);
                    let tx_builder = self
                        .resources
                        .output_manager_service
                        .prepare_transaction_to_send(
                            temp_tx_id,
                            amount,
                            selection_criteria,
                            *output_features.clone(),
                            fee_per_gram,
                            script,
                            covenant,
                            payment_id.clone(),
                        )
                        .await?;
                    let fee = tx_builder.get_fee_estimate_without_change()?;
                    payment_id.set_fee(fee);

                    let recipients = [PaymentRecipient {
                        amount,
                        output_features: (*output_features).clone(),
                        address: destination.clone(),
                        payment_id: payment_id.clone(),
                    }];

                    let res = prepare_one_sided_transaction_for_signing(
                        temp_tx_id,
                        tx_builder,
                        &recipients,
                        payment_id,
                        self.resources.one_sided_tari_address.clone(),
                    )?;

                    self.resources
                        .output_manager_service
                        .confirm_pending_transaction(temp_tx_id, Some(res.tx_id), None)
                        .await
                        .map_err(|e| TransactionServiceProtocolError::new(res.tx_id, e.into()))?;
                    Ok(TransactionServiceResponse::OneSidedTransactionPreparedForSigning(
                        Box::new(res),
                    ))
                }
                .await
            },

            TransactionServiceRequest::PrepareDepositMultisigTransaction { request } => {
                async {
                    self.verify_send(&request.recipient_address, TariAddressFeatures::create_one_sided_only())?;

                    let temp_tx_id = TxId::new_random();
                    let script = push_pubkey_script(&Default::default());
                    let uuid = Uuid::new_v4();
                    let user_data = uuid.as_bytes().to_vec();
                    let fee_per_gram = MicroMinotari::from(1);
                    let output_features = OutputFeatures::default();
                    let covenant = Covenant::default();
                    let temp_payment_id = MemoField::new_address_and_data(
                        request.recipient_address.clone(),
                        0.into(),
                        true,
                        TxType::PaymentToOther,
                        user_data.clone(),
                    )
                    .map_err(|e| TransactionServiceError::Other(format!("Failed to create MemoField: {}", e)))?;
                    let tx_builder = self
                        .resources
                        .output_manager_service
                        .prepare_transaction_to_send(
                            temp_tx_id,
                            request.amount,
                            UtxoSelectionCriteria::default(),
                            output_features.clone(),
                            fee_per_gram,
                            script,
                            covenant,
                            temp_payment_id,
                        )
                        .await?;
                    let fee = tx_builder.get_fee_estimate_without_change()?;

                    let payment_id = MemoField::new_address_and_data(
                        request.recipient_address.clone(),
                        fee,
                        true,
                        TxType::PaymentToOther,
                        user_data,
                    )
                    .map_err(|e| TransactionServiceError::Other(format!("Failed to create MemoField: {}", e)))?;

                    let response = prepare_deposit_multisig_transaction(
                        temp_tx_id,
                        tx_builder,
                        request.amount,
                        payment_id,
                        output_features,
                        request.party_number,
                        request.public_keys,
                        self.resources.one_sided_tari_address.clone(),
                        request.recipient_address,
                    )?;

                    self.resources
                        .output_manager_service
                        .confirm_pending_transaction(temp_tx_id, Some(response.tx_id), None)
                        .await
                        .map_err(|e| TransactionServiceProtocolError::new(response.tx_id, e.into()))?;

                    Ok(TransactionServiceResponse::PrepareDepositMultisigTransaction(Box::new(
                        response,
                    )))
                }
                .await
            },

            TransactionServiceRequest::PrepareWithdrawMultisigTransaction { request } => {
                async {
                    self.verify_send(&request.recipient_address, TariAddressFeatures::create_one_sided_only())?;

                    let mut query = OutputBackendQuery::default();
                    query.commitments.push(request.utxo_commitment.clone());

                    query.status.push(OutputStatus::Unspent);

                    let utxos = self
                        .resources
                        .output_manager_service
                        .clone()
                        .get_outputs_by_query(query)
                        .await
                        .map_err(TransactionServiceError::OutputManagerError)?;

                    let selected_utxo = utxos.first().ok_or(TransactionServiceError::Other(format!(
                        "UTXO with commitment {:?} not found",
                        request.utxo_commitment
                    )))?;

                    let signatures = request.signatures;

                    // Enforce correct signature count and ordering for the multisig script
                    let (_ephemeral_pubkeys, threshold) =
                        get_multi_sig_script_components(selected_utxo.wallet_output.script())
                            .ok_or(TransactionError::BuilderError("no keys found".to_string()))?;

                    if signatures.len() < threshold as usize {
                        return Err(TransactionServiceError::Other(format!(
                            "Insufficient signatures: need at least {}, got {}",
                            threshold,
                            signatures.len()
                        )));
                    }

                    let mut input_stack = ExecutionStack::default();
                    for sig in signatures.clone() {
                        input_stack
                            .push(StackItem::Signature(sig))
                            .map_err(|e| TransactionServiceError::Other(format!("Failed to push signature: {}", e)))?;
                    }

                    let mut input_wallet_output = selected_utxo.wallet_output.clone();
                    input_wallet_output.set_input_data(input_stack);

                    let amount = selected_utxo.wallet_output.value();

                    let fee_per_gram = MicroMinotari::from(1);
                    let height = self.resources.db.get_last_scanned_height()?.unwrap_or(0);
                    let consensus_constants = self.resources.consensus_manager.consensus_constants(height);
                    let mut tx_builder = TransactionBuilder::new(
                        consensus_constants.clone(),
                        self.resources.transaction_key_manager_service.clone(),
                        self.resources.network,
                    )?;

                    let fee_calculator = Fee::new(*consensus_constants.transaction_weight_params());
                    let script = push_pubkey_script(&Default::default());

                    let output_features = OutputFeatures::default();
                    let features_and_scripts_byte_size = consensus_constants
                        .transaction_weight_params()
                        .round_up_features_and_scripts_size(
                            output_features
                                .get_serialized_size()
                                .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                                script
                                    .get_serialized_size()
                                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                                Covenant::default()
                                    .get_serialized_size()
                                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
                        );

                    let fee: MicroMinotari =
                        fee_calculator.calculate(fee_per_gram, 1, 1, 1, features_and_scripts_byte_size);

                    if fee > amount {
                        return Err(TransactionServiceError::Other(format!(
                            "insufficient funds: fee: {}, amount: {}",
                            fee, amount
                        )));
                    }
                    let total_amount = amount
                        .checked_sub(fee)
                        .ok_or(TransactionServiceError::Other("Amount too small to cover fee".into()))?;

                    tx_builder.with_input(input_wallet_output)?;
                    tx_builder.with_fee_per_gram(fee_per_gram);
                    tx_builder.with_lock_height(0);

                    let payment_id = MemoField::new_address_and_data(
                        request.recipient_address.clone(),
                        fee,
                        true,
                        TxType::PaymentToOther,
                        vec![],
                    )
                    .map_err(|e| TransactionServiceError::Other(format!("Failed to create MemoField: {}", e)))?;
                    let tx_id = TxId::new_random();
                    let response = prepare_withdraw_multisig_transaction(
                        tx_id,
                        tx_builder,
                        total_amount,
                        payment_id,
                        output_features,
                        self.resources.one_sided_tari_address.clone(),
                        request.recipient_address,
                    )?;

                    self.resources
                        .output_manager_service
                        .confirm_pending_transaction(response.tx_id, None, None)
                        .await
                        .map_err(|e| TransactionServiceProtocolError::new(response.tx_id, e.into()))?;

                    Ok(TransactionServiceResponse::PrepareWithdrawMultisigTransaction(
                        Box::new(response),
                    ))
                }
                .await
            },

            TransactionServiceRequest::SignOneSidedTransaction { request } => {
                async {
                    let tip_height = self.resources.db.get_last_scanned_height()?.unwrap_or(0);
                    let res = sign_locked_transaction(
                        self.resources.transaction_key_manager_service.key_manager(),
                        self.resources.consensus_manager.consensus_constants(tip_height).clone(),
                        self.resources.network,
                        request,
                    )?;
                    Ok(TransactionServiceResponse::SignedOneSidedTransaction(Box::new(res)))
                }
                .await
            },

            TransactionServiceRequest::SignOneSidedDepositMultisigTransaction { request } => {
                async {
                    let tip_height = self.resources.db.get_last_scanned_height()?.unwrap_or(0);
                    let res = sign_locked_deposit_multisig_transaction(
                        self.resources.transaction_key_manager_service.key_manager(),
                        self.resources.consensus_manager.consensus_constants(tip_height).clone(),
                        self.resources.network,
                        request,
                    )?;
                    Ok(TransactionServiceResponse::SignedOneSidedDepositMultisigTransaction(
                        Box::new(res),
                    ))
                }
                .await
            },

            TransactionServiceRequest::SignOneSidedWithdrawMultisigTransaction { request } => {
                async {
                    let key_manager = self.resources.transaction_key_manager_service.clone();
                    let mut request = request;

                    for pair_output in &mut request.info.inputs.iter_mut() {
                        let view_key = key_manager.get_view_key();
                        let spend_key = key_manager.get_spend_key();

                        let commitment_mask_key_id = TariKeyId::DHCommitmentMask {
                            private_key: view_key.key_id.clone().into(),
                            public_key: pair_output.sender_offset_public_key().clone(),
                        };
                        let script_pubkey = key_manager
                            .stealth_address_script_spending_key(&commitment_mask_key_id, &spend_key.pub_key)?;
                        let script_key = TariKeyId::Derived {
                            key: SerializedKeyString::from(commitment_mask_key_id.to_string()),
                        };

                        let pushed_pk = pair_output
                            .script()
                            .as_slice()
                            .iter()
                            .find_map(|op| {
                                if let Opcode::PushPubKey(pk) = op {
                                    Some(pk.as_ref())
                                } else {
                                    None
                                }
                            })
                            .ok_or_else(|| TransactionServiceError::Other("Script has no PushPubKey opcode".into()))?;

                        if pushed_pk != &script_pubkey {
                            return Err(TransactionServiceError::Other(format!(
                                "Script-spend key mismatch: script[1]={} derived(k')={}",
                                pushed_pk.to_hex(),
                                script_pubkey.to_hex()
                            )));
                        }

                        if *pair_output.commitment_mask_key_id() == TariKeyId::Zero {
                            return Err(TransactionServiceError::ServiceError(
                                "Input commitment mask key id is zero".into(),
                            ));
                        }

                        // 5) Attach k' so signer uses the correct key
                        pair_output.set_script_key_id(script_key);
                    }

                    let tip_height = self.resources.db.get_last_scanned_height()?.unwrap_or(0);
                    let res = sign_locked_withdraw_multisig_transaction(
                        self.resources.transaction_key_manager_service.key_manager(),
                        self.resources.consensus_manager.consensus_constants(tip_height).clone(),
                        self.resources.network,
                        request,
                    )?;

                    Ok(TransactionServiceResponse::SignedOneSidedWithdrawMultisigTransaction(
                        Box::new(res),
                    ))
                }
                .await
            },

            TransactionServiceRequest::BroadcastSignedOneSidedTransaction { request } => {
                async {
                    let res = self
                        .submit_signed_one_sided_transaction(request, transaction_broadcast_join_handles)
                        .await?;
                    Ok(TransactionServiceResponse::TransactionsSent(res))
                }
                .await
            },

            TransactionServiceRequest::SendOneSidedTransaction {
                destination,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                payment_id,
            } => {
                async {
                    let res = self
                        .send_one_sided_transaction(
                            destination,
                            amount,
                            selection_criteria,
                            *output_features,
                            fee_per_gram,
                            payment_id,
                            transaction_broadcast_join_handles,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::TransactionSent(res))
                }
                .await
            },

            TransactionServiceRequest::SendManyOneSidedTransactions {
                destinations,
                selection_criteria,
                output_features,
                fee_per_gram,
            } => {
                async {
                    let res = self
                        .send_many_one_sided_transactions(
                            destinations,
                            selection_criteria,
                            *output_features,
                            fee_per_gram,
                            transaction_broadcast_join_handles,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::TransactionsSent(res))
                }
                .await
            },

            TransactionServiceRequest::ScrapeWallet {
                destination,
                fee_per_gram,
            } => {
                async {
                    let res = self
                        .scrape_wallet(destination, fee_per_gram, transaction_broadcast_join_handles)
                        .await?;
                    Ok(TransactionServiceResponse::TransactionSent(res))
                }
                .await
            },

            TransactionServiceRequest::SendRangeLimitedCoinJoinTransaction {
                selection_criteria,
                output_features,
                fee,
                payment_id,
            } => {
                async {
                    let res = self
                        .send_range_limited_coin_join(
                            selection_criteria,
                            *output_features,
                            fee,
                            transaction_broadcast_join_handles,
                            payment_id,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::TransactionSent(res))
                }
                .await
            },

            TransactionServiceRequest::SendOneSidedToStealthAddressTransaction {
                destination,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                payment_id,
            } => {
                async {
                    let res = self
                        .send_one_sided_to_stealth_address_transaction(
                            destination,
                            amount,
                            selection_criteria,
                            *output_features,
                            fee_per_gram,
                            payment_id,
                            transaction_broadcast_join_handles,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::TransactionSent(res))
                }
                .await
            },

            TransactionServiceRequest::BurnTari {
                amount,
                selection_criteria,
                fee_per_gram,
                payment_id,
                claim_public_key,
                sidechain_deployment_key,
            } => {
                async {
                    let (tx_id, proof) = self
                        .burn_tari(
                            amount,
                            selection_criteria,
                            fee_per_gram,
                            payment_id,
                            claim_public_key,
                            sidechain_deployment_key,
                            transaction_broadcast_join_handles,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::BurntTransactionSent {
                        tx_id,
                        proof: proof.map(Box::new),
                    })
                }
                .await
            },

            TransactionServiceRequest::EncumberAggregateUtxo {
                fee_per_gram,
                expected_commitment,
                script_input_shares,
                script_signature_public_nonces,
                sender_offset_public_key_shares,
                metadata_ephemeral_public_key_shares,
                dh_shared_secret_shares,
                recipient_address,
                original_maturity,
                use_output,
                payment_id,
            } => {
                async {
                    let (
                        tx_id,
                        tx,
                        total_script_pubkey,
                        total_metadata_ephemeral_public_key,
                        total_script_nonce,
                        shared_secret,
                    ) = self
                        .encumber_aggregate_tx(
                            fee_per_gram,
                            expected_commitment,
                            script_input_shares,
                            script_signature_public_nonces,
                            sender_offset_public_key_shares,
                            metadata_ephemeral_public_key_shares,
                            dh_shared_secret_shares,
                            recipient_address,
                            original_maturity,
                            use_output,
                            payment_id,
                        )
                        .await?;
                    Ok({
                        TransactionServiceResponse::EncumberAggregateUtxo(
                            tx_id,
                            Box::new(tx),
                            Box::new(total_script_pubkey),
                            Box::new(total_metadata_ephemeral_public_key),
                            Box::new(total_script_nonce),
                            Box::new(shared_secret),
                        )
                    })
                }
                .await
            },

            TransactionServiceRequest::SpendBackupPreMineUtxo {
                fee_per_gram,
                output_hash,
                expected_commitment,
                recipient_address,
                payment_id,
            } => {
                async {
                    let res = self
                        .spend_backup_pre_mine_utxo(
                            fee_per_gram,
                            output_hash,
                            expected_commitment,
                            recipient_address,
                            payment_id,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::TransactionSent(res))
                }
                .await
            },

            TransactionServiceRequest::FetchUnspentOutputs { output_hashes } => {
                let reply_channel = reply_channel.take().expect("reply_channel is Some");
                self.handle_fetch_unspent_outputs_request(output_hashes, reply_channel);
                return Ok(());
            },

            TransactionServiceRequest::FinalizeSentAggregateTransaction {
                tx_id,
                total_meta_data_signature,
                total_script_data_signature,
                script_offset,
            } => {
                async {
                    Ok(TransactionServiceResponse::TransactionSent(
                        self.finalized_aggregate_encumbed_tx(
                            tx_id.into(),
                            total_meta_data_signature,
                            total_script_data_signature,
                            script_offset,
                            transaction_broadcast_join_handles,
                        )
                        .await?,
                    ))
                }
                .await
            },

            TransactionServiceRequest::RegisterValidatorNode {
                amount,
                validator_node_public_key,
                validator_node_signature,
                validator_node_claim_public_key,
                sidechain_deployment_key,
                max_epoch,
                selection_criteria,
                fee_per_gram,
                payment_id,
            } => {
                async {
                    let tx_id = self
                        .register_validator_node(
                            amount,
                            validator_node_public_key,
                            validator_node_signature,
                            validator_node_claim_public_key,
                            sidechain_deployment_key,
                            max_epoch,
                            selection_criteria,
                            fee_per_gram,
                            payment_id,
                            transaction_broadcast_join_handles,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::TransactionSent(tx_id))
                }
                .await
            },

            TransactionServiceRequest::SubmitValidatorNodeExit {
                amount,
                validator_node_public_key,
                validator_node_signature,
                sidechain_deployment_key,
                max_epoch,
                selection_criteria,
                fee_per_gram,
                payment_id,
            } => {
                async {
                    let tx_id = self
                        .submit_validator_exit(
                            amount,
                            validator_node_public_key,
                            validator_node_signature,
                            sidechain_deployment_key,
                            selection_criteria,
                            max_epoch,
                            fee_per_gram,
                            payment_id,
                            transaction_broadcast_join_handles,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::TransactionSent(tx_id))
                }
                .await
            },

            TransactionServiceRequest::RegisterCodeTemplate {
                template_name,
                template_version,
                template_type,
                build_info,
                binary_sha,
                binary_url,
                fee_per_gram,
                sidechain_deployment_key,
            } => {
                async {
                    let payment_id = MemoField::new_open(
                        format!("Template Registration: {template_name}").into_bytes(),
                        TxType::CodeTemplateRegistration,
                    )
                    .map_err(|e| TransactionServiceError::InvalidPaymentId(e.to_string()))?;
                    let (tx_id, template_address) = self
                        .register_code_template(
                            fee_per_gram,
                            template_name,
                            template_version,
                            template_type,
                            build_info,
                            binary_sha,
                            binary_url,
                            sidechain_deployment_key,
                            UtxoSelectionCriteria::default(),
                            payment_id,
                            transaction_broadcast_join_handles,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::CodeRegistrationTransactionSent {
                        tx_id,
                        template_address,
                    })
                }
                .await
            },

            TransactionServiceRequest::SubmitValidatorEvictionProof {
                amount,
                proof,
                fee_per_gram,
                payment_id,
                sidechain_deployment_key,
            } => {
                async {
                    let tx_id = self
                        .submit_validator_eviction_proof(
                            amount,
                            proof,
                            sidechain_deployment_key,
                            UtxoSelectionCriteria::default(),
                            fee_per_gram,
                            payment_id,
                            transaction_broadcast_join_handles,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::TransactionSent(tx_id))
                }
                .await
            },

            TransactionServiceRequest::SendShaAtomicSwapTransaction(
                destination,
                amount,
                selection_criteria,
                fee_per_gram,
                payment_id,
            ) => {
                async {
                    let res = self
                        .send_sha_atomic_swap_transaction(
                            destination,
                            amount,
                            selection_criteria,
                            fee_per_gram,
                            payment_id,
                            transaction_broadcast_join_handles,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::ShaAtomicSwapTransactionSent(res))
                }
                .await
            },

            TransactionServiceRequest::CancelPendingTransaction(tx_id) => {
                async {
                    self.cancel_pending_transaction(tx_id).await?;
                    Ok(TransactionServiceResponse::TransactionCancelled)
                }
                .await
            },

            TransactionServiceRequest::CancelCompletedTransaction(tx_id) => {
                async {
                    self.cancel_completed_transaction(tx_id).await?;
                    Ok(TransactionServiceResponse::TransactionCancelled)
                }
                .await
            },

            TransactionServiceRequest::GetPendingInboundTransactions => {
                async {
                    Ok(TransactionServiceResponse::PendingInboundTransactions(
                        self.db.get_pending_inbound_transactions()?,
                    ))
                }
                .await
            },

            TransactionServiceRequest::GetPendingOutboundTransactions => {
                async {
                    Ok(TransactionServiceResponse::PendingOutboundTransactions(
                        self.db.get_pending_outbound_transactions()?,
                    ))
                }
                .await
            },

            TransactionServiceRequest::GetCompletedTransactions {
                payment_id,
                block_hash,
                block_height,
                max_limit,
            } => {
                async {
                    Ok(TransactionServiceResponse::CompletedTransactions(
                        self.db
                            .get_completed_transactions(payment_id, block_hash, block_height, max_limit)?,
                    ))
                }
                .await
            },

            TransactionServiceRequest::GetCompletedTransactionsByAddresses {
                source_address,
                destination_address,
            } => {
                async {
                    Ok(TransactionServiceResponse::CompletedTransactions(
                        self.db
                            .get_completed_transactions_by_addresses(source_address, destination_address)?,
                    ))
                }
                .await
            },

            TransactionServiceRequest::GetCompletedTransactionsPaginated {
                offset,
                limit,
                status_filter,
            } => {
                async {
                    if limit == 0 {
                        return Err(TransactionServiceError::InvalidArgument(
                            "limit must be greater than 0".to_string(),
                        ));
                    }
                    Ok(TransactionServiceResponse::CompletedTransactions(
                        self.db
                            .get_completed_transactions_paginated(offset, limit, status_filter)?,
                    ))
                }
                .await
            },

            TransactionServiceRequest::GetCancelledPendingInboundTransactions => {
                async {
                    Ok(TransactionServiceResponse::PendingInboundTransactions(
                        self.db.get_cancelled_pending_inbound_transactions()?,
                    ))
                }
                .await
            },

            TransactionServiceRequest::GetCancelledPendingOutboundTransactions => {
                async {
                    Ok(TransactionServiceResponse::PendingOutboundTransactions(
                        self.db.get_cancelled_pending_outbound_transactions()?,
                    ))
                }
                .await
            },

            TransactionServiceRequest::GetCancelledCompletedTransactions(max_limit) => {
                async {
                    Ok(TransactionServiceResponse::CompletedTransactions(
                        self.db.get_cancelled_completed_transactions(max_limit)?,
                    ))
                }
                .await
            },

            TransactionServiceRequest::GetCompletedTransaction(tx_id) => {
                async {
                    Ok(TransactionServiceResponse::CompletedTransaction(Box::new(
                        self.db.get_completed_transaction(tx_id)?,
                    )))
                }
                .await
            },

            TransactionServiceRequest::GetAnyTransaction(tx_id) => {
                async {
                    Ok(TransactionServiceResponse::AnyTransaction(Box::new(
                        self.db.get_any_transaction(tx_id)?,
                    )))
                }
                .await
            },

            TransactionServiceRequest::ImportTransaction(tx) => {
                async {
                    let tx_id = match tx {
                        PendingInbound(inbound_tx) => {
                            let tx_id = inbound_tx.tx_id;
                            check_transaction_size(&inbound_tx, tx_id)?;
                            self.db.insert_pending_inbound_transaction(tx_id, inbound_tx)?;
                            tx_id
                        },
                        PendingOutbound(outbound_tx) => {
                            let tx_id = outbound_tx.tx_id;
                            check_transaction_size(&outbound_tx, tx_id)?;
                            self.db.insert_pending_outbound_transaction(tx_id, outbound_tx)?;
                            tx_id
                        },
                        Completed(completed_tx) => {
                            let tx_id = completed_tx.tx_id;
                            check_transaction_size(&completed_tx.transaction, tx_id)?;
                            self.db.insert_completed_transaction(tx_id, completed_tx)?;
                            tx_id
                        },
                    };
                    let _size = self
                        .event_publisher
                        .send(Arc::new(TransactionEvent::TransactionImported(tx_id)));
                    Ok(TransactionServiceResponse::TransactionImported(tx_id))
                }
                .await
            },

            TransactionServiceRequest::ImportUtxoWithStatus {
                amount,
                source_address,
                import_status,
                current_height,
                mined_timestamp,
                scanned_output,
                payment_id,
                optional_tx_id,
                lock_height,
            } => {
                async {
                    let res = self
                        .add_utxo_import_transaction_with_status(
                            amount,
                            source_address,
                            import_status,
                            current_height,
                            mined_timestamp,
                            scanned_output,
                            payment_id,
                            optional_tx_id,
                            lock_height,
                        )
                        .await?;
                    Ok(TransactionServiceResponse::UtxoImported(res))
                }
                .await
            },

            TransactionServiceRequest::SubmitTransactionToSelf(tx_id, tx, fee, amount, payment_id) => {
                async {
                    self.submit_transaction_to_self(
                        transaction_broadcast_join_handles,
                        tx_id,
                        tx,
                        fee,
                        amount,
                        payment_id,
                    )
                    .await?;
                    Ok(TransactionServiceResponse::TransactionSubmitted)
                }
                .await
            },

            TransactionServiceRequest::RestartBroadcastProtocols => {
                async {
                    self.restart_broadcast_protocols(transaction_broadcast_join_handles)?;
                    Ok(TransactionServiceResponse::ProtocolsRestarted)
                }
                .await
            },

            TransactionServiceRequest::GetNumConfirmationsRequired => Ok(
                TransactionServiceResponse::NumConfirmationsRequired(self.resources.config.num_confirmations_required),
            ),

            TransactionServiceRequest::SetNumConfirmationsRequired(number) => {
                self.resources.config.num_confirmations_required = number;
                Ok(TransactionServiceResponse::NumConfirmationsSet)
            },

            TransactionServiceRequest::ValidateTransactions => {
                async {
                    let res = self
                        .start_transaction_validation_protocol(transaction_validation_join_handles)
                        .await?;
                    Ok(TransactionServiceResponse::ValidationStarted(res))
                }
                .await
            },
            TransactionServiceRequest::ReValidateRejectedTransactions => {
                async {
                    let res = self
                        .start_rejected_transaction_revalidation(transaction_validation_join_handles)
                        .await?;
                    Ok(TransactionServiceResponse::ValidationStarted(res))
                }
                .await
            },
            TransactionServiceRequest::ReValidateTransactions => {
                async {
                    let res = self
                        .start_transaction_revalidation(transaction_validation_join_handles)
                        .await?;
                    Ok(TransactionServiceResponse::ValidationStarted(res))
                }
                .await
            },
            TransactionServiceRequest::ReplaceByFee { tx_id, fee_increase } => {
                async {
                    let res = self
                        .replace_by_fee(tx_id, fee_increase, transaction_broadcast_join_handles)
                        .await?;
                    Ok(TransactionServiceResponse::TransactionReplaced(res))
                }
                .await
            },

            TransactionServiceRequest::UserPayForFee {
                tx_id,
                destination,
                fee,
            } => {
                async {
                    let tx_id = self
                        .user_pay_for_fee(tx_id, destination, fee, transaction_broadcast_join_handles)
                        .await?;
                    Ok(TransactionServiceResponse::TransactionSent(tx_id))
                }
                .await
            },

            TransactionServiceRequest::GetFeePerGramStatsPerBlock { count } => {
                let reply_channel = reply_channel.take().expect("reply_channel is Some");
                self.handle_get_fee_per_gram_stats_per_block_request(count, reply_channel);
                return Ok(());
            },

            TransactionServiceRequest::GetPaymentByReference { payref } => {
                async {
                    let res = self.get_payment_by_reference(payref)?;
                    Ok(TransactionServiceResponse::PaymentDetails(res))
                }
                .await
            },

            TransactionServiceRequest::GetTransactionByPaymentReference(payref) => {
                async {
                    match self.get_transaction_with_payref(payref)? {
                        Some(tx) => Ok(TransactionServiceResponse::CompletedTransaction(Box::new(tx))),
                        None => Err(TransactionServiceError::TransactionStorageError(
                            TransactionStorageError::ValueNotFound(DbKey::CompletedTransactions(1)),
                        ))?,
                    }
                }
                .await
            },

            TransactionServiceRequest::GetPayrefHistoryByTxId(tx_id) => {
                async {
                    let history = self.db.get_payref_history_by_tx_id(tx_id)?;
                    Ok(TransactionServiceResponse::PayrefHistory(history))
                }
                .await
            },

            TransactionServiceRequest::GetTransactionByHistoricalPayref(payref) => {
                async {
                    let txs = self.db.get_transaction_with_historical_payref(&payref)?;
                    Ok(TransactionServiceResponse::HistoricalPayrefTransactions(txs))
                }
                .await
            },

            TransactionServiceRequest::CreateMultisigUtxo { request } => {
                async {
                    let fee_per_gram = MicroMinotari::from(1);
                    let selected_criteria = UtxoSelectionCriteria {
                        excluding_multisig: true,
                        ..Default::default()
                    };
                    let temp_tx_id = TxId::new_random();
                    let uuid = Uuid::new_v4();
                    let payment_id = MemoField::new_address_and_data(
                        request.recipient_address.clone(),
                        0.into(),
                        true,
                        TxType::PaymentToOther,
                        uuid.as_bytes().to_vec(),
                    )
                    .map_err(|e| TransactionError::BuilderError(format!("Failed to create MemoField: {}", e)))?;
                    let tx_builder = self
                        .resources
                        .output_manager_service
                        .prepare_transaction_to_send(
                            temp_tx_id,
                            request.amount,
                            selected_criteria,
                            OutputFeatures::default(),
                            fee_per_gram,
                            push_pubkey_script(&Default::default()),
                            Covenant::default(),
                            payment_id,
                        )
                        .await?;
                    let mut multisig_session =
                        MultisigSession::new(self.resources.transaction_key_manager_service.clone());
                    let (tx, payment_id, sent_hashes, change_hashes, change, tx_id) = multisig_session
                        .create_deposit_multisig_transaction(
                            request.amount,
                            request.party_number,
                            request.public_keys,
                            request.recipient_address.clone(),
                            tx_builder,
                            uuid,
                        )
                        .await?;

                    let fee = tx.body.get_total_fee()?;

                    self.resources
                        .output_manager_service
                        .confirm_pending_transaction(temp_tx_id, Some(tx_id), change)
                        .await
                        .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

                    drop(
                        self.event_publisher
                            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id))),
                    );

                    self.submit_transaction(
                        transaction_broadcast_join_handles,
                        CompletedTransaction::new_with_output_hashes(
                            tx_id,
                            self.resources.one_sided_tari_address.clone(),
                            request.recipient_address.clone(),
                            request.amount,
                            fee,
                            tx.clone(),
                            LegacyTransactionStatus::Completed,
                            Utc::now(),
                            TransactionDirection::Outbound,
                            None,
                            None,
                            payment_id,
                            sent_hashes,
                            vec![],
                            change_hashes,
                            0,
                        )?,
                    )
                    .await?;

                    Ok(TransactionServiceResponse::CreateMultisigUtxo(tx_id))
                }
                .await
            },

            TransactionServiceRequest::GetMultisigUtxoData { utxo_commitment } => {
                async {
                    let mut query = OutputBackendQuery::default();
                    query.commitments.push(utxo_commitment.clone());

                    query.status.push(OutputStatus::Unspent);

                    let utxos = self
                        .resources
                        .output_manager_service
                        .clone()
                        .get_outputs_by_query(query)
                        .await
                        .map_err(TransactionServiceError::OutputManagerError)?;

                    let selected_utxo = utxos.first().ok_or(TransactionError::BuilderError(format!(
                        "UTXO with commitment {:?} not found",
                        utxo_commitment
                    )))?;

                    let scripts = selected_utxo.wallet_output.script().clone();
                    let mut challenge = Box::new([0; 32]);
                    let mut public_keys = Vec::new();

                    let sender_offset_pub_key =
                        selected_utxo.wallet_output.sender_offset_public_key().to_public_key()?;

                    for op in scripts.as_slice() {
                        if let Opcode::CheckMultiSigVerify(_m, _n, k, msg) = op {
                            challenge.clone_from_slice(msg.as_bytes());

                            public_keys.extend(k.clone());
                        }
                    }

                    let output = GetMultisigUtxoDataOutput {
                        challenge,
                        public_keys,
                        commitment: selected_utxo.commitment.clone(),
                        sender_offset_pub_key: CompressedPublicKey::new_from_pk(sender_offset_pub_key),
                    };

                    Ok(TransactionServiceResponse::GetMultisigUtxoData(Box::new(output)))
                }
                .await
            },

            TransactionServiceRequest::SendMultisigUtxo {
                utxo_commitment,
                recipient_address,
                signatures,
            } => {
                async {
                    let mut query = OutputBackendQuery::default();
                    query.commitments.push(utxo_commitment.clone());

                    query.status.push(OutputStatus::Unspent);

                    let utxos = self
                        .resources
                        .output_manager_service
                        .clone()
                        .get_outputs_by_query(query)
                        .await
                        .map_err(TransactionServiceError::OutputManagerError)?;

                    let selected_utxo = utxos.first().ok_or(TransactionError::BuilderError(format!(
                        "UTXO with utxo_commitment {:?} not found",
                        utxo_commitment
                    )))?;

                    let multisig_session = MultisigSession::new(self.resources.transaction_key_manager_service.clone());
                    let current_height = self.db.get_last_scanned_height()?.unwrap_or(0);
                    let consensus_constants = self.resources.consensus_manager.consensus_constants(current_height);
                    let (finalized_transaction, payment_id, amount) = multisig_session.spend_multisig_utxo(
                        signatures,
                        recipient_address.clone(),
                        selected_utxo.clone().into(),
                        consensus_constants,
                    )?;
                    let view_key = self.resources.transaction_key_manager_service.get_view_key().pub_key;
                    let (change_hashes, change, tx_id) = match finalized_transaction.change {
                        Some(change_output) => (
                            vec![change_output.output_hash()],
                            Some(vec![change_output.clone()]),
                            change_output.calculate_tx_id(view_key.as_bytes()),
                        ),
                        None => (
                            vec![],
                            None,
                            tx_outputs_to_tx_id(view_key.as_bytes(), finalized_transaction.transaction.body.outputs()),
                        ),
                    };
                    self.resources
                        .output_manager_service
                        .clone()
                        .confirm_pending_transaction(tx_id, None, change)
                        .await
                        .map_err(|e| {
                            TransactionError::BuilderError(format!("Failed to confirm pending transaction: {:?}", e))
                        })?;

                    let fee = finalized_transaction.transaction.body.get_total_fee()?;

                    // This event being sent is important, but not critical to the protocol being successful. Send only
                    // fails if there are no subscribers.
                    let _result = self
                        .event_publisher
                        .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));
                    let sent_hashes = finalized_transaction.sent_output_hashes.clone();
                    self.submit_transaction(
                        transaction_broadcast_join_handles,
                        CompletedTransaction::new_with_output_hashes(
                            tx_id,
                            self.resources.one_sided_tari_address.clone(),
                            recipient_address,
                            amount,
                            fee,
                            finalized_transaction.transaction,
                            LegacyTransactionStatus::Completed,
                            Utc::now(),
                            TransactionDirection::Outbound,
                            None,
                            None,
                            payment_id,
                            sent_hashes,
                            vec![],
                            change_hashes,
                            0,
                        )?,
                    )
                    .await?;
                    Ok(TransactionServiceResponse::SendMultisigUtxo(tx_id))
                }
                .await
            },
            TransactionServiceRequest::GetBurnProof { output_hash } => self
                .db
                .fetch_burn_proof(&output_hash)
                .map(|proof| TransactionServiceResponse::GetBurnProof {
                    proof: proof.map(Box::new),
                })
                .map_err(TransactionServiceError::TransactionStorageError),
        };

        // If the individual handlers did not already send the API response then do it here.
        if let Some(rp) = reply_channel {
            let _result = rp
                .send(response.inspect_err(|e1| {
                    let mut msg = format!("{}", e1);
                    msg.truncate(100);
                    warn!(target: LOG_TARGET, "{}", msg);
                }))
                .inspect_err(|e2| {
                    let mut msg = format!("{:?}", e2);
                    msg.truncate(100);
                    warn!(target: LOG_TARGET, "Failed to send reply: {}", msg);
                });
        }

        Ok(())
    }

    fn handle_get_fee_per_gram_stats_per_block_request(
        &self,
        count: u64,
        reply_channel: oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
    ) {
        let connectivity = self.resources.connectivity.clone();

        let query_base_node_fut = async move {
            let client = connectivity.obtain_base_node_wallet_rpc_client().await;

            match client.get_mempool_fee_per_gram_stats(count).await {
                Ok(resp) => Ok(TransactionServiceResponse::FeePerGramStatsPerBlock(resp)),
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Error handling 'TransactionServiceRequest::GetFeePerGramStatsPerBlock' {:?}",
                        e
                    );
                    Err(TransactionServiceError::Other(e.to_string()))
                },
            }
        };

        tokio::spawn(async move {
            let resp = query_base_node_fut.await;
            if reply_channel.send(resp).is_err() {
                warn!(
                    target: LOG_TARGET,
                    "handle_get_fee_per_gram_stats_per_block_request: service reply cancelled"
                );
            }
        });
    }

    fn handle_fetch_unspent_outputs_request(
        &self,
        hashes: Vec<HashOutput>,
        reply_channel: oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
    ) {
        let connectivity = self.resources.connectivity.clone();

        let query_base_node_fut = async move {
            let mut res = vec![];
            let client = connectivity.obtain_base_node_wallet_rpc_client().await;
            for hash in hashes {
                match client
                    .fetch_utxo(hash.to_vec())
                    .await
                    .map_err(|e| TransactionServiceError::Other(e.to_string()))?
                {
                    Some(output) => res.push(output),
                    None => warn!(target: LOG_TARGET, "UTXO not found for hash: {hash}"),
                }
            }
            Ok(TransactionServiceResponse::UnspentOutputs(res))
        };

        tokio::spawn(async move {
            let resp = query_base_node_fut.await;
            if reply_channel.send(resp).is_err() {
                warn!(
                    target: LOG_TARGET,
                    "handle_fetch_unspent_outputs_request: service reply cancelled"
                );
            }
        });
    }

    async fn handle_base_node_service_event(&mut self, event: Arc<BaseNodeEvent>) {
        match (*event).clone() {
            BaseNodeEvent::BaseNodeStateChanged(_state) => {
                trace!(target: LOG_TARGET, "Received BaseNodeStateChanged event, but igoring",);
            },
        }
    }

    async fn handle_output_manager_service_event(
        &mut self,
        event: Arc<OutputManagerEvent>,
        transaction_validation_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<OperationId, TransactionServiceProtocolError<OperationId>>>,
        >,
    ) {
        if let OutputManagerEvent::TxoValidationSuccess(tx) = (*event).clone() {
            debug!(target: LOG_TARGET, "Received txo validation success event for oms: {}, starting output detection", tx);
            let _operation_id = self
                .start_transaction_validation_protocol(transaction_validation_join_handles)
                .await
                .map_err(|e| {
                    warn!(target: LOG_TARGET, "Error validating  txos: {e:?}");
                    e
                });
        }
    }

    async fn handle_utxo_scanner_service_event(
        &mut self,
        event: UtxoScannerEvent,
        transaction_validation_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<OperationId, TransactionServiceProtocolError<OperationId>>>,
        >,
    ) {
        match event {
            UtxoScannerEvent::ScanningRoundFailed { .. } => {},
            UtxoScannerEvent::Progress { .. } => {},
            UtxoScannerEvent::Completed { .. } => {
                let _operation_id = self
                    .start_transaction_validation_protocol(transaction_validation_join_handles)
                    .await
                    .map_err(|e| {
                        warn!(target: LOG_TARGET, "Error validating  txos: {e:?}");
                        e
                    });
            },
        }
    }

    /// Creates an encumbered uninitialized transaction
    #[allow(clippy::mutable_key_type)]
    pub async fn encumber_aggregate_tx(
        &mut self,
        fee_per_gram: MicroMinotari,
        expected_commitment: CompressedCommitment,
        script_input_shares: HashMap<CompressedPublicKey, CompressedCheckSigSchnorrSignature>,
        script_signature_public_nonces: Vec<CompressedPublicKey>,
        sender_offset_public_key_shares: Vec<CompressedPublicKey>,
        metadata_ephemeral_public_key_shares: Vec<CompressedPublicKey>,
        dh_shared_secret_shares: Vec<CompressedPublicKey>,
        recipient_address: TariAddress,
        original_maturity: u64,
        use_output: UseOutput,
        payment_id: MemoField,
    ) -> Result<
        (
            TxId,
            Transaction,
            CompressedPublicKey,
            CompressedPublicKey,
            CompressedPublicKey,
            CompressedPublicKey,
        ),
        TransactionServiceError,
    > {
        match self
            .resources
            .output_manager_service
            .encumber_aggregate_utxo(
                fee_per_gram,
                expected_commitment,
                script_input_shares,
                script_signature_public_nonces,
                sender_offset_public_key_shares,
                metadata_ephemeral_public_key_shares,
                dh_shared_secret_shares,
                recipient_address.clone(),
                original_maturity,
                use_output,
                payment_id.clone(),
            )
            .await
        {
            Ok((
                tx_id,
                transaction,
                amount,
                fee,
                total_script_key,
                total_metadata_ephemeral_public_key,
                total_script_nonce,
                shared_secret,
            )) => {
                let all_outputs = transaction
                    .body
                    .outputs()
                    .iter()
                    .map(|o| o.hash())
                    .collect::<Vec<HashOutput>>();
                let completed_tx = CompletedTransaction::new_with_output_hashes(
                    tx_id,
                    self.resources.one_sided_tari_address.clone(),
                    recipient_address,
                    amount,
                    fee,
                    transaction.clone(),
                    LegacyTransactionStatus::Pending,
                    Utc::now(),
                    TransactionDirection::Outbound,
                    None,
                    None,
                    payment_id.clone(),
                    all_outputs,
                    vec![],
                    vec![],
                    0,
                )
                .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
                self.db.insert_completed_transaction(tx_id, completed_tx)?;
                Ok((
                    tx_id,
                    transaction,
                    total_script_key,
                    total_metadata_ephemeral_public_key,
                    total_script_nonce,
                    shared_secret,
                ))
            },
            Err(e) => Err(e.into()),
        }
    }

    pub async fn spend_backup_pre_mine_utxo(
        &mut self,
        fee_per_gram: MicroMinotari,
        output_hash: HashOutput,
        expected_commitment: CompressedCommitment,
        recipient_address: TariAddress,
        payment_id: MemoField,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .resources
            .output_manager_service
            .spend_backup_pre_mine_utxo(
                fee_per_gram,
                output_hash,
                expected_commitment,
                recipient_address.clone(),
            )
            .await
        {
            Ok((tx_id, transaction, amount, fee)) => {
                let all_outputs = transaction
                    .body
                    .outputs()
                    .iter()
                    .map(|o| o.hash())
                    .collect::<Vec<HashOutput>>();
                let completed_tx = CompletedTransaction::new_with_output_hashes(
                    tx_id,
                    self.resources.one_sided_tari_address.clone(),
                    recipient_address,
                    amount,
                    fee,
                    transaction.clone(),
                    LegacyTransactionStatus::Pending,
                    Utc::now(),
                    TransactionDirection::Outbound,
                    None,
                    None,
                    payment_id,
                    all_outputs,
                    vec![],
                    vec![],
                    0,
                )
                .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
                self.db.insert_completed_transaction(tx_id, completed_tx)?;
                Ok(tx_id)
            },
            Err(e) => Err(e.into()),
        }
    }

    /// Creates an encumbered uninitialized transaction
    #[allow(clippy::too_many_lines)]
    pub async fn finalized_aggregate_encumbed_tx(
        &mut self,
        tx_id: TxId,
        total_meta_data_signature: CompressedSignature,
        total_script_data_signature: CompressedSignature,
        script_offset: PrivateKey,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        trace!(target: LOG_TARGET, "finalized_aggregate_encumbed_tx: start");
        let mut transaction = self.db.get_completed_transaction(tx_id)?;
        trace!(target: LOG_TARGET, "finalized_aggregate_encumbed_tx: completed_transaction");

        // Add the aggregate signature components
        transaction.transaction.script_offset = &transaction.transaction.script_offset + &script_offset;

        transaction.transaction.body.update_metadata_signature(
            &(transaction
                .transaction
                .body
                .outputs()
                .first()
                .expect("cannot be empty")
                .commitment
                .clone()),
            ComAndPubSignature::new_from_capk_signature(
                &transaction
                    .transaction
                    .body
                    .outputs()
                    .first()
                    .expect("Cannot be empty")
                    .metadata_signature
                    .to_capk_signature()? +
                    &total_meta_data_signature.to_schnorr_signature()?,
            ),
        )?;
        trace!(target: LOG_TARGET, "finalized_aggregate_encumbed_tx: updated metadata_signature");

        transaction.transaction.body.update_script_signature(
            &(transaction
                .transaction
                .body
                .inputs()
                .first()
                .expect("Cannot be empty")
                .commitment()?
                .clone()),
            ComAndPubSignature::new_from_capk_signature(
                &transaction
                    .transaction
                    .body
                    .inputs()
                    .first()
                    .expect("Cannot be empty")
                    .script_signature
                    .to_capk_signature()? +
                    &total_script_data_signature.to_schnorr_signature()?,
            ),
        )?;
        trace!(target: LOG_TARGET, "finalized_aggregate_encumbed_tx: updated script_signature");

        // Validate the aggregate signatures and script offset
        let factory = CommitmentFactory::default();
        let mut input_keys = UncompressedPublicKey::default();
        let last_seen_tip_height = self.db.get_last_scanned_height()?.unwrap_or(0);
        for input in transaction.transaction.body.inputs() {
            let context = ScriptContext::new(
                last_seen_tip_height,
                &[0; 32],
                input
                    .commitment()
                    .map_err(|e| TransactionServiceError::ServiceError(format!("TxId: {tx_id}, {e}")))?,
            );
            trace!(target: LOG_TARGET, "finalized_aggregate_encumbed_tx: input_data {:?}", input.input_data);
            input_keys = input_keys +
                input
                    .run_and_verify_script(&factory, Some(context))
                    .map_err(|e| TransactionServiceError::ServiceError(format!("TxId: {tx_id}, {e}")))?
                    .to_public_key()?;
        }
        trace!(target: LOG_TARGET, "finalized_aggregate_encumbed_tx: validated inputs");
        let mut output_keys = UncompressedPublicKey::default();
        for output in transaction.transaction.body.outputs() {
            output
                .verify_metadata_signature()
                .map_err(|e| TransactionServiceError::ServiceError(format!("TxId: {tx_id}, {e}")))?;
            output_keys = output_keys + output.sender_offset_public_key.clone().to_public_key()?;
        }
        trace!(target: LOG_TARGET, "finalized_aggregate_encumbed_tx: validated outputs");
        let lhs = input_keys - output_keys;
        if lhs != UncompressedPublicKey::from_secret_key(&transaction.transaction.script_offset) {
            return Err(TransactionServiceError::ServiceError(format!(
                "Invalid script offset (TxId: {tx_id})"
            )));
        }
        trace!(target: LOG_TARGET, "finalized_aggregate_encumbed_tx: validated script offstet");

        // Update the wallet database
        let _res = self
            .resources
            .output_manager_service
            .update_output_metadata_signature(
                transaction
                    .transaction
                    .body
                    .outputs()
                    .first()
                    .expect("Cannot be empty")
                    .clone(),
            )
            .await;

        self.db.update_completed_transaction(tx_id, transaction)?;

        self.resources
            .output_manager_service
            .confirm_pending_transaction(tx_id, None, None)
            .await?;

        // Notify that the transaction was successfully resolved.
        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));

        self.complete_send_transaction_protocol(
            Ok(TransactionSendResult {
                tx_id,
                transaction_status: LegacyTransactionStatus::Completed,
            }),
            transaction_broadcast_join_handles,
        );

        Ok(tx_id)
    }

    /// broadcasts a SHA-XTR atomic swap transaction
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    #[allow(clippy::too_many_lines)]
    pub async fn send_sha_atomic_swap_transaction(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<Box<(TxId, CompressedPublicKey, TransactionOutput)>, TransactionServiceError> {
        if selection_criteria.range_limit.is_some() {
            return Err(TransactionServiceError::RangeLimitError {
                reason: "Range limit coin-join cannot be set for send_sha_atomic_swap_transaction".to_string(),
            });
        }
        let temp_tx_id = TxId::new_random();
        self.verify_send(&destination, TariAddressFeatures::create_one_sided_only())?;
        // this can be anything, so lets generate a random private key
        let pre_image = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng));
        let hash: [u8; 32] = Sha256::digest(pre_image.as_bytes()).into();

        // lets make the unlock height a day from now, 2 min blocks which gives us 30 blocks per hour * 24 hours

        let tip_height = self.resources.db.get_last_scanned_height()?.unwrap_or(0);

        let height = tip_height + (24 * 30);

        // lets create the HTLC script
        let script = script!(
            HashSha256 PushHash(Box::new(hash)) Equal IfThen
                PushPubKey(Box::new(destination.public_spend_key().clone()))
            Else
                CheckHeightVerify(height) PushPubKey(Box::new(self.resources.one_sided_tari_address.public_spend_key().clone()))
            EndIf
        )?;

        // Prepare sender part of the transaction
        let covenant = Covenant::default();
        let output_features = OutputFeatures::default();
        let temp_payment_id = payment_id
            .clone()
            .add_sender_address(self.resources.one_sided_tari_address.clone(), false, 0.into(), None)
            .map_err(TransactionServiceError::InvalidPaymentId)?;
        let mut tx_builder = self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                temp_tx_id,
                amount,
                selection_criteria,
                output_features.clone(),
                fee_per_gram,
                script.clone(),
                covenant.clone(),
                temp_payment_id,
            )
            .await?;
        let fee_estimate = tx_builder.get_fee_estimate_without_change()?;

        let payment_id = payment_id
            .add_sender_address(self.resources.one_sided_tari_address.clone(), false, fee_estimate, None)
            .map_err(TransactionServiceError::InvalidPaymentId)?;

        tx_builder.with_tx_type(TxType::ClaimAtomicSwap);

        // Diffie-Hellman shared secret `k_Ob * K_Sb = K_Ob * k_Sb` results in a public key, which is fed into
        // KDFs to produce the spending, rewind, and encryption keys
        let sender_offset_private_key = self
            .resources
            .transaction_key_manager_service
            .get_random_key(None, None)?;

        let shared_secret = self
            .resources
            .transaction_key_manager_service
            .get_diffie_hellman_shared_secret(
                &sender_offset_private_key.key_id,
                destination
                    .public_view_key()
                    .ok_or(TransactionServiceProtocolError::new(
                        temp_tx_id,
                        TransactionServiceError::InvalidAddress("Missing public view key".to_string()),
                    ))?,
            )?;
        let spending_key = public_key_to_output_spending_key(&shared_secret)
            .map_err(|e| TransactionServiceProtocolError::new(temp_tx_id, e.into()))?;

        let encryption_private_key = public_key_to_output_encryption_key(&shared_secret)?;
        let encryption_key = self
            .resources
            .transaction_key_manager_service
            .create_encrypted_key(encryption_private_key, None)?;

        let sender_offset_public_key = self
            .resources
            .transaction_key_manager_service
            .get_public_key_at_key_id(&sender_offset_private_key.key_id)?;

        let spending_key_id = self
            .resources
            .transaction_key_manager_service
            .create_encrypted_key(spending_key, None)?;

        let minimum_value_promise = MicroMinotari::zero();
        let output = WalletOutputBuilder::new(amount, spending_key_id)
            .with_features(output_features)
            .with_script(script)
            .encrypt_data_for_recovery(
                &self.resources.transaction_key_manager_service,
                Some(&encryption_key),
                payment_id.clone(),
            )?
            .with_input_data(ExecutionStack::default())
            .with_covenant(covenant)
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(self.resources.transaction_key_manager_service.get_spend_key().key_id)
            .with_minimum_value_promise(minimum_value_promise)
            .sign_metadata_signature(
                &self.resources.transaction_key_manager_service,
                &sender_offset_private_key.key_id,
            )
            .unwrap()
            .try_build(&self.resources.transaction_key_manager_service)
            .unwrap();

        tx_builder.add_recipient(
            destination.clone(),
            output.clone(),
            Some(sender_offset_private_key.key_id),
            Some(encryption_key),
        )?;

        // Finalize
        let finalized = tx_builder.build()?;

        info!(target: LOG_TARGET, "Finalized one-side transaction TxId: {}", finalized.tx_id);

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(
                finalized.tx_id,
            )));

        // Broadcast one-sided transaction

        let tx = finalized.transaction.clone();
        let fee = finalized.fee;
        self.resources
            .output_manager_service
            .add_output_with_tx_id(temp_tx_id, output.clone(), Some(SpendingPriority::HtlcSpendAsap))
            .await?;
        let change = finalized.change.clone().map(|change| vec![change]);
        self.resources
            .output_manager_service
            .confirm_pending_transaction(temp_tx_id, Some(finalized.tx_id), change)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(finalized.tx_id, e.into()))?;
        let sent_hashes = finalized.sent_output_hashes.clone();
        let change_hashes = finalized.change_output_hashes.clone();

        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new_with_output_hashes(
                finalized.tx_id,
                self.resources.one_sided_tari_address.clone(),
                destination,
                amount,
                fee,
                tx.clone(),
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Outbound,
                None,
                None,
                finalized.payment_id.clone(),
                sent_hashes,
                vec![],
                change_hashes,
                0,
            )?,
        )
        .await?;

        let tx_output = output.to_transaction_output()?;

        Ok(Box::new((finalized.tx_id, pre_image, tx_output)))
    }

    #[allow(clippy::too_many_lines)]
    async fn send_one_sided_or_stealth(
        &mut self,
        dest_address: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        mut payment_id: MemoField,
    ) -> Result<TxId, TransactionServiceError> {
        debug!(target: LOG_TARGET, "Sending one sided transaction to {dest_address} with amount {amount}");
        self.verify_send(&dest_address, TariAddressFeatures::create_one_sided_only())?;
        if selection_criteria.range_limit.is_some() {
            return Err(TransactionServiceError::RangeLimitError {
                reason: "Range limit coin-join cannot be set for send_one_sided_or_stealth".to_string(),
            });
        }
        let temp_tx_id = TxId::new_random();
        // let override the payment_id if the address says we should
        if dest_address.features().contains(TariAddressFeatures::PAYMENT_ID) {
            debug!(target: LOG_TARGET, "Address contains memo, overriding memo {} with {:?}", payment_id, dest_address.get_memo_field_payment_id_bytes());
            payment_id = MemoField::new_open(dest_address.get_memo_field_payment_id_bytes(), TxType::PaymentToOther)
                .map_err(OutputManagerError::InvalidPaymentIdFormat)?;
        }

        // Prepare sender part of the transaction
        let script = push_pubkey_script(&Default::default());
        let covenant = Covenant::default();
        payment_id = payment_id
            .add_sender_address(
                self.resources.one_sided_tari_address.clone(),
                true,
                0.into(),
                if dest_address == self.resources.one_sided_tari_address {
                    Some(TxType::PaymentToSelf)
                } else {
                    Some(TxType::PaymentToOther)
                },
            )
            .map_err(TransactionServiceError::InvalidPaymentId)?;
        let mut tx_builder = self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                temp_tx_id,
                amount,
                selection_criteria.clone(),
                output_features.clone(),
                fee_per_gram,
                script,
                covenant,
                payment_id.clone(),
            )
            .await?;
        if let UtxoSelectionFilter::MustInclude { commitments } = selection_criteria.filter {
            let inputs = tx_builder.inputs();
            for commitment in commitments {
                if !inputs.iter().any(|input| input.output.commitment() == &commitment) {
                    return Err(TransactionServiceError::OutputManagerError(
                        OutputManagerError::BuildError(format!(
                            "The required UTXO with commitment {} was not selected",
                            commitment.to_hex()
                        )),
                    ));
                }
            }
        }
        let fee_estimate = tx_builder.get_fee_estimate_without_change()?;
        payment_id.set_fee(fee_estimate);

        trace!(target: LOG_TARGET, "Finalized payment_id: {payment_id}");

        tx_builder.add_stealth_recipient(
            dest_address.clone(),
            amount,
            output_features.clone(),
            payment_id.clone(),
        )?;
        tx_builder.with_memo(payment_id.clone());
        let finalized = tx_builder.build()?;

        // Finalize

        info!(target: LOG_TARGET, "Finalized one-side transaction TxId: {}", finalized.tx_id);

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _result = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(
                finalized.tx_id,
            )));

        // Broadcast one-sided transaction

        let tx = finalized.transaction.clone();
        let fee = finalized.fee;
        let change = finalized.change.clone().map(|change| vec![change]);
        self.resources
            .output_manager_service
            .confirm_pending_transaction(temp_tx_id, Some(finalized.tx_id), change)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(finalized.tx_id, e.into()))?;
        let sent_hashes = finalized.sent_output_hashes.clone();
        let change_hashes = finalized.change_output_hashes.clone();
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new_with_output_hashes(
                finalized.tx_id,
                self.resources.one_sided_tari_address.clone(),
                dest_address.clone(),
                amount,
                fee,
                tx.clone(),
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Outbound,
                None,
                None,
                finalized.payment_id.clone(),
                sent_hashes,
                vec![],
                change_hashes,
                0,
            )?,
        )
        .await?;

        Ok(finalized.tx_id)
    }

    #[allow(clippy::too_many_lines)]
    async fn send_range_limited_coin_join(
        &mut self,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee: FeeType,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        payment_id: MemoField,
    ) -> Result<TxId, TransactionServiceError> {
        let range_limit_criteria =
            selection_criteria
                .clone()
                .range_limit
                .ok_or_else(|| OutputManagerError::RangeLimitError {
                    reason: "Range limit must be specified for range limited coin-join UTXO selection".to_string(),
                    range_exhausted: false,
                })?;
        let temp_tx_id = TxId::new_random();

        // Prepare sender part of the transaction
        let script = push_pubkey_script(&Default::default());
        let covenant = Covenant::default();
        let mut tx_builder = self
            .resources
            .output_manager_service
            .prepare_range_limited_coin_join_transaction_to_send(
                temp_tx_id,
                selection_criteria,
                output_features.clone(),
                fee,
                script,
                covenant,
            )
            .await?;
        let fee_estimate = tx_builder.get_fee_estimate_without_change()?;
        let amount_without_fee = tx_builder.get_total_input_value()?.saturating_sub(fee_estimate);
        let dest_address = self.resources.one_sided_tari_address.clone();
        debug!(
            target: LOG_TARGET,
            "Sending range_limit_coin_join transaction to {} with amount {amount_without_fee} fee {fee_estimate} total {}",
            dest_address.to_hex(), tx_builder.get_total_input_value()?
        );

        let payment_id = payment_id
            .add_sender_address(
                self.resources.one_sided_tari_address.clone(),
                true,
                fee_estimate,
                Some(TxType::CoinJoin),
            )
            .map_err(TransactionServiceError::InvalidPaymentId)?;
        trace!(target: LOG_TARGET, "Finalized payment_id: {payment_id}");
        self.verify_send(&dest_address, TariAddressFeatures::create_one_sided_only())?;

        // Note: Division by zero is checked during 'prepare_range_limited_coin_join_transaction_to_send'
        let number_of_outputs =
            usize::try_from(amount_without_fee.as_u64() / range_limit_criteria.target_minimum_amount)
                .map_err(|_e| OutputManagerError::ConversionError("number_of_outputs".to_string()))?
                .max(1);
        let mut values = vec![MicroMinotari(range_limit_criteria.target_minimum_amount); number_of_outputs];
        // Note: 'amount_without_fee >= target_minimum_amount' is checked during
        //       'prepare_range_limited_coin_join_transaction_to_send'
        let residual = amount_without_fee
            .as_u64()
            .saturating_sub(range_limit_criteria.target_minimum_amount * number_of_outputs as u64);
        values.get_mut(0).expect("index exists").0 += residual;

        for value in values {
            tx_builder.add_stealth_recipient(
                dest_address.clone(),
                value,
                output_features.clone(),
                payment_id.clone(),
            )?;
        }
        tx_builder.with_memo(payment_id.clone()).with_tx_type(TxType::CoinJoin);

        // Finalize
        let finalized = tx_builder.build()?;
        if let Some(change) = finalized.change {
            let msg = format!(
                "One sided range_limit_coin_join transaction cannot have a change output: {}",
                change.value()
            );
            error!(target: LOG_TARGET, "{}", msg);
            return Err(TransactionServiceError::RangeLimitError { reason: msg });
        }
        if amount_without_fee != finalized.amount {
            let msg = format!(
                "One sided range_limit_coin_join transaction amount mismatch: expected {}, got {}",
                amount_without_fee, finalized.amount
            );
            error!(target: LOG_TARGET, "{}", msg);
            return Err(TransactionServiceError::RangeLimitError { reason: msg });
        }
        if fee_estimate != finalized.fee {
            let msg = format!(
                "One sided range_limit_coin_join transaction fee mismatch: expected {}, got {}",
                fee_estimate, finalized.fee
            );
            error!(target: LOG_TARGET, "{}", msg);
            return Err(TransactionServiceError::RangeLimitError { reason: msg });
        }

        info!(target: LOG_TARGET, "Finalized one-side transaction TxId: {}", finalized.tx_id);

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _result = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(
                finalized.tx_id,
            )));

        // Broadcast one-sided transaction

        let tx = finalized.transaction.clone();
        let final_fee = finalized.fee;
        let change = finalized.change.clone().map(|change| vec![change]);
        self.resources
            .output_manager_service
            .confirm_pending_transaction(temp_tx_id, Some(finalized.tx_id), change)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(finalized.tx_id, e.into()))?;
        let sent_hashes = finalized.sent_output_hashes.clone();
        let change_hashes = finalized.change_output_hashes.clone();
        let mut final_payment_id = payment_id.clone();
        final_payment_id.set_fee(final_fee);
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new_with_output_hashes(
                finalized.tx_id,
                self.resources.one_sided_tari_address.clone(),
                dest_address.clone(),
                amount_without_fee,
                final_fee,
                tx.clone(),
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Outbound,
                None,
                None,
                final_payment_id,
                sent_hashes,
                vec![],
                change_hashes,
                0,
            )?,
        )
        .await?;

        Ok(finalized.tx_id)
    }

    #[allow(clippy::too_many_lines)]
    async fn scrape_wallet(
        &mut self,
        dest_address: TariAddress,
        fee_per_gram: MicroMinotari,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        let temp_tx_id = TxId::new_random();
        self.verify_send(&dest_address, TariAddressFeatures::create_one_sided_only())?;

        // Prepare sender part of the transaction
        let mut tx_builder = self
            .resources
            .output_manager_service
            .scrape_wallet(temp_tx_id, fee_per_gram)
            .await?;

        // Prepare receiver part of the transaction

        // Diffie-Hellman shared secret `k_Ob * K_Sb = K_Ob * k_Sb` results in a public key, which is fed into
        // KDFs to produce the spending, rewind, and encryption keys
        let sender_offset_private_key = self
            .resources
            .transaction_key_manager_service
            .get_random_key(None, Some(LedgerKeyBranch::OneSidedSenderOffset))?;

        let shared_secret = self
            .resources
            .transaction_key_manager_service
            .get_diffie_hellman_shared_secret(
                &sender_offset_private_key.key_id,
                dest_address
                    .public_view_key()
                    .ok_or(TransactionServiceProtocolError::new(
                        temp_tx_id,
                        TransactionServiceError::OneSidedTransactionError("Missing public view key".to_string()),
                    ))?,
            )?;
        let commitment_mask_private_key = public_key_to_output_spending_key(&shared_secret)
            .map_err(|e| TransactionServiceProtocolError::new(temp_tx_id, e.into()))?;
        let commitment_mask_key_id = &self
            .resources
            .transaction_key_manager_service
            .create_encrypted_key(commitment_mask_private_key.clone(), None)?;

        let script_spending_key = self
            .resources
            .transaction_key_manager_service
            .stealth_address_script_spending_key(commitment_mask_key_id, dest_address.public_spend_key())?;
        let script = push_pubkey_script(&script_spending_key);

        let encryption_private_key = public_key_to_output_encryption_key(&shared_secret)?;
        let encryption_key = self
            .resources
            .transaction_key_manager_service
            .create_encrypted_key(encryption_private_key, None)?;

        let spending_key_id = self
            .resources
            .transaction_key_manager_service
            .create_encrypted_key(commitment_mask_private_key, None)?;

        let sender_offset_public_key = self
            .resources
            .transaction_key_manager_service
            .get_public_key_at_key_id(&sender_offset_private_key.key_id)?;
        let amount = tx_builder.get_total_input_value()?;
        let fee = tx_builder.get_fee_estimate_without_change()?;
        let minimum_value_promise = MicroMinotari::zero();
        let payment_id = MemoField::new_address_and_data(
            self.resources.one_sided_tari_address.clone(),
            fee,
            true,
            TxType::PaymentToOther,
            vec![],
        )
        .map_err(|e| TransactionServiceError::InvalidPaymentId(e.to_string()))?;
        let output = WalletOutputBuilder::new(amount, spending_key_id)
            .with_features(Default::default())
            .with_script(script)
            .encrypt_data_for_recovery(
                &self.resources.transaction_key_manager_service,
                Some(&encryption_key),
                payment_id.clone(),
            )?
            .with_input_data(Default::default())
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(TariKeyId::Zero)
            .with_minimum_value_promise(minimum_value_promise)
            .sign_metadata_signature_user_verified(
                &self.resources.transaction_key_manager_service,
                &sender_offset_private_key.key_id,
                &dest_address,
            )?
            .try_build(&self.resources.transaction_key_manager_service)?;

        tx_builder.add_recipient(
            dest_address.clone(),
            output.clone(),
            Some(sender_offset_private_key.key_id),
            Some(encryption_key),
        )?;

        let finalized = tx_builder.build()?;

        info!(target: LOG_TARGET, "Finalized one-side transaction TxId: {}", finalized.tx_id);

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _result = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(
                finalized.tx_id,
            )));

        // Broadcast one-sided transaction

        let tx = finalized.transaction.clone();
        let fee = finalized.fee;
        self.resources
            .output_manager_service
            .add_output_with_tx_id(temp_tx_id, output.clone(), Some(SpendingPriority::HtlcSpendAsap))
            .await?;
        let change = finalized.change.clone().map(|change| vec![change]);
        self.resources
            .output_manager_service
            .confirm_pending_transaction(temp_tx_id, Some(finalized.tx_id), change)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(finalized.tx_id, e.into()))?;
        let received_hashes = finalized.sent_output_hashes.clone();
        let change_hashes = finalized.change_output_hashes.clone();

        let mut final_payment_id = payment_id.clone();
        final_payment_id.set_fee(fee);
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new_with_output_hashes(
                finalized.tx_id,
                self.resources.one_sided_tari_address.clone(),
                dest_address,
                amount,
                fee,
                tx.clone(),
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Outbound,
                None,
                None,
                final_payment_id,
                vec![],
                received_hashes,
                change_hashes,
                0,
            )?,
        )
        .await?;

        Ok(finalized.tx_id)
    }

    /// Sends a one side payment transaction to a recipient
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    pub async fn send_one_sided_transaction(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        self.send_one_sided_or_stealth(
            destination,
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            transaction_broadcast_join_handles,
            payment_id,
        )
        .await
    }

    /// Sends a one side payment transaction to each of the recipients. Although only a single transaction will be
    /// submitted to be broadcast, a separate completed transaction is created and saved for each recipient. Each
    /// completed transactions will be allocated to one of the recipients, with its corresponding recipient address,
    /// amount and memo field. The fee and transaction id will correlate to the first saved transaction; each
    /// consecutive saved transaction will have a random transaction id and zero fee. The memos to each recipient will
    /// also correlate with the apportioned fees.
    /// # Arguments
    /// 'destinations': array of destinations of (TariAddress, amount, MemoField)
    /// 'selection_criteria': The UTXO selection criteria to use for coin selection
    /// 'output_features': The output features to use for the transaction outputs
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    #[allow(clippy::too_many_lines)]
    pub async fn send_many_one_sided_transactions(
        &mut self,
        mut destinations: Vec<(TariAddress, MicroMinotari, MemoField)>,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<Vec<TxId>, TransactionServiceError> {
        debug!(target: LOG_TARGET, "Sending many one sided transactions");
        if destinations.is_empty() {
            return Err(TransactionServiceError::TransactionBuilderError(
                TransactionBuilderError::NoRecipients,
            ));
        }
        if selection_criteria.range_limit.is_some() {
            return Err(TransactionServiceError::RangeLimitError {
                reason: "Range limit coin-join cannot be set for send_many_one_sided_transactions".to_string(),
            });
        }
        let temp_tx_id = TxId::new_random();
        // let override the payment_id if the address says we should
        let mut total_send = MicroMinotari::zero();
        let covenant = Covenant::default();
        let script = push_pubkey_script(&Default::default());
        let tip_height = self.db.get_last_scanned_height()?.unwrap_or(0);
        for (address, amount, memo) in &mut destinations {
            self.verify_send(address, TariAddressFeatures::create_one_sided_only())?;
            if address.features().contains(TariAddressFeatures::PAYMENT_ID) {
                debug!(target: LOG_TARGET, "Address contains memo, overriding memo {} with {:?}", memo, address.get_memo_field_payment_id_bytes());
                *memo = MemoField::new_open(address.get_memo_field_payment_id_bytes(), TxType::PaymentToOther)
                    .map_err(OutputManagerError::InvalidPaymentIdFormat)?;
            }
            *memo = memo
                .clone()
                .add_sender_address(
                    self.resources.one_sided_tari_address.clone(),
                    true,
                    0.into(),
                    if *address == self.resources.one_sided_tari_address {
                        Some(TxType::PaymentToSelf)
                    } else {
                        Some(TxType::PaymentToOther)
                    },
                )
                .map_err(TransactionServiceError::InvalidPaymentId)?;

            // Doing the fee estimate in the oms is going to very complex so lets rather over estimate the fee so that
            // we have enough to send here
            let features_and_scripts_byte_size = self
                .resources
                .consensus_manager
                .consensus_constants(tip_height)
                .transaction_weight_params()
                .round_up_features_and_scripts_size(
                    OutputFeatures::default()
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                        TariScript::default()
                            .get_serialized_size()
                            .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                        Covenant::new()
                            .get_serialized_size()
                            .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                        memo.get_size(),
                );
            let fee_calc = Fee::new(
                *self
                    .resources
                    .consensus_manager
                    .consensus_constants(tip_height)
                    .transaction_weight_params(),
            );
            let default_output_fee = fee_calc.calculate(fee_per_gram, 0, 0, 1, features_and_scripts_byte_size);
            total_send += *amount;
            total_send += default_output_fee;
        }

        // Prepare sender part of the transaction
        let mut tx_builder = self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                temp_tx_id,
                total_send,
                selection_criteria,
                output_features.clone(),
                fee_per_gram,
                script,
                covenant,
                destinations.first().expect("already checked").2.clone(),
            )
            .await?;
        let fee_estimate = tx_builder.get_fee_estimate_without_change()?;
        for (address, amount, memo) in &mut destinations {
            // Let's override the payment_id if the address says we should

            memo.set_fee(fee_estimate);

            tx_builder.add_stealth_recipient(address.clone(), *amount, output_features.clone(), memo.clone())?;
        }

        let finalized = tx_builder.build()?;

        // Finalize

        info!(target: LOG_TARGET, "Finalized one-side transaction TxId: {}", finalized.tx_id);

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _result = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(
                finalized.tx_id,
            )));

        // Broadcast one-sided transaction

        let tx = finalized.transaction.clone();
        let change = finalized.change.clone().map(|change| vec![change]);
        self.resources
            .output_manager_service
            .confirm_pending_transaction(temp_tx_id, Some(finalized.tx_id), change)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(finalized.tx_id, e.into()))?;
        let change_hashes = finalized.change_output_hashes.clone();

        check_transaction_size(&tx, finalized.tx_id)?;

        let mut tx_ids = Vec::new();
        let mut completed_txs = Vec::new();
        let view_key = self.resources.transaction_key_manager_service.get_view_key().pub_key;

        for (i, (address, amount, memo)) in destinations.into_iter().enumerate() {
            let tx_id = if i == 0 {
                finalized.tx_id
            } else {
                finalized
                    .sent_outputs
                    .get(i)
                    .ok_or(TransactionServiceError::Other(
                        "sent_outputs index out of bounds".to_string(),
                    ))?
                    .output
                    .calculate_tx_id(view_key.as_bytes())
            };

            let sent_hash = finalized
                .sent_output_hashes
                .get(i)
                .copied()
                .ok_or(TransactionServiceError::Other(
                    "sent_output_hashes index out of bounds".to_string(),
                ))?;

            tx_ids.push(tx_id);
            let mut final_payment_id = memo.clone();
            final_payment_id.set_fee(finalized.fee);
            let completed_tx = CompletedTransaction::new_with_output_hashes(
                tx_id,
                self.resources.one_sided_tari_address.clone(),
                address,
                amount,
                finalized.fee,
                tx.clone(),
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Outbound,
                None,
                None,
                final_payment_id,
                vec![sent_hash],
                vec![],
                change_hashes.clone(),
                0,
            )?;
            completed_txs.push(completed_tx);
        }

        let first_completed_tx = completed_txs.remove(0);
        self.submit_transaction(transaction_broadcast_join_handles, first_completed_tx)
            .await?;

        for completed_tx in completed_txs {
            self.db.insert_completed_transaction(completed_tx.tx_id, completed_tx)?;
            trace!(
                target: LOG_TARGET,
                "Created transaction for ({}).", finalized.tx_id
            );
        }

        Ok(tx_ids)
    }

    /// Creates a transaction to burn some Minotari. The optional _claim public key_ parameter is used in the challenge
    /// of the
    // corresponding optional _ownership proof_ return value. Burn commitments and ownership proofs will exclusively be
    // used in the 2nd layer (DAN layer). When such an _ownership proof_ is presented later on as part of some
    // transaction metadata, the _claim public key_ can be revealed to enable verification of the _ownership proof_
    // and the transaction can be signed with the private key corresponding to the claim public key.
    #[allow(clippy::too_many_lines)]
    pub async fn burn_tari(
        &mut self,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
        claim_public_key: Option<CompressedPublicKey>,
        sidechain_deployment_key: Option<PrivateKey>,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<(TxId, Option<PartialBurnClaimProof>), TransactionServiceError> {
        if selection_criteria.range_limit.is_some() {
            return Err(TransactionServiceError::RangeLimitError {
                reason: "Range limit coin-join cannot be set for burn_tari".to_string(),
            });
        }
        let temp_tx_id = TxId::new_random();

        if claim_public_key.is_none() && sidechain_deployment_key.is_some() {
            return Err(TransactionServiceError::InvalidBurnTransaction(
                "A sidechain deployment key was provided without a claim public key".to_string(),
            ));
        }
        let output_features = claim_public_key
            .as_ref()
            .cloned()
            .map(|c| OutputFeatures::create_burn_confidential_output(c, sidechain_deployment_key.as_ref()))
            .unwrap_or_else(OutputFeatures::create_burn_output);

        // Prepare sender part of the transaction
        let covenant = Covenant::default();
        let script = script!(Nop)?;
        let temp_payment_id = payment_id
            .clone()
            .add_sender_address(
                self.resources.one_sided_tari_address.clone(),
                false,
                0.into(),
                Some(TxType::Burn),
            )
            .map_err(TransactionServiceError::InvalidPaymentId)?;
        let mut tx_builder = self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                temp_tx_id,
                amount,
                selection_criteria.clone(),
                output_features.clone(),
                fee_per_gram,
                script,
                covenant,
                temp_payment_id,
            )
            .await?;
        let fee = tx_builder.get_fee_estimate_without_change()?;

        let payment_id = payment_id
            .add_sender_address(
                self.resources.one_sided_tari_address.clone(),
                false,
                fee,
                Some(TxType::Burn),
            )
            .map_err(TransactionServiceError::InvalidPaymentId)?;
        trace!(
            target: LOG_TARGET,
            "Burning transaction start - TxId: {}, amount: {}, fee per gram: {}, payment id: {}, claim pk: {}, \
            selection: {}",
            temp_tx_id, amount, fee_per_gram, payment_id, claim_public_key.clone().unwrap_or_default(), selection_criteria
        );

        tx_builder.with_tx_type(TxType::Burn);
        tx_builder.with_kernel_features(KernelFeatures::create_burn());
        // This call is needed to advance the state from `SingleRoundMessageReady` to `SingleRoundMessageReady`,
        // but the returned value is not used
        let (commitment_mask_key, _) = self
            .resources
            .transaction_key_manager_service
            .get_next_commitment_mask_and_script_key()?;

        let sender_offset_private_key = self
            .resources
            .transaction_key_manager_service
            .get_random_key(None, None)?;
        let recovery_key_id = if let Some(ref cp) = claim_public_key {
            TariKeyId::DHEncryptedData {
                public_key: cp.clone(),
                private_key: sender_offset_private_key.key_id.clone().into(),
            }
        } else {
            self.resources.transaction_key_manager_service.get_view_key().key_id
        };
        let mut output_builder = WalletOutputBuilder::new(amount, commitment_mask_key.key_id.clone())
            .with_features(output_features)
            .with_script(script!(Nop)?)
            .with_input_data(Default::default())
            .with_sender_offset_public_key(sender_offset_private_key.pub_key.clone())
            .with_script_key(TariKeyId::Zero)
            .with_minimum_value_promise(MicroMinotari::zero());

        output_builder = output_builder.encrypt_data_for_recovery(
            &self.resources.transaction_key_manager_service,
            Some(&recovery_key_id),
            payment_id.clone(),
        )?;

        let output = output_builder
            .sign_metadata_signature(
                &self.resources.transaction_key_manager_service,
                &sender_offset_private_key.key_id,
            )?
            .try_build(&self.resources.transaction_key_manager_service)?;

        tx_builder.add_recipient(
            Default::default(),
            output.clone(),
            Some(sender_offset_private_key.key_id),
            Some(recovery_key_id),
        )?;

        let finalized = tx_builder.build()?;

        self.resources
            .output_manager_service
            .add_output_with_tx_id(temp_tx_id, output, None)
            .await?;

        let change = finalized.change.map(|change| vec![change]);
        self.resources
            .output_manager_service
            .confirm_pending_transaction(temp_tx_id, Some(finalized.tx_id), change)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(finalized.tx_id, e.into()))?;

        info!(target: LOG_TARGET, "Finalized burning transaction - TxId: {}", finalized.tx_id);

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _result = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(
                finalized.tx_id,
            )));
        let mut final_payment_id = payment_id.clone();
        final_payment_id.set_fee(finalized.fee);
        let completed_transaction = CompletedTransaction::new_with_output_hashes(
            finalized.tx_id,
            self.resources.one_sided_tari_address.clone(),
            TariAddress::default(),
            amount,
            finalized.fee,
            finalized.transaction,
            LegacyTransactionStatus::Completed,
            Utc::now(),
            TransactionDirection::Outbound,
            None,
            None,
            final_payment_id,
            finalized.sent_output_hashes,
            vec![],
            finalized.change_output_hashes,
            0,
        )?;

        let burn_kernel = completed_transaction
            .transaction
            .body
            .kernels()
            .iter()
            .find(|k| k.features.is_burned())
            .ok_or(TransactionServiceError::InvalidBurnTransaction(
                "No burn kernel found in transaction".to_string(),
            ))?
            .clone();

        self.submit_transaction(transaction_broadcast_join_handles, completed_transaction)
            .await?;
        info!(target: LOG_TARGET, "Submitted burning transaction - TxId: {}", finalized.tx_id);

        // Generate claim proof if needed
        let mut burn_proof = None;
        if let Some(claim_public_key) = claim_public_key {
            let tx_output = finalized
                .sent_outputs
                .first()
                .expect("a recipient was added, so there must be at least one output");
            let output_hash = tx_output.output.output_hash();
            let commitment = tx_output.output.commitment().clone();

            let ownership_proof = self
                .resources
                .transaction_key_manager_service
                .generate_burn_claim_signature(&commitment_mask_key.key_id, amount.as_u64(), &claim_public_key)?;
            let proof = PartialBurnClaimProof {
                // Nonce part of the DH key exchange to derive the shared secret and decryption key
                claim_public_key,
                commitment,
                ownership_proof,
                kernel_excess: burn_kernel.excess.as_bytes().to_vec(),
                kernel_excess_nonce: burn_kernel.excess_sig.get_compressed_public_nonce().to_vec(),
                kernel_excess_signature: burn_kernel.excess_sig.get_signature().to_vec(),
                sender_offset_public_key: sender_offset_private_key.pub_key.clone(),
            };

            self.db.insert_burn_proof(
                output_hash,
                &proof,
                &burn_kernel,
                tx_output.output.encrypted_data(),
                tx_output.output.value(),
            )?;
            burn_proof = Some(proof);
        }

        Ok((finalized.tx_id, burn_proof))
    }

    async fn register_validator_node(
        &mut self,
        amount: MicroMinotari,
        validator_node_public_key: CommsPublicKey,
        validator_node_signature: CompressedSignature,
        validator_node_claim_public_key: CompressedPublicKey,
        sidechain_deployment_key: Option<PrivateKey>,
        max_epoch: VnEpoch,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        let signature = ValidatorNodeSignature::new(validator_node_public_key, validator_node_signature);
        let sidechain_pk = sidechain_deployment_key
            .as_ref()
            .map(CompressedPublicKey::from_secret_key);
        if !signature.is_valid_registration_signature_for(
            sidechain_pk.as_ref(),
            &validator_node_claim_public_key,
            max_epoch,
        ) {
            return Err(TransactionServiceError::InvalidValidatorNodeSignature);
        }

        let output_features = OutputFeatures::for_validator_node_registration(
            signature,
            validator_node_claim_public_key,
            sidechain_deployment_key.as_ref(),
            max_epoch,
        );

        let (fee, transaction, tx_id) = self
            .resources
            .output_manager_service
            .create_pay_to_self_transaction(
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                None,
                payment_id.clone(),
                // Set minimum value promise to the amount provided. VN Reg outputs are required by validation to use
                // this.
                amount,
            )
            .await?;

        // Notify that the transaction was successfully resolved.
        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));
        let all_outputs = transaction
            .body
            .outputs()
            .iter()
            .map(|o| o.hash())
            .collect::<Vec<HashOutput>>();
        let lock_height = 0;
        let mut final_payment_id = payment_id.clone();
        final_payment_id.set_fee(fee);
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new_with_output_hashes(
                tx_id,
                self.resources.one_sided_tari_address.clone(),
                self.resources.one_sided_tari_address.clone(),
                amount,
                fee,
                transaction,
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Inbound,
                None,
                None,
                final_payment_id,
                vec![],
                all_outputs,
                vec![],
                lock_height,
            )?,
        )
        .await?;

        Ok(tx_id)
    }

    async fn submit_validator_exit(
        &mut self,
        amount: MicroMinotari,
        validator_node_public_key: CommsPublicKey,
        validator_node_signature: CompressedSignature,
        sidechain_deployment_key: Option<PrivateKey>,
        selection_criteria: UtxoSelectionCriteria,
        max_epoch: VnEpoch,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        let signature = ValidatorNodeSignature::new(validator_node_public_key, validator_node_signature);
        let sidechain_pk = sidechain_deployment_key
            .as_ref()
            .map(CompressedPublicKey::from_secret_key);
        if !signature.is_valid_exit_signature_for(sidechain_pk.as_ref(), max_epoch) {
            return Err(TransactionServiceError::InvalidValidatorNodeSignature);
        }

        let output_features =
            OutputFeatures::for_validator_node_exit(signature, sidechain_deployment_key.as_ref(), max_epoch);

        let (fee, transaction, tx_id) = self
            .resources
            .output_manager_service
            .create_pay_to_self_transaction(
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                None,
                payment_id.clone(),
                MicroMinotari::zero(),
            )
            .await?;

        // Notify that the transaction was successfully resolved.
        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));
        let all_outputs = transaction
            .body
            .outputs()
            .iter()
            .map(|o| o.hash())
            .collect::<Vec<HashOutput>>();
        let lock_height = CompletedTransaction::calculate_lock_height(&transaction);
        let mut final_payment_id = payment_id.clone();
        final_payment_id.set_fee(fee);
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new_with_output_hashes(
                tx_id,
                self.resources.one_sided_tari_address.clone(),
                self.resources.one_sided_tari_address.clone(),
                amount,
                fee,
                transaction,
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Inbound,
                None,
                None,
                final_payment_id,
                vec![],
                all_outputs,
                vec![],
                lock_height,
            )?,
        )
        .await?;

        Ok(tx_id)
    }

    async fn submit_validator_eviction_proof(
        &mut self,
        amount: MicroMinotari,
        eviction_proof: EvictionProof,
        sidechain_deployment_key: Option<PrivateKey>,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        let output_features =
            OutputFeatures::for_validator_node_eviction(eviction_proof, sidechain_deployment_key.as_ref());

        let (fee, transaction, tx_id) = self
            .resources
            .output_manager_service
            .create_pay_to_self_transaction(
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                None,
                payment_id.clone(),
                MicroMinotari::zero(),
            )
            .await?;

        // Notify that the transaction was successfully resolved.
        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));
        let all_outputs = transaction
            .body
            .outputs()
            .iter()
            .map(|o| o.hash())
            .collect::<Vec<HashOutput>>();
        let lock_height = CompletedTransaction::calculate_lock_height(&transaction);
        let mut final_payment_id = payment_id.clone();
        final_payment_id.set_fee(fee);
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new_with_output_hashes(
                tx_id,
                self.resources.one_sided_tari_address.clone(),
                self.resources.one_sided_tari_address.clone(),
                amount,
                fee,
                transaction,
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Inbound,
                None,
                None,
                final_payment_id,
                vec![],
                all_outputs,
                vec![],
                lock_height,
            )?,
        )
        .await?;

        Ok(tx_id)
    }

    async fn register_code_template(
        &mut self,
        fee_per_gram: MicroMinotari,
        template_name: MaxSizeString<32>,
        template_version: u16,
        template_type: TemplateType,
        build_info: BuildInfo,
        binary_sha: FixedHash,
        binary_url: MaxSizeString<255>,
        sidechain_deployment_key: Option<PrivateKey>,
        selection_criteria: UtxoSelectionCriteria,
        payment_id: MemoField,

        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<(TxId, FixedHash), TransactionServiceError> {
        let author_key_id = TariKeyId::CodeTemplateAuthor;
        let author_key = self
            .resources
            .transaction_key_manager_service
            .get_public_key_at_key_id(&author_key_id)?;
        let nonce = self
            .resources
            .transaction_key_manager_service
            .get_random_key(None, None)?;
        let mut template_registration = CodeTemplateRegistration {
            author_public_key: author_key.clone(),
            author_signature: CompressedSignature::default(),
            template_name,
            template_version,
            template_type,
            build_info,
            binary_sha,
            binary_url,
        };

        let signature_message = template_registration.create_signature_message(&nonce.pub_key);

        let author_sig = self
            .resources
            .transaction_key_manager_service
            .sign_with_nonce_and_challenge(&author_key_id, &nonce.key_id, &signature_message)
            .map_err(|e| TransactionServiceError::SidechainSigningError(e.to_string()))?;

        template_registration.author_signature = author_sig;

        let output_features =
            OutputFeatures::for_template_registration(template_registration, sidechain_deployment_key.as_ref());
        let (fee, transaction, tx_id) = self
            .resources
            .output_manager_service
            .create_pay_to_self_transaction(
                0.into(),
                selection_criteria,
                output_features,
                fee_per_gram,
                None,
                payment_id.clone(),
                MicroMinotari::zero(),
            )
            .await?;
        let template_output = transaction
            .body
            .outputs()
            .iter()
            .find(|o| o.features.output_type.is_template_registration())
            .ok_or_else(|| {
                TransactionServiceError::ServiceError(format!(
                    "Transaction {tx_id} did not contain a template registration utxo"
                ))
            })?;
        let template_address = template_output.hash();

        self.submit_transaction_to_self(
            transaction_broadcast_join_handles,
            tx_id,
            transaction,
            fee,
            0.into(),
            payment_id,
        )
        .await?;
        Ok((tx_id, template_address))
    }

    /// Sends a one side payment transaction to a recipient
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    pub async fn send_one_sided_to_stealth_address_transaction(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        self.send_one_sided_or_stealth(
            destination,
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            transaction_broadcast_join_handles,
            payment_id,
        )
        .await
    }

    /// Handle the final clean up after a Send Transaction protocol completes
    fn complete_send_transaction_protocol(
        &mut self,
        join_result: Result<TransactionSendResult, TransactionServiceProtocolError<TxId>>,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) {
        match join_result {
            Ok(val) => {
                if val.transaction_status != LegacyTransactionStatus::Queued {
                    let _sender = self.send_transaction_cancellation_senders.remove(&val.tx_id);
                    let completed_tx = match self.db.get_completed_transaction(val.tx_id) {
                        Ok(v) => v,
                        Err(e) => {
                            error!(
                                target: LOG_TARGET,
                                "Error starting Broadcast Protocol after completed Send Transaction Protocol: {e:?}"
                            );
                            return;
                        },
                    };
                    let _result = self
                        .broadcast_completed_transaction(completed_tx, transaction_broadcast_join_handles)
                        .inspect_err(|resp| {
                            error!(
                                target: LOG_TARGET,
                                "Error starting Broadcast Protocol after completed Send Transaction Protocol: {resp:?}"
                            );
                        });
                } else if val.transaction_status == LegacyTransactionStatus::Queued {
                    trace!(
                        target: LOG_TARGET,
                        "Send Transaction Protocol for TxId: {} not completed successfully, transaction Queued",
                        val.tx_id
                    );
                } else {
                    // dont care
                }
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _result = self.send_transaction_cancellation_senders.remove(&id);
                if let TransactionServiceError::Shutdown = error {
                    return;
                }
                warn!(
                    target: LOG_TARGET,
                    "Error completing Send Transaction Protocol (Id: {id}): {error:?}"
                );
                let _size = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{error:?}"))));
            },
        }
    }

    /// Cancel a pending transaction
    async fn cancel_pending_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let transaction = self.db.get_any_transaction(tx_id)?;

        if let Some(transaction) = transaction {
            if transaction.is_mined() {
                return Err(TransactionServiceError::FailedToCancelTransaction(format!(
                    "Invalid transaction status: {}",
                    transaction.status()
                )));
            }

            if transaction.is_pending() {
                self.resources
                    .output_manager_service
                    .clear_short_term_encumberances()
                    .await
                    .map_err(TransactionServiceError::from)?
            };
        };

        let _unused = self.db.cancel_pending_transaction(tx_id).inspect_err(|e| {
            warn!(
                target: LOG_TARGET,
                "Pending Transaction does not exist and could not be cancelled: {e:?}"
            );
        });

        let _unused = self
            .resources
            .output_manager_service
            .cancel_pending_transaction(tx_id)
            .await
            .inspect_err(|e| {
                warn!(
                    target: LOG_TARGET,
                    "Locked UTXO's could not be unlocked: {e:?}"
                );
            });

        if let Some(cancellation_sender) = self.send_transaction_cancellation_senders.remove(&tx_id) {
            let _result = cancellation_sender.send(());
        }

        if let Some(cancellation_sender) = self.receiver_transaction_cancellation_senders.remove(&tx_id) {
            let _result = cancellation_sender.send(());
        }
        let _public_key = self.finalized_transaction_senders.remove(&tx_id);

        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCancelled(
                tx_id,
                TxCancellationReason::UserCancelled,
            )))
            .inspect_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event because there are no subscribers: {e:?}"
                );
            });

        info!(target: LOG_TARGET, "Pending Transaction (TxId: {tx_id}) cancelled");

        Ok(())
    }

    /// Cancel a completed transaction
    async fn cancel_completed_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let transaction = self.db.get_any_transaction(tx_id)?;

        if let Some(transaction) = transaction &&
            transaction.is_mined()
        {
            return Err(TransactionServiceError::FailedToCancelTransaction(format!(
                "Invalid transaction status: {}",
                transaction.status()
            )));
        }

        let _unused = self
            .db
            .reject_completed_transaction(tx_id, TxCancellationReason::UserCancelled)
            .inspect_err(|e| {
                warn!(
                    target: LOG_TARGET,
                    "Completed Transaction does not exist and could not be cancelled: {e:?}"
                );
            });

        let _unused = self
            .resources
            .output_manager_service
            .cancel_completed_transaction(tx_id)
            .await
            .inspect_err(|e| {
                warn!(
                    target: LOG_TARGET,
                    "Locked UTXO's could not be unlocked: {e:?}"
                );
            });

        if let Some(cancellation_sender) = self.send_transaction_cancellation_senders.remove(&tx_id) {
            let _result = cancellation_sender.send(());
        }

        if let Some(cancellation_sender) = self.receiver_transaction_cancellation_senders.remove(&tx_id) {
            let _result = cancellation_sender.send(());
        }
        let _public_key = self.finalized_transaction_senders.remove(&tx_id);

        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCancelled(
                tx_id,
                TxCancellationReason::UserCancelled,
            )))
            .inspect_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event because there are no subscribers: {e:?}"
                );
            });

        info!(target: LOG_TARGET, "Pending Transaction (TxId: {tx_id}) cancelled");

        Ok(())
    }

    /// Handle the final clean up after a Send Transaction protocol completes
    fn complete_receive_transaction_protocol(
        &mut self,
        join_result: Result<TxId, TransactionServiceProtocolError<TxId>>,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) {
        match join_result {
            Ok(id) => {
                let _public_key = self.finalized_transaction_senders.remove(&id);
                let _result = self.receiver_transaction_cancellation_senders.remove(&id);

                let completed_tx = match self.db.get_completed_transaction(id) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error broadcasting completed transaction TxId: {id} to mempool: {e:?}"
                        );
                        return;
                    },
                };
                let _result = self
                    .broadcast_completed_transaction(completed_tx, transaction_broadcast_join_handles)
                    .inspect_err(|e| {
                        warn!(
                            target: LOG_TARGET,
                            "Error broadcasting completed transaction TxId: {id} to mempool: {e:?}"
                        );
                    });

                trace!(
                    target: LOG_TARGET,
                    "Receive Transaction Protocol for TxId: {id} completed successfully"
                );
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _public_key = self.finalized_transaction_senders.remove(&id);
                let _result = self.receiver_transaction_cancellation_senders.remove(&id);
                match error {
                    TransactionServiceError::RepeatedMessageError => debug!(
                        target: LOG_TARGET,
                        "Receive Transaction Protocol (Id: {id}) aborted as it is a repeated transaction that has \
                         already been processed"
                    ),
                    TransactionServiceError::Shutdown => {
                        return;
                    },
                    _ => warn!(
                        target: LOG_TARGET,
                        "Error completing Receive Transaction Protocol (Id: {id}): {error}"
                    ),
                }

                let _size = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{error:?}"))));
            },
        }
    }

    async fn start_rejected_transaction_revalidation(
        &mut self,
        join_handles: &mut FuturesUnordered<
            JoinHandle<Result<OperationId, TransactionServiceProtocolError<OperationId>>>,
        >,
    ) -> Result<OperationId, TransactionServiceError> {
        self.resources.db.mark_all_rejected_transactions_as_unvalidated()?;
        self.start_transaction_validation_protocol(join_handles).await
    }

    async fn start_transaction_revalidation(
        &mut self,
        join_handles: &mut FuturesUnordered<
            JoinHandle<Result<OperationId, TransactionServiceProtocolError<OperationId>>>,
        >,
    ) -> Result<OperationId, TransactionServiceError> {
        self.resources.db.mark_all_non_coinbases_transactions_as_unvalidated()?;
        self.start_transaction_validation_protocol(join_handles).await
    }

    async fn start_transaction_validation_protocol(
        &mut self,
        join_handles: &mut FuturesUnordered<
            JoinHandle<Result<OperationId, TransactionServiceProtocolError<OperationId>>>,
        >,
    ) -> Result<OperationId, TransactionServiceError> {
        trace!(target: LOG_TARGET, "Starting transaction validation protocol");
        let id = OperationId::new_random();

        let protocol = TransactionValidationProtocol::new(
            id,
            self.resources.db.clone(),
            self.resources.connectivity.clone(),
            self.resources.config.clone(),
            self.event_publisher.clone(),
            self.resources.output_manager_service.clone(),
        );

        let validation_in_progress = self.validation_in_progress.clone();

        let mut utxo_scanner_service_event_stream = self.resources.utxo_scanner_handle.get_event_receiver();

        let join_handle = tokio::spawn(async move {
            let _lock = validation_in_progress.try_lock().map_err(|_| {
                debug!(
                    target: LOG_TARGET,
                    "Transaction Validation Protocol (Id: {id}) spawned while a previous protocol was busy, ignored"
                );
                TransactionServiceProtocolError::new(id, TransactionServiceError::TransactionValidationInProgress)
            })?;
            let mut num_resets = 0;
            'outer: loop {
                let local_run = protocol.clone();
                let exec_fut = local_run.execute();
                tokio::pin!(exec_fut);
                loop {
                    tokio::select! {
                        result = &mut exec_fut => {
                           return result;
                        },
                        event = utxo_scanner_service_event_stream.recv() => {
                            if let Ok(UtxoScannerEvent::Completed{..}) = event {
                                debug!(target: LOG_TARGET, "TXO Validation Protocol (Id: {id}) resetting because base node height changed");
                                num_resets += 1;
                                // We limit the number of resets to avoid infinite loops, if the block validation takes longer than new blocks coming in, we want to at least finish the validation
                                if num_resets < 1{
                                    continue 'outer;
                                }
                            }
                        }
                    }
                }
            }
        });
        join_handles.push(join_handle);

        Ok(id)
    }

    /// Handle the final clean up after a Transaction Validation protocol completes
    fn complete_transaction_validation_protocol(
        &mut self,
        join_result: Result<OperationId, TransactionServiceProtocolError<OperationId>>,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) {
        match join_result {
            Ok(id) => {
                debug!(
                    target: LOG_TARGET,
                    "Transaction Validation Protocol (Id: {id}) completed successfully"
                );
                // Restart broadcast protocols for any transactions that were found to be no longer mined.
                let _ = self
                    .restart_broadcast_protocols(transaction_broadcast_join_handles)
                    .map_err(|e| warn!(target: LOG_TARGET, "Error restarting broadcast protocols: {e}"));
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                if let TransactionServiceError::Shutdown = error {
                    return;
                }
                warn!(
                    target: LOG_TARGET,
                    "Error completing Transaction Validation Protocol (id: {id}): {error:?}"
                );
                let reason = match error {
                    TransactionServiceError::TransactionValidationInProgress => 1,
                    TransactionServiceError::ProtobufConversionError(_) |
                    TransactionServiceError::RpcError(_) |
                    TransactionServiceError::InvalidMessageError(_) |
                    TransactionServiceError::BaseNodeChanged { .. } => 3,
                    _ => 2,
                };
                let _size = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::TransactionValidationFailed(id, reason)));
            },
        }
    }

    fn restart_broadcast_protocols(
        &mut self,
        broadcast_join_handles: &mut FuturesUnordered<JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>>,
    ) -> Result<(), TransactionServiceError> {
        trace!(target: LOG_TARGET, "Restarting transaction broadcast protocols");
        self.broadcast_completed_and_broadcast_transactions(broadcast_join_handles)
            .inspect_err(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Error broadcasting all valid and not cancelled Completed Transactions with status 'Completed' \
                     and 'Broadcast': {resp:?}"
                );
            })?;

        Ok(())
    }

    /// Start to protocol to Broadcast the specified Completed Transaction to the Base Node.
    fn broadcast_completed_transaction(
        &mut self,
        completed_tx: CompletedTransaction,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>>,
    ) -> Result<(), TransactionServiceError> {
        let tx_id = completed_tx.tx_id;
        if !(completed_tx.status == LegacyTransactionStatus::Completed ||
            completed_tx.status == LegacyTransactionStatus::Broadcast ||
            completed_tx.status == LegacyTransactionStatus::MinedUnconfirmed) ||
            completed_tx.transaction.body.kernels().is_empty()
        {
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }
        // Check if the protocol has already been started
        if self.active_transaction_broadcast_protocols.insert(tx_id) {
            let protocol = TransactionBroadcastProtocol::new(
                tx_id,
                self.resources.clone(),
                self.timeout_update_watch.get_receiver(),
            );
            let join_handle = tokio::spawn(protocol.execute());
            join_handles.push(join_handle);
        } else {
            trace!(
                target: LOG_TARGET,
                "Transaction Broadcast Protocol (TxId: {tx_id}) already started"
            );
        }

        Ok(())
    }

    /// Broadcast all valid and not cancelled completed transactions with status 'Completed' and 'Broadcast' to the base
    /// node.
    fn broadcast_completed_and_broadcast_transactions(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>>,
    ) -> Result<(), TransactionServiceError> {
        trace!(
            target: LOG_TARGET,
            "Attempting to Broadcast all valid and not cancelled Completed Transactions with status 'Completed' and \
             'Broadcast'"
        );
        let txn_list = self.db.get_transactions_to_be_broadcast()?;
        for completed_txn in txn_list {
            self.broadcast_completed_transaction(completed_txn, join_handles)?;
        }

        Ok(())
    }

    /// Handle the final clean up after a Transaction Broadcast protocol completes
    fn complete_transaction_broadcast_protocol(
        &mut self,
        join_result: Result<TxId, TransactionServiceProtocolError<TxId>>,
    ) {
        match join_result {
            Ok(id) => {
                debug!(
                    target: LOG_TARGET,
                    "Transaction Broadcast Protocol for TxId: {id} completed successfully"
                );
                let _ = self.active_transaction_broadcast_protocols.remove(&id);
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _ = self.active_transaction_broadcast_protocols.remove(&id);

                if let TransactionServiceError::Shutdown = error {
                    return;
                }
                warn!(
                    target: LOG_TARGET,
                    "Error completing Transaction Broadcast Protocol (Id: {id}): {error:?}"
                );
                let _size = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{error:?}"))));
            },
        }
    }

    /// Add a completed transaction to the Transaction Manager to record directly importing a spendable UTXO.
    pub async fn add_utxo_import_transaction_with_status(
        &mut self,
        value: MicroMinotari,
        source_address: TariAddress,
        import_status: LegacyImportStatus,
        current_height: Option<u64>,
        mined_timestamp: Option<DateTime<Utc>>,
        scanned_output: TransactionOutput,
        payment_id: MemoField,
        optional_tx_id: Option<TxId>,
        lock_height: u64,
    ) -> Result<TxId, TransactionServiceError> {
        // Faux transactions for scanned change outputs must correspond to the original transaction
        let (direction, amount, destination_address) =
            if let Some((recipient_address, amount, tx_type, _)) = payment_id.get_transaction_info_details() {
                (
                    match tx_type {
                        TxType::PaymentToOther |
                        TxType::Burn |
                        TxType::CodeTemplateRegistration |
                        TxType::ValidatorNodeRegistration |
                        TxType::CoinSplit |
                        TxType::PaymentToSelf => TransactionDirection::Outbound,
                        TxType::CoinJoin |
                        TxType::ClaimAtomicSwap |
                        TxType::HtlcAtomicSwapRefund |
                        TxType::ImportedUtxoNoneRewindable |
                        TxType::Coinbase => TransactionDirection::Inbound,
                    },
                    amount,
                    if tx_type == TxType::Burn {
                        TariAddress::default()
                    } else {
                        recipient_address.clone()
                    },
                )
            } else {
                (
                    TransactionDirection::Inbound,
                    value,
                    self.resources.one_sided_tari_address.clone(),
                )
            };

        let tx_id = match optional_tx_id {
            Some(id) => id,
            None => TxId::new_deterministic(
                self.resources
                    .transaction_key_manager_service
                    .get_view_key()
                    .pub_key
                    .as_bytes(),
                &scanned_output.hash(),
            ),
        };
        self.db.add_utxo_import_transaction_with_status(
            tx_id,
            amount,
            source_address,
            destination_address,
            LegacyTransactionStatus::try_from(import_status.clone())?,
            current_height,
            mined_timestamp,
            scanned_output,
            payment_id,
            direction,
            lock_height,
        )?;
        let transaction_event = match import_status {
            LegacyImportStatus::Broadcast => TransactionEvent::TransactionBroadcast(tx_id),
            LegacyImportStatus::Imported => TransactionEvent::DetectedTransactionUnconfirmed {
                tx_id,
                num_confirmations: 0,
                is_valid: true,
            },
            LegacyImportStatus::OneSidedUnconfirmed | LegacyImportStatus::CoinbaseUnconfirmed => {
                TransactionEvent::DetectedTransactionUnconfirmed {
                    tx_id,
                    num_confirmations: 0,
                    is_valid: true,
                }
            },
            LegacyImportStatus::OneSidedConfirmed | LegacyImportStatus::CoinbaseConfirmed => {
                TransactionEvent::DetectedTransactionConfirmed { tx_id, is_valid: true }
            },
        };
        let _size = self.event_publisher.send(Arc::new(transaction_event)).inspect_err(|e| {
            trace!(
                target: LOG_TARGET,
                "Error sending event, usually because there are no subscribers: {e:?}"
            );
        });
        Ok(tx_id)
    }

    /// Submit a completed transaction to the Transaction Manager
    async fn submit_transaction(
        &mut self,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionServiceError> {
        let tx_id = completed_transaction.tx_id;
        trace!(target: LOG_TARGET, "Submit transaction ({tx_id}) to db.");
        self.db
            .insert_completed_transaction(tx_id, completed_transaction.clone())?;
        trace!(
            target: LOG_TARGET,
            "Launch the transaction broadcast protocol for submitted transaction ({tx_id})."
        );
        if let Err(e) = check_transaction_size(&completed_transaction.transaction, tx_id) {
            self.cancel_transaction(tx_id, TxCancellationReason::Oversized).await;
            return Err(e.into());
        }
        self.complete_send_transaction_protocol(
            Ok(TransactionSendResult {
                tx_id,
                transaction_status: LegacyTransactionStatus::Completed,
            }),
            transaction_broadcast_join_handles,
        );
        Ok(())
    }

    /// Replace a pending outbound transaction with a new one with higher fee
    ///
    /// # Arguments
    /// * `tx_id` - The transaction ID of the pending outbound transaction to replace
    /// * `fee_increase` - Fee increase for replaced transaction. It cannot be zero
    /// * `transaction_broadcast_join_handle` - Transaction broadcast join handle
    ///
    /// # Returns
    /// The new transaction ID or an error
    pub async fn replace_by_fee(
        &mut self,
        tx_id: TxId,
        fee_increase: MicroMinotari,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        if fee_increase == MicroMinotari::zero() {
            return Err(TransactionServiceError::ZeroFeeIncrease);
        }

        let original_transaction = self.resources.db.get_transaction_to_be_broadcast(tx_id).map_err(|_| {
            TransactionServiceError::TransactionStorageError(TransactionStorageError::ValueNotFound(
                DbKey::CompletedTransaction(tx_id),
            ))
        })?;

        if original_transaction.status.is_mined() {
            return Err(TransactionServiceError::TransactionAlreadyMined(tx_id.to_string()));
        }

        let destination = original_transaction.destination_address.clone();
        let original_amount = original_transaction.amount;
        let payment_id = original_transaction.payment_id.clone();

        let original_inputs = original_transaction.get_input_commitments_from_completed_transaction()?;
        let fee = original_transaction.fee + fee_increase;

        // Calculate transaction weight and fee_per_gram from total fee using original transaction
        let num_inputs = original_inputs.len();
        let num_outputs = original_transaction.transaction.body().outputs().len();
        let tip_height = self.db.get_last_scanned_height()?.unwrap_or(0);
        let (weight_in_grams, fee_per_gram) = original_transaction.calculate_fee_per_gram_from_total_fee(
            fee,
            self.resources.consensus_manager.consensus_constants(tip_height),
            num_inputs,
            num_outputs,
        )?;

        debug!(
            target: LOG_TARGET,
            "Replace-by-fee: Creating replacement for transaction {tx_id} with total fee {fee} (weight: {weight_in_grams} grams, fee_per_gram: {fee_per_gram})"
        );

        // Cancel the original transaction to free its inputs in the output manager before creating the replacement
        self.cancel_transaction(tx_id, TxCancellationReason::UserCancelled)
            .await;

        let new_tx_id = self
            .send_one_sided_transaction(
                destination,
                original_amount,
                UtxoSelectionCriteria::must_include(original_inputs),
                OutputFeatures::default(),
                fee_per_gram,
                payment_id,
                transaction_broadcast_join_handles,
            )
            .await?;

        info!(
            target: LOG_TARGET,
            "Replace-by-fee: Created new transaction {new_tx_id} to replace {tx_id} with total fee: {fee}, fee_per_gram: {fee_per_gram}"
        );

        Ok(new_tx_id)
    }

    /// Spend all outputs from an unmined transaction to a given destination address    ///
    ///
    /// # Arguments
    /// * `tx_id` - The transaction ID of the original transaction whose outputs will be spent
    /// * `destination` - The destination address where the remaining amount (after fee) will be sent
    /// * `fee` - The total fee amount to be paid
    /// * `transaction_broadcast_join_handles` - Join handles for transaction broadcast protocols
    ///
    /// # Returns
    /// Returns the new transaction ID of the fee payment transaction, or an error if the operation fails.
    ///
    /// # Errors
    /// * `TransactionStorageError` - If the original transaction cannot be found
    /// * `TransactionAlreadyMined` - If the original transaction has already been mined
    /// * `KeyManagerServiceError` - If there are issues accessing cryptographic keys
    #[allow(clippy::too_many_lines)]
    async fn user_pay_for_fee(
        &mut self,
        tx_id: TxId,
        destination: TariAddress,
        fee: MicroMinotari,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        let original_transaction = self.resources.db.get_transaction_to_be_broadcast(tx_id).map_err(|_| {
            TransactionServiceError::TransactionStorageError(TransactionStorageError::ValueNotFound(
                DbKey::CompletedTransaction(tx_id),
            ))
        })?;
        if original_transaction.status.is_mined() {
            return Err(TransactionServiceError::TransactionAlreadyMined(tx_id.to_string()));
        }

        let all_outputs = original_transaction
            .transaction
            .body
            .outputs()
            .iter()
            .collect::<Vec<_>>();
        let mut spendable_outputs = Vec::new();
        let mut total_amount = MicroMinotari::zero();
        let view_key = self.resources.transaction_key_manager_service.get_private_view_key();

        // only those outputs that can be decrypted are spendable and can be used as inputs
        for output in all_outputs {
            match EncryptedData::decrypt_data(&view_key, output.commitment(), output.encrypted_data()) {
                Ok((amount, _, _)) => {
                    total_amount += amount;
                    spendable_outputs.push(output.commitment().clone());
                },
                Err(_) => {
                    debug!(target: LOG_TARGET, "Output {:?} not decryptable; skipping.", output.commitment());
                },
            }
        }

        debug!(
            target: LOG_TARGET,
            "user-pay-for-fee: Filtered {} outputs from {} total outputs for transaction {}, total amount: {}",
            spendable_outputs.len(),
            original_transaction.transaction.body.outputs().len(),
            tx_id,
            total_amount
        );

        let num_inputs = spendable_outputs.len();
        let num_outputs = 2; // destination + change
        let tip_height = self.db.get_last_scanned_height()?.unwrap_or(0);
        let (weight_in_grams, fee_per_gram) = original_transaction.calculate_fee_per_gram_from_total_fee(
            fee,
            self.resources.consensus_manager.consensus_constants(tip_height),
            num_inputs,
            num_outputs,
        )?;

        debug!(
            target: LOG_TARGET,
            "user-pay-for-fee: Fee calculation - target_fee: {}, weight: {} grams, fee_per_gram: {} (calculated as {:.3} rounded)",
            fee,
            weight_in_grams,
            fee_per_gram,
            if weight_in_grams > 0 { fee.0 as f64 / weight_in_grams as f64 } else { 0.0 }
        );

        let new_tx_id = self
            .send_one_sided_transaction(
                destination,
                total_amount,
                UtxoSelectionCriteria::must_include(spendable_outputs),
                OutputFeatures::default(),
                fee_per_gram,
                original_transaction.payment_id,
                transaction_broadcast_join_handles,
            )
            .await?;

        info!(
            target: LOG_TARGET,
            "user-pay-for-fee: Created new transaction {new_tx_id} to spend outputs transaction with id: {tx_id}, weight: {weight_in_grams} grams"
        );

        Ok(new_tx_id)
    }

    async fn cancel_transaction(&mut self, tx_id: TxId, reason: TxCancellationReason) {
        if let Err(e) = self
            .resources
            .output_manager_service
            .cancel_pending_transaction(tx_id)
            .await
        {
            warn!(
                target: LOG_TARGET,
                "Failed to Cancel outputs for TxId: {tx_id} after failed sending attempt with error {e:?}"
            );
        }
        if let Err(e) = self.resources.db.reject_completed_transaction(tx_id, reason) {
            warn!(
                target: LOG_TARGET,
                "Failed to Cancel TxId: {tx_id} after failed sending attempt with error {e:?}"
            );
        }
    }

    /// Submit a completed coin split transaction to the Transaction Manager. This is different from
    /// `submit_transaction` in that it will expose less information about the completed transaction.
    pub async fn submit_transaction_to_self(
        &mut self,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        tx_id: TxId,
        tx: Transaction,
        fee: MicroMinotari,
        amount: MicroMinotari,
        payment_id: MemoField,
    ) -> Result<(), TransactionServiceError> {
        let all_outputs = tx.body.outputs().iter().map(|o| o.hash()).collect::<Vec<HashOutput>>();
        let lock_height = CompletedTransaction::calculate_lock_height(&tx);
        let mut final_payment_id = payment_id.clone();
        final_payment_id.set_fee(fee);
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new_with_output_hashes(
                tx_id,
                self.resources.one_sided_tari_address.clone(),
                self.resources.one_sided_tari_address.clone(),
                amount,
                fee,
                tx,
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Inbound,
                None,
                None,
                final_payment_id,
                vec![],
                all_outputs,
                vec![],
                lock_height,
            )?,
        )
        .await?;
        Ok(())
    }

    fn verify_send(
        &self,
        address: &TariAddress,
        sending_method: TariAddressFeatures,
    ) -> Result<(), TransactionServiceError> {
        if address.network() != self.resources.one_sided_tari_address.network() {
            return Err(TransactionServiceError::InvalidNetwork);
        }
        if !address.features().contains(sending_method) {
            return Err(TransactionServiceError::InvalidAddress(format!(
                "Address does not support feature {sending_method} "
            )));
        }
        if sending_method.contains(TariAddressFeatures::create_interactive_only()) &&
            matches!(*self.resources.wallet_type, LegacyWalletType::Ledger(_))
        {
            return Err(TransactionServiceError::NotSupported(
                "Interactive transactions are not supported on Ledger wallets".to_string(),
            ));
        }
        Ok(())
    }

    /// Get payment details by PayRef
    fn get_payment_by_reference(&self, payref: FixedHash) -> Result<Option<PaymentDetails>, TransactionServiceError> {
        let current_height = self.db.get_last_scanned_height()?.unwrap_or(0);
        let txn = match self.db.get_transaction_with_payref(&payref)? {
            Some(txn) => txn,
            None => return Ok(None), // No transaction found with the given PayRef
        };

        let block_hash = match txn.mined_in_block {
            Some(hash) => hash,
            None => return Ok(None), // This should not happen, but just in case
        };

        let mined_height = match txn.mined_height {
            Some(height) => height,
            None => return Ok(None), // This should not happen, but just in case
        };

        let payment_id_bytes = txn.payment_id.payment_id_as_bytes();

        // Check if PayRef matches any sent output by generating proper PayRef
        for output_hash in &txn.sent_output_hashes {
            let generated_payref = generate_payment_reference(&block_hash, output_hash);
            if generated_payref == payref {
                return Ok(Some(PaymentDetails {
                    tx_id: txn.tx_id,
                    payment_reference: payref,
                    amount: txn.amount,
                    direction: txn.direction,
                    block_height: mined_height,
                    confirmations: current_height.saturating_sub(mined_height),
                    timestamp: Some(txn.timestamp),
                    payment_id: Some(payment_id_bytes),
                }));
            }
        }

        // Check if PayRef matches any received output by generating proper PayRef
        for output_hash in &txn.received_output_hashes {
            let generated_payref = generate_payment_reference(&block_hash, output_hash);
            if generated_payref == payref {
                return Ok(Some(PaymentDetails {
                    tx_id: txn.tx_id,
                    payment_reference: payref,
                    amount: txn.amount,
                    direction: txn.direction,
                    block_height: mined_height,
                    confirmations: current_height.saturating_sub(mined_height),
                    timestamp: Some(txn.timestamp),
                    payment_id: Some(payment_id_bytes),
                }));
            }
        }

        // Check if PayRef matches any change output by generating proper PayRef
        for output_hash in &txn.change_output_hashes {
            let generated_payref = generate_payment_reference(&block_hash, output_hash);
            if generated_payref == payref {
                return Ok(Some(PaymentDetails {
                    tx_id: txn.tx_id,
                    payment_reference: payref,
                    amount: txn.amount,
                    direction: txn.direction,
                    block_height: mined_height,
                    confirmations: current_height.saturating_sub(mined_height),
                    timestamp: Some(txn.timestamp),
                    payment_id: Some(payment_id_bytes),
                }));
            }
        }

        Ok(None)
    }

    fn get_transaction_with_payref(
        &self,
        payref: FixedHash,
    ) -> Result<Option<CompletedTransaction>, TransactionServiceError> {
        let transactions = self.db.get_transaction_with_payref(&payref)?;

        Ok(transactions)
    }

    async fn submit_signed_one_sided_transaction(
        &mut self,
        request: SignedOneSidedTransactionResult,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<Vec<TxId>, TransactionServiceError> {
        let old_tx_id = request.request.tx_id;
        let new_tx_id = request.signed_transaction.tx_id;
        for recipient in &request.request.info.recipients {
            self.verify_send(&recipient.address, TariAddressFeatures::create_one_sided_only())?;
        }
        let payment_id = request.request.info.payment_id;
        // Use original keys generated in this wallet (they correspond to keys with the same values)
        let change = request.signed_transaction.change_output.map(|v| vec![v.clone()]);

        let _result = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(new_tx_id)));

        let fee = request.signed_transaction.transaction.body.get_total_fee()?;

        self.resources
            .output_manager_service
            .confirm_pending_transaction(old_tx_id, Some(new_tx_id), change)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(new_tx_id, e.into()))?;

        let mut tx_ids = Vec::new();
        let mut completed_txs = Vec::new();

        for (i, recipient) in request.request.info.recipients.iter().enumerate() {
            let tx_id = if i == 0 {
                new_tx_id
            } else {
                TxId::new_deterministic(
                    self.resources
                        .transaction_key_manager_service
                        .get_private_view_key()
                        .as_bytes(),
                    request
                        .signed_transaction
                        .sent_hashes
                        .get(i)
                        .ok_or(TransactionServiceError::Other(
                            "sent_outputs index out of bounds".to_string(),
                        ))?,
                )
            };
            tx_ids.push(tx_id);
            let sent_hash =
                request
                    .signed_transaction
                    .sent_hashes
                    .get(i)
                    .copied()
                    .ok_or(TransactionServiceError::Other(
                        "sent_output_hashes index out of bounds".to_string(),
                    ))?;
            let mut final_payment_id = payment_id.clone();
            final_payment_id.set_fee(fee);
            let completed_tx = CompletedTransaction::new_with_output_hashes(
                tx_id,
                self.resources.one_sided_tari_address.clone(),
                recipient.address.clone(),
                recipient.amount,
                fee,
                request.signed_transaction.transaction.clone(),
                LegacyTransactionStatus::Completed,
                Utc::now(),
                TransactionDirection::Outbound,
                None,
                None,
                final_payment_id,
                vec![sent_hash],
                vec![],
                request.signed_transaction.change_hashes.clone(),
                0,
            )?;
            completed_txs.push(completed_tx);
        }
        let first_completed_tx = completed_txs.remove(0);
        self.submit_transaction(transaction_broadcast_join_handles, first_completed_tx)
            .await?;
        for completed_tx in completed_txs {
            self.db.insert_completed_transaction(completed_tx.tx_id, completed_tx)?;
        }

        Ok(tx_ids)
    }
}

/// This struct is a collection of the common resources that a protocol in the service requires.
#[derive(Clone)]
pub struct TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    pub db: TransactionDatabase<TBackend>,
    pub output_manager_service: OutputManagerHandle<TKeyManagerInterface>,
    pub transaction_key_manager_service: TKeyManagerInterface,
    pub connectivity: TWalletConnectivity,
    pub event_publisher: TransactionEventSender,
    pub one_sided_tari_address: TariAddress,
    pub node_identity: Arc<NodeIdentity>,
    pub consensus_manager: ConsensusManager,
    pub factories: CryptoFactories,
    pub config: TransactionServiceConfig,
    pub shutdown_signal: ShutdownSignal,
    pub wallet_type: Arc<LegacyWalletType>,
    pub utxo_scanner_handle: UtxoScannerHandle,
    pub network: Network,
}

/// Contains the generated TxId and TransactionStatus transaction send result
#[derive(Debug)]
pub struct TransactionSendResult {
    pub tx_id: TxId,
    pub transaction_status: LegacyTransactionStatus,
}
