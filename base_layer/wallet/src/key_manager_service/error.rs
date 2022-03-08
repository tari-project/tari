//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use diesel::result::Error as DieselError;
use tari_crypto::script::ScriptError;
use tari_key_manager::error::KeyManagerError as KMError;
use tari_utilities::{hex::HexError, ByteArrayError};

use crate::error::WalletStorageError;

#[derive(Debug, thiserror::Error)]
pub enum KeyManagerError {
    #[error("Branch does not exist")]
    UnknownKeyBranch,
    #[error("Master seed does not match stored version")]
    MasterSeedMismatch,
    #[error("Could not find key in key manager")]
    KeyNotFoundInKeyChain,
    #[error("Storage error: `{0}`")]
    KeyManagerStorageError(#[from] KeyManagerStorageError),
    #[error("Byte array error: `{0}`")]
    ByteArrayError(#[from] ByteArrayError),
    #[error("Tari Key Manager error: `{0}`")]
    TariKeyManagerError(#[from] KMError),
}

#[derive(Debug, thiserror::Error)]
pub enum KeyManagerStorageError {
    #[error("Value not found")]
    ValueNotFound,
    #[error("Unexpected result: `{0}`")]
    UnexpectedResult(String),
    #[error("Pending transaction does not exist to be confirmed")]
    PendingTransactionNotFound,
    #[error("This write operation is not supported for provided DbKey")]
    OperationNotSupported,
    #[error("Could not find all values specified for batch operation")]
    ValuesNotFound,
    #[error("Error converting a type: {reason}")]
    ConversionError { reason: String },
    #[error("Key Manager not initialized")]
    KeyManagerNotInitialized,
    #[error("Wallet storage error: `{0}`")]
    WalletStorageError(#[from] WalletStorageError),
    #[error("Diesel error: `{0}`")]
    DieselError(#[from] DieselError),
    #[error("Diesel connection error: `{0}`")]
    DieselConnectionError(#[from] diesel::ConnectionError),
    #[error("Database migration error: `{0}`")]
    DatabaseMigrationError(String),
    #[error("Blocking task spawn error: `{0}`")]
    BlockingTaskSpawnError(String),
    #[error("Wallet db is already encrypted and cannot be encrypted until the previous encryption is removed")]
    AlreadyEncrypted,
    #[error("Wallet db is currently encrypted, decrypt before use")]
    ValueEncrypted,
    #[error("Byte array error: `{0}`")]
    ByteArrayError(#[from] ByteArrayError),
    #[error("Aead error: `{0}`")]
    AeadError(String),
    #[error("Tari script error : {0}")]
    ScriptError(#[from] ScriptError),
    #[error("Binary not stored as valid hex:{0}")]
    HexError(#[from] HexError),
    #[error("Tari Key Manager error: `{0}`")]
    TariKeyManagerError(#[from] KMError),
}
