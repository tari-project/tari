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

use std::{
    fmt,
    fmt::Formatter,
    hash::{Hash, Hasher},
};

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
pub struct TxId(u64);

impl TxId {
    pub fn new_random() -> Self {
        TxId(OsRng.next_u64())
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns a cast to i64. This number may be negative.
    /// Although this is usually a bad idea, in this case TxId is never used in calculations and
    /// the data within TxId is not lost when converting to i64.
    ///
    /// Use this function to say explicitly that this is acceptable.
    ///
    /// ```rust
    /// let a = u64::MAX;
    /// let b = a as i64; // -1
    /// assert_eq!(a, b as u64);
    /// ```
    #[allow(clippy::cast_possible_wrap)]
    pub fn as_i64_wrapped(self) -> i64 {
        self.0 as i64
    }
}

impl Hash for TxId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl PartialEq for TxId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<u64> for TxId {
    fn eq(&self, other: &u64) -> bool {
        self.0.eq(other)
    }
}

impl PartialEq<TxId> for u64 {
    fn eq(&self, other: &TxId) -> bool {
        self.eq(&other.0)
    }
}

impl Eq for TxId {}

impl From<u64> for TxId {
    fn from(s: u64) -> Self {
        Self(s)
    }
}

impl From<usize> for TxId {
    fn from(s: usize) -> Self {
        Self(s as u64)
    }
}

impl From<TxId> for u64 {
    fn from(s: TxId) -> Self {
        s.0
    }
}

impl fmt::Display for TxId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
