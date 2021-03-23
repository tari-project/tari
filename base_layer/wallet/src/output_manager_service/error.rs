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

use crate::{base_node_service::error::BaseNodeServiceError, output_manager_service::storage::database::DbKey};
use diesel::result::Error as DieselError;
use tari_comms::{peer_manager::node_id::NodeIdError, protocol::rpc::RpcError};
use tari_comms_dht::outbound::DhtOutboundError;
use tari_core::transactions::{
    transaction::TransactionError,
    transaction_protocol::TransactionProtocolError,
    CoinbaseBuildError,
};
use tari_crypto::{script::ScriptError, tari_utilities::ByteArrayError};
use tari_key_manager::{key_manager::KeyManagerError, mnemonic::MnemonicError};
use tari_service_framework::reply_channel::TransportChannelError;
use thiserror::Error;
use time::OutOfRangeError;

#[derive(Debug, Error)]
pub enum OutputManagerError {
    #[error("Build error: `{0}`")]
    BuildError(String),
    #[error("Byte array error: `{0}`")]
    ByteArrayError(#[from] ByteArrayError),
    #[error("Transaction protocol error: `{0}`")]
    TransactionProtocolError(#[from] TransactionProtocolError),
    #[error("Transport channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),
    #[error("Out of range error: `{0}`")]
    OutOfRangeError(#[from] OutOfRangeError),
    #[error("Output manager storage error: `{0}`")]
    OutputManagerStorageError(#[from] OutputManagerStorageError),
    #[error("Mnemonic error: `{0}`")]
    MnemonicError(#[from] MnemonicError),
    #[error("Key manager error: `{0}`")]
    KeyManagerError(#[from] KeyManagerError),
    #[error("Transaction error: `{0}`")]
    TransactionError(#[from] TransactionError),
    #[error("DHT outbound error: `{0}`")]
    DhtOutboundError(#[from] DhtOutboundError),
    #[error("Conversion error: `{0}`")]
    ConversionError(String),
    #[error("Not all the transaction inputs and outputs are present to be confirmed: {0}")]
    IncompleteTransaction(&'static str),
    #[error("Not enough funds to fulfil transaction")]
    NotEnoughFunds,
    #[error("Output already exists")]
    DuplicateOutput,
    #[error("Error sending a message to the public API")]
    ApiSendFailed,
    #[error("Error receiving a message from the public API")]
    ApiReceiveFailed,
    #[error("API returned something unexpected.")]
    UnexpectedApiResponse,
    #[error("Invalid config provided to Output Manager")]
    InvalidConfig,
    #[error("The response received from another service is an incorrect variant: `{0}`")]
    InvalidResponseError(String),
    #[error("No Base Node public key has been provided for this service to use for contacting a base node")]
    NoBaseNodeKeysProvided,
    #[error("An error occured sending an event out on the event stream")]
    EventStreamError,
    #[error("Maximum Attempts Exceeded")]
    MaximumAttemptsExceeded,
    #[error("An error has been experienced in the service: `{0}`")]
    ServiceError(String),
    #[error("Base node is not synced")]
    BaseNodeNotSynced,
    #[error("Invalid Sender Message Type")]
    InvalidSenderMessage,
    #[error("Coinbase build error: `{0}`")]
    CoinbaseBuildError(#[from] CoinbaseBuildError),
    #[error("TXO Validation protocol cancelled")]
    Cancellation,
    #[error("Base NodeService Error: `{0}`")]
    BaseNodeServiceError(#[from] BaseNodeServiceError),
    #[error("Shutdown Signal Received")]
    Shutdown,
    #[error("RpcError: `{0}`")]
    RpcError(#[from] RpcError),
    #[error("Node ID error: `{0}`")]
    NodeIdError(#[from] NodeIdError),
    #[error("Script hash does not match expected script")]
    InvalidScriptHash,
    #[error("Tari script error : {0}")]
    ScriptError(#[from] ScriptError),
}

#[derive(Debug, Error, PartialEq)]
pub enum OutputManagerStorageError {
    #[error("Tried to insert an output that already exists in the database")]
    DuplicateOutput,
    #[error("Value not found: `{0}`")]
    ValueNotFound(DbKey),
    #[error("Unexpected result: `{0}`")]
    UnexpectedResult(String),
    #[error("If an pending transaction does not exist to be confirmed")]
    PendingTransactionNotFound,
    #[error("This write operation is not supported for provided DbKey")]
    OperationNotSupported,
    #[error("Could not find all values specified for batch operation")]
    ValuesNotFound,
    #[error("Error converting a type")]
    ConversionError,
    #[error("Output has already been spent")]
    OutputAlreadySpent,
    #[error("Key Manager not initialized")]
    KeyManagerNotInitialized,
    #[error("Out of range error: `{0}`")]
    OutOfRangeError(#[from] OutOfRangeError),
    #[error("R2d2 error")]
    R2d2Error,
    #[error("Transaction error: `{0}`")]
    TransactionError(#[from] TransactionError),
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
    #[error("Byte array error: `{0}`")]
    ByteArrayError(#[from] ByteArrayError),
    #[error("Aead error: `{0}`")]
    AeadError(String),
    #[error("Tari script error : {0}")]
    ScriptError(#[from] ScriptError),
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
