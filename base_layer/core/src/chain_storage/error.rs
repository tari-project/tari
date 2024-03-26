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

use lmdb_zero::error;
use tari_common_types::{chain_metadata::ChainMetaDataError, types::FixedHashSizeError};
use tari_mmr::{error::MerkleMountainRangeError, sparse_merkle_tree::SMTError, MerkleProofError};
use tari_storage::lmdb_store::LMDBError;
use thiserror::Error;
use tokio::task;

use crate::{
    blocks::BlockError,
    chain_storage::MmrTree,
    common::{BanPeriod, BanReason},
    proof_of_work::PowError,
    transactions::transaction_components::TransactionError,
    validation::ValidationError,
};

#[derive(Debug, Error)]
pub enum ChainStorageError {
    #[error("Access to the underlying storage mechanism failed: {0}")]
    AccessError(String),
    #[error(
        "The database may be corrupted or otherwise be in an inconsistent state. Please check logs to try and \
         identify the issue: {0}"
    )]
    CorruptedDatabase(String),
    #[error("A given input could not be spent because it was not in the UTXO set")]
    UnspendableInput,
    #[error("A problem occurred trying to move a STXO back into the UTXO pool during a reorg.")]
    UnspendError,
    #[error(
        "An unexpected result type was received for the given database request. This suggests that there is an \
         internal error or bug of sorts: {0}"
    )]
    UnexpectedResult(String),
    #[error("You tried to execute an invalid Database operation: {0}")]
    InvalidOperation(String),
    #[error("DATABASE INCONSISTENCY DETECTED at {function}: {details}")]
    DataInconsistencyDetected { function: &'static str, details: String },
    #[error("There appears to be a critical error on the back end: {0}. Check the logs for more information.")]
    CriticalError(String),
    #[error("Could not insert {table}: {error}")]
    InsertError { table: &'static str, error: String },
    #[error("An invalid query was attempted: {0}")]
    InvalidQuery(String),
    #[error("Invalid argument `{arg}` in `{func}`: {message}")]
    InvalidArguments {
        func: &'static str,
        arg: &'static str,
        message: String,
    },
    #[error("The requested {entity} was not found via {field}:{value} in the database")]
    ValueNotFound {
        entity: &'static str,
        field: &'static str,
        value: String,
    },
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
    #[error("Validation error: {source}")]
    ValidationError {
        #[from]
        source: ValidationError,
    },
    #[error("The MMR root for {0} in the provided block header did not match the MMR root in the database")]
    MismatchedMmrRoot(MmrTree),
    #[error("An invalid block was submitted to the database: {0}")]
    InvalidBlock(String),
    #[error("Blocking task spawn error: {0}")]
    BlockingTaskSpawnError(String),
    #[error("A request was out of range")]
    OutOfRange,
    #[error("LMDB error: {source}")]
    LmdbError {
        #[from]
        source: LMDBError,
    },
    #[error("Invalid proof of work: {source}")]
    ProofOfWorkError {
        #[from]
        source: PowError,
    },
    #[error("Cannot acquire exclusive file lock, another instance of the application is already running")]
    CannotAcquireFileLock,
    #[error("IO Error: `{0}`")]
    IoError(#[from] std::io::Error),
    #[error("Cannot calculate MMR roots for block that does not form a chain with the current tip. {0}")]
    CannotCalculateNonTipMmr(String),
    #[error("Key {key} in {table_name} already exists")]
    KeyExists { table_name: &'static str, key: String },
    #[error("Database resize required")]
    DbResizeRequired(Option<usize>),
    #[error("DB transaction was too large ({0} operations)")]
    DbTransactionTooLarge(usize),
    #[error("DB needs to be resynced: {0}")]
    DatabaseResyncRequired(&'static str),
    #[error("Block error: {0}")]
    BlockError(#[from] BlockError),
    #[error("Add block is currently locked. No blocks may be added using add_block until the flag is cleared.")]
    AddBlockOperationLocked,
    #[error("Transaction Error: {0}")]
    TransactionError(#[from] TransactionError),
    #[error("Could not convert data:{0}")]
    ConversionError(String),
    #[error("FixedHashSize Error: {0}")]
    FixedHashSizeError(#[from] FixedHashSizeError),
    #[error("Composite key length was exceeded (THIS SHOULD NEVER HAPPEN)")]
    CompositeKeyLengthExceeded,
    #[error("Failed to decode key bytes: {0}")]
    FromKeyBytesFailed(String),
    #[error("Sparse Merkle Tree error: {0}")]
    SMTError(#[from] SMTError),
    #[error("Invalid ChainMetaData: {0}")]
    InvalidChainMetaData(#[from] ChainMetaDataError),
}

impl ChainStorageError {
    pub fn is_value_not_found(&self) -> bool {
        matches!(self, ChainStorageError::ValueNotFound { .. })
    }

    pub fn is_key_exist_error(&self) -> bool {
        matches!(self, ChainStorageError::KeyExists { .. })
    }

    pub fn get_ban_reason(&self) -> Option<BanReason> {
        match self {
            ChainStorageError::ProofOfWorkError { source: e } => e.get_ban_reason(),
            ChainStorageError::ValidationError { source: e } => e.get_ban_reason(),
            err @ ChainStorageError::UnspendableInput |
            err @ ChainStorageError::MerkleMountainRangeError { .. } |
            err @ ChainStorageError::MismatchedMmrRoot(_) |
            err @ ChainStorageError::TransactionError(_) |
            err @ ChainStorageError::SMTError(_) => Some(BanReason {
                reason: err.to_string(),
                ban_duration: BanPeriod::Long,
            }),
            _err @ ChainStorageError::AccessError(_) |
            _err @ ChainStorageError::CorruptedDatabase(_) |
            _err @ ChainStorageError::UnexpectedResult(_) |
            _err @ ChainStorageError::InvalidOperation(_) |
            _err @ ChainStorageError::UnspendError |
            _err @ ChainStorageError::DataInconsistencyDetected { .. } |
            _err @ ChainStorageError::CriticalError(_) |
            _err @ ChainStorageError::InsertError { .. } |
            _err @ ChainStorageError::InvalidQuery(_) |
            _err @ ChainStorageError::InvalidArguments { .. } |
            _err @ ChainStorageError::ValueNotFound { .. } |
            _err @ ChainStorageError::MerkleProofError { .. } |
            _err @ ChainStorageError::InvalidBlock(_) |
            _err @ ChainStorageError::BlockingTaskSpawnError(_) |
            _err @ ChainStorageError::LmdbError { .. } |
            _err @ ChainStorageError::CannotAcquireFileLock |
            _err @ ChainStorageError::IoError(_) |
            _err @ ChainStorageError::CannotCalculateNonTipMmr(_) |
            _err @ ChainStorageError::KeyExists { .. } |
            _err @ ChainStorageError::DbResizeRequired(_) |
            _err @ ChainStorageError::DbTransactionTooLarge(_) |
            _err @ ChainStorageError::DatabaseResyncRequired(_) |
            _err @ ChainStorageError::BlockError(_) |
            _err @ ChainStorageError::AddBlockOperationLocked |
            _err @ ChainStorageError::ConversionError(_) |
            _err @ ChainStorageError::FixedHashSizeError(_) |
            _err @ ChainStorageError::CompositeKeyLengthExceeded |
            _err @ ChainStorageError::FromKeyBytesFailed(_) |
            _err @ ChainStorageError::InvalidChainMetaData(_) |
            _err @ ChainStorageError::OutOfRange => None,
        }
    }
}

impl From<task::JoinError> for ChainStorageError {
    fn from(err: task::JoinError) -> Self {
        Self::BlockingTaskSpawnError(err.to_string())
    }
}

impl From<lmdb_zero::Error> for ChainStorageError {
    fn from(err: lmdb_zero::Error) -> Self {
        use lmdb_zero::Error::Code;
        match err {
            Code(error::NOTFOUND) => ChainStorageError::ValueNotFound {
                entity: "<unspecified entity>",
                field: "<unknown>",
                value: "<unknown>".to_string(),
            },
            Code(error::MAP_FULL) => ChainStorageError::DbResizeRequired(None),
            _ => ChainStorageError::AccessError(err.to_string()),
        }
    }
}

pub trait Optional<U> {
    fn optional(self) -> Result<Option<U>, ChainStorageError>;
}

impl<U> Optional<U> for Result<U, ChainStorageError> {
    fn optional(self) -> Result<Option<U>, ChainStorageError> {
        match self {
            Ok(item) => Ok(Some(item)),
            Err(err) if err.is_value_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }
}

pub trait OrNotFound<U> {
    fn or_not_found(self, entity: &'static str, field: &'static str, value: String) -> Result<U, ChainStorageError>;
}

impl<U> OrNotFound<U> for Result<Option<U>, ChainStorageError> {
    fn or_not_found(self, entity: &'static str, field: &'static str, value: String) -> Result<U, ChainStorageError> {
        self.and_then(|inner| inner.ok_or(ChainStorageError::ValueNotFound { entity, field, value }))
    }
}

impl<U> OrNotFound<U> for Result<U, lmdb_zero::Error> {
    fn or_not_found(self, entity: &'static str, field: &'static str, value: String) -> Result<U, ChainStorageError> {
        use lmdb_zero::Error::Code;
        match self {
            Ok(v) => Ok(v),
            Err(err) => match err {
                Code(c) if c == lmdb_zero::error::NOTFOUND => {
                    Err(ChainStorageError::ValueNotFound { entity, field, value })
                },
                err => Err(err.into()),
            },
        }
    }
}
