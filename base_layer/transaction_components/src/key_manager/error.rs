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

// use diesel::result::Error as DieselError;
// use tari_common_sqlite::error::{SqliteStorageError, StorageError};

use tari_crypto::{
    errors::RangeProofError,
    signatures::{CommitmentAndPublicKeySignatureError, SchnorrSignatureError},
};
use tari_utilities::ByteArrayError;
use thiserror::Error;

use crate::transaction_components::{EncryptedDataError, TransactionError};
#[derive(Debug, Error, PartialEq, Clone)]
pub enum KeyManagerError {
    #[error("Error generating Commitment and PublicKey signature: `{0}`")]
    CommitmentAndPublicKeySignatureError(String),
    #[error("Transaction error: `{0}`")]
    TransactionError(#[from] TransactionError),
    #[error("Ledger error: `{0}`")]
    LedgerError(String),
    #[error("Invalid wallet type: `{0}`")]
    InvalidWalletType(String),
    #[error("Failed to encrypt: `{0}`")]
    EncryptionFailed(String),
    #[error("Invalid key id string: `{0}`")]
    InvalidKeyId(String),
    #[error("Unexpected error: `{0}`")]
    UnexpectedError(String),
    #[error("Byte array error: `{0}`")]
    ByteArrayError(String),
    #[error("Invalid range proof: `{0}`")]
    RangeProofError(String),
    #[error("EncryptedData error: `{0}`")]
    EncryptedDataError(#[from] EncryptedDataError),
}

impl From<RangeProofError> for KeyManagerError {
    fn from(e: RangeProofError) -> Self {
        KeyManagerError::RangeProofError(e.to_string())
    }
}

impl From<CommitmentAndPublicKeySignatureError> for KeyManagerError {
    fn from(err: CommitmentAndPublicKeySignatureError) -> Self {
        KeyManagerError::CommitmentAndPublicKeySignatureError(err.to_string())
    }
}

impl From<ByteArrayError> for KeyManagerError {
    fn from(e: ByteArrayError) -> Self {
        KeyManagerError::ByteArrayError(e.to_string())
    }
}

impl From<SchnorrSignatureError> for KeyManagerError {
    fn from(e: SchnorrSignatureError) -> Self {
        KeyManagerError::TransactionError(TransactionError::InvalidSignatureError(e.to_string()))
    }
}
