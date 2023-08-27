//  Copyright 2022, The Taiji Project
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

use std::{
    fmt::{Display, Formatter},
    ops::{Deref, DerefMut},
};

use lmdb_zero::traits::AsLmdbBytes;
use tari_utilities::hex::to_hex;

use crate::chain_storage::ChainStorageError;

#[derive(Debug, Clone)]
pub(super) struct CompositeKey<const L: usize> {
    bytes: Box<[u8; L]>,
    len: usize,
}

impl<const L: usize> CompositeKey<L> {
    pub fn new() -> Self {
        Self {
            bytes: Self::new_buf(),
            len: 0,
        }
    }

    pub fn try_from_parts<T: AsRef<[u8]>>(parts: &[T]) -> Result<Self, ChainStorageError> {
        let mut key = Self::new();
        for part in parts {
            if !key.push(part) {
                return Err(ChainStorageError::CompositeKeyLengthExceeded);
            }
        }
        Ok(key)
    }

    pub fn push<T: AsRef<[u8]>>(&mut self, bytes: T) -> bool {
        let b = bytes.as_ref();
        let new_len = self.len + b.len();
        if new_len > L {
            return false;
        }
        self.bytes[self.len..new_len].copy_from_slice(b);
        self.len = new_len;
        true
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes[..self.len]
    }

    /// Returns a fixed 0-filled byte array.
    fn new_buf() -> Box<[u8; L]> {
        Box::new([0x0u8; L])
    }
}

impl<const L: usize> Display for CompositeKey<L> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", to_hex(self.as_bytes()))
    }
}

impl<const L: usize> Deref for CompositeKey<L> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl<const L: usize> DerefMut for CompositeKey<L> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_bytes_mut()
    }
}

impl<const L: usize> AsRef<[u8]> for CompositeKey<L> {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<const L: usize> AsLmdbBytes for CompositeKey<L> {
    fn as_lmdb_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}
