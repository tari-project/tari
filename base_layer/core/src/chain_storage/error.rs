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

use crate::chain_storage::db_transaction::DbKey;
use derive_error::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ChainStorageError {
    // Access to the underlying storage mechanism failed
    #[error(non_std, no_from)]
    AccessError(String),
    // The database may be corrupted or otherwise be in an inconsistent state. Please check logs to try and identify
    // the issue
    #[error(non_std, no_from)]
    CorruptedDatabase(String),
    // A given input could not be spent because it was not in the UTXO set
    UnspendableInput,
    // A problem occurred trying to move a STXO back into the UTXO pool during a re-org.
    UnspendError,
    // An unexpected result type was received for the given database request. This suggests that there is an internal
    // error or bug of sorts.
    #[error(msg_embedded, non_std, no_from)]
    UnexpectedResult(String),
    // You tried to execute an invalid Database operation
    #[error(msg_embedded, non_std, no_from)]
    InvalidOperation(String),
    // There appears to be a critical error on the back end. The database might be in an inconsistent state. Check
    // the logs for more information.
    CriticalError,
    // Cannot return data for requests older than the current pruning horizon
    BeyondPruningHorizon,
    // A parameter to the request was invalid
    #[error(msg_embedded, non_std, no_from)]
    InvalidQuery(String),
    // The requested value was not found in the database
    #[error(non_std, no_from)]
    ValueNotFound(DbKey),
}
