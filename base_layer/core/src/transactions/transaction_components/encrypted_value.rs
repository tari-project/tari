// Copyright 2022 The Tari Project
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

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use tari_utilities::{ByteArray, ByteArrayError};

use crate::consensus::{ConsensusDecoding, ConsensusEncoding};

const SIZE: usize = 24;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EncryptedValue {
    /// value: u64 + tag: [u8; 16]
    pub data: [u8; SIZE],
}

impl Default for EncryptedValue {
    fn default() -> Self {
        Self { data: [0; SIZE] }
    }
}

impl ByteArray for EncryptedValue {
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        if bytes.len() == SIZE {
            let mut this = Self::default();
            this.data.copy_from_slice(bytes);
            Ok(this)
        } else {
            Err(ByteArrayError::IncorrectLength)
        }
    }

    fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

impl EncryptedValue {
    /// TODO: Replace this method with a real call of encryption service
    /// that will produce an encrypted value from the given `amount`.
    pub fn todo_encrypt_from(amount: impl Into<u64>) -> Self {
        let mut data: [u8; SIZE] = [0; SIZE];
        let value = amount.into().to_le_bytes();
        data[0..8].copy_from_slice(&value);
        Self { data }
    }
}

impl ConsensusEncoding for EncryptedValue {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.data.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusDecoding for EncryptedValue {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let data = <[u8; 24]>::consensus_decode(reader)?;
        Ok(Self { data })
    }
}
