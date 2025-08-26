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
use tari_utilities::ByteArrayError;
use thiserror::Error;

use crate::{
    tari_amount::MicroMinotari,
    transaction_components::{covenants::CovenantError, OutputType, RangeProofType, TransactionError},
};

#[derive(Debug, Error)]
pub enum AggregatedBodyValidationError {
    #[error("Serialization failed: {0}")]
    SerializationError(String),
    #[error("Contains kernels or inputs that are not yet spendable")]
    MaturityError,
    #[error("The block weight ({actual_weight}) is above the maximum ({max_weight})")]
    BlockTooLarge { actual_weight: u64, max_weight: u64 },
    #[error("The transaction is invalid: {0}")]
    TransactionError(#[from] TransactionError),
    #[error(
        "The total expected supply plus the total accumulated (offset) excess does not equal the sum of all UTXO \
         commitments."
    )]
    InvalidAccountingBalance,
    #[error("Duplicate or unsorted input found in block body")]
    UnsortedOrDuplicateInput,
    #[error("Duplicate or unsorted output found in block body")]
    UnsortedOrDuplicateOutput,
    #[error("Duplicate or unsorted kernel found in block body")]
    UnsortedOrDuplicateKernel,
    #[error(
        "Script exceeded maximum script size, expected less than {max_script_size} but was
    {actual_script_size}"
    )]
    TariScriptExceedsMaxSize {
        max_script_size: usize,
        actual_script_size: usize,
    },
    #[error(
        "Encrypted data exceeded maximum encrytped data size, expected less than {max_encrypted_data_size} but was \
         {actual_encrypted_data_size}"
    )]
    EncryptedDataExceedsMaxSize {
        max_encrypted_data_size: usize,
        actual_encrypted_data_size: usize,
    },
    #[error("Consensus Error: {0}")]
    ConsensusError(String),
    #[error("Covenant failed to validate: {0}")]
    CovenantError(#[from] CovenantError),
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
    #[error("Author signature not valid for template registration")]
    TemplateAuthorSignatureNotValid,
    #[error("Validator node registration signature failed verification")]
    InvalidValidatorNodeSignature,
    #[error("Sidechain ID knowledge proof not valid for validator node registration")]
    ValidatorNodeInvalidSidechainIdKnowledgeProof,
    #[error("Covenant too large. Max size: {max_size}, Actual size: {actual_size}")]
    CovenantTooLarge { max_size: usize, actual_size: usize },
    #[error("Invalid Serialized Public key: {0}")]
    InvalidSerializedPublicKey(String),
}

impl From<ByteArrayError> for AggregatedBodyValidationError {
    fn from(err: ByteArrayError) -> Self {
        Self::InvalidSerializedPublicKey(err.to_string())
    }
}
