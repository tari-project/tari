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
    convert::TryInto,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{NaiveDateTime, Utc};
use digest::Digest;
use futures::{pin_mut, stream::FuturesUnordered, Stream, StreamExt};
use log::*;
use rand::rngs::OsRng;
use sha2::Sha256;
use tari_common::configuration::Network;
use tari_common_types::{
    burnt_proof::BurntProof,
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::{ImportStatus, TransactionDirection, TransactionStatus, TxId},
    types::{CommitmentFactory, FixedHash, HashOutput, PrivateKey, PublicKey, Signature},
};
use tari_comms::{types::CommsPublicKey, NodeIdentity};
use tari_comms_dht::outbound::OutboundMessageRequester;
use tari_core::{
    consensus::ConsensusManager,
    covenants::Covenant,
    mempool::FeePerGramStat,
    one_sided::{
        public_key_to_output_encryption_key,
        shared_secret_to_output_encryption_key,
        shared_secret_to_output_spending_key,
        stealth_address_script_spending_key,
    },
    proto::{base_node as base_node_proto, base_node::FetchMatchingUtxos},
    transactions::{
        key_manager::{TariKeyId, TransactionKeyManagerBranch, TransactionKeyManagerInterface},
        tari_amount::MicroMinotari,
        transaction_components::{
            encrypted_data::PaymentId,
            CodeTemplateRegistration,
            KernelFeatures,
            OutputFeatures,
            RangeProofType,
            Transaction,
            TransactionOutput,
            WalletOutputBuilder,
        },
        transaction_protocol::{
            proto::protocol as proto,
            recipient::RecipientSignedMessage,
            sender::TransactionSenderMessage,
            TransactionMetadata,
        },
        CryptoFactories,
        ReceiverTransactionProtocol,
    },
};
use tari_crypto::{
    keys::{PublicKey as PKtrait, SecretKey},
    ristretto::pedersen::PedersenCommitment,
    tari_utilities::ByteArray,
};
use tari_key_manager::key_manager_service::KeyId;
use tari_p2p::domain_message::DomainMessage;
use tari_script::{
    inputs,
    push_pubkey_script,
    script,
    slice_to_boxed_message,
    CheckSigSchnorrSignature,
    ExecutionStack,
    ScriptContext,
    TariScript,
};
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_shutdown::ShutdownSignal;
use tokio::{
    sync::{mpsc, mpsc::Sender, oneshot, Mutex},
    task::JoinHandle,
};

use crate::{
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::{
        handle::{OutputManagerEvent, OutputManagerHandle},
        storage::models::SpendingPriority,
        UtxoSelectionCriteria,
    },
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        config::TransactionServiceConfig,
        error::{TransactionServiceError, TransactionServiceProtocolError},
        handle::{
            FeePerGramStatsResponse,
            TransactionEvent,
            TransactionEventSender,
            TransactionServiceRequest,
            TransactionServiceResponse,
        },
        protocols::{
            check_transaction_size,
            transaction_broadcast_protocol::TransactionBroadcastProtocol,
            transaction_receive_protocol::{TransactionReceiveProtocol, TransactionReceiveProtocolStage},
            transaction_send_protocol::{TransactionSendProtocol, TransactionSendProtocolStage},
            transaction_validation_protocol::TransactionValidationProtocol,
        },
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::{
                CompletedTransaction,
                TxCancellationReason,
                WalletTransaction::{Completed, PendingInbound, PendingOutbound},
            },
        },
        tasks::{
            check_faux_transaction_status::check_detected_transactions,
            send_finalized_transaction::send_finalized_transaction_message,
            send_transaction_cancelled::send_transaction_cancelled_message,
            send_transaction_reply::send_transaction_reply,
        },
        utc::utc_duration_since,
    },
    util::{wallet_identity::WalletIdentity, watch::Watch},
    utxo_scanner_service::RECOVERY_KEY,
    OperationId,
};

const LOG_TARGET: &str = "wallet::transaction_service::service";

/// TransactionService allows for the management of multiple inbound and outbound transaction protocols
/// which are uniquely identified by a tx_id. The TransactionService generates and accepts the various protocol
/// messages and applies them to the appropriate protocol instances based on the tx_id.
/// The TransactionService allows for the sending of transactions to single receivers, when the appropriate recipient
/// response is handled the transaction is completed and moved to the completed_transaction buffer.
/// The TransactionService will accept inbound transactions and generate a reply. Received transactions will remain
/// in the pending_inbound_transactions buffer.
/// # Fields
/// `pending_outbound_transactions` - List of transaction protocols sent by this client and waiting response from the
/// recipient
/// `pending_inbound_transactions` - List of transaction protocols that have been received and responded to.
/// `completed_transaction` - List of sent transactions that have been responded to and are completed.

pub struct TransactionService<
    TTxStream,
    TTxReplyStream,
    TTxFinalizedStream,
    BNResponseStream,
    TBackend,
    TTxCancelledStream,
    TWalletBackend,
    TWalletConnectivity,
    TKeyManagerInterface,
> {
    config: TransactionServiceConfig,
    db: TransactionDatabase<TBackend>,
    transaction_stream: Option<TTxStream>,
    transaction_reply_stream: Option<TTxReplyStream>,
    transaction_finalized_stream: Option<TTxFinalizedStream>,
    base_node_response_stream: Option<BNResponseStream>,
    transaction_cancelled_stream: Option<TTxCancelledStream>,
    request_stream: Option<
        reply_channel::Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    >,
    event_publisher: TransactionEventSender,
    resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
    pending_transaction_reply_senders: HashMap<TxId, Sender<(CommsPublicKey, RecipientSignedMessage)>>,
    base_node_response_senders: HashMap<TxId, (TxId, Sender<base_node_proto::BaseNodeServiceResponse>)>,
    send_transaction_cancellation_senders: HashMap<TxId, oneshot::Sender<()>>,
    finalized_transaction_senders: HashMap<TxId, Sender<(TariAddress, TxId, Transaction)>>,
    receiver_transaction_cancellation_senders: HashMap<TxId, oneshot::Sender<()>>,
    active_transaction_broadcast_protocols: HashSet<TxId>,
    timeout_update_watch: Watch<Duration>,
    wallet_db: WalletDatabase<TWalletBackend>,
    base_node_service: BaseNodeServiceHandle,
    last_seen_tip_height: Option<u64>,
    validation_in_progress: Arc<Mutex<()>>,
    consensus_manager: ConsensusManager,
}

impl<
        TTxStream,
        TTxReplyStream,
        TTxFinalizedStream,
        BNResponseStream,
        TBackend,
        TTxCancelledStream,
        TWalletBackend,
        TWalletConnectivity,
        TKeyManagerInterface,
    >
    TransactionService<
        TTxStream,
        TTxReplyStream,
        TTxFinalizedStream,
        BNResponseStream,
        TBackend,
        TTxCancelledStream,
        TWalletBackend,
        TWalletConnectivity,
        TKeyManagerInterface,
    >
where
    TTxStream: Stream<Item = DomainMessage<Result<proto::TransactionSenderMessage, prost::DecodeError>>>,
    TTxReplyStream: Stream<Item = DomainMessage<Result<proto::RecipientSignedMessage, prost::DecodeError>>>,
    TTxFinalizedStream: Stream<Item = DomainMessage<Result<proto::TransactionFinalizedMessage, prost::DecodeError>>>,
    BNResponseStream:
        Stream<Item = DomainMessage<Result<base_node_proto::BaseNodeServiceResponse, prost::DecodeError>>>,
    TTxCancelledStream: Stream<Item = DomainMessage<Result<proto::TransactionCancelledMessage, prost::DecodeError>>>,
    TBackend: TransactionBackend + 'static,
    TWalletBackend: WalletBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    pub async fn new(
        config: TransactionServiceConfig,
        db: TransactionDatabase<TBackend>,
        wallet_db: WalletDatabase<TWalletBackend>,
        request_stream: Receiver<
            TransactionServiceRequest,
            Result<TransactionServiceResponse, TransactionServiceError>,
        >,
        transaction_stream: TTxStream,
        transaction_reply_stream: TTxReplyStream,
        transaction_finalized_stream: TTxFinalizedStream,
        base_node_response_stream: BNResponseStream,
        transaction_cancelled_stream: TTxCancelledStream,
        output_manager_service: OutputManagerHandle,
        core_key_manager_service: TKeyManagerInterface,
        outbound_message_service: OutboundMessageRequester,
        connectivity: TWalletConnectivity,
        event_publisher: TransactionEventSender,
        node_identity: Arc<NodeIdentity>,
        network: Network,
        consensus_manager: ConsensusManager,
        factories: CryptoFactories,
        shutdown_signal: ShutdownSignal,
        base_node_service: BaseNodeServiceHandle,
    ) -> Result<Self, TransactionServiceError> {
        // Collect the resources that all protocols will need so that they can be neatly cloned as the protocols are
        // spawned.
        let view_key = core_key_manager_service.get_view_key_id().await?;
        let view_key = core_key_manager_service.get_public_key_at_key_id(&view_key).await?;
        let tari_address =
            TariAddress::new_dual_address_with_default_features(view_key, node_identity.public_key().clone(), network);
        let wallet_identity = WalletIdentity::new(node_identity.clone(), tari_address);
        let resources = TransactionServiceResources {
            db: db.clone(),
            output_manager_service,
            transaction_key_manager_service: core_key_manager_service,
            outbound_message_service,
            connectivity,
            event_publisher: event_publisher.clone(),
            wallet_identity,
            factories,
            config: config.clone(),
            shutdown_signal,
            consensus_manager: consensus_manager.clone(),
        };
        let power_mode = PowerMode::default();
        let timeout = match power_mode {
            PowerMode::Low => config.low_power_polling_timeout,
            PowerMode::Normal => config.broadcast_monitoring_timeout,
        };
        let timeout_update_watch = Watch::new(timeout);

        Ok(Self {
            config,
            db,
            transaction_stream: Some(transaction_stream),
            transaction_reply_stream: Some(transaction_reply_stream),
            transaction_finalized_stream: Some(transaction_finalized_stream),
            base_node_response_stream: Some(base_node_response_stream),
            transaction_cancelled_stream: Some(transaction_cancelled_stream),
            request_stream: Some(request_stream),
            event_publisher,
            resources,
            pending_transaction_reply_senders: HashMap::new(),
            base_node_response_senders: HashMap::new(),
            send_transaction_cancellation_senders: HashMap::new(),
            finalized_transaction_senders: HashMap::new(),
            receiver_transaction_cancellation_senders: HashMap::new(),
            active_transaction_broadcast_protocols: HashSet::new(),
            timeout_update_watch,
            base_node_service,
            wallet_db,
            last_seen_tip_height: None,
            validation_in_progress: Arc::new(Mutex::new(())),
            consensus_manager,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub async fn start(mut self) -> Result<(), TransactionServiceError> {
        // we need to ensure the wallet identity secret key is stored in the key manager
        let _key_id = self
            .resources
            .transaction_key_manager_service
            .import_key(self.resources.wallet_identity.node_identity.secret_key().clone())
            .await?;

        let request_stream = self
            .request_stream
            .take()
            .expect("Transaction Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);
        let transaction_stream = self
            .transaction_stream
            .take()
            .expect("Transaction Service initialized without transaction_stream")
            .fuse();
        pin_mut!(transaction_stream);
        let transaction_reply_stream = self
            .transaction_reply_stream
            .take()
            .expect("Transaction Service initialized without transaction_reply_stream")
            .fuse();
        pin_mut!(transaction_reply_stream);
        let transaction_finalized_stream = self
            .transaction_finalized_stream
            .take()
            .expect("Transaction Service initialized without transaction_finalized_stream")
            .fuse();
        pin_mut!(transaction_finalized_stream);
        let base_node_response_stream = self
            .base_node_response_stream
            .take()
            .expect("Transaction Service initialized without base_node_response_stream")
            .fuse();
        pin_mut!(base_node_response_stream);
        let transaction_cancelled_stream = self
            .transaction_cancelled_stream
            .take()
            .expect("Transaction Service initialized without transaction_cancelled_stream")
            .fuse();
        pin_mut!(transaction_cancelled_stream);

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
        let mut output_manager_event_stream = self.resources.output_manager_service.get_event_stream();

        debug!(target: LOG_TARGET, "Transaction Service started");
        loop {
            tokio::select! {
                event = output_manager_event_stream.recv() => {
                    match event {
                        Ok(msg) => self.handle_output_manager_service_event(msg).await,
                        Err(e) => debug!(target: LOG_TARGET, "Lagging read on base node event broadcast channel: {}", e),
                    };
                },
                // Base Node Monitoring Service event
                event = base_node_service_event_stream.recv() => {
                    match event {
                        Ok(msg) => self.handle_base_node_service_event(msg, &mut transaction_validation_protocol_handles).await,
                        Err(e) => debug!(target: LOG_TARGET, "Lagging read on base node event broadcast channel: {}", e),
                    };
                },
                //Incoming request
                Some(request_context) = request_stream.next() => {
                    let start = Instant::now();
                    let (request, reply_tx) = request_context.split();
                    let event = format!("Handling Service API Request ({})", request);
                    trace!(target: LOG_TARGET, "{}", event);
                    let _result = self.handle_request(request,
                        &mut send_transaction_protocol_handles,
                        &mut receive_transaction_protocol_handles,
                        &mut transaction_broadcast_protocol_handles,
                        &mut transaction_validation_protocol_handles,
                        reply_tx,
                    ).await.map_err(|e| {
                        warn!(target: LOG_TARGET, "Error handling request: {:?}", e);
                        e
                    });
                    trace!(target: LOG_TARGET,
                        "{}, processed in {}ms",
                        event,
                        start.elapsed().as_millis()
                    );
                },
                // Incoming Transaction messages from the Comms layer
                Some(msg) = transaction_stream.next() => {
                    let start = Instant::now();
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Transaction Message, Trace: {}", msg.dht_header.message_tag);

                    let result  = self.accept_transaction(origin_public_key, inner_msg,
                        msg.dht_header.message_tag.as_value(), &mut receive_transaction_protocol_handles);

                    match result {
                        Err(TransactionServiceError::RepeatedMessageError) => {
                            trace!(target: LOG_TARGET, "A repeated Transaction message was received, Trace: {}",
                            msg.dht_header.message_tag);
                        }
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to handle incoming Transaction message: {} for NodeID: {}, Trace: {}",
                                e, self.resources.wallet_identity.node_identity.node_id().short_str(), msg.dht_header.message_tag);
                            let _size = self.event_publisher.send(Arc::new(TransactionEvent::Error(format!("Error handling \
                                Transaction Sender message: {:?}", e).to_string())));
                        }
                        _ => (),
                    }
                    trace!(target: LOG_TARGET,
                        "Handling Transaction Message, Trace: {}, processed in {}ms",
                        msg.dht_header.message_tag,
                        start.elapsed().as_millis(),
                    );
                },
                 // Incoming Transaction Reply messages from the Comms layer
                Some(msg) = transaction_reply_stream.next() => {
                    let start = Instant::now();
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Transaction Reply Message, Trace: {}", msg.dht_header.message_tag);
                    let result = self.accept_recipient_reply(origin_public_key, inner_msg).await;

                    match result {
                        Err(TransactionServiceError::TransactionDoesNotExistError) => {
                            trace!(target: LOG_TARGET, "Unable to handle incoming Transaction Reply message from NodeId: \
                            {} due to Transaction not existing. This usually means the message was a repeated message \
                            from Store and Forward, Trace: {}", self.resources.wallet_identity.node_identity.node_id().short_str(),
                            msg.dht_header.message_tag);
                        },
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to handle incoming Transaction Reply message: {} \
                            for NodeId: {}, Trace: {}", e, self.resources.wallet_identity.node_identity.node_id().short_str(),
                            msg.dht_header.message_tag);
                            let _size = self.event_publisher.send(Arc::new(TransactionEvent::Error("Error handling \
                            Transaction Recipient Reply message".to_string())));
                        },
                        Ok(_) => (),
                    }
                    trace!(target: LOG_TARGET,
                        "Handling Transaction Reply Message, Trace: {}, processed in {}ms",
                        msg.dht_header.message_tag,
                        start.elapsed().as_millis(),
                    );
                },
               // Incoming Finalized Transaction messages from the Comms layer
                Some(msg) = transaction_finalized_stream.next() => {
                    let start = Instant::now();
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET,
                        "Handling Transaction Finalized Message, Trace: {}",
                        msg.dht_header.message_tag.as_value()
                    );
                    let result = self.accept_finalized_transaction(
                        origin_public_key,
                        inner_msg,
                        &mut receive_transaction_protocol_handles,
                    ).await;

                    match result {
                        Err(TransactionServiceError::TransactionDoesNotExistError) => {
                            trace!(target: LOG_TARGET, "Unable to handle incoming Finalized Transaction message from NodeId: \
                            {} due to Transaction not existing. This usually means the message was a repeated message \
                            from Store and Forward, Trace: {}", self.resources.wallet_identity.node_identity.node_id().short_str(),
                            msg.dht_header.message_tag);
                        },
                       Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to handle incoming Transaction Finalized message: {} \
                            for NodeID: {}, Trace: {}", e , self.resources.wallet_identity.node_identity.node_id().short_str(),
                            msg.dht_header.message_tag.as_value());
                            let _size = self.event_publisher.send(Arc::new(TransactionEvent::Error("Error handling Transaction \
                            Finalized message".to_string(),)));
                       },
                       Ok(_) => ()
                    }
                    trace!(target: LOG_TARGET,
                        "Handling Transaction Finalized Message, Trace: {}, processed in {}ms",
                        msg.dht_header.message_tag.as_value(),
                        start.elapsed().as_millis(),
                    );
                },
                // Incoming messages from the Comms layer
                Some(msg) = base_node_response_stream.next() => {
                    let start = Instant::now();
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Base Node Response, Trace: {}", msg.dht_header.message_tag);
                    let _result = self.handle_base_node_response(inner_msg).await.map_err(|e| {
                        warn!(target: LOG_TARGET, "Error handling base node service response from {}: {:?} for \
                        NodeID: {}, Trace: {}", origin_public_key, e, self.resources.wallet_identity.node_identity.node_id().short_str(),
                        msg.dht_header.message_tag.as_value());
                        e
                    });
                    trace!(target: LOG_TARGET,
                        "Handling Base Node Response, Trace: {}, processed in {}ms",
                        msg.dht_header.message_tag,
                        start.elapsed().as_millis(),
                    );
                }
                // Incoming messages from the Comms layer
                Some(msg) = transaction_cancelled_stream.next() => {
                    let start = Instant::now();
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Transaction Cancelled message, Trace: {}", msg.dht_header.message_tag);
                    if let Err(e) = self.handle_transaction_cancelled_message(origin_public_key, inner_msg, ).await {
                        warn!(target: LOG_TARGET, "Error handing Transaction Cancelled Message: {:?}", e);
                    }
                    trace!(target: LOG_TARGET,
                        "Handling Transaction Cancelled message, Trace: {}, processed in {}ms",
                        msg.dht_header.message_tag,
                        start.elapsed().as_millis(),
                    );
                }
                Some(join_result) = send_transaction_protocol_handles.next() => {
                    trace!(target: LOG_TARGET, "Send Protocol for Transaction has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_send_transaction_protocol(
                            join_result_inner,
                            &mut transaction_broadcast_protocol_handles
                        ),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Send Transaction Protocol: {:?}", e),
                    };
                }
                Some(join_result) = receive_transaction_protocol_handles.next() => {
                    trace!(target: LOG_TARGET, "Receive Transaction Protocol has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_receive_transaction_protocol(
                            join_result_inner,
                            &mut transaction_broadcast_protocol_handles
                        ),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Send Transaction Protocol: {:?}", e),
                    };
                }
                Some(join_result) = transaction_broadcast_protocol_handles.next() => {
                    trace!(target: LOG_TARGET, "Transaction Broadcast protocol has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_transaction_broadcast_protocol(join_result_inner),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Broadcast Protocol: {:?}", e),
                    };
                }
                Some(join_result) = transaction_validation_protocol_handles.next() => {
                    trace!(target: LOG_TARGET, "Transaction Validation protocol has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_transaction_validation_protocol(
                            join_result_inner,
                            &mut transaction_broadcast_protocol_handles,
                        ),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Transaction Validation protocol: {:?}", e),
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
        send_transaction_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TransactionSendResult, TransactionServiceProtocolError<TxId>>>,
        >,
        receive_transaction_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        transaction_validation_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<OperationId, TransactionServiceProtocolError<OperationId>>>,
        >,
        reply_channel: oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
    ) -> Result<(), TransactionServiceError> {
        let mut reply_channel = Some(reply_channel);

        trace!(target: LOG_TARGET, "Handling Service Request: {}", request);
        let response = match request {
            TransactionServiceRequest::SendTransaction {
                destination,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                message,
            } => {
                let rp = reply_channel.take().expect("Cannot be missing");
                self.send_transaction(
                    destination,
                    amount,
                    selection_criteria,
                    *output_features,
                    fee_per_gram,
                    message,
                    TransactionMetadata::default(),
                    send_transaction_join_handles,
                    transaction_broadcast_join_handles,
                    rp,
                )
                .await?;
                return Ok(());
            },
            TransactionServiceRequest::SendOneSidedTransaction {
                destination,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                message,
                payment_id,
            } => self
                .send_one_sided_transaction(
                    destination,
                    amount,
                    selection_criteria,
                    *output_features,
                    fee_per_gram,
                    message,
                    payment_id,
                    transaction_broadcast_join_handles,
                )
                .await
                .map(TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::SendOneSidedToStealthAddressTransaction {
                destination,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                message,
                payment_id,
            } => self
                .send_one_sided_to_stealth_address_transaction(
                    destination,
                    amount,
                    selection_criteria,
                    *output_features,
                    fee_per_gram,
                    message,
                    payment_id,
                    transaction_broadcast_join_handles,
                )
                .await
                .map(TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::BurnTari {
                amount,
                selection_criteria,
                fee_per_gram,
                message,
                claim_public_key,
            } => self
                .burn_tari(
                    amount,
                    selection_criteria,
                    fee_per_gram,
                    message,
                    claim_public_key,
                    transaction_broadcast_join_handles,
                )
                .await
                .map(|(tx_id, proof)| TransactionServiceResponse::BurntTransactionSent {
                    tx_id,
                    proof: Box::new(proof),
                }),
            TransactionServiceRequest::CreateNMUtxo {
                amount,
                fee_per_gram,
                n,
                m,
                public_keys,
                message,
                maturity,
            } => self
                .create_aggregate_signature_utxo(
                    amount,
                    fee_per_gram,
                    n,
                    m,
                    public_keys,
                    message,
                    maturity,
                    transaction_broadcast_join_handles,
                )
                .await
                .map(|(tx_id, output_hash)| {
                    TransactionServiceResponse::TransactionSentWithOutputHash(tx_id, output_hash)
                }),
            TransactionServiceRequest::EncumberAggregateUtxo {
                fee_per_gram,
                output_hash,
                expected_commitment,
                script_input_shares,
                script_signature_public_nonces,
                sender_offset_public_key_shares,
                metadata_ephemeral_public_key_shares,
                dh_shared_secret_shares,
                recipient_address,
            } => self
                .encumber_aggregate_tx(
                    fee_per_gram,
                    output_hash,
                    expected_commitment,
                    script_input_shares,
                    script_signature_public_nonces,
                    sender_offset_public_key_shares,
                    metadata_ephemeral_public_key_shares,
                    dh_shared_secret_shares,
                    recipient_address,
                )
                .await
                .map(
                    |(tx_id, tx, total_script_pubkey, total_metadata_ephemeral_public_key, total_script_nonce)| {
                        TransactionServiceResponse::EncumberAggregateUtxo(
                            tx_id,
                            Box::new(tx),
                            Box::new(total_script_pubkey),
                            Box::new(total_metadata_ephemeral_public_key),
                            Box::new(total_script_nonce),
                        )
                    },
                ),
            TransactionServiceRequest::FetchUnspentOutputs { output_hashes } => {
                let unspent_outputs = self.fetch_unspent_outputs_from_node(output_hashes).await?;
                Ok(TransactionServiceResponse::UnspentOutputs(unspent_outputs))
            },
            TransactionServiceRequest::FinalizeSentAggregateTransaction {
                tx_id,
                total_meta_data_signature,
                total_script_data_signature,
                script_offset,
            } => Ok(TransactionServiceResponse::TransactionSent(
                self.finalized_aggregate_encumbed_tx(
                    tx_id.into(),
                    total_meta_data_signature,
                    total_script_data_signature,
                    script_offset,
                    transaction_broadcast_join_handles,
                )
                .await?,
            )),
            TransactionServiceRequest::RegisterValidatorNode {
                amount,
                validator_node_public_key,
                validator_node_signature,
                selection_criteria,
                fee_per_gram,
                message,
            } => {
                let rp = reply_channel.take().expect("Cannot be missing");
                self.register_validator_node(
                    amount,
                    validator_node_public_key,
                    validator_node_signature,
                    selection_criteria,
                    fee_per_gram,
                    message,
                    send_transaction_join_handles,
                    transaction_broadcast_join_handles,
                    rp,
                )
                .await?;
                return Ok(());
            },
            TransactionServiceRequest::RegisterCodeTemplate {
                author_public_key,
                author_signature,
                template_name,
                template_version,
                template_type,
                build_info,
                binary_sha,
                binary_url,
                fee_per_gram,
            } => {
                self.register_code_template(
                    fee_per_gram,
                    CodeTemplateRegistration {
                        author_public_key,
                        author_signature,
                        template_name: template_name.clone(),
                        template_version,
                        template_type,
                        build_info,
                        binary_sha,
                        binary_url,
                    },
                    UtxoSelectionCriteria::default(),
                    format!("Template Registration: {}", template_name),
                    send_transaction_join_handles,
                    transaction_broadcast_join_handles,
                    reply_channel.take().expect("Reply channel is not set"),
                )
                .await?;

                return Ok(());
            },
            TransactionServiceRequest::SendShaAtomicSwapTransaction(
                destination,
                amount,
                selection_criteria,
                fee_per_gram,
                message,
            ) => Ok(TransactionServiceResponse::ShaAtomicSwapTransactionSent(
                self.send_sha_atomic_swap_transaction(
                    destination,
                    amount,
                    selection_criteria,
                    fee_per_gram,
                    message,
                    transaction_broadcast_join_handles,
                )
                .await?,
            )),
            TransactionServiceRequest::CancelTransaction(tx_id) => self
                .cancel_pending_transaction(tx_id)
                .await
                .map(|_| TransactionServiceResponse::TransactionCancelled),
            TransactionServiceRequest::GetPendingInboundTransactions => Ok(
                TransactionServiceResponse::PendingInboundTransactions(self.db.get_pending_inbound_transactions()?),
            ),
            TransactionServiceRequest::GetPendingOutboundTransactions => Ok(
                TransactionServiceResponse::PendingOutboundTransactions(self.db.get_pending_outbound_transactions()?),
            ),

            TransactionServiceRequest::GetCompletedTransactions => Ok(
                TransactionServiceResponse::CompletedTransactions(self.db.get_completed_transactions()?),
            ),
            TransactionServiceRequest::GetCancelledPendingInboundTransactions => {
                Ok(TransactionServiceResponse::PendingInboundTransactions(
                    self.db.get_cancelled_pending_inbound_transactions()?,
                ))
            },
            TransactionServiceRequest::GetCancelledPendingOutboundTransactions => {
                Ok(TransactionServiceResponse::PendingOutboundTransactions(
                    self.db.get_cancelled_pending_outbound_transactions()?,
                ))
            },
            TransactionServiceRequest::GetCancelledCompletedTransactions => Ok(
                TransactionServiceResponse::CompletedTransactions(self.db.get_cancelled_completed_transactions()?),
            ),
            TransactionServiceRequest::GetCompletedTransaction(tx_id) => Ok(
                TransactionServiceResponse::CompletedTransaction(Box::new(self.db.get_completed_transaction(tx_id)?)),
            ),
            TransactionServiceRequest::GetAnyTransaction(tx_id) => Ok(TransactionServiceResponse::AnyTransaction(
                Box::new(self.db.get_any_transaction(tx_id)?),
            )),
            TransactionServiceRequest::ImportTransaction(tx) => {
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
                Ok(TransactionServiceResponse::TransactionImported(tx_id))
            },
            TransactionServiceRequest::ImportUtxoWithStatus {
                amount,
                source_address,
                message,
                import_status,
                tx_id,
                current_height,
                mined_timestamp,
                scanned_output,
                payment_id,
            } => self
                .add_utxo_import_transaction_with_status(
                    amount,
                    source_address,
                    message,
                    import_status,
                    tx_id,
                    current_height,
                    mined_timestamp,
                    scanned_output,
                    payment_id,
                )
                .await
                .map(TransactionServiceResponse::UtxoImported),
            TransactionServiceRequest::SubmitTransactionToSelf(tx_id, tx, fee, amount, message) => self
                .submit_transaction_to_self(transaction_broadcast_join_handles, tx_id, tx, fee, amount, message)
                .await
                .map(|_| TransactionServiceResponse::TransactionSubmitted),
            TransactionServiceRequest::SetLowPowerMode => {
                self.set_power_mode(PowerMode::Low).await?;
                Ok(TransactionServiceResponse::LowPowerModeSet)
            },
            TransactionServiceRequest::SetNormalPowerMode => {
                self.set_power_mode(PowerMode::Normal).await?;
                Ok(TransactionServiceResponse::NormalPowerModeSet)
            },
            TransactionServiceRequest::RestartTransactionProtocols => self
                .restart_transaction_negotiation_protocols(
                    send_transaction_join_handles,
                    receive_transaction_join_handles,
                )
                .map(|_| TransactionServiceResponse::ProtocolsRestarted),
            TransactionServiceRequest::RestartBroadcastProtocols => self
                .restart_broadcast_protocols(transaction_broadcast_join_handles)
                .map(|_| TransactionServiceResponse::ProtocolsRestarted),
            TransactionServiceRequest::GetNumConfirmationsRequired => Ok(
                TransactionServiceResponse::NumConfirmationsRequired(self.resources.config.num_confirmations_required),
            ),
            TransactionServiceRequest::SetNumConfirmationsRequired(number) => {
                self.resources.config.num_confirmations_required = number;
                Ok(TransactionServiceResponse::NumConfirmationsSet)
            },
            TransactionServiceRequest::ValidateTransactions => self
                .start_transaction_validation_protocol(transaction_validation_join_handles)
                .await
                .map(TransactionServiceResponse::ValidationStarted),
            TransactionServiceRequest::ReValidateTransactions => self
                .start_transaction_revalidation(transaction_validation_join_handles)
                .await
                .map(TransactionServiceResponse::ValidationStarted),
            TransactionServiceRequest::GetFeePerGramStatsPerBlock { count } => {
                let reply_channel = reply_channel.take().expect("reply_channel is Some");
                self.handle_get_fee_per_gram_stats_per_block_request(count, reply_channel);
                return Ok(());
            },
        };

        // If the individual handlers did not already send the API response then do it here.
        if let Some(rp) = reply_channel {
            let _result = rp.send(response).inspect_err(|_| {
                warn!(target: LOG_TARGET, "Failed to send reply");
            });
        }
        Ok(())
    }

    fn handle_get_fee_per_gram_stats_per_block_request(
        &self,
        count: usize,
        reply_channel: oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
    ) {
        let mut connectivity = self.resources.connectivity.clone();

        let query_base_node_fut = async move {
            let mut client = connectivity
                .obtain_base_node_wallet_rpc_client()
                .await
                .ok_or(TransactionServiceError::Shutdown)?;

            let resp = client
                .get_mempool_fee_per_gram_stats(base_node_proto::GetMempoolFeePerGramStatsRequest {
                    count: count as u64,
                })
                .await?;
            let mut resp = FeePerGramStatsResponse::from(resp);
            // If there are no transactions in the mempool, populate with a minimal fee per gram.
            if resp.stats.is_empty() {
                resp.stats = vec![FeePerGramStat {
                    order: 0,
                    min_fee_per_gram: 1.into(),
                    avg_fee_per_gram: 1.into(),
                    max_fee_per_gram: 1.into(),
                }]
            }
            Ok(TransactionServiceResponse::FeePerGramStatsPerBlock(resp))
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

    async fn handle_base_node_service_event(
        &mut self,
        event: Arc<BaseNodeEvent>,
        transaction_validation_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<OperationId, TransactionServiceProtocolError<OperationId>>>,
        >,
    ) {
        match (*event).clone() {
            BaseNodeEvent::BaseNodeStateChanged(_state) => {
                trace!(target: LOG_TARGET, "Received BaseNodeStateChanged event, but igoring",);
            },
            BaseNodeEvent::NewBlockDetected(_hash, height) => {
                let _operation_id = self
                    .start_transaction_validation_protocol(transaction_validation_join_handles)
                    .await
                    .map_err(|e| {
                        warn!(target: LOG_TARGET, "Error validating  txos: {:?}", e);
                        e
                    });

                self.last_seen_tip_height = Some(height);
            },
        }
    }

    async fn handle_output_manager_service_event(&mut self, event: Arc<OutputManagerEvent>) {
        if let OutputManagerEvent::TxoValidationSuccess(_) = (*event).clone() {
            let db = self.db.clone();
            let output_manager_handle = self.resources.output_manager_service.clone();
            let metadata = self.wallet_db.get_chain_metadata().unwrap_or_default();
            let tip_height = match metadata {
                Some(val) => val.best_block_height(),
                None => 0u64,
            };
            let event_publisher = self.event_publisher.clone();
            tokio::spawn(check_detected_transactions(
                output_manager_handle,
                db,
                event_publisher,
                tip_height,
            ));
        }
    }

    /// Sends a new transaction to a single recipient
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    pub async fn send_transaction(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        message: String,
        tx_meta: TransactionMetadata,
        join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TransactionSendResult, TransactionServiceProtocolError<TxId>>>,
        >,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        reply_channel: oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
    ) -> Result<(), TransactionServiceError> {
        let tx_id = TxId::new_random();
        if destination.network() != self.resources.wallet_identity.address.network() {
            let _result = reply_channel
                .send(Err(TransactionServiceError::InvalidNetwork))
                .inspect_err(|_| {
                    warn!(target: LOG_TARGET, "Failed to send service reply");
                });
            return Err(TransactionServiceError::InvalidNetwork);
        }
        // If we're paying ourselves, let's complete and submit the transaction immediately
        if self.resources.wallet_identity.address.comms_public_key() == destination.comms_public_key() {
            debug!(
                target: LOG_TARGET,
                "Received transaction with spend-to-self transaction"
            );

            let (fee, transaction) = self
                .resources
                .output_manager_service
                .create_pay_to_self_transaction(tx_id, amount, selection_criteria, output_features, fee_per_gram, None)
                .await?;

            // Notify that the transaction was successfully resolved.
            let _size = self
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));

            self.submit_transaction(
                transaction_broadcast_join_handles,
                CompletedTransaction::new(
                    tx_id,
                    self.resources.wallet_identity.address.clone(),
                    self.resources.wallet_identity.address.clone(),
                    amount,
                    fee,
                    transaction,
                    TransactionStatus::Completed,
                    message,
                    Utc::now().naive_utc(),
                    TransactionDirection::Inbound,
                    None,
                    None,
                    None,
                )?,
            )
            .await?;

            let _result = reply_channel
                .send(Ok(TransactionServiceResponse::TransactionSent(tx_id)))
                .inspect_err(|_| {
                    warn!(target: LOG_TARGET, "Failed to send service reply");
                });

            return Ok(());
        }

        let (tx_reply_sender, tx_reply_receiver) = mpsc::channel(100);
        let (cancellation_sender, cancellation_receiver) = oneshot::channel();
        self.pending_transaction_reply_senders.insert(tx_id, tx_reply_sender);
        self.send_transaction_cancellation_senders
            .insert(tx_id, cancellation_sender);

        let protocol = TransactionSendProtocol::new(
            tx_id,
            self.resources.clone(),
            tx_reply_receiver,
            cancellation_receiver,
            destination,
            amount,
            fee_per_gram,
            message,
            tx_meta,
            Some(reply_channel),
            TransactionSendProtocolStage::Initial,
            None,
        );
        let join_handle = tokio::spawn(protocol.execute());
        join_handles.push(join_handle);

        Ok(())
    }

    /// Creates a utxo with aggregate public key out of m-of-n public keys
    #[allow(clippy::too_many_lines)]
    pub async fn create_aggregate_signature_utxo(
        &mut self,
        amount: MicroMinotari,
        fee_per_gram: MicroMinotari,
        n: u8,
        m: u8,
        public_keys: Vec<PublicKey>,
        message: [u8; 32],
        maturity: u64,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<(TxId, FixedHash), TransactionServiceError> {
        let tx_id = TxId::new_random();

        let msg = slice_to_boxed_message(message.as_bytes());
        let script = script!(CheckMultiSigVerifyAggregatePubKey(n, m, public_keys.clone(), msg));

        // Empty covenant
        let covenant = Covenant::default();

        // Default range proof
        let minimum_value_promise = amount;

        // Prepare sender part of transaction
        let mut stp = self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                tx_id,
                amount,
                UtxoSelectionCriteria::default(),
                OutputFeatures {
                    range_proof_type: RangeProofType::RevealedValue,
                    maturity,
                    ..Default::default()
                },
                fee_per_gram,
                TransactionMetadata::default(),
                "".to_string(),
                script.clone(),
                covenant.clone(),
                minimum_value_promise,
            )
            .await?;
        let sender_message = TransactionSenderMessage::new_single_round_message(
            stp.get_single_round_message(&self.resources.transaction_key_manager_service)
                .await?,
        );

        // This call is needed to advance the state from `SingleRoundMessageReady` to `CollectingSingleSignature`,
        // but the returned value is not used
        let _single_round_sender_data = stp
            .build_single_round_message(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        self.resources
            .output_manager_service
            .confirm_pending_transaction(tx_id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Prepare receiver part of the transaction

        // In generating an aggregate public key utxo, we can use a randomly generated spend key
        let spending_key = PrivateKey::random(&mut OsRng);
        let sum_keys = public_keys.iter().fold(PublicKey::default(), |acc, x| acc + x);
        let encryption_private_key = public_key_to_output_encryption_key(&sum_keys)?;

        let sender_offset_private_key = stp
            .get_recipient_sender_offset_private_key()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?
            .ok_or(TransactionServiceProtocolError::new(
                tx_id,
                TransactionServiceError::InvalidKeyId("Missing sender offset keyid".to_string()),
            ))?;

        let encryption_key_id = self
            .resources
            .transaction_key_manager_service
            .import_key(encryption_private_key)
            .await?;

        let sender_offset_public_key = self
            .resources
            .transaction_key_manager_service
            .get_public_key_at_key_id(&sender_offset_private_key)
            .await?;

        let spending_key_id = self
            .resources
            .transaction_key_manager_service
            .import_key(spending_key.clone())
            .await?;

        let wallet_output = WalletOutputBuilder::new(amount, spending_key_id)
            .with_features(
                sender_message
                    .single()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::InvalidMessageError("Sent invalid message type".to_string()),
                    ))?
                    .features
                    .clone(),
            )
            .with_script(script)
            // We don't want the given utxo to be spendable as an input to a later transaction, so we set
            // spendable height of the current utxo to be u64::MAx
            .with_script_lock_height(u64::MAX)
            .encrypt_data_for_recovery(
                &self.resources.transaction_key_manager_service,
                Some(&encryption_key_id),
                PaymentId::Empty,
            )
            .await?
            .with_input_data(
                ExecutionStack::default(),
            )
            .with_covenant(covenant)
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(TariKeyId::default())
            .with_minimum_value_promise(minimum_value_promise)
            .sign_as_sender_and_receiver(
                &self.resources.transaction_key_manager_service,
                &sender_offset_private_key,
            )
            .await
            .unwrap()
            .try_build(&self.resources.transaction_key_manager_service)
            .await
            .unwrap();

        let tip_height = self.last_seen_tip_height.unwrap_or(0);
        let consensus_constants = self.consensus_manager.consensus_constants(tip_height);
        let rtp = ReceiverTransactionProtocol::new(
            sender_message,
            wallet_output.clone(),
            &self.resources.transaction_key_manager_service,
            consensus_constants,
        )
        .await;
        let recipient_reply = rtp.get_signed_data()?.clone();

        // Start finalize
        stp.add_single_recipient_info(recipient_reply, &self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Finalize:
        stp.finalize(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) could not be finalized. Failure error: {:?}", tx_id, e,
                );
                TransactionServiceProtocolError::new(tx_id, e.into())
            })?;
        info!(
            target: LOG_TARGET,
            "Finalized create n of m transaction TxId: {}", tx_id
        );

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));

        // Broadcast create n of m aggregate public key transaction
        let tx = stp
            .get_transaction()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        let fee = stp
            .get_fee_amount()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        self.resources
            .output_manager_service
            .add_output_with_tx_id(tx_id, wallet_output.clone(), Some(SpendingPriority::Normal))
            .await?;
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new(
                tx_id,
                self.resources.wallet_identity.address.clone(),
                self.resources.wallet_identity.address.clone(),
                amount,
                fee,
                tx.clone(),
                TransactionStatus::Completed,
                "".to_string(),
                Utc::now().naive_utc(),
                TransactionDirection::Outbound,
                None,
                None,
                None,
            )
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?,
        )
        .await?;

        // we want to print out the hash of the utxo
        let output_hash = wallet_output
            .hash(&self.resources.transaction_key_manager_service)
            .await?;
        Ok((tx_id, output_hash))
    }

    async fn fetch_unspent_outputs_from_node(
        &mut self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionOutput>, TransactionServiceError> {
        // lets get the output from the blockchain
        let req = FetchMatchingUtxos {
            output_hashes: hashes.iter().map(|v| v.to_vec()).collect(),
        };
        let results: Vec<TransactionOutput> = self
            .resources
            .connectivity
            .obtain_base_node_wallet_rpc_client()
            .await
            .ok_or_else(|| {
                TransactionServiceError::ServiceError("Could not connect to base node rpc client".to_string())
            })?
            .fetch_matching_utxos(req)
            .await?
            .outputs
            .into_iter()
            .filter_map(|o| match o.try_into() {
                Ok(output) => Some(output),
                _ => None,
            })
            .collect();
        Ok(results)
    }

    /// Creates an encumbered uninitialized transaction
    #[allow(clippy::mutable_key_type)]
    pub async fn encumber_aggregate_tx(
        &mut self,
        fee_per_gram: MicroMinotari,
        output_hash: HashOutput,
        expected_commitment: PedersenCommitment,
        script_input_shares: HashMap<PublicKey, CheckSigSchnorrSignature>,
        script_signature_public_nonces: Vec<PublicKey>,
        sender_offset_public_key_shares: Vec<PublicKey>,
        metadata_ephemeral_public_key_shares: Vec<PublicKey>,
        dh_shared_secret_shares: Vec<PublicKey>,
        recipient_address: TariAddress,
    ) -> Result<(TxId, Transaction, PublicKey, PublicKey, PublicKey), TransactionServiceError> {
        let tx_id = TxId::new_random();

        match self
            .resources
            .output_manager_service
            .encumber_aggregate_utxo(
                tx_id,
                fee_per_gram,
                output_hash,
                expected_commitment,
                script_input_shares,
                script_signature_public_nonces,
                sender_offset_public_key_shares,
                metadata_ephemeral_public_key_shares,
                dh_shared_secret_shares,
                recipient_address.clone(),
            )
            .await
        {
            Ok((
                transaction,
                amount,
                fee,
                total_script_key,
                total_metadata_ephemeral_public_key,
                total_script_nonce,
            )) => {
                let completed_tx = CompletedTransaction::new(
                    tx_id,
                    self.resources.wallet_identity.address.clone(),
                    recipient_address,
                    amount,
                    fee,
                    transaction.clone(),
                    TransactionStatus::Pending,
                    "claimed n-of-m utxo".to_string(),
                    Utc::now().naive_utc(),
                    TransactionDirection::Outbound,
                    None,
                    None,
                    None,
                )
                .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
                self.db.insert_completed_transaction(tx_id, completed_tx)?;
                Ok((
                    tx_id,
                    transaction,
                    total_script_key,
                    total_metadata_ephemeral_public_key,
                    total_script_nonce,
                ))
            },
            Err(e) => Err(e.into()),
        }
    }

    /// Creates an encumbered uninitialized transaction
    pub async fn finalized_aggregate_encumbed_tx(
        &mut self,
        tx_id: TxId,
        total_meta_data_signature: Signature,
        total_script_data_signature: Signature,
        script_offset: PrivateKey,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        let mut transaction = self.db.get_completed_transaction(tx_id)?;

        // Add the aggregate signature components
        transaction.transaction.script_offset = &transaction.transaction.script_offset + &script_offset;

        transaction.transaction.body.update_metadata_signature(
            &(transaction.transaction.body.outputs()[0].commitment.clone()),
            &transaction.transaction.body.outputs()[0].metadata_signature + &total_meta_data_signature,
        )?;

        transaction.transaction.body.update_script_signature(
            &(transaction.transaction.body.inputs()[0].commitment()?.clone()),
            &transaction.transaction.body.inputs()[0].script_signature + &total_script_data_signature,
        )?;

        // Validate the aggregate signatures and script offset
        let factory = CommitmentFactory::default();
        let mut input_keys = PublicKey::default();
        for input in transaction.transaction.body.inputs() {
            let context = ScriptContext::new(
                self.last_seen_tip_height.unwrap_or(0),
                &[0; 32],
                input
                    .commitment()
                    .map_err(|e| TransactionServiceError::ServiceError(format!("TxId: {}, {}", tx_id, e)))?,
            );
            input_keys = input_keys +
                input
                    .run_and_verify_script(&factory, Some(context))
                    .map_err(|e| TransactionServiceError::ServiceError(format!("TxId: {}, {}", tx_id, e)))?;
        }
        let mut output_keys = PublicKey::default();
        for output in transaction.transaction.body.outputs() {
            output
                .verify_metadata_signature()
                .map_err(|e| TransactionServiceError::ServiceError(format!("TxId: {}, {}", tx_id, e)))?;
            output_keys = output_keys + output.sender_offset_public_key.clone();
        }
        let lhs = input_keys - output_keys;
        if lhs != PublicKey::from_secret_key(&transaction.transaction.script_offset) {
            return Err(TransactionServiceError::ServiceError(format!(
                "Invalid script offset (TxId: {})",
                tx_id
            )));
        }

        // Update the wallet database
        let _res = self
            .resources
            .output_manager_service
            .update_output_metadata_signature(transaction.transaction.body.outputs()[0].clone())
            .await;

        self.db.update_completed_transaction(tx_id, transaction)?;

        self.resources
            .output_manager_service
            .confirm_pending_transaction(tx_id)
            .await?;

        // Notify that the transaction was successfully resolved.
        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));

        self.complete_send_transaction_protocol(
            Ok(TransactionSendResult {
                tx_id,
                transaction_status: TransactionStatus::Completed,
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
        message: String,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<Box<(TxId, PublicKey, TransactionOutput)>, TransactionServiceError> {
        let tx_id = TxId::new_random();
        // this can be anything, so lets generate a random private key
        let pre_image = PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng));
        let hash: [u8; 32] = Sha256::digest(pre_image.as_bytes()).into();

        // lets make the unlock height a day from now, 2 min blocks which gives us 30 blocks per hour * 24 hours
        let tip_height = self.last_seen_tip_height.unwrap_or(0);
        let height = tip_height + (24 * 30);

        // lets create the HTLC script
        let script = script!(
            HashSha256 PushHash(Box::new(hash)) Equal IfThen
                PushPubKey(Box::new(destination.public_spend_key().clone()))
            Else
                CheckHeightVerify(height) PushPubKey(Box::new(self.resources.wallet_identity.node_identity.public_key().clone()))
            EndIf
        );

        // Empty covenant
        let covenant = Covenant::default();

        // Default range proof
        let minimum_value_promise = MicroMinotari::zero();

        // Prepare sender part of the transaction
        let mut stp = self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                tx_id,
                amount,
                selection_criteria,
                OutputFeatures::default(),
                fee_per_gram,
                TransactionMetadata::default(),
                message.clone(),
                script.clone(),
                covenant.clone(),
                minimum_value_promise,
            )
            .await?;

        // This call is needed to advance the state from `SingleRoundMessageReady` to `SingleRoundMessageReady`,
        // but the returned value is not used
        let _single_round_sender_data = stp
            .build_single_round_message(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        self.resources
            .output_manager_service
            .confirm_pending_transaction(tx_id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Prepare receiver part of the transaction

        // Diffie-Hellman shared secret `k_Ob * K_Sb = K_Ob * k_Sb` results in a public key, which is fed into
        // KDFs to produce the spending, rewind, and encryption keys
        let sender_offset_private_key = stp
            .get_recipient_sender_offset_private_key()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?
            .ok_or(TransactionServiceProtocolError::new(
                tx_id,
                TransactionServiceError::InvalidKeyId("Missing sender offset keyid".to_string()),
            ))?;

        let shared_secret = self
            .resources
            .transaction_key_manager_service
            .get_diffie_hellman_shared_secret(
                &sender_offset_private_key,
                destination
                    .public_view_key()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::InvalidAddress("Missing public view key".to_string()),
                    ))?,
            )
            .await?;
        let spending_key = shared_secret_to_output_spending_key(&shared_secret)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        let sender_message = TransactionSenderMessage::new_single_round_message(
            stp.get_single_round_message(&self.resources.transaction_key_manager_service)
                .await?,
        );
        let encryption_private_key = shared_secret_to_output_encryption_key(&shared_secret)?;
        let encryption_key = self
            .resources
            .transaction_key_manager_service
            .import_key(encryption_private_key)
            .await?;

        let sender_offset_public_key = self
            .resources
            .transaction_key_manager_service
            .get_public_key_at_key_id(&sender_offset_private_key)
            .await?;

        let spending_key_id = self
            .resources
            .transaction_key_manager_service
            .import_key(spending_key)
            .await?;

        let minimum_value_promise = MicroMinotari::zero();
        let output = WalletOutputBuilder::new(amount, spending_key_id)
            .with_features(
                sender_message
                    .single()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::InvalidMessageError("Sent invalid message type".to_string()),
                    ))?
                    .features
                    .clone(),
            )
            .with_script(script)
            .encrypt_data_for_recovery(
                &self.resources.transaction_key_manager_service,
                Some(&encryption_key),
                PaymentId::Empty,
            )
            .await?
            .with_input_data(ExecutionStack::default())
            .with_covenant(covenant)
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(self.resources.wallet_identity.wallet_node_key_id.clone())
            .with_minimum_value_promise(minimum_value_promise)
            .sign_as_sender_and_receiver(
                &self.resources.transaction_key_manager_service,
                &sender_offset_private_key,
            )
            .await
            .unwrap()
            .try_build(&self.resources.transaction_key_manager_service)
            .await
            .unwrap();

        let consensus_constants = self.consensus_manager.consensus_constants(tip_height);
        let rtp = ReceiverTransactionProtocol::new(
            sender_message,
            output.clone(),
            &self.resources.transaction_key_manager_service,
            consensus_constants,
        )
        .await;

        let recipient_reply = rtp.get_signed_data()?.clone();

        // Start finalizing

        stp.add_presigned_recipient_info(recipient_reply)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Finalize

        stp.finalize(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) could not be finalized. Failure error: {:?}", tx_id, e,
                );
                TransactionServiceProtocolError::new(tx_id, e.into())
            })?;
        info!(target: LOG_TARGET, "Finalized one-side transaction TxId: {}", tx_id);

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _size = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));

        // Broadcast one-sided transaction

        let tx = stp
            .get_transaction()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        let fee = stp
            .get_fee_amount()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        self.resources
            .output_manager_service
            .add_output_with_tx_id(tx_id, output.clone(), Some(SpendingPriority::HtlcSpendAsap))
            .await?;
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new(
                tx_id,
                self.resources.wallet_identity.address.clone(),
                destination,
                amount,
                fee,
                tx.clone(),
                TransactionStatus::Completed,
                message.clone(),
                Utc::now().naive_utc(),
                TransactionDirection::Outbound,
                None,
                None,
                None,
            )?,
        )
        .await?;

        let tx_output = output
            .to_transaction_output(&self.resources.transaction_key_manager_service)
            .await?;

        Ok(Box::new((tx_id, pre_image, tx_output)))
    }

    #[allow(clippy::too_many_lines)]
    async fn send_one_sided_or_stealth(
        &mut self,
        dest_address: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        message: String,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        recipient_script: Option<TariScript>,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        let tx_id = TxId::new_random();

        // For a stealth transaction, the script is not provided because the public key that should be included
        // is not known at this stage. This will only be known later. For now,
        // we include a default public key to ensure that the script size is correct.
        let (mut script, use_stealth_address) = match recipient_script {
            Some(s) => (s, false),
            None => (push_pubkey_script(&Default::default()), true),
        };
        // Prepare sender part of the transaction
        let mut stp = self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                tx_id,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                TransactionMetadata::default(),
                message.clone(),
                script.clone(),
                Covenant::default(),
                MicroMinotari::zero(),
            )
            .await?;

        // This call is needed to advance the state from `SingleRoundMessageReady` to `SingleRoundMessageReady`,
        // but the returned value is not used. We have to wait until the sender transaction protocol creates a
        // sender_offset_private_key for us, so we can use it to create the shared secret
        let (key, _) = self
            .resources
            .transaction_key_manager_service
            .get_next_key(TransactionKeyManagerBranch::SenderOffsetLedger.get_branch_key())
            .await?;

        stp.change_recipient_sender_offset_private_key(key)?;
        let _single_round_sender_data = stp
            .build_single_round_message(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        self.resources
            .output_manager_service
            .confirm_pending_transaction(tx_id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Prepare receiver part of the transaction

        // Diffie-Hellman shared secret `k_Ob * K_Sb = K_Ob * k_Sb` results in a public key, which is fed into
        // KDFs to produce the spending, rewind, and encryption keys
        let sender_offset_private_key = stp
            .get_recipient_sender_offset_private_key()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?
            .ok_or(TransactionServiceProtocolError::new(
                tx_id,
                TransactionServiceError::InvalidKeyId("Missing sender offset keyid".to_string()),
            ))?;

        if use_stealth_address {
            // lets fix the script with the correct one
            let c = self
                .resources
                .transaction_key_manager_service
                .get_diffie_hellman_stealth_domain_hasher(
                    &sender_offset_private_key,
                    dest_address
                        .public_view_key()
                        .ok_or(TransactionServiceError::OneSidedTransactionError(
                            "Missing public view key".to_string(),
                        ))?,
                )
                .await?;

            let script_spending_key = stealth_address_script_spending_key(&c, dest_address.public_spend_key());
            script = push_pubkey_script(&script_spending_key);
        }

        let shared_secret = self
            .resources
            .transaction_key_manager_service
            .get_diffie_hellman_shared_secret(
                &sender_offset_private_key,
                dest_address
                    .public_view_key()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::OneSidedTransactionError("Missing public view key".to_string()),
                    ))?,
            )
            .await?;
        let spending_key = shared_secret_to_output_spending_key(&shared_secret)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        let sender_message = TransactionSenderMessage::new_single_round_message(
            stp.get_single_round_message(&self.resources.transaction_key_manager_service)
                .await?,
        );

        let encryption_private_key = shared_secret_to_output_encryption_key(&shared_secret)?;
        let encryption_key = self
            .resources
            .transaction_key_manager_service
            .import_key(encryption_private_key)
            .await?;

        let spending_key_id = self
            .resources
            .transaction_key_manager_service
            .import_key(spending_key)
            .await?;

        let sender_offset_public_key = self
            .resources
            .transaction_key_manager_service
            .get_public_key_at_key_id(&sender_offset_private_key)
            .await?;

        let minimum_value_promise = MicroMinotari::zero();
        let output = WalletOutputBuilder::new(amount, spending_key_id)
            .with_features(
                sender_message
                    .single()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::InvalidMessageError("Sent invalid message type".to_string()),
                    ))?
                    .features
                    .clone(),
            )
            .with_script(script)
            .encrypt_data_for_recovery(
                &self.resources.transaction_key_manager_service,
                Some(&encryption_key),
                payment_id.clone(),
            )
            .await?
            .with_input_data(inputs!(PublicKey::from_secret_key(
                self.resources.wallet_identity.node_identity.secret_key()
            )))
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(self.resources.wallet_identity.wallet_node_key_id.clone())
            .with_minimum_value_promise(minimum_value_promise)
            .sign_as_sender_and_receiver(
                &self.resources.transaction_key_manager_service,
                &sender_offset_private_key,
            )
            .await?
            .try_build(&self.resources.transaction_key_manager_service)
            .await?;

        let tip_height = self.last_seen_tip_height.unwrap_or(0);
        let consensus_constants = self.consensus_manager.consensus_constants(tip_height);
        let rtp = ReceiverTransactionProtocol::new(
            sender_message,
            output,
            &self.resources.transaction_key_manager_service,
            consensus_constants,
        )
        .await;

        let recipient_reply = rtp.get_signed_data()?.clone();

        // Start finalizing
        stp.add_presigned_recipient_info(recipient_reply)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Finalize

        stp.finalize(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) could not be finalized. Failure error: {:?}", tx_id, e,
                );
                TransactionServiceProtocolError::new(tx_id, e.into())
            })?;
        info!(target: LOG_TARGET, "Finalized one-side transaction TxId: {}", tx_id);

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _result = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));

        // Broadcast one-sided transaction

        let tx = stp
            .get_transaction()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        let fee = stp
            .get_fee_amount()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new(
                tx_id,
                self.resources.wallet_identity.address.clone(),
                dest_address,
                amount,
                fee,
                tx.clone(),
                TransactionStatus::Completed,
                message.clone(),
                Utc::now().naive_utc(),
                TransactionDirection::Outbound,
                None,
                None,
                Some(payment_id),
            )?,
        )
        .await?;

        Ok(tx_id)
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
        message: String,
        payment_id: PaymentId,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        if destination.network() != self.resources.wallet_identity.address.network() {
            return Err(TransactionServiceError::InvalidNetwork);
        }
        if self.resources.wallet_identity.node_identity.public_key() == destination.comms_public_key() {
            warn!(target: LOG_TARGET, "One-sided spend-to-self transactions not supported");
            return Err(TransactionServiceError::OneSidedTransactionError(
                "One-sided spend-to-self transactions not supported".to_string(),
            ));
        }
        let dest_pubkey = destination.public_spend_key().clone();
        self.send_one_sided_or_stealth(
            destination,
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            message,
            transaction_broadcast_join_handles,
            Some(push_pubkey_script(&dest_pubkey)),
            payment_id,
        )
        .await
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
        message: String,
        claim_public_key: Option<PublicKey>,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<(TxId, BurntProof), TransactionServiceError> {
        let tx_id = TxId::new_random();
        trace!(target: LOG_TARGET, "Burning transaction start - TxId: {}", tx_id);
        let output_features = claim_public_key
            .as_ref()
            .cloned()
            .map(OutputFeatures::create_burn_confidential_output)
            .unwrap_or_else(OutputFeatures::create_burn_output);

        // Prepare sender part of the transaction
        let tx_meta = TransactionMetadata::new_with_features(0.into(), 0, KernelFeatures::create_burn());
        let mut stp = self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                tx_id,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                tx_meta,
                message.clone(),
                script!(Nop),
                Covenant::default(),
                MicroMinotari::zero(),
            )
            .await?;

        // This call is needed to advance the state from `SingleRoundMessageReady` to `SingleRoundMessageReady`,
        // but the returned value is not used
        let _single_round_sender_data = stp
            .build_single_round_message(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        self.resources
            .output_manager_service
            .confirm_pending_transaction(tx_id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        let sender_message = TransactionSenderMessage::new_single_round_message(
            stp.get_single_round_message(&self.resources.transaction_key_manager_service)
                .await?,
        );
        let (spend_key_id, public_spend_key, _script_key_id, _) = self
            .resources
            .transaction_key_manager_service
            .get_next_spend_and_script_key_ids()
            .await?;

        let recovery_key_id = self.resources.transaction_key_manager_service.get_view_key_id().await?;

        let recovery_key_id = match claim_public_key {
            Some(ref claim_public_key) => {
                // For claimable L2 burn transactions, we derive a shared secret and encryption key from a nonce (in
                // this case a new spend key from the key manager) and the provided claim public key. The public
                // nonce/spend_key is returned back to the caller.
                let shared_secret = self
                    .resources
                    .transaction_key_manager_service
                    .get_diffie_hellman_shared_secret(&spend_key_id, claim_public_key)
                    .await?;
                let encryption_key = shared_secret_to_output_encryption_key(&shared_secret)?;
                self.resources
                    .transaction_key_manager_service
                    .import_key(encryption_key.clone())
                    .await?;
                KeyId::Imported {
                    key: PublicKey::from_secret_key(&encryption_key),
                }
            },
            // No claim key provided, no shared secret or encryption key needed
            None => recovery_key_id,
        };
        let sender_offset_private_key = stp
            .get_recipient_sender_offset_private_key()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?
            .ok_or(TransactionServiceProtocolError::new(
                tx_id,
                TransactionServiceError::InvalidKeyId("Missing sender offset keyid".to_string()),
            ))?;
        let output = WalletOutputBuilder::new(amount, spend_key_id.clone())
            .with_features(
                sender_message
                    .single()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::InvalidMessageError("Sent invalid message type".to_string()),
                    ))?
                    .features
                    .clone(),
            )
            .with_script(script!(Nop))
            .encrypt_data_for_recovery(
                &self.resources.transaction_key_manager_service,
                Some(&recovery_key_id),
                PaymentId::Empty,
            )
            .await?
            .with_input_data(inputs!(PublicKey::from_secret_key(
                self.resources.wallet_identity.node_identity.secret_key()
            )))
            .with_sender_offset_public_key(
                sender_message
                    .single()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::InvalidMessageError("Sent invalid message type".to_string()),
                    ))?
                    .sender_offset_public_key
                    .clone(),
            )
            .with_script_key(self.resources.wallet_identity.wallet_node_key_id.clone())
            .with_minimum_value_promise(
                sender_message
                    .single()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::InvalidMessageError("Sent invalid message type".to_string()),
                    ))?
                    .minimum_value_promise,
            )
            .sign_as_sender_and_receiver(
                &self.resources.transaction_key_manager_service,
                &sender_offset_private_key,
            )
            .await?
            .try_build(&self.resources.transaction_key_manager_service)
            .await?;

        let tip_height = self.last_seen_tip_height.unwrap_or(0);
        let consensus_constants = self.consensus_manager.consensus_constants(tip_height);
        let rtp = ReceiverTransactionProtocol::new(
            sender_message,
            output,
            &self.resources.transaction_key_manager_service,
            consensus_constants,
        )
        .await;

        let recipient_reply = rtp.get_signed_data()?.clone();
        let range_proof = recipient_reply.output.proof_result()?.clone();
        let mut ownership_proof = None;
        let commitment = recipient_reply.output.commitment.clone();

        if let Some(claim_public_key) = claim_public_key {
            ownership_proof = Some(
                self.resources
                    .transaction_key_manager_service
                    .generate_burn_proof(&spend_key_id, &amount.into(), &claim_public_key)
                    .await?,
            );
        }

        // Start finalizing
        stp.add_presigned_recipient_info(recipient_reply)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Finalize
        stp.finalize(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) could not be finalized. Failure error: {:?}", tx_id, e,
                );
                TransactionServiceProtocolError::new(tx_id, e.into())
            })?;
        info!(target: LOG_TARGET, "Finalized burning transaction - TxId: {}", tx_id);

        // This event being sent is important, but not critical to the protocol being successful. Send only fails if
        // there are no subscribers.
        let _result = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));

        // Broadcast burn transaction
        let tx = stp
            .get_transaction()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        let fee = stp
            .get_fee_amount()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new(
                tx_id,
                self.resources.wallet_identity.address.clone(),
                TariAddress::default(),
                amount,
                fee,
                tx.clone(),
                TransactionStatus::Completed,
                message.clone(),
                Utc::now().naive_utc(),
                TransactionDirection::Outbound,
                None,
                None,
                None,
            )?,
        )
        .await?;
        info!(target: LOG_TARGET, "Submitted burning transaction - TxId: {}", tx_id);

        Ok((tx_id, BurntProof {
            // Key used to claim the burn on L2
            reciprocal_claim_public_key: public_spend_key,
            commitment,
            ownership_proof,
            range_proof,
        }))
    }

    pub async fn register_validator_node(
        &mut self,
        amount: MicroMinotari,
        validator_node_public_key: CommsPublicKey,
        validator_node_signature: Signature,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        message: String,
        join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TransactionSendResult, TransactionServiceProtocolError<TxId>>>,
        >,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        reply_channel: oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
    ) -> Result<(), TransactionServiceError> {
        let output_features =
            OutputFeatures::for_validator_node_registration(validator_node_public_key, validator_node_signature);
        self.send_transaction(
            self.resources.wallet_identity.address.clone(),
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            message,
            TransactionMetadata::default(),
            join_handles,
            transaction_broadcast_join_handles,
            reply_channel,
        )
        .await
    }

    pub async fn register_code_template(
        &mut self,
        fee_per_gram: MicroMinotari,
        template_registration: CodeTemplateRegistration,
        selection_criteria: UtxoSelectionCriteria,
        message: String,
        join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TransactionSendResult, TransactionServiceProtocolError<TxId>>>,
        >,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        reply_channel: oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
    ) -> Result<(), TransactionServiceError> {
        self.send_transaction(
            self.resources.wallet_identity.address.clone(),
            0.into(),
            selection_criteria,
            OutputFeatures::for_template_registration(template_registration),
            fee_per_gram,
            message,
            TransactionMetadata::default(),
            join_handles,
            transaction_broadcast_join_handles,
            reply_channel,
        )
        .await
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
        message: String,
        payment_id: PaymentId,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        if destination.network() != self.resources.wallet_identity.address.network() {
            return Err(TransactionServiceError::InvalidNetwork);
        }

        self.send_one_sided_or_stealth(
            destination,
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            message,
            transaction_broadcast_join_handles,
            None, // The stealth address for the script will be calculated in the next step
            payment_id,
        )
        .await
    }

    /// Accept the public reply from a recipient and apply the reply to the relevant transaction protocol
    /// # Arguments
    /// 'recipient_reply' - The public response from a recipient with data required to complete the transaction
    #[allow(clippy::too_many_lines)]
    pub async fn accept_recipient_reply(
        &mut self,
        source_pubkey: CommsPublicKey,
        recipient_reply: Result<proto::RecipientSignedMessage, prost::DecodeError>,
    ) -> Result<(), TransactionServiceError> {
        // Check if a wallet recovery is in progress, if it is we will ignore this request
        self.check_recovery_status()?;

        if let Err(e) = recipient_reply {
            // We should ban but there is no banning in the wallet...
            return Err(TransactionServiceError::InvalidMessageError(format!(
                "Could not decode RecipientSignedMessage: {:?}",
                e
            )));
        }
        let recipient_reply: RecipientSignedMessage = recipient_reply
            .unwrap()
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let tx_id = recipient_reply.tx_id;

        // First we check if this Reply is for a cancelled Pending Outbound Tx or a Completed Tx
        let cancelled_outbound_tx = self.db.get_cancelled_pending_outbound_transaction(tx_id);
        let completed_tx = self.db.get_completed_transaction_cancelled_or_not(tx_id);

        // This closure will check if the timestamps are beyond the cooldown period
        let check_cooldown = |timestamp: Option<NaiveDateTime>| {
            if let Some(t) = timestamp {
                // Check if the last reply is beyond the resend cooldown
                if let Ok(elapsed_time) = Utc::now().naive_utc().signed_duration_since(t).to_std() {
                    if elapsed_time < self.resources.config.resend_response_cooldown {
                        trace!(
                            target: LOG_TARGET,
                            "A repeated Transaction Reply (TxId: {}) has been received before the resend cooldown has \
                             expired. Ignoring.",
                            tx_id
                        );
                        return false;
                    }
                }
            }
            true
        };

        if let Ok(ctx) = completed_tx {
            // Check that it is from the same person
            if ctx.destination_address.comms_public_key() != &source_pubkey {
                return Err(TransactionServiceError::InvalidSourcePublicKey);
            }
            if !check_cooldown(ctx.last_send_timestamp) {
                return Ok(());
            }

            if ctx.cancelled.is_some() {
                // Send a cancellation message
                debug!(
                    target: LOG_TARGET,
                    "A repeated Transaction Reply (TxId: {}) has been received for cancelled completed transaction. \
                     Transaction Cancelled response is being sent.",
                    tx_id
                );
                tokio::spawn(send_transaction_cancelled_message(
                    tx_id,
                    source_pubkey,
                    self.resources.outbound_message_service.clone(),
                ));
            } else {
                // Resend the reply
                debug!(
                    target: LOG_TARGET,
                    "A repeated Transaction Reply (TxId: {}) has been received. Reply is being resent.", tx_id
                );
                tokio::spawn(send_finalized_transaction_message(
                    tx_id,
                    ctx.transaction,
                    source_pubkey,
                    self.resources.outbound_message_service.clone(),
                    self.resources.config.direct_send_timeout,
                    self.resources.config.transaction_routing_mechanism,
                ));
            }

            if let Err(e) = self.resources.db.increment_send_count(tx_id) {
                warn!(
                    target: LOG_TARGET,
                    "Could not increment send count for completed transaction TxId {}: {:?}", tx_id, e
                );
            }
            return Ok(());
        }

        if let Ok(otx) = cancelled_outbound_tx {
            // Check that it is from the same person
            if otx.destination_address.comms_public_key() != &source_pubkey {
                return Err(TransactionServiceError::InvalidSourcePublicKey);
            }
            if !check_cooldown(otx.last_send_timestamp) {
                return Ok(());
            }

            // Send a cancellation message
            debug!(
                target: LOG_TARGET,
                "A repeated Transaction Reply (TxId: {}) has been received for cancelled pending outbound \
                 transaction. Transaction Cancelled response is being sent.",
                tx_id
            );
            tokio::spawn(send_transaction_cancelled_message(
                tx_id,
                source_pubkey,
                self.resources.outbound_message_service.clone(),
            ));

            if let Err(e) = self.resources.db.increment_send_count(tx_id) {
                warn!(
                    target: LOG_TARGET,
                    "Could not increment send count for completed transaction TxId {}: {:?}", tx_id, e
                );
            }
            return Ok(());
        }

        // Is this a new Transaction Reply for an existing pending transaction?
        let sender = match self.pending_transaction_reply_senders.get_mut(&tx_id) {
            None => return Err(TransactionServiceError::TransactionDoesNotExistError),
            Some(s) => s,
        };

        sender
            .send((source_pubkey, recipient_reply))
            .await
            .map_err(|_| TransactionServiceError::ProtocolChannelError)?;

        Ok(())
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
                if val.transaction_status != TransactionStatus::Queued {
                    let _sender = self.pending_transaction_reply_senders.remove(&val.tx_id);
                    let _sender = self.send_transaction_cancellation_senders.remove(&val.tx_id);
                    let completed_tx = match self.db.get_completed_transaction(val.tx_id) {
                        Ok(v) => v,
                        Err(e) => {
                            error!(
                                target: LOG_TARGET,
                                "Error starting Broadcast Protocol after completed Send Transaction Protocol: {:?}", e
                            );
                            return;
                        },
                    };
                    let _result = self
                        .broadcast_completed_transaction(completed_tx, transaction_broadcast_join_handles)
                        .map_err(|resp| {
                            error!(
                                target: LOG_TARGET,
                                "Error starting Broadcast Protocol after completed Send Transaction Protocol: {:?}",
                                resp
                            );
                            resp
                        });
                } else if val.transaction_status == TransactionStatus::Queued {
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
                let _public_key = self.pending_transaction_reply_senders.remove(&id);
                let _result = self.send_transaction_cancellation_senders.remove(&id);
                if let TransactionServiceError::Shutdown = error {
                    return;
                }
                warn!(
                    target: LOG_TARGET,
                    "Error completing Send Transaction Protocol (Id: {}): {:?}", id, error
                );
                let _size = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    /// Cancel a pending transaction
    async fn cancel_pending_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        self.db.cancel_pending_transaction(tx_id).map_err(|e| {
            warn!(
                target: LOG_TARGET,
                "Pending Transaction does not exist and could not be cancelled: {:?}", e
            );
            e
        })?;

        self.resources.output_manager_service.cancel_transaction(tx_id).await?;

        if let Some(cancellation_sender) = self.send_transaction_cancellation_senders.remove(&tx_id) {
            let _result = cancellation_sender.send(());
        }
        let _public_key = self.pending_transaction_reply_senders.remove(&tx_id);

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
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event because there are no subscribers: {:?}",
                    e
                );
                e
            });

        info!(target: LOG_TARGET, "Pending Transaction (TxId: {}) cancelled", tx_id);

        Ok(())
    }

    /// Handle a Transaction Cancelled message received from the Comms layer
    pub async fn handle_transaction_cancelled_message(
        &mut self,
        source_pubkey: CommsPublicKey,
        transaction_cancelled: Result<proto::TransactionCancelledMessage, prost::DecodeError>,
    ) -> Result<(), TransactionServiceError> {
        let transaction_cancelled = match transaction_cancelled {
            Ok(v) => v,
            Err(e) => {
                // Should ban....
                return Err(TransactionServiceError::InvalidMessageError(format!(
                    "Could not decode TransactionCancelledMessage: {:?}",
                    e
                )));
            },
        };
        let tx_id = transaction_cancelled.tx_id.into();

        // Check that an inbound transaction exists to be cancelled and that the Source Public key for that transaction
        // is the same as the cancellation message
        if let Ok(inbound_tx) = self.db.get_pending_inbound_transaction(tx_id) {
            if inbound_tx.source_address.comms_public_key() == &source_pubkey {
                self.cancel_pending_transaction(tx_id).await?;
            } else {
                trace!(
                    target: LOG_TARGET,
                    "Received a Transaction Cancelled (TxId: {}) message from an unknown source, ignoring",
                    tx_id
                );
            }
        }

        Ok(())
    }

    #[allow(clippy::map_entry)]
    fn restart_all_send_transaction_protocols(
        &mut self,
        join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TransactionSendResult, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<(), TransactionServiceError> {
        let outbound_txs = self.db.get_pending_outbound_transactions()?;
        for (tx_id, tx) in outbound_txs {
            let (sender_protocol, stage) = if tx.send_count > 0 {
                (None, TransactionSendProtocolStage::WaitForReply)
            } else {
                (Some(tx.sender_protocol), TransactionSendProtocolStage::Queued)
            };
            let (not_yet_pending, queued) = (
                !self.pending_transaction_reply_senders.contains_key(&tx_id),
                stage == TransactionSendProtocolStage::Queued,
            );

            if not_yet_pending {
                debug!(
                    target: LOG_TARGET,
                    "Restarting listening for Reply for Pending Outbound Transaction TxId: {}", tx_id
                );
            } else if queued {
                debug!(
                    target: LOG_TARGET,
                    "Retry sending queued Pending Outbound Transaction TxId: {}", tx_id
                );
                let _sender = self.pending_transaction_reply_senders.remove(&tx_id);
                let _sender = self.send_transaction_cancellation_senders.remove(&tx_id);
            } else {
                // dont care
            }

            if not_yet_pending || queued {
                let (tx_reply_sender, tx_reply_receiver) = mpsc::channel(100);
                let (cancellation_sender, cancellation_receiver) = oneshot::channel();
                self.pending_transaction_reply_senders.insert(tx_id, tx_reply_sender);
                self.send_transaction_cancellation_senders
                    .insert(tx_id, cancellation_sender);

                let protocol = TransactionSendProtocol::new(
                    tx_id,
                    self.resources.clone(),
                    tx_reply_receiver,
                    cancellation_receiver,
                    tx.destination_address,
                    tx.amount,
                    tx.fee,
                    tx.message,
                    TransactionMetadata::default(),
                    None,
                    stage,
                    sender_protocol,
                );

                let join_handle = tokio::spawn(protocol.execute());
                join_handles.push(join_handle);
            }
        }

        Ok(())
    }

    /// Accept a new transaction from a sender by handling a public SenderMessage. The reply is generated and sent.
    /// # Arguments
    /// 'source_pubkey' - The pubkey from which the message was sent and to which the reply will be sent.
    /// 'sender_message' - Message from a sender containing the setup of the transaction being sent to you
    #[allow(clippy::too_many_lines)]
    pub fn accept_transaction(
        &mut self,
        source_pubkey: CommsPublicKey,
        sender_message: Result<proto::TransactionSenderMessage, prost::DecodeError>,
        traced_message_tag: u64,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>>,
    ) -> Result<(), TransactionServiceError> {
        // Check if a wallet recovery is in progress, if it is we will ignore this request
        self.check_recovery_status()?;

        if let Err(e) = sender_message {
            return Err(TransactionServiceError::InvalidMessageError(format!(
                "Could not decode TransactionSenderMessage: {:?}",
                e
            )));
        }
        let sender_message: TransactionSenderMessage = sender_message
            .unwrap()
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        // Currently we will only reply to a Single sender transaction protocol
        if let TransactionSenderMessage::Single(data) = sender_message.clone() {
            trace!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) received from {}, Trace: {}",
                data.tx_id,
                source_pubkey,
                traced_message_tag
            );

            // Check if this transaction has already been received and cancelled.
            if let Ok(Some(any_tx)) = self.db.get_any_cancelled_transaction(data.tx_id) {
                let tx = CompletedTransaction::from(any_tx);

                if tx.source_address.comms_public_key() != &source_pubkey {
                    return Err(TransactionServiceError::InvalidSourcePublicKey);
                }
                trace!(
                    target: LOG_TARGET,
                    "A repeated Transaction (TxId: {}) has been received but has been previously cancelled or rejected",
                    tx.tx_id
                );
                tokio::spawn(send_transaction_cancelled_message(
                    tx.tx_id,
                    source_pubkey,
                    self.resources.outbound_message_service.clone(),
                ));

                return Ok(());
            }

            // Check if this transaction has already been received.
            if let Ok(inbound_tx) = self.db.get_pending_inbound_transaction(data.tx_id) {
                // Check that it is from the same person
                if inbound_tx.source_address.comms_public_key() != &source_pubkey {
                    return Err(TransactionServiceError::InvalidSourcePublicKey);
                }
                // Check if the last reply is beyond the resend cooldown
                if let Some(timestamp) = inbound_tx.last_send_timestamp {
                    let elapsed_time = utc_duration_since(&timestamp)?;
                    if elapsed_time < self.resources.config.resend_response_cooldown {
                        trace!(
                            target: LOG_TARGET,
                            "A repeated Transaction (TxId: {}) has been received before the resend cooldown has \
                             expired. Ignoring.",
                            inbound_tx.tx_id
                        );
                        return Ok(());
                    }
                }
                debug!(
                    target: LOG_TARGET,
                    "A repeated Transaction (TxId: {}) has been received. Reply is being resent.", inbound_tx.tx_id
                );
                let tx_id = inbound_tx.tx_id;
                // Ok we will resend the reply
                tokio::spawn(send_transaction_reply(
                    inbound_tx,
                    self.resources.outbound_message_service.clone(),
                    self.resources.config.direct_send_timeout,
                    self.resources.config.transaction_routing_mechanism,
                ));
                if let Err(e) = self.resources.db.increment_send_count(tx_id) {
                    warn!(
                        target: LOG_TARGET,
                        "Could not increment send count for inbound transaction TxId {}: {:?}", tx_id, e
                    );
                }

                return Ok(());
            }

            if self.finalized_transaction_senders.contains_key(&data.tx_id) ||
                self.receiver_transaction_cancellation_senders.contains_key(&data.tx_id)
            {
                trace!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) has already been received, this is probably a repeated message, Trace:
                {}.",
                    data.tx_id,
                    traced_message_tag
                );
                return Err(TransactionServiceError::RepeatedMessageError);
            }

            let (tx_finalized_sender, tx_finalized_receiver) = mpsc::channel(100);
            let (cancellation_sender, cancellation_receiver) = oneshot::channel();
            self.finalized_transaction_senders
                .insert(data.tx_id, tx_finalized_sender);
            self.receiver_transaction_cancellation_senders
                .insert(data.tx_id, cancellation_sender);
            // We are recieving an interactive transaction from someone on our network, so we assume its features are
            // interactive and its the same network
            let source_address = TariAddress::new_single_address(
                source_pubkey,
                self.resources.wallet_identity.network(),
                TariAddressFeatures::INTERACTIVE,
            );
            let protocol = TransactionReceiveProtocol::new(
                data.tx_id,
                source_address,
                sender_message,
                TransactionReceiveProtocolStage::Initial,
                self.resources.clone(),
                tx_finalized_receiver,
                cancellation_receiver,
            );

            let join_handle = tokio::spawn(protocol.execute());
            join_handles.push(join_handle);
            Ok(())
        } else {
            Err(TransactionServiceError::InvalidStateError)
        }
    }

    /// Accept the public reply from a recipient and apply the reply to the relevant transaction protocol
    /// # Arguments
    /// 'recipient_reply' - The public response from a recipient with data required to complete the transaction
    pub async fn accept_finalized_transaction(
        &mut self,
        source_pubkey: CommsPublicKey,
        finalized_transaction: Result<proto::TransactionFinalizedMessage, prost::DecodeError>,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>>,
    ) -> Result<(), TransactionServiceError> {
        // Check if a wallet recovery is in progress, if it is we will ignore this request
        self.check_recovery_status()?;

        if let Err(e) = finalized_transaction {
            // Should ban but there is no banning in the wallet...
            return Err(TransactionServiceError::InvalidMessageError(format!(
                "Could not decode TransactionFinalizedMessage: {:?}",
                e
            )));
        }
        let finalized_transaction = finalized_transaction.unwrap();
        let tx_id = finalized_transaction.tx_id.into();
        let transaction: Transaction = finalized_transaction
            .transaction
            .ok_or_else(|| {
                TransactionServiceError::InvalidMessageError(
                    "Finalized Transaction missing Transaction field".to_string(),
                )
            })?
            .try_into()
            .map_err(|_| {
                TransactionServiceError::InvalidMessageError(
                    "Cannot convert Transaction field from TransactionFinalized message".to_string(),
                )
            })?;

        // assuming since we talked to the node, that it has an interactive address, we dont know what the view key is
        // but we know its interactive, so make the view key 0, and the spend key the source public key.
        let source_address = TariAddress::new_single_address(
            source_pubkey,
            self.resources.wallet_identity.address.network(),
            TariAddressFeatures::INTERACTIVE,
        );
        let sender = match self.finalized_transaction_senders.get_mut(&tx_id) {
            None => {
                // First check if perhaps we know about this inbound transaction but it was cancelled
                match self.db.get_cancelled_pending_inbound_transaction(tx_id) {
                    Ok(t) => {
                        if t.source_address != source_address {
                            debug!(
                                target: LOG_TARGET,
                                "Received Finalized Transaction for a cancelled pending Inbound Transaction (TxId: \
                                 {}) but Source Public Key did not match",
                                tx_id
                            );
                            return Err(TransactionServiceError::TransactionDoesNotExistError);
                        }
                        info!(
                            target: LOG_TARGET,
                            "Received Finalized Transaction for a cancelled pending Inbound Transaction (TxId: {}). \
                             Restarting protocol",
                            tx_id
                        );
                        self.db.uncancel_pending_transaction(tx_id)?;
                        self.resources
                            .output_manager_service
                            .reinstate_cancelled_inbound_transaction_outputs(tx_id)
                            .await?;
                        self.restart_receive_transaction_protocol(tx_id, source_address.clone(), join_handles);
                        match self.finalized_transaction_senders.get_mut(&tx_id) {
                            None => return Err(TransactionServiceError::TransactionDoesNotExistError),
                            Some(s) => s,
                        }
                    },
                    Err(_) => return Err(TransactionServiceError::TransactionDoesNotExistError),
                }
            },
            Some(s) => s,
        };

        sender
            .send((source_address, tx_id, transaction))
            .await
            .map_err(|_| TransactionServiceError::ProtocolChannelError)?;

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
                            "Error broadcasting completed transaction TxId: {} to mempool: {:?}", id, e
                        );
                        return;
                    },
                };
                let _result = self
                    .broadcast_completed_transaction(completed_tx, transaction_broadcast_join_handles)
                    .map_err(|e| {
                        warn!(
                            target: LOG_TARGET,
                            "Error broadcasting completed transaction TxId: {} to mempool: {:?}", id, e
                        );
                        e
                    });

                trace!(
                    target: LOG_TARGET,
                    "Receive Transaction Protocol for TxId: {} completed successfully",
                    id
                );
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _public_key = self.finalized_transaction_senders.remove(&id);
                let _result = self.receiver_transaction_cancellation_senders.remove(&id);
                match error {
                    TransactionServiceError::RepeatedMessageError => debug!(
                        target: LOG_TARGET,
                        "Receive Transaction Protocol (Id: {}) aborted as it is a repeated transaction that has \
                         already been processed",
                        id
                    ),
                    TransactionServiceError::Shutdown => {
                        return;
                    },
                    _ => warn!(
                        target: LOG_TARGET,
                        "Error completing Receive Transaction Protocol (Id: {}): {}", id, error
                    ),
                }

                let _size = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    fn restart_all_receive_transaction_protocols(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>>,
    ) -> Result<(), TransactionServiceError> {
        let inbound_txs = self.db.get_pending_inbound_transaction_sender_info()?;
        for txn in inbound_txs {
            self.restart_receive_transaction_protocol(txn.tx_id, txn.source_address, join_handles);
        }

        Ok(())
    }

    fn restart_receive_transaction_protocol(
        &mut self,
        tx_id: TxId,
        source_address: TariAddress,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>>,
    ) {
        if !self.pending_transaction_reply_senders.contains_key(&tx_id) {
            debug!(
                target: LOG_TARGET,
                "Restarting listening for Transaction Finalize for Pending Inbound Transaction TxId: {}", tx_id
            );
            let (tx_finalized_sender, tx_finalized_receiver) = mpsc::channel(100);
            let (cancellation_sender, cancellation_receiver) = oneshot::channel();
            self.finalized_transaction_senders.insert(tx_id, tx_finalized_sender);
            self.receiver_transaction_cancellation_senders
                .insert(tx_id, cancellation_sender);
            let protocol = TransactionReceiveProtocol::new(
                tx_id,
                source_address,
                TransactionSenderMessage::None,
                TransactionReceiveProtocolStage::WaitForFinalize,
                self.resources.clone(),
                tx_finalized_receiver,
                cancellation_receiver,
            );

            let join_handle = tokio::spawn(protocol.execute());
            join_handles.push(join_handle);
        }
    }

    fn restart_transaction_negotiation_protocols(
        &mut self,
        send_transaction_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TransactionSendResult, TransactionServiceProtocolError<TxId>>>,
        >,
        receive_transaction_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<(), TransactionServiceError> {
        trace!(target: LOG_TARGET, "Restarting transaction negotiation protocols");
        self.restart_all_send_transaction_protocols(send_transaction_join_handles)
            .map_err(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Error restarting protocols for all pending outbound transactions: {:?}", resp
                );
                resp
            })?;

        self.restart_all_receive_transaction_protocols(receive_transaction_join_handles)
            .map_err(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Error restarting protocols for all pending inbound transactions: {:?}", resp
                );
                resp
            })?;

        Ok(())
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
        let current_base_node = self
            .resources
            .connectivity
            .get_current_base_node_id()
            .ok_or(TransactionServiceError::NoBaseNodeKeysProvided)?;

        trace!(target: LOG_TARGET, "Starting transaction validation protocol");
        let id = OperationId::new_random();

        let protocol = TransactionValidationProtocol::new(
            id,
            self.resources.db.clone(),
            self.resources.connectivity.clone(),
            self.resources.config.clone(),
            self.event_publisher.clone(),
        );

        let mut base_node_watch = self.connectivity().get_current_base_node_watcher();
        let validation_in_progress = self.validation_in_progress.clone();
        let join_handle = tokio::spawn(async move {
            let mut _lock = validation_in_progress.try_lock().map_err(|_| {
                debug!(
                    target: LOG_TARGET,
                    "Transaction Validation Protocol (Id: {}) spawned while a previous protocol was busy, ignored", id
                );
                TransactionServiceProtocolError::new(id, TransactionServiceError::TransactionValidationInProgress)
            })?;
            let exec_fut = protocol.execute();
            tokio::pin!(exec_fut);
            loop {
                tokio::select! {
                    result = &mut exec_fut => {
                       return result;
                    },
                    _ = base_node_watch.changed() => {
                         if let Some(peer) = base_node_watch.borrow().as_ref() {
                            if peer.node_id != current_base_node {
                                debug!(target: LOG_TARGET, "Base node changed, exiting transaction validation protocol");
                                return Err(TransactionServiceProtocolError::new(id, TransactionServiceError::BaseNodeChanged {
                                    task_name: "transaction validation_protocol",
                                }));
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
                    "Transaction Validation Protocol (Id: {}) completed successfully", id
                );
                // Restart broadcast protocols for any transactions that were found to be no longer mined.
                let _ = self
                    .restart_broadcast_protocols(transaction_broadcast_join_handles)
                    .map_err(|e| warn!(target: LOG_TARGET, "Error restarting broadcast protocols: {}", e));
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                if let TransactionServiceError::Shutdown = error {
                    return;
                }
                warn!(
                    target: LOG_TARGET,
                    "Error completing Transaction Validation Protocol (id: {}): {:?}", id, error
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
        if !self.connectivity().is_base_node_set() {
            return Err(TransactionServiceError::NoBaseNodeKeysProvided);
        }

        trace!(target: LOG_TARGET, "Restarting transaction broadcast protocols");
        self.broadcast_completed_and_broadcast_transactions(broadcast_join_handles)
            .map_err(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Error broadcasting all valid and not cancelled Completed Transactions with status 'Completed' \
                     and 'Broadcast': {:?}",
                    resp
                );
                resp
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
        if !(completed_tx.status == TransactionStatus::Completed ||
            completed_tx.status == TransactionStatus::Broadcast ||
            completed_tx.status == TransactionStatus::MinedUnconfirmed) ||
            completed_tx.transaction.body.kernels().is_empty()
        {
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }

        if !self.resources.connectivity.is_base_node_set() {
            return Err(TransactionServiceError::NoBaseNodeKeysProvided);
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
                "Transaction Broadcast Protocol (TxId: {}) already started",
                tx_id
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
                    "Transaction Broadcast Protocol for TxId: {} completed successfully", id
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
                    "Error completing Transaction Broadcast Protocol (Id: {}): {:?}", id, error
                );
                let _size = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    /// Handle an incoming basenode response message
    pub async fn handle_base_node_response(
        &mut self,
        response: Result<base_node_proto::BaseNodeServiceResponse, prost::DecodeError>,
    ) -> Result<(), TransactionServiceError> {
        if let Err(e) = response {
            // Should we switch base nodes?
            return Err(TransactionServiceError::InvalidMessageError(format!(
                "Could not decode BaseNodeServiceResponse: {:?}",
                e
            )));
        }
        let response = response.unwrap();
        let sender = match self.base_node_response_senders.get_mut(&response.request_key.into()) {
            None => {
                trace!(
                    target: LOG_TARGET,
                    "Received Base Node response with unexpected key: {}. Not for this service",
                    response.request_key
                );
                return Ok(());
            },
            Some((_, s)) => s,
        };
        sender
            .send(response.clone())
            .await
            .map_err(|_| TransactionServiceError::ProtocolChannelError)?;

        Ok(())
    }

    async fn set_power_mode(&mut self, mode: PowerMode) -> Result<(), TransactionServiceError> {
        let timeout = match mode {
            PowerMode::Low => self.config.low_power_polling_timeout,
            PowerMode::Normal => self.config.broadcast_monitoring_timeout,
        };
        self.timeout_update_watch.send(timeout);

        Ok(())
    }

    /// Add a completed transaction to the Transaction Manager to record directly importing a spendable UTXO.
    pub async fn add_utxo_import_transaction_with_status(
        &mut self,
        value: MicroMinotari,
        source_address: TariAddress,
        message: String,
        import_status: ImportStatus,
        tx_id: Option<TxId>,
        current_height: Option<u64>,
        mined_timestamp: Option<NaiveDateTime>,
        scanned_output: TransactionOutput,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        let tx_id = if let Some(id) = tx_id { id } else { TxId::new_random() };
        self.db.add_utxo_import_transaction_with_status(
            tx_id,
            value,
            source_address,
            self.resources.wallet_identity.address.clone(),
            message,
            import_status.clone(),
            current_height,
            mined_timestamp,
            scanned_output,
            payment_id,
        )?;
        let transaction_event = match import_status {
            ImportStatus::Imported => TransactionEvent::DetectedTransactionUnconfirmed {
                tx_id,
                num_confirmations: 0,
                is_valid: true,
            },
            ImportStatus::OneSidedUnconfirmed | ImportStatus::CoinbaseUnconfirmed => {
                TransactionEvent::DetectedTransactionUnconfirmed {
                    tx_id,
                    num_confirmations: 0,
                    is_valid: true,
                }
            },
            ImportStatus::OneSidedConfirmed | ImportStatus::CoinbaseConfirmed => {
                TransactionEvent::DetectedTransactionConfirmed { tx_id, is_valid: true }
            },
        };
        let _size = self.event_publisher.send(Arc::new(transaction_event)).map_err(|e| {
            trace!(
                target: LOG_TARGET,
                "Error sending event, usually because there are no subscribers: {:?}",
                e
            );
            e
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
        trace!(target: LOG_TARGET, "Submit transaction ({}) to db.", tx_id);
        self.db
            .insert_completed_transaction(tx_id, completed_transaction.clone())?;
        trace!(
            target: LOG_TARGET,
            "Launch the transaction broadcast protocol for submitted transaction ({}).",
            tx_id
        );
        if let Err(e) = check_transaction_size(&completed_transaction.transaction, tx_id) {
            self.cancel_transaction(tx_id, TxCancellationReason::Oversized).await;
            return Err(e.into());
        }
        self.complete_send_transaction_protocol(
            Ok(TransactionSendResult {
                tx_id,
                transaction_status: TransactionStatus::Completed,
            }),
            transaction_broadcast_join_handles,
        );
        Ok(())
    }

    async fn cancel_transaction(&mut self, tx_id: TxId, reason: TxCancellationReason) {
        if let Err(e) = self.resources.output_manager_service.cancel_transaction(tx_id).await {
            warn!(
                target: LOG_TARGET,
                "Failed to Cancel outputs for TxId: {} after failed sending attempt with error {:?}", tx_id, e
            );
        }
        if let Err(e) = self.resources.db.reject_completed_transaction(tx_id, reason) {
            warn!(
                target: LOG_TARGET,
                "Failed to Cancel TxId: {} after failed sending attempt with error {:?}", tx_id, e
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
        message: String,
    ) -> Result<(), TransactionServiceError> {
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new(
                tx_id,
                self.resources.wallet_identity.address.clone(),
                self.resources.wallet_identity.address.clone(),
                amount,
                fee,
                tx,
                TransactionStatus::Completed,
                message,
                Utc::now().naive_utc(),
                TransactionDirection::Inbound,
                None,
                None,
                None,
            )?,
        )
        .await?;
        Ok(())
    }

    /// Check if a Recovery Status is currently stored in the databse, this indicates that a wallet recovery is in
    /// progress
    fn check_recovery_status(&self) -> Result<(), TransactionServiceError> {
        let value = self.wallet_db.get_client_key_value(RECOVERY_KEY.to_owned())?;
        match value {
            None => Ok(()),
            Some(_) => Err(TransactionServiceError::WalletRecoveryInProgress),
        }
    }

    fn connectivity(&self) -> &TWalletConnectivity {
        &self.resources.connectivity
    }
}

/// This struct is a collection of the common resources that a protocol in the service requires.
#[derive(Clone)]
pub struct TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    pub db: TransactionDatabase<TBackend>,
    pub output_manager_service: OutputManagerHandle,
    pub transaction_key_manager_service: TKeyManagerInterface,
    pub outbound_message_service: OutboundMessageRequester,
    pub connectivity: TWalletConnectivity,
    pub event_publisher: TransactionEventSender,
    pub wallet_identity: WalletIdentity,
    pub consensus_manager: ConsensusManager,
    pub factories: CryptoFactories,
    pub config: TransactionServiceConfig,
    pub shutdown_signal: ShutdownSignal,
}

#[derive(Default, Clone, Copy)]
enum PowerMode {
    Low,
    #[default]
    Normal,
}

/// Contains the generated TxId and TransactionStatus transaction send result
#[derive(Debug)]
pub struct TransactionSendResult {
    pub tx_id: TxId,
    pub transaction_status: TransactionStatus,
}
