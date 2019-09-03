// Copyright 2019 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

use crate::{
    output_manager_service::{error::OutputManagerError, output_manager_service::OutputManagerServiceApi},
    types::TransactionRng,
};
use crossbeam_channel as channel;
use derive_error::Error;
use log::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_comms::{
    domain_subscriber::SyncDomainSubscription,
    message::MessageFlags,
    outbound_message_service::{outbound_message_service::OutboundMessageService, BroadcastStrategy, OutboundError},
    types::CommsPublicKey,
};
use tari_core::{
    tari_amount::MicroTari,
    transaction::{KernelFeatures, OutputFeatures, Transaction},
    transaction_protocol::{
        recipient::RecipientSignedMessage,
        sender::TransactionSenderMessage,
        TransactionProtocolError,
    },
    types::{PrivateKey, COMMITMENT_FACTORY, PROVER},
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};
use tari_crypto::keys::SecretKey;
use tari_p2p::{
    sync_services::{
        Service,
        ServiceApiWrapper,
        ServiceContext,
        ServiceControlMessage,
        ServiceError,
        DEFAULT_API_TIMEOUT_MS,
    },
    tari_message::{BlockchainMessage, TariMessageType},
};

const LOG_TARGET: &'static str = "base_layer::wallet::transaction_service";

#[derive(Debug, Error)]
pub enum TransactionServiceError {
    // Transaction protocol is not in the correct state for this operation
    InvalidStateError,
    // Transaction Protocol Error
    TransactionProtocolError(TransactionProtocolError),
    // The message being process is not recognized by the Transaction Manager
    InvalidMessageTypeError,
    // A message for a specific tx_id has been repeated
    RepeatedMessageError,
    // A recipient reply was received for a non-existent tx_id
    TransactionDoesNotExistError,
    /// The Outbound Message Service is not initialized
    OutboundMessageServiceNotInitialized,
    /// Received an unexpected API response
    UnexpectedApiResponse,
    /// Failed to send from API
    ApiSendFailed,
    /// Failed to receive in API from service
    ApiReceiveFailed,
    OutboundError(OutboundError),
    OutputManagerError(OutputManagerError),
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

pub struct TransactionService {
    pending_outbound_transactions: HashMap<u64, SenderTransactionProtocol>,
    pending_inbound_transactions: HashMap<u64, ReceiverTransactionProtocol>,
    completed_transactions: HashMap<u64, Transaction>,
    outbound_message_service: Option<Arc<OutboundMessageService>>,
    api: ServiceApiWrapper<TransactionServiceApi, TransactionServiceApiRequest, TransactionServiceApiResult>,
    output_manager_service: Arc<OutputManagerServiceApi>,
}

impl TransactionService {
    pub fn new(output_manager_service: Arc<OutputManagerServiceApi>) -> TransactionService {
        TransactionService {
            pending_outbound_transactions: HashMap::new(),
            pending_inbound_transactions: HashMap::new(),
            completed_transactions: HashMap::new(),
            outbound_message_service: None,
            api: Self::setup_api(),
            output_manager_service,
        }
    }

    fn setup_api() -> ServiceApiWrapper<TransactionServiceApi, TransactionServiceApiRequest, TransactionServiceApiResult>
    {
        let (api_sender, service_receiver) = channel::bounded(0);
        let (service_sender, api_receiver) = channel::bounded(0);

        let api = Arc::new(TransactionServiceApi::new(api_sender, api_receiver));
        ServiceApiWrapper::new(service_receiver, service_sender, api)
    }

    /// Return this service's API
    pub fn get_api(&self) -> Arc<TransactionServiceApi> {
        self.api.get_api()
    }

    /// Sends a new transaction to a recipient
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    pub fn send_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        fee_per_gram: MicroTari,
    ) -> Result<(), TransactionServiceError>
    {
        let outbound_message_service = self
            .outbound_message_service
            .clone()
            .ok_or(TransactionServiceError::OutboundMessageServiceNotInitialized)?;

        let mut stp = self
            .output_manager_service
            .prepare_transaction_to_send(amount, fee_per_gram, None)?;

        if !stp.is_single_round_message_ready() {
            return Err(TransactionServiceError::InvalidStateError);
        }

        let msg = stp.build_single_round_message()?;
        outbound_message_service.send_message(
            BroadcastStrategy::DirectPublicKey(dest_pubkey.clone()),
            MessageFlags::ENCRYPTED,
            TariMessageType::new(BlockchainMessage::Transaction),
            TransactionSenderMessage::Single(Box::new(msg.clone())),
        )?;

        self.pending_outbound_transactions.insert(msg.tx_id.clone(), stp);

        info!(
            target: LOG_TARGET,
            "Transaction with TX_ID = {} sent to {}",
            msg.tx_id.clone(),
            dest_pubkey
        );

        Ok(())
    }

    /// Accept the public reply from a recipient and apply the reply to the relevant transaction protocol
    /// # Arguments
    /// 'recipient_reply' - The public response from a recipient with data required to complete the transaction
    pub fn accept_recipient_reply(
        &mut self,
        recipient_reply: RecipientSignedMessage,
    ) -> Result<(), TransactionServiceError>
    {
        let mut marked_for_removal = None;

        for (tx_id, stp) in self.pending_outbound_transactions.iter_mut() {
            let recp_tx_id = recipient_reply.tx_id.clone();
            if stp.check_tx_id(recp_tx_id) && stp.is_collecting_single_signature() {
                stp.add_single_recipient_info(recipient_reply, &PROVER)?;
                stp.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY)?;
                let tx = stp.get_transaction()?;
                self.completed_transactions.insert(recp_tx_id, tx.clone());
                // TODO Broadcast this to the chain
                // TODO Only confirm this transaction once it is detected on chain. For now just confirming it directly.
                self.output_manager_service.confirm_sent_transaction(
                    recp_tx_id,
                    tx.body.inputs.clone(),
                    tx.body.outputs.clone(),
                )?;

                marked_for_removal = Some(tx_id.clone());
                break;
            }
        }

        if marked_for_removal.is_none() {
            return Err(TransactionServiceError::TransactionDoesNotExistError);
        }

        if let Some(tx_id) = marked_for_removal {
            self.pending_outbound_transactions.remove(&tx_id);
            info!(
                target: LOG_TARGET,
                "Transaction Recipient Reply for TX_ID = {} received", tx_id,
            );
        }

        Ok(())
    }

    /// Accept a new transaction from a sender by handling a public SenderMessage. The reply is generated and sent.
    /// # Arguments
    /// 'source_pubkey' - The pubkey from which the message was sent and to which the reply will be sent.
    /// 'sender_message' - Message from a sender containing the setup of the transaction being sent to you
    pub fn accept_transaction(
        &mut self,
        source_pubkey: CommsPublicKey,
        sender_message: TransactionSenderMessage,
    ) -> Result<(), TransactionServiceError>
    {
        let outbound_message_service = self
            .outbound_message_service
            .clone()
            .ok_or(TransactionServiceError::OutboundMessageServiceNotInitialized)?;

        // Currently we will only reply to a Single sender transaction protocol
        if let TransactionSenderMessage::Single(data) = sender_message.clone() {
            let spending_key = self
                .output_manager_service
                .get_recipient_spending_key(data.tx_id, data.amount)?;
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
            if self.pending_outbound_transactions.contains_key(&recipient_reply.tx_id) ||
                self.pending_inbound_transactions.contains_key(&recipient_reply.tx_id) ||
                self.completed_transactions.contains_key(&recipient_reply.tx_id)
            {
                return Err(TransactionServiceError::RepeatedMessageError);
            }

            outbound_message_service.send_message(
                BroadcastStrategy::DirectPublicKey(source_pubkey.clone()),
                MessageFlags::ENCRYPTED,
                TariMessageType::new(BlockchainMessage::TransactionReply),
                recipient_reply.clone(),
            )?;

            // Otherwise add it to our pending transaction list and return reply
            self.pending_inbound_transactions
                .insert(recipient_reply.tx_id.clone(), rtp);

            info!(
                target: LOG_TARGET,
                "Transaction with TX_ID = {} received from {}. Reply Sent",
                recipient_reply.tx_id.clone(),
                source_pubkey.clone()
            );
        }
        Ok(())
    }

    /// This handler is called when the Service executor loops receives an API request
    fn handle_api_message(&mut self, msg: TransactionServiceApiRequest) -> Result<(), ServiceError> {
        trace!(target: LOG_TARGET, "[{}] Received API message", self.get_name(),);
        let resp = match msg {
            TransactionServiceApiRequest::SendTransaction((dest_pubkey, amount, fee_per_gram)) => self
                .send_transaction(dest_pubkey, amount, fee_per_gram)
                .map(|_| TransactionServiceApiResponse::TransactionSent),
            TransactionServiceApiRequest::GetPendingInboundTransactions => Ok(
                TransactionServiceApiResponse::PendingInboundTransactions(self.pending_inbound_transactions.clone()),
            ),
            TransactionServiceApiRequest::GetPendingOutboundTransactions => Ok(
                TransactionServiceApiResponse::PendingOutboundTransactions(self.pending_outbound_transactions.clone()),
            ),
            TransactionServiceApiRequest::GetCompletedTransactions => Ok(
                TransactionServiceApiResponse::CompletedTransactions(self.completed_transactions.clone()),
            ),
        };

        trace!(target: LOG_TARGET, "[{}] Replying to API", self.get_name());
        self.api
            .send_reply(resp)
            .map_err(ServiceError::internal_service_error())
    }
}

/// The Domain Service trait implementation for the TestMessageService
impl Service for TransactionService {
    fn get_name(&self) -> String {
        "Transaction service".to_string()
    }

    fn get_message_types(&self) -> Vec<TariMessageType> {
        vec![
            BlockchainMessage::Transaction.into(),
            BlockchainMessage::TransactionReply.into(),
        ]
    }

    /// Function called by the Service Executor in its own thread. This function polls for both API request and Comms
    /// layer messages from the Message Broker
    fn execute(&mut self, context: ServiceContext) -> Result<(), ServiceError> {
        let mut subscription_transaction = SyncDomainSubscription::new(
            context
                .inbound_message_subscription_factory()
                .get_subscription_fused(BlockchainMessage::Transaction.into()),
        );

        let mut subscription_transaction_reply = SyncDomainSubscription::new(
            context
                .inbound_message_subscription_factory()
                .get_subscription_fused(BlockchainMessage::TransactionReply.into()),
        );

        self.outbound_message_service = Some(context.outbound_message_service());
        debug!(target: LOG_TARGET, "Starting Transaction Service executor");
        loop {
            if let Some(msg) = context.get_control_message(Duration::from_millis(5)) {
                match msg {
                    ServiceControlMessage::Shutdown => break,
                }
            }

            for m in subscription_transaction.receive_messages()?.drain(..) {
                if let Err(e) = self.accept_transaction(m.0.origin_source.clone(), m.1) {
                    error!(target: LOG_TARGET, "Transaction service had error: {:?}", e);
                }
            }

            for m in subscription_transaction_reply.receive_messages()?.drain(..) {
                if let Err(e) = self.accept_recipient_reply(m.1) {
                    error!(target: LOG_TARGET, "Transaction service had error: {:?}", e);
                }
            }

            if let Some(msg) = self
                .api
                .recv_timeout(Duration::from_millis(50))
                .map_err(ServiceError::internal_service_error())?
            {
                self.handle_api_message(msg)?;
            }
        }

        Ok(())
    }
}

/// API Request enum
#[derive(Debug)]
pub enum TransactionServiceApiRequest {
    GetPendingInboundTransactions,
    GetPendingOutboundTransactions,
    GetCompletedTransactions,
    SendTransaction((CommsPublicKey, MicroTari, MicroTari)),
}

/// API Response enum
#[derive(Debug)]
pub enum TransactionServiceApiResponse {
    TransactionSent,
    PendingInboundTransactions(HashMap<u64, ReceiverTransactionProtocol>),
    PendingOutboundTransactions(HashMap<u64, SenderTransactionProtocol>),
    CompletedTransactions(HashMap<u64, Transaction>),
}

/// Result for all API requests
pub type TransactionServiceApiResult = Result<TransactionServiceApiResponse, TransactionServiceError>;

/// The TextMessage service public API that other services and application will use to interact with this service.
/// The requests and responses are transmitted via channels into the Service Executor thread where this service is
/// running
pub struct TransactionServiceApi {
    sender: channel::Sender<TransactionServiceApiRequest>,
    receiver: channel::Receiver<TransactionServiceApiResult>,
    mutex: Mutex<()>,
    timeout: Duration,
}

impl TransactionServiceApi {
    fn new(
        sender: channel::Sender<TransactionServiceApiRequest>,
        receiver: channel::Receiver<TransactionServiceApiResult>,
    ) -> Self
    {
        Self {
            sender,
            receiver,
            mutex: Mutex::new(()),
            timeout: Duration::from_millis(DEFAULT_API_TIMEOUT_MS),
        }
    }

    pub fn send_transaction(
        &self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        fee_per_gram: MicroTari,
    ) -> Result<(), TransactionServiceError>
    {
        self.send_recv(TransactionServiceApiRequest::SendTransaction((
            dest_pubkey,
            amount,
            fee_per_gram,
        )))
        .and_then(|resp| match resp {
            TransactionServiceApiResponse::TransactionSent => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        })
    }

    pub fn get_pending_inbound_transaction(
        &self,
    ) -> Result<HashMap<u64, ReceiverTransactionProtocol>, TransactionServiceError> {
        self.send_recv(TransactionServiceApiRequest::GetPendingInboundTransactions)
            .and_then(|resp| match resp {
                TransactionServiceApiResponse::PendingInboundTransactions(p) => Ok(p),
                _ => Err(TransactionServiceError::UnexpectedApiResponse),
            })
    }

    pub fn get_pending_outbound_transaction(
        &self,
    ) -> Result<HashMap<u64, SenderTransactionProtocol>, TransactionServiceError> {
        self.send_recv(TransactionServiceApiRequest::GetPendingOutboundTransactions)
            .and_then(|resp| match resp {
                TransactionServiceApiResponse::PendingOutboundTransactions(p) => Ok(p),
                _ => Err(TransactionServiceError::UnexpectedApiResponse),
            })
    }

    pub fn get_completed_transaction(&self) -> Result<HashMap<u64, Transaction>, TransactionServiceError> {
        self.send_recv(TransactionServiceApiRequest::GetCompletedTransactions)
            .and_then(|resp| match resp {
                TransactionServiceApiResponse::CompletedTransactions(c) => Ok(c),
                _ => Err(TransactionServiceError::UnexpectedApiResponse),
            })
    }

    fn send_recv(&self, msg: TransactionServiceApiRequest) -> TransactionServiceApiResult {
        self.lock(|| -> TransactionServiceApiResult {
            self.sender
                .send(msg)
                .map_err(|_| TransactionServiceError::ApiSendFailed)?;
            self.receiver
                .recv_timeout(self.timeout.clone())
                .map_err(|_| TransactionServiceError::ApiReceiveFailed)?
        })
    }

    fn lock<F, T>(&self, func: F) -> T
    where F: FnOnce() -> T {
        let lock = acquire_lock!(self.mutex);
        let res = func();
        drop(lock);
        res
    }
}
