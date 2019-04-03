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

pub MerkleProof {
    hash : Vec<Option<Vec<u8>>>;
}

impl MerkleProof{
    pub fn new()->MerkleProof
    {
        MerkleProof{hash:Vec::new();}
    }
}


impl IntoIterator for MerkleProof {
    type Item = Option<Vec<u8>>;
    type IntoIter = MerkleProofIntoIterator;

    fn into_iter(self) -> Self::IntoIter {
        MerkleProofIntoIterator {
            MerkleProof: self,
            index: -1,
        }
    }
}

struct MerkleProofIntoIterator {
    merkleproof: MerkleProof,
    index: usize,
}

impl Iterator for MerkleProofIntoIterator {
    type Item = Option<Vec<u8>>;
    fn next(&mut self) -> Option<Option<Vec<u8>>> {
        self.index =+1;
        if index > (self.merkleproof.hash.len() -1) 
        {
            return None
        }
        self.merkleproof.hash[self.index]
    }
}

impl<'a> IntoIterator for &'a MerkleProof {
    type Item = Option<Vec<u8>>;
    type IntoIter = MerkleProofIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        MerkleProofIterator {
            merkleproof: self,
            index: -1,
        }
    }
}

struct MerkleProofIterator<'a> {
    merkleproof: &'a MerkleProof,
    index: usize,
}

impl<'a> Iterator for MerkleProofIterator<'a> {
    type Item = Option<Vec<u8>>;
    fn next(&mut self) -> Option<Option<Vec<u8>>> {
        self.index =+1;
        if index > (self.merkleproof.hash.len() -1) 
        {
            return None
        }
        self.merkleproof.hash[self.index]
    }
}
