// Copyright 2021 The Grin Developers
// Modifications copyright 2025 The Tari Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// This file was modified from its original form:
// https://github.com/mimblewimble/grin/blob/master/core/src/pow/siphash.rs

//! Simple implementation of the siphash 2-4 hashing function from
//! Jean-Philippe Aumasson and Daniel J. Bernstein.

// Parameters to the siphash block algorithm. Used by Cuckaroo but can be
// seen as a generic way to derive a hash within a block of them.
const SIPHASH_BLOCK_BITS: u64 = 6;
const SIPHASH_BLOCK_SIZE: usize = 1 << SIPHASH_BLOCK_BITS;
const SIPHASH_BLOCK_MASK: u64 = SIPHASH_BLOCK_SIZE as u64 - 1;

/// Utility function to compute a single siphash 2-4 based on a seed and
/// a nonce
pub fn siphash24(v: &[u64; 4], nonce: u64) -> u64 {
    let mut siphash = SipHash24::new(v);
    siphash.hash(nonce, 21); // 21 is standard rotation constant
    siphash.digest()
}

/// Builds a block of siphash values by repeatedly hashing from the nonce
/// truncated to its closest block start, up to the end of the block. Returns
/// the resulting hash at the nonce's position.
pub fn siphash_block(v: &[u64; 4], nonce: u64, rot_e: u8, xor_all: bool) -> u64 {
    // beginning of the block of hashes
    let edge0 = nonce & !SIPHASH_BLOCK_MASK;
    // let nonce_i = nonce & SIPHASH_BLOCK_MASK;
    let mut nonce_hash = vec![0u64; SIPHASH_BLOCK_SIZE as usize];

    // repeated hashing over the whole block
    let mut siphash = SipHash24::new(v);
    for i in 0..SIPHASH_BLOCK_SIZE {
        siphash.hash(edge0 + i as u64, rot_e);
        nonce_hash[i] = siphash.digest();
    }
    let last = nonce_hash[SIPHASH_BLOCK_SIZE - 1];

    // lolminer

    let mut siphashb = SipHash24::new(v);
    let last_nonce = nonce & SIPHASH_BLOCK_MASK;
    for i in 0..=last_nonce {
        siphashb.hash(edge0 + i, rot_e);
    }
    let b_hash = siphashb.digest();
    let final_edge = b_hash ^ last;
    final_edge

    // original cuckaroo in tromps's c
    // .....
    // for i in 0..SIPHASH_BLOCK_MASK {
    // nonce_hash[i as usize] ^= last;
    // }
    // return nonce_hash[nonce as usize & SIPHASH_BLOCK_MASK as usize];

    // Grin original code
    // .....

    // xor the hash at nonce_i < SIPHASH_BLOCK_MASK with some or all later hashes to force hashing the whole block
    // let mut xor: u64 = *nonce_hash.get(nonce_i as usize).expect("Already checked");
    // let xor_from = if xor_all || nonce_i == SIPHASH_BLOCK_MASK {
    //     nonce_i + 1
    // } else {
    //     SIPHASH_BLOCK_MASK
    // };
    // for i in xor_from..SIPHASH_BLOCK_SIZE {
    //     #[allow(clippy::cast_possible_truncation)]
    //     let i_usize = i as usize;
    //     xor ^= *nonce_hash.get(i_usize).expect("Already checked");
    // }
    // xor
}

/// Implements siphash 2-4 specialized for a 4 u64 array key and a u64 nonce
/// that can be used for a single or multiple repeated hashing.
///
/// The siphash structure is represented by a vector of four 64-bits words
/// that we simply reference by their position. A hashing round consists of
/// a series of arithmetic operations on those words, while the resulting
/// hash digest is an xor of xor on them.
///
/// Note that this implementation is only secure if it's already fed words
/// output from a previous hash function (in our case blake2).
pub struct SipHash24(u64, u64, u64, u64);

impl SipHash24 {
    /// Create a new siphash context
    pub fn new(v: &[u64; 4]) -> SipHash24 {
        SipHash24(v[0], v[1], v[2], v[3])
    }

    /// One siphash24 hashing, consisting of 2 and then 4 rounds
    pub fn hash(&mut self, nonce: u64, rot_e: u8) {
        self.3 ^= nonce;
        self.round(rot_e);
        self.round(rot_e);

        self.0 ^= nonce;
        self.2 ^= 0xff;

        for _ in 0..4 {
            self.round(rot_e);
        }
    }

    /// Resulting hash digest
    pub fn digest(&self) -> u64 {
        (self.0 ^ self.1) ^ (self.2 ^ self.3)
    }

    fn round(&mut self, rot_e: u8) {
        self.0 = self.0.wrapping_add(self.1);
        self.2 = self.2.wrapping_add(self.3);
        self.1 = self.1.rotate_left(13);
        self.3 = self.3.rotate_left(16);
        self.1 ^= self.0;
        self.3 ^= self.2;
        self.0 = self.0.rotate_left(32);
        self.2 = self.2.wrapping_add(self.1);
        self.0 = self.0.wrapping_add(self.3);
        self.1 = self.1.rotate_left(17);
        self.3 = self.3.rotate_left(rot_e.into());
        self.1 ^= self.2;
        self.3 ^= self.0;
        self.2 = self.2.rotate_left(32);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn hash_some() {
        assert_eq!(siphash24(&[1, 2, 3, 4], 10), 928382149599306901);
        assert_eq!(siphash24(&[1, 2, 3, 4], 111), 10524991083049122233);
        assert_eq!(siphash24(&[9, 7, 6, 7], 12), 1305683875471634734);
        assert_eq!(siphash24(&[9, 7, 6, 7], 10), 11589833042187638814);
    }

    #[test]
    fn hash_block() {
        assert_eq!(siphash_block(&[1, 2, 3, 4], 10, 21, false), 1182162244994096396);
        assert_eq!(siphash_block(&[1, 2, 3, 4], 123, 21, false), 11303676240481718781);
        assert_eq!(siphash_block(&[9, 7, 6, 7], 12, 21, false), 4886136884237259030);
    }
}
