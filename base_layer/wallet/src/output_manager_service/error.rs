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

use crate::output_manager_service::storage::database::DbKey;
use derive_error::Error;
use diesel::result::Error as DieselError;
use tari_comms_dht::outbound::DhtOutboundError;
use tari_core::transactions::{transaction::TransactionError, transaction_protocol::TransactionProtocolError};
use tari_crypto::tari_utilities::ByteArrayError;
use tari_key_manager::{key_manager::KeyManagerError, mnemonic::MnemonicError};
use tari_service_framework::reply_channel::TransportChannelError;
use time::OutOfRangeError;

#[derive(Debug, Error)]
pub enum OutputManagerError {
    #[error(msg_embedded, no_from, non_std)]
    BuildError(String),
    ByteArrayError(ByteArrayError),
    TransactionProtocolError(TransactionProtocolError),
    TransportChannelError(TransportChannelError),
    OutOfRangeError(OutOfRangeError),
    OutputManagerStorageError(OutputManagerStorageError),
    MnemonicError(MnemonicError),
    KeyManagerError(KeyManagerError),
    TransactionError(TransactionError),
    DhtOutboundError(DhtOutboundError),
    #[error(msg_embedded, no_from, non_std)]
    ConversionError(String),
    /// Not all the transaction inputs and outputs are present to be confirmed
    IncompleteTransaction,
    /// Not enough funds to fulfil transaction
    NotEnoughFunds,
    /// Output already exists
    DuplicateOutput,
    /// Error sending a message to the public API
    ApiSendFailed,
    /// Error receiving a message from the public API
    ApiReceiveFailed,
    /// API returned something unexpected.
    UnexpectedApiResponse,
    /// Invalid config provided to Output Manager
    InvalidConfig,
    /// The response received from another service is an incorrect variant
    #[error(msg_embedded, no_from, non_std)]
    InvalidResponseError(String),
    /// No Base Node public key has been provided for this service to use for contacting a base node
    NoBaseNodeKeysProvided,
    /// An error occured sending an event out on the event stream
    EventStreamError,
    /// Maximum Attempts Exceeded
    MaximumAttemptsExceeded,
    /// An error has been experienced in the service
    #[error(msg_embedded, non_std, no_from)]
    ServiceError(String),
}

#[derive(Debug, Error, PartialEq)]
pub enum OutputManagerStorageError {
    /// Tried to insert an output that already exists in the database
    DuplicateOutput,
    #[error(non_std, no_from)]
    ValueNotFound(DbKey),
    #[error(msg_embedded, non_std, no_from)]
    UnexpectedResult(String),
    /// If an pending transaction does not exist to be confirmed
    PendingTransactionNotFound,
    /// This write operation is not supported for provided DbKey
    OperationNotSupported,
    /// Could not find all values specified for batch operation
    ValuesNotFound,
    /// Error converting a type
    ConversionError,
    /// Output has already been spent
    OutputAlreadySpent,
    /// Key Manager not initialized
    KeyManagerNotInitialized,

    OutOfRangeError(OutOfRangeError),
    R2d2Error,
    TransactionError(TransactionError),
    DieselError(DieselError),
    DieselConnectionError(diesel::ConnectionError),
    #[error(msg_embedded, no_from, non_std)]
    DatabaseMigrationError(String),
    #[error(msg_embedded, non_std, no_from)]
    BlockingTaskSpawnError(String),
}

/// This error type is used to return OutputManagerError from inside a Output Manager Service protocol but also
/// include the ID of the protocol
#[derive(Debug)]
pub struct OutputManagerProtocolError {
    pub id: u64,
    pub error: OutputManagerError,
}

impl OutputManagerProtocolError {
    pub fn new(id: u64, error: OutputManagerError) -> Self {
        Self { id, error }
    }
}

impl From<OutputManagerProtocolError> for OutputManagerError {
    fn from(tspe: OutputManagerProtocolError) -> Self {
        tspe.error
    }
}
