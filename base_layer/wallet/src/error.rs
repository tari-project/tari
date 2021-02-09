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
    base_node_service::error::BaseNodeServiceError,
    contacts_service::error::ContactsServiceError,
    output_manager_service::error::OutputManagerError,
    storage::database::DbKey,
    transaction_service::error::TransactionServiceError,
};
use diesel::result::Error as DieselError;
use log::SetLoggerError;
use serde_json::Error as SerdeJsonError;
use tari_comms::{connectivity::ConnectivityError, multiaddr, peer_manager::PeerManagerError};
use tari_comms_dht::store_forward::StoreAndForwardError;
use tari_crypto::tari_utilities::{hex::HexError, ByteArrayError};
use tari_p2p::{initialization::CommsInitializationError, services::liveness::error::LivenessError};
use tari_service_framework::ServiceInitializationError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WalletError {
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
    #[error("R2d2 error")]
    R2d2Error,
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
}
