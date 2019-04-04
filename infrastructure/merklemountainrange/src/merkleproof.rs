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

use digest::Digest;

/// This is a wrapper function to make working with the proof of a merklemountain range more efficient
/// This will hold a proof. Every value that can be calculated will or that needs to be checked will be none inside.
pub struct MerkleProof {
    hash: Vec<Option<Vec<u8>>>,
}

impl MerkleProof {
    /// Creates a new empty merkleproof
    pub fn new() -> MerkleProof {
        MerkleProof { hash: Vec::new() }
    }

    /// Adds a new hash or empty value that needs to be completed
    pub fn push(&mut self, new_value: Option<Vec<u8>>) {
        self.hash.push(new_value);
    }

    /// Gets a referance to the value at the provided index
    pub fn get(&self, index: usize) -> Option<&Option<Vec<u8>>> {
        if index > self.hash.len() - 1 {
            return None;
        }
        Some(&self.hash[index])
    }

    /// This will verify the merkleproof
    pub fn verify<D>(&self) -> bool
    where D: Digest {
        let mut hashes = Vec::new();
        let mut hasher = D::new();
        if (self.hash.len() < 3) || (self.hash[0].is_none()) || (self.hash[1].is_none()) {
            return false;
        };
        hashes.push(self.hash[0].clone().unwrap());
        hashes.push(self.hash[1].clone().unwrap());
        let mut i = 2;
        let hash_count = self.hash.len() - 1;
        while i <= hash_count {
            hasher.input(&hashes[i - 2]);
            hasher.input(&hashes[i - 1]);
            let result = hasher.result_reset().to_vec();
            if i == hash_count {
                if self.hash[i].is_some() && self.hash[1].clone().unwrap() == result {
                    return true;
                }
                return false;
            }
            if self.hash[i].is_none() {
                hashes.push(result)
            } else if (i + 1 <= hash_count) && (self.hash[i + 1].is_none()) {
                hashes.push(self.hash[i].clone().unwrap());
                hashes.push(result);
            }
            i += 2;
        }
        false
    }

    /// This will compare and validate the provided proof ensuring that they are the same.
    /// Both merkleproofs needs to be longer than 2 hashes
    pub fn compare<D>(&self, merkleproof: MerkleProof) -> bool
    where D: Digest {
        if (self.hash.len() != merkleproof.hash.len()) && self.hash.len() < 3 {
            return false;
        }
        if self.hash[0] != merkleproof.hash[0] ||
            self.hash[1] != merkleproof.hash[1] ||
            self.hash[self.hash.len() - 1] != merkleproof.hash[merkleproof.hash.len() - 1]
        {
            // if the begin and ends dont match the merkle proof is invalid
            // we only really care that the first 2 values are correct and that the merkleroot is correctly calculated
            return false;
        }
        // verify root
        self.verify::<D>()
    }
}

impl IntoIterator for MerkleProof {
    type IntoIter = MerkleProofIntoIterator;
    type Item = Option<Vec<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        MerkleProofIntoIterator {
            merkleproof: self,
            index: 0,
        }
    }
}

pub struct MerkleProofIntoIterator {
    merkleproof: MerkleProof,
    index: usize,
}

impl Iterator for MerkleProofIntoIterator {
    type Item = Option<Vec<u8>>;

    fn next(&mut self) -> Option<Option<Vec<u8>>> {
        if self.index > (self.merkleproof.hash.len() - 1) {
            return None;
        }
        self.index += 1;
        Some(self.merkleproof.hash[self.index - 1].clone())
    }
}

impl<'a> IntoIterator for &'a MerkleProof {
    type IntoIter = MerkleProofIterator<'a>;
    type Item = Option<Vec<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        MerkleProofIterator {
            merkleproof: self,
            index: 0,
        }
    }
}

pub struct MerkleProofIterator<'a> {
    merkleproof: &'a MerkleProof,
    index: usize,
}

impl<'a> Iterator for MerkleProofIterator<'a> {
    type Item = Option<Vec<u8>>;

    fn next(&mut self) -> Option<Option<Vec<u8>>> {
        if self.index > (self.merkleproof.hash.len() - 1) {
            return None;
        }
        self.index += 1;
        Some(self.merkleproof.hash[self.index - 1].clone())
    }
}
