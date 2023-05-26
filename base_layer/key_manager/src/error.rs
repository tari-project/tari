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

use argon2::password_hash::Error as PasswordHashError;
use tari_crypto::errors::SliceError;
use tari_utilities::ByteArrayError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum KeyManagerError {
    #[error("Could not convert into byte array: `{0}`")]
    ByteArrayError(#[from] ByteArrayError),
    #[error("Mnemonic Error: `{0}`")]
    MnemonicError(#[from] MnemonicError),
    #[error("Error with password hashing: `{0}`")]
    PasswordHashError(#[from] PasswordHashError),
    #[error("Cryptographic operation error: `{0}`")]
    CryptographicError(String),
    #[error("Cannot parse CipherSeed from the provided vector, it is of the incorrect length")]
    InvalidData,
    #[error("CipherSeed CRC32 validation failed")]
    CrcError,
    #[error("Invalid CipherSeed version")]
    VersionMismatch,
    #[error("Decrypted data failed Version or MAC validation")]
    DecryptionFailed,
    #[error("The requested fixed slice length exceeds the available slice length")]
    SliceError(#[from] SliceError),
    #[error("Key ID not valid")]
    InvalidKeyID,
}

#[derive(Debug, Error, PartialEq)]
pub enum MnemonicError {
    #[error(
        "Only ChineseSimplified, ChineseTraditional, English, French, Italian, Japanese, Korean and Spanish are \
         defined natural languages"
    )]
    UnknownLanguage,
    #[error("Word not found: `{0}`")]
    WordNotFound(String),
    #[error("A mnemonic word does not exist for the requested index")]
    IndexOutOfBounds,
    #[error("A problem encountered constructing a secret key from bytes or mnemonic sequence: `{0}`")]
    ByteArrayError(#[from] ByteArrayError),
    #[error("Encoding a mnemonic sequence to bytes requires exactly 24 mnemonic words")]
    EncodeInvalidLength,
    #[error("Bits to integer conversion error")]
    BitsToIntConversion,
    #[error("Integer to bits conversion error")]
    IntToBitsConversion,
}
