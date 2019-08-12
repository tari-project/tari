// Copyright 2019. The Tari Project
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

use crate::{error::MerkleMountainRangeError, Hash};

/// A trait describing generic array-like behaviour, without imposing any specific details on how this is actually done.
pub trait ArrayLike {
    type Value;
    type Error: std::error::Error;

    /// Returns the number of hashes stored in the backend
    fn len(&self) -> usize;

    /// Store a new hash and return the index of the stored item
    fn push(&mut self, hash: Self::Value) -> Result<usize, Self::Error>;

    /// Return the hash at the given index
    fn get(&self, index: usize) -> Option<&Self::Value>;

    /// Return the hash at the given index. Use this if you *know* that the index is valid. Requesting a hash for an
    /// invalid index may cause the a panic
    fn get_or_panic(&self, index: usize) -> &Self::Value;
}

/// A [`std::vec::Vector`]-based backend for Merkle Mountain ranges. Useful for testing or ephemeral data structures
pub struct VectorBackend {
    hashes: Vec<Hash>,
}

impl VectorBackend {
    pub fn new(hashes: Vec<Hash>) -> VectorBackend {
        VectorBackend { hashes }
    }
}

impl Default for VectorBackend {
    fn default() -> VectorBackend {
        VectorBackend {
            hashes: Vec::with_capacity(512),
        }
    }
}

impl ArrayLike for VectorBackend {
    type Error = MerkleMountainRangeError;
    type Value = Hash;

    #[inline(always)]
    fn len(&self) -> usize {
        self.hashes.len()
    }

    fn push(&mut self, hash: Hash) -> Result<usize, MerkleMountainRangeError> {
        self.hashes.push(hash);
        Ok(self.len() - 1)
    }

    fn get(&self, index: usize) -> Option<&Hash> {
        self.hashes.get(index)
    }

    fn get_or_panic(&self, index: usize) -> &Vec<u8> {
        &self.hashes[index]
    }
}
