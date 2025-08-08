//  Copyright 2025, The Tari Project
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

use std::collections::{HashMap, HashSet};

use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use thiserror::Error;

use crate::{
    blocks::BlockHeader,
    proof_of_work::{siphash::siphash_block, Difficulty},
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CuckarooVerificationError {
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
}

pub fn cuckaroo_result(
    header: &BlockHeader,
    required_cycle_length: usize,
    edge_bits: u8,
) -> Result<Vec<u8>, anyhow::Error> {
    let pow = header.pow.to_bytes();

    // First byte must be 3 for Cuckaroo
    if pow[0] != 3 {
        return Err(anyhow::anyhow!(
            CuckarooVerificationError::BlockHeaderInvalidPowAlgorithm
        ));
    }
    let mut hasher = Blake2bVar::new(32).expect("Could not create Blake2bVar hasher");
    hasher.update(&header.nonce.to_le_bytes());
    hasher.update(header.mining_hash().as_slice());
    let mut blob = Vec::with_capacity(32);
    hasher.finalize_variable(&mut blob)?;
    let mut nonces = Vec::new();

    let pow_data = &pow[1..];

    for i in 0..(pow_data.len() * 8 / edge_bits as usize) {
        let mut nonce = 0u64;
        for j in 0..edge_bits {
            nonce |= ((pow_data[i * edge_bits as usize + j as usize] as u64) & 1) << j;
        }
        nonces.push(nonce);
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
    let mut res = Vec::with_capacity(32);
    hasher.finalize_variable(&mut res)?;

    Ok(res)
}

fn verify(
    siphash_keys: &[u64; 4],
    nonces: &[u64],
    cycle_length: usize,
    edge_bits: u8,
) -> Result<(), CuckarooVerificationError> {
    let node_mask = (1u64 << edge_bits) - 1;
    let mut uvs = Vec::with_capacity(cycle_length);
    for i in 0..cycle_length {
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

fn verify_from_edges(
    // siphash_keys: &[u64; 4],
    // nonces: &[u64],
    uvs: &[(u64, u64)],
    cycle_length: usize,
) -> Result<(), CuckarooVerificationError> {
    let proof_size = uvs.len();
    if proof_size != cycle_length {
        if proof_size > cycle_length {
            return Err(CuckarooVerificationError::CycleTooLong);
        }
        return Err(CuckarooVerificationError::CycleTooShort);
    }

    // Step 1: Generate edges and build adjacency list
    let mut graph: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut xor_sum = 0;

    for i in 0..cycle_length {
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
    let mut current = *graph.keys().next().unwrap();
    let start_node = current;
    let mut previous = None;

    for _ in 0..cycle_length {
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

    if visited_edges.len() != cycle_length {
        return Err(CuckarooVerificationError::CycleDidNotUseAllEdges);
    }

    Ok(())
}

pub fn cuckaroo_difficulty(
    header: &BlockHeader,
    required_cycle_length: usize,
    num_bits: u8,
) -> Result<Difficulty, anyhow::Error> {
    let difficulty = cuckaroo_result(header, required_cycle_length, num_bits)?;
    Ok(Difficulty::big_endian_difficulty(&difficulty)?)
}

#[cfg(test)]
mod test {
    use super::*;

    const KEYS: [u64; 4] = [123u64, 123u64, 234u64, 23423u64];
    #[test]
    fn test_cuckaroo_nonce_too_large() {
        let nonces = vec![0, 127, 128];
        let cycle_length = 3;
        let edge_bits = 7;
        let result = verify(&KEYS, &nonces, cycle_length, edge_bits);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NonceTooLarge);
    }

    #[test]
    fn test_cuckaroo_nonces_not_ascending_1() {
        let nonces = vec![0, 127, 127];
        let result = verify(&KEYS, &nonces, 3, 7);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NoncesNotAscending)
    }

    #[test]
    fn test_cuckaroo_nonces_not_ascending_2() {
        let nonces = vec![0, 127, 126];

        let result = verify(&KEYS, &nonces, 3, 7);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NoncesNotAscending)
    }

    #[test]
    fn test_cuckaroo_verify_endpoints_dont_match() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 4)];
        let result = verify_from_edges(&uvs, 4);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::EndpointsDontMatch);
    }
    #[test]
    fn test_cuckaroo_verify_from_edges() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 0)];
        let result = verify_from_edges(&uvs, 4);

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_cuckaroo_verify_from_edges_out_of_order() {
        let uvs = vec![(0, 1), (1, 2), (3, 0), (2, 3)];
        let result = verify_from_edges(&uvs, 4);

        assert_eq!(result, Ok(()));

        use rand::prelude::SliceRandom;

        let mut uvs = uvs;
        uvs.shuffle(&mut rand::thread_rng());
        let result = verify_from_edges(&uvs, 4);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_cuckaroo_cycle_too_short() {
        let uvs = vec![(0, 1), (1, 2)];
        let result = verify_from_edges(&uvs, 3);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::CycleTooShort);
    }

    #[test]
    fn test_cuckaroo_cycle_too_long() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)];
        let result = verify_from_edges(&uvs, 4);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::CycleTooLong);
    }

    #[ignore = "This test is ignored because it is caught be NodeHasMoreThanTwoEdges"]
    #[test]
    fn test_cuckaroo_edge_already_visited() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (0, 1), (3, 0), (1, 0)];
        let result = verify_from_edges(&uvs, 6);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::EdgeAlreadyVisited);
    }

    #[test]
    fn test_cuckaroo_node_has_more_than_two_edges() {
        let uvs = vec![(0, 1), (1, 2), (2, 3), (3, 0), (0, 2), (2, 0)];
        let result = verify_from_edges(&uvs, 6);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NodeHasMoreThanTwoEdges);
    }
}
