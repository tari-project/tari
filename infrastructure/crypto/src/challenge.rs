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

use digest::{generic_array::typenum::U32, FixedOutput};
use sha2::Digest;

pub type Challenge256Bit = [u8; 32];

/// A Challenge of the form H(P || R || ... || m).
/// Challenges are often used in constructing signatures. Given some hash function H, the value is a
/// scalar hand can be used as a secret key.
/// `Challenge` is really just a thin wrapper around Digest, but we make use of Rust's typing system to prevent type
/// mixing
/// ## Usage
///
/// Challenge makes use of a fluent interface to build up the challenge parts:
///
/// ```edition2018
///     use crypto::challenge::*;
///     use sha2::Sha256;
///
///     let challenge = Challenge::<Sha256>::new();
///     challenge
///         .concat(b"The colour of magic")
///         .concat(b"The light fantastic")
///         .hash();
/// ```
#[derive(Clone, Copy)]
pub struct Challenge<D: Digest> {
    hasher: D,
}

#[allow(non_snake_case)]
impl<D: Digest> Challenge<D> {
    /// Create a new challenge instance with the [Digest](trait.Digest.html) provided
    pub fn new() -> Challenge<D> {
        let hasher = D::new();
        Challenge { hasher }
    }

    /// Append a new set of bytes to the hash. `concat` returns the `Challenge` instance so that you can easily chain
    /// concatenation calls together.
    pub fn concat(mut self, value: &[u8]) -> Self {
        self.hasher.input(value);
        self
    }

    /// Hash the challenge input, consuming the challenge in the process.
    pub fn hash(self) -> Vec<u8> {
        self.hasher.result().to_vec()
    }
}

impl<D> From<Challenge<D>> for Challenge256Bit
where D: Digest + FixedOutput<OutputSize = U32>
{
    fn from(challenge: Challenge<D>) -> Challenge256Bit {
        let mut v = [0u8; 32];
        let h = challenge.hash();
        v.copy_from_slice(&h[0..32]);
        v as Challenge256Bit
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::common::ByteArray;
    use blake2::Blake2b;
    use sha2::Sha256;

    #[test]
    fn hash_with_sha256() {
        let e = Challenge::<Sha256>::new();
        let result = e.concat(b"Hi").concat(b"World").hash();
        assert_eq!(result.to_hex(), "f1007761429621683d6f843fdc3d0de3c8c02497f38cf73789cb9e41ce49fa6e");
    }

    #[test]
    fn hash_with_blake() {
        let e = Challenge::<Blake2b>::new();
        let result = e.concat("Now is the winter".as_bytes()).concat("of our discontent".as_bytes()).hash();
        assert_eq!(result.to_hex(),
                   "521143c1e862cd458164c5c48ffa354ada324ff4f20b830a5c98de205ed0c8b8b49170101a209386608fc1bc5715f6c536b4a5a74d65a02c609b80231d3d72bd");
    }
}
