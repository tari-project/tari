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

use std::{
    fmt::{Display, Formatter},
    ops::{Deref, DerefMut},
};

use tari_utilities::hex::to_hex;

#[derive(Debug, Clone, Copy)]
pub(super) struct CompositeKey<const KEY_LEN: usize> {
    bytes: [u8; KEY_LEN],
    len: usize,
}

impl<const KEY_LEN: usize> CompositeKey<KEY_LEN> {
    pub fn new() -> Self {
        Self {
            bytes: Self::new_buf(),
            len: 0,
        }
    }

    pub fn push<T: AsRef<[u8]>>(&mut self, bytes: T) -> bool {
        let b = bytes.as_ref();
        let new_len = self.len + b.len();
        if new_len > KEY_LEN {
            return false;
        }
        self.bytes[self.len..new_len].copy_from_slice(b);
        self.len = new_len;
        true
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes[..self.len]
    }

    /// Returns a fixed 0-filled byte array.
    const fn new_buf() -> [u8; KEY_LEN] {
        [0x0u8; KEY_LEN]
    }
}

impl<const KEY_LEN: usize> Display for CompositeKey<KEY_LEN> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", to_hex(self.as_bytes()))
    }
}

impl<const KEY_LEN: usize> Deref for CompositeKey<KEY_LEN> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl<const KEY_LEN: usize> DerefMut for CompositeKey<KEY_LEN> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_bytes_mut()
    }
}

impl<const KEY_LEN: usize> AsRef<[u8]> for CompositeKey<KEY_LEN> {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}
