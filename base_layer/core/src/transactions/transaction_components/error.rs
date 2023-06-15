// Copyright 2018 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use serde::{Deserialize, Serialize};
use tari_crypto::{
    errors::RangeProofError,
    signatures::{CommitmentAndPublicKeySignatureError, SchnorrSignatureError},
};
use tari_key_manager::key_manager_service::KeyManagerServiceError;
use tari_script::ScriptError;
use thiserror::Error;

use crate::{covenants::CovenantError, transactions::transaction_components::EncryptedDataError};

//----------------------------------------     TransactionError   ----------------------------------------------------//
#[derive(Clone, Debug, PartialEq, Error, Deserialize, Serialize)]
pub enum TransactionError {
    #[error("Error validating the transaction: {0}")]
    ValidationError(String),
    #[error("Signature is invalid: {0}")]
    InvalidSignatureError(String),
    #[error("Transaction kernel does not contain a signature")]
    NoSignatureError,
    #[error("A range proof construction or verification has produced an error: {0}")]
    RangeProofError(#[from] RangeProofError),
    #[error("An error occurred while performing a commitment signature: {0}")]
    SigningError(#[from] CommitmentAndPublicKeySignatureError),
    #[error("Invalid kernel in body : {0}")]
    InvalidKernel(String),
    #[error("Invalid coinbase in body")]
    InvalidCoinbase,
    #[error("Invalid coinbase maturity in body")]
    InvalidCoinbaseMaturity,
    #[error("More than one coinbase in body")]
    MoreThanOneCoinbase,
    #[error("No coinbase in body")]
    NoCoinbase,
    #[error("Missing range proof")]
    MissingRangeProof,
    #[error("Input maturity not reached")]
    InputMaturity,
    #[error("Tari script error : {0}")]
    ScriptError(#[from] ScriptError),
    #[error("Schnorr signature error : {0}")]
    SchnorrSignatureError(#[from] SchnorrSignatureError),
    #[error("Error performing conversion: {0}")]
    ConversionError(String),
    #[error("Error performing encryption: {0}")]
    EncryptionError(String),
    #[error("The script offset in body does not balance")]
    ScriptOffset,
    #[error("Error executing script: {0}")]
    ScriptExecutionError(String),
    #[error("TransactionInput is missing the data from the output being spent")]
    MissingTransactionInputData,
    #[error("Error executing covenant: {0}")]
    CovenantError(String),
    #[error("Committee contains too many members: contains {len} members but maximum is {max}")]
    InvalidCommitteeLength { len: usize, max: usize },
    #[error("Missing validator node signature")]
    MissingValidatorNodeSignature,
    #[error("Only coinbase outputs may have extra coinbase info")]
    NonCoinbaseHasOutputFeaturesCoinbaseExtra,
    #[error("Coinbase extra size is {len} but the maximum is {max}")]
    InvalidOutputFeaturesCoinbaseExtraSize { len: usize, max: u32 },
    #[error("Invalid revealed value : {0}")]
    InvalidRevealedValue(String),
    #[error("KeyManager encountered an error: {0}")]
    KeyManagerError(String),
    #[error("EncryptedData error : {0}")]
    EncryptedDataError(String),
}

impl From<CovenantError> for TransactionError {
    fn from(err: CovenantError) -> Self {
        TransactionError::CovenantError(err.to_string())
    }
}

impl From<KeyManagerServiceError> for TransactionError {
    fn from(err: KeyManagerServiceError) -> Self {
        TransactionError::KeyManagerError(err.to_string())
    }
}

impl From<EncryptedDataError> for TransactionError {
    fn from(err: EncryptedDataError) -> Self {
        TransactionError::EncryptedDataError(err.to_string())
    }
}
