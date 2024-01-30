//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::io;

use diesel::ConnectionError;
use minotari_app_utilities::identity_management::IdentityError;
use tari_common_sqlite::error::StorageError as SqliteStorageError;
use tari_comms::peer_manager::PeerManagerError;
use tari_contacts::contacts_service::error::ContactsServiceError;
use tari_p2p::initialization::CommsInitializationError;
use tari_storage::lmdb_store::LMDBError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Couldn't initialize the chat client: {0}")]
    InitializationError(String),
    #[error("Networking error: {0}")]
    NetworkingError(#[from] NetworkingError),
    #[error("The client had a problem communication with the contacts service: {0}")]
    ContactsServiceError(#[from] ContactsServiceError),
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Couldn't connect to database: {0}")]
    ConnectionError(#[from] ConnectionError),
    #[error("Couldn't create chat storage: {0}")]
    CreationError(#[from] io::Error),
    #[error("Couldn't convert db file path to string")]
    FilePathError,
    #[error("Couldn't create chat storage: {0}")]
    LMDBError(#[from] LMDBError),
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkingError {
    #[error("Couldn't initialize comms: {0}")]
    CommsInitializationError(#[from] CommsInitializationError),
    #[error("Storage error: {0}")]
    SqliteStorageError(#[from] SqliteStorageError),
    #[error("Identity error: {0}")]
    IdentityError(#[from] IdentityError),
    #[error("Error mapping the peer seeds: {0}")]
    PeerSeeds(String),
    #[error("Identity error: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    #[error("Service initializer error: {0}")]
    ServiceInitializerError(#[from] anyhow::Error),
    #[error("Comms failed to spawn")]
    CommsSpawnError,
}
