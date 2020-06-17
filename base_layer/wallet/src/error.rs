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
    contacts_service::error::ContactsServiceError,
    output_manager_service::error::OutputManagerError,
    storage::database::DbKey,
    transaction_service::error::TransactionServiceError,
};
use derive_error::Error;
use diesel::result::Error as DieselError;
use log::SetLoggerError;
use serde_json::Error as SerdeJsonError;
use tari_comms::{connectivity::ConnectivityError, multiaddr, peer_manager::PeerManagerError};
use tari_comms_dht::store_forward::StoreAndForwardError;
use tari_crypto::tari_utilities::hex::HexError;
use tari_p2p::{initialization::CommsInitializationError, services::liveness::error::LivenessError};

#[derive(Debug, Error)]
pub enum WalletError {
    CommsInitializationError(CommsInitializationError),
    OutputManagerError(OutputManagerError),
    TransactionServiceError(TransactionServiceError),
    PeerManagerError(PeerManagerError),
    MultiaddrError(multiaddr::Error),
    WalletStorageError(WalletStorageError),
    SetLoggerError(SetLoggerError),
    ContactsServiceError(ContactsServiceError),
    LivenessServiceError(LivenessError),
    StoreAndForwardError(StoreAndForwardError),
    ConnectivityError(ConnectivityError),
}

#[derive(Debug, Error)]
pub enum WalletStorageError {
    /// Tried to insert an output that already exists in the database
    DuplicateContact,
    /// This write operation is not supported for provided DbKey
    OperationNotSupported,
    /// Error converting a type
    ConversionError,
    /// Could not find all values specified for batch operation
    ValuesNotFound,
    /// Db Path does not exist
    DbPathDoesNotExist,
    SerdeJsonError(SerdeJsonError),
    R2d2Error,
    DieselError(DieselError),
    DieselConnectionError(diesel::ConnectionError),
    #[error(msg_embedded, no_from, non_std)]
    DatabaseMigrationError(String),
    #[error(non_std, no_from)]
    ValueNotFound(DbKey),
    #[error(msg_embedded, non_std, no_from)]
    UnexpectedResult(String),
    #[error(msg_embedded, non_std, no_from)]
    BlockingTaskSpawnError(String),
    /// The storage path was invalid unicode or not supported by the host OS
    InvalidUnicodePath,
    HexError(HexError),
}
