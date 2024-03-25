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
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_common_sqlite::error::SqliteStorageError;
use tari_comms::{connectivity::ConnectivityError, peer_manager::node_id::NodeIdError, protocol::rpc::RpcError};
use tari_comms_dht::outbound::DhtOutboundError;
use tari_core::transactions::{
    transaction_components::{EncryptedDataError, TransactionError},
    transaction_protocol::TransactionProtocolError,
};
use tari_crypto::errors::RangeProofError;
use tari_key_manager::{
    error::{KeyManagerError, MnemonicError},
    key_manager_service::KeyManagerServiceError,
};
use tari_script::ScriptError;
use tari_service_framework::reply_channel::TransportChannelError;
use tari_utilities::{hex::HexError, ByteArrayError};
use thiserror::Error;

use crate::{
    base_node_service::error::BaseNodeServiceError,
    error::WalletStorageError,
    output_manager_service::UtxoSelectionCriteria,
};

#[derive(Debug, Error)]
pub enum OutputManagerError {
    #[error("Build error: `{0}`")]
    BuildError(String),
    #[error("Byte array error: `{0}`")]
    ByteArrayError(String),
    #[error("Transaction protocol error: `{0}`")]
    TransactionProtocolError(#[from] TransactionProtocolError),
    #[error("Transport channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),
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
    #[error("Inconsistent data received from BaseNode: {0}")]
    InconsistentBaseNodeDataError(&'static str),
    #[error("Not enough funds to fulfil transaction")]
    NotEnoughFunds,
    #[error("Funds are still pending. Unable to fulfil transaction right now.")]
    FundsPending,
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
    #[error("Base node changed")]
    BaseNodeChanged,
    #[error("Invalid Sender Message Type")]
    InvalidSenderMessage,
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
    #[error("Unsupported Covenant")]
    InvalidCovenant,
    #[error("Unsupported Output Features")]
    InvalidOutputFeatures,
    #[error("Unsupported Kernel Features")]
    InvalidKernelFeatures,
    #[error("Unsupported Lock Height")]
    InvalidLockHeight,
    #[error("Tari script error: {0}")]
    ScriptError(#[from] ScriptError),
    #[error("Master secret key does not match persisted key manager state")]
    MasterSeedMismatch,
    #[error("Private Key is not found in the current Key Chain")]
    KeyNotFoundInKeyChain,
    #[error("No UTXOs selected as inputs for {criteria}")]
    NoUtxosSelected { criteria: UtxoSelectionCriteria },
    #[error("Connectivity error: {source}")]
    ConnectivityError {
        #[from]
        source: ConnectivityError,
    },
    #[error("Invalid message received: {0}")]
    InvalidMessageError(String),
    #[error("Key manager service error: {0}")]
    KeyManagerServiceError(#[from] KeyManagerServiceError),
    #[error("Value can't be encrypted/decrypted")]
    ValueEncryptionError(#[from] EncryptedDataError),
    #[error("No commitments were provided")]
    NoCommitmentsProvided,
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Validation in progress")]
    ValidationInProgress,
    #[error("Invalid data: `{0}`")]
    RangeProofError(String),
    #[error("Transaction is over sized: `{0}`")]
    TooManyInputsToFulfillTransaction(String),
}

impl From<RangeProofError> for OutputManagerError {
    fn from(e: RangeProofError) -> Self {
        OutputManagerError::RangeProofError(e.to_string())
    }
}

impl From<ByteArrayError> for OutputManagerError {
    fn from(e: ByteArrayError) -> Self {
        OutputManagerError::ByteArrayError(e.to_string())
    }
}

#[derive(Debug, Error)]
pub enum OutputManagerStorageError {
    #[error("Tried to insert an output that already exists in the database")]
    DuplicateOutput,
    #[error(
        "Tried to insert an pending transaction encumberance for a transaction ID that already exists in the database"
    )]
    DuplicateTransaction,
    #[error("Value not found")]
    ValueNotFound,
    #[error("Unexpected result: `{0}`")]
    UnexpectedResult(String),
    #[error("If an pending transaction does not exist to be confirmed")]
    PendingTransactionNotFound,
    #[error("This write operation is not supported for provided DbKey")]
    OperationNotSupported,
    #[error("Could not find all values specified for batch operation")]
    ValuesNotFound,
    #[error("Error converting a type: {reason}")]
    ConversionError { reason: String },
    #[error("Output has already been spent")]
    OutputAlreadySpent,
    #[error("Output is already encumbered")]
    OutputAlreadyEncumbered,
    #[error("Key Manager not initialized")]
    KeyManagerNotInitialized,
    #[error("Diesel R2d2 error: `{0}`")]
    DieselR2d2Error(#[from] WalletStorageError),
    #[error("Transaction error: `{0}`")]
    TransactionError(#[from] TransactionError),
    #[error("Diesel error: `{0}`")]
    DieselError(#[from] DieselError),
    #[error("Diesel connection error: `{0}`")]
    DieselConnectionError(#[from] diesel::ConnectionError),
    #[error("Database migration error: `{0}`")]
    DatabaseMigrationError(String),
    #[error("Wallet db is already encrypted and cannot be encrypted until the previous encryption is removed")]
    AlreadyEncrypted,
    #[error("Byte array error: `{0}`")]
    ByteArrayError(String),
    #[error("Aead error: `{0}`")]
    AeadError(String),
    #[error("Tried to insert a script that already exists in the database")]
    DuplicateScript,
    #[error("Tari script error: {0}")]
    ScriptError(#[from] ScriptError),
    #[error("Binary not stored as valid hex:{0}")]
    HexError(String),
    #[error("Key Manager Service Error: `{0}`")]
    KeyManagerServiceError(#[from] KeyManagerServiceError),
    #[error("IO Error: `{0}`")]
    IoError(#[from] std::io::Error),
    #[error("Error: `{0}`")]
    SqliteStorageError(#[from] SqliteStorageError),
    #[error("Encryption error: `{0}`")]
    EncryptedOpeningsError(#[from] EncryptedDataError),
}

impl From<HexError> for OutputManagerStorageError {
    fn from(err: HexError) -> Self {
        OutputManagerStorageError::HexError(err.to_string())
    }
}

impl From<ByteArrayError> for OutputManagerStorageError {
    fn from(e: ByteArrayError) -> Self {
        OutputManagerStorageError::ByteArrayError(e.to_string())
    }
}

impl From<OutputManagerError> for ExitError {
    fn from(err: OutputManagerError) -> Self {
        log::error!(target: crate::error::LOG_TARGET, "{}", err);
        Self::new(ExitCode::WalletError, err.to_string())
    }
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

pub trait OutputManagerProtocolErrorExt<TRes> {
    fn for_protocol(self, id: u64) -> Result<TRes, OutputManagerProtocolError>;
}

impl<TRes, TErr: Into<OutputManagerError>> OutputManagerProtocolErrorExt<TRes> for Result<TRes, TErr> {
    fn for_protocol(self, id: u64) -> Result<TRes, OutputManagerProtocolError> {
        match self {
            Ok(r) => Ok(r),
            Err(e) => Err(OutputManagerProtocolError::new(id, e.into())),
        }
    }
}
