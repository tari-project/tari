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
use std::ops::{Index, IndexMut};

/// This is a wrapper function for a vec to make working with the proof of a merklemountain range more efficient
/// This will hold a proof. Every value that can be calculated will or that needs to be checked will be none inside.
/// The data in the merkleproof will be created in form of the Lchild-Rchild-parent(Lchild)-Rchild-parent-..
/// This pattern will be repeated until the parent is the root of the MMR
#[derive(PartialEq, Debug)]
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

    /// Adds a new hash or empty value that needs to be completed at the given point
    pub fn insert(&mut self, index: usize, new_value: Option<Vec<u8>>) {
        self.hash.insert(index, new_value);
    }

    /// Gets a referance to the value at the provided index
    pub fn get(&self, index: usize) -> Option<&Option<Vec<u8>>> {
        if index > self.hash.len() - 1 {
            return None;
        }
        Some(&self.hash[index])
    }

    /// This function will return the length of the merkleroot
    pub fn len(&self) -> usize {
        self.hash.len()
    }

    /// This will verify the merkleproof
    /// This function will fill in the provided proof
    pub fn verify<D>(&mut self) -> bool
    where D: Digest {
        let mut hasher = D::new();
        if (self.hash.len() < 3) || (self.hash[0].is_none()) || (self.hash[1].is_none()) {
            return false;
        };
        let mut i = 2;
        let hash_count = self.hash.len() - 1;
        while i <= hash_count {
            if self.hash[i - 2].is_none() && self.hash[i - 1].is_none() {
                // if this happend something went wrong or the provided proof is broken
                return false;
            }
            hasher.input(&self.hash[i - 2].clone().unwrap());
            hasher.input(&self.hash[i - 1].clone().unwrap());
            let result = hasher.result_reset().to_vec();
            if i == hash_count {
                if self.hash[i].is_some() && self.hash[i].clone().unwrap() == result {
                    return true;
                }
                return false;
            }
            if self.hash[i].is_none() {
                self.hash[i] = Some(result);
            } else if (i + 1 <= hash_count) && (self.hash[i + 1].is_none()) {
                self.hash[i + 1] = Some(result);
            }
            i += 2;
        }
        false
    }

    /// This will verify the merkleproof
    /// This function will reset in the provided proof after verification
    pub fn verify_reset<D>(&mut self) -> bool
    where D: Digest {
        let mut hasher = D::new();
        let reset = self.hash.clone();
        if (self.hash.len() < 3) || (self.hash[0].is_none()) || (self.hash[1].is_none()) {
            return false;
        };
        let mut i = 2;
        let hash_count = self.hash.len() - 1;
        while i <= hash_count {
            if self.hash[i - 2].is_none() && self.hash[i - 1].is_none() {
                // if this happend something went wrong or the provided proof is broken
                self.hash = reset;
                return false;
            }
            hasher.input(&self.hash[i - 2].clone().unwrap());
            hasher.input(&self.hash[i - 1].clone().unwrap());
            let result = hasher.result_reset().to_vec();
            if i == hash_count {
                if self.hash[i].is_some() && self.hash[i].clone().unwrap() == result {
                    self.hash = reset;
                    return true;
                }
                self.hash = reset;
                return false;
            }
            if self.hash[i].is_none() {
                self.hash[i] = Some(result);
            } else if (i + 1 <= hash_count) && (self.hash[i + 1].is_none()) {
                self.hash[i + 1] = Some(result);
            }
            i += 2;
        }
        self.hash = reset;
        false
    }

    /// This will compare and validate the provided proof ensuring that they are the same.
    /// Both merkleproofs needs to be longer than 2 hashes
    /// This function will fill in the provided proof
    pub fn compare<D>(&mut self, merkleproof: &MerkleProof) -> bool
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

    /// This will compare and validate the provided proof ensuring that they are the same.
    /// Both merkleproofs needs to be longer than 2 hashes
    /// This function reset the proof after compare
    pub fn compare_reset<D>(&mut self, merkleproof: &MerkleProof) -> bool
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
        self.verify_reset::<D>()
    }

    /// This function will search if the provided hash in contained in the proof
    /// It will reset the proof to before the verification
    pub fn verify_proof_reset<D>(&mut self, hash: &Vec<u8>) -> bool
    where D: Digest {
        let reset = self.hash.clone();
        if self.hash.len() < 3 {
            // our merkle proof is not a valid tree
            return false;
        }
        // verify root
        let verification_result = self.verify::<D>();
        if !verification_result {
            self.hash = reset;
            return false;
        };
        for i in 0..(self.hash.len() - 1) {
            if self.hash[i].is_some() && self.hash[i].clone().unwrap() == *hash {
                self.hash = reset;
                return true;
            }
        }
        self.hash = reset;
        false
    }

    /// This function will search if the provided hash in contained in the proof
    /// This function will fill in the proof
    pub fn verify_proof<D>(&mut self, hash: &Vec<u8>) -> bool
    where D: Digest {
        if self.hash.len() < 3 {
            // our merkle proof is not a valid tree
            return false;
        }
        // verify root
        let verification_result = self.verify::<D>();
        if !verification_result {
            return false;
        };
        for i in 0..(self.hash.len() - 1) {
            if self.hash[i].is_some() && self.hash[i].clone().unwrap() == *hash {
                return true;
            }
        }
        false
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

impl Index<usize> for MerkleProof {
    type Output = Option<Vec<u8>>;

    fn index(&self, index: usize) -> &Option<Vec<u8>> {
        &self.hash[index]
    }
}

impl IndexMut<usize> for MerkleProof {
    fn index_mut<'a>(&'a mut self, index: usize) -> &'a mut Option<Vec<u8>> {
        &mut self.hash[index]
    }
}
