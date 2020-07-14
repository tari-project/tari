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
    transactions::transaction::TransactionError,
};
use derive_error::Error;

#[derive(Clone, Debug, PartialEq, Error)]
pub enum ValidationError {
    BlockHeaderError(BlockHeaderValidationError),
    BlockError(BlockValidationError),
    /// Contains kernels or inputs that are not yet spendable
    MaturityError,
    /// Contains unknown inputs
    UnknownInputs,
    /// The transaction has some transaction error
    TransactionError(TransactionError),
    /// Custom error with string message
    #[error(no_from, non_std, msg_embedded)]
    CustomError(String),
    /// A database instance must be set for this validator
    NoDatabaseConfigured,
    /// The total expected supply plus the total accumulated (offset) excess does not equal the sum of all UTXO
    /// commitments.
    InvalidAccountingBalance,
    /// Transaction contains already spent inputs
    ContainsSTxO,
    /// The recorded chain accumulated difficulty was stronger
    WeakerAccumulatedDifficulty,
    /// Invalid output merkle root
    InvalidOutputMr,
    /// Invalid kernel merkle root
    InvalidKernelMr,
    /// Invalid range proof merkle root
    InvalidRangeProofMr,
}

impl ValidationError {
    pub fn custom_error<T: ToString>(err: T) -> Self {
        ValidationError::CustomError(err.to_string())
    }
}
