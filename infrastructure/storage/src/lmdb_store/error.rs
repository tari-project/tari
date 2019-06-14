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

use derive_error::Error;

#[derive(Debug, Error)]
pub enum LMDBError {
    /// Cannot create LMDB. The path does not exist
    InvalidPath,
    /// An error occurred with the underlying data store implementation
    #[error(embedded_msg, no_from, non_std)]
    InternalError(String),
    /// An error occurred during serialization
    #[error(no_from, non_std)]
    SerializationErr(String),
    /// An error occurred during deserialization
    #[error(no_from, non_std)]
    DeserializationErr(String),
    /// Occurs when trying to perform an action that requires us to be in a live transaction
    TransactionNotLiveError,
    /// A transaction or query was attempted while no database was open.
    DatabaseNotOpen,
    /// A database with the requested name does not exist
    UnknownDatabase,
    /// An error occurred during a put query
    #[error(embedded_msg, no_from, non_std)]
    PutError(String),
    /// An error occurred during a get query
    #[error(embedded_msg, no_from, non_std)]
    GetError(String),
    #[error(embedded_msg, no_from, non_std)]
    CommitError(String),
    /// An LMDB error occurred
    DatabaseError(lmdb_zero::error::Error),
}
