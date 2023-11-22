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


use std::cmp::min;
use monero::VarInt;

struct MerkleTreeParameters{
    pub number_of_chains : u8,
    pub aux_nonce : u32,
}

impl MerkleTreeParameters{
    // pub fn from_varint(merkle_tree_varint: VarInt)-> MerkleTreeParameters{
    //
    //
    // }
}

pub fn get_decode_bits(num: u64) -> u8{
    let bits_num:Vec<u8> = (61..=63).rev().map(|n| ((num >> n) & 1) as u8).collect();
    bits_num.iter().fold(0,|result, &bit|{(result<<1)^bit})
}

pub fn get_aux_chain_count(num: u64, bits: u8) -> u8{
    let start = 60 - min(8,bits);
    dbg!(start);
    let bits_num:Vec<u8> = (start..=60).rev().map(|n| ((num >> n) & 1) as u8).collect();
    dbg!(&bits_num);
    bits_num.iter().fold(0,|result, &bit|{(result<<1)^bit})
}

pub fn get_aux_noncet(num: u64, bits: u8) -> u32{
    let start = 60 - min(8,bits as u32) -32;
    let end = 60 - min(8,bits as u32);
    let bits_num:Vec<u32> = (start..=end).rev().map(|n| ((num >> n) & 1) as u32).collect();
    bits_num.iter().fold(0,|result, &bit|{(result<<1)^bit})
}

#[cfg(test)]
mod test {
    use crate::proof_of_work::monero_rx::merkle_tree_parameters::{get_aux_chain_count, get_decode_bits};

    #[test]
    fn get_decode_bits_test() {
        let num = 0b1100000000000000000000000000000000000000000000000000000000000101;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 6);

        let num = 0b0100000000000000000000000000000000000000000000000000000000000101;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 2);

        let num = 0b1110000000000000000000000000000000000000000000000000000000000101;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 7);

        let num = 0b0011000000000000000000000000000000000000000000000000000000000101;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 1);
    }

    #[test]
    fn get_decode_aux_test() {
        let num = 0b1100000000000000000000000000000000000000000000000000011111111101;
        let aux_number = get_aux_chain_count(num, 8);
        assert_eq!(aux_number, 255);


    }
}