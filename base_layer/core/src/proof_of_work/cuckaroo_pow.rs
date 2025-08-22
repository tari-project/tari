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
use digest::{consts::U32, FixedOutput};
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
) -> Result<Vec<u8>, CuckarooVerificationError> {
    let pow = header.pow.to_bytes();
    let required_cycle_length = NonZeroUsize::try_from(required_cycle_length as usize)
        .map_err(|_| CuckarooVerificationError::UnsupportedCycleLength)?;
    let packed_size = required_cycle_length.get() * edge_bits as usize;
    let packed_bytes = packed_size.div_ceil(8);
    if pow.is_empty() || pow.len() < 1 + packed_bytes {
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
    )
}

fn cuckaroo_result_inner(
    header_before_nonce: &[u8],
    nonce: u64,
    packed_edge_data: &[u8],
    required_cycle_length: NonZeroUsize,
    edge_bits: u8,
) -> Result<Vec<u8>, CuckarooVerificationError> {
    let packed_size = required_cycle_length.get() * edge_bits as usize;
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
    verify(&siphash_keys, &nonces, required_cycle_length, edge_bits)?;

    // Replace the Blake2bVar hasher with Blake2b (fixed size)
    let mut hasher = Blake2b::<U32>::new();

    hasher.update(packed_edge_data);
    let res = hasher.finalize_fixed().to_vec();

    Ok(res)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
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
    let node_mask = (1u64 << edge_bits) - 1;
    let mut mini_buffer = 0u64;
    let mut remaining = 64;
    let bytes = pow.iter().copied();
    for byte in bytes {
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
) -> Result<(), CuckarooVerificationError> {
    let uvs = generate_edges(siphash_keys, edge_bits, cycle_length, nonces)?;
    // Verify the cycle from the edges
    verify_from_edges(&uvs, cycle_length)
}

fn generate_edges(
    siphash_keys: &[u64; 4],
    edge_bits: u8,
    cycle_length: NonZeroUsize,
    nonces: &[u64],
) -> Result<Vec<(u64, u64)>, CuckarooVerificationError> {
    let node_mask = (1u64 << edge_bits) - 1;
    let mut uvs = Vec::with_capacity(cycle_length.get());
    for i in 0..cycle_length.get() {
        if *nonces.get(i).expect("Already checked") > node_mask {
            return Err(CuckarooVerificationError::NonceTooLarge);
        }
        if i > 0 && *nonces.get(i).expect("Already checked") <= *nonces.get(i - 1).expect("Already checked") {
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
    #![allow(clippy::indexing_slicing)]

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

        let sip_hash_keys = determine_sip_hash(&blob, nonce.into());
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

        let sip_hash_keys = determine_sip_hash(&blob, nonce.into());
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
        )
        .unwrap();
        let expected = hex::decode("06d52b90ccfd4db1a52cc133a46dac0dc4577343c1a27d200865a81d717a60e1").unwrap();
        assert_eq!(hex::encode(res), hex::encode(expected));
    }
}
