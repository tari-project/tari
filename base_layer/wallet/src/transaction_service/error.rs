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
    output_manager_service::{error::OutputManagerError, TxId},
    transaction_service::storage::database::DbKey,
};
use diesel::result::Error as DieselError;
use futures::channel::oneshot::Canceled;
use serde_json::Error as SerdeJsonError;
use tari_comms::peer_manager::node_id::NodeIdError;
use tari_comms_dht::outbound::DhtOutboundError;
use tari_core::transactions::{
    transaction::TransactionError,
    transaction_protocol::TransactionProtocolError,
    CoinbaseBuildError,
};
use tari_p2p::services::liveness::error::LivenessError;
use tari_service_framework::reply_channel::TransportChannelError;
use thiserror::Error;
use time::OutOfRangeError;
use tokio::sync::broadcast::RecvError;

#[derive(Debug, Error)]
pub enum TransactionServiceError {
    #[error("Transaction protocol is not in the correct state for this operation")]
    InvalidStateError,
    #[error("Transaction Protocol Error: `{0}`")]
    TransactionProtocolError(#[from] TransactionProtocolError),
    #[error("The message being processed is not recognized by the Transaction Manager")]
    InvalidMessageTypeError,
    #[error("A message for a specific tx_id has been repeated")]
    RepeatedMessageError,
    #[error("A recipient reply was received for a non-existent tx_id")]
    TransactionDoesNotExistError,
    #[error("The Outbound Message Service is not initialized")]
    OutboundMessageServiceNotInitialized,
    #[error("Received an unexpected API response")]
    UnexpectedApiResponse,
    #[error("Failed to send from API")]
    ApiSendFailed,
    #[error("Failed to receive in API from service")]
    ApiReceiveFailed,
    #[error("An error has occurred reading or writing the event subscriber stream")]
    EventStreamError,
    #[error(
        "The Source Public Key on the received transaction does not match the transaction with the same TX_ID in the \
         database"
    )]
    InvalidSourcePublicKey,
    #[error("The transaction does not contain the receivers output")]
    ReceiverOutputNotFound,
    #[error("Outbound Service send failed")]
    OutboundSendFailure,
    #[error(
        "Outbound Service Discovery process needed to be conducted before message could be sent. The result of the \
         process will be communicated via the callback at some time in the future (could be minutes): TxId `{0}`"
    )]
    OutboundSendDiscoveryInProgress(TxId),
    #[error("Discovery process failed to return a result: TxId `{0}`")]
    DiscoveryProcessFailed(TxId),
    #[error("Invalid Completed Transaction provided")]
    InvalidCompletedTransaction,
    #[error("No Base Node public keys are provided for Base chain broadcast and monitoring")]
    NoBaseNodeKeysProvided,
    #[error("Error sending data to Protocol via register channels")]
    ProtocolChannelError,
    #[error("Transaction detected as rejected by mempool")]
    MempoolRejection,
    #[error("Mempool response key does not match on that is expected")]
    UnexpectedMempoolResponse,
    #[error("Base Node response key does not match on that is expected")]
    UnexpectedBaseNodeResponse,
    #[error("The current transaction has been cancelled")]
    TransactionCancelled,
    #[error("Chain tip has moved beyond this coinbase before it was mined so it must be cancelled")]
    ChainTipHigherThanCoinbaseHeight,
    #[error("DHT outbound error: `{0}`")]
    DhtOutboundError(#[from] DhtOutboundError),
    #[error("Output manager error: `{0}`")]
    OutputManagerError(#[from] OutputManagerError),
    #[error("Transport channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),
    #[error("Transaction storage error: `{0}`")]
    TransactionStorageError(#[from] TransactionStorageError),
    #[error("Invalid message error: `{0}`")]
    InvalidMessageError(String),
    #[cfg(feature = "test_harness")]
    #[error("Test harness error: `{0}`")]
    TestHarnessError(String),
    #[error("Transaction error: `{0}`")]
    TransactionError(#[from] TransactionError),
    #[error("Conversion error: `{0}`")]
    ConversionError(String),
    #[error("Node ID error: `{0}`")]
    NodeIdError(#[from] NodeIdError),
    #[error("Broadcast recv error: `{0}`")]
    BroadcastRecvError(#[from] RecvError),
    #[error("Broadcast send error: `{0}`")]
    BroadcastSendError(String),
    #[error("Oneshot cancelled error: `{0}`")]
    OneshotCancelled(#[from] Canceled),
    #[error("Liveness error: `{0}`")]
    LivenessError(#[from] LivenessError),
    #[error("Coinbase build error: `{0}`")]
    CoinbaseBuildError(#[from] CoinbaseBuildError),
    #[error("Pending Transaction Timed out")]
    Timeout,
    #[error("Shutdown Signal Received")]
    Shutdown,
}

#[derive(Debug, Error)]
pub enum TransactionStorageError {
    #[error("Tried to insert an output that already exists in the database")]
    DuplicateOutput,
    #[error("Value not found: `{0}`")]
    ValueNotFound(DbKey),
    #[error("Unexpected result: `{0}`")]
    UnexpectedResult(String),
    #[error("This write operation is not supported for provided DbKey")]
    OperationNotSupported,
    #[error("Could not find all values specified for batch operation")]
    ValuesNotFound,
    #[error("Transaction is already present in the database")]
    TransactionAlreadyExists,
    #[error("Out of range error: `{0}`")]
    OutOfRangeError(#[from] OutOfRangeError),
    #[error("Error converting a type: `{0}`")]
    ConversionError(String),
    #[error("Serde json error: `{0}`")]
    SerdeJsonError(#[from] SerdeJsonError),
    #[error("R2d2 error")]
    R2d2Error,
    #[error("Diesel error: `{0}`")]
    DieselError(#[from] DieselError),
    #[error("Diesel connection error: `{0}`")]
    DieselConnectionError(#[from] diesel::ConnectionError),
    #[error("Database migration error: `{0}`")]
    DatabaseMigrationError(String),
    #[error("Blocking task spawn error: `{0}`")]
    BlockingTaskSpawnError(String),
    #[error("Wallet db is already encrypted and cannot be encrypted until the previous encryption is removed")]
    AlreadyEncrypted,
    #[error("Aead error: `{0}`")]
    AeadError(String),
}

/// This error type is used to return TransactionServiceErrors from inside a Transaction Service protocol but also
/// include the ID of the protocol
#[derive(Debug)]
pub struct TransactionServiceProtocolError {
    pub id: u64,
    pub error: TransactionServiceError,
}

impl TransactionServiceProtocolError {
    pub fn new(id: u64, error: TransactionServiceError) -> Self {
        Self { id, error }
    }
}

impl From<TransactionServiceProtocolError> for TransactionServiceError {
    fn from(tspe: TransactionServiceProtocolError) -> Self {
        tspe.error
    }
}
