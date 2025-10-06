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
    sync::OnceLock,
};

use blake2::Blake2bMac;
use digest::{consts::U8, FixedOutput, OutputSizeUser, Update};
use serde::{Deserialize, Serialize};

type Blake2bTxIdMac = Blake2bMac<U8>; // 8-byte keyed BLAKE2b
const MAC_SIZE: usize = 8;
const TAG_TX_ID: &str = "tari/tx_id_64";
static TX_ID_MAC: OnceLock<Blake2bTxIdMac> = OnceLock::new();

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
pub struct TxId(u64);

impl TxId {
    /// Initialize the TX_ID_MAC with the given view key. This must be called once before using
    /// TxId::new_deterministic.
    pub fn init_mac(view_key: &[u8]) {
        let _unused = TX_ID_MAC.set(Self::blake2b_tx_id_mac(view_key, TAG_TX_ID));
        debug_assert_eq!(Blake2bTxIdMac::output_size(), MAC_SIZE);
    }

    fn blake2b_tx_id_mac(key: &[u8], domain: &str) -> Blake2bTxIdMac {
        Blake2bTxIdMac::new_with_salt_and_personal(
            key,               // Unique key
            b"",               // Salt is not used
            domain.as_bytes(), // Domain separation string
        )
        .expect("key length ok")
    }

    /// Create a new random TxId. Only for temporary use.
    pub fn new_random() -> Self {
        use rand::{rngs::OsRng, RngCore};
        TxId(OsRng.next_u64())
    }

    /// Create a new TxId deterministically from the given 32-byte output hash.
    pub fn new_deterministic(out_hash32: &[u8; 32]) -> Self {
        let mac_result = TX_ID_MAC.get();
        debug_assert!(
            mac_result.is_some(),
            "MAC should be initialized - call TxId::init_mac(&view_key)"
        );
        let mut mac = mac_result
            .expect("MAC should be initialized - call TxId::init_mac(&view_key)")
            .clone();
        mac.update(out_hash32);
        let out = mac.finalize_fixed();
        let mut buffer = [0u8; MAC_SIZE];
        buffer.copy_from_slice(&out);
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
        let view_key = b"example-view-key-32bytes-len----"; // 32 bytes
        TxId::init_mac(view_key);

        let hash = bytes32_inc(0x10);

        let id1 = TxId::new_deterministic(&hash);
        let id2 = TxId::new_deterministic(&hash);
        assert_eq!(id1, id2, "same inputs must produce same tx_id");

        let hash = bytes32_inc(0x11);

        let id3 = TxId::new_deterministic(&hash);
        let id4 = TxId::new_deterministic(&hash);
        assert_eq!(id3, id4, "same inputs must produce same tx_id");

        assert_ne!(id1, id3, "different inputs must produce different tx_ids");
    }
}
