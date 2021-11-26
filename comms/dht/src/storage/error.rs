// Copyright 2020, The Tari Project
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

use tari_common_sqlite::error::SqliteStorageError;
use tari_utilities::message_format::MessageFormatError;
use thiserror::Error;
use tokio::task;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database path contained non-UTF8 characters that are not supported by the host OS")]
    InvalidUnicodePath,
    #[error("ConnectionError: {0}")]
    ConnectionError(#[from] diesel::ConnectionError),
    #[error("UniqueViolation")]
    UniqueViolation(String),
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
