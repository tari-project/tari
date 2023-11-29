//  Copyright 2023, The Tari Project
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

use std::{cmp::min, convert::TryFrom};

use monero::VarInt;

// This is based on https://github.com/SChernykh/p2pool/blob/merge-mining/docs/MERGE_MINING.MD#merge-mining-tx_extra-tag-format
#[derive(Debug)]
pub struct MerkleTreeParameters {
    pub number_of_chains: u8,
    pub aux_nonce: u32,
}

impl MerkleTreeParameters {
    pub fn from_varint(merkle_tree_varint: VarInt) -> MerkleTreeParameters {
        let bits = get_decode_bits(merkle_tree_varint.0);
        let number_of_chains = get_aux_chain_count(merkle_tree_varint.0, bits);
        let aux_nonce = get_aux_nonce(merkle_tree_varint.0, bits);
        MerkleTreeParameters {
            number_of_chains,
            aux_nonce,
        }
    }

    pub fn to_varint(&self) -> VarInt {
        let size = u8::try_from(self.number_of_chains.leading_zeros())
            .expect("This cant fail, u8 can only have 8 leading 0's which will fit in 255");
        let mut bits = encode_bits(8 - size);
        let mut n = encode_aux_chain_count(self.number_of_chains, 8 - size);
        let mut nonce = encode_aux_nonce(self.aux_nonce);
        bits.append(&mut n);
        bits.append(&mut nonce);
        if bits.len() < 64 {
            let mut missing_zeroes = vec![0; 64 - bits.len()];
            bits.append(&mut missing_zeroes);
        }
        let num: u64 = bits.iter().fold(0, |result, &bit| (result << 1) ^ u64::from(bit));
        VarInt(num)
    }
}

fn get_decode_bits(num: u64) -> u8 {
    let bits_num: Vec<u8> = (61..=63).rev().map(|n| ((num >> n) & 1) as u8).collect();
    bits_num.iter().fold(0, |result, &bit| (result << 1) ^ bit)
}

fn encode_bits(num: u8) -> Vec<u8> {
    (0..=2).rev().map(|n| (num >> n) & 1).collect()
}

fn get_aux_chain_count(num: u64, bits: u8) -> u8 {
    let start = 60 - min(8, bits) + 1;
    let bits_num: Vec<u8> = (start..=60).rev().map(|n| ((num >> n) & 1) as u8).collect();
    bits_num.iter().fold(0, |result, &bit| (result << 1) ^ bit)
}

fn encode_aux_chain_count(num: u8, bit_length: u8) -> Vec<u8> {
    (0..bit_length).rev().map(|n| (num >> n) & 1).collect()
}

fn get_aux_nonce(num: u64, bits: u8) -> u32 {
    let start = 60 - min(8, u32::from(bits)) + 1 - 32;
    let end = 60 - min(8, u32::from(bits));
    let bits_num: Vec<u32> = (start..=end).rev().map(|n| ((num >> n) & 1) as u32).collect();
    bits_num.iter().fold(0, |result, &bit| (result << 1) ^ bit)
}

fn encode_aux_nonce(num: u32) -> Vec<u8> {
    (0..=31).rev().map(|n| ((num >> n) & 1) as u8).collect()
}

#[cfg(test)]
mod test {
    use crate::proof_of_work::monero_rx::merkle_tree_parameters::{
        encode_aux_chain_count,
        encode_aux_nonce,
        encode_bits,
        get_aux_chain_count,
        get_aux_nonce,
        get_decode_bits,
    };

    #[test]
    fn en_decode_bits_test() {
        let num = 0b1100000000000000000000000000000000000000000000000000000000000101;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 6);
        let bits = encode_bits(6);
        let array = vec![1, 1, 0];
        assert_eq!(bits, array);

        let num = 0b0100000000000000000000000000000000000000000000000000000000000101;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 2);
        let bits = encode_bits(2);
        let array = vec![0, 1, 0];
        assert_eq!(bits, array);

        let num = 0b1110000000000000000000000000000000000000000000000000000000000101;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 7);
        let bits = encode_bits(7);
        let array = vec![1, 1, 1];
        assert_eq!(bits, array);

        let num = 0b0011000000000000000000000000000000000000000000000000000000000101;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 1);
        let bits = encode_bits(1);
        let array = vec![0, 0, 1];
        assert_eq!(bits, array);
    }

    #[test]
    fn get_decode_aux_chain_test() {
        let num = 0b1101111111100000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 8);
        assert_eq!(aux_number, 255);
        let bits = encode_aux_chain_count(255, 8);
        let array = vec![1, 1, 1, 1, 1, 1, 1, 1];
        assert_eq!(bits, array);

        let num = 0b1100000000100000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 8);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_chain_count(1, 8);
        let array = vec![0, 0, 0, 0, 0, 0, 0, 1];
        assert_eq!(bits, array);

        let num = 0b1100000000000000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 8);
        assert_eq!(aux_number, 0);
        let bits = encode_aux_chain_count(0, 8);
        let array = vec![0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(bits, array);

        let num = 0b1100111000000000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 8);
        assert_eq!(aux_number, 112);
        let bits = encode_aux_chain_count(112, 8);
        let array = vec![0, 1, 1, 1, 0, 0, 0, 0];
        assert_eq!(bits, array);

        let num = 0b1100000100000000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 8);
        assert_eq!(aux_number, 8);
        let bits = encode_aux_chain_count(8, 8);
        let array = vec![0, 0, 0, 0, 1, 0, 0, 0];
        assert_eq!(bits, array);

        let num = 0b1100000001000000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 7);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_chain_count(1, 7);
        let array = vec![0, 0, 0, 0, 0, 0, 1];
        assert_eq!(bits, array);

        let num = 0b1100000010000000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 6);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_chain_count(1, 6);
        let array = vec![0, 0, 0, 0, 0, 1];
        assert_eq!(bits, array);

        let num = 0b1100000100000000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 5);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_chain_count(1, 5);
        let array = vec![0, 0, 0, 0, 1];
        assert_eq!(bits, array);

        let num = 0b1100000110000000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 5);
        assert_eq!(aux_number, 1);

        let num = 0b1111000110000000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 1);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_chain_count(1, 1);
        let array = vec![1];
        assert_eq!(bits, array);
    }

    #[test]
    fn get_decode_aux_nonce_test() {
        let num = 0b1100000000110000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_nonce(num, 8);
        assert_eq!(aux_number, 2147483648);
        let bits = encode_aux_nonce(2147483648);
        let array = vec![
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000011111111111111111111111111111111000000000000000000101;
        let aux_number = get_aux_nonce(num, 8);
        assert_eq!(aux_number, u32::MAX);
        let bits = encode_aux_nonce(u32::MAX);
        let array = vec![
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000111111111111111111111111111111110000000000000000000101;
        let aux_number = get_aux_nonce(num, 7);
        assert_eq!(aux_number, u32::MAX);

        let num = 0b1100111111111111111111111111111111110000000000000000000000000101;
        let aux_number = get_aux_nonce(num, 1);
        assert_eq!(aux_number, u32::MAX);

        let num = 0b1100000000100000000000000000000000000000001000000000000000000101;
        let aux_number = get_aux_nonce(num, 8);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000100000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_nonce(num, 8);
        assert_eq!(aux_number, 0);
        let bits = encode_aux_nonce(0);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(bits, array);
    }
}
