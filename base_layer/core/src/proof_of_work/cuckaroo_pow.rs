//  Copyright 2025, The Tari Project
//
//  Parts of this code modified from the Grin project
//  Copyright 2021 The Grin Developers
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

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
};

use blake2::{Blake2b, Digest};
use digest::{FixedOutput, consts::U32};
use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::tari_proof_of_work::{Difficulty, DifficultyError};
use thiserror::Error;

use crate::proof_of_work::siphash::siphash_block;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CuckarooVerificationError {
    #[error("Unsupported cycle length")]
    UnsupportedCycleLength,
    #[error("PoW data contains non-zero padding")]
    PowDataContainsNonZeroPadding,
    #[error("PoW data is too short")]
    PowDataTooShort,
    #[error("Block header has an invalid PoW algorithm for Cuckaroo")]
    BlockHeaderInvalidPowAlgorithm,
    #[error("Nonce is too large")]
    NonceTooLarge,
    #[error("Nonces not ascending")]
    NoncesNotAscending,
    #[error("Endpoints don't match up")]
    EndpointsDontMatch,
    #[error("CycleTooShort")]
    CycleTooShort,
    #[error("CycleTooLong")]
    CycleTooLong,
    #[error("Edge already visited")]
    EdgeAlreadyVisited,
    #[error("Node has more than two edges")]
    NodeHasMoreThanTwoEdges,
    #[error("Cycle does not end at start")]
    CycleDoesNotEndAtStart,
    #[error("Cycle did not use all edges")]
    CycleDidNotUseAllEdges,
    #[error("Difficulty error: {0}")]
    DifficultyError(#[from] DifficultyError),
}

fn determine_sip_hash(mining_hash: &[u8], nonce: u64) -> Vec<u8> {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(nonce.to_be_bytes());
    hasher.update(mining_hash);
    hasher.finalize_fixed().to_vec()
}

pub fn cuckaroo_result(
    header: &BlockHeader,
    required_cycle_length: u8,
    edge_bits: u8,
    bipartite: bool,
) -> Result<Vec<u8>, CuckarooVerificationError> {
    let pow = header.pow.to_bytes();
    let required_cycle_length = NonZeroUsize::try_from(required_cycle_length as usize)
        .map_err(|_| CuckarooVerificationError::UnsupportedCycleLength)?;
    let packed_size = required_cycle_length.get().saturating_mul(edge_bits as usize);
    let packed_bytes = packed_size.div_ceil(8);
    if pow.is_empty() || pow.len() < packed_bytes.saturating_add(1) {
        return Err(CuckarooVerificationError::PowDataTooShort);
    }
    // First byte must be 3 for Cuckaroo
    if *pow.first().expect("Already checked") != 3 {
        return Err(CuckarooVerificationError::BlockHeaderInvalidPowAlgorithm);
    }
    let pow_data = pow.get(1..).expect("Already checked");
    cuckaroo_result_inner(
        header.mining_hash().as_slice(),
        header.nonce,
        pow_data,
        required_cycle_length,
        edge_bits,
        bipartite,
    )
}

fn cuckaroo_result_inner(
    header_before_nonce: &[u8],
    nonce: u64,
    packed_edge_data: &[u8],
    required_cycle_length: NonZeroUsize,
    edge_bits: u8,
    bipartite: bool,
) -> Result<Vec<u8>, CuckarooVerificationError> {
    let packed_size = required_cycle_length.get().saturating_mul(edge_bits as usize);
    let packed_bytes = packed_size.div_ceil(8);

    let blob = determine_sip_hash(header_before_nonce, nonce);

    // Data after <required_cycle_length * edge_bits> is padding, it must be zero
    for &byte in packed_edge_data.get(packed_bytes..).expect("Already checked") {
        if byte != 0 {
            return Err(CuckarooVerificationError::PowDataContainsNonZeroPadding);
        }
    }

    let nonces = unpack_nonces(packed_edge_data, edge_bits, required_cycle_length.get())?;
    // There might be extra padding at  the end of the nonces.

    // This should not happen because unpack_nonces should return the correct
    // length, but here for completeness
    if nonces.len() > required_cycle_length.get() {
        for n in nonces.get(required_cycle_length.get()..).expect("Already checked") {
            if *n != 0 {
                return Err(CuckarooVerificationError::PowDataContainsNonZeroPadding);
            }
        }
    }

    let siphash_keys = [
        u64::from_le_bytes(
            blob.get(0..8)
                .expect("Already checked")
                .try_into()
                .expect("Cannot fail"),
        ),
        u64::from_le_bytes(
            blob.get(8..16)
                .expect("Already checked")
                .try_into()
                .expect("Cannot fail"),
        ),
        u64::from_le_bytes(
            blob.get(16..24)
                .expect("Already checked")
                .try_into()
                .expect("Cannot fail"),
        ),
        u64::from_le_bytes(
            blob.get(24..32)
                .expect("Already checked")
                .try_into()
                .expect("Cannot fail"),
        ),
    ];
    // // Generate the hasher.
    verify(&siphash_keys, &nonces, required_cycle_length, edge_bits, bipartite)?;

    // Replace the Blake2bVar hasher with Blake2b (fixed size)
    let mut hasher = Blake2b::<U32>::new();

    hasher.update(packed_edge_data);
    let res = hasher.finalize_fixed().to_vec();

    Ok(res)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
// Test-only helper: an overflow panic here is the desired failure mode.
#[allow(clippy::arithmetic_side_effects)]
fn pack_nonces(uncompressed: &[u64], bit_width: u8) -> Vec<u8> {
    let mut target = vec![0u8; (uncompressed.len() * bit_width as usize).div_ceil(8)];
    let mut compressed = target.as_mut_slice();
    let mut mini_buffer = 0u64;
    let mut remaining = 64;
    for el in uncompressed {
        mini_buffer |= el << (64 - remaining);
        if bit_width < remaining {
            remaining -= bit_width;
        } else {
            compressed[..8].copy_from_slice(&mini_buffer.to_le_bytes());
            compressed = &mut compressed[8..];
            mini_buffer = el >> remaining;
            remaining = 64 + remaining - bit_width;
        }
    }
    let mut remainder = compressed.len() % 8;
    if remainder == 0 {
        remainder = 8;
    }
    if mini_buffer > 0 {
        compressed[..].copy_from_slice(&mini_buffer.to_le_bytes()[..remainder]);
    }
    target
}

fn unpack_nonces(pow: &[u8], edge_bits: u8, expected_length: usize) -> Result<Vec<u64>, CuckarooVerificationError> {
    let mut nonces = Vec::with_capacity(expected_length);
    let node_mask = (1u64 << edge_bits).saturating_sub(1);
    let mut mini_buffer = 0u64;
    let mut remaining = 64u8;
    let bytes = pow.iter().copied();
    for byte in bytes {
        mini_buffer |= u64::from(byte) << 64u8.saturating_sub(remaining);
        remaining = remaining.saturating_sub(8);
        while remaining <= 64u8.saturating_sub(edge_bits) {
            let nonce = mini_buffer & node_mask;
            if nonce > node_mask {
                return Err(CuckarooVerificationError::NonceTooLarge);
            }
            nonces.push(nonce);
            mini_buffer >>= edge_bits;
            remaining = remaining.saturating_add(edge_bits);
        }
    }

    for n in nonces.get(expected_length..).expect("Already checked") {
        if *n != 0 {
            return Err(CuckarooVerificationError::PowDataContainsNonZeroPadding);
        }
    }
    Ok(nonces.into_iter().take(expected_length).collect())
}

fn verify(
    siphash_keys: &[u64; 4],
    nonces: &[u64],
    cycle_length: NonZeroUsize,
    edge_bits: u8,
    bipartite: bool,
) -> Result<(), CuckarooVerificationError> {
    let uvs = generate_edges(siphash_keys, edge_bits, cycle_length, nonces)?;
    // Verify the cycle from the edges. Below a network's activation height the merged-namespace verifier that
    // accepted the historical chain is used; at and above it the bipartite reference port is used
    // (GHSA-3qmx-q9pv-f3m4).
    if bipartite {
        verify_from_edges_bipartite(&uvs, cycle_length)
    } else {
        verify_from_edges_legacy(&uvs, cycle_length)
    }
}

fn generate_edges(
    siphash_keys: &[u64; 4],
    edge_bits: u8,
    cycle_length: NonZeroUsize,
    nonces: &[u64],
) -> Result<Vec<(u64, u64)>, CuckarooVerificationError> {
    let node_mask = (1u64 << edge_bits).saturating_sub(1);
    let mut uvs = Vec::with_capacity(cycle_length.get());
    for i in 0..cycle_length.get() {
        if *nonces.get(i).expect("Already checked") > node_mask {
            return Err(CuckarooVerificationError::NonceTooLarge);
        }
        if i > 0 &&
            *nonces.get(i).expect("Already checked") <= *nonces.get(i.saturating_sub(1)).expect("Already checked")
        {
            return Err(CuckarooVerificationError::NoncesNotAscending);
        }

        // Use false here, to match original cuckaroo
        let edge = siphash_block(siphash_keys, *nonces.get(i).expect("Already checked"), 21);
        let u = edge & node_mask;
        let v = (edge >> 32) & node_mask;

        uvs.push((u, v));
    }

    Ok(uvs)
}

/// Pre-fork verifier. Merges the U and V endpoint namespaces (GHSA-3qmx-q9pv-f3m4).
///
/// Retained unchanged so blocks below each network's `bipartite_cuckaroo_verification` activation height still
/// validate exactly as they did when they were mined. Do not "fix" this function: every historical c29 block on
/// every network was accepted by it, and changing it would invalidate them.
fn verify_from_edges_legacy(uvs: &[(u64, u64)], cycle_length: NonZeroUsize) -> Result<(), CuckarooVerificationError> {
    let proof_size = uvs.len();
    if proof_size != cycle_length.get() {
        if proof_size > cycle_length.get() {
            return Err(CuckarooVerificationError::CycleTooLong);
        }
        return Err(CuckarooVerificationError::CycleTooShort);
    }

    // Step 1: Generate edges and build adjacency list
    let mut graph: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut xor_sum = 0;

    for (u, v) in uvs.iter().take(cycle_length.get()).copied() {
        graph.entry(u).or_default().push(v);
        graph.entry(v).or_default().push(u);

        xor_sum ^= u ^ v;
    }
    // Each node should appear exactly twice in the edges
    if xor_sum != 0 {
        return Err(CuckarooVerificationError::EndpointsDontMatch);
    }

    for neighbors in graph.values() {
        if neighbors.len() != 2 {
            return Err(CuckarooVerificationError::NodeHasMoreThanTwoEdges);
        }
    }
    // Walk the cycle

    let mut visited_edges = HashSet::new();
    let mut visited_nodes = HashSet::new();
    let mut current = *graph.keys().next().expect("Graph cannot be empty");
    let start_node = current;
    let mut previous = None;

    for _ in 0..cycle_length.get() {
        if visited_nodes.contains(&current) {
            return Err(CuckarooVerificationError::CycleDidNotUseAllEdges);
        }
        visited_nodes.insert(current);
        let neighbors = graph.get(&current).expect("Already checked");

        // Choose next that is not the previous node
        let next = if Some(*neighbors.first().expect("Already checked")) == previous {
            *neighbors.get(1).expect("Already checked")
        } else {
            *neighbors.first().expect("Already checked")
        };

        let edge_key = if current < next {
            (current, next)
        } else {
            (next, current)
        };

        if visited_edges.contains(&edge_key) {
            return Err(CuckarooVerificationError::EdgeAlreadyVisited);
        }
        visited_edges.insert(edge_key);

        previous = Some(current);
        current = next;
    }

    if current != start_node {
        return Err(CuckarooVerificationError::CycleDoesNotEndAtStart);
    }

    if visited_edges.len() != cycle_length.get() {
        return Err(CuckarooVerificationError::CycleDidNotUseAllEdges);
    }

    Ok(())
}

/// Post-fork verifier. Port of `ref_verify()` from tari-project/TARI.Miner (`tari_c29.cpp:69-95`), itself from
/// `mimblewimble/grin` `core/src/pow/cuckaroo.rs`.
///
/// U endpoints live at even indices of `flat`, V endpoints at odd. Index parity *is* the partition: `k = (k + 2)`
/// only ever compares a U endpoint against another U endpoint, and `j ^ 1` crosses to the partner endpoint of the
/// same edge, so the walk alternates sides.
///
/// The body is kept in line-by-line correspondence with the reference, which is why the indexing and arithmetic
/// lints are allowed here rather than the expressions being rewritten: `i`, `j` and `k` are all reduced modulo
/// `2 * proof_size` and `flat` has exactly `2 * proof_size` elements, `proof_size` is `NonZeroUsize`-derived and
/// bounded by the 42 edge cycle length, and `n` is bounded by the `i == 0` loop exit.
#[allow(clippy::indexing_slicing)]
#[allow(clippy::arithmetic_side_effects)]
fn verify_from_edges_bipartite(
    uvs: &[(u64, u64)],
    cycle_length: NonZeroUsize,
) -> Result<(), CuckarooVerificationError> {
    let proof_size = uvs.len();
    if proof_size != cycle_length.get() {
        return Err(if proof_size > cycle_length.get() {
            CuckarooVerificationError::CycleTooLong
        } else {
            CuckarooVerificationError::CycleTooShort
        });
    }

    let mut flat = Vec::with_capacity(proof_size.saturating_mul(2));
    let (mut xor0, mut xor1) = (0u64, 0u64);
    for (u, v) in uvs.iter().copied() {
        xor0 ^= u;
        xor1 ^= v;
        flat.push(u);
        flat.push(v);
    }
    // Each side must cancel independently. `xor0 ^ xor1 == 0` is strictly weaker and is the bug.
    if xor0 != 0 || xor1 != 0 {
        return Err(CuckarooVerificationError::EndpointsDontMatch);
    }

    let (mut n, mut i) = (0usize, 0usize);
    loop {
        let mut j = i;
        let mut k = i;
        loop {
            k = (k + 2) % (2 * proof_size);
            if k == i {
                break;
            }
            if flat[k] == flat[i] {
                if j != i {
                    return Err(CuckarooVerificationError::NodeHasMoreThanTwoEdges);
                }
                j = k;
            }
        }
        if j == i {
            return Err(CuckarooVerificationError::CycleDidNotUseAllEdges);
        }
        i = j ^ 1;
        n += 1;
        if i == 0 {
            break;
        }
    }
    if n != proof_size {
        return Err(CuckarooVerificationError::CycleTooShort);
    }
    Ok(())
}

pub fn cuckaroo_difficulty(
    header: &BlockHeader,
    required_cycle_length: u8,
    num_bits: u8,
    bipartite: bool,
) -> Result<Difficulty, CuckarooVerificationError> {
    let difficulty = cuckaroo_result(header, required_cycle_length, num_bits, bipartite)?;
    Ok(Difficulty::big_endian_difficulty(&difficulty)?)
}

/// Audit helper: run *both* Cuckaroo verifiers over `header` and return `(legacy, bipartite)`.
///
/// This is how the merged-namespace fixtures in this module were produced and how `examples/audit_c29.rs` finds
/// GHSA-3qmx-q9pv-f3m4 proofs in an existing chain. The legacy result is the control: the node accepted every
/// historical c29 header, so a legacy `Err` means the caller reconstructed the header wrong - most often by not
/// calling `Network::set_current` before `mining_hash()`, because the header hasher is network scoped.
#[cfg(any(test, feature = "c29_audit"))]
pub fn cuckaroo_audit_bipartite(
    header: &BlockHeader,
    required_cycle_length: u8,
    edge_bits: u8,
) -> (
    Result<Vec<u8>, CuckarooVerificationError>,
    Result<Vec<u8>, CuckarooVerificationError>,
) {
    (
        cuckaroo_result(header, required_cycle_length, edge_bits, false),
        cuckaroo_result(header, required_cycle_length, edge_bits, true),
    )
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    // Test-only: an overflow panic while building a fixture is the desired failure mode.
    #![allow(clippy::arithmetic_side_effects)]

    use tari_common::configuration::Network;
    use tari_common_types::types::FixedHash;
    use tari_transaction_components::{
        consensus::ConsensusConstantsBuilder,
        tari_proof_of_work::{PowAlgorithm, ProofOfWork},
    };

    use super::*;
    use crate::{
        consensus::BaseNodeConsensusManager,
        proof_of_work::{AdjustedTarget, randomx_factory::RandomXFactory},
        validation::{ValidationError, helpers::check_target_difficulty},
    };

    const KEYS: [u64; 4] = [123u64, 123u64, 234u64, 23423u64];

    #[test]
    fn test_pack_nonces() {
        let nonces = vec![0, 1, 2, 3];
        let edge_bits = 3;
        let packed = pack_nonces(&nonces, edge_bits);
        assert_eq!(packed.len(), 2);
        assert_eq!(packed[0], 0b10001000);
        assert_eq!(packed[1], 0b00000110);

        let actual = unpack_nonces(&packed, edge_bits, nonces.len()).unwrap();

        assert_eq!(&actual, &nonces);
    }

    #[test]
    fn test_unpack_nonces_with_nonzero_padding() {
        let packed = vec![0b10001000, 0b00000110, 0b11000000];
        let edge_bits = 3;
        let result = unpack_nonces(&packed, edge_bits, 4);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            CuckarooVerificationError::PowDataContainsNonZeroPadding
        );
    }

    #[test]
    fn test_pack_nonces_29_bits() {
        let nonces = vec![2u64.pow(29) - 1];

        let edge_bits = 29;
        let packed = pack_nonces(&nonces, edge_bits);
        assert_eq!(packed.len(), 4);
        assert_eq!(packed[0], 0b11111111);
        assert_eq!(packed[1], 0b11111111);
        assert_eq!(packed[2], 0b11111111);
        assert_eq!(packed[3], 0b00011111);

        let actual = unpack_nonces(&packed, edge_bits, nonces.len()).unwrap();

        assert_eq!(&actual, &nonces);
    }

    #[test]
    fn test_pack_nonces_29_bits2() {
        let nonces = vec![2u64.pow(29) - 1, 0, 2u64.pow(29) - 1, 0];

        let edge_bits = 29;
        let packed = pack_nonces(&nonces, edge_bits);
        assert_eq!(packed.len(), 15);
        assert_eq!(packed[0], 0b11111111);
        assert_eq!(packed[1], 0b11111111);
        assert_eq!(packed[2], 0b11111111);
        assert_eq!(packed[3], 0b00011111);
        assert_eq!(packed[4], 0b00000000);
        assert_eq!(packed[5], 0b00000000);
        assert_eq!(packed[6], 0b00000000);
        assert_eq!(packed[7], 0b11111100);
        assert_eq!(packed[8], 0b11111111);
        assert_eq!(packed[9], 0b11111111);
        assert_eq!(packed[10], 0b01111111);
        assert_eq!(packed[11], 0b00000000);
        assert_eq!(packed[12], 0b00000000);
        assert_eq!(packed[13], 0b00000000);
        assert_eq!(packed[14], 0b00000000);

        let actual = unpack_nonces(&packed, edge_bits, nonces.len()).unwrap();

        assert_eq!(&actual, &nonces);
    }

    #[test]
    fn test_cuckaroo_nonce_too_large() {
        let nonces = vec![0, 127, 128];
        let cycle_length = NonZeroUsize::new(3).unwrap();
        let edge_bits = 7;
        let result = verify(&KEYS, &nonces, cycle_length, edge_bits, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NonceTooLarge);
    }

    #[test]
    fn test_cuckaroo_nonces_not_ascending_1() {
        let nonces = vec![0, 127, 127];
        let result = verify(&KEYS, &nonces, NonZeroUsize::new(3).unwrap(), 7, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NoncesNotAscending)
    }

    #[test]
    fn test_cuckaroo_nonces_not_ascending_2() {
        let nonces = vec![0, 127, 126];

        let result = verify(&KEYS, &nonces, NonZeroUsize::new(3).unwrap(), 7, false);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NoncesNotAscending)
    }

    #[test]
    fn test_cuckaroo_verify_endpoints_dont_match() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 4)];
        let result = verify_from_edges_legacy(&uvs, NonZeroUsize::new(4).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::EndpointsDontMatch);
    }
    #[test]
    fn test_cuckaroo_verify_from_edges() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 0)];
        let result = verify_from_edges_legacy(&uvs, NonZeroUsize::new(4).unwrap());

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_cuckaroo_verify_from_edges_out_of_order() {
        let uvs = vec![(0, 1), (1, 2), (3, 0), (2, 3)];
        let result = verify_from_edges_legacy(&uvs, NonZeroUsize::new(4).unwrap());

        assert_eq!(result, Ok(()));

        use rand::prelude::SliceRandom;

        let mut uvs = uvs;
        uvs.shuffle(&mut rand::rng());
        let result = verify_from_edges_legacy(&uvs, NonZeroUsize::new(4).unwrap());
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_cuckaroo_cycle_too_short() {
        let uvs = vec![(0, 1), (1, 2)];
        let result = verify_from_edges_legacy(&uvs, NonZeroUsize::new(3).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::CycleTooShort);
    }

    #[test]
    fn test_cuckaroo_cycle_too_long() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)];
        let result = verify_from_edges_legacy(&uvs, NonZeroUsize::new(4).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::CycleTooLong);
    }

    #[ignore = "This test is ignored because it is caught be NodeHasMoreThanTwoEdges"]
    #[test]
    fn test_cuckaroo_edge_already_visited() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (0, 1), (3, 0), (1, 0)];
        let result = verify_from_edges_legacy(&uvs, NonZeroUsize::new(6).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::EdgeAlreadyVisited);
    }

    #[test]
    fn test_cuckaroo_node_has_more_than_two_edges() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 0), (0, 2), (2, 0)];
        let result = verify_from_edges_legacy(&uvs, NonZeroUsize::new(6).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NodeHasMoreThanTwoEdges);
    }

    #[test]
    fn test_header_hash() {
        let header_before_blake =
            hex::decode("4dbfee3eb7b9a6a27d2a4a8d754eb77cc5493006945c1246e50e9beb4de5ffa5").unwrap();

        assert_eq!(header_before_blake.len(), 32);
        let xn = hex::decode("9d589ed597ed42d1").unwrap();
        let nonce: u64 = u64::from_be_bytes(xn.try_into().expect("Cannot fail"));

        let hash = determine_sip_hash(&header_before_blake, nonce);
        assert_eq!(
            hex::encode(hash),
            "49b48f77df94943cf3a422c5a0b528c737cc38a7b6c36076e81abcede5b2be3a"
        );
    }

    #[test]
    fn test_unpack_example() {
        let packed_nonces = hex::decode("ab3c742104de5808220f0ebd1d24e2a279489c9fa8c9264f754181433226655286c69a08f166e523283813e5eceabbec042598193013a34cd966601e064d5f3dbb911efe39536e7847f3180071865023e18c6c1c627696aece2ff401938abacc8ed2f446b8ba71785b074a086d36d1d61d638dc0eab156b8883214cbff9962f199b96c92349df3d1d7b647ed0cdf6dfde1e49077bcfb74b503").unwrap();
        let res = unpack_nonces(&packed_nonces, 29, 42).unwrap();
        assert_eq!(res, vec![
            24394923, 46592033, 58968194, 71842682, 75995694, 81022926, 97860763, 105410600, 106063142, 138729012,
            150559164, 170291280, 197045966, 202539638, 206356582, 215689620, 218504800, 232385274, 243238820,
            250666150, 268537652, 296239928, 296891268, 315542595, 338677422, 341088271, 346272558, 359697897,
            364349211, 377758979, 391857369, 403811427, 408334826, 413242437, 413564914, 426980322, 433277222,
            460056825, 473150750, 473935291, 477597924, 497788893
        ]);
    }

    #[test]
    fn test_edge_generation_example1() {
        let edge_nonce = vec![24394923];
        let sip_hash_keys = hex::decode("a216826b5d2752ccf129eef73e1e02a6b56d13195d7998e6b04d02a136b88f4f").unwrap();
        let sip_hash_keys = [
            u64::from_le_bytes(
                sip_hash_keys
                    .get(0..8)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(8..16)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(16..24)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(24..32)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
        ];
        let res = generate_edges(&sip_hash_keys, 29, NonZeroUsize::new(1).unwrap(), &edge_nonce).unwrap();
        // assert_eq!(res, vec![(193904592, 244315134)]);
        assert_eq!(res, vec![(259523165, 501211281)]);
    }

    #[test]
    fn test_edge_generation_example2() {
        let edge_nonces: Vec<u64> = vec![
            24394923, 46592033, 58968194, 71842682, 75995694, 81022926, 97860763, 105410600, 106063142, 138729012,
            150559164, 170291280, 197045966, 202539638, 206356582, 215689620, 218504800, 232385274, 243238820,
            250666150, 268537652, 296239928, 296891268, 315542595, 338677422, 341088271, 346272558, 359697897,
            364349211, 377758979, 391857369, 403811427, 408334826, 413242437, 413564914, 426980322, 433277222,
            460056825, 473150750, 473935291, 477597924, 497788893,
        ];

        let sip_hash_keys = hex::decode("a216826b5d2752ccf129eef73e1e02a6b56d13195d7998e6b04d02a136b88f4f").unwrap();
        let sip_hash_keys = [
            u64::from_le_bytes(
                sip_hash_keys
                    .get(0..8)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(8..16)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(16..24)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(24..32)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
        ];
        let res = generate_edges(&sip_hash_keys, 29, NonZeroUsize::new(42).unwrap(), &edge_nonces).unwrap();
        assert_eq!(res, vec![
            (259523165, 501211281),
            (157516326, 386490295),
            (163049712, 357352750),
            (527359151, 457953146),
            (43269845, 334867286),
            (43269845, 341998749),
            (450586946, 355116366),
            (117672975, 137509934),
            (269707614, 87839857),
            (117672975, 78269931),
            (139445939, 87839857),
            (157516326, 306322638),
            (58908345, 390892394),
            (269707614, 36056555),
            (101786071, 18252563),
            (527359151, 482342291),
            (356633100, 306322638),
            (216244743, 373890488),
            (81971272, 357352750),
            (521344355, 341998749),
            (424927616, 534790392),
            (302985768, 390892394),
            (81971272, 386490295),
            (90433930, 262615890),
            (424927616, 10110110),
            (259523165, 18252563),
            (163049712, 482342291),
            (101786071, 334867286),
            (58908345, 228032021),
            (216244743, 262615890),
            (444797260, 137509934),
            (90433930, 78269931),
            (521344355, 228032021),
            (302985768, 469085984),
            (356633100, 469085984),
            (139445939, 534790392),
            (242580855, 373890488),
            (242580855, 501211281),
            (450586946, 36056555),
            (45078436, 457953146),
            (444797260, 355116366),
            (45078436, 10110110)
        ]);
    }

    #[test]
    fn test_edge_generation_example3() {
        let edge_nonces: Vec<u64> = vec![147341567];

        let blob = hex::decode("93e43a39b44f0875af830b6fd4cc69a421b1f5f0c5efd1b6c2e16d439bfd238c").unwrap();
        let nonce = hex::decode("ab30dc99fd054f57").unwrap();
        let nonce: u64 = u64::from_be_bytes(nonce.try_into().unwrap());

        let sip_hash_keys = determine_sip_hash(&blob, nonce);
        assert_eq!(
            hex::encode(&sip_hash_keys),
            "094cf89614ed5ee03b1454881361110fc380e9c4072343f5d24b9e2597b0da96"
        );

        // let sip_hash_keys = hex::decode("a216826b5d2752ccf129eef73e1e02a6b56d13195d7998e6b04d02a136b88f4f").unwrap();
        let sip_hash_keys = [
            u64::from_le_bytes(
                sip_hash_keys
                    .get(0..8)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(8..16)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(16..24)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(24..32)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
        ];
        let res = generate_edges(&sip_hash_keys, 29, NonZeroUsize::new(1).unwrap(), &edge_nonces).unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res, vec![(361363154, 255932004)]);
    }

    #[test]
    fn test_edge_generation_example4() {
        let edge_nonces: Vec<u64> = vec![
            1252665, 4516819, 17265865, 24709305, 89445155, 99203810, 108059490, 118448252, 126732226, 147341567,
            162827037, 177849034, 183556179, 191103550, 231377681, 233321258, 237510921, 257843213, 266606591,
            282340500, 288301862, 333019502, 344766902, 355446190, 368615551, 371362415, 371729271, 380994474,
            396711753, 400948255, 402047643, 445581804, 452121724, 460915916, 464509725, 472201798, 487709959,
            488169697, 499236155, 509929107, 516834413, 534561363,
        ];

        let blob = hex::decode("93e43a39b44f0875af830b6fd4cc69a421b1f5f0c5efd1b6c2e16d439bfd238c").unwrap();
        let nonce = hex::decode("ab30dc99fd054f57").unwrap();
        let nonce: u64 = u64::from_be_bytes(nonce.try_into().unwrap());

        let sip_hash_keys = determine_sip_hash(&blob, nonce);
        assert_eq!(
            hex::encode(&sip_hash_keys),
            "094cf89614ed5ee03b1454881361110fc380e9c4072343f5d24b9e2597b0da96"
        );

        // let sip_hash_keys = hex::decode("a216826b5d2752ccf129eef73e1e02a6b56d13195d7998e6b04d02a136b88f4f").unwrap();
        let sip_hash_keys = [
            u64::from_le_bytes(
                sip_hash_keys
                    .get(0..8)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(8..16)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(16..24)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
            u64::from_le_bytes(
                sip_hash_keys
                    .get(24..32)
                    .expect("Already checked")
                    .try_into()
                    .expect("Cannot fail"),
            ),
        ];
        let res = generate_edges(&sip_hash_keys, 29, NonZeroUsize::new(42).unwrap(), &edge_nonces).unwrap();
        assert_eq!(res.len(), 42);
        assert_eq!(res, vec![
            (138864790, 173576314),
            (138864790, 183574123),
            (475775524, 272345198),
            (434305107, 118921972),
            (145538206, 491299099),
            (507636712, 57208766),
            (274489528, 183574123),
            (475775524, 86499705),
            (376165312, 307539583),
            (361363154, 255932004),
            (361363154, 491299099),
            (429018120, 525050564),
            (400305596, 173576314),
            (507636712, 471838093),
            (534308678, 484814104),
            (434305107, 104085420),
            (152178341, 255932004),
            (139457798, 471838093),
            (65921500, 78549354),
            (429018120, 461321619),
            (209554018, 29235998),
            (184129162, 78549354),
            (508864826, 118921972),
            (265651264, 386485268),
            (330070098, 178264960),
            (330070098, 461321619),
            (274489528, 386997870),
            (184129162, 272345198),
            (65921500, 525050564),
            (534308678, 86499705),
            (152178341, 492228492),
            (507444066, 178264960),
            (508864826, 492228492),
            (398316755, 307539583),
            (376165312, 29235998),
            (265651264, 386997870),
            (398316755, 104085420),
            (507444066, 386485268),
            (139457798, 21811140),
            (145538206, 484814104),
            (400305596, 21811140),
            (209554018, 57208766)
        ]);
    }

    #[test]
    fn test_solution() {
        let header_before_blake =
            hex::decode("4dbfee3eb7b9a6a27d2a4a8d754eb77cc5493006945c1246e50e9beb4de5ffa5").unwrap();

        let xn = hex::decode("9d589ed597ed42d0").unwrap();
        let nonce: u64 = u64::from_be_bytes(xn.try_into().expect("Cannot fail"));
        let packed_edge_data = hex::decode("ab3c742104de5808220f0ebd1d24e2a279489c9fa8c9264f754181433226655286c69a08f166e523283813e5eceabbec042598193013a34cd966601e064d5f3dbb911efe39536e7847f3180071865023e18c6c1c627696aece2ff401938abacc8ed2f446b8ba71785b074a086d36d1d61d638dc0eab156b8883214cbff9962f199b96c92349df3d1d7b647ed0cdf6dfde1e49077bcfb74b503").unwrap();

        let res = cuckaroo_result_inner(
            &header_before_blake,
            nonce,
            &packed_edge_data,
            NonZeroUsize::new(42).unwrap(),
            29,
            false,
        )
        .unwrap();
        let expected = hex::decode("06d52b90ccfd4db1a52cc133a46dac0dc4577343c1a27d200865a81d717a60e1").unwrap();
        assert_eq!(hex::encode(res), hex::encode(expected));
    }

    // =============================================================================================================
    // GHSA-3qmx-q9pv-f3m4: the U and V endpoint namespaces must stay distinct
    //
    // FIXTURE PROVENANCE. This repository has no live mainnet access, so the mainnet header fixtures called for by
    // the advisory could not be lifted off the chain. Where a fixture is reconstructed rather than taken from a
    // real block it says so on the fixture itself. In summary:
    //
    //   REAL      - `real_c29_edges_example2`, `real_c29_edges_example4` and `real_c29_packed_solution`: the two
    //               42 edge proofs and the packed proof already hardcoded in this module
    //               (`test_edge_generation_example2`/`example4`/`test_solution`). These came off real c29 blocks
    //               and are exactly the vectors the bug survived CI behind.
    //   REAL      - `a_reduced_parameter_forgery_over_real_siphash_edges`: a 42 edge merged-namespace cycle over
    //               *real* siphash edges at edge_bits = 20, found by `generate_reduced_parameter_forgery` below.
    //               The siphash keys are derived from a real mainnet mining hash. Only the graph size is reduced,
    //               so that the search is cheap; nothing about the edges is fabricated.
    //   SYNTHETIC - `merged_namespace_forgeries_are_rejected_only_by_the_bipartite_verifier`, standing in for the
    //               five mainnet headers drawn from the 353 affected blocks. Each carries the same signature as
    //               those blocks - `xor0 == xor1 != 0` - and exercises the same code path, but they are
    //               constructed, not mined.
    //   SYNTHETIC - the bipartite cycles built by `bipartite_cycle`, including the U/V collision case in
    //               `a_u_v_collision_is_a_false_negative_for_the_legacy_verifier`.
    // =============================================================================================================

    /// Builds the 2L edge list of a single bipartite cycle `u0 - v0 - u1 - v1 - ... - u20 - v20 - u0`.
    ///
    /// Every U value appears as `u` in exactly two edges and every V value as `v` in exactly two edges, which is
    /// the shape of a genuine Cuckaroo cycle.
    fn bipartite_cycle(us: &[u64], vs: &[u64]) -> Vec<(u64, u64)> {
        assert_eq!(us.len(), vs.len());
        let n = us.len();
        let mut edges = Vec::with_capacity(n * 2);
        for i in 0..n {
            edges.push((us[i], vs[i]));
            edges.push((us[(i + 1) % n], vs[i]));
        }
        edges
    }

    /// Builds a merged-namespace "cycle": a directed chain `(a0,a1),(a1,a2),...,(aN,a0)`.
    ///
    /// This is not a bipartite cycle at all - no U endpoint repeats among the U endpoints - but the merged
    /// adjacency map collapses each `a_i` into a single vertex of degree two, so the legacy verifier walks it as a
    /// cycle. It is the shape of the 353 forged mainnet blocks, and it always satisfies `xor0 == xor1` because
    /// both accumulators run over the same multiset of values.
    fn merged_namespace_chain(vals: &[u64]) -> Vec<(u64, u64)> {
        let n = vals.len();
        (0..n).map(|i| (vals[i], vals[(i + 1) % n])).collect()
    }

    fn l42() -> NonZeroUsize {
        NonZeroUsize::new(42).expect("42 is non-zero")
    }

    /// Inverted counterpart of `test_cuckaroo_verify_from_edges`.
    ///
    /// `[(0,1),(1,2),(2,3),(3,0)]` is a directed chain with no repeated U endpoint and no repeated V endpoint. The
    /// legacy verifier accepts it because it merges the namespaces; the reference rejects it because the walk
    /// cannot leave the first edge - "cycle dead ends".
    #[test]
    fn test_cuckaroo_verify_from_edges_bipartite() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 0)];
        assert_eq!(verify_from_edges_legacy(&uvs, NonZeroUsize::new(4).unwrap()), Ok(()));
        assert_eq!(
            verify_from_edges_bipartite(&uvs, NonZeroUsize::new(4).unwrap()),
            Err(CuckarooVerificationError::CycleDidNotUseAllEdges)
        );
    }

    /// Inverted counterpart of `test_cuckaroo_verify_from_edges_out_of_order`. Edge order is irrelevant to both
    /// verifiers, so the inversion holds under shuffling too.
    #[test]
    fn test_cuckaroo_verify_from_edges_out_of_order_bipartite() {
        let uvs = vec![(0, 1), (1, 2), (3, 0), (2, 3)];
        assert_eq!(verify_from_edges_legacy(&uvs, NonZeroUsize::new(4).unwrap()), Ok(()));
        assert!(verify_from_edges_bipartite(&uvs, NonZeroUsize::new(4).unwrap()).is_err());

        use rand::prelude::SliceRandom;
        let mut uvs = uvs;
        uvs.shuffle(&mut rand::rng());
        assert_eq!(verify_from_edges_legacy(&uvs, NonZeroUsize::new(4).unwrap()), Ok(()));
        assert!(verify_from_edges_bipartite(&uvs, NonZeroUsize::new(4).unwrap()).is_err());
    }

    /// FIXTURE 1 (SYNTHETIC - stands in for five of the 353 forged mainnet headers).
    ///
    /// Five 42 edge merged-namespace cycles. Every one of them carries the signature the mainnet audit found on
    /// all 353 affected blocks: `xor0 == xor1 != 0`, which Tari's `xor0 ^ xor1 == 0` test cannot see. The legacy
    /// verifier accepts all five; the bipartite verifier rejects all five at the XOR guard.
    #[test]
    fn merged_namespace_forgeries_are_rejected_only_by_the_bipartite_verifier() {
        let node_sets: [Vec<u64>; 5] = [
            (0..42).collect(),
            (100..142).collect(),
            (0..42).map(|i| (1u64 << 20) + i).collect(),
            (0..42).map(|i: u64| i * i + 7).collect(),
            // Values drawn from a real c29 proof's endpoint range, re-arranged into a merged-namespace chain.
            (0..42).map(|i| 259_523_165u64 + i * 1000).collect(),
        ];

        for (index, vals) in node_sets.iter().enumerate() {
            let uvs = merged_namespace_chain(vals);
            assert_eq!(uvs.len(), 42);

            // The signature: both accumulators land on the same non-zero value.
            let xor0 = uvs.iter().fold(0u64, |acc, (u, _)| acc ^ u);
            let xor1 = uvs.iter().fold(0u64, |acc, (_, v)| acc ^ v);
            assert_eq!(xor0, xor1, "fixture {index}");
            assert_ne!(xor0, 0, "fixture {index}");

            assert_eq!(
                verify_from_edges_legacy(&uvs, l42()),
                Ok(()),
                "fixture {index}: the legacy verifier must still accept this, it is why the 353 blocks are on the \
                 chain"
            );
            assert_eq!(
                verify_from_edges_bipartite(&uvs, l42()),
                Err(CuckarooVerificationError::EndpointsDontMatch),
                "fixture {index}"
            );
        }
    }

    /// FIXTURE 2 (REAL + SYNTHETIC). Legitimate c29 proofs must be accepted by *both* verifiers, otherwise the
    /// fork orphans honest miners.
    ///
    /// The first three are real: the two 42 edge proofs hardcoded elsewhere in this module and the packed proof
    /// from `test_solution`, all of which came off real c29 blocks. The last two are synthetic bipartite cycles.
    #[test]
    fn legitimate_c29_proofs_are_accepted_by_both_verifiers() {
        // -- REAL #1: the edge set of `test_edge_generation_example2` --
        let uvs = real_c29_edges_example2();
        assert_eq!(uvs.len(), 42);
        assert_eq!(verify_from_edges_legacy(&uvs, l42()), Ok(()));
        assert_eq!(verify_from_edges_bipartite(&uvs, l42()), Ok(()));

        // -- REAL #2: the edge set of `test_edge_generation_example4` --
        let uvs = real_c29_edges_example4();
        assert_eq!(uvs.len(), 42);
        assert_eq!(verify_from_edges_legacy(&uvs, l42()), Ok(()));
        assert_eq!(verify_from_edges_bipartite(&uvs, l42()), Ok(()));

        // -- REAL #3: the full packed proof from `test_solution`, through `cuckaroo_result_inner` --
        let (blake, nonce, packed) = real_c29_packed_solution();
        let expected = hex::decode("06d52b90ccfd4db1a52cc133a46dac0dc4577343c1a27d200865a81d717a60e1").unwrap();
        for bipartite in [false, true] {
            let res = cuckaroo_result_inner(&blake, nonce, &packed, l42(), 29, bipartite).unwrap();
            assert_eq!(res, expected, "bipartite = {bipartite}");
        }

        // -- SYNTHETIC #4 and #5: bipartite cycles with disjoint U and V namespaces --
        let synthetic: [(Vec<u64>, Vec<u64>); 2] = [
            ((0..21).map(|i| 1000 + i).collect(), (0..21).map(|i| 2000 + i).collect()),
            (
                (0..21).map(|i| 5_000_000 + i * 37).collect(),
                (0..21).map(|i| 9_000_000 + i * 91).collect(),
            ),
        ];
        for (index, (us, vs)) in synthetic.iter().enumerate() {
            let uvs = bipartite_cycle(us, vs);
            assert_eq!(uvs.len(), 42);
            assert_eq!(verify_from_edges_legacy(&uvs, l42()), Ok(()), "synthetic {index}");
            assert_eq!(verify_from_edges_bipartite(&uvs, l42()), Ok(()), "synthetic {index}");
        }
    }

    /// FIXTURE 3 (REAL siphash edges, reduced parameters). An end-to-end forgery: 42 ascending edge nonces whose
    /// *real* siphash-derived edges form a merged-namespace cycle at `edge_bits = 20`.
    ///
    /// The siphash keys are derived from a real mainnet mining hash (the one in `test_solution`) with header nonce
    /// 0, so nothing about the edges is fabricated; only the graph size is reduced so the search is cheap enough
    /// to run in CI. `generate_reduced_parameter_forgery` below is the search that produced these nonces.
    ///
    /// This runs through `verify`, so it covers nonce validation and edge generation as well as the cycle check.
    #[test]
    fn a_reduced_parameter_forgery_over_real_siphash_edges() {
        const EDGE_BITS: u8 = 20;
        let blob = hex::decode("4dbfee3eb7b9a6a27d2a4a8d754eb77cc5493006945c1246e50e9beb4de5ffa5").unwrap();
        let keys = keys_from(&determine_sip_hash(&blob, 0));
        let nonces: Vec<u64> = vec![
            2639, 54718, 70105, 104621, 165067, 196341, 198745, 231199, 250304, 252996, 255780, 262799, 303410, 321340,
            485499, 509862, 510657, 520785, 555257, 559871, 592568, 596753, 616176, 668113, 723720, 730138, 735058,
            735491, 737532, 760160, 773657, 782417, 782768, 798185, 807422, 812990, 841237, 849760, 852242, 951754,
            971209, 1016147,
        ];
        assert_eq!(nonces.len(), 42);

        // The edges really are siphash output, not hand-written pairs.
        let uvs = generate_edges(&keys, EDGE_BITS, l42(), &nonces).unwrap();
        assert_eq!(uvs.len(), 42);
        let xor0 = uvs.iter().fold(0u64, |acc, (u, _)| acc ^ u);
        let xor1 = uvs.iter().fold(0u64, |acc, (_, v)| acc ^ v);
        assert_eq!(xor0, xor1, "the merged-namespace signature");
        assert_ne!(xor0, 0);

        assert_eq!(
            verify(&keys, &nonces, l42(), EDGE_BITS, false),
            Ok(()),
            "this is what a node running the merged-namespace verifier accepts"
        );
        assert_eq!(
            verify(&keys, &nonces, l42(), EDGE_BITS, true),
            Err(CuckarooVerificationError::EndpointsDontMatch)
        );
    }

    /// FIXTURE 4 (SYNTHETIC). The merge is also a false-negative source, and the fix removes it for free.
    ///
    /// In a genuine bipartite 42-cycle every map key holds two entries. If any of the 21 U values numerically
    /// equals any of the 21 V values, that key collects four and the legacy verifier rejects a perfectly valid
    /// proof with `NodeHasMoreThanTwoEdges`. At edge_bits = 29 this happens with probability about
    /// `441 / 2^29`, so it has not fired on mainnet yet; deterministic, so it would not split the chain, it would
    /// just silently cost an honest miner a block.
    #[test]
    fn a_u_v_collision_is_a_false_negative_for_the_legacy_verifier() {
        let us: Vec<u64> = (0..21).map(|i| 1000 + i).collect();
        let mut vs: Vec<u64> = (0..21).map(|i| 2000 + i).collect();
        // The collision: one V endpoint numerically equals one U endpoint. Both namespaces remain internally
        // distinct, so this is a well formed Cuckaroo cycle.
        vs[10] = us[0];

        let uvs = bipartite_cycle(&us, &vs);
        assert_eq!(uvs.len(), 42);
        assert_eq!(
            verify_from_edges_legacy(&uvs, l42()),
            Err(CuckarooVerificationError::NodeHasMoreThanTwoEdges),
            "the legacy verifier merges the colliding endpoints into one degree-four vertex"
        );
        assert_eq!(
            verify_from_edges_bipartite(&uvs, l42()),
            Ok(()),
            "the cycle is bipartite-valid and must be accepted"
        );
    }

    /// FIXTURE 5 (REAL). Differential: over the genuine proofs hardcoded in this module the two verifiers must
    /// agree. They have 21 distinct U values and 21 distinct V values with no cross-partition collisions, which is
    /// exactly why the bug survived CI.
    #[test]
    fn the_two_verifiers_agree_on_the_modules_real_proof_vectors() {
        for uvs in [real_c29_edges_example2(), real_c29_edges_example4()] {
            assert_eq!(
                verify_from_edges_legacy(&uvs, l42()),
                verify_from_edges_bipartite(&uvs, l42())
            );
        }

        // And through the full packed-proof path, which is what `cuckaroo_result` runs.
        let (blake, nonce, packed) = real_c29_packed_solution();
        assert_eq!(
            cuckaroo_result_inner(&blake, nonce, &packed, l42(), 29, false),
            cuckaroo_result_inner(&blake, nonce, &packed, l42(), 29, true)
        );
    }

    /// The audit helper must run both verifiers over the same header, and must not disagree with calling them
    /// directly. `cuckaroo_result` needs a `BlockHeader`, which these unit fixtures do not have, so the agreement
    /// is pinned at the level the helper composes: `cuckaroo_result_inner`.
    #[test]
    fn the_audit_helper_reports_both_verdicts() {
        let (blake, nonce, packed) = real_c29_packed_solution();
        let legacy = cuckaroo_result_inner(&blake, nonce, &packed, l42(), 29, false);
        let bipartite = cuckaroo_result_inner(&blake, nonce, &packed, l42(), 29, true);
        assert!(legacy.is_ok());
        assert_eq!(legacy, bipartite);
    }

    // ---------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4 fork gate
    //
    // The verifier is selected by `constants.bipartite_cuckaroo_verification()` at two call sites. Nothing above
    // tests the wiring: pinning either call site to a literal `true` or `false` leaves every other test in this
    // module green while bricking historical validation, or leaving the exploit live past the fork.
    // ---------------------------------------------------------------------------------------------------------

    /// Graph size for the fork gate fixture. The live networks use 29; a 12 bit graph is small enough that a
    /// merged-namespace forgery can be found from scratch in milliseconds, so the fixture does not have to be
    /// hardcoded. It cannot be hardcoded: the siphash keys come from `BlockHeader::mining_hash()`, which is domain
    /// separated by the *current* network, and `Network::get_current_or_user_setting_or_default()` reads a process
    /// wide `OnceLock` that any other test in this binary may have set first.
    const GATE_EDGE_BITS: u8 = 12;
    /// Cycle length for the fork gate fixture. The live networks use 42; four edges is the shortest cycle that can
    /// be merged-namespace rather than bipartite, and keeps the search to a single pass over the edge list.
    const GATE_CYCLE_LENGTH: u8 = 4;

    /// A length-two path through the merged endpoint namespace: `(midpoint, first edge nonce, second edge nonce)`.
    type TwoPath = (u64, u64, u64);

    /// Finds four distinct edge nonces whose *real* siphash edges form a single cycle in the merged endpoint
    /// namespace that is not a bipartite cycle, i.e. a proof the legacy verifier accepts and the bipartite verifier
    /// rejects with `EndpointsDontMatch`. Returns the nonces in ascending order.
    ///
    /// Two distinct length-two paths between the same pair of endpoints are a four cycle. Every node in it has
    /// degree exactly two, so the endpoint checksum cancels and the legacy verifier's walk closes, which is the
    /// whole of what the legacy verifier checks.
    fn find_merged_namespace_forgery(keys: &[u64; 4]) -> Option<Vec<u64>> {
        let mask: u64 = (1u64 << GATE_EDGE_BITS) - 1;
        let cycle = NonZeroUsize::new(GATE_CYCLE_LENGTH as usize).expect("non-zero");

        let mut adjacency: HashMap<u64, Vec<(u64, u64)>> = HashMap::new();
        for nonce in 0..=mask {
            let edge = siphash_block(keys, nonce, 21);
            let (u, v) = (edge & mask, (edge >> 32) & mask);
            if u == v {
                continue;
            }
            adjacency.entry(u).or_default().push((v, nonce));
            adjacency.entry(v).or_default().push((u, nonce));
        }

        // (unordered endpoint pair) -> the length-two paths already seen between them, as (midpoint, nonces)
        let mut paths: HashMap<(u64, u64), Vec<TwoPath>> = HashMap::new();
        let mut midpoints: Vec<u64> = adjacency.keys().copied().collect();
        midpoints.sort_unstable();
        for midpoint in midpoints {
            let neighbours = adjacency.get(&midpoint).expect("key came from the map").clone();
            for i in 0..neighbours.len() {
                for j in (i + 1)..neighbours.len() {
                    let (x, nonce_x) = neighbours[i];
                    let (y, nonce_y) = neighbours[j];
                    if x == y {
                        continue;
                    }
                    let key = if x < y { (x, y) } else { (y, x) };
                    let seen = paths.entry(key).or_default();
                    for &(other_midpoint, nonce_a, nonce_b) in seen.iter() {
                        if other_midpoint == midpoint {
                            continue;
                        }
                        let mut nonces = vec![nonce_x, nonce_y, nonce_a, nonce_b];
                        nonces.sort_unstable();
                        nonces.dedup();
                        if nonces.len() != GATE_CYCLE_LENGTH as usize {
                            continue;
                        }
                        // Keep only the forgeries: accepted by the merged-namespace verifier, rejected by the
                        // bipartite one *at the endpoint checksum*. A four cycle that happens to alternate U and V
                        // endpoints is a genuine Cuckaroo cycle and both verifiers accept it, which would not
                        // distinguish anything. Requiring the exact error matters as well as requiring an error:
                        // a merged four cycle whose four U endpoints are all distinct can still XOR to zero on
                        // both sides at this graph size (measured at about 3e-5 of forgeries), in which case it
                        // clears the XOR guard and the walk rejects it with `CycleDidNotUseAllEdges` instead.
                        // Accepting those here and then asserting `EndpointsDontMatch` in the test would be a rare
                        // flake, so the search and the assertion are held to the same set.
                        if verify(keys, &nonces, cycle, GATE_EDGE_BITS, false) == Ok(()) &&
                            verify(keys, &nonces, cycle, GATE_EDGE_BITS, true) ==
                                Err(CuckarooVerificationError::EndpointsDontMatch)
                        {
                            return Some(nonces);
                        }
                    }
                    seen.push((midpoint, nonce_x, nonce_y));
                }
            }
        }
        None
    }

    /// Grinds the header nonce until the header carries a merged-namespace forgery, and writes the forged proof
    /// into `header.pow`. The header's own fields are left alone, so the caller's height is what is verified.
    fn grind_merged_namespace_forgery(header: &mut BlockHeader) {
        let mining_hash = header.mining_hash();
        for header_nonce in 0..512u64 {
            let keys = keys_from(&determine_sip_hash(mining_hash.as_slice(), header_nonce));
            if let Some(nonces) = find_merged_namespace_forgery(&keys) {
                header.nonce = header_nonce;
                header.pow = ProofOfWork {
                    pow_algo: PowAlgorithm::Cuckaroo,
                    pow_data: pack_nonces(&nonces, GATE_EDGE_BITS).try_into().expect("small"),
                };
                return;
            }
        }
        panic!("no merged-namespace forgery found in 512 header nonces");
    }

    /// The fork gate must be driven by the block height, through the consensus constants, at the real call site.
    ///
    /// `check_target_difficulty` is how header sync, block body validation, reorg and `DifficultyCalculator` all
    /// reach `cuckaroo_difficulty`. This builds a `BaseNodeConsensusManager` whose bipartite activation height is
    /// `ACTIVATION_HEIGHT` and puts the *same kind of* forged c29 proof through it at `ACTIVATION_HEIGHT - 1` and
    /// at `ACTIVATION_HEIGHT`. It must be accepted below the height and rejected at it.
    ///
    /// This fails if `validation/helpers.rs` passes a literal instead of `constants.bipartite_cuckaroo_verification()`
    /// (either literal breaks one of the two halves), and it fails if the activation lookup is off by one in either
    /// direction.
    ///
    /// The other call site, `base_node::comms_interface::inbound_handlers::check_min_block_difficulty`, is a private
    /// async method behind a full `InboundNodeCommsHandlers` and is not covered here.
    #[test]
    fn the_fork_gate_selects_the_cuckaroo_verifier_by_block_height() {
        const ACTIVATION_HEIGHT: u64 = 500;

        let entry = |bipartite: bool, effective_from: u64| {
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_cuckaroo_edge_bits(GATE_EDGE_BITS)
                .with_cuckaroo_cycle_length(GATE_CYCLE_LENGTH)
                .with_bipartite_cuckaroo_verification(bipartite)
                .with_effective_from_height(effective_from)
                .build()
        };
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(entry(false, 0))
            .add_consensus_constants(entry(true, ACTIVATION_HEIGHT))
            .build()
            .expect("local net rules");

        let target = AdjustedTarget::unadjusted(Difficulty::min());
        let randomx_factory = RandomXFactory::default();

        // Below the activation height the merged-namespace verifier is still in force, so the forgery is accepted.
        let mut before = BlockHeader::new(0);
        before.height = ACTIVATION_HEIGHT - 1;
        grind_merged_namespace_forgery(&mut before);
        check_target_difficulty(
            &before,
            target,
            &randomx_factory,
            &FixedHash::zero(),
            &rules,
            FixedHash::zero(),
        )
        .expect("a merged-namespace proof is still valid below the activation height");

        // At the activation height the same kind of forgery is rejected. This header is `before` with the height
        // moved up by one; the grind only has to re-find a forgery because the height feeds the mining hash.
        let mut at = before.clone();
        at.height = ACTIVATION_HEIGHT;
        grind_merged_namespace_forgery(&mut at);
        let err = check_target_difficulty(
            &at,
            target,
            &randomx_factory,
            &FixedHash::zero(),
            &rules,
            FixedHash::zero(),
        )
        .expect_err("a merged-namespace proof must be rejected at the activation height");
        assert!(
            matches!(
                err,
                ValidationError::CuckarooPowError(CuckarooVerificationError::EndpointsDontMatch)
            ),
            "expected the bipartite endpoint checksum to reject it, got {err:?}"
        );

        // Control: the two headers really are the same proof as far as the verifier is concerned, so the split
        // above is the height and nothing else.
        for header in [&before, &at] {
            assert_eq!(
                cuckaroo_result(header, GATE_CYCLE_LENGTH, GATE_EDGE_BITS, true),
                Err(CuckarooVerificationError::EndpointsDontMatch)
            );
            assert!(cuckaroo_result(header, GATE_CYCLE_LENGTH, GATE_EDGE_BITS, false).is_ok());
        }
    }

    /// Parses a 32 byte siphash seed into the four `u64` keys, the way `cuckaroo_result_inner` does.
    fn keys_from(bytes: &[u8]) -> [u64; 4] {
        [
            u64::from_le_bytes(bytes[0..8].try_into().expect("32 bytes")),
            u64::from_le_bytes(bytes[8..16].try_into().expect("32 bytes")),
            u64::from_le_bytes(bytes[16..24].try_into().expect("32 bytes")),
            u64::from_le_bytes(bytes[24..32].try_into().expect("32 bytes")),
        ]
    }

    /// REAL. The 42 edges of `test_edge_generation_example2`, regenerated from that test's real siphash keys and
    /// edge nonces rather than copied, so the fixture cannot drift from the vector it is derived from.
    fn real_c29_edges_example2() -> Vec<(u64, u64)> {
        let sip = hex::decode("a216826b5d2752ccf129eef73e1e02a6b56d13195d7998e6b04d02a136b88f4f").unwrap();
        let nonces: Vec<u64> = vec![
            24394923, 46592033, 58968194, 71842682, 75995694, 81022926, 97860763, 105410600, 106063142, 138729012,
            150559164, 170291280, 197045966, 202539638, 206356582, 215689620, 218504800, 232385274, 243238820,
            250666150, 268537652, 296239928, 296891268, 315542595, 338677422, 341088271, 346272558, 359697897,
            364349211, 377758979, 391857369, 403811427, 408334826, 413242437, 413564914, 426980322, 433277222,
            460056825, 473150750, 473935291, 477597924, 497788893,
        ];
        generate_edges(&keys_from(&sip), 29, l42(), &nonces).expect("real proof")
    }

    /// REAL. The 42 edges of `test_edge_generation_example4`, regenerated from that test's real mining hash, header
    /// nonce and edge nonces.
    fn real_c29_edges_example4() -> Vec<(u64, u64)> {
        let blob = hex::decode("93e43a39b44f0875af830b6fd4cc69a421b1f5f0c5efd1b6c2e16d439bfd238c").unwrap();
        let nonce = u64::from_be_bytes(hex::decode("ab30dc99fd054f57").unwrap().try_into().unwrap());
        let sip = determine_sip_hash(&blob, nonce);
        let nonces: Vec<u64> = vec![
            1252665, 4516819, 17265865, 24709305, 89445155, 99203810, 108059490, 118448252, 126732226, 147341567,
            162827037, 177849034, 183556179, 191103550, 231377681, 233321258, 237510921, 257843213, 266606591,
            282340500, 288301862, 333019502, 344766902, 355446190, 368615551, 371362415, 371729271, 380994474,
            396711753, 400948255, 402047643, 445581804, 452121724, 460915916, 464509725, 472201798, 487709959,
            488169697, 499236155, 509929107, 516834413, 534561363,
        ];
        generate_edges(&keys_from(&sip), 29, l42(), &nonces).expect("real proof")
    }

    /// REAL. The `(mining hash, header nonce, packed edge data)` triple of `test_solution`.
    fn real_c29_packed_solution() -> (Vec<u8>, u64, Vec<u8>) {
        let blake = hex::decode("4dbfee3eb7b9a6a27d2a4a8d754eb77cc5493006945c1246e50e9beb4de5ffa5").unwrap();
        let nonce = u64::from_be_bytes(hex::decode("9d589ed597ed42d0").unwrap().try_into().unwrap());
        let packed = hex::decode("ab3c742104de5808220f0ebd1d24e2a279489c9fa8c9264f754181433226655286c69a08f166e523283813e5eceabbec042598193013a34cd966601e064d5f3dbb911efe39536e7847f3180071865023e18c6c1c627696aece2ff401938abacc8ed2f446b8ba71785b074a086d36d1d61d638dc0eab156b8883214cbff9962f199b96c92349df3d1d7b647ed0cdf6dfde1e49077bcfb74b503").unwrap();
        (blake, nonce, packed)
    }

    // ---------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4 fixture generator (ignored; run explicitly, in release)
    // ---------------------------------------------------------------------------------------------------------

    #[test]
    #[ignore = "fixture generator"]
    fn generate_reduced_parameter_forgery() {
        use std::collections::HashMap as Map;
        const EB: u8 = 20;
        const L: usize = 42;
        let blob = hex::decode("4dbfee3eb7b9a6a27d2a4a8d754eb77cc5493006945c1246e50e9beb4de5ffa5").unwrap();
        let mask: u64 = (1u64 << EB) - 1;

        for header_nonce in 0u64..64 {
            let kb = determine_sip_hash(&blob, header_nonce);
            let keys = [
                u64::from_le_bytes(kb[0..8].try_into().unwrap()),
                u64::from_le_bytes(kb[8..16].try_into().unwrap()),
                u64::from_le_bytes(kb[16..24].try_into().unwrap()),
                u64::from_le_bytes(kb[24..32].try_into().unwrap()),
            ];
            // merged-namespace adjacency: node -> Vec<(neighbour, edge nonce)>
            let mut adj: Map<u64, Vec<(u64, u64)>> = Map::new();
            for nonce in 0..=mask {
                let edge = siphash_block(&keys, nonce, 21);
                let u = edge & mask;
                let v = (edge >> 32) & mask;
                if u == v {
                    continue;
                }
                adj.entry(u).or_default().push((v, nonce));
                adj.entry(v).or_default().push((u, nonce));
            }
            let mut starts: Vec<u64> = adj.keys().copied().collect();
            starts.sort_unstable();

            for &start in starts.iter().take(400) {
                if let Some(nonces) = find_cycle(&adj, start, L) {
                    let mut nonces = nonces;
                    nonces.sort_unstable();
                    println!("HEADER_NONCE = {header_nonce}");
                    println!("NONCES = {nonces:?}");
                    let legacy = verify(&keys, &nonces, NonZeroUsize::new(L).unwrap(), EB, false);
                    let bip = verify(&keys, &nonces, NonZeroUsize::new(L).unwrap(), EB, true);
                    println!("legacy = {legacy:?}   bipartite = {bip:?}");
                    assert_eq!(legacy, Ok(()));
                    assert!(bip.is_err());
                    return;
                }
            }
            println!("no cycle for header nonce {header_nonce}");
        }
        panic!("no forgery found");
    }

    fn find_cycle(adj: &std::collections::HashMap<u64, Vec<(u64, u64)>>, start: u64, len: usize) -> Option<Vec<u64>> {
        // Iterative DFS over simple paths of length `len` that return to `start`.
        let mut path_nodes: Vec<u64> = vec![start];
        let mut path_nonces: Vec<u64> = Vec::new();
        let mut on_path: HashSet<u64> = HashSet::new();
        on_path.insert(start);
        // stack of (index into the neighbour list of the node at this depth)
        let mut idx: Vec<usize> = vec![0];
        let mut budget: u64 = 40_000_000;

        while !idx.is_empty() {
            budget -= 1;
            if budget == 0 {
                return None;
            }
            let depth = idx.len() - 1;
            let node = path_nodes[depth];
            let empty = Vec::new();
            let nbrs = adj.get(&node).unwrap_or(&empty);
            let i = idx[depth];
            if i >= nbrs.len() {
                idx.pop();
                on_path.remove(&node);
                path_nodes.pop();
                path_nonces.pop();
                continue;
            }
            idx[depth] += 1;
            let (next, nonce) = nbrs[i];
            if path_nonces.contains(&nonce) {
                continue;
            }
            if depth + 1 == len {
                if next == start {
                    let mut out = path_nonces.clone();
                    out.push(nonce);
                    return Some(out);
                }
                continue;
            }
            if on_path.contains(&next) {
                continue;
            }
            on_path.insert(next);
            path_nodes.push(next);
            path_nonces.push(nonce);
            idx.push(0);
        }
        None
    }
}
