use blake2::Blake2b;
use thiserror::Error;

use crate::{blocks::BlockHeader, proof_of_work::Difficulty};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CuckarooVerificationError {
    #[error("Nonce is too large")]
    NonceTooLarge,
}

pub fn cuckaroo_result(
    header: &BlockHeader,
    required_cycle_length: u32,
    num_bits: u32,
) -> Result<Vec<u8>, anyhow::Error> {
    let pow = header.pow.to_bytes();

    // First byte must be 3 for Cuckaroo
    if pow[0] != 3 {
        return Err(anyhow::Error::msg("Invalid PoW algorithm for Cuckaroo"));
    }
    let blake2b = Blake2b::new();
    let mut hasher = blake2b.chain_update(header.nonce.to_le_bytes());
    hasher = hasher.chain_update(header.mining_hash());
    let mut blob = hasher.finalize().to_vec();
    // Ensure the blob is 32 bytes long
    blob.resize(32, 0);
    let mut nonces = Vec::new();

    // let pow_data = &pow[1..];

    // for i in 0..(pow_data.len() * 8 / num_bits as usize) {
    //     let mut nonce = 0u64;
    //     for j in 0..num_bits {
    //         nonce |= ((pow_data[i * num_bits as usize + j as usize] as u64) & 1) << j;
    //     }
    //     nonces.push(nonce);
    // }

    // // Generate the hasher.
    // cuckaroo_result_inner(blob, nonces, required_cycle_length)
}

fn check_nonces_and_generate_edges(
    cycle_length: usize,
    nonces: &[u64],
    edge_bits: u8,
) -> Result<(), CuckarooVerificationError> {
    let num_edges = 1u64 << edge_bits;
    let edge_mask = num_edges - 1;
    // let num_nodes = 1u64 << node_bits;
    // let node_mask = num_nodes - 1;
    for n in 0..cycle_length {
        if nonces[n] > edge_mask {
            return Err(CuckarooVerificationError::NonceTooLarge);
        }
    }

    // Here you would implement the logic to check the nonces and generate edges.
    // This is a placeholder for the actual implementation.
    // For example, you might check if the nonces form a valid cycle.

    Ok(())
}

fn cuckaroo_result_inner(
    sip_hash_key: &[u8; 32],
    nonces: Vec<u64>,
    required_cycle_length: u32,
) -> Result<Vec<u8>, anyhow::Error> {
    todo!();
    // let hasher = SipHasher24.new_with_key(sip_hash_key);

    // Extra the nonces from the pow_data, which are packed as `num_bits` bits each.
    // First check if the cycle is valid
    // todo;

    // Sort the nonces, and hash for difficulty.
    // nonces.sort();
    // let difficulty_hash = {
    //     let mut hasher = Blake2b::new();
    //     for nonce in &nonces {
    //         hasher = hasher.chain_update(nonce.to_le_bytes());
    //     }
    //     hasher.finalize()
    // };

    // Ok(difficulty_hash.into())
}

pub fn cuckaroo_difficulty(
    header: &BlockHeader,
    required_cycle_length: u32,
    num_bits: u32,
) -> Result<Difficulty, anyhow::Error> {
    let difficulty = cuckaroo_result(header, 42, 6)?;
    todo!()
    // Difficulty::big_endian_difficulty(&difficulty)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_cuckaroo_nonce_too_large() {
        let nonces = vec![0, 255];
        let cycle_length = 2;
        let edge_bits = 8;
        let result = check_nonces_and_generate_edges(cycle_length, &nonces, edge_bits);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CuckarooVerificationError::NonceTooLarge);
    }
}
