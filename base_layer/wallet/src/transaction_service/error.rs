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

use diesel::result::Error as DieselError;
use futures::channel::oneshot::Canceled;
use serde_json::Error as SerdeJsonError;
use tari_common_sqlite::error::SqliteStorageError;
use tari_common_types::{
    tari_address::TariAddressError,
    transaction::{TransactionConversionError, TransactionDirectionError, TxId},
    types::FixedHashSizeError,
};
use tari_comms::{connectivity::ConnectivityError, peer_manager::node_id::NodeIdError, protocol::rpc::RpcError};
use tari_comms_dht::outbound::DhtOutboundError;
use tari_core::transactions::{
    transaction_components::{EncryptedDataError, TransactionError},
    transaction_protocol::TransactionProtocolError,
};
use tari_crypto::{errors::RangeProofError, signatures::CommitmentSignatureError};
use tari_key_manager::key_manager_service::KeyManagerServiceError;
use tari_p2p::services::liveness::error::LivenessError;
use tari_script::ScriptError;
use tari_service_framework::reply_channel::TransportChannelError;
use tari_utilities::ByteArrayError;
use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;

use crate::{
    error::WalletStorageError,
    output_manager_service::error::OutputManagerError,
    transaction_service::{
        storage::{database::DbKey, sqlite_db::CompletedTransactionConversionError},
        utc::NegativeDurationError,
    },
};

#[derive(Debug, Error)]
pub enum TransactionServiceError {
    #[error("Transaction protocol is not in the correct state for this operation")]
    InvalidStateError,
    #[error("Transaction is sending to a network different than ours")]
    InvalidNetwork,
    #[error("One-sided transaction error: `{0}`")]
    OneSidedTransactionError(String),
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
    #[error("Base node changed during {task_name}")]
    BaseNodeChanged { task_name: &'static str },
    #[error("Error sending data to Protocol via registered channels")]
    ProtocolChannelError,
    #[error("Transaction detected as rejected by mempool")]
    MempoolRejection,
    #[error("Mempool response key does not match on that is expected")]
    UnexpectedMempoolResponse,
    #[error("Base Node response key does not match on that is expected")]
    UnexpectedBaseNodeResponse,
    #[error("The current transaction has been cancelled")]
    TransactionCancelled,
    #[error("DHT outbound error: `{0}`")]
    DhtOutboundError(#[from] DhtOutboundError),
    #[error("Output manager error: `{0}`")]
    OutputManagerError(#[from] OutputManagerError),
    #[error("Transport channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),
    #[error("Transaction storage error: `{0}`")]
    TransactionStorageError(#[from] TransactionStorageError),
    #[error("Wallet storage error: `{0}`")]
    WalletStorageError(#[from] WalletStorageError),
    #[error("Invalid message error: `{0}`")]
    InvalidMessageError(String),
    #[error("Transaction error: `{0}`")]
    TransactionError(#[from] TransactionError),
    #[error("Conversion error: `{0}`")]
    ConversionError(#[from] TransactionConversionError),
    #[error("duration::NegativeDurationError: {0}")]
    DurationOutOfRange(#[from] NegativeDurationError),
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
    #[error("Pending Transaction Timed out")]
    Timeout,
    #[error("Shutdown Signal Received")]
    Shutdown,
    #[error("Transaction detected as rejected by mempool due to containing time-locked input")]
    MempoolRejectionTimeLocked,
    #[error("Transaction detected as rejected by mempool due to containing  orphan input")]
    MempoolRejectionOrphan,
    #[error("Transaction detected as rejected by mempool due to containing double spend")]
    MempoolRejectionDoubleSpend,
    #[error("Transaction detected as rejected by mempool due to invalid transaction")]
    MempoolRejectionInvalidTransaction,
    #[error("Transaction is malformed")]
    InvalidTransaction,
    #[error("RpcError: `{0}`")]
    RpcError(#[from] RpcError),
    #[error("Protobuf Conversion Error: `{0}`")]
    ProtobufConversionError(String),
    #[error("Maximum Attempts Exceeded")]
    MaximumAttemptsExceeded,
    #[error("Byte array error")]
    ByteArrayError(String),
    #[error("Transaction Service Error: `{0}`")]
    ServiceError(String),
    #[error("Wallet Recovery in progress so Transaction Service Messaging Requests ignored")]
    WalletRecoveryInProgress,
    #[error("Wallet Transaction Validation already in progress, request ignored")]
    TransactionValidationInProgress,
    #[error("Connectivity error: {source}")]
    ConnectivityError {
        #[from]
        source: ConnectivityError,
    },
    #[error("Base Node is not synced")]
    BaseNodeNotSynced,
    #[error("Value encryption error: `{0}`")]
    EncryptionError(#[from] EncryptedDataError),
    #[error("FixedHash size error: `{0}`")]
    FixedHashSizeError(#[from] FixedHashSizeError),
    #[error("Commitment signature error: {0}")]
    CommitmentSignatureError(String),
    #[error("Invalid data: `{0}`")]
    RangeProofError(String),
    #[error("Key manager error: `{0}`")]
    InvalidKeyId(String),
    #[error("Invalid key manager data: `{0}`")]
    KeyManagerServiceError(#[from] KeyManagerServiceError),
    #[error("Serialization error: `{0}`")]
    SerializationError(String),
    #[error("Transaction exceed maximum byte size. Expected < {expected} but got {got}.")]
    TransactionTooLarge { got: usize, expected: usize },
    #[error("Pending Transaction was oversized")]
    Oversized,
    #[error("Transaction has invalid address: `{0}`")]
    InvalidAddress(String),
    #[error("Transaction is not supported: `{0}`")]
    NotSupported(String),
    #[error("Tari script error: {0}")]
    ScriptError(#[from] ScriptError),
}

impl From<RangeProofError> for TransactionServiceError {
    fn from(e: RangeProofError) -> Self {
        TransactionServiceError::RangeProofError(e.to_string())
    }
}

impl From<CommitmentSignatureError> for TransactionServiceError {
    fn from(e: CommitmentSignatureError) -> Self {
        TransactionServiceError::CommitmentSignatureError(e.to_string())
    }
}

impl From<ByteArrayError> for TransactionServiceError {
    fn from(err: ByteArrayError) -> Self {
        TransactionServiceError::ByteArrayError(err.to_string())
    }
}

#[derive(Debug, Error)]
pub enum TransactionKeyError {
    #[error("Invalid source address")]
    Source(TariAddressError),
    #[error("Invalid destination address")]
    Destination(TariAddressError),
    #[error("Invalid transaction signature nonce")]
    SignatureNonce(ByteArrayError),
    #[error("Invalid transaction signature key")]
    SignatureKey(ByteArrayError),
}

#[derive(Debug, Error)]
pub enum TransactionStorageError {
    #[error("Tried to insert an output that already exists in the database")]
    DuplicateOutput,
    #[error("Value not found: `{0}`")]
    ValueNotFound(DbKey),
    #[error("Unexpected result: `{0}`")]
    UnexpectedResult(String),
    #[error("Bincode error: `{0}`")]
    BincodeSerialize(String),
    #[error("Bincode error: `{0}`")]
    BincodeDeserialize(String),
    #[error("This write operation is not supported for provided DbKey")]
    OperationNotSupported,
    #[error("Could not find all values specified for batch operation")]
    ValuesNotFound,
    #[error("Transaction is already present in the database")]
    TransactionAlreadyExists,
    #[error("Out of range error: `{0}`")]
    TransactionKeyError(#[from] TransactionKeyError),
    #[error("Transaction direction error: `{0}`")]
    TransactionDirectionError(#[from] TransactionDirectionError),
    #[error("Error converting a type: `{0}`")]
    NegativeDurationError(#[from] NegativeDurationError),
    #[error("Error converting a type: `{0}`")]
    ConversionError(#[from] TransactionConversionError),
    #[error("Completed transaction conversion error: `{0}`")]
    CompletedConversionError(#[from] CompletedTransactionConversionError),
    #[error("Serde json error: `{0}`")]
    SerdeJsonError(#[from] SerdeJsonError),
    #[error("Diesel R2d2 error: `{0}`")]
    DieselR2d2Error(#[from] WalletStorageError),
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
    #[error("Transaction (TxId: '{0}') is not mined")]
    TransactionNotMined(TxId),
    #[error("Conversion error: `{0}`")]
    ByteArrayError(String),
    #[error("Tari address error: `{0}`")]
    TariAddressError(#[from] TariAddressError),
    #[error("Db error: `{0}`")]
    SqliteStorageError(#[from] SqliteStorageError),
    #[error("Coinbase transactions are not supported in the wallet")]
    CoinbaseNotSupported,
}

impl From<ByteArrayError> for TransactionStorageError {
    fn from(e: ByteArrayError) -> Self {
        TransactionStorageError::ByteArrayError(e.to_string())
    }
}

/// This error type is used to return TransactionServiceErrors from inside a Transaction Service protocol but also
/// include the ID of the protocol
#[derive(Debug)]
pub struct TransactionServiceProtocolError<T: Into<u64>> {
    pub id: T,
    pub error: TransactionServiceError,
}

impl<T: Into<u64>> TransactionServiceProtocolError<T> {
    pub fn new(id: T, error: TransactionServiceError) -> Self {
        Self { id, error }
    }
}

impl<T: Into<u64>> From<TransactionServiceProtocolError<T>> for TransactionServiceError {
    fn from(tspe: TransactionServiceProtocolError<T>) -> Self {
        tspe.error
    }
}

pub trait TransactionServiceProtocolErrorExt<TRes, T: Into<u64>> {
    fn for_protocol(self, id: T) -> Result<TRes, TransactionServiceProtocolError<T>>;
}

impl<TRes, TErr: Into<TransactionServiceError>, T: Into<u64>> TransactionServiceProtocolErrorExt<TRes, T>
    for Result<TRes, TErr>
{
    fn for_protocol(self, id: T) -> Result<TRes, TransactionServiceProtocolError<T>> {
        match self {
            Ok(r) => Ok(r),
            Err(e) => Err(TransactionServiceProtocolError::new(id, e.into())),
        }
    }
}
