// Copyright 2021. The Tari Project
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

use serde::{Deserialize, Serialize};
use tari_crypto::signatures::CommitmentAndPublicKeySignatureError;
use tari_key_manager::error::KeyManagerError;
use tari_utilities::ByteArrayError;
use thiserror::Error;

use crate::transactions::transaction_components::TransactionError;

#[derive(Debug, Error, PartialEq)]
pub enum CoreKeyManagerError {
    #[error("KeyManagerError: `{0}`")]
    KeyManagerError(#[from] KeyManagerError),
    #[error("Error generating Commitment and PublicKey signature: `{0}`")]
    CommitmentAndPublicKeySignatureError(String),
    #[error("Transaction error: `{0}`")]
    TransactionError(#[from] TransactionError),
}

impl From<CommitmentAndPublicKeySignatureError> for CoreKeyManagerError {
    fn from(err: CommitmentAndPublicKeySignatureError) -> Self {
        CoreKeyManagerError::CommitmentAndPublicKeySignatureError(err.to_string())
    }
}

/// Ledger device errors.
#[derive(Debug, PartialEq, Error, Deserialize, Serialize, Clone, Eq)]
pub enum LedgerDeviceError {
    /// HID API error
    #[error("HID API error `{0}`")]
    HidApi(String),
    /// Native HID transport error
    #[error("Native HID transport error `{0}`")]
    NativeTransport(String),
    /// Ledger application not started
    #[error("Ledger application not started")]
    ApplicationNotStarted,
    /// Ledger application instruction error
    #[error("Ledger application instruction error `{0}`")]
    Instruction(String),
    /// Ledger application processing error
    #[error("Processing error `{0}`")]
    Processing(String),
    /// Conversion error to or from ledger
    #[error("Conversion failed: {0}")]
    ByteArrayError(String),
    /// Not yet supported
    #[error("Ledger is not fully supported")]
    NotSupported,
}

impl From<ByteArrayError> for LedgerDeviceError {
    fn from(e: ByteArrayError) -> Self {
        LedgerDeviceError::ByteArrayError(e.to_string())
    }
}
