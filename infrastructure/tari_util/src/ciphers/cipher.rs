// Copyright 2019 The Tari Project
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

use crate::{ByteArray, ByteArrayError};
use derive_error::Error;

#[derive(Debug, Error, PartialEq)]
pub enum CipherError {
    /// Provided key is the incorrect size to be used by the Cipher
    KeyLengthError,
    /// Provided Nonce is the incorrect size to be used by the Cipher
    NonceLengthError,
    /// No data was provided for encryption/decryption
    NoDataError,
    /// Byte Array conversion error
    ByteArrayError(ByteArrayError),
}

/// A trait describing an interface to a symmetrical encryption scheme
pub trait Cipher<D>
where D: ByteArray
{
    /// Encrypt using a cipher and provided key and nonce
    fn seal(plain_text: &D, key: &[u8], nonce: &[u8]) -> Result<Vec<u8>, CipherError>;

    /// Decrypt using a cipher and provided key and nonce
    fn open(cipher_text: &[u8], key: &[u8], nonce: &[u8]) -> Result<D, CipherError>;

    /// Encrypt using a cipher and provided key, the nonce will be generate internally and appended to the cipher text
    fn seal_with_integral_nonce(plain_text: &D, key: &[u8]) -> Result<Vec<u8>, CipherError>;

    /// Decrypt using a cipher and provided key. The integral nonce will be read from the cipher text
    fn open_with_integral_nonce(cipher_text: &[u8], key: &[u8]) -> Result<D, CipherError>;
}
