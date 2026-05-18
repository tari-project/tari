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

use blake2::Blake2b;
use digest::consts::U32;
use serde::{Deserialize, Serialize};
use tari_crypto::hashing::Mac;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
pub struct TxId(u64);

impl TxId {
    /// Create a new random TxId. Only for temporary use.
    pub fn new_random() -> Self {
        use rand::Rng;
        TxId(rand::rng().next_u64())
    }

    /// Create a new TxId deterministically from the given 32-byte output hash and MAC key.
    pub fn new_deterministic(view_key: &[u8], output_hash: &[u8; 32]) -> Self {
        let hash = Mac::<Blake2b<U32>>::generate(view_key, output_hash, "tari/tx_id_64");
        let hash = hash.as_ref();

        let mut buffer = [0u8; 8];
        buffer.copy_from_slice(hash.get(..8).expect("we have 8 bytes"));
        TxId(u64::from_le_bytes(buffer))
    }

    /// Returns the inner u64 value.
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

#[cfg(test)]
mod tests {
    use crate::tx_id::TxId;

    fn bytes32_inc(start: u8) -> [u8; 32] {
        let mut a = [0u8; 32];
        for (i, v) in a.iter_mut().enumerate() {
            *v = start.wrapping_add(u8::try_from(i).unwrap());
        }
        a
    }

    #[test]
    fn it_gives_deterministic_tx_ids() {
        let view_key_1 = b"example-view-key-32bytes-len----"; // 32 bytes

        let hash_1 = bytes32_inc(0x10);

        let id1 = TxId::new_deterministic(view_key_1, &hash_1);
        let id2 = TxId::new_deterministic(view_key_1, &hash_1);
        assert_eq!(id1, id2, "same inputs must produce same tx_id");

        let hash_2 = bytes32_inc(0x11);

        let id3 = TxId::new_deterministic(view_key_1, &hash_2);
        let id4 = TxId::new_deterministic(view_key_1, &hash_2);
        assert_eq!(id3, id4, "same inputs must produce same tx_id");

        assert_ne!(id1, id3, "different inputs must produce different tx_ids");

        let view_key_2 = b"example-view-key-32bytes-len---2"; // 32 bytes
        let id5 = TxId::new_deterministic(view_key_2, &hash_2);

        let view_key_3 = b"example-view-key-32bytes-len---3"; // 32 bytes
        let id6 = TxId::new_deterministic(view_key_3, &hash_2);

        assert_ne!(
            id5, id6,
            "the same hash input with a different mac key must produce different tx_ids"
        );
    }
}
