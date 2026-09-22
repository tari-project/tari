//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Port of monero's tree hash algorithm
//!
//! See <https://github.com/monero-project/monero/blob/master/src/crypto/tree-hash.c>

use std::{io, io::Write};

use borsh::{BorshDeserialize, BorshSerialize};
use integer_encoding::{VarIntReader, VarIntWriter};
use monero::{
    Hash,
    consensus::{Decodable, Encodable},
};

use crate::proof_of_work::monero_rx::error::MergeMineError;

// Binary tree of depth 32 means u32::MAX tree, this is more than large enough to support most trees.
const MAX_MERKLE_TREE_PROOF_SIZE: usize = 32;

/// Returns the Keccak 256 hash of the byte input
fn cn_fast_hash(data: &[u8]) -> Hash {
    Hash::new(data)
}

/// Returns the Keccak 256 hash of 2 hashes
fn cn_fast_hash2(hash1: &Hash, hash2: &Hash) -> Hash {
    let mut tmp = [0u8; 64];
    tmp[..32].copy_from_slice(hash1.as_bytes());
    tmp[32..].copy_from_slice(hash2.as_bytes());
    cn_fast_hash(&tmp)
}

/// Round down to power of two. Will return an error for count < 3 or if the count is unreasonably large for tree hash
/// calculations.
fn tree_hash_count(count: usize) -> Result<usize, MergeMineError> {
    if count < 3 {
        return Err(MergeMineError::HashingError(format!(
            "Cannot calculate tree hash root. Expected count to be greater than 3 but got {count}"
        )));
    }

    if count > 0x10000000 {
        return Err(MergeMineError::HashingError(format!(
            "Cannot calculate tree hash root. Expected count to be less than 0x10000000 but got {count}",
        )));
    }

    // Essentially we are doing 1 << floor(log2(count))
    let mut pow: usize = 2;
    while pow < count {
        pow <<= 1;
    }

    Ok(pow >> 1)
}

/// Tree hash algorithm in monero
pub fn tree_hash(hashes: &[Hash]) -> Result<Hash, MergeMineError> {
    if hashes.is_empty() {
        return Err(MergeMineError::HashingError(
            "Cannot calculate merkle root, `hashes` is empty".to_string(),
        ));
    }

    match hashes.len() {
        1 => Ok(*hashes.first().expect("Cannot fail")),
        2 => Ok(cn_fast_hash2(
            hashes.first().expect("Cannot fail"),
            hashes.get(1).expect("Cannot fail"),
        )),
        n => {
            let mut cnt = tree_hash_count(n)?;
            let mut buf = vec![Hash::null(); cnt];

            // c is the number of elements between the number of hashes and the next power of 2.
            let c = cnt.saturating_mul(2).saturating_sub(hashes.len());

            buf.get_mut(..c)
                .expect("Cannot fail")
                .copy_from_slice(hashes.get(..c).expect("Cannot fail"));

            // Hash the rest of the hashes together to
            let mut i: usize = c;
            for b in buf.get_mut(c..cnt).expect("Cannot fail") {
                *b = cn_fast_hash2(
                    hashes.get(i).expect("Cannot fail"),
                    hashes.get(i.saturating_add(1)).expect("Cannot fail"),
                );
                i = i.saturating_add(2);
            }

            if i != hashes.len() {
                return Err(MergeMineError::HashingError(
                    "Cannot calculate the merkle root, hashes not equal to count".to_string(),
                ));
            }

            while cnt > 2 {
                cnt >>= 1;
                let mut i = 0;
                for j in 0..cnt {
                    *buf.get_mut(j).expect("Cannot fail") = cn_fast_hash2(
                        buf.get(i).expect("Cannot fail"),
                        buf.get(i.saturating_add(1)).expect("Cannot fail"),
                    );
                    i = i.saturating_add(2);
                }
            }

            Ok(cn_fast_hash2(
                buf.first().expect("Cannot fail"),
                buf.get(1).expect("Cannot fail"),
            ))
        },
    }
}

/// The branch length an honest merkle proof has for the leaf at `pos` in a Monero tree over `aux_chain_count`
/// leaves.
///
/// Monero's tree (see `tree_hash` / `create_merkle_proof`, ports of `tree-hash.c` and `tree_branch`) is *not*
/// balanced. `tree_hash_count` rounds the leaf count *down* to a power of two, so with `depth = ceil(log2(n))` and
/// `k = 2^depth - n`, the first `k` leaves are carried straight into the second level and therefore sit one level
/// higher than the remaining `n - k` leaves, which are paired up first. Consequently:
///
/// * `n == 1` - the leaf is the root, so the branch is empty;
/// * `pos < k` - the leaf is one of the promoted ones, branch length `depth - 1`;
/// * `pos >= k` - branch length `depth`.
///
/// This is exactly the invariant `get_position_from_path` assumes when it consumes `depth - 1` path bits and only
/// reads one more when the partial position lands at or above `k`.
///
/// `aux_chain_count` is a `u8` because that is what the merge mining tag encodes
/// (`MerkleTreeParameters::number_of_chains`). The narrowing is not cosmetic: the loop below doubles `k` until it
/// reaches the chain count, so a count above `2^31` would make `k <<= 1` wrap to zero and the loop spin forever. A
/// `u8` caps `k` at 256 and `depth` at 8, which makes termination structural rather than incidental.
pub fn expected_branch_len(aux_chain_count: u8, pos: u32) -> usize {
    if aux_chain_count <= 1 {
        return 0;
    }

    let aux_chain_count = u32::from(aux_chain_count);
    let mut depth = 0usize;
    let mut k = 1u32;
    while k < aux_chain_count {
        depth = depth.saturating_add(1);
        k <<= 1;
    }

    // `k` is 2^depth here; the number of leaves promoted a level is 2^depth - n.
    let promoted = k.saturating_sub(aux_chain_count);
    if pos < promoted { depth.saturating_sub(1) } else { depth }
}

/// The Monero merkle proof
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct MerkleProof {
    branch: Vec<Hash>,
    path_bitmap: u32,
}

impl BorshSerialize for MerkleProof {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_varint(self.branch.len())?;
        for hash in &self.branch {
            hash.consensus_encode(writer)?;
        }
        writer.write_varint(self.path_bitmap)?;
        Ok(())
    }
}

impl BorshDeserialize for MerkleProof {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let len = reader.read_varint()?;
        if len >= MAX_MERKLE_TREE_PROOF_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Larger than max merkle tree length".to_string(),
            ));
        }
        let mut branch = Vec::with_capacity(len);
        for _ in 0..len {
            branch.push(
                Hash::consensus_decode(reader)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?,
            );
        }
        let path_bitmap = reader.read_varint()?;
        Ok(Self { branch, path_bitmap })
    }
}

impl MerkleProof {
    fn try_construct(branch: Vec<Hash>, path_bitmap: u32) -> Option<Self> {
        if branch.len() >= MAX_MERKLE_TREE_PROOF_SIZE {
            return None;
        }
        Some(Self { branch, path_bitmap })
    }

    /// Returns the merkle proof branch as a list of Monero hashes
    #[inline]
    pub fn branch(&self) -> &[Hash] {
        &self.branch
    }

    /// returns the path bitmap of the proof
    pub fn path(&self) -> u32 {
        self.path_bitmap
    }

    /// The coinbase must be the first transaction in the block, so
    /// that you can't have multiple coinbases in a block. That means the coinbase
    /// is always the leftmost branch in the merkle tree, this test if the given proof is for the left most branch in
    /// the merkle tree
    pub fn check_coinbase_path(&self) -> bool {
        if self.path_bitmap == 0b00000000 {
            return true;
        }
        false
    }

    /// Calculates the merkle root hash from the provide Monero hash, together with the leaf position the proof's
    /// path bitmap encodes.
    ///
    /// When `enforce_depth_binding` is set, the branch length is required to match the length an honest proof for
    /// that leaf position in a tree of `aux_chain_count` leaves would have. Without that binding `calculate_root`
    /// walks an attacker-chosen number of levels while `get_position_from_path` derives its own depth from the
    /// chain count, and the two are never reconciled - which lets one Monero proof of work commit to many distinct
    /// Tari headers at the same height.
    ///
    /// The same flag also requires the path bitmap to be canonical: no bit may be set at or above the expected
    /// branch length. Only the low `expected` bits are ever read - `calculate_root` walks `branch.len()` levels and
    /// `get_position_from_path` consumes at most `depth` bits - so every higher bit is free space that changes the
    /// serialized proof, and therefore `BlockHeader::hash()` (which chains `pow`), without changing the merge
    /// mining hash, the root, the position or the proof of work. That is a malleability: a peer can take an honest
    /// block off the wire, flip a high bit, and produce a distinct but still valid block hash for free, missing
    /// every hash keyed dedup, bad block cache and reconciliation lock and forcing a fresh RandomX hash. In
    /// production `aux_chain_count == 1` the branch is empty and neither reader touches the bitmap at all, so all
    /// 32 bits are free.
    ///
    /// This mirrors `MerkleProof::check_coinbase_path`, which already requires exactly `path_bitmap == 0` for the
    /// *coinbase* proof (enforced in `MoneroPowData::is_coinbase_valid_merkle_root`). The aux proof is now held to
    /// the same standard; this is consistency with an existing rule, not a new design.
    pub fn calculate_root_with_pos(
        &self,
        hash: &Hash,
        aux_chain_count: u8,
        enforce_depth_binding: bool,
    ) -> Result<(Hash, u32), MergeMineError> {
        let pos = self.get_position_from_path(aux_chain_count);
        if enforce_depth_binding {
            let expected = expected_branch_len(aux_chain_count, pos);
            if self.branch.len() != expected {
                return Err(MergeMineError::ValidationError(format!(
                    "Aux chain merkle proof branch length ({}) does not match the length implied by the aux chain \
                     count ({aux_chain_count}) at position {pos} ({expected})",
                    self.branch.len(),
                )));
            }
            // Bits at or above `expected` are never read. `checked_shr` yields `None` once the shift reaches the
            // width of the type, in which case every bit is below the branch length and there is nothing to reject,
            // so an unreadable shift can neither panic nor wrap into a false accept.
            let unread_bits = u32::try_from(expected)
                .ok()
                .and_then(|shift| self.path_bitmap.checked_shr(shift))
                .unwrap_or(0);
            if unread_bits != 0 {
                return Err(MergeMineError::ValidationError(format!(
                    "Aux chain merkle proof path bitmap ({:#010x}) has bits set at or above the branch length \
                     ({expected}) implied by the aux chain count ({aux_chain_count}) at position {pos}; those bits \
                     are never read, so they are pure block hash malleability",
                    self.path_bitmap,
                )));
            }
        }
        Ok((self.calculate_root(hash), pos))
    }

    pub fn calculate_root(&self, hash: &Hash) -> Hash {
        if self.branch.is_empty() {
            return *hash;
        }

        let mut root = *hash;
        let depth = self.branch.len();
        for d in 0..depth {
            if (self.path_bitmap >> depth.saturating_sub(d).saturating_sub(1)) & 1 > 0 {
                root = cn_fast_hash2(self.branch.get(d).expect("Should exist"), &root);
            } else {
                root = cn_fast_hash2(&root, self.branch.get(d).expect("Should exist"));
            }
        }

        root
    }

    /// The leaf position the path bitmap encodes, for a Monero tree over `aux_chain_count` leaves.
    ///
    /// `aux_chain_count` is a `u8` for the same reason as in `expected_branch_len`: the doubling loop below only
    /// terminates while the count stays under `2^31`, and the merge mining tag never encodes more than a `u8`.
    pub fn get_position_from_path(&self, aux_chain_count: u8) -> u32 {
        if aux_chain_count <= 1 {
            return 0;
        }

        let aux_chain_count = u32::from(aux_chain_count);
        let mut depth = 0usize;
        let mut k = 1u32;

        while k < aux_chain_count {
            depth = depth.saturating_add(1);
            k <<= 1;
        }

        k = k.saturating_sub(aux_chain_count);

        let mut pos = 0;
        let mut path = self.path_bitmap;

        for _i in 1..depth {
            pos = (pos << 1) | (path & 1);
            path >>= 1;
        }

        if pos < k {
            return pos;
        }

        ((pos.saturating_sub(k) << 1) | (path & 1)).saturating_add(k)
    }
}

impl Default for MerkleProof {
    fn default() -> Self {
        Self {
            branch: vec![Hash::null()],
            path_bitmap: 0,
        }
    }
}

/// Creates a merkle proof for the given hash within the set of hashes. This function returns None if the hash is not in
/// hashes. This is a port of Monero's tree_branch function
#[allow(clippy::cognitive_complexity)]
pub fn create_merkle_proof(hashes: &[Hash], hash: &Hash) -> Option<MerkleProof> {
    match hashes.len() {
        0 => None,
        1 => {
            if hashes.first().expect("Already checked") != hash {
                return None;
            }
            MerkleProof::try_construct(vec![], 0)
        },
        2 => hashes.iter().enumerate().find_map(|(pos, h)| {
            if h != hash {
                return None;
            }
            let i = usize::from(pos == 0);
            MerkleProof::try_construct(vec![*hashes.get(i).expect("Already checked")], u32::from(pos != 0))
        }),
        len => {
            let mut idx = hashes.iter().position(|node| node == hash)?;
            let mut count = tree_hash_count(len).ok()?;

            let mut ints = vec![Hash::null(); count];

            let c = count.saturating_mul(2).saturating_sub(len);
            ints.get_mut(..c)
                .expect("Already checked")
                .copy_from_slice(hashes.get(..c).expect("Already checked"));

            let mut branch = Vec::new();
            let mut path = 0u32;
            let mut i = c;
            for (j, val) in ints.iter_mut().enumerate().take(count).skip(c) {
                // Left or right
                let next = i.saturating_add(1);
                if idx == i || idx == next {
                    let ii = if idx == i { next } else { i };
                    branch.push(*hashes.get(ii).expect("Already checked"));
                    path = (path << 1) | u32::from(idx != i);
                    idx = j;
                }
                *val = cn_fast_hash2(
                    hashes.get(i).expect("Already checked"),
                    hashes.get(next).expect("Already checked"),
                );
                i = i.saturating_add(2);
            }

            debug_assert_eq!(i, len);

            while count > 2 {
                count >>= 1;
                let mut i = 0usize;
                for j in 0..count {
                    let next = i.saturating_add(1);
                    if idx == i || idx == next {
                        let ii = if idx == i { next } else { i };
                        branch.push(*ints.get(ii).expect("Already checked"));
                        path = (path << 1) | u32::from(idx != i);
                        idx = j;
                    }
                    *ints.get_mut(j).expect("Already checked") = cn_fast_hash2(
                        ints.get(i).expect("Already checked"),
                        ints.get(next).expect("Already checked"),
                    );
                    i = i.saturating_add(2);
                }
            }

            if idx == 0 || idx == 1 {
                let ii = usize::from(idx == 0);
                branch.push(*ints.get(ii).expect("Already checked"));
                path = (path << 1) | u32::from(idx != 0);
            }

            MerkleProof::try_construct(branch, path)
        },
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use std::str::FromStr;

    use monero::{
        blockdata::block::BlockHeader,
        consensus::encode::{VarInt, serialize},
    };
    use tari_test_utils::unpack_enum;
    use tari_utilities::hex::{Hex, from_hex};

    use super::*;
    use crate::proof_of_work::randomx_factory::RandomXFactory;
    mod quicktest {
        use monero::Hash;
        use quickcheck::{Arbitrary, Gen, quickcheck};

        use crate::proof_of_work::monero_rx::merkle_tree::{MAX_MERKLE_TREE_PROOF_SIZE, MerkleProof};

        #[derive(Clone, Debug)]
        struct QuickHash {
            pub bits: Vec<u8>,
        }

        impl Arbitrary for QuickHash {
            fn arbitrary(g: &mut Gen) -> QuickHash {
                let mut hash = Vec::new();
                for _ in 0..32 {
                    hash.push(u8::arbitrary(g));
                }
                QuickHash { bits: hash }
            }
        }

        fn create_monero_hashes(input_vec: Vec<QuickHash>) -> Vec<Hash> {
            input_vec
                .into_iter()
                .map(|v| Hash::from_slice(v.bits.as_slice()))
                .collect()
        }
        #[test]
        fn test_create() {
            fn try_create(input_vec: Vec<QuickHash>, path: u32) -> bool {
                let hashes = create_monero_hashes(input_vec);
                let length = hashes.len();
                let res = MerkleProof::try_construct(hashes, path);
                if length >= MAX_MERKLE_TREE_PROOF_SIZE {
                    return res.is_none();
                }
                res.is_some()
            }
            quickcheck(try_create as fn(Vec<QuickHash>, u32) -> bool)
        }

        #[test]
        fn test_proof() {
            fn proof_first(input_vec: Vec<QuickHash>, path: u32) -> bool {
                if input_vec.is_empty() {
                    return true;
                }
                let hashes = create_monero_hashes(input_vec);
                let hash = hashes[0];
                let length = hashes.len();
                if length >= MAX_MERKLE_TREE_PROOF_SIZE {
                    return true;
                }
                let proof = MerkleProof::try_construct(hashes, path).unwrap();
                proof.calculate_root(&hash);
                true
            }

            fn proof_random(input_vec: Vec<QuickHash>, hash: QuickHash, path: u32) -> bool {
                let hashes = create_monero_hashes(input_vec);
                let hash = Hash::from_slice(hash.bits.as_slice());
                let length = hashes.len();
                if length >= MAX_MERKLE_TREE_PROOF_SIZE {
                    return true;
                }
                let proof = MerkleProof::try_construct(hashes, path).unwrap();
                proof.calculate_root(&hash);
                true
            }
            quickcheck(proof_first as fn(Vec<QuickHash>, u32) -> bool);
            quickcheck(proof_random as fn(Vec<QuickHash>, QuickHash, u32) -> bool);
        }
    }
    /// Binding the aux-chain merkle proof's branch length to the depth implied by the chain count.
    ///
    /// Monero's tree is the "narrow" tree: with `depth = ceil(log2(n))` and `k = 2^depth - n`, the first `k` leaves
    /// sit one level higher than the rest, so the honest branch length is position-dependent. A flat
    /// `ceil(log2(n))` rule would reject honest proofs for every non-power-of-two chain count.
    mod depth_binding {
        use super::*;

        /// `n` distinct leaf hashes. Distinctness matters: `create_merkle_proof` locates the leaf by value.
        fn distinct_hashes(n: usize) -> Vec<Hash> {
            (0..n)
                .map(|i| {
                    let mut buf = [0u8; 32];
                    buf[..4].copy_from_slice(&u32::try_from(i).unwrap().to_le_bytes());
                    Hash::from_slice(&buf)
                })
                .collect()
        }

        /// The per-position lengths, pinned as literals. n = 3 and n = 5 are the cases a flat `ceil(log2(n))` rule
        /// gets wrong, and the cases where testing a single position passes by luck.
        #[test]
        fn the_narrow_tree_branch_lengths_are_pinned() {
            assert_eq!(expected_branch_len(1, 0), 0, "a single chain is its own root");
            // depth 1, k = 0: a balanced pair.
            assert_eq!([0, 1].map(|pos| expected_branch_len(2, pos)), [1, 1]);
            // depth 2, k = 1: leaf 0 is promoted a level.
            assert_eq!([0, 1, 2].map(|pos| expected_branch_len(3, pos)), [1, 2, 2]);
            // depth 2, k = 0: balanced.
            assert_eq!([0, 1, 2, 3].map(|pos| expected_branch_len(4, pos)), [2, 2, 2, 2]);
            // depth 3, k = 3: leaves 0, 1 and 2 are promoted a level.
            assert_eq!([0, 1, 2, 3, 4].map(|pos| expected_branch_len(5, pos)), [2, 2, 2, 3, 3]);
            // depth 3, k = 2, the shape the pre-existing `simple_proof_construction` test walks by hand.
            assert_eq!([0, 1, 2, 3, 4, 5].map(|pos| expected_branch_len(6, pos)), [
                2, 2, 3, 3, 3, 3
            ]);
            // depth 8, k = 1.
            assert_eq!(expected_branch_len(255, 0), 7);
            assert_eq!([1, 2, 254].map(|pos| expected_branch_len(255, pos)), [8, 8, 8]);
        }

        /// The round trip: for every leaf of every tree size, the proof `create_merkle_proof` actually produces
        /// must be accepted, and the position and root must be exactly what the unbound code returned.
        #[test]
        fn honest_proofs_are_accepted_at_every_leaf_position() {
            for n in (1..=17u32).chain([64, 255]) {
                let hashes = distinct_hashes(n as usize);
                let expected_root = tree_hash(&hashes).unwrap();
                let count = u8::try_from(n).unwrap();
                for pos in 0..n {
                    let leaf = hashes[pos as usize];
                    let proof = create_merkle_proof(&hashes, &leaf).unwrap();
                    assert_eq!(
                        proof.branch().len(),
                        expected_branch_len(count, pos),
                        "n={n} pos={pos}: create_merkle_proof disagrees with expected_branch_len"
                    );
                    // The canonical bitmap rule must hold for proofs the honest producer really emits, otherwise
                    // enforcing it would reject real blocks.
                    assert_eq!(
                        proof.path() >> u32::try_from(proof.branch().len()).unwrap(),
                        0,
                        "n={n} pos={pos}: create_merkle_proof set a path bit at or above the branch length"
                    );

                    let bound = proof
                        .calculate_root_with_pos(&leaf, count, true)
                        .unwrap_or_else(|e| panic!("n={n} pos={pos} honest proof rejected: {e}"));
                    assert_eq!(bound, (expected_root, pos), "n={n} pos={pos}");
                    // The binding must not change the answer for an honest proof, only reject dishonest ones.
                    let unbound = proof.calculate_root_with_pos(&leaf, count, false).unwrap();
                    assert_eq!(bound, unbound, "n={n} pos={pos}: binding changed an honest result");
                }
            }
        }

        /// A single chain is a zero-depth tree, so any branch at all is a forgery. This is the case the
        /// `VarInt(0)` encoding makes reachable with a vacuous position check.
        #[test]
        fn a_single_chain_with_a_non_empty_branch_is_rejected() {
            let leaf = distinct_hashes(1)[0];
            for len in 1..5 {
                let forged = MerkleProof::try_construct(vec![Hash::null(); len], 0).unwrap();
                assert!(
                    forged.calculate_root_with_pos(&leaf, 1, true).is_err(),
                    "single chain accepted a branch of length {len}"
                );
                // ...and this is exactly what the pre-fork rules let through.
                assert!(forged.calculate_root_with_pos(&leaf, 1, false).is_ok());
            }
        }

        /// Every off-by-one and every over-long branch, for every leaf of every listed tree size.
        #[test]
        fn a_branch_length_that_does_not_match_the_chain_count_is_rejected() {
            for n in [1u32, 2, 3, 4, 255] {
                let hashes = distinct_hashes(n as usize);
                let count = u8::try_from(n).unwrap();
                for pos in 0..n {
                    let leaf = hashes[pos as usize];
                    let honest = create_merkle_proof(&hashes, &leaf).unwrap();
                    let honest_len = honest.branch().len();
                    for len in 0..(honest_len + 3).min(MAX_MERKLE_TREE_PROOF_SIZE) {
                        if len == honest_len {
                            continue;
                        }
                        // Keep the honest path bitmap, so the derived position is unchanged and the length is the
                        // only thing that differs.
                        let mut branch = honest.branch().to_vec();
                        branch.resize(len, Hash::null());
                        let forged = MerkleProof::try_construct(branch, honest.path()).unwrap();
                        assert!(
                            forged.calculate_root_with_pos(&leaf, count, true).is_err(),
                            "n={n} pos={pos}: branch of length {len} accepted, honest length is {honest_len}"
                        );
                    }
                }
            }
        }

        /// The bits of the path bitmap at or above the branch length are never read: `calculate_root` walks
        /// `branch.len()` levels and `get_position_from_path` stops at `depth`. Flipping one therefore leaves the
        /// root, the position and the proof of work identical while changing the serialized proof - and with it
        /// `BlockHeader::hash()`, which chains `pow` where `merge_mining_hash()` does not. Post-fork they must be
        /// rejected; pre-fork they were free, and must stay accepted so no existing block is retroactively invalid.
        #[test]
        fn path_bitmap_bits_above_the_branch_length_are_rejected() {
            for n in (1..=17u32).chain([64, 255]) {
                let hashes = distinct_hashes(n as usize);
                let count = u8::try_from(n).unwrap();
                for pos in 0..n {
                    let leaf = hashes[pos as usize];
                    let honest = create_merkle_proof(&hashes, &leaf).unwrap();
                    let honest_unbound = honest.calculate_root_with_pos(&leaf, count, false).unwrap();
                    let len = u32::try_from(honest.branch().len()).unwrap();
                    for bit in len..32 {
                        let forged =
                            MerkleProof::try_construct(honest.branch().to_vec(), honest.path() | (1 << bit)).unwrap();
                        assert_ne!(
                            forged, honest,
                            "n={n} pos={pos} bit={bit}: the forgery must really differ"
                        );
                        assert!(
                            forged.calculate_root_with_pos(&leaf, count, true).is_err(),
                            "n={n} pos={pos}: an unread path bit {bit} was accepted"
                        );
                        // ...and this is the malleability itself: pre-fork the forgery is accepted and is
                        // indistinguishable from the honest proof in every value the validator looks at.
                        assert_eq!(
                            forged.calculate_root_with_pos(&leaf, count, false).unwrap(),
                            honest_unbound,
                            "n={n} pos={pos} bit={bit}: the unread bit changed a checked value"
                        );
                    }
                }
            }
        }

        /// The production case. With one aux chain the branch is empty, `calculate_root` returns early and
        /// `get_position_from_path` returns 0 without reading the bitmap at all, so all 32 bits are free.
        #[test]
        fn a_single_chain_leaves_no_free_path_bits() {
            let leaf = distinct_hashes(1)[0];
            let honest = create_merkle_proof(&[leaf], &leaf).unwrap();
            assert_eq!(honest.path(), 0, "the honest single-chain proof has an empty bitmap");
            assert!(honest.calculate_root_with_pos(&leaf, 1, true).is_ok());

            let mut roots = std::collections::HashSet::new();
            for bit in 0..32 {
                let forged = MerkleProof::try_construct(vec![], 1 << bit).unwrap();
                assert!(
                    forged.calculate_root_with_pos(&leaf, 1, true).is_err(),
                    "single chain accepted free path bit {bit}"
                );
                let (root, pos) = forged.calculate_root_with_pos(&leaf, 1, false).unwrap();
                assert_eq!((root, pos), (leaf, 0), "bit {bit} is genuinely unread pre-fork");
                roots.insert(root);
            }
            assert_eq!(
                roots.len(),
                1,
                "all 32 bits produce the same root - which is what makes them free malleability"
            );
        }

        /// The attack the binding closes: without it, one leaf hash and one chain count admit many branch lengths,
        /// each producing a different root - i.e. one Monero solution committing to many distinct Tari headers.
        #[test]
        fn without_the_binding_one_leaf_yields_many_distinct_roots() {
            let leaf = distinct_hashes(1)[0];
            let mut roots = std::collections::HashSet::new();
            for len in 0..8 {
                let forged = MerkleProof::try_construct(vec![Hash::from([7u8; 32]); len], 0).unwrap();
                let (root, pos) = forged.calculate_root_with_pos(&leaf, 1, false).unwrap();
                assert_eq!(pos, 0, "the position check is vacuous for a single chain");
                roots.insert(root);
            }
            assert_eq!(roots.len(), 8, "expected a distinct root per branch length");
        }
    }

    mod tree_hash {
        use super::*;

        fn randomx_hash(input: &[u8], key: &str) -> String {
            let key = from_hex(key).unwrap();
            RandomXFactory::default()
                .create(&key, None, None)
                .unwrap()
                .calculate_hash(input)
                .unwrap()
                .to_hex()
        }

        #[test]
        fn test_tree_hash() {
            let tx_hash = [
                88, 176, 48, 182, 128, 13, 67, 59, 188, 178, 181, 96, 175, 226, 160, 142, 77, 193, 82, 250, 119, 234,
                217, 109, 55, 170, 241, 72, 151, 211, 192, 150,
            ];
            let mut hashes = vec![Hash::from(tx_hash)];

            // Single hash
            let mut root = tree_hash(&hashes).unwrap();
            assert_eq!(root.as_bytes(), tx_hash);

            // 2 hashes
            hashes.push(Hash::from(tx_hash));
            root = tree_hash(&hashes).unwrap();
            let correct_root = [
                187, 251, 201, 6, 70, 27, 80, 117, 95, 97, 244, 143, 194, 245, 73, 174, 158, 255, 98, 175, 74, 22, 173,
                223, 217, 17, 59, 183, 230, 39, 76, 202,
            ];
            assert_eq!(root.as_bytes(), correct_root);

            // More than 2 hashes
            hashes.push(Hash::from(tx_hash));
            root = tree_hash(&hashes).unwrap();
            let correct_root = [
                37, 100, 243, 131, 133, 33, 135, 169, 23, 215, 243, 10, 213, 152, 21, 10, 89, 86, 217, 49, 245, 237,
                205, 194, 102, 162, 128, 225, 215, 192, 158, 251,
            ];
            assert_eq!(root.as_bytes(), correct_root);

            hashes.push(Hash::from(tx_hash));
            root = tree_hash(&hashes).unwrap();
            let correct_root = [
                52, 199, 248, 213, 213, 138, 52, 0, 145, 179, 81, 247, 174, 31, 183, 196, 124, 186, 100, 21, 36, 252,
                171, 66, 250, 247, 122, 64, 36, 127, 184, 46,
            ];
            assert_eq!(root.as_bytes(), correct_root);
        }

        #[test]
        fn tree_hash_4_elements() {
            let hashes = (1..=4).map(|i| Hash::from([i; 32])).collect::<Vec<_>>();
            let h01 = cn_fast_hash2(&hashes[0], &hashes[1]);
            let h23 = cn_fast_hash2(&hashes[2], &hashes[3]);
            let expected = cn_fast_hash2(&h01, &h23);

            let root_hash = tree_hash(&hashes).unwrap();
            assert_eq!(root_hash, expected);
        }

        #[test]
        fn tree_hash_6_elements() {
            //        { root }
            //      /        \
            //     h01       h2345
            //   /    \     /    \
            //  0     1    h23   h45
            //            /  \   /  \
            //           2    3 4    5

            let hashes = (1..=6).map(|i| Hash::from([i; 32])).collect::<Vec<_>>();
            let h23 = cn_fast_hash2(&hashes[2], &hashes[3]);
            let h45 = cn_fast_hash2(&hashes[4], &hashes[5]);
            let h01 = cn_fast_hash2(&hashes[0], &hashes[1]);
            let h2345 = cn_fast_hash2(&h23, &h45);

            let h012345 = cn_fast_hash2(&h01, &h2345);

            let root_hash = tree_hash(&hashes).unwrap();
            assert_eq!(root_hash, h012345);
        }

        #[test]
        fn check_tree_hash_against_mainnet_block() {
            // Data from block https://xmrchain.net/search?value=2375600
            let header = BlockHeader {
                major_version: VarInt(14),
                minor_version: VarInt(14),
                timestamp: VarInt(1622783559),
                prev_id: Hash::from_str("fd3ce7d80ec86167f74e52cacc0eb8bd8c9e674786fc2cbbaee5879eab906986").unwrap(),
                nonce: 16657,
            };
            let tx_hashes = &[
                "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
                "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
                "c723329b1036e4e05313c6ec3bdda3a2e1ab4db17661cad1a6a33512d9b86bcd",
                "5d863b3d275bacd46dbe8a5f3edce86f88cbc01232bd2788b6f44684076ef8a8",
                "16d945de6c96ea7f986b6c70ad373a9203a1ddd1c5d12effc3c69b8648826deb",
                "ccec8f06c5bab1b87bb9af1a3cba94304f87dc037e03b5d2a00406d399316ff7",
                "c8d52ed0712f0725531f8f72da029201b71e9e215884015f7050dde5f33269e7",
                "4360ba7fe3872fa8bbc9655486a02738ee000d0c48bda84a15d4730fea178519",
                "3c8c6b54dcffc75abff89d604ebf1e216bfcb2844b9720ab6040e8e49ae9743c",
                "6dc19de81e509fba200b652fbdde8fe2aeb99bb9b17e0af79d0c682dff194e08",
                "3ef031981bc4e2375eebd034ffda4e9e89936962ad2c94cfcc3e6d4cfa8a2e8c",
                "9e4b865ebe51dcc9cfb09a9b81e354b8f423c59c902d5a866919f053bfbc374e",
                "fa58575f7d1d377709f1621fac98c758860ca6dc5f2262be9ce5fd131c370d1a",
            ]
            .iter()
            .map(|hash| Hash::from_str(hash).unwrap())
            .collect::<Vec<_>>();

            let num_transactions = VarInt(tx_hashes.len() as u64);

            let tx_root = tree_hash(tx_hashes).unwrap();
            let mut blob = Vec::new();
            blob.extend(serialize(&header));
            blob.extend_from_slice(tx_root.as_bytes());
            blob.extend(serialize(&num_transactions));

            let pow_hash = randomx_hash(
                &blob,
                "85170d70e15e4035c3e664a8192f11d347d2939371d840e3f65db5a6645c571d",
            );
            let expected_pow_hash = "53f9876405e60c1d37a67b4cf09670061c745a18c70f89dc2d61020100000000";
            assert_eq!(&pow_hash, expected_pow_hash);
        }

        #[test]
        fn check_tree_hash_against_empty_stagenet_block() {
            // Taken from block: https://stagenet.xmrchain.net/search?value=672576
            let header = BlockHeader {
                major_version: VarInt(12),
                minor_version: VarInt(12),
                timestamp: VarInt(1601031202),
                prev_id: Hash::from_str("046f4fe371f9acdc27c377f4adee84e93b11f89246a74dd77f1bf0856141da5c").unwrap(),
                nonce: 307182078,
            };
            let num_transactions = VarInt(1);
            let tx_hashes = &["77139305ea53cfe95cf7235d2fed6fca477395b019b98060acdbc0f8fb0b8b92"]
                .iter()
                .map(|hash| Hash::from_str(hash).unwrap())
                .collect::<Vec<_>>();

            let tx_root = tree_hash(tx_hashes).unwrap();
            let mut blob = Vec::new();
            blob.extend(serialize(&header));
            blob.extend_from_slice(tx_root.as_bytes());
            blob.extend(serialize(&num_transactions));

            // Key obtained by using the block hash at height `h - (h % 2048)` where `h` is the height if this block
            // (672576)
            let pow_hash = randomx_hash(
                &blob,
                "2aca6501719a5c7ab7d4acbc7cc5d277b57ad8c27c6830788c2d5a596308e5b1",
            );
            let expected_pow_hash = "f68fbc8cc85bde856cd1323e9f8e6f024483038d728835de2f8c014ff6260000";
            assert_eq!(&pow_hash, expected_pow_hash);
        }

        #[test]
        fn test_tree_hash_fail() {
            let err = tree_hash(&[]).unwrap_err();
            unpack_enum!(MergeMineError::HashingError(_e) = err);
        }
    }

    mod create_merkle_proof {
        use rand::Rng;

        use super::*;

        #[test]
        fn empty_hashset_has_no_proof() {
            assert!(create_merkle_proof(&[], &Hash::null()).is_none());
        }

        #[test]
        fn single_hash_is_its_own_proof() {
            let tx_hashes =
                &[Hash::from_str("fa58575f7d1d377709f1621fac98c758860ca6dc5f2262be9ce5fd131c370d1a").unwrap()];
            let proof = create_merkle_proof(&tx_hashes[..], &tx_hashes[0]).unwrap();
            assert_eq!(proof.branch.len(), 0);
            assert_eq!(proof.calculate_root(&tx_hashes[0]), tx_hashes[0]);

            assert!(create_merkle_proof(&tx_hashes[..], &Hash::null()).is_none());
        }

        #[test]
        fn two_hash_proof_construction() {
            let tx_hashes = &[
                "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
                "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
            ]
            .iter()
            .map(|hash| Hash::from_str(hash).unwrap())
            .collect::<Vec<_>>();

            let expected_root = cn_fast_hash2(&tx_hashes[0], &tx_hashes[1]);
            let proof = create_merkle_proof(tx_hashes, &tx_hashes[0]).unwrap();
            assert_eq!(proof.branch()[0], tx_hashes[1]);
            assert_eq!(proof.branch.len(), 1);
            assert_eq!(proof.branch[0], tx_hashes[1]);
            assert_eq!(proof.path_bitmap, 0b00000000);
            assert_eq!(proof.calculate_root(&tx_hashes[0]), expected_root);

            let proof = create_merkle_proof(tx_hashes, &tx_hashes[1]).unwrap();
            assert_eq!(proof.branch()[0], tx_hashes[0]);
            assert_eq!(proof.calculate_root(&tx_hashes[1]), expected_root);

            assert!(create_merkle_proof(tx_hashes, &Hash::null()).is_none());
        }

        #[test]
        fn simple_proof_construction() {
            //        { root }
            //      /        \
            //     h01       h2345
            //   /    \     /    \
            //  h0    h1    h23   h45
            //            /  \    /  \
            //          h2    h3 h4   h5

            let hashes = (1..=6).map(|i| Hash::from([i - 1; 32])).collect::<Vec<_>>();
            let h23 = cn_fast_hash2(&hashes[2], &hashes[3]);
            let h45 = cn_fast_hash2(&hashes[4], &hashes[5]);
            let h01 = cn_fast_hash2(&hashes[0], &hashes[1]);
            let h2345 = cn_fast_hash2(&h23, &h45);
            let expected_root = cn_fast_hash2(&h01, &h2345);

            // Proof for h0
            let proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
            assert_eq!(proof.calculate_root(&hashes[0]), expected_root);
            assert_eq!(proof.branch().len(), 2);
            assert_eq!(proof.branch()[0], hashes[1]);
            assert_eq!(proof.branch()[1], h2345);
            assert_eq!(proof.path_bitmap, 0b00000000);

            // Proof for h2
            let proof = create_merkle_proof(&hashes, &hashes[2]).unwrap();
            assert_eq!(proof.calculate_root(&hashes[2]), expected_root);
            assert_eq!(proof.path_bitmap, 0b00000001);
            let branch = proof.branch();
            assert_eq!(branch[0], hashes[3]);
            assert_eq!(branch[1], h45);
            assert_eq!(branch[2], h01);
            assert_eq!(branch.len(), 3);

            // Proof for h5
            let proof = create_merkle_proof(&hashes, &hashes[5]).unwrap();
            assert_eq!(proof.calculate_root(&hashes[5]), expected_root);
            assert_eq!(proof.branch.len(), 3);
            assert_eq!(proof.path_bitmap, 0b00000111);
            let branch = proof.branch();
            assert_eq!(branch[0], hashes[4]);
            assert_eq!(branch[1], h23);
            assert_eq!(branch[2], h01);
            assert_eq!(branch.len(), 3);

            // Proof for h4
            let proof = create_merkle_proof(&hashes, &hashes[4]).unwrap();
            assert_eq!(proof.calculate_root(&hashes[4]), expected_root);
            assert_eq!(proof.branch.len(), 3);
            assert_eq!(proof.path_bitmap, 0b00000011);
            let branch = proof.branch();
            assert_eq!(branch[0], hashes[5]);
            assert_eq!(branch[1], h23);
            assert_eq!(branch[2], h01);
            assert_eq!(branch.len(), 3);
        }

        #[test]
        fn more_complex_proof_construction() {
            let tx_hashes = &[
                "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
                "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
                "c723329b1036e4e05313c6ec3bdda3a2e1ab4db17661cad1a6a33512d9b86bcd",
                "5d863b3d275bacd46dbe8a5f3edce86f88cbc01232bd2788b6f44684076ef8a8",
                "16d945de6c96ea7f986b6c70ad373a9203a1ddd1c5d12effc3c69b8648826deb",
                "ccec8f06c5bab1b87bb9af1a3cba94304f87dc037e03b5d2a00406d399316ff7",
                "c8d52ed0712f0725531f8f72da029201b71e9e215884015f7050dde5f33269e7",
                "4360ba7fe3872fa8bbc9655486a02738ee000d0c48bda84a15d4730fea178519",
                "3c8c6b54dcffc75abff89d604ebf1e216bfcb2844b9720ab6040e8e49ae9743c",
                "6dc19de81e509fba200b652fbdde8fe2aeb99bb9b17e0af79d0c682dff194e08",
                "3ef031981bc4e2375eebd034ffda4e9e89936962ad2c94cfcc3e6d4cfa8a2e8c",
                "9e4b865ebe51dcc9cfb09a9b81e354b8f423c59c902d5a866919f053bfbc374e",
                "fa58575f7d1d377709f1621fac98c758860ca6dc5f2262be9ce5fd131c370d1a",
            ]
            .iter()
            .map(|hash| Hash::from_str(hash).unwrap())
            .collect::<Vec<_>>();

            let expected_root = tree_hash(tx_hashes).unwrap();

            let hash = Hash::from_str("fa58575f7d1d377709f1621fac98c758860ca6dc5f2262be9ce5fd131c370d1a").unwrap();
            let proof = create_merkle_proof(tx_hashes, &hash).unwrap();

            assert_eq!(proof.path_bitmap, 0b00001111);

            assert_eq!(proof.calculate_root(&hash), expected_root);

            assert!(!proof.branch().contains(&hash));
            assert!(!proof.branch().contains(&expected_root));
        }

        #[test]
        fn big_proof_construction() {
            // 65536 transactions is beyond what is reasonable to fit in a block
            let mut thread_rng = rand::rng();
            let tx_hashes = std::iter::repeat_n((), 0x10000)
                .map(|_| {
                    let mut buf = [0u8; 32];
                    thread_rng.fill_bytes(&mut buf[..]);
                    // Actually performing the keccak hash serves no purpose in this test
                    Hash::from_slice(&buf[..])
                })
                .collect::<Vec<_>>();

            let expected_root = tree_hash(&tx_hashes).unwrap();

            let hash = tx_hashes.last().unwrap();
            let proof = create_merkle_proof(&tx_hashes, hash).unwrap();

            assert_eq!(proof.branch.len(), 16);
            assert_eq!(proof.path_bitmap, 0b1111_1111_1111_1111);

            assert_eq!(proof.calculate_root(hash), expected_root);

            assert!(!proof.branch().contains(hash));
            assert!(!proof.branch().contains(&expected_root));
        }

        #[test]
        fn test_borsh_de_serialization() {
            let tx_hashes =
                &[Hash::from_str("fa58575f7d1d377709f1621fac98c758860ca6dc5f2262be9ce5fd131c370d1a").unwrap()];
            let proof = create_merkle_proof(&tx_hashes[..], &tx_hashes[0]).unwrap();
            let mut buf = Vec::new();
            proof.serialize(&mut buf).unwrap();
            buf.extend_from_slice(&[1, 2, 3]);
            let buf = &mut buf.as_slice();
            assert_eq!(proof, MerkleProof::deserialize(buf).unwrap());
            assert_eq!(buf, &[1, 2, 3]);
        }

        #[tokio::test]
        async fn test_borsh_de_serialization_too_large() {
            // We dont care about the actual merkle tree here, just that its not too large on the varint size
            // We lie about the size to try and get a mem panic, and say this merkle tree is u64::max large.
            let buf = vec![255, 255, 255, 255, 255, 255, 255, 255, 255, 1, 49, 8, 2, 5, 6];
            let buf = &mut buf.as_slice();
            assert!(MerkleProof::deserialize(buf).is_err());
        }
    }
}
