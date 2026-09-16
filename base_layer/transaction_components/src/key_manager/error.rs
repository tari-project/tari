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
    #[error("Invalid key branch: `{0}`")]
    InvalidKeyBranch(String),
    #[error("Unexpected error: `{0}`")]
    UnexpectedError(String),
    #[error(
        "A script offset over {script_keys} contributing script key(s) and {sender_offset_keys} sender offset key(s) \
         would leave one side of the sum unblinded by a term the caller cannot compute. Both counts must be at least \
         one, and every script key id must contribute (`TariKeyId::Zero` does not)."
    )]
    UnblindedScriptOffset {
        script_keys: usize,
        sender_offset_keys: usize,
    },
    #[error(
        "None of the {script_keys} script key(s) in this script offset were derived on the ledger device, so the \
         reply would be a sender offset private key the device just generated. Outputs recovered by an older build \
         carry a random script key that the device cannot derive; re-run wallet recovery to replace them."
    )]
    NoDeviceScriptKeys { script_keys: usize },
    #[error(
        "The ledger device derives at most {max} sender offset keys in one exchange, but {requested} were requested"
    )]
    TooManySenderOffsetKeys { requested: usize, max: usize },
    #[error(
        "The ephemeral nonce handle counter is exhausted. This is not a full store - a full store evicts its oldest \
         entry - it is the one case that must refuse, because re-issuing a handle that an earlier nonce still answers \
         to would allow that nonce to be signed with twice."
    )]
    EphemeralNonceHandlesExhausted,
    #[error(
        "Ephemeral nonce handle `{handle}` was never issued, has already been signed with, or was evicted to make \
         room for a newer reservation. A nonce may only ever be used once: two signatures over different challenges \
         under one nonce give up the private key."
    )]
    UnknownEphemeralNonce { handle: u64 },
    #[error("The ephemeral nonce store lock is poisoned")]
    EphemeralNonceStorePoisoned,
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
