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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crate::{
    backend::ArrayLike,
    common::{family, family_branch, find_peaks, hash_together, is_leaf, is_left_sibling, leaf_index},
    error::MerkleMountainRangeError,
    serde_support,
    Hash,
    HashSlice,
    MerkleMountainRange,
};
use derive_error::Error;
use digest::Digest;
use log::error;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use tari_utilities::hex::Hex;

/// Merkle proof errors.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum MerkleProofError {
    // Merkle proof root hash does not match when attempting to verify.
    RootMismatch,
    // You tried to construct or verify a Merkle proof using a non-leaf node as the inclusion candidate
    NonLeafNode,
    // There was no hash in the merkle tree backend with the given position
    #[error(non_std, no_from)]
    HashNotFound(usize),
    // The list of peak hashes provided in the proof has an error
    IncorrectPeakMap,
    // Unexpected
    Unexpected,
    MerkleMountainRangeError(MerkleMountainRangeError),
}

/// A Merkle proof that proves a particular element at a particular position exists in an MMR.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, PartialOrd, Ord)]
pub struct MerkleProof {
    /// The size of the MMR at the time the proof was created.
    mmr_size: usize,
    /// The sibling path from the leaf up to the final sibling hashing to the local root.
    #[serde(with = "serde_support::hash")]
    path: Vec<Hash>,
    /// The set of MMR peaks, not including the local peak for the candidate node
    #[serde(with = "serde_support::hash")]
    peaks: Vec<Hash>,
}

impl Default for MerkleProof {
    fn default() -> MerkleProof {
        MerkleProof {
            mmr_size: 0,
            path: Vec::default(),
            peaks: Vec::default(),
        }
    }
}

impl MerkleProof {
    /// Build a Merkle Proof the given MMR at the given *leaf* position. This is usually the version you'll want to
    /// call, since you'll know the leaf index more often than the MMR index.
    ///
    /// For the difference between leaf node and MMR node indices, see the [mod level](:tari_mmr) documentation.
    ///
    /// See [MerkleProof::for_node] for more details on how the proof is constructed.
    pub fn for_leaf_node<D, B>(
        mmr: &MerkleMountainRange<D, B>,
        leaf_pos: usize,
    ) -> Result<MerkleProof, MerkleProofError>
    where
        D: Digest,
        B: ArrayLike<Value = Hash>,
    {
        let pos = leaf_index(leaf_pos);
        MerkleProof::generate_proof(mmr, pos)
    }

    /// Build a Merkle proof for the candidate node at the given MMR index. If you want to build a proof using the
    /// leaf position, call [MerkleProof::for_leaf_node] instead. The given node position must be a leaf node,
    /// otherwise a `MerkleProofError::NonLeafNode` error will be returned.
    ///
    /// The proof for the MMR consists of two parts:
    /// a) A list of sibling node hashes starting from the candidate node and walking up the tree to the local root
    /// (i.e. the root of the binary tree that the candidate node lives in.
    /// b) A list of MMR peaks, excluding the local node hash.
    /// The final Merkle proof is constructed by hashing all the peaks together (this is slightly different to how
    /// other MMR implementations work).
    pub fn for_node<D, B>(mmr: &MerkleMountainRange<D, B>, pos: usize) -> Result<MerkleProof, MerkleProofError>
    where
        D: Digest,
        B: ArrayLike<Value = Hash>,
    {
        // check this pos is actually a leaf in the MMR
        if !is_leaf(pos) {
            return Err(MerkleProofError::NonLeafNode);
        }

        MerkleProof::generate_proof(mmr, pos)
    }

    fn generate_proof<D, B>(mmr: &MerkleMountainRange<D, B>, pos: usize) -> Result<MerkleProof, MerkleProofError>
    where
        D: Digest,
        B: ArrayLike<Value = Hash>,
    {
        // check we actually have a hash in the MMR at this pos
        mmr.get_node_hash(pos)?.ok_or(MerkleProofError::HashNotFound(pos))?;
        let mmr_size = mmr.len()?;
        let family_branch = family_branch(pos, mmr_size);

        // Construct a vector of sibling hashes from the candidate node's position to the local peak
        let path = family_branch
            .iter()
            .map(|(_, sibling)| {
                mmr.get_node_hash(*sibling)?
                    .ok_or(MerkleProofError::HashNotFound(*sibling))
            })
            .collect::<Result<_, _>>()?;

        let peak_pos = match family_branch.last() {
            Some(&(parent, _)) => parent,
            None => pos,
        };

        // Get the peaks of the merkle trees, which are bagged together to form the root
        // For the proof, we must leave out the local root for the candidate node
        let peaks = find_peaks(mmr_size);
        let mut peak_hashes = Vec::with_capacity(peaks.len() - 1);
        for peak_index in peaks {
            if peak_index != peak_pos {
                let hash = mmr
                    .get_node_hash(peak_index)?
                    .ok_or(MerkleProofError::HashNotFound(peak_index))?
                    .clone();
                peak_hashes.push(hash);
            }
        }
        Ok(MerkleProof {
            mmr_size,
            path,
            peaks: peak_hashes,
        })
    }

    pub fn verify_leaf<D: Digest>(
        &self,
        root: &HashSlice,
        hash: &HashSlice,
        leaf_pos: usize,
    ) -> Result<(), MerkleProofError>
    {
        let pos = leaf_index(leaf_pos);
        self.verify::<D>(root, hash, pos)
    }

    /// Verifies the Merkle proof against the provided root hash, element and position in the MMR.
    pub fn verify<D: Digest>(&self, root: &HashSlice, hash: &HashSlice, pos: usize) -> Result<(), MerkleProofError> {
        let mut proof = self.clone();
        // calculate the peaks once as these are based on overall MMR size (and will not change)
        let peaks = find_peaks(self.mmr_size);
        proof.verify_consume::<D>(root, hash, pos, &peaks)
    }

    /// Calculate a merkle root from the given hash, its peak position, and the peak hashes given with the proof
    /// Because of how the proofs are generated, the peak hashes given in the proof will always be an array one
    /// shorter then the canonical peak list for an MMR of a given size. e.g.: For an MMR of size 10:
    /// ```text
    ///       6
    ///    2     5    9
    ///   0 1   3 4  7 8
    /// ```
    /// The peak list is (6,9). But if we have an inclusion proof for say, 3, then we'll calculate 6 from the sibling
    /// data, therefore the proof only needs to provide 9.
    ///
    /// After running [verify_consume], we'll know the hash of 6 and it's position (the local root), and so we'll also
    /// know where to insert the hash in the peak list.
    fn check_root<D: Digest>(&self, hash: &HashSlice, pos: usize, peaks: &[usize]) -> Result<Hash, MerkleProofError> {
        // The peak hash list provided in the proof does not include the local peak determined from the candidate
        // node, so len(peak) must be len(self.peaks) + 1.
        if peaks.len() != self.peaks.len() + 1 {
            return Err(MerkleProofError::IncorrectPeakMap);
        }
        let hasher = D::new();
        // We're going to hash the peaks together, but insert the provided hash in the correct position.
        let peak_hashes = self.peaks.iter();
        let (hasher, _) = peaks
            .iter()
            .fold((hasher, peak_hashes), |(hasher, mut peak_hashes), i| {
                if *i == pos {
                    (hasher.chain(hash), peak_hashes)
                } else {
                    let hash = peak_hashes.next().unwrap();
                    (hasher.chain(hash), peak_hashes)
                }
            });
        Ok(hasher.result().to_vec())
    }

    /// Consumes the Merkle proof while verifying it.
    /// This method works by walking up the sibling path given in the proof. Since the only info we're given in the
    /// proof are the sibling hashes and the size of the MMR, there are a lot of bit-twiddling checks to determine
    /// where we are in the MMR.
    ///
    /// This algorithm works as follows:
    /// First we calculate the "local root" of the MMR by getting to the root of the full binary tree indicated by
    /// `pos` and `self.mmr_size`.
    /// This is done by popping a sibling hash off `self.path`, figuring out if it's on the left or right branch,
    /// calculating the parent hash, and then calling `verify_consume` again using the parent hash and position.
    /// Once `self.path` is empty, we have the local root and position, this data is used to hash all the peaks
    /// together in `check_root` to calculate the final merkle root.
    fn verify_consume<D: Digest>(
        &mut self,
        root: &HashSlice,
        hash: &HashSlice,
        pos: usize,
        peaks: &[usize],
    ) -> Result<(), MerkleProofError>
    {
        // If path is empty, we've got the hash of a local peak, so now we need to hash all the peaks together to
        // calculate the merkle root
        if self.path.is_empty() {
            let calculated_root = self.check_root::<D>(hash, pos, peaks)?;
            return if root == calculated_root.as_slice() {
                Ok(())
            } else {
                Err(MerkleProofError::RootMismatch)
            };
        }

        let sibling = self.path.remove(0); // FIXME Compare perf vs using a VecDeque
        let (parent_pos, sibling_pos) = family(pos);
        if parent_pos > self.mmr_size {
            error!(
                "Found edge case. pos: {}, peaks: {:?}, mmr_size: {}, siblings: {:?}, peak_path: {:?}",
                pos, peaks, self.mmr_size, &self.path, &self.peaks
            );
            Err(MerkleProofError::Unexpected)
        } else {
            let parent = if is_left_sibling(sibling_pos) {
                hash_together::<D>(&sibling, hash)
            } else {
                hash_together::<D>(hash, &sibling)
            };
            self.verify_consume::<D>(root, &parent, parent_pos, peaks)
        }
    }
}

impl Display for MerkleProof {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(&format!("MMR Size: {}\n", self.mmr_size))?;
        f.write_str("Siblings:\n")?;
        self.path
            .iter()
            .enumerate()
            .fold(Ok(()), |_, (i, h)| f.write_str(&format!("{:3}: {}\n", i, h.to_hex())))?;
        f.write_str("Peaks:\n")?;
        self.peaks
            .iter()
            .enumerate()
            .fold(Ok(()), |_, (i, h)| f.write_str(&format!("{:3}: {}\n", i, h.to_hex())))?;
        Ok(())
    }
}
