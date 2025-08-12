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

use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use thiserror::Error;

use crate::{
    blocks::BlockHeader,
    proof_of_work::{siphash::siphash_block, Difficulty, DifficultyError},
};

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

pub fn cuckaroo_result(
    header: &BlockHeader,
    required_cycle_length: u8,
    edge_bits: u8,
) -> Result<Vec<u8>, CuckarooVerificationError> {
    let pow = header.pow.to_bytes();
    let required_cycle_length = NonZeroUsize::try_from(required_cycle_length as usize)
        .map_err(|_| CuckarooVerificationError::UnsupportedCycleLength)?;

    let packed_size = required_cycle_length.get() * edge_bits as usize;
    let packed_bytes = (packed_size + 7) / 8;

    if pow.is_empty() || pow.len() < 1 + packed_bytes {
        return Err(CuckarooVerificationError::PowDataTooShort);
    }
    // First byte must be 3 for Cuckaroo
    if pow[0] != 3 {
        return Err(CuckarooVerificationError::BlockHeaderInvalidPowAlgorithm);
    }
    let mut hasher = Blake2bVar::new(32).expect("Could not create Blake2bVar hasher");
    hasher.update(&header.nonce.to_le_bytes());
    hasher.update(header.mining_hash().as_slice());
    let mut blob = vec![0u8; hasher.output_size()];
    hasher
        .finalize_variable(&mut blob)
        .expect("Infallible because we've set the output size");

    let pow_data = &pow[1..];
    // Data after <required_cycle_length * edge_bits> is padding, it must be zero
    for &byte in &pow_data[packed_bytes..] {
        if byte != 0 {
            return Err(CuckarooVerificationError::PowDataContainsNonZeroPadding);
        }
    }

    let nonces = unpack_nonces(pow_data, edge_bits, required_cycle_length.get())?;
    // There might be extra padding at  the end of the nonces.
    for n in &nonces[required_cycle_length.get()..] {
        if *n != 0 {
            return Err(CuckarooVerificationError::PowDataContainsNonZeroPadding);
        }
    }
    let siphash_keys = [
        u64::from_le_bytes(blob[0..8].try_into().unwrap()),
        u64::from_le_bytes(blob[8..16].try_into().unwrap()),
        u64::from_le_bytes(blob[16..24].try_into().unwrap()),
        u64::from_le_bytes(blob[24..32].try_into().unwrap()),
    ];
    // // Generate the hasher.
    verify(&siphash_keys, &nonces, required_cycle_length, edge_bits)?;

    // nonces must be sorted, so we just hash them
    let mut hasher = Blake2bVar::new(32).expect("Could not create Blake2bVar hasher");

    for nonce in &nonces {
        hasher.update(&nonce.to_le_bytes());
    }
    let mut res = vec![0u8; hasher.output_size()];
    hasher
        .finalize_variable(&mut res)
        .expect("Infallible because we've set the output size");

    Ok(res)
}

#[cfg(test)]
fn pack_nonces(uncompressed: &[u64], bit_width: u8) -> Vec<u8> {
    let mut target = vec![0u8; (uncompressed.len() * bit_width as usize + 7) / 8];
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
    let node_mask = (1u64 << edge_bits) - 1;
    let mut mini_buffer = 0u64;
    let mut remaining = 64;
    let mut bytes = pow.iter().copied();
    while let Some(byte) = bytes.next() {
        mini_buffer |= u64::from(byte) << (64 - remaining);
        remaining -= 8;
        while remaining <= 64 - edge_bits {
            let nonce = mini_buffer & node_mask;
            if nonce > node_mask {
                return Err(CuckarooVerificationError::NonceTooLarge);
            }
            nonces.push(nonce);
            mini_buffer >>= edge_bits;
            remaining += edge_bits;
        }
    }

    for n in nonces[expected_length..].iter() {
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
) -> Result<(), CuckarooVerificationError> {
    let node_mask = (1u64 << edge_bits) - 1;
    let mut uvs = Vec::with_capacity(cycle_length.get());
    for i in 0..cycle_length.get() {
        if nonces[i] > node_mask {
            return Err(CuckarooVerificationError::NonceTooLarge);
        }
        if i > 0 && nonces[i] <= nonces[i - 1] {
            return Err(CuckarooVerificationError::NoncesNotAscending);
        }

        let edge = siphash_block(&siphash_keys, nonces[i], 21, true);
        let u = edge & node_mask;
        let v = (edge >> 32) & node_mask;

        uvs.push((u, v));
    }

    // Verify the cycle from the edges
    verify_from_edges(&uvs, cycle_length)
}

fn verify_from_edges(uvs: &[(u64, u64)], cycle_length: NonZeroUsize) -> Result<(), CuckarooVerificationError> {
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

    for i in 0..cycle_length.get() {
        let (u, v) = uvs[i];
        graph.entry(u).or_default().push(v);
        graph.entry(v).or_default().push(u);

        xor_sum ^= u ^ v;
    }
    // Each node should appear exactly twice in the edges
    if xor_sum != 0 {
        return Err(CuckarooVerificationError::EndpointsDontMatch);
    }

    for (_node, neighbors) in &graph {
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
        let neighbors = &graph[&current];

        // Choose next that is not the previous node
        let next = if Some(neighbors[0]) != previous {
            neighbors[0]
        } else {
            neighbors[1]
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

pub fn cuckaroo_difficulty(
    header: &BlockHeader,
    required_cycle_length: u8,
    num_bits: u8,
) -> Result<Difficulty, CuckarooVerificationError> {
    let difficulty = cuckaroo_result(header, required_cycle_length, num_bits)?;
    Ok(Difficulty::big_endian_difficulty(&difficulty)?)
}

#[cfg(test)]
mod test {

    use super::*;

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
        let result = verify(&KEYS, &nonces, cycle_length, edge_bits);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NonceTooLarge);
    }

    #[test]
    fn test_cuckaroo_nonces_not_ascending_1() {
        let nonces = vec![0, 127, 127];
        let result = verify(&KEYS, &nonces, NonZeroUsize::new(3).unwrap(), 7);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NoncesNotAscending)
    }

    #[test]
    fn test_cuckaroo_nonces_not_ascending_2() {
        let nonces = vec![0, 127, 126];

        let result = verify(&KEYS, &nonces, NonZeroUsize::new(3).unwrap(), 7);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NoncesNotAscending)
    }

    #[test]
    fn test_cuckaroo_verify_endpoints_dont_match() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 4)];
        let result = verify_from_edges(&uvs, NonZeroUsize::new(4).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::EndpointsDontMatch);
    }
    #[test]
    fn test_cuckaroo_verify_from_edges() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 0)];
        let result = verify_from_edges(&uvs, NonZeroUsize::new(4).unwrap());

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_cuckaroo_verify_from_edges_out_of_order() {
        let uvs = vec![(0, 1), (1, 2), (3, 0), (2, 3)];
        let result = verify_from_edges(&uvs, NonZeroUsize::new(4).unwrap());

        assert_eq!(result, Ok(()));

        use rand::prelude::SliceRandom;

        let mut uvs = uvs;
        uvs.shuffle(&mut rand::thread_rng());
        let result = verify_from_edges(&uvs, NonZeroUsize::new(4).unwrap());
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_cuckaroo_cycle_too_short() {
        let uvs = vec![(0, 1), (1, 2)];
        let result = verify_from_edges(&uvs, NonZeroUsize::new(3).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::CycleTooShort);
    }

    #[test]
    fn test_cuckaroo_cycle_too_long() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)];
        let result = verify_from_edges(&uvs, NonZeroUsize::new(4).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::CycleTooLong);
    }

    #[ignore = "This test is ignored because it is caught be NodeHasMoreThanTwoEdges"]
    #[test]
    fn test_cuckaroo_edge_already_visited() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (0, 1), (3, 0), (1, 0)];
        let result = verify_from_edges(&uvs, NonZeroUsize::new(6).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::EdgeAlreadyVisited);
    }

    #[test]
    fn test_cuckaroo_node_has_more_than_two_edges() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 0), (0, 2), (2, 0)];
        let result = verify_from_edges(&uvs, NonZeroUsize::new(6).unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NodeHasMoreThanTwoEdges);
    }
}
