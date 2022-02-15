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

use tari_common_types::types::HashOutput;
use thiserror::Error;
use tokio::task;

use crate::{
    blocks::{BlockHeaderValidationError, BlockValidationError},
    chain_storage::ChainStorageError,
    covenants::CovenantError,
    proof_of_work::{monero_rx::MergeMineError, PowError},
    transactions::transaction_components::TransactionError,
};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Block header validation failed: {0}")]
    BlockHeaderError(#[from] BlockHeaderValidationError),
    #[error("Block validation error: {0}")]
    BlockError(#[from] BlockValidationError),
    #[error("Contains kernels or inputs that are not yet spendable")]
    MaturityError,
    #[error("Contains {} unknown inputs", .0.len())]
    UnknownInputs(Vec<HashOutput>),
    #[error("Contains an unknown input")]
    UnknownInput,
    #[error("The transaction is invalid: {0}")]
    TransactionError(#[from] TransactionError),
    #[error("Error: {0}")]
    CustomError(String),
    #[error("Fatal storage error during validation: {0}")]
    FatalStorageError(String),
    #[error(
        "The total expected supply plus the total accumulated (offset) excess does not equal the sum of all UTXO \
         commitments."
    )]
    InvalidAccountingBalance,
    #[error("Transaction contains already spent inputs")]
    ContainsSTxO,
    #[error("Transaction contains outputs that already exist")]
    ContainsTxO,
    #[error("Transaction contains an output commitment that already exists")]
    ContainsDuplicateUtxoCommitment,
    #[error("Transaction contains an output unique_id that already exists")]
    ContainsDuplicateUtxoUniqueID,
    #[error("Unique ID in input is not present in outputs")]
    UniqueIdInInputNotPresentInOutputs,
    #[error("Unique ID was present in more than one output")]
    DuplicateUniqueIdInOutputs,
    #[error("Unique ID was marked as burned, but was present in a new output")]
    UniqueIdBurnedButPresentInOutputs,
    #[error("Final state validation failed: The UTXO set did not balance with the expected emission at height {0}")]
    ChainBalanceValidationFailed(u64),
    #[error("Proof of work error: {0}")]
    ProofOfWorkError(#[from] PowError),
    #[error("Attempted to validate genesis block")]
    ValidatingGenesis,
    #[error("Previous block hash not found")]
    PreviousHashNotFound,
    #[error("Duplicate or unsorted input found in block body")]
    UnsortedOrDuplicateInput,
    #[error("Duplicate or unsorted output found in block body")]
    UnsortedOrDuplicateOutput,
    #[error("Duplicate or unsorted kernel found in block body")]
    UnsortedOrDuplicateKernel,
    #[error("Error in merge mine data:{0}")]
    MergeMineError(#[from] MergeMineError),
    #[error("Contains an input with an invalid mined-height in body")]
    InvalidMinedHeight,
    #[error("Maximum transaction weight exceeded")]
    MaxTransactionWeightExceeded,
    #[error("End of time: {0}")]
    EndOfTimeError(String),
    #[error("Expected block height to be {expected}, but was {block_height}")]
    IncorrectNextTipHeight { expected: u64, block_height: u64 },
    #[error("Expected block previous hash to be {expected}, but was {block_hash}")]
    IncorrectPreviousHash { expected: String, block_hash: String },
    #[error("Async validation task failed: {0}")]
    AsyncTaskFailed(#[from] task::JoinError),
    #[error("Could not find the Output being spent by Transaction Input")]
    TransactionInputSpentOutputMissing,
    #[error("Output being spent by Transaction Input has already been pruned")]
    TransactionInputSpendsPrunedOutput,
    #[error("Bad block with hash {hash} found")]
    BadBlockFound { hash: String },
    #[error("Script exceeded maximum script size, expected less than {max_script_size} but was {actual_script_size}")]
    TariScriptExceedsMaxSize {
        max_script_size: usize,
        actual_script_size: usize,
    },
    #[error("Consensus Error: {0}")]
    ConsensusError(String),
    #[error("Covenant failed to validate: {0}")]
    CovenantError(#[from] CovenantError),
}

// ChainStorageError has a ValidationError variant, so to prevent a cyclic dependency we use a string representation in
// for storage errors that cause validation failures.
impl From<ChainStorageError> for ValidationError {
    fn from(err: ChainStorageError) -> Self {
        Self::FatalStorageError(err.to_string())
    }
}

impl ValidationError {
    pub fn custom_error<T: ToString>(err: T) -> Self {
        ValidationError::CustomError(err.to_string())
    }
}
