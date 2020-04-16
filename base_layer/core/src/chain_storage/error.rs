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
    chain_storage::{db_transaction::DbKey, MmrTree},
    validation::ValidationError,
};
use tari_mmr::{error::MerkleMountainRangeError, MerkleProofError};
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq)]
pub enum ChainStorageError {
    #[error("Access to the underlying storage mechanism failed:{0}")]
    AccessError(String),
    #[error(
        "The database may be corrupted or otherwise be in an inconsistent state. Please check logs to try and \
         identify the issue:{0}"
    )]
    CorruptedDatabase(String),
    #[error("A given input could not be spent because it was not in the UTXO set")]
    UnspendableInput,
    #[error("A problem occurred trying to move a STXO back into the UTXO pool during a re-org.")]
    UnspendError,
    #[error(
        "An unexpected result type was received for the given database request. This suggests that there is an \
         internal error or bug of sorts: {0}"
    )]
    UnexpectedResult(String),
    #[error("You tried to execute an invalid Database operation:{0}")]
    InvalidOperation(String),
    #[error("There appears to be a critical error on the back end:{0}. Check the logs for more information.")]
    CriticalError(String),
    #[error("Cannot return data for requests older than the current pruning horizon")]
    BeyondPruningHorizon,
    #[error("A parameter to the request was invalid")]
    InvalidQuery(String),
    #[error("The requested value '{0}' was not found in the database")]
    ValueNotFound(DbKey),
    #[error("MMR error: {source}")]
    MerkleMountainRangeError {
        #[from]
        source: MerkleMountainRangeError,
    },
    #[error("Merkle proof error: {source}")]
    MerkleProofError {
        #[from]
        source: MerkleProofError,
    },
    #[error("Validation error:{source}")]
    ValidationError {
        #[from]
        source: ValidationError,
    },
    #[error("The MMR root for {0} in the provided block header did not match the MMR root in the database")]
    MismatchedMmrRoot(MmrTree),
    #[error("An invalid block was submitted to the database")]
    InvalidBlock,
    #[error("Blocking task spawn error:{0}")]
    BlockingTaskSpawnError(String),
    #[error("A request was out of range")]
    OutOfRange,
}
