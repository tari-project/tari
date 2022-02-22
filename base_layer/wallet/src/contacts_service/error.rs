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
use tari_p2p::services::liveness::error::LivenessError;
use tari_service_framework::reply_channel::TransportChannelError;
use thiserror::Error;

use crate::{contacts_service::storage::database::DbKey, error::WalletStorageError};

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum ContactsServiceError {
    #[error("Contact is not found")]
    ContactNotFound,
    #[error("Received incorrect response from service request")]
    UnexpectedApiResponse,
    #[error("Contacts service storage error: `{0}`")]
    ContactsServiceStorageError(#[from] ContactsServiceStorageError),
    #[error("Transport channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),
    #[error("Livenessl error: `{0}`")]
    LivenessError(#[from] LivenessError),
}

#[derive(Debug, Error)]
pub enum ContactsServiceStorageError {
    #[error("This write operation is not supported for provided DbKey")]
    OperationNotSupported,
    #[error("Error converting a type")]
    ConversionError,
    #[error("Could not find all values specified for batch operation")]
    ValuesNotFound,
    #[error("Value not found error: `{0}`")]
    ValueNotFound(DbKey),
    #[error("Unexpected result error: `{0}`")]
    UnexpectedResult(String),
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
}
