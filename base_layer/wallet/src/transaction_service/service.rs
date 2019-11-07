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

#[cfg(feature = "test_harness")]
use crate::output_manager_service::TxId;
use crate::{
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionServiceRequest, TransactionServiceResponse},
        storage::database::{
            CompletedTransaction,
            InboundTransaction,
            OutboundTransaction,
            TransactionBackend,
            TransactionDatabase,
            TransactionStatus,
        },
    },
    types::TransactionRng,
};
use chrono::Utc;
use futures::{pin_mut, SinkExt, Stream, StreamExt};
use log::*;
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tari_broadcast_channel::Publisher;
use tari_comms::{peer_manager::NodeIdentity, types::CommsPublicKey};
use tari_comms_dht::{
    broadcast_strategy::BroadcastStrategy,
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{OutboundEncryption, OutboundMessageRequester},
};
use tari_crypto::keys::SecretKey;
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_transactions::{
    aggregated_body::AggregateBody,
    tari_amount::MicroTari,
    transaction::{KernelFeatures, OutputFeatures, Transaction},
    transaction_protocol::{proto, recipient::RecipientSignedMessage, sender::TransactionSenderMessage},
    types::{PrivateKey, COMMITMENT_FACTORY, PROVER},
    ReceiverTransactionProtocol,
};

const LOG_TARGET: &'static str = "base_layer::wallet::transaction_service::service";

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

pub struct TransactionService<TTxStream, TTxReplyStream, TBackend>
where TBackend: TransactionBackend
{
    db: TransactionDatabase<TBackend>,
    outbound_message_service: OutboundMessageRequester,
    output_manager_service: OutputManagerHandle,
    transaction_stream: Option<TTxStream>,
    transaction_reply_stream: Option<TTxReplyStream>,
    request_stream: Option<
        reply_channel::Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    >,
    event_publisher: Publisher<TransactionEvent>,
    node_identity: Arc<NodeIdentity>,
}

impl<TTxStream, TTxReplyStream, TBackend> TransactionService<TTxStream, TTxReplyStream, TBackend>
where
    TTxStream: Stream<Item = DomainMessage<proto::TransactionSenderMessage>>,
    TTxReplyStream: Stream<Item = DomainMessage<proto::RecipientSignedMessage>>,
    TBackend: TransactionBackend,
{
    pub fn new(
        db: TransactionDatabase<TBackend>,
        request_stream: Receiver<
            TransactionServiceRequest,
            Result<TransactionServiceResponse, TransactionServiceError>,
        >,
        transaction_stream: TTxStream,
        transaction_reply_stream: TTxReplyStream,
        output_manager_service: OutputManagerHandle,
        outbound_message_service: OutboundMessageRequester,
        event_publisher: Publisher<TransactionEvent>,
        node_identity: Arc<NodeIdentity>,
    ) -> Self
    {
        TransactionService {
            db,
            outbound_message_service,
            output_manager_service,
            transaction_stream: Some(transaction_stream),
            transaction_reply_stream: Some(transaction_reply_stream),
            request_stream: Some(request_stream),
            event_publisher,
            node_identity,
        }
    }

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
        loop {
            futures::select! {
                //Incoming request
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                // Incoming messages from the Comms layer
                msg = transaction_stream.select_next_some() => {
                    let result  = self.accept_transaction(msg.origin_pubkey, msg.inner).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming message: {:?}", err);
                        Err(err)
                    });

                    if result.is_err() {
                        let _ = self.event_publisher
                                .send(TransactionEvent::Error(
                                    "Error handling Transaction Sender message".to_string(),
                                ))
                                .await;
                    }
                },
                 // Incoming messages from the Comms layer
                msg = transaction_reply_stream.select_next_some() => {
                    let result = self.accept_recipient_reply(msg.inner).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming message: {:?}", err);
                        Err(err)
                    });

                    if result.is_err() {
                        let _ = self.event_publisher
                                .send(TransactionEvent::Error(
                                    "Error handling Transaction Recipient Reply message".to_string(),
                                ))
                                .await;
                    }
                },
                complete => {
                    info!(target: LOG_TARGET, "Text message service shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    /// This handler is called when requests arrive from the various streams
    async fn handle_request(
        &mut self,
        request: TransactionServiceRequest,
    ) -> Result<TransactionServiceResponse, TransactionServiceError>
    {
        match request {
            TransactionServiceRequest::SendTransaction((dest_pubkey, amount, fee_per_gram, message)) => self
                .send_transaction(dest_pubkey, amount, fee_per_gram, message)
                .await
                .map(|_| TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::GetPendingInboundTransactions => Ok(
                TransactionServiceResponse::PendingInboundTransactions(self.get_pending_inbound_transactions()?),
            ),
            TransactionServiceRequest::GetPendingOutboundTransactions => Ok(
                TransactionServiceResponse::PendingOutboundTransactions(self.get_pending_outbound_transactions()?),
            ),
            TransactionServiceRequest::GetCompletedTransactions => Ok(
                TransactionServiceResponse::CompletedTransactions(self.get_completed_transactions()?),
            ),
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::CompletePendingOutboundTransaction(completed_transaction) => {
                self.complete_pending_outbound_transaction(completed_transaction)
                    .await?;
                Ok(TransactionServiceResponse::CompletedPendingTransaction)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::AcceptTestTransaction((tx_id, amount, source_pubkey)) => {
                self.receive_test_transaction(tx_id, amount, source_pubkey).await?;
                Ok(TransactionServiceResponse::AcceptedTestTransaction)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::MineCompletedTransaction(tx_id) => {
                self.mine_broadcast_transaction(tx_id).await?;
                Ok(TransactionServiceResponse::CompletedTransactionMined)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::BroadcastInboundTransaction(tx_id) => {
                self.detect_broadcast_of_inbound_transaction(tx_id).await?;
                Ok(TransactionServiceResponse::InboundTransactionBroadcast)
            },
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
    ) -> Result<(), TransactionServiceError>
    {
        let mut sender_protocol = self
            .output_manager_service
            .prepare_transaction_to_send(amount, fee_per_gram, None, message)
            .await?;

        if !sender_protocol.is_single_round_message_ready() {
            return Err(TransactionServiceError::InvalidStateError);
        }

        let msg = sender_protocol.build_single_round_message()?;
        let tx_id = msg.tx_id;
        let proto_message = proto::TransactionSenderMessage::single(msg.into());

        self.outbound_message_service
            .send_direct(
                dest_pubkey.clone(),
                OutboundEncryption::EncryptForDestination,
                OutboundDomainMessage::new(TariMessageType::Transaction, proto_message),
            )
            .await?;

        self.db.add_pending_outbound_transaction(tx_id, OutboundTransaction {
            tx_id,
            destination_public_key: dest_pubkey.clone(),
            amount,
            fee: sender_protocol.get_fee_amount()?,
            sender_protocol,
            message: "".to_string(),
            timestamp: Utc::now().naive_utc(),
        })?;

        info!(
            target: LOG_TARGET,
            "Transaction with TX_ID = {} sent to {}", tx_id, dest_pubkey
        );

        Ok(())
    }

    /// Accept the public reply from a recipient and apply the reply to the relevant transaction protocol
    /// # Arguments
    /// 'recipient_reply' - The public response from a recipient with data required to complete the transaction
    pub async fn accept_recipient_reply(
        &mut self,
        recipient_reply: proto::RecipientSignedMessage,
    ) -> Result<(), TransactionServiceError>
    {
        let recipient_reply: RecipientSignedMessage = recipient_reply
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let mut outbound_tx = self
            .db
            .get_pending_outbound_transaction(recipient_reply.tx_id.clone())?;

        let tx_id = recipient_reply.tx_id.clone();
        if !outbound_tx.sender_protocol.check_tx_id(tx_id.clone()) ||
            !outbound_tx.sender_protocol.is_collecting_single_signature()
        {
            return Err(TransactionServiceError::InvalidStateError);
        }

        outbound_tx
            .sender_protocol
            .add_single_recipient_info(recipient_reply, &PROVER)?;
        outbound_tx
            .sender_protocol
            .finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY)?;
        let tx = outbound_tx.sender_protocol.get_transaction()?;

        // TODO Broadcast this to the chain
        // TODO Only confirm this transaction once it is detected on chain and then complete in Output Manager Service
        // to make funds available
        self.db
            .complete_outbound_transaction(tx_id.clone(), CompletedTransaction {
                tx_id,
                source_public_key: self.node_identity.public_key().clone(),
                destination_public_key: outbound_tx.destination_public_key,
                amount: outbound_tx.amount,
                fee: outbound_tx.fee,
                transaction: tx.clone(),
                status: TransactionStatus::Broadcast,
                message: "".to_string(),
                timestamp: Utc::now().naive_utc(),
            })?;

        info!(
            target: LOG_TARGET,
            "Transaction Recipient Reply for TX_ID = {} received", tx_id,
        );
        self.event_publisher
            .send(TransactionEvent::ReceivedTransactionReply)
            .await
            .map_err(|_| TransactionServiceError::EventStreamError)?;

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
    ) -> Result<(), TransactionServiceError>
    {
        let sender_message: TransactionSenderMessage = sender_message
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        // Currently we will only reply to a Single sender transaction protocol
        if let TransactionSenderMessage::Single(data) = sender_message.clone() {
            let amount = data.amount.clone();

            let spending_key = self
                .output_manager_service
                .get_recipient_spending_key(data.tx_id, data.amount)
                .await?;
            let mut rng = TransactionRng::new().unwrap();
            let nonce = PrivateKey::random(&mut rng);

            let rtp = ReceiverTransactionProtocol::new(
                sender_message,
                nonce,
                spending_key,
                OutputFeatures::default(),
                &PROVER,
                &COMMITMENT_FACTORY,
            );
            let recipient_reply = rtp.get_signed_data()?.clone();

            // Check this is not a repeat message i.e. tx_id doesn't already exist in our pending or completed
            // transactions
            if self.db.transaction_exists(&recipient_reply.tx_id)? {
                return Err(TransactionServiceError::RepeatedMessageError);
            }

            let tx_id = recipient_reply.tx_id;
            let proto_message: proto::RecipientSignedMessage = recipient_reply.into();
            self.outbound_message_service
                .send_message(
                    BroadcastStrategy::DirectPublicKey(source_pubkey.clone()),
                    NodeDestination::Unknown,
                    OutboundEncryption::EncryptForDestination,
                    OutboundDomainMessage::new(TariMessageType::TransactionReply, proto_message),
                )
                .await?;

            // Otherwise add it to our pending transaction list and return reply
            self.db.add_pending_inbound_transaction(tx_id, InboundTransaction {
                tx_id,
                source_public_key: source_pubkey.clone(),
                amount,
                receiver_protocol: rtp.clone(),
                message: data.message,
                timestamp: Utc::now().naive_utc(),
            })?;

            info!(
                target: LOG_TARGET,
                "Transaction with TX_ID = {} received from {}. Reply Sent",
                tx_id,
                source_pubkey.clone()
            );

            self.event_publisher
                .send(TransactionEvent::ReceivedTransaction)
                .await
                .map_err(|_| TransactionServiceError::EventStreamError)?;
        }
        Ok(())
    }

    pub fn get_pending_inbound_transactions(
        &self,
    ) -> Result<HashMap<u64, InboundTransaction>, TransactionServiceError> {
        Ok(self.db.get_pending_inbound_transactions()?)
    }

    pub fn get_pending_outbound_transactions(
        &self,
    ) -> Result<HashMap<u64, OutboundTransaction>, TransactionServiceError> {
        Ok(self.db.get_pending_outbound_transactions()?)
    }

    pub fn get_completed_transactions(&self) -> Result<HashMap<u64, CompletedTransaction>, TransactionServiceError> {
        Ok(self.db.get_completed_transactions()?)
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
            .complete_outbound_transaction(completed_tx.tx_id.clone(), completed_tx.clone())?;
        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function will simulate the process
    /// when a compelted transaction is detected as mined on the base layer. The function will update the status of the
    /// completed transaction AND complete the transaction on the Output Manager Service which will update the status of
    /// the outputs
    #[cfg(feature = "test_harness")]
    pub async fn mine_broadcast_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        use tari_transactions::transaction::TransactionOutput;

        let completed_txs = self.db.get_completed_transactions()?;
        let _found_tx = completed_txs
            .get(&tx_id.clone())
            .ok_or(TransactionServiceError::TestHarnessError(
                "Could not find Completed TX to mine.".to_string(),
            ))?;

        let pending_tx_outputs = self.output_manager_service.get_pending_transactions().await?;
        let pending_tx = pending_tx_outputs
            .get(&tx_id.clone())
            .ok_or(TransactionServiceError::TestHarnessError(
                "Could not find Pending TX to complete.".to_string(),
            ))?;

        let outputs_to_be_spent = pending_tx
            .outputs_to_be_spent
            .clone()
            .iter()
            .map(|o| o.as_transaction_input(&COMMITMENT_FACTORY, OutputFeatures::default()))
            .collect();

        let mut outputs_to_be_received = Vec::new();

        for o in pending_tx.outputs_to_be_received.clone() {
            outputs_to_be_received.push(o.as_transaction_output(&PROVER, &COMMITMENT_FACTORY)?)
        }
        outputs_to_be_received.push(TransactionOutput::default());

        self.output_manager_service
            .confirm_sent_transaction(tx_id.clone(), outputs_to_be_spent, outputs_to_be_received)
            .await?;

        self.db.mine_completed_transaction(tx_id)?;

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
        use crate::output_manager_service::{
            service::OutputManagerService,
            storage::{database::OutputManagerDatabase, memory_db::OutputManagerMemoryDatabase},
            OutputManagerConfig,
        };
        use tari_comms::types::CommsSecretKey;
        use tari_crypto::keys::PublicKey;

        let (_sender, receiver) = reply_channel::unbounded();
        let mut rng = rand::OsRng::new().unwrap();
        let (secret_key, _public_key): (CommsSecretKey, CommsPublicKey) = PublicKey::random_keypair(&mut rng);

        let mut fake_oms = OutputManagerService::new(
            receiver,
            OutputManagerConfig {
                master_seed: secret_key,
                branch_seed: "".to_string(),
                primary_key_index: 0,
            },
            OutputManagerDatabase::new(OutputManagerMemoryDatabase::new()),
        )?;

        use crate::testnet_utils::make_input;
        let (_ti, uo) = make_input(&mut rng.clone(), MicroTari::from(amount + MicroTari::from(1_000_000)));

        fake_oms.add_output(uo)?;

        let mut stp = fake_oms.prepare_transaction_to_send(amount, MicroTari::from(100), None, "".to_string())?;

        let msg = stp.build_single_round_message()?;
        let proto_msg = proto::TransactionSenderMessage::single(msg.into());
        let sender_message: TransactionSenderMessage = proto_msg
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let spending_key = self
            .output_manager_service
            .get_recipient_spending_key(tx_id.clone(), amount.clone())
            .await?;
        let nonce = PrivateKey::random(&mut rng);
        let rtp = ReceiverTransactionProtocol::new(
            sender_message,
            nonce,
            spending_key.clone(),
            OutputFeatures::default(),
            &PROVER,
            &COMMITMENT_FACTORY,
        );

        self.db
            .add_pending_inbound_transaction(tx_id.clone(), InboundTransaction {
                tx_id,
                source_public_key,
                amount,
                receiver_protocol: rtp,
                message: "".to_string(),
                timestamp: Utc::now().naive_utc(),
            })?;

        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. It simulates the detection of a
    /// `PendingInboundTransaction` as being broadcast to base layer which means the Pending transaction must become a
    /// `CompletedTransaction` with the `Broadcast` status.
    #[cfg(feature = "test_harness")]
    pub async fn detect_broadcast_of_inbound_transaction(
        &mut self,
        tx_id: TxId,
    ) -> Result<(), TransactionServiceError>
    {
        let pending_inbound_txs = self.db.get_pending_inbound_transactions()?;

        let found_tx = pending_inbound_txs
            .get(&tx_id.clone())
            .ok_or(TransactionServiceError::TestHarnessError(
                "Could not find Pending Inbound TX to detect as broadcast.".to_string(),
            ))?;

        self.db
            .complete_inbound_transaction(found_tx.tx_id.clone(), CompletedTransaction {
                tx_id: found_tx.tx_id,
                source_public_key: found_tx.source_public_key.clone(),
                destination_public_key: self.node_identity.public_key().clone(),
                amount: found_tx.amount,
                fee: MicroTari::from(0),
                transaction: Transaction {
                    offset: Default::default(),
                    body: AggregateBody::empty(),
                },
                status: TransactionStatus::Broadcast,
                message: "".to_string(),
                timestamp: Utc::now().naive_utc(),
            })?;

        Ok(())
    }
}
