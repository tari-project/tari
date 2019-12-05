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
use tari_key_manager::{key_manager::KeyManagerError, mnemonic::MnemonicError};
use tari_service_framework::reply_channel::TransportChannelError;
use tari_transactions::transaction_protocol::TransactionProtocolError;
use tari_utilities::ByteArrayError;
use time::OutOfRangeError;

#[derive(Debug, Error, PartialEq)]
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
    OutOfRangeError(OutOfRangeError),
    R2d2Error,
    DieselError(DieselError),
    DieselConnectionError(diesel::ConnectionError),
    #[error(msg_embedded, no_from, non_std)]
    DatabaseMigrationError(String),
}
