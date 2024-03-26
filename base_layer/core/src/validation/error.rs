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

use crate::{
    blocks::{BlockHeaderValidationError, BlockValidationError},
    chain_storage::ChainStorageError,
    common::{BanPeriod, BanReason},
    covenants::CovenantError,
    proof_of_work::{monero_rx::MergeMineError, DifficultyError, PowError},
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{OutputType, RangeProofType, TransactionError},
    },
};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Serialization failed: {0}")]
    SerializationError(String),
    #[error("Block header validation failed: {0}")]
    BlockHeaderError(#[from] BlockHeaderValidationError),
    #[error("Block validation error: {0}")]
    BlockError(#[from] BlockValidationError),
    #[error("Contains kernels or inputs that are not yet spendable")]
    MaturityError,
    #[error("The block weight ({actual_weight}) is above the maximum ({max_weight})")]
    BlockTooLarge { actual_weight: u64, max_weight: u64 },
    #[error("Contains {} unknown inputs", .0.len())]
    UnknownInputs(Vec<HashOutput>),
    #[error("Contains an unknown input")]
    UnknownInput,
    #[error("The transaction is invalid: {0}")]
    TransactionError(#[from] TransactionError),
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
    #[error("Final state validation failed: The UTXO set did not balance with the expected emission at height {0}")]
    ChainBalanceValidationFailed(u64),
    #[error("The total value + fees of the block exceeds the maximum allowance on chain")]
    CoinbaseExceedsMaxLimit,
    #[error("Proof of work error: {0}")]
    ProofOfWorkError(#[from] PowError),
    #[error("Attempted to validate genesis block")]
    ValidatingGenesis,
    #[error("Duplicate or unsorted input found in block body")]
    UnsortedOrDuplicateInput,
    #[error("Duplicate or unsorted output found in block body")]
    UnsortedOrDuplicateOutput,
    #[error("Duplicate or unsorted kernel found in block body")]
    UnsortedOrDuplicateKernel,
    #[error("Error in merge mine data:{0}")]
    MergeMineError(#[from] MergeMineError),
    #[error("Maximum transaction weight exceeded")]
    MaxTransactionWeightExceeded,
    #[error("Expected block height to be {expected}, but was {block_height}")]
    IncorrectHeight { expected: u64, block_height: u64 },
    #[error("Expected block previous hash to be {expected}, but was {block_hash}")]
    IncorrectPreviousHash { expected: String, block_hash: String },
    #[error("Bad block with hash {hash} found")]
    BadBlockFound { hash: String, reason: String },
    #[error("Script exceeded maximum script size, expected less than {max_script_size} but was {actual_script_size}")]
    TariScriptExceedsMaxSize {
        max_script_size: usize,
        actual_script_size: usize,
    },
    #[error("Consensus Error: {0}")]
    ConsensusError(String),
    #[error("Duplicate kernel Error: {0}")]
    DuplicateKernelError(String),
    #[error("Covenant failed to validate: {0}")]
    CovenantError(#[from] CovenantError),
    #[error("Invalid or unsupported blockchain version {version}")]
    InvalidBlockchainVersion { version: u16 },
    #[error("Contains Invalid Burn: {0}")]
    InvalidBurnError(String),
    #[error("Output type '{output_type}' is not permitted")]
    OutputTypeNotPermitted { output_type: OutputType },
    #[error("Range proof type '{range_proof_type}' is not permitted")]
    RangeProofTypeNotPermitted { range_proof_type: RangeProofType },
    #[error("Output type '{output_type}' is not matched to any range proof type")]
    OutputTypeNotMatchedToRangeProofType { output_type: OutputType },
    #[error("Validator registration has invalid minimum amount {actual}, must be at least {min}")]
    ValidatorNodeRegistrationMinDepositAmount { min: MicroMinotari, actual: MicroMinotari },
    #[error("Validator registration has invalid maturity {actual}, must be at least {min}")]
    ValidatorNodeRegistrationMinLockHeight { min: u64, actual: u64 },
    #[error("Validator node registration signature failed verification")]
    InvalidValidatorNodeSignature,
    #[error(
        "An unexpected number of timestamps were provided to the header validator. THIS IS A BUG. Expected \
         {expected}, got {actual}"
    )]
    IncorrectNumberOfTimestampsProvided { expected: u64, actual: u64 },
    #[error("Invalid difficulty: {0}")]
    DifficultyError(#[from] DifficultyError),
    #[error("Covenant too large. Max size: {max_size}, Actual size: {actual_size}")]
    CovenantTooLarge { max_size: usize, actual_size: usize },
}

// ChainStorageError has a ValidationError variant, so to prevent a cyclic dependency we use a string representation in
// for storage errors that cause validation failures.
impl From<ChainStorageError> for ValidationError {
    fn from(err: ChainStorageError) -> Self {
        Self::FatalStorageError(err.to_string())
    }
}

impl ValidationError {
    pub fn get_ban_reason(&self) -> Option<BanReason> {
        match self {
            ValidationError::ProofOfWorkError(e) => e.get_ban_reason(),
            err @ ValidationError::SerializationError(_) |
            err @ ValidationError::BlockHeaderError(_) |
            err @ ValidationError::BlockError(_) |
            err @ ValidationError::MaturityError |
            err @ ValidationError::BlockTooLarge { .. } |
            err @ ValidationError::UnknownInputs(_) |
            err @ ValidationError::UnknownInput |
            err @ ValidationError::TransactionError(_) |
            err @ ValidationError::InvalidAccountingBalance |
            err @ ValidationError::ContainsSTxO |
            err @ ValidationError::ContainsTxO |
            err @ ValidationError::ContainsDuplicateUtxoCommitment |
            err @ ValidationError::ChainBalanceValidationFailed(_) |
            err @ ValidationError::ValidatingGenesis |
            err @ ValidationError::UnsortedOrDuplicateInput |
            err @ ValidationError::UnsortedOrDuplicateOutput |
            err @ ValidationError::UnsortedOrDuplicateKernel |
            err @ ValidationError::MaxTransactionWeightExceeded |
            err @ ValidationError::IncorrectHeight { .. } |
            err @ ValidationError::IncorrectPreviousHash { .. } |
            err @ ValidationError::BadBlockFound { .. } |
            err @ ValidationError::TariScriptExceedsMaxSize { .. } |
            err @ ValidationError::ConsensusError(_) |
            err @ ValidationError::DuplicateKernelError(_) |
            err @ ValidationError::CovenantError(_) |
            err @ ValidationError::InvalidBlockchainVersion { .. } |
            err @ ValidationError::InvalidBurnError(_) |
            err @ ValidationError::OutputTypeNotPermitted { .. } |
            err @ ValidationError::RangeProofTypeNotPermitted { .. } |
            err @ ValidationError::OutputTypeNotMatchedToRangeProofType { .. } |
            err @ ValidationError::ValidatorNodeRegistrationMinDepositAmount { .. } |
            err @ ValidationError::ValidatorNodeRegistrationMinLockHeight { .. } |
            err @ ValidationError::InvalidValidatorNodeSignature |
            err @ ValidationError::DifficultyError(_) |
            err @ ValidationError::CoinbaseExceedsMaxLimit |
            err @ ValidationError::CovenantTooLarge { .. } => Some(BanReason {
                reason: err.to_string(),
                ban_duration: BanPeriod::Long,
            }),
            ValidationError::MergeMineError(e) => e.get_ban_reason(),
            ValidationError::FatalStorageError(_) | ValidationError::IncorrectNumberOfTimestampsProvided { .. } => None,
        }
    }
}
