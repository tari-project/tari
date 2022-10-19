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
use tari_common_types::{
    transaction::{ImportStatus, TransactionDirection, TransactionStatus, TxId},
    types::{PrivateKey, PublicKey},
};
use tari_comms::{peer_manager::NodeIdentity, types::CommsPublicKey};
use tari_comms_dht::outbound::OutboundMessageRequester;
use tari_core::{
    covenants::Covenant,
    mempool::FeePerGramStat,
    proto::base_node as base_node_proto,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{
            EncryptedValue,
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionOutput,
            UnblindedOutput,
        },
        transaction_protocol::{
            proto::protocol as proto,
            recipient::RecipientSignedMessage,
            sender::TransactionSenderMessage,
            RewindData,
            TransactionMetadata,
        },
        CryptoFactories,
        ReceiverTransactionProtocol,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{DiffieHellmanSharedSecret, PublicKey as PKtrait, SecretKey},
    tari_utilities::ByteArray,
};
use tari_p2p::domain_message::DomainMessage;
use tari_script::{inputs, script, TariScript};
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
            transaction_broadcast_protocol::TransactionBroadcastProtocol,
            transaction_receive_protocol::{TransactionReceiveProtocol, TransactionReceiveProtocolStage},
            transaction_send_protocol::{TransactionSendProtocol, TransactionSendProtocolStage},
            transaction_validation_protocol::TransactionValidationProtocol,
        },
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::{CompletedTransaction, TxCancellationReason},
        },
        tasks::{
            check_faux_transaction_status::check_faux_transactions,
            send_finalized_transaction::send_finalized_transaction_message,
            send_transaction_cancelled::send_transaction_cancelled_message,
            send_transaction_reply::send_transaction_reply,
        },
        utc::utc_duration_since,
    },
    types::WalletHasher,
    util::watch::Watch,
    utxo_scanner_service::RECOVERY_KEY,
    OperationId,
    WalletSecretKeysDomainHasher,
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
> {
    config: TransactionServiceConfig,
    db: TransactionDatabase<TBackend>,
    output_manager_service: OutputManagerHandle,
    transaction_stream: Option<TTxStream>,
    transaction_reply_stream: Option<TTxReplyStream>,
    transaction_finalized_stream: Option<TTxFinalizedStream>,
    base_node_response_stream: Option<BNResponseStream>,
    transaction_cancelled_stream: Option<TTxCancelledStream>,
    request_stream: Option<
        reply_channel::Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    >,
    event_publisher: TransactionEventSender,
    node_identity: Arc<NodeIdentity>,
    resources: TransactionServiceResources<TBackend, TWalletConnectivity>,
    pending_transaction_reply_senders: HashMap<TxId, Sender<(CommsPublicKey, RecipientSignedMessage)>>,
    base_node_response_senders: HashMap<TxId, (TxId, Sender<base_node_proto::BaseNodeServiceResponse>)>,
    send_transaction_cancellation_senders: HashMap<TxId, oneshot::Sender<()>>,
    finalized_transaction_senders: HashMap<TxId, Sender<(CommsPublicKey, TxId, Transaction)>>,
    receiver_transaction_cancellation_senders: HashMap<TxId, oneshot::Sender<()>>,
    active_transaction_broadcast_protocols: HashSet<TxId>,
    timeout_update_watch: Watch<Duration>,
    wallet_db: WalletDatabase<TWalletBackend>,
    base_node_service: BaseNodeServiceHandle,
    last_seen_tip_height: Option<u64>,
    validation_in_progress: Arc<Mutex<()>>,
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
    >
where
    TTxStream: Stream<Item = DomainMessage<proto::TransactionSenderMessage>>,
    TTxReplyStream: Stream<Item = DomainMessage<proto::RecipientSignedMessage>>,
    TTxFinalizedStream: Stream<Item = DomainMessage<proto::TransactionFinalizedMessage>>,
    BNResponseStream: Stream<Item = DomainMessage<base_node_proto::BaseNodeServiceResponse>>,
    TTxCancelledStream: Stream<Item = DomainMessage<proto::TransactionCancelledMessage>>,
    TBackend: TransactionBackend + 'static,
    TWalletBackend: WalletBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
{
    pub fn new(
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
        outbound_message_service: OutboundMessageRequester,
        connectivity: TWalletConnectivity,
        event_publisher: TransactionEventSender,
        node_identity: Arc<NodeIdentity>,
        factories: CryptoFactories,
        shutdown_signal: ShutdownSignal,
        base_node_service: BaseNodeServiceHandle,
    ) -> Self {
        // Collect the resources that all protocols will need so that they can be neatly cloned as the protocols are
        // spawned.
        let resources = TransactionServiceResources {
            db: db.clone(),
            output_manager_service: output_manager_service.clone(),
            outbound_message_service,
            connectivity,
            event_publisher: event_publisher.clone(),
            node_identity: node_identity.clone(),
            factories,
            config: config.clone(),

            shutdown_signal,
        };
        let power_mode = PowerMode::default();
        let timeout = match power_mode {
            PowerMode::Low => config.low_power_polling_timeout,
            PowerMode::Normal => config.broadcast_monitoring_timeout,
        };
        let timeout_update_watch = Watch::new(timeout);

        Self {
            config,
            db,
            output_manager_service,
            transaction_stream: Some(transaction_stream),
            transaction_reply_stream: Some(transaction_reply_stream),
            transaction_finalized_stream: Some(transaction_finalized_stream),
            base_node_response_stream: Some(base_node_response_stream),
            transaction_cancelled_stream: Some(transaction_cancelled_stream),
            request_stream: Some(request_stream),
            event_publisher,
            node_identity,
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
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn start(mut self) -> Result<(), TransactionServiceError> {
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
        let mut output_manager_event_stream = self.output_manager_service.get_event_stream();

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
                    // TODO: Remove time measurements; this is to aid in system testing only #LOGGED
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
                    // TODO: Remove time measurements; this is to aid in system testing only #LOGGED
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
                                e, self.node_identity.node_id().short_str(), msg.dht_header.message_tag);
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
                    // TODO: Remove time measurements; this is to aid in system testing only  #LOGGED
                    let start = Instant::now();
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Transaction Reply Message, Trace: {}", msg.dht_header.message_tag);
                    let result = self.accept_recipient_reply(origin_public_key, inner_msg).await;

                    match result {
                        Err(TransactionServiceError::TransactionDoesNotExistError) => {
                            trace!(target: LOG_TARGET, "Unable to handle incoming Transaction Reply message from NodeId: \
                            {} due to Transaction not existing. This usually means the message was a repeated message \
                            from Store and Forward, Trace: {}", self.node_identity.node_id().short_str(),
                            msg.dht_header.message_tag);
                        },
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to handle incoming Transaction Reply message: {} \
                            for NodeId: {}, Trace: {}", e, self.node_identity.node_id().short_str(),
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
                    // TODO: Remove time measurements; this is to aid in system testing only  #LOGGED
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
                            from Store and Forward, Trace: {}", self.node_identity.node_id().short_str(),
                            msg.dht_header.message_tag);
                        },
                       Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to handle incoming Transaction Finalized message: {} \
                            for NodeID: {}, Trace: {}", e , self.node_identity.node_id().short_str(),
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
                    // TODO: Remove time measurements; this is to aid in system testing only  #LOGGED
                    let start = Instant::now();
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Base Node Response, Trace: {}", msg.dht_header.message_tag);
                    let _result = self.handle_base_node_response(inner_msg).await.map_err(|e| {
                        warn!(target: LOG_TARGET, "Error handling base node service response from {}: {:?} for \
                        NodeID: {}, Trace: {}", origin_public_key, e, self.node_identity.node_id().short_str(),
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
                    // TODO: Remove time measurements; this is to aid in system testing only #LOGGED
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
                dest_pubkey,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                message,
            } => {
                let rp = reply_channel.take().expect("Cannot be missing");
                self.send_transaction(
                    dest_pubkey,
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
                dest_pubkey,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                message,
            } => self
                .send_one_sided_transaction(
                    dest_pubkey,
                    amount,
                    selection_criteria,
                    *output_features,
                    fee_per_gram,
                    message,
                    transaction_broadcast_join_handles,
                )
                .await
                .map(TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::SendOneSidedToStealthAddressTransaction {
                dest_pubkey,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                message,
            } => self
                .send_one_sided_to_stealth_address_transaction(
                    dest_pubkey,
                    amount,
                    selection_criteria,
                    *output_features,
                    fee_per_gram,
                    message,
                    transaction_broadcast_join_handles,
                )
                .await
                .map(TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::BurnTari {
                amount,
                selection_criteria,
                fee_per_gram,
                message,
            } => self
                .burn_tari(
                    amount,
                    selection_criteria,
                    fee_per_gram,
                    message,
                    transaction_broadcast_join_handles,
                )
                .await
                .map(TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::SendShaAtomicSwapTransaction(
                dest_pubkey,
                amount,
                selection_criteria,
                fee_per_gram,
                message,
            ) => Ok(TransactionServiceResponse::ShaAtomicSwapTransactionSent(
                self.send_sha_atomic_swap_transaction(
                    dest_pubkey,
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
            TransactionServiceRequest::ImportUtxoWithStatus {
                amount,
                source_public_key,
                message,
                maturity,
                import_status,
                tx_id,
                current_height,
                mined_timestamp,
            } => self
                .add_utxo_import_transaction_with_status(
                    amount,
                    source_public_key,
                    message,
                    maturity,
                    import_status,
                    tx_id,
                    current_height,
                    mined_timestamp,
                )
                .map(TransactionServiceResponse::UtxoImported),
            TransactionServiceRequest::SubmitTransactionToSelf(tx_id, tx, fee, amount, message) => self
                .submit_transaction_to_self(transaction_broadcast_join_handles, tx_id, tx, fee, amount, message)
                .map(|_| TransactionServiceResponse::TransactionSubmitted),
            TransactionServiceRequest::GenerateCoinbaseTransaction(reward, fees, block_height) => self
                .generate_coinbase_transaction(reward, fees, block_height)
                .await
                .map(|tx| TransactionServiceResponse::CoinbaseTransactionGenerated(Box::new(tx))),
            TransactionServiceRequest::SetLowPowerMode => {
                self.set_power_mode(PowerMode::Low).await?;
                Ok(TransactionServiceResponse::LowPowerModeSet)
            },
            TransactionServiceRequest::SetNormalPowerMode => {
                self.set_power_mode(PowerMode::Normal).await?;
                Ok(TransactionServiceResponse::NormalPowerModeSet)
            },
            TransactionServiceRequest::ApplyEncryption(cipher) => self
                .db
                .apply_encryption(*cipher)
                .map(|_| TransactionServiceResponse::EncryptionApplied)
                .map_err(TransactionServiceError::TransactionStorageError),
            TransactionServiceRequest::RemoveEncryption => self
                .db
                .remove_encryption()
                .map(|_| TransactionServiceResponse::EncryptionRemoved)
                .map_err(TransactionServiceError::TransactionStorageError),
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
            let _result = rp.send(response).map_err(|e| {
                warn!(target: LOG_TARGET, "Failed to send reply");
                e
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
            BaseNodeEvent::BaseNodeStateChanged(state) => {
                let trigger_validation = match (self.last_seen_tip_height, state.chain_metadata.clone()) {
                    (Some(last_seen_tip_height), Some(cm)) => last_seen_tip_height != cm.height_of_longest_chain(),
                    (None, _) => true,
                    _ => false,
                };

                if trigger_validation {
                    let _operation_id = self
                        .start_transaction_validation_protocol(transaction_validation_join_handles)
                        .await
                        .map_err(|e| {
                            warn!(target: LOG_TARGET, "Error validating  txos: {:?}", e);
                            e
                        });
                }
                self.last_seen_tip_height = state.chain_metadata.map(|cm| cm.height_of_longest_chain());
            },
            BaseNodeEvent::NewBlockDetected(_) => {},
        }
    }

    async fn handle_output_manager_service_event(&mut self, event: Arc<OutputManagerEvent>) {
        if let OutputManagerEvent::TxoValidationSuccess(_) = (*event).clone() {
            let db = self.db.clone();
            let output_manager_handle = self.output_manager_service.clone();
            let metadata = match self.wallet_db.get_chain_metadata() {
                Ok(data) => data,
                Err(_) => None,
            };
            let tip_height = match metadata {
                Some(val) => val.height_of_longest_chain(),
                None => 0u64,
            };
            let event_publisher = self.event_publisher.clone();
            tokio::spawn(check_faux_transactions(
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
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroTari,
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

        // If we're paying ourselves, let's complete and submit the transaction immediately
        if self.node_identity.public_key() == &dest_pubkey {
            debug!(
                target: LOG_TARGET,
                "Received transaction with spend-to-self transaction"
            );

            let (fee, transaction) = self
                .output_manager_service
                .create_pay_to_self_transaction(
                    tx_id,
                    amount,
                    selection_criteria,
                    output_features,
                    fee_per_gram,
                    None,
                    message.clone(),
                )
                .await?;

            // Notify that the transaction was successfully resolved.
            let _size = self
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionCompletedImmediately(tx_id)));

            self.submit_transaction(
                transaction_broadcast_join_handles,
                CompletedTransaction::new(
                    tx_id,
                    self.node_identity.public_key().clone(),
                    self.node_identity.public_key().clone(),
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
                ),
            )?;

            let _result = reply_channel
                .send(Ok(TransactionServiceResponse::TransactionSent(tx_id)))
                .map_err(|e| {
                    warn!(target: LOG_TARGET, "Failed to send service reply");
                    e
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
            dest_pubkey,
            amount,
            fee_per_gram,
            message,
            tx_meta,
            Some(reply_channel),
            TransactionSendProtocolStage::Initial,
            None,
            self.last_seen_tip_height,
            None,
        );
        let join_handle = tokio::spawn(protocol.execute());
        join_handles.push(join_handle);

        Ok(())
    }

    /// broadcasts a SHA-XTR atomic swap transaction
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    #[allow(clippy::too_many_lines)]
    pub async fn send_sha_atomic_swap_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroTari,
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
        let height = self.last_seen_tip_height.unwrap_or(0) + (24 * 30);

        // lets create the HTLC script
        let script = script!(
            HashSha256 PushHash(Box::new(hash)) Equal IfThen
                PushPubKey(Box::new(dest_pubkey.clone()))
            Else
                CheckHeightVerify(height) PushPubKey(Box::new(self.node_identity.public_key().clone()))
            EndIf
        );

        // Empty covenant
        let covenant = Covenant::default();

        // Default range proof
        let minimum_value_promise = MicroTari::zero();

        // Prepare sender part of the transaction
        let mut stp = self
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
            .build_single_round_message()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        self.output_manager_service
            .confirm_pending_transaction(tx_id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Prepare receiver part of the transaction

        // Diffie-Hellman shared secret `k_Ob * K_Sb = K_Ob * k_Sb` results in a public key, which is converted to
        // bytes to enable conversion into a private key to be used as the spending key
        let sender_offset_private_key = stp
            .get_recipient_sender_offset_private_key(0)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        let spend_key = PrivateKey::from_bytes(
            CommsPublicKey::shared_secret(&sender_offset_private_key.clone(), &dest_pubkey.clone()).as_bytes(),
        )
        .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        let sender_message = TransactionSenderMessage::new_single_round_message(stp.get_single_round_message()?);
        let rewind_blinding_key = PrivateKey::from_bytes(&hash_secret_key(&spend_key))?;
        let encryption_key = PrivateKey::from_bytes(&hash_secret_key(&rewind_blinding_key))?;

        let rewind_data = RewindData {
            rewind_blinding_key: rewind_blinding_key.clone(),
            encryption_key: encryption_key.clone(),
        };

        let rtp = ReceiverTransactionProtocol::new_with_rewindable_output(
            sender_message,
            PrivateKey::random(&mut OsRng),
            spend_key.clone(),
            &self.resources.factories,
            &rewind_data,
        );

        let recipient_reply = rtp.get_signed_data()?.clone();
        let output = recipient_reply.output.clone();
        let commitment = self
            .resources
            .factories
            .commitment
            .commit_value(&spend_key, amount.into());
        let encrypted_value = EncryptedValue::encrypt_value(&rewind_data.encryption_key, &commitment, amount)?;
        let minimum_value_promise = MicroTari::zero();
        let unblinded_output = UnblindedOutput::new_current_version(
            amount,
            spend_key,
            output.features.clone(),
            script,
            inputs!(PublicKey::from_secret_key(self.node_identity.secret_key())),
            self.node_identity.secret_key().clone(),
            output.sender_offset_public_key.clone(),
            output.metadata_signature.clone(),
            height,
            covenant,
            encrypted_value,
            minimum_value_promise,
        );

        // Start finalizing

        stp.add_single_recipient_info(recipient_reply, &self.resources.factories.range_proof)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Finalize

        stp.finalize(
            &self.resources.factories,
            None,
            self.last_seen_tip_height.unwrap_or(u64::MAX),
        )
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
        self.output_manager_service
            .add_rewindable_output_with_tx_id(
                tx_id,
                unblinded_output,
                Some(SpendingPriority::HtlcSpendAsap),
                Some(rewind_data),
            )
            .await?;
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new(
                tx_id,
                self.resources.node_identity.public_key().clone(),
                dest_pubkey.clone(),
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
            ),
        )?;

        Ok(Box::new((tx_id, pre_image, output)))
    }

    async fn send_one_sided_or_stealth(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroTari,
        message: String,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        script: TariScript,
    ) -> Result<TxId, TransactionServiceError> {
        let tx_id = TxId::new_random();

        // Prepare sender part of the transaction
        let mut stp = self
            .output_manager_service
            .prepare_transaction_to_send(
                tx_id,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                TransactionMetadata::default(),
                message.clone(),
                script,
                Covenant::default(),
                MicroTari::zero(),
            )
            .await?;

        // This call is needed to advance the state from `SingleRoundMessageReady` to `SingleRoundMessageReady`,
        // but the returned value is not used
        let _single_round_sender_data = stp
            .build_single_round_message()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        self.output_manager_service
            .confirm_pending_transaction(tx_id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Prepare receiver part of the transaction

        // Diffie-Hellman shared secret `k_Ob * K_Sb = K_Ob * k_Sb` results in a public key, which is converted to
        // bytes to enable conversion into a private key to be used as the spending key
        let sender_offset_private_key = stp
            .get_recipient_sender_offset_private_key(0)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        let spend_key = PrivateKey::from_bytes(
            CommsPublicKey::shared_secret(&sender_offset_private_key, &dest_pubkey.clone()).as_bytes(),
        )
        .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        let sender_message = TransactionSenderMessage::new_single_round_message(stp.get_single_round_message()?);
        let rewind_blinding_key = PrivateKey::from_bytes(&hash_secret_key(&spend_key))?;
        let encryption_key = PrivateKey::from_bytes(&hash_secret_key(&rewind_blinding_key))?;
        let rewind_data = RewindData {
            rewind_blinding_key,
            encryption_key,
        };

        let rtp = ReceiverTransactionProtocol::new_with_rewindable_output(
            sender_message,
            PrivateKey::random(&mut OsRng),
            spend_key,
            &self.resources.factories,
            &rewind_data,
        );

        let recipient_reply = rtp.get_signed_data()?.clone();

        // Start finalizing

        stp.add_single_recipient_info(recipient_reply, &self.resources.factories.range_proof)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Finalize

        stp.finalize(
            &self.resources.factories,
            None,
            self.last_seen_tip_height.unwrap_or(u64::MAX),
        )
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
                self.resources.node_identity.public_key().clone(),
                dest_pubkey.clone(),
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
            ),
        )?;

        Ok(tx_id)
    }

    /// Sends a one side payment transaction to a recipient
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    pub async fn send_one_sided_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroTari,
        message: String,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        if self.node_identity.public_key() == &dest_pubkey {
            warn!(target: LOG_TARGET, "One-sided spend-to-self transactions not supported");
            return Err(TransactionServiceError::OneSidedTransactionError(
                "One-sided spend-to-self transactions not supported".to_string(),
            ));
        }
        self.send_one_sided_or_stealth(
            dest_pubkey.clone(),
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            message,
            transaction_broadcast_join_handles,
            script!(PushPubKey(Box::new(dest_pubkey))),
        )
        .await
    }

    /// Creates a transaction to burn some Tari
    /// # Arguments
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    pub async fn burn_tari(
        &mut self,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroTari,
        message: String,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        let tx_id = TxId::new_random();
        let output_features = OutputFeatures::create_burn_output();
        // Prepare sender part of the transaction
        let tx_meta = TransactionMetadata::new_with_features(0.into(), 0, KernelFeatures::create_burn());
        let mut stp = self
            .output_manager_service
            .prepare_transaction_to_send(
                tx_id,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                tx_meta,
                message.clone(),
                TariScript::default(),
                Covenant::default(),
                MicroTari::zero(),
            )
            .await?;

        // This call is needed to advance the state from `SingleRoundMessageReady` to `SingleRoundMessageReady`,
        // but the returned value is not used
        let _single_round_sender_data = stp
            .build_single_round_message()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        self.output_manager_service
            .confirm_pending_transaction(tx_id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        let sender_message = TransactionSenderMessage::new_single_round_message(stp.get_single_round_message()?);
        let spend_key = PrivateKey::random(&mut OsRng);
        let rtp = ReceiverTransactionProtocol::new(
            sender_message,
            PrivateKey::random(&mut OsRng),
            spend_key,
            &self.resources.factories,
        );

        let recipient_reply = rtp.get_signed_data()?.clone();

        // Start finalizing

        stp.add_single_recipient_info(recipient_reply, &self.resources.factories.range_proof)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        // Finalize

        stp.finalize(
            &self.resources.factories,
            None,
            self.last_seen_tip_height.unwrap_or(u64::MAX),
        )
        .map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) could not be finalized. Failure error: {:?}", tx_id, e,
            );
            TransactionServiceProtocolError::new(tx_id, e.into())
        })?;
        info!(target: LOG_TARGET, "Finalized burning transaction TxId: {}", tx_id);

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
                self.resources.node_identity.public_key().clone(),
                CommsPublicKey::default(),
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
            ),
        )?;

        Ok(tx_id)
    }

    /// Sends a one side payment transaction to a recipient
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    pub async fn send_one_sided_to_stealth_address_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroTari,
        message: String,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
    ) -> Result<TxId, TransactionServiceError> {
        if self.node_identity.public_key() == &dest_pubkey {
            warn!(target: LOG_TARGET, "One-sided spend-to-self transactions not supported");
            return Err(TransactionServiceError::OneSidedTransactionError(
                "One-sided-to-stealth-address spend-to-self transactions not supported".to_string(),
            ));
        }

        let (nonce_private_key, nonce_public_key) = PublicKey::random_keypair(&mut OsRng);

        let c = WalletHasher::new_with_label("stealth_address")
            .chain((dest_pubkey.clone() * nonce_private_key).as_bytes())
            .finalize();

        let script_spending_key =
            PublicKey::from_secret_key(&PrivateKey::from_bytes(c.as_ref()).unwrap()) + dest_pubkey.clone();

        self.send_one_sided_or_stealth(
            dest_pubkey,
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            message,
            transaction_broadcast_join_handles,
            script!(PushPubKey(Box::new(nonce_public_key)) Drop PushPubKey(Box::new(script_spending_key))),
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
        recipient_reply: proto::RecipientSignedMessage,
    ) -> Result<(), TransactionServiceError> {
        // Check if a wallet recovery is in progress, if it is we will ignore this request
        self.check_recovery_status()?;

        let recipient_reply: RecipientSignedMessage = recipient_reply
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
            if ctx.destination_public_key != source_pubkey {
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
            if otx.destination_public_key != source_pubkey {
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
                                "Error starting Broadcast Protocol after completed Send Transaction Protocol : {:?}", e
                            );
                            return;
                        },
                    };
                    let _result = self
                        .broadcast_completed_transaction(completed_tx, transaction_broadcast_join_handles)
                        .map_err(|resp| {
                            error!(
                                target: LOG_TARGET,
                                "Error starting Broadcast Protocol after completed Send Transaction Protocol : {:?}",
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

        self.output_manager_service.cancel_transaction(tx_id).await?;

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
        transaction_cancelled: proto::TransactionCancelledMessage,
    ) -> Result<(), TransactionServiceError> {
        let tx_id = transaction_cancelled.tx_id.into();

        // Check that an inbound transaction exists to be cancelled and that the Source Public key for that transaction
        // is the same as the cancellation message
        if let Ok(inbound_tx) = self.db.get_pending_inbound_transaction(tx_id) {
            if inbound_tx.source_public_key == source_pubkey {
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
                    tx.destination_public_key,
                    tx.amount,
                    tx.fee,
                    tx.message,
                    TransactionMetadata::default(),
                    None,
                    stage,
                    None,
                    self.last_seen_tip_height,
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
        sender_message: proto::TransactionSenderMessage,
        traced_message_tag: u64,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>>,
    ) -> Result<(), TransactionServiceError> {
        // Check if a wallet recovery is in progress, if it is we will ignore this request
        self.check_recovery_status()?;

        let sender_message: TransactionSenderMessage = sender_message
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

                if tx.source_public_key != source_pubkey {
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
                if inbound_tx.source_public_key != source_pubkey {
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

            let protocol = TransactionReceiveProtocol::new(
                data.tx_id,
                source_pubkey,
                sender_message,
                TransactionReceiveProtocolStage::Initial,
                self.resources.clone(),
                tx_finalized_receiver,
                cancellation_receiver,
                None,
                self.last_seen_tip_height,
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
        finalized_transaction: proto::TransactionFinalizedMessage,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>>,
    ) -> Result<(), TransactionServiceError> {
        // Check if a wallet recovery is in progress, if it is we will ignore this request
        self.check_recovery_status()?;

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

        let sender = match self.finalized_transaction_senders.get_mut(&tx_id) {
            None => {
                // First check if perhaps we know about this inbound transaction but it was cancelled
                match self.db.get_cancelled_pending_inbound_transaction(tx_id) {
                    Ok(t) => {
                        if t.source_public_key != source_pubkey {
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
                        self.output_manager_service
                            .reinstate_cancelled_inbound_transaction_outputs(tx_id)
                            .await?;

                        self.restart_receive_transaction_protocol(tx_id, source_pubkey.clone(), join_handles);
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
            .send((source_pubkey, tx_id, transaction))
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
            self.restart_receive_transaction_protocol(txn.tx_id, txn.source_public_key, join_handles);
        }

        Ok(())
    }

    fn restart_receive_transaction_protocol(
        &mut self,
        tx_id: TxId,
        source_public_key: CommsPublicKey,
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
                source_public_key,
                TransactionSenderMessage::None,
                TransactionReceiveProtocolStage::WaitForFinalize,
                self.resources.clone(),
                tx_finalized_receiver,
                cancellation_receiver,
                None,
                self.last_seen_tip_height,
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
                    "Error restarting protocols for all coinbase transactions: {:?}", resp
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
        self.resources.db.mark_all_transactions_as_unvalidated()?;
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
            self.resources.output_manager_service.clone(),
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
        if completed_tx.is_coinbase() {
            return Err(TransactionServiceError::AttemptedToBroadcastCoinbaseTransaction(
                completed_tx.tx_id,
            ));
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
        response: base_node_proto::BaseNodeServiceResponse,
    ) -> Result<(), TransactionServiceError> {
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
    pub fn add_utxo_import_transaction_with_status(
        &mut self,
        value: MicroTari,
        source_public_key: CommsPublicKey,
        message: String,
        maturity: Option<u64>,
        import_status: ImportStatus,
        tx_id: Option<TxId>,
        current_height: Option<u64>,
        mined_timestamp: Option<NaiveDateTime>,
    ) -> Result<TxId, TransactionServiceError> {
        let tx_id = if let Some(id) = tx_id { id } else { TxId::new_random() };
        self.db.add_utxo_import_transaction_with_status(
            tx_id,
            value,
            source_public_key,
            self.node_identity.public_key().clone(),
            message,
            maturity,
            import_status.clone(),
            current_height,
            mined_timestamp,
        )?;
        let transaction_event = match import_status {
            ImportStatus::Imported => TransactionEvent::TransactionImported(tx_id),
            ImportStatus::FauxUnconfirmed => TransactionEvent::FauxTransactionUnconfirmed {
                tx_id,
                num_confirmations: 0,
                is_valid: true,
            },
            ImportStatus::FauxConfirmed | ImportStatus::Coinbase => {
                TransactionEvent::FauxTransactionConfirmed { tx_id, is_valid: true }
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
    fn submit_transaction(
        &mut self,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionServiceError> {
        let tx_id = completed_transaction.tx_id;
        trace!(target: LOG_TARGET, "Submit transaction ({}) to db.", tx_id);
        self.db.insert_completed_transaction(tx_id, completed_transaction)?;
        trace!(
            target: LOG_TARGET,
            "Launch the transaction broadcast protocol for submitted transaction ({}).",
            tx_id
        );
        self.complete_send_transaction_protocol(
            Ok(TransactionSendResult {
                tx_id,
                transaction_status: TransactionStatus::Completed,
            }),
            transaction_broadcast_join_handles,
        );
        Ok(())
    }

    /// Submit a completed coin split transaction to the Transaction Manager. This is different from
    /// `submit_transaction` in that it will expose less information about the completed transaction.
    pub fn submit_transaction_to_self(
        &mut self,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<TxId, TransactionServiceProtocolError<TxId>>>,
        >,
        tx_id: TxId,
        tx: Transaction,
        fee: MicroTari,
        amount: MicroTari,
        message: String,
    ) -> Result<(), TransactionServiceError> {
        self.submit_transaction(
            transaction_broadcast_join_handles,
            CompletedTransaction::new(
                tx_id,
                self.node_identity.public_key().clone(),
                self.node_identity.public_key().clone(),
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
            ),
        )?;
        Ok(())
    }

    async fn generate_coinbase_transaction(
        &mut self,
        reward: MicroTari,
        fees: MicroTari,
        block_height: u64,
    ) -> Result<Transaction, TransactionServiceError> {
        let amount = reward + fees;

        // first check if we already have a coinbase tx for this height and amount
        let find_result = self
            .db
            .find_coinbase_transaction_at_block_height(block_height, amount)?;

        let completed_transaction = match find_result {
            Some(completed_tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Coinbase transaction (TxId: {}) for Block Height: {} found, with Amount {}.",
                    completed_tx.tx_id,
                    block_height,
                    amount
                );

                completed_tx.transaction
            },
            None => {
                // otherwise create a new coinbase tx
                let tx_id = TxId::new_random();
                let tx = self
                    .output_manager_service
                    .get_coinbase_transaction(tx_id, reward, fees, block_height)
                    .await?;
                self.db.insert_completed_transaction(
                    tx_id,
                    CompletedTransaction::new(
                        tx_id,
                        self.node_identity.public_key().clone(),
                        self.node_identity.public_key().clone(),
                        amount,
                        MicroTari::from(0),
                        tx.clone(),
                        TransactionStatus::Coinbase,
                        format!("Coinbase Transaction for Block #{}", block_height),
                        Utc::now().naive_utc(),
                        TransactionDirection::Inbound,
                        Some(block_height),
                        None,
                        None,
                    ),
                )?;

                let _size = self
                    .resources
                    .event_publisher
                    .send(Arc::new(TransactionEvent::ReceivedFinalizedTransaction(tx_id)))
                    .map_err(|e| {
                        trace!(
                            target: LOG_TARGET,
                            "Error sending event because there are no subscribers: {:?}",
                            e
                        );
                        e
                    });

                info!(
                    target: LOG_TARGET,
                    "Coinbase transaction (TxId: {}) for Block Height: {} added", tx_id, block_height
                );
                tx
            },
        };

        Ok(completed_transaction)
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
pub struct TransactionServiceResources<TBackend, TWalletConnectivity> {
    pub db: TransactionDatabase<TBackend>,
    pub output_manager_service: OutputManagerHandle,
    pub outbound_message_service: OutboundMessageRequester,
    pub connectivity: TWalletConnectivity,
    pub event_publisher: TransactionEventSender,
    pub node_identity: Arc<NodeIdentity>,
    pub factories: CryptoFactories,
    pub config: TransactionServiceConfig,
    pub shutdown_signal: ShutdownSignal,
}

#[derive(Clone, Copy)]
enum PowerMode {
    Low,
    Normal,
}

impl Default for PowerMode {
    fn default() -> Self {
        PowerMode::Normal
    }
}

/// Contains the generated TxId and SpendingKey for a Pending Coinbase transaction
#[derive(Debug)]
pub struct PendingCoinbaseSpendingKey {
    pub tx_id: TxId,
    pub spending_key: PrivateKey,
}

fn hash_secret_key(key: &PrivateKey) -> Vec<u8> {
    WalletSecretKeysDomainHasher::new()
        .chain(key.as_bytes())
        .finalize()
        .as_ref()
        .to_vec()
}

/// Contains the generated TxId and TransactionStatus transaction send result
#[derive(Debug)]
pub struct TransactionSendResult {
    pub tx_id: TxId,
    pub transaction_status: TransactionStatus,
}

#[cfg(test)]
mod tests {
    use tari_crypto::ristretto::RistrettoSecretKey;
    use tari_script::Opcode;
    use WalletHasher;

    use super::*;

    #[test]
    fn test_stealth_addresses() {
        // recipient's keys
        let (a, big_a) = PublicKey::random_keypair(&mut OsRng);
        let (b, big_b) = PublicKey::random_keypair(&mut OsRng);

        // Sender generates a random nonce key-pair: R=r⋅G
        let (r, big_r) = PublicKey::random_keypair(&mut OsRng);

        // Sender calculates a ECDH shared secret: c=H(r⋅a⋅G)=H(a⋅R)=H(r⋅A),
        // where H(⋅) is a cryptographic hash function
        let c = WalletHasher::new_with_label("stealth_address")
            .chain(PublicKey::shared_secret(&r, &big_a).as_bytes())
            .finalize();

        // using spending key `Ks=c⋅G+B` as the last public key in the one-sided payment script
        let sender_spending_key =
            PublicKey::from_secret_key(&RistrettoSecretKey::from_bytes(c.as_ref()).unwrap()) + big_b.clone();

        let script = script!(PushPubKey(Box::new(big_r)) Drop PushPubKey(Box::new(sender_spending_key.clone())));

        // ----------------------------------------------------------------------------
        // imitating the receiving end, scanning and extraction

        // Extracting the nonce R and a spending key from the script
        if let [Opcode::PushPubKey(big_r), Opcode::Drop, Opcode::PushPubKey(provided_spending_key)] = script.as_slice()
        {
            // calculating Ks with the provided R nonce from the script
            let c = WalletHasher::new_with_label("stealth_address")
                .chain(PublicKey::shared_secret(&a, big_r).as_bytes())
                .finalize();

            // computing a spending key `Ks=(c+b)G` for comparison
            let receiver_spending_key =
                PublicKey::from_secret_key(&(RistrettoSecretKey::from_bytes(c.as_ref()).unwrap() + b));

            // computing a scanning key `Ks=cG+B` for comparison
            let scanning_key = PublicKey::from_secret_key(&RistrettoSecretKey::from_bytes(c.as_ref()).unwrap()) + big_b;

            assert_eq!(provided_spending_key.as_ref(), &sender_spending_key);
            assert_eq!(receiver_spending_key, sender_spending_key);
            assert_eq!(scanning_key, sender_spending_key);
            assert_eq!(scanning_key, receiver_spending_key);
        }
    }
}
