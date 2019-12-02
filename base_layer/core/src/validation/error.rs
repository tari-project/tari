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
use tari_transactions::transaction::TransactionError;

#[derive(Clone, Debug, PartialEq, Error)]
pub enum ValidationError {
    BlockError(BlockValidationError),
    BodyError(BodyValidationError),
    /// Custom error with string message
    InvalidRangeProof,
    /// Custom error with string message
    #[error(no_from, non_std, msg_embedded)]
    CustomError(String),
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum BlockValidationError {
    /// A transaction in the block failed to validate
    TransactionError(TransactionError),
    /// Invalid Proof of work for the block
    InvalidPow,
    /// Invalid kernel in block
    InvalidKernel,
    /// Invalid input in block
    InvalidInput,
    /// Input maturity not reached
    InputMaturity,
    /// Invalid coinbase maturity in block or more than one coinbase
    InvalidCoinbase,
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum BodyValidationError {
    /// The sum of the input and output commitments doesn't equal the sum of the kernel excesses
    InconsistentCommitmentSum,
    /// A kernel signature was invalid
    InvalidKernelSignature,
}
