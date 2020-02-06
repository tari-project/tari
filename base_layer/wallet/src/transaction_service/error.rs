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
use derive_error::Error;
use diesel::result::Error as DieselError;
use serde_json::Error as SerdeJsonError;
use tari_comms_dht::outbound::DhtOutboundError;
use tari_core::transactions::{transaction::TransactionError, transaction_protocol::TransactionProtocolError};
use tari_service_framework::reply_channel::TransportChannelError;
use time::OutOfRangeError;

#[derive(Debug, Error)]
pub enum TransactionServiceError {
    /// Transaction protocol is not in the correct state for this operation
    InvalidStateError,
    /// Transaction Protocol Error
    TransactionProtocolError(TransactionProtocolError),
    /// The message being processed is not recognized by the Transaction Manager
    InvalidMessageTypeError,
    /// A message for a specific tx_id has been repeated
    RepeatedMessageError,
    /// A recipient reply was received for a non-existent tx_id
    TransactionDoesNotExistError,
    /// The Outbound Message Service is not initialized
    OutboundMessageServiceNotInitialized,
    /// Received an unexpected API response
    UnexpectedApiResponse,
    /// Failed to send from API
    ApiSendFailed,
    /// Failed to receive in API from service
    ApiReceiveFailed,
    /// An error has occurred reading or writing the event subscriber stream
    EventStreamError,
    /// The Source Public Key on the received transaction does not match the transaction with the same TX_ID in the
    /// database
    InvalidSourcePublicKey,
    /// The transaction does not contain the receivers output
    ReceiverOutputNotFound,
    /// Outbound Service send failed
    OutboundSendFailure,
    /// Outbound Service Discovery process needed to be conducted before message could be sent. The result of the
    /// process will be communicated via the callback at some time in the future (could be minutes)
    #[error(no_from, non_std)]
    OutboundSendDiscoveryInProgress(TxId),
    /// Discovery process failed to return a result
    #[error(no_from, non_std)]
    DiscoveryProcessFailed(TxId),
    /// Invalid Completed Transaction provided
    InvalidCompletedTransaction,
    /// No Base Node public keys are provided for Base chain broadcast and monitoring
    NoBaseNodeKeysProvided,
    DhtOutboundError(DhtOutboundError),
    OutputManagerError(OutputManagerError),
    TransportChannelError(TransportChannelError),
    TransactionStorageError(TransactionStorageError),
    #[error(msg_embedded, no_from, non_std)]
    InvalidMessageError(String),
    #[cfg(feature = "test_harness")]
    #[error(msg_embedded, no_from, non_std)]
    TestHarnessError(String),
    TransactionError(TransactionError),
    #[error(msg_embedded, no_from, non_std)]
    ConversionError(String),
}

#[derive(Debug, Error)]
pub enum TransactionStorageError {
    /// Tried to insert an output that already exists in the database
    DuplicateOutput,
    #[error(non_std, no_from)]
    ValueNotFound(DbKey),
    #[error(msg_embedded, non_std, no_from)]
    UnexpectedResult(String),
    /// This write operation is not supported for provided DbKey
    OperationNotSupported,
    /// Could not find all values specified for batch operation
    ValuesNotFound,
    /// Transaction is already present in the database
    TransactionAlreadyExists,
    OutOfRangeError(OutOfRangeError),
    /// Error converting a type
    ConversionError,
    SerdeJsonError(SerdeJsonError),
    R2d2Error,
    DieselError(DieselError),
    DieselConnectionError(diesel::ConnectionError),
    #[error(msg_embedded, no_from, non_std)]
    DatabaseMigrationError(String),
    #[error(msg_embedded, non_std, no_from)]
    BlockingTaskSpawnError(String),
}
