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
    output_manager_service::{handle::OutputManagerHandle, TxId},
    transaction_service::{
        config::TransactionServiceConfig,
        error::{TransactionServiceError, TransactionServiceProtocolError},
        handle::{TransactionEvent, TransactionEventSender, TransactionServiceRequest, TransactionServiceResponse},
        protocols::{
            transaction_broadcast_protocol::TransactionBroadcastProtocol,
            transaction_chain_monitoring_protocol::TransactionChainMonitoringProtocol,
            transaction_coinbase_monitoring_protocol::TransactionCoinbaseMonitoringProtocol,
            transaction_receive_protocol::{TransactionReceiveProtocol, TransactionReceiveProtocolStage},
            transaction_send_protocol::{TransactionSendProtocol, TransactionSendProtocolStage},
        },
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::{CompletedTransaction, TransactionDirection, TransactionStatus},
        },
        tasks::{
            send_finalized_transaction::send_finalized_transaction_message,
            send_transaction_cancelled::send_transaction_cancelled_message,
            send_transaction_reply::send_transaction_reply,
        },
    },
};
use chrono::{NaiveDateTime, Utc};
use futures::{
    channel::{mpsc, mpsc::Sender, oneshot},
    pin_mut,
    stream::FuturesUnordered,
    SinkExt,
    Stream,
    StreamExt,
};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    sync::Arc,
    time::Duration,
};
use tari_comms::{peer_manager::NodeIdentity, types::CommsPublicKey};
use tari_comms_dht::outbound::OutboundMessageRequester;
#[cfg(feature = "test_harness")]
use tari_core::transactions::{tari_amount::uT, types::BlindingFactor};
use tari_core::{
    base_node::proto::base_node as BaseNodeProto,
    consensus::ConsensusConstants,
    mempool::{proto::mempool as MempoolProto, service::MempoolServiceResponse},
    transactions::{
        tari_amount::MicroTari,
        transaction::Transaction,
        transaction_protocol::{proto, recipient::RecipientSignedMessage, sender::TransactionSenderMessage},
        types::{CryptoFactories, PrivateKey},
        CoinbaseBuilder,
        OutputBuilder,
    },
};
use tari_crypto::keys::SecretKey;
use tari_p2p::domain_message::DomainMessage;
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_shutdown::ShutdownSignal;
use tokio::{sync::broadcast, task::JoinHandle};

const LOG_TARGET: &str = "wallet::transaction_service::service";

/// Contains the generated TxId and SpendingKey for a Pending Coinbase transaction
#[derive(Debug)]
pub struct PendingCoinbaseSpendingKey {
    pub tx_id: TxId,
    pub spending_key: PrivateKey,
}

/// TransactionService allows for the management of multiple inbound and outbound transaction protocols
/// which are uniquely identified by a tx_id. The TransactionService generates and accepts the various protocol
/// messages and applies them to the appropriate protocol instances based on the tx_id.
/// The TransactionService allows for the sending of transactions to single receivers, when the appropriate recipient
/// response is handled the transaction is completed and moved to the completed_transaction buffer.
/// The TransactionService will accept inbound transactions and generate a reply. Received transactions will remain
/// in the pending_inbound_transactions buffer.
/// TODO Allow for inbound transactions that are detected on the blockchain to be marked as complete in the
/// OutputManagerService
/// TODO Detect Completed Transactions on the blockchain before marking them as completed in OutputManagerService
/// # Fields
/// `pending_outbound_transactions` - List of transaction protocols sent by this client and waiting response from the
/// recipient
/// `pending_inbound_transactions` - List of transaction protocols that have been received and responded to.
/// `completed_transaction` - List of sent transactions that have been responded to and are completed.

pub struct TransactionService<
    TTxStream,
    TTxReplyStream,
    TTxFinalizedStream,
    MReplyStream,
    BNResponseStream,
    TBackend,
    TTxCancelledStream,
> where TBackend: TransactionBackend + 'static
{
    config: TransactionServiceConfig,
    db: TransactionDatabase<TBackend>,
    output_manager_service: OutputManagerHandle,
    transaction_stream: Option<TTxStream>,
    transaction_reply_stream: Option<TTxReplyStream>,
    transaction_finalized_stream: Option<TTxFinalizedStream>,
    mempool_response_stream: Option<MReplyStream>,
    base_node_response_stream: Option<BNResponseStream>,
    transaction_cancelled_stream: Option<TTxCancelledStream>,
    request_stream: Option<
        reply_channel::Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    >,
    event_publisher: TransactionEventSender,
    node_identity: Arc<NodeIdentity>,
    base_node_public_key: Option<CommsPublicKey>,
    resources: TransactionServiceResources<TBackend>,
    pending_transaction_reply_senders: HashMap<TxId, Sender<(CommsPublicKey, RecipientSignedMessage)>>,
    mempool_response_senders: HashMap<u64, Sender<MempoolServiceResponse>>,
    base_node_response_senders: HashMap<u64, Sender<BaseNodeProto::BaseNodeServiceResponse>>,
    send_transaction_cancellation_senders: HashMap<u64, oneshot::Sender<()>>,
    finalized_transaction_senders: HashMap<u64, Sender<(CommsPublicKey, TxId, Transaction)>>,
    receiver_transaction_cancellation_senders: HashMap<u64, oneshot::Sender<()>>,
    timeout_update_publisher: broadcast::Sender<Duration>,
    power_mode: PowerMode,
}

#[allow(clippy::too_many_arguments)]
impl<TTxStream, TTxReplyStream, TTxFinalizedStream, MReplyStream, BNResponseStream, TBackend, TTxCancelledStream>
    TransactionService<
        TTxStream,
        TTxReplyStream,
        TTxFinalizedStream,
        MReplyStream,
        BNResponseStream,
        TBackend,
        TTxCancelledStream,
    >
where
    TTxStream: Stream<Item = DomainMessage<proto::TransactionSenderMessage>>,
    TTxReplyStream: Stream<Item = DomainMessage<proto::RecipientSignedMessage>>,
    TTxFinalizedStream: Stream<Item = DomainMessage<proto::TransactionFinalizedMessage>>,
    MReplyStream: Stream<Item = DomainMessage<MempoolProto::MempoolServiceResponse>>,
    BNResponseStream: Stream<Item = DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>,
    TTxCancelledStream: Stream<Item = DomainMessage<proto::TransactionCancelledMessage>>,
    TBackend: TransactionBackend + 'static,
{
    pub fn new(
        config: TransactionServiceConfig,
        db: TransactionDatabase<TBackend>,
        request_stream: Receiver<
            TransactionServiceRequest,
            Result<TransactionServiceResponse, TransactionServiceError>,
        >,
        transaction_stream: TTxStream,
        transaction_reply_stream: TTxReplyStream,
        transaction_finalized_stream: TTxFinalizedStream,
        mempool_response_stream: MReplyStream,
        base_node_response_stream: BNResponseStream,
        transaction_cancelled_stream: TTxCancelledStream,
        output_manager_service: OutputManagerHandle,
        outbound_message_service: OutboundMessageRequester,
        event_publisher: TransactionEventSender,
        node_identity: Arc<NodeIdentity>,
        factories: CryptoFactories,
        consensus_constants: ConsensusConstants,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        // Collect the resources that all protocols will need so that they can be neatly cloned as the protocols are
        // spawned.
        let resources = TransactionServiceResources {
            db: db.clone(),
            output_manager_service: output_manager_service.clone(),
            outbound_message_service,
            event_publisher: event_publisher.clone(),
            node_identity: node_identity.clone(),
            factories,
            config: config.clone(),
            consensus_constants,
            shutdown_signal,
        };
        let (timeout_update_publisher, _) = broadcast::channel(20);

        TransactionService {
            config,
            db,
            output_manager_service,
            transaction_stream: Some(transaction_stream),
            transaction_reply_stream: Some(transaction_reply_stream),
            transaction_finalized_stream: Some(transaction_finalized_stream),
            mempool_response_stream: Some(mempool_response_stream),
            base_node_response_stream: Some(base_node_response_stream),
            transaction_cancelled_stream: Some(transaction_cancelled_stream),
            request_stream: Some(request_stream),
            event_publisher,
            node_identity,
            base_node_public_key: None,
            resources,
            pending_transaction_reply_senders: HashMap::new(),
            mempool_response_senders: HashMap::new(),
            base_node_response_senders: HashMap::new(),
            send_transaction_cancellation_senders: HashMap::new(),
            finalized_transaction_senders: HashMap::new(),
            receiver_transaction_cancellation_senders: HashMap::new(),
            timeout_update_publisher,
            power_mode: PowerMode::Normal,
        }
    }

    #[warn(unreachable_code)]
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
        let mempool_response_stream = self
            .mempool_response_stream
            .take()
            .expect("Transaction Service initialized without mempool_response_stream")
            .fuse();
        pin_mut!(mempool_response_stream);
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
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        > = FuturesUnordered::new();

        let mut receive_transaction_protocol_handles: FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        > = FuturesUnordered::new();

        let mut transaction_broadcast_protocol_handles: FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        > = FuturesUnordered::new();

        let mut transaction_chain_monitoring_protocol_handles: FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        > = FuturesUnordered::new();

        let mut coinbase_transaction_monitoring_protocol_handles: FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        > = FuturesUnordered::new();

        info!(target: LOG_TARGET, "Transaction Service started");
        loop {
            futures::select! {
                //Incoming request
                request_context = request_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Handling Service API Request");
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request,
                        &mut send_transaction_protocol_handles,
                        &mut receive_transaction_protocol_handles,
                        &mut transaction_broadcast_protocol_handles,
                        &mut transaction_chain_monitoring_protocol_handles,
                        &mut coinbase_transaction_monitoring_protocol_handles).await.or_else(|resp| {
                        warn!(target: LOG_TARGET, "Error handling request: {:?}", resp);
                        Err(resp)
                    })).or_else(|resp| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                // Incoming Transaction messages from the Comms layer
                msg = transaction_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Transaction Message, Trace: {}", msg.dht_header.message_tag);

                    let result  = self.accept_transaction(origin_public_key, inner_msg,
                        msg.dht_header.message_tag.as_value(), &mut receive_transaction_protocol_handles).await;

                    match result {
                        Err(TransactionServiceError::RepeatedMessageError) => {
                            trace!(target: LOG_TARGET, "A repeated Transaction message was received, Trace: {}",
                            msg.dht_header.message_tag);
                        }
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to handle incoming Transaction message: {:?} for NodeID: {}, Trace: {}",
                                e, self.node_identity.node_id().short_str(), msg.dht_header.message_tag);
                            let _ = self.event_publisher.send(Arc::new(TransactionEvent::Error(format!("Error handling \
                                Transaction Sender message: {:?}", e).to_string())));
                        }
                        _ => (),
                    }
                },
                 // Incoming Transaction Reply messages from the Comms layer
                msg = transaction_reply_stream.select_next_some() => {
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
                            warn!(target: LOG_TARGET, "Failed to handle incoming Transaction Reply message: {:?} \
                            for NodeId: {}, Trace: {}", e, self.node_identity.node_id().short_str(),
                            msg.dht_header.message_tag);
                            let _ = self.event_publisher.send(Arc::new(TransactionEvent::Error("Error handling \
                            Transaction Recipient Reply message".to_string())));
                        },
                        Ok(_) => (),
                    }
                },
               // Incoming Finalized Transaction messages from the Comms layer
                msg = transaction_finalized_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Transaction Finalized Message, Trace: {}",
                    msg.dht_header.message_tag.as_value());
                    let result = self.accept_finalized_transaction(origin_public_key, inner_msg, ).await;

                    match result {
                        Err(TransactionServiceError::TransactionDoesNotExistError) => {
                            trace!(target: LOG_TARGET, "Unable to handle incoming Finalized Transaction message from NodeId: \
                            {} due to Transaction not existing. This usually means the message was a repeated message \
                            from Store and Forward, Trace: {}", self.node_identity.node_id().short_str(),
                            msg.dht_header.message_tag);
                        },
                       Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to handle incoming Transaction Finalized message: {:?} \
                            for NodeID: {}, Trace: {}", e , self.node_identity.node_id().short_str(),
                            msg.dht_header.message_tag.as_value());
                            let _ = self.event_publisher.send(Arc::new(TransactionEvent::Error("Error handling Transaction \
                            Finalized message".to_string(),)));
                       },
                       Ok(_) => ()
                    }
                },
                // Incoming messages from the Comms layer
                msg = mempool_response_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Mempool Response, Trace: {}", msg.dht_header.message_tag);
                    let _ = self.handle_mempool_response(inner_msg).await.or_else(|resp| {
                        warn!(target: LOG_TARGET, "Error handling mempool service response: {:?}, Trace: {}", resp,
                        msg.dht_header.message_tag.as_value());
                        Err(resp)
                    });
                }
                // Incoming messages from the Comms layer
                msg = base_node_response_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Base Node Response, Trace: {}", msg.dht_header.message_tag);
                    let _ = self.handle_base_node_response(inner_msg).await.or_else(|resp| {
                        warn!(target: LOG_TARGET, "Error handling base node service response from {}: {:?} for \
                        NodeID: {}, Trace: {}", origin_public_key, resp, self.node_identity.node_id().short_str(),
                        msg.dht_header.message_tag.as_value());
                        Err(resp)
                    });
                }
                // Incoming messages from the Comms layer
                msg = transaction_cancelled_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Transaction Cancelled message, Trace: {}", msg.dht_header.message_tag);
                    match self.handle_transaction_cancelled_message(origin_public_key, inner_msg, ).await {
                        Err(e) => warn!(target: LOG_TARGET, "Error handing Transaction Cancelled Message: {:?}", e),
                        _ => (),
                    }
                }
                join_result = send_transaction_protocol_handles.select_next_some() => {
                    trace!(target: LOG_TARGET, "Send Protocol for Transaction has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_send_transaction_protocol(join_result_inner,
                        &mut transaction_broadcast_protocol_handles).await,
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Send Transaction Protocol: {:?}", e),
                    };
                }
                join_result = receive_transaction_protocol_handles.select_next_some() => {
                    trace!(target: LOG_TARGET, "Receive Transaction Protocol has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_receive_transaction_protocol(join_result_inner,
                        &mut transaction_broadcast_protocol_handles).await,
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Send Transaction Protocol: {:?}", e),
                    };
                }
                join_result = transaction_broadcast_protocol_handles.select_next_some() => {
                    trace!(target: LOG_TARGET, "Transaction Broadcast protocol has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_transaction_broadcast_protocol(join_result_inner,
                        &mut transaction_chain_monitoring_protocol_handles).await,
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Broadcast Protocol: {:?}", e),
                    };
                }
                join_result = transaction_chain_monitoring_protocol_handles.select_next_some() => {
                    trace!(target: LOG_TARGET, "Transaction chain monitoring protocol has ended with result {:?}",
                    join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_transaction_chain_monitoring_protocol(join_result_inner),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Chain Monitoring protocol: {:?}", e),
                    };
                }
                join_result = coinbase_transaction_monitoring_protocol_handles.select_next_some() => {
                    trace!(target: LOG_TARGET, "Coinbase transaction monitoring protocol has ended with result {:?}",
                    join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_coinbase_transaction_monitoring_protocol(join_result_inner),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Coinbase Monitoring protocol: {:?}", e),
                    };
                }
                 _ = shutdown => {
                    info!(target: LOG_TARGET, "Transaction service shutting down because it received the shutdown signal");
                    break;
                }
                complete => {
                    info!(target: LOG_TARGET, "Transaction service shutting down");
                    break;
                }
            }
        }
        info!(target: LOG_TARGET, "Transaction service shut down");
        Ok(())
    }

    /// This handler is called when requests arrive from the various streams
    async fn handle_request(
        &mut self,
        request: TransactionServiceRequest,
        send_transaction_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
        receive_transaction_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
        chain_monitoring_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
        coinbase_monitoring_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
    ) -> Result<TransactionServiceResponse, TransactionServiceError>
    {
        trace!(target: LOG_TARGET, "Handling Service Request: {}", request);
        match request {
            TransactionServiceRequest::SendTransaction((dest_pubkey, amount, fee_per_gram, message)) => self
                .send_transaction(
                    dest_pubkey,
                    amount,
                    fee_per_gram,
                    message,
                    send_transaction_join_handles,
                )
                .await
                .map(TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::CancelTransaction(tx_id) => self
                .cancel_transaction(tx_id)
                .await
                .map(|_| TransactionServiceResponse::TransactionCancelled),
            TransactionServiceRequest::GetPendingInboundTransactions => {
                Ok(TransactionServiceResponse::PendingInboundTransactions(
                    self.db.get_pending_inbound_transactions().await?,
                ))
            },
            TransactionServiceRequest::GetPendingOutboundTransactions => {
                Ok(TransactionServiceResponse::PendingOutboundTransactions(
                    self.db.get_pending_outbound_transactions().await?,
                ))
            },

            TransactionServiceRequest::GetCompletedTransactions => Ok(
                TransactionServiceResponse::CompletedTransactions(self.db.get_completed_transactions().await?),
            ),
            TransactionServiceRequest::GetCancelledPendingInboundTransactions => {
                Ok(TransactionServiceResponse::PendingInboundTransactions(
                    self.db.get_cancelled_pending_inbound_transactions().await?,
                ))
            },
            TransactionServiceRequest::GetCancelledPendingOutboundTransactions => {
                Ok(TransactionServiceResponse::PendingOutboundTransactions(
                    self.db.get_cancelled_pending_outbound_transactions().await?,
                ))
            },
            TransactionServiceRequest::GetCancelledCompletedTransactions => {
                Ok(TransactionServiceResponse::CompletedTransactions(
                    self.db.get_cancelled_completed_transactions().await?,
                ))
            },
            TransactionServiceRequest::GetCompletedTransaction(tx_id) => {
                Ok(TransactionServiceResponse::CompletedTransaction(Box::new(
                    self.db.get_completed_transaction(tx_id).await?,
                )))
            },
            TransactionServiceRequest::GetAnyTransaction(tx_id) => Ok(TransactionServiceResponse::AnyTransaction(
                Box::new(self.db.get_any_transaction(tx_id).await?),
            )),
            TransactionServiceRequest::SetBaseNodePublicKey(public_key) => {
                self.set_base_node_public_key(public_key);
                Ok(TransactionServiceResponse::BaseNodePublicKeySet)
            },
            TransactionServiceRequest::ImportUtxo(value, source_public_key, message) => self
                .add_utxo_import_transaction(value, source_public_key, message)
                .await
                .map(TransactionServiceResponse::UtxoImported),
            TransactionServiceRequest::SubmitTransaction((tx_id, tx, fee, amount, message)) => self
                .submit_transaction(transaction_broadcast_join_handles, tx_id, tx, fee, amount, message)
                .await
                .map(|_| TransactionServiceResponse::TransactionSubmitted),
            TransactionServiceRequest::GenerateCoinbaseTransaction(reward, fees, block_height) => self
                .generate_coinbase_transaction(reward, fees, block_height, coinbase_monitoring_join_handles)
                .await
                .map(|tx| TransactionServiceResponse::CoinbaseTransactionGenerated(Box::new(tx))),
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::CompletePendingOutboundTransaction(completed_transaction) => {
                self.complete_pending_outbound_transaction(completed_transaction)
                    .await?;
                Ok(TransactionServiceResponse::CompletedPendingTransaction)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::FinalizePendingInboundTransaction(tx_id) => {
                self.finalize_received_test_transaction(tx_id).await?;
                Ok(TransactionServiceResponse::FinalizedPendingInboundTransaction)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::AcceptTestTransaction((tx_id, amount, source_pubkey)) => {
                self.receive_test_transaction(tx_id, amount, source_pubkey).await?;
                Ok(TransactionServiceResponse::AcceptedTestTransaction)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::BroadcastTransaction(tx_id) => {
                self.broadcast_transaction(tx_id).await?;
                Ok(TransactionServiceResponse::TransactionBroadcast)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::MineTransaction(tx_id) => {
                self.mine_transaction(tx_id).await?;
                Ok(TransactionServiceResponse::TransactionMined)
            },
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
                .await
                .map(|_| TransactionServiceResponse::EncryptionApplied)
                .map_err(TransactionServiceError::TransactionStorageError),
            TransactionServiceRequest::RemoveEncryption => self
                .db
                .remove_encryption()
                .await
                .map(|_| TransactionServiceResponse::EncryptionRemoved)
                .map_err(TransactionServiceError::TransactionStorageError),
            TransactionServiceRequest::RestartTransactionProtocols => self
                .restart_transaction_negotiation_protocols(
                    send_transaction_join_handles,
                    receive_transaction_join_handles,
                )
                .await
                .map(|_| TransactionServiceResponse::ProtocolsRestarted),

            TransactionServiceRequest::RestartBroadcastProtocols => self
                .restart_broadcast_protocols(
                    transaction_broadcast_join_handles,
                    chain_monitoring_join_handles,
                    coinbase_monitoring_join_handles,
                )
                .await
                .map(|_| TransactionServiceResponse::ProtocolsRestarted),
        }
    }

    /// Sends a new transaction to a recipient
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    pub async fn send_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        message: String,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<TxId, TransactionServiceError>
    {
        let sender_protocol = self
            .output_manager_service
            .prepare_transaction_to_send(amount, fee_per_gram, None, message.clone())
            .await?;

        let tx_id = sender_protocol.get_tx_id()?;

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
            message,
            sender_protocol,
            TransactionSendProtocolStage::Initial,
        );

        let join_handle = tokio::spawn(protocol.execute());
        join_handles.push(join_handle);

        Ok(tx_id)
    }

    /// Accept the public reply from a recipient and apply the reply to the relevant transaction protocol
    /// # Arguments
    /// 'recipient_reply' - The public response from a recipient with data required to complete the transaction
    pub async fn accept_recipient_reply(
        &mut self,
        source_pubkey: CommsPublicKey,
        recipient_reply: proto::RecipientSignedMessage,
    ) -> Result<(), TransactionServiceError>
    {
        let recipient_reply: RecipientSignedMessage = recipient_reply
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let tx_id = recipient_reply.tx_id;

        // First we check if this Reply is for a cancelled Pending Outbound Tx or a Completed Tx
        let cancelled_outbound_tx = self.db.get_cancelled_pending_outbound_transaction(tx_id).await;
        let completed_tx = self.db.get_completed_transaction_cancelled_or_not(tx_id).await;

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

            if ctx.cancelled {
                // Send a cancellation message
                debug!(
                    target: LOG_TARGET,
                    "A repeated Transaction Reply (TxId: {}) has been received for cancelled completed transaction. \
                     Transaction Cancelled response is being sent.",
                    tx_id
                );
                tokio::spawn(send_transaction_cancelled_message(
                    tx_id,
                    source_pubkey.clone(),
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
                    source_pubkey.clone(),
                    self.resources.outbound_message_service.clone(),
                    self.resources.config.direct_send_timeout,
                ));
            }

            if let Err(e) = self.resources.db.increment_send_count(tx_id).await {
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
                source_pubkey.clone(),
                self.resources.outbound_message_service.clone(),
            ));

            if let Err(e) = self.resources.db.increment_send_count(tx_id).await {
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
    async fn complete_send_transaction_protocol(
        &mut self,
        join_result: Result<u64, TransactionServiceProtocolError>,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
    )
    {
        match join_result {
            Ok(id) => {
                let _ = self.pending_transaction_reply_senders.remove(&id);
                let _ = self.send_transaction_cancellation_senders.remove(&id);
                let _ = self
                    .broadcast_completed_transaction_to_mempool(id, transaction_broadcast_join_handles)
                    .await
                    .map_err(|resp| {
                        error!(
                            target: LOG_TARGET,
                            "Error starting Broadcast Protocol after completed Send Transaction Protocol : {:?}", resp
                        );
                        resp
                    });
                trace!(
                    target: LOG_TARGET,
                    "Send Transaction Protocol for TxId: {} completed successfully",
                    id
                );
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _ = self.pending_transaction_reply_senders.remove(&id);
                let _ = self.send_transaction_cancellation_senders.remove(&id);
                if let TransactionServiceError::Shutdown = error {
                    return;
                }
                warn!(
                    target: LOG_TARGET,
                    "Error completing Send Transaction Protocol (Id: {}): {:?}", id, error
                );
                let _ = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    /// Cancel a pending transaction
    async fn cancel_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        self.db.cancel_pending_transaction(tx_id).await.map_err(|e| {
            warn!(
                target: LOG_TARGET,
                "Pending Transaction does not exist and could not be cancelled: {:?}", e
            );
            e
        })?;

        self.output_manager_service.cancel_transaction(tx_id).await?;

        if let Some(cancellation_sender) = self.send_transaction_cancellation_senders.remove(&tx_id) {
            let _ = cancellation_sender.send(());
        }
        let _ = self.pending_transaction_reply_senders.remove(&tx_id);

        if let Some(cancellation_sender) = self.receiver_transaction_cancellation_senders.remove(&tx_id) {
            let _ = cancellation_sender.send(());
        }
        let _ = self.finalized_transaction_senders.remove(&tx_id);

        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCancelled(tx_id)))
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
    ) -> Result<(), TransactionServiceError>
    {
        let tx_id = transaction_cancelled.tx_id;

        // Check that an inbound transaction exists to be cancelled and that the Source Public key for that transaction
        // is the same as the cancellation message
        if let Ok(inbound_tx) = self.db.get_pending_inbound_transaction(tx_id).await {
            if inbound_tx.source_public_key == source_pubkey {
                self.cancel_transaction(tx_id).await?;
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
    async fn restart_all_send_transaction_protocols(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        let outbound_txs = self.db.get_pending_outbound_transactions().await?;
        for (tx_id, tx) in outbound_txs {
            if !self.pending_transaction_reply_senders.contains_key(&tx_id) {
                debug!(
                    target: LOG_TARGET,
                    "Restarting listening for Reply for Pending Outbound Transaction TxId: {}", tx_id
                );
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
                    tx.message,
                    tx.sender_protocol,
                    TransactionSendProtocolStage::WaitForReply,
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
    pub async fn accept_transaction(
        &mut self,
        source_pubkey: CommsPublicKey,
        sender_message: proto::TransactionSenderMessage,
        traced_message_tag: u64,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
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

            // Check if this transaction has already been received.
            if let Ok(inbound_tx) = self.db.get_pending_inbound_transaction(data.tx_id).await {
                // Check that it is from the same person
                if inbound_tx.source_public_key != source_pubkey {
                    return Err(TransactionServiceError::InvalidSourcePublicKey);
                }
                // Check if the last reply is beyond the resend cooldown
                if let Some(timestamp) = inbound_tx.last_send_timestamp {
                    let elapsed_time = Utc::now()
                        .naive_utc()
                        .signed_duration_since(timestamp)
                        .to_std()
                        .map_err(|_| {
                            TransactionServiceError::ConversionError("duration::OutOfRangeError".to_string())
                        })?;
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
                ));
                if let Err(e) = self.resources.db.increment_send_count(tx_id).await {
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
    ) -> Result<(), TransactionServiceError>
    {
        let tx_id = finalized_transaction.tx_id;
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
            None => return Err(TransactionServiceError::TransactionDoesNotExistError),
            Some(s) => s,
        };

        sender
            .send((source_pubkey, tx_id, transaction))
            .await
            .map_err(|_| TransactionServiceError::ProtocolChannelError)?;

        Ok(())
    }

    /// Handle the final clean up after a Send Transaction protocol completes
    async fn complete_receive_transaction_protocol(
        &mut self,
        join_result: Result<u64, TransactionServiceProtocolError>,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
    )
    {
        match join_result {
            Ok(id) => {
                let _ = self.finalized_transaction_senders.remove(&id);
                let _ = self.receiver_transaction_cancellation_senders.remove(&id);

                let _ = self
                    .broadcast_completed_transaction_to_mempool(id, transaction_broadcast_join_handles)
                    .await
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
                let _ = self.finalized_transaction_senders.remove(&id);
                let _ = self.receiver_transaction_cancellation_senders.remove(&id);
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

                let _ = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    async fn restart_all_receive_transaction_protocols(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        let inbound_txs = self.db.get_pending_inbound_transactions().await?;
        for (tx_id, tx) in inbound_txs {
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
                    tx.source_public_key,
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

        Ok(())
    }

    /// Add a base node public key to the list that will be used to broadcast transactions and monitor the base chain
    /// for the presence of spendable outputs. If this is the first time the base node public key is set do the initial
    /// mempool broadcast
    fn set_base_node_public_key(&mut self, base_node_public_key: CommsPublicKey) {
        self.base_node_public_key = Some(base_node_public_key);
    }

    async fn restart_transaction_negotiation_protocols(
        &mut self,
        send_transaction_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
        receive_transaction_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
    ) -> Result<(), TransactionServiceError>
    {
        trace!(target: LOG_TARGET, "Restarting transaction negotiation protocols");
        self.restart_all_send_transaction_protocols(send_transaction_join_handles)
            .await
            .map_err(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Error restarting protocols for all pending outbound transactions: {:?}", resp
                );
                resp
            })?;

        self.restart_all_receive_transaction_protocols(receive_transaction_join_handles)
            .await
            .map_err(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Error restarting protocols for all coinbase transactions: {:?}", resp
                );
                resp
            })?;

        Ok(())
    }

    async fn restart_broadcast_protocols(
        &mut self,
        broadcast_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
        chain_monitoring_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
        coinbase_transaction_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
    ) -> Result<(), TransactionServiceError>
    {
        if self.base_node_public_key.is_none() {
            return Err(TransactionServiceError::NoBaseNodeKeysProvided);
        }
        trace!(target: LOG_TARGET, "Restarting transaction broadcast protocols");
        self.broadcast_all_completed_transactions_to_mempool(broadcast_join_handles)
            .await
            .map_err(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Error broadcasting all completed transactions: {:?}", resp
                );
                resp
            })?;

        self.start_chain_monitoring_for_all_broadcast_transactions(chain_monitoring_join_handles)
            .await
            .map_err(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Error querying base_node for all completed transactions: {:?}", resp
                );
                resp
            })?;

        self.restart_chain_monitoring_for_all_coinbase_transactions(coinbase_transaction_join_handles)
            .await
            .map_err(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Error restarting protocols for all coinbase transactions: {:?}", resp
                );
                resp
            })?;
        Ok(())
    }

    /// Broadcast the specified Completed Transaction to the Base Node. After sending the transaction send a Mempool
    /// request to check that the transaction has been received. The final step is to set a timeout future to check on
    /// the status of the transaction in the future.
    pub async fn broadcast_completed_transaction_to_mempool(
        &mut self,
        tx_id: TxId,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_tx = self.db.get_completed_transaction(tx_id).await?;

        if completed_tx.status != TransactionStatus::Completed || completed_tx.transaction.body.kernels().is_empty() {
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }
        let timeout = match self.power_mode {
            PowerMode::Normal => self.config.broadcast_monitoring_timeout,
            PowerMode::Low => self.config.low_power_polling_timeout,
        };
        match self.base_node_public_key.clone() {
            None => return Err(TransactionServiceError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let (mempool_response_sender, mempool_response_receiver) = mpsc::channel(500);
                let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(500);
                self.mempool_response_senders.insert(tx_id, mempool_response_sender);
                self.base_node_response_senders.insert(tx_id, base_node_response_sender);
                let protocol = TransactionBroadcastProtocol::new(
                    tx_id,
                    self.resources.clone(),
                    timeout,
                    pk,
                    mempool_response_receiver,
                    base_node_response_receiver,
                    self.timeout_update_publisher.subscribe(),
                );
                let join_handle = tokio::spawn(protocol.execute());
                join_handles.push(join_handle);
            },
        }

        Ok(())
    }

    /// Go through all completed transactions that have not yet been broadcast and broadcast all of them to the base
    /// node followed by mempool requests to confirm that they have been received
    async fn broadcast_all_completed_transactions_to_mempool(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        trace!(target: LOG_TARGET, "Attempting to Broadcast all Completed Transactions");
        let completed_txs = self.db.get_completed_transactions().await?;
        for completed_tx in completed_txs.values() {
            if completed_tx.status == TransactionStatus::Completed &&
                !self.mempool_response_senders.contains_key(&completed_tx.tx_id)
            {
                self.broadcast_completed_transaction_to_mempool(completed_tx.tx_id, join_handles)
                    .await?;
            }
        }

        Ok(())
    }

    /// Handle an incoming mempool response message
    pub async fn handle_mempool_response(
        &mut self,
        response: MempoolProto::MempoolServiceResponse,
    ) -> Result<(), TransactionServiceError>
    {
        let response = MempoolServiceResponse::try_from(response).unwrap();
        trace!(target: LOG_TARGET, "Received Mempool Response: {:?}", response);

        let tx_id = response.request_key;

        let sender = match self.mempool_response_senders.get_mut(&tx_id) {
            None => {
                trace!(
                    target: LOG_TARGET,
                    "Received Mempool response with unexpected key: {}. Not for this service",
                    response.request_key
                );
                return Ok(());
            },
            Some(s) => s,
        };
        sender
            .send(response)
            .await
            .map_err(|_| TransactionServiceError::ProtocolChannelError)?;

        Ok(())
    }

    /// Handle the final clean up after a Transaction Broadcast protocol completes
    async fn complete_transaction_broadcast_protocol(
        &mut self,
        join_result: Result<u64, TransactionServiceProtocolError>,
        transaction_chain_monitoring_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
    )
    {
        match join_result {
            Ok(id) => {
                // Cleanup any registered senders
                let _ = self.mempool_response_senders.remove(&id);
                let _ = self.base_node_response_senders.remove(&id);
                trace!(
                    target: LOG_TARGET,
                    "Transaction Broadcast Protocol for TxId: {} completed successfully",
                    id
                );
                let _ = self
                    .start_transaction_chain_monitoring_protocol(id, transaction_chain_monitoring_join_handles)
                    .await
                    .map_err(|resp| {
                        match resp {
                            TransactionServiceError::InvalidCompletedTransaction => trace!(
                                target: LOG_TARGET,
                                "Not starting Chain monitoring protocol as transaction cannot be found, either \
                                 cancelled or already mined."
                            ),
                            _ => error!(
                                target: LOG_TARGET,
                                "Error starting Chain Monitoring Protocol after completed Broadcast Protocol : {:?}",
                                resp
                            ),
                        }
                        resp
                    });
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _ = self.mempool_response_senders.remove(&id);
                let _ = self.base_node_response_senders.remove(&id);

                if let TransactionServiceError::Shutdown = error {
                    return;
                }

                warn!(
                    target: LOG_TARGET,
                    "Error completing Transaction Broadcast Protocol (Id: {}): {:?}", id, error
                );
                let _ = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    /// Send a request to the Base Node to see if the specified transaction has been mined yet. This function will send
    /// the request and store a timeout future to check in on the status of the transaction in the future.
    async fn start_transaction_chain_monitoring_protocol(
        &mut self,
        tx_id: TxId,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_tx = self.db.get_completed_transaction(tx_id).await?;

        if completed_tx.status != TransactionStatus::Broadcast || completed_tx.transaction.body.kernels().is_empty() {
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }
        let timeout = match self.power_mode {
            PowerMode::Normal => self.config.chain_monitoring_timeout,
            PowerMode::Low => self.config.low_power_polling_timeout,
        };
        match self.base_node_public_key.clone() {
            None => return Err(TransactionServiceError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let protocol_id = OsRng.next_u64();

                let (mempool_response_sender, mempool_response_receiver) = mpsc::channel(100);
                let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(100);
                self.mempool_response_senders
                    .insert(protocol_id, mempool_response_sender);
                self.base_node_response_senders
                    .insert(protocol_id, base_node_response_sender);
                let protocol = TransactionChainMonitoringProtocol::new(
                    protocol_id,
                    completed_tx.tx_id,
                    self.resources.clone(),
                    timeout,
                    pk,
                    mempool_response_receiver,
                    base_node_response_receiver,
                    self.timeout_update_publisher.subscribe(),
                );
                let join_handle = tokio::spawn(protocol.execute());
                join_handles.push(join_handle);
            },
        }
        Ok(())
    }

    /// Handle the final clean up after a Transaction Chain Monitoring protocol completes
    fn complete_transaction_chain_monitoring_protocol(
        &mut self,
        join_result: Result<u64, TransactionServiceProtocolError>,
    )
    {
        match join_result {
            Ok(id) => {
                // Cleanup any registered senders
                let _ = self.mempool_response_senders.remove(&id);
                let _ = self.base_node_response_senders.remove(&id);

                trace!(
                    target: LOG_TARGET,
                    "Transaction chain monitoring Protocol for TxId: {} completed successfully",
                    id
                );
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _ = self.mempool_response_senders.remove(&id);
                let _ = self.base_node_response_senders.remove(&id);
                if let TransactionServiceError::Shutdown = error {
                    return;
                }
                warn!(
                    target: LOG_TARGET,
                    "Error completing Transaction chain monitoring Protocol (Id: {}): {:?}", id, error
                );
                let _ = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    /// Handle an incoming basenode response message
    pub async fn handle_base_node_response(
        &mut self,
        response: BaseNodeProto::BaseNodeServiceResponse,
    ) -> Result<(), TransactionServiceError>
    {
        let sender = match self.base_node_response_senders.get_mut(&response.request_key) {
            None => {
                trace!(
                    target: LOG_TARGET,
                    "Received Base Node response with unexpected key: {}. Not for this service",
                    response.request_key
                );
                return Ok(());
            },
            Some(s) => s,
        };
        sender
            .send(response.clone())
            .await
            .map_err(|_| TransactionServiceError::ProtocolChannelError)?;

        Ok(())
    }

    /// Go through all completed transactions that have been broadcast and start querying the base_node to see if they
    /// have been mined
    async fn start_chain_monitoring_for_all_broadcast_transactions(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        trace!(
            target: LOG_TARGET,
            "Starting Chain monitoring for all Broadcast Transactions"
        );
        let completed_txs = self.db.get_completed_transactions().await?;
        for completed_tx in completed_txs.values() {
            if completed_tx.status == TransactionStatus::Broadcast {
                self.start_transaction_chain_monitoring_protocol(completed_tx.tx_id, join_handles)
                    .await?;
            }
        }

        Ok(())
    }

    async fn set_power_mode(&mut self, mode: PowerMode) -> Result<(), TransactionServiceError> {
        self.power_mode = mode;
        let timeout = match mode {
            PowerMode::Low => self.config.low_power_polling_timeout,
            PowerMode::Normal => self.config.broadcast_monitoring_timeout,
        };
        if let Err(e) = self.timeout_update_publisher.send(timeout) {
            trace!(
                target: LOG_TARGET,
                "Could not send Timeout update, no subscribers to receive. (Err {:?})",
                e
            );
        }

        Ok(())
    }

    /// Add a completed transaction to the Transaction Manager to record directly importing a spendable UTXO.
    pub async fn add_utxo_import_transaction(
        &mut self,
        value: MicroTari,
        source_public_key: CommsPublicKey,
        message: String,
    ) -> Result<TxId, TransactionServiceError>
    {
        let tx_id = OsRng.next_u64();
        self.db
            .add_utxo_import_transaction(
                tx_id,
                value,
                source_public_key,
                self.node_identity.public_key().clone(),
                message,
            )
            .await?;
        Ok(tx_id)
    }

    /// Submit a completed transaction to the Transaction Manager
    pub async fn submit_transaction(
        &mut self,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
        tx_id: TxId,
        tx: Transaction,
        fee: MicroTari,
        amount: MicroTari,
        message: String,
    ) -> Result<(), TransactionServiceError>
    {
        trace!(target: LOG_TARGET, "Submit transaction ({}) to db.", tx_id);
        self.db
            .insert_completed_transaction(
                tx_id,
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
                ),
            )
            .await?;
        trace!(
            target: LOG_TARGET,
            "Launch the transaction broadcast protocol for submitted transaction ({}).",
            tx_id
        );
        self.complete_send_transaction_protocol(Ok(tx_id), transaction_broadcast_join_handles)
            .await;
        Ok(())
    }

    async fn generate_coinbase_transaction(
        &mut self,
        reward: MicroTari,
        fees: MicroTari,
        block_height: u64,
        coinbase_monitoring_protocol_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
    ) -> Result<Transaction, TransactionServiceError>
    {
        let tx_id = OsRng.next_u64();
        let total_reward = reward + fees;
        let spending_key = self
            .output_manager_service
            .get_coinbase_spending_key(tx_id, total_reward, block_height)
            .await?;
        let nonce = PrivateKey::random(&mut OsRng);
        let (tx, _) = CoinbaseBuilder::new(self.resources.factories.clone())
            .with_block_height(block_height)
            .with_fees(fees)
            .with_spend_key(spending_key)
            .with_nonce(nonce)
            .build_with_reward(&self.resources.consensus_constants, reward)?;

        // Cancel existing unmined coinbase transactions for this blockheight
        self.db
            .cancel_coinbase_transaction_at_block_height(block_height)
            .await?;

        self.db
            .insert_completed_transaction(
                tx_id,
                CompletedTransaction::new(
                    tx_id,
                    self.node_identity.public_key().clone(),
                    self.node_identity.public_key().clone(),
                    total_reward,
                    MicroTari::from(0),
                    tx.clone(),
                    TransactionStatus::Coinbase,
                    format!("Coinbase Transaction for Block {}", block_height),
                    Utc::now().naive_utc(),
                    TransactionDirection::Inbound,
                    Some(block_height),
                ),
            )
            .await?;

        debug!(
            target: LOG_TARGET,
            "Coinbase transaction (TxId: {}) for Block Height: {} added", tx_id, block_height
        );

        if let Err(e) = self
            .start_coinbase_transaction_monitoring_protocol(tx_id, coinbase_monitoring_protocol_join_handles)
            .await
        {
            warn!(
                target: LOG_TARGET,
                "Could not start chain monitoring for Coinbase transaction (TxId: {}): {:?}", tx_id, e
            );
        }

        Ok(tx)
    }

    /// Send a request to the Base Node to see if the specified coinbase transaction has been mined yet. This function
    /// will send the request and store a timeout future to check in on the status of the transaction in the future.
    async fn start_coinbase_transaction_monitoring_protocol(
        &mut self,
        tx_id: TxId,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_tx = self.db.get_completed_transaction(tx_id).await?;

        if completed_tx.status != TransactionStatus::Coinbase || completed_tx.coinbase_block_height.is_none() {
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }

        let block_height = if let Some(bh) = completed_tx.coinbase_block_height {
            bh
        } else {
            0
        };

        let timeout = match self.power_mode {
            PowerMode::Normal => self.config.broadcast_monitoring_timeout,
            PowerMode::Low => self.config.low_power_polling_timeout,
        };
        match self.base_node_public_key.clone() {
            None => return Err(TransactionServiceError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let protocol_id = OsRng.next_u64();

                let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(100);
                self.base_node_response_senders
                    .insert(protocol_id, base_node_response_sender);
                let protocol = TransactionCoinbaseMonitoringProtocol::new(
                    protocol_id,
                    completed_tx.tx_id,
                    block_height,
                    self.resources.clone(),
                    timeout,
                    pk,
                    base_node_response_receiver,
                    self.timeout_update_publisher.subscribe(),
                );
                let join_handle = tokio::spawn(protocol.execute());
                join_handles.push(join_handle);
            },
        }
        Ok(())
    }

    /// Handle the final clean up after a Coinbase Transaction Monitoring protocol completes
    fn complete_coinbase_transaction_monitoring_protocol(
        &mut self,
        join_result: Result<u64, TransactionServiceProtocolError>,
    )
    {
        match join_result {
            Ok(id) => {
                // Cleanup any registered senders
                let _ = self.base_node_response_senders.remove(&id);

                trace!(
                    target: LOG_TARGET,
                    "Coinbase Transaction monitoring Protocol for TxId: {} completed successfully",
                    id
                );
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _ = self.base_node_response_senders.remove(&id);
                if let TransactionServiceError::Shutdown = error {
                    return;
                }
                warn!(
                    target: LOG_TARGET,
                    "Error completing Coinbase Transaction monitoring Protocol (Id: {}): {:?}", id, error
                );
                let _ = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    /// Go through all completed transactions that have the Coinbase status and start querying the base_node to see if
    /// they have been mined
    async fn restart_chain_monitoring_for_all_coinbase_transactions(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        trace!(
            target: LOG_TARGET,
            "Starting Coinbase monitoring for all Broadcast Transactions"
        );
        let completed_txs = self.db.get_completed_transactions().await?;
        for completed_tx in completed_txs.values() {
            if completed_tx.status == TransactionStatus::Coinbase {
                self.start_coinbase_transaction_monitoring_protocol(completed_tx.tx_id, join_handles)
                    .await?;
            }
        }

        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. It simulates a receiver accepting and
    /// replying to a Pending Outbound Transaction. This results in that transaction being "completed" and it's status
    /// set to `Broadcast` which indicated it is in a base_layer mempool.
    #[cfg(feature = "test_harness")]
    pub async fn complete_pending_outbound_transaction(
        &mut self,
        completed_tx: CompletedTransaction,
    ) -> Result<(), TransactionServiceError>
    {
        self.db
            .complete_outbound_transaction(completed_tx.tx_id, completed_tx.clone())
            .await?;
        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function will simulate the process
    /// when a completed transaction is broadcast in a mempool on the base layer. The function will update the status of
    /// the completed transaction.
    #[cfg(feature = "test_harness")]
    pub async fn broadcast_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let completed_txs = self.db.get_completed_transactions().await?;
        completed_txs.get(&tx_id).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Completed TX to broadcast.".to_string())
        })?;

        self.db.broadcast_completed_transaction(tx_id).await?;

        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionBroadcast(tx_id)))
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event, usually because there are no subscribers: {:?}",
                    e
                );
                e
            });

        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function will simulate the process
    /// when a completed transaction is detected as mined on the base layer. The function will update the status of the
    /// completed transaction AND complete the transaction on the Output Manager Service which will update the status of
    /// the outputs
    #[cfg(feature = "test_harness")]
    pub async fn mine_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let completed_txs = self.db.get_completed_transactions().await?;
        let _found_tx = completed_txs.get(&tx_id).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Completed TX to mine.".to_string())
        })?;

        let pending_tx_outputs = self.output_manager_service.get_pending_transactions().await?;
        let pending_tx = pending_tx_outputs.get(&tx_id).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Pending TX to complete.".to_string())
        })?;

        self.output_manager_service
            .confirm_transaction(
                tx_id,
                pending_tx
                    .outputs_to_be_spent
                    .iter()
                    .map(|o| o.unblinded_output.as_transaction_input())
                    .collect(),
                pending_tx
                    .outputs_to_be_received
                    .iter()
                    .map(|o| {
                        o.unblinded_output
                            .as_transaction_output(&self.resources.factories)
                            .expect("Failed to convert to Transaction Output")
                    })
                    .collect(),
            )
            .await?;

        self.db.mine_completed_transaction(tx_id).await?;

        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionMined(tx_id)))
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event, usually because there are no subscribers: {:?}",
                    e
                );
                e
            });

        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function simulates an external
    /// wallet sending a transaction to this wallet which will become a PendingInboundTransaction
    #[cfg(feature = "test_harness")]
    pub async fn receive_test_transaction(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        source_public_key: CommsPublicKey,
    ) -> Result<(), TransactionServiceError>
    {
        use crate::{
            output_manager_service::{
                config::OutputManagerServiceConfig,
                service::OutputManagerService,
                storage::{database::OutputManagerDatabase, memory_db::OutputManagerMemoryDatabase},
            },
            transaction_service::{handle::TransactionServiceHandle, storage::models::InboundTransaction},
        };
        use futures::stream;
        use tari_core::{
            consensus::{ConsensusConstantsBuilder, Network},
            transactions::{OutputFeatures, ReceiverTransactionProtocol},
        };
        use tari_crypto::keys::SecretKey as SecretKeyTrait;
        use tari_shutdown::Shutdown;

        let (_sender, receiver) = reply_channel::unbounded();
        let (tx, _rx) = mpsc::channel(20);
        let (oms_event_publisher, _oms_event_subscriber) = broadcast::channel(100);
        let (ts_request_sender, _ts_request_receiver) = reply_channel::unbounded();
        let (event_publisher, _) = broadcast::channel(100);
        let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher.clone());
        let constants = ConsensusConstantsBuilder::new(Network::Rincewind).build();
        let shutdown = Shutdown::new();
        let mut fake_oms = OutputManagerService::new(
            OutputManagerServiceConfig::default(),
            OutboundMessageRequester::new(tx),
            ts_handle,
            receiver,
            stream::empty(),
            OutputManagerDatabase::new(OutputManagerMemoryDatabase::new()),
            oms_event_publisher,
            self.resources.factories.clone(),
            constants.coinbase_lock_height(),
            shutdown.to_signal(),
        )
        .await?;

        let uo = OutputBuilder::new()
            .with_value(amount + 1000 * uT)
            .build(&self.resources.factories.commitment)
            .unwrap();

        fake_oms.add_output(uo).await?;

        let mut stp = fake_oms
            .prepare_transaction_to_send(amount, MicroTari::from(25), None, "".to_string())
            .await?;

        let msg = stp.build_single_round_message()?;
        let proto_msg = proto::TransactionSenderMessage::single(msg.into());
        let sender_message: TransactionSenderMessage = proto_msg
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let spending_key = self
            .output_manager_service
            .get_recipient_spending_key(tx_id, amount)
            .await?;
        let nonce = PrivateKey::random(&mut OsRng);
        let rtp = ReceiverTransactionProtocol::new(
            sender_message,
            nonce,
            spending_key.clone(),
            OutputFeatures::default(),
            &self.resources.factories,
        );

        let inbound_transaction = InboundTransaction::new(
            tx_id,
            source_public_key,
            amount,
            rtp,
            TransactionStatus::Pending,
            "".to_string(),
            Utc::now().naive_utc(),
        );

        self.db
            .add_pending_inbound_transaction(tx_id, inbound_transaction.clone())
            .await?;

        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::ReceivedTransaction(tx_id)))
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event, usually because there are no subscribers: {:?}",
                    e
                );
                e
            });

        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function simulates an external
    /// wallet sending a transaction to this wallet which will become a PendingInboundTransaction
    #[cfg(feature = "test_harness")]
    pub async fn finalize_received_test_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let inbound_txs = self.db.get_pending_inbound_transactions().await?;

        let found_tx = inbound_txs.get(&tx_id).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Pending Inbound TX to finalize.".to_string())
        })?;

        let completed_transaction = CompletedTransaction::new(
            tx_id,
            found_tx.source_public_key.clone(),
            self.node_identity.public_key().clone(),
            found_tx.amount,
            MicroTari::from(2000), // a placeholder fee for this test function
            Transaction::new(Vec::new(), Vec::new(), Vec::new(), BlindingFactor::default()),
            TransactionStatus::Completed,
            found_tx.message.clone(),
            found_tx.timestamp,
            TransactionDirection::Inbound,
            None,
        );

        self.db
            .complete_inbound_transaction(tx_id, completed_transaction.clone())
            .await?;
        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::ReceivedFinalizedTransaction(tx_id)))
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event, usually because there are no subscribers: {:?}",
                    e
                );
                e
            });
        Ok(())
    }
}

/// This struct is a collection of the common resources that a protocol in the service requires.
#[derive(Clone)]
pub struct TransactionServiceResources<TBackend>
where TBackend: TransactionBackend + 'static
{
    pub db: TransactionDatabase<TBackend>,
    pub output_manager_service: OutputManagerHandle,
    pub outbound_message_service: OutboundMessageRequester,
    pub event_publisher: TransactionEventSender,
    pub node_identity: Arc<NodeIdentity>,
    pub factories: CryptoFactories,
    pub config: TransactionServiceConfig,
    pub consensus_constants: ConsensusConstants,
    pub shutdown_signal: ShutdownSignal,
}

#[derive(Clone, Copy)]
enum PowerMode {
    Low,
    Normal,
}
