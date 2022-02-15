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

use std::io;

use serde::{Deserialize, Serialize};
use tari_crypto::{range_proof::RangeProofError, script::ScriptError, signatures::CommitmentSignatureError};
use thiserror::Error;

use crate::covenants::CovenantError;

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
    SigningError(#[from] CommitmentSignatureError),
    #[error("Invalid kernel in body")]
    InvalidKernel,
    #[error("Invalid coinbase in body")]
    InvalidCoinbase,
    #[error("Invalid coinbase maturity in body")]
    InvalidCoinbaseMaturity,
    #[error("More than one coinbase in body")]
    MoreThanOneCoinbase,
    #[error("No coinbase in body")]
    NoCoinbase,
    #[error("Input maturity not reached")]
    InputMaturity,
    #[error("Tari script error : {0}")]
    ScriptError(#[from] ScriptError),
    #[error("Error performing conversion: {0}")]
    ConversionError(String),
    #[error("The script offset in body does not balance")]
    ScriptOffset,
    #[error("Error executing script: {0}")]
    ScriptExecutionError(String),
    #[error("TransactionInput is missing the data from the output being spent")]
    MissingTransactionInputData,
    #[error("Error executing covenant: {0}")]
    CovenantError(String),
    #[error("Consensus encoding error: {0}")]
    ConsensusEncodingError(String),
}

impl From<CovenantError> for TransactionError {
    fn from(err: CovenantError) -> Self {
        TransactionError::CovenantError(err.to_string())
    }
}

impl From<io::Error> for TransactionError {
    fn from(err: io::Error) -> Self {
        TransactionError::ConsensusEncodingError(err.to_string())
    }
}
