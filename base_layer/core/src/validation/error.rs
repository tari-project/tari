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
    blocks::{blockheader::BlockHeaderValidationError, BlockValidationError},
    chain_storage::ChainStorageError,
    proof_of_work::PowError,
    transactions::transaction::TransactionError,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Block header validation failed: {0}")]
    BlockHeaderError(#[from] BlockHeaderValidationError),
    #[error("Block validation error: {0}")]
    BlockError(#[from] BlockValidationError),
    #[error("Contains kernels or inputs that are not yet spendable")]
    MaturityError,
    #[error("Contains unknown inputs")]
    UnknownInputs,
    #[error("The transaction has some transaction error")]
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
    #[error("Transaction contains duplicate already existing Transactional output")]
    ContainsKnownTxO,
    #[error("The recorded chain accumulated difficulty was stronger")]
    WeakerAccumulatedDifficulty,
    #[error("Invalid output merkle root")]
    InvalidOutputMr,
    #[error("Invalid kernel merkle root")]
    InvalidKernelMr,
    #[error("Invalid range proof merkle root")]
    InvalidRangeProofMr,
    #[error("Final state validation failed: The UTXO set did not balance with the expected emission at height {0}")]
    ChainBalanceValidationFailed(u64),
    #[error("Proof of work error: {0}")]
    ProofOfWorkError(#[from] PowError),
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
