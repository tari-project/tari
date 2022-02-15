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
use log::SetLoggerError;
use serde_json::Error as SerdeJsonError;
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_common_sqlite::error::SqliteStorageError;
use tari_comms::{
    connectivity::ConnectivityError,
    multiaddr,
    peer_manager::{node_id::NodeIdError, PeerManagerError},
};
use tari_comms_dht::store_forward::StoreAndForwardError;
use tari_core::transactions::transaction_components::TransactionError;
use tari_crypto::tari_utilities::{hex::HexError, ByteArrayError};
use tari_key_manager::error::KeyManagerError;
use tari_p2p::{initialization::CommsInitializationError, services::liveness::error::LivenessError};
use tari_service_framework::{reply_channel::TransportChannelError, ServiceInitializationError};
use thiserror::Error;

use crate::{
    base_node_service::error::BaseNodeServiceError,
    contacts_service::error::ContactsServiceError,
    output_manager_service::error::OutputManagerError,
    storage::database::DbKey,
    transaction_service::error::TransactionServiceError,
    utxo_scanner_service::error::UtxoScannerError,
};

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("Argument supplied `{argument}` has an invalid value: {value}. {message}")]
    ArgumentError {
        argument: String,
        value: String,
        message: String,
    },
    #[error("Comms initialization error: `{0}`")]
    CommsInitializationError(#[from] CommsInitializationError),
    #[error("Output manager error: `{0}`")]
    OutputManagerError(#[from] OutputManagerError),
    #[error("Transaction service error: `{0}`")]
    TransactionServiceError(#[from] TransactionServiceError),
    #[error("Peer manager error: `{0}`")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Multiaddr error: `{0}`")]
    MultiaddrError(#[from] multiaddr::Error),
    #[error("Wallet storage error: `{0}`")]
    WalletStorageError(#[from] WalletStorageError),
    #[error("Set logger error: `{0}`")]
    SetLoggerError(#[from] SetLoggerError),
    #[error("Contacts service error: `{0}`")]
    ContactsServiceError(#[from] ContactsServiceError),
    #[error("Liveness service error: `{0}`")]
    LivenessServiceError(#[from] LivenessError),
    #[error("Store and forward error: `{0}`")]
    StoreAndForwardError(#[from] StoreAndForwardError),
    #[error("Connectivity error: `{0}`")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("Failed to initialize services: {0}")]
    ServiceInitializationError(#[from] ServiceInitializationError),
    #[error("Base Node Service error: {0}")]
    BaseNodeServiceError(#[from] BaseNodeServiceError),
    #[error("Node ID error: `{0}`")]
    NodeIdError(#[from] NodeIdError),
    #[error("Error performing wallet recovery: '{0}'")]
    WalletRecoveryError(String),
    #[error("Shutdown Signal Received")]
    Shutdown,
    #[error("Transaction Error: {0}")]
    TransactionError(#[from] TransactionError),
    #[error("Byte array error")]
    ByteArrayError(#[from] tari_crypto::tari_utilities::ByteArrayError),
    #[error("Utxo Scanner Error: {0}")]
    UtxoScannerError(#[from] UtxoScannerError),
    #[error("Key manager error: `{0}`")]
    KeyManagerError(#[from] KeyManagerError),

    #[error("Transport channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),

    #[error("Unexpected API Response while calling method `{method}` on `{api}`")]
    UnexpectedApiResponse { method: String, api: String },
}

pub const LOG_TARGET: &str = "tari::application";

impl From<WalletError> for ExitError {
    fn from(err: WalletError) -> Self {
        log::error!(target: LOG_TARGET, "{}", err);
        Self::new(ExitCode::WalletError, err)
    }
}

#[derive(Debug, Error)]
pub enum WalletStorageError {
    #[error("Tried to insert an output that already exists in the database")]
    DuplicateContact,
    #[error("This write operation is not supported for provided DbKey")]
    OperationNotSupported,
    #[error("Error converting a type: `{0}`")]
    ConversionError(String),
    #[error("Could not find all values specified for batch operation")]
    ValuesNotFound,
    #[error("Db Path does not exist")]
    DbPathDoesNotExist,
    #[error("Serde json error: `{0}`")]
    SerdeJsonError(#[from] SerdeJsonError),
    #[error("Diesel R2d2 error: `{0}`")]
    DieselR2d2Error(#[from] SqliteStorageError),
    #[error("Diesel error: `{0}`")]
    DieselError(#[from] DieselError),
    #[error("Diesel connection error: `{0}`")]
    DieselConnectionError(#[from] diesel::ConnectionError),
    #[error("Database migration error")]
    DatabaseMigrationError(String),
    #[error("Value not found: `{0}`")]
    ValueNotFound(DbKey),
    #[error("Unexpected result: `{0}`")]
    UnexpectedResult(String),
    #[error("Blocking task spawn error: `{0}`")]
    BlockingTaskSpawnError(String),
    #[error("File error: `{0}`")]
    FileError(String),
    #[error("The storage path was invalid unicode or not supported by the host OS")]
    InvalidUnicodePath,
    #[error("Hex error: `{0}`")]
    HexError(#[from] HexError),
    #[error("Invalid Encryption Cipher was provided to database")]
    InvalidEncryptionCipher,
    #[error("Invalid passphrase was provided")]
    InvalidPassphrase,
    #[error("Missing Nonce in encrypted data")]
    MissingNonce,
    #[error("Aead error: `{0}`")]
    AeadError(String),
    #[error("Wallet db is already encrypted and cannot be encrypted until the previous encryption is removed")]
    AlreadyEncrypted,
    #[error("Byte array error: `{0}`")]
    ByteArrayError(#[from] ByteArrayError),
    #[error("Cannot acquire exclusive file lock, another instance of the application is already running")]
    CannotAcquireFileLock,
    #[error("Database file cannot be a root path")]
    DatabasePathIsRootPath,
    #[error("IO Error: `{0}`")]
    IoError(#[from] std::io::Error),
    #[error("No password provided for encrypted wallet")]
    NoPasswordError,
    #[error("Deprecated operation error")]
    DeprecatedOperation,
    #[error("Key Manager Error: `{0}`")]
    KeyManagerError(#[from] KeyManagerError),
    #[error("Recovery Seed Error: {0}")]
    RecoverySeedError(String),
}

impl From<WalletStorageError> for ExitError {
    fn from(err: WalletStorageError) -> Self {
        use WalletStorageError::*;
        match err {
            NoPasswordError | InvalidPassphrase => ExitCode::IncorrectOrEmptyPassword.into(),
            e => ExitError::new(ExitCode::WalletError, e),
        }
    }
}
