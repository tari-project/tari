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

use std::num::TryFromIntError;

use diesel::{self, r2d2};
use tari_utilities::message_format::MessageFormatError;
use thiserror::Error;
use tokio::task;

#[derive(Debug, Error)]
pub enum SqliteStorageError {
    #[error("Poolsize is too big")]
    PoolSize(#[from] TryFromIntError),
    #[error("Operation not supported")]
    OperationNotSupported,
    #[error("Conversion error: `{0}`")]
    ConversionError(String),
    #[error("Database error: `{0}`")]
    R2d2Error(#[from] r2d2::Error),
    #[error("Database error: `{0}`")]
    DieselR2d2Error(String),
    #[error("Database error: `{0}`")]
    DieselConnectionError(#[from] diesel::ConnectionError),
    #[error("Database error: `{0}`")]
    DatabaseMigrationError(String),
    #[error("Database error: `{0}`")]
    BlockingTaskSpawnError(String),
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("ConnectionError: {0}")]
    ConnectionError(#[from] diesel::ConnectionError),
    #[error("Error when joining to tokio task : {0}")]
    JoinError(#[from] task::JoinError),
    #[error("DatabaseMigrationFailed: {0}")]
    DatabaseMigrationFailed(String),
    #[error("ResultError: {0}")]
    ResultError(#[from] diesel::result::Error),
    #[error("MessageFormatError: {0}")]
    MessageFormatError(#[from] MessageFormatError),
    #[error("Unexpected result: {0}")]
    UnexpectedResult(String),
    #[error("Diesel R2d2 error: `{0}`")]
    DieselR2d2Error(#[from] SqliteStorageError),
}
