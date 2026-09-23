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

use std::convert::TryFrom;

use monero::VarInt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Error, Deserialize, Serialize, Eq)]
pub enum MerkleTreeParametersError {
    #[error("Cannot have zero chains")]
    NumberOfChainZero,
    #[error("Encoded aux chain count is {0}, which exceeds the maximum of 255")]
    NumberOfChainsOutOfRange(u32),
    #[error(
        "Merge mining parameters {varint} are not a canonical encoding; the parameters they decode to encode to \
         {canonical}"
    )]
    NonCanonicalEncoding { varint: u64, canonical: u64 },
}

// This is based on https://github.com/SChernykh/p2pool/blob/merge-mining/docs/MERGE_MINING.MD#merge-mining-tx_extra-tag-format
#[derive(Debug, Clone, PartialEq)]
pub struct MerkleTreeParameters {
    number_of_chains: u8,
    aux_nonce: u32,
}

impl MerkleTreeParameters {
    pub fn new(number_of_chains: u8, aux_nonce: u32) -> Result<MerkleTreeParameters, MerkleTreeParametersError> {
        if number_of_chains == 0u8 {
            return Err(MerkleTreeParametersError::NumberOfChainZero);
        }
        Ok(MerkleTreeParameters {
            number_of_chains,
            aux_nonce,
        })
    }

    /// Decodes the merge mining parameters from their varint wire form.
    ///
    /// This is the strict decoder (GHSA-3qmx-q9pv-f3m4, item 3). It accepts a varint only if that varint is the
    /// canonical encoding of the parameters it decodes to, i.e. only if it is a fixed point of
    /// `from_varint` -> [`MerkleTreeParameters::to_varint`]. That makes it injective by construction: `to_varint` is
    /// a left inverse on the accepted set, so two distinct accepted varints cannot decode to the same parameters.
    ///
    /// The bit readers alone are very far from injective, and widening the aux chain count arithmetic - the original
    /// finding - closes only one collision pair out of millions. Three independent families remain:
    ///
    /// 1. **Non-canonical width.** The 3 bit size field chooses the width of the aux chain count field, but any width
    ///    wide enough to hold the count decodes to that count, and the nonce simply starts higher. Two chains as `size
    ///    = 0, raw = 0b1` and as `size = 1, raw = 0b01` are different varints with identical parameters.
    /// 2. **A nonce bit that is read and thrown away.** [`get_aux_nonce`] walks an inclusive range of 33 bits and folds
    ///    them into a `u32`, so the topmost bit it reads is shifted straight back out. Flipping that bit changes the
    ///    varint and not the result.
    /// 3. **Bits that are never read.** At the widest size field the decoder touches bits 0..=43 only; bits 44..=63 are
    ///    ignored entirely.
    ///
    /// Re-encoding and comparing closes all three with one rule, and subsumes the aux chain count overflow as well:
    /// [`MerkleTreeParameters::to_varint`] can never emit the raw field `0b11111111`, so that form could never round
    /// trip. [`MerkleTreeParametersError::NumberOfChainsOutOfRange`] is still returned for it in preference to the
    /// generic error, so an operator reading the log can tell the two apart.
    ///
    /// `to_varint` is unchanged by this fix and is the definition of the canonical form; changing it is a consensus
    /// change in its own right.
    pub fn from_varint(merkle_tree_varint: VarInt) -> Result<MerkleTreeParameters, MerkleTreeParametersError> {
        let bits = get_decode_bits(merkle_tree_varint.0);

        let number_of_chains = get_aux_chain_count(merkle_tree_varint.0, bits)?;
        let aux_nonce = get_aux_nonce(merkle_tree_varint.0, bits);
        let params = MerkleTreeParameters {
            number_of_chains,
            aux_nonce,
        };

        let canonical = params.to_varint();
        if canonical.0 != merkle_tree_varint.0 {
            return Err(MerkleTreeParametersError::NonCanonicalEncoding {
                varint: merkle_tree_varint.0,
                canonical: canonical.0,
            });
        }
        Ok(params)
    }

    /// The pre-fork decoder, kept byte for byte so that every block below a network's
    /// `strict_merkle_tree_parameter_decoding` activation height validates exactly as it does today. It folds the raw
    /// aux chain count into a `u8` and saturates, so the two distinct raw fields `0b11111110` and `0b11111111` both
    /// decode to 255 chains, and it performs no canonicality check at all - non-canonical widths, the discarded
    /// nonce bit and the unread high bits are all still accepted here. That is deliberate: the whole point of the
    /// gate is that historical blocks decode exactly as they always have, so neither the saturation nor the 33 bit
    /// nonce read is "fixed" on this path. Do not use it for anything but historical block validation.
    ///
    /// `pub(crate)` rather than `pub`: the module is private but `MerkleTreeParameters` is re-exported from
    /// `monero_rx`, so an inherent `pub fn` here would be callable from any crate depending on `tari_core` with
    /// nothing but this doc comment guarding it.
    pub(crate) fn from_varint_legacy(merkle_tree_varint: VarInt) -> MerkleTreeParameters {
        let bits = get_decode_bits(merkle_tree_varint.0);

        let number_of_chains = get_aux_chain_count_legacy(merkle_tree_varint.0, bits);
        let aux_nonce = get_aux_nonce(merkle_tree_varint.0, bits);
        MerkleTreeParameters {
            number_of_chains,
            aux_nonce,
        }
    }

    pub fn to_varint(&self) -> VarInt {
        // 1 is encoded as 0
        let num = self.number_of_chains.saturating_sub(1);
        let size = u8::try_from(num.leading_zeros())
            .expect("This cant fail, u8 can only have 8 leading 0's which will fit in 255");
        // size must be greater than 0, so saturating sub should be safe.
        let mut size_bits = encode_bits(7u8.saturating_sub(size));
        let mut n_bits = encode_aux_chain_count(self.number_of_chains);
        let mut nonce_bits = encode_aux_nonce(self.aux_nonce);
        // this wont underflow as max size will be size_bits(3) + n_bits(8) + nonce_bits(32) = 43
        let mut zero_bits = vec![
            0;
            64usize
                .saturating_sub(size_bits.len())
                .saturating_sub(n_bits.len())
                .saturating_sub(nonce_bits.len())
        ];
        zero_bits.append(&mut nonce_bits);
        zero_bits.append(&mut n_bits);
        zero_bits.append(&mut size_bits);

        let num: u64 = zero_bits.iter().fold(0, |result, &bit| (result << 1) ^ u64::from(bit));
        VarInt(num)
    }

    pub fn number_of_chains(&self) -> u8 {
        self.number_of_chains
    }

    pub fn aux_nonce(&self) -> u32 {
        self.aux_nonce
    }
}

fn get_decode_bits(num: u64) -> u8 {
    let bits_num: Vec<u8> = (0..=2).rev().map(|n| ((num >> n) & 1) as u8).collect();
    bits_num.iter().fold(0, |result, &bit| (result << 1) ^ bit)
}

fn encode_bits(num: u8) -> Vec<u8> {
    (0..=2).rev().map(|n| (num >> n) & 1).collect()
}

/// Reads the raw aux chain count field: `bits + 1` bits starting at bit 3. The field is at most 8 bits wide, so the
/// raw value is in `0..=255` and the decoded count, which is the raw value plus one, is in `1..=256`.
fn get_raw_aux_chain_count(num: u64, bits: u8) -> u32 {
    let end = bits.saturating_add(3);
    let bits_num: Vec<u32> = (3..=end).rev().map(|n| ((num >> n) & 1) as u32).collect();
    // At most 8 bits are folded in, so a u32 accumulator cannot overflow.
    bits_num.iter().fold(0u32, |result, &bit| (result << 1) ^ bit)
}

/// The strict aux chain count decoder. The intermediate arithmetic is widened to `u32` so that the `+ 1` is exact, and
/// a count of 256 - the raw field `0b11111111` - is rejected instead of being clamped onto 255. Without this, the raw
/// fields 254 and 255 both decoded to 255.
///
/// RECORDED DECISION: this permanently caps Tari merge mining at 255 aux chains. The wire format stores `count - 1`,
/// so the full 8 bit raw field denotes 1..=256 and 256 is a legitimate encoding in the p2pool format this derives
/// from. Tari cannot represent it, because `number_of_chains` is a `u8`, and rejecting it is chosen over clamping so
/// that the encoding stays injective. Supporting 256 aux chains later would mean widening `number_of_chains` to a
/// `u16`, changing `to_varint`, and forking again. This is a deliberate choice, not an accident of the field width.
fn get_aux_chain_count(num: u64, bits: u8) -> Result<u8, MerkleTreeParametersError> {
    // 1 is encoded as 0, so the count is the raw field plus one. `get_raw_aux_chain_count` returns at most 255, so
    // this cannot overflow a u32.
    let count = get_raw_aux_chain_count(num, bits).saturating_add(1);
    u8::try_from(count).map_err(|_| MerkleTreeParametersError::NumberOfChainsOutOfRange(count))
}

/// The pre-fork aux chain count decoder, kept byte for byte. It folds into a `u8` and then saturates, collapsing the
/// raw fields `0b11111110` (255 chains) and `0b11111111` (256 chains) onto the same value.
fn get_aux_chain_count_legacy(num: u64, bits: u8) -> u8 {
    let end = bits.saturating_add(3);
    let bits_num: Vec<u8> = (3..=end).rev().map(|n| ((num >> n) & 1) as u8).collect();
    (bits_num.iter().fold(0, |result, &bit| (result << 1) ^ bit)).saturating_add(1)
}

fn encode_aux_chain_count(num: u8) -> Vec<u8> {
    // 1 is encoded as 0
    let num = num.saturating_sub(1);
    if num == 0 {
        return vec![0];
    }
    let size = u8::try_from(num.leading_zeros())
        .expect("This cant fail, u8 can only have 8 leading 0's which will fit in 255");
    let bit_length = 8u8.saturating_sub(size);
    (0..bit_length).rev().map(|n| (num >> n) & 1).collect()
}

fn get_aux_nonce(num: u64, bits: u8) -> u32 {
    // 0,1,2 is storing bits, then amount of bits, then start at next bit to read
    let start = bits.saturating_add(4);
    let end = start.saturating_add(32);
    let bits_num: Vec<u32> = (start..=end).rev().map(|n| ((num >> n) & 1) as u32).collect();
    bits_num.iter().fold(0, |result, &bit| (result << 1) ^ bit)
}

fn encode_aux_nonce(num: u32) -> Vec<u8> {
    (0..=31).rev().map(|n| ((num >> n) & 1) as u8).collect()
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use monero::VarInt;

    use crate::proof_of_work::monero_rx::{
        MerkleTreeParameters,
        merkle_tree_parameters::{
            MerkleTreeParametersError,
            encode_aux_chain_count,
            encode_aux_nonce,
            encode_bits,
            get_aux_chain_count,
            get_aux_chain_count_legacy,
            get_aux_nonce,
            get_decode_bits,
            get_raw_aux_chain_count,
        },
    };

    #[test]
    fn en_decode_bits_test() {
        let num = 24u64; // 11000
        let bit = get_decode_bits(num);
        assert_eq!(bit, 0);
        let bits = encode_bits(0);
        let array = vec![0, 0, 0];
        assert_eq!(bits, array);

        let num = 0b1100000000000000000000000000000000000000000000000000000000000101;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 5);
        let bits = encode_bits(5);
        let array = vec![1, 0, 1];
        assert_eq!(bits, array);

        let num = 0b0100000000000000000000000000000000000000000000000000000000000110;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 6);
        let bits = encode_bits(6);
        let array = vec![1, 1, 0];
        assert_eq!(bits, array);

        let num = 0b1010000000000000000000000000000000000000000000000000000000000111;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 7);
        let bits = encode_bits(7);
        let array = vec![1, 1, 1];
        assert_eq!(bits, array);

        let num = 0b0011000000000000000000000000000000000000000000000000000000000001;
        let bit = get_decode_bits(num);
        assert_eq!(bit, 1);
        let bits = encode_bits(1);
        let array = vec![0, 0, 1];
        assert_eq!(bits, array);
    }

    #[test]
    fn get_decode_aux_chain_test() {
        let num = 24u64; // 11000
        let aux_number = get_aux_chain_count(num, 0);
        assert_eq!(aux_number, Ok(2));
        let bits = encode_aux_chain_count(2);
        let array: Vec<u8> = vec![1];
        assert_eq!(bits, array);

        let num = 0b1101111111100000000000000000000000000000000000000000011111110000;
        let aux_number = get_aux_chain_count(num, 7);
        assert_eq!(aux_number, Ok(255));
        let bits = encode_aux_chain_count(255);
        let array = vec![1, 1, 1, 1, 1, 1, 1, 0];
        assert_eq!(bits, array);

        let num = 0b1100000000100000000000000000000000000000000000000000000000101101;
        let aux_number = get_aux_chain_count(num, 3);
        assert_eq!(aux_number, Ok(6));
        let bits = encode_aux_chain_count(6);
        let array = vec![1, 0, 1];
        assert_eq!(bits, array);

        let num = 0b1100000000000000000000000000000000000000000000000000000000011101;
        let aux_number = get_aux_chain_count(num, 2);
        assert_eq!(aux_number, Ok(4));
        let bits = encode_aux_chain_count(4);
        let array = vec![1, 1];
        assert_eq!(bits, array);

        let num = 0b1100111000000000000000000000000000000000000000000000000000000101;
        let aux_number = get_aux_chain_count(num, 1);
        assert_eq!(aux_number, Ok(1));
        let bits = encode_aux_chain_count(1);
        let array = vec![0];
        assert_eq!(bits, array);

        let num = 0b1100000100000000000000000000000000000000000000000000000000111101;
        let aux_number = get_aux_chain_count(num, 3);
        assert_eq!(aux_number, Ok(8));
        let bits = encode_aux_chain_count(8);
        let array = vec![1, 1, 1];
        assert_eq!(bits, array);

        let num = 0b1100000001000000000000000000000000000000000000000000000001111101;
        let aux_number = get_aux_chain_count(num, 4);
        assert_eq!(aux_number, Ok(16));
        let bits = encode_aux_chain_count(16);
        let array = vec![1, 1, 1, 1];
        assert_eq!(bits, array);

        let num = 0b1100000010000000000000000000000000000000000000000000001111000101;
        let aux_number = get_aux_chain_count(num, 7);
        assert_eq!(aux_number, Ok(121));
        let bits = encode_aux_chain_count(121);
        let array = vec![1, 1, 1, 1, 0, 0, 0];
        assert_eq!(bits, array);

        let num = 0b1100000100000000000000000000000000000000000000000000001100000101;
        let aux_number = get_aux_chain_count(num, 7);
        assert_eq!(aux_number, Ok(97));
        let bits = encode_aux_chain_count(97);
        let array = vec![1, 1, 0, 0, 0, 0, 0];
        assert_eq!(bits, array);

        let num = 0b1111000110000000000000000000000000000000000000000000000111000101;
        let aux_number = get_aux_chain_count(num, 6);
        assert_eq!(aux_number, Ok(57));
        let bits = encode_aux_chain_count(57);
        let array = vec![1, 1, 1, 0, 0, 0];
        assert_eq!(bits, array);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn get_decode_aux_nonce_test() {
        let num = 24u64; // 11000
        let aux_number = get_aux_nonce(num, 0);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000000000000000000000000000000000000100000000101;
        let aux_number = get_aux_nonce(num, 7);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000000000000000000000000000000000000010000000101;
        let aux_number = get_aux_nonce(num, 6);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000000000000000000000000000000000000001000000101;
        let aux_number = get_aux_nonce(num, 5);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000000000000000000000000000000000000000100000101;
        let aux_number = get_aux_nonce(num, 4);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000000000000000000000000000000000000000010000101;
        let aux_number = get_aux_nonce(num, 3);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000000000000000000000000000000000000000001000101;
        let aux_number = get_aux_nonce(num, 2);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000000000000000000000000000000000000000000100101;
        let aux_number = get_aux_nonce(num, 1);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000000000000000000000000000000000000000000010101;
        let aux_number = get_aux_nonce(num, 0);
        assert_eq!(aux_number, 1);
        let bits = encode_aux_nonce(1);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000000000000000000000000000000000000010000000101;
        let aux_number = get_aux_nonce(num, 7);
        assert_eq!(aux_number, 0);
        let bits = encode_aux_nonce(0);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000001111111111111111111111111111111110000000101;
        let aux_number = get_aux_nonce(num, 7);
        assert_eq!(aux_number, u32::MAX);
        let bits = encode_aux_nonce(u32::MAX);
        let array = vec![
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000001111111111100011111111111111111110000000101;
        let aux_number = get_aux_nonce(num, 7);
        assert_eq!(aux_number, 4293132287);
        let bits = encode_aux_nonce(4293132287);
        let array = vec![
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        ];
        assert_eq!(bits, array);

        let num = 0b1100000000110000000001010101010101010101010101010101010000000101;
        let aux_number = get_aux_nonce(num, 7);
        assert_eq!(aux_number, 2863311530);
        let bits = encode_aux_nonce(2863311530);
        let array = vec![
            1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0,
        ];
        assert_eq!(bits, array);

        let num = 0b110000000011000000000000000000000000011110011110111010000000101;
        let aux_number = get_aux_nonce(num, 7);
        assert_eq!(aux_number, 31214);
        let bits = encode_aux_nonce(31214);
        let array = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 0,
        ];
        assert_eq!(bits, array);
    }

    #[test]
    fn merkle_complete() {
        let num = VarInt(24);
        let merkle_tree_params = MerkleTreeParameters::from_varint(num).unwrap();
        assert_eq!(merkle_tree_params.aux_nonce, 1);
        assert_eq!(merkle_tree_params.number_of_chains, 2);

        let ser_num = merkle_tree_params.to_varint();
        assert_eq!(ser_num, VarInt(24));
    }

    /// Builds a varint by hand from its three fields, the way `to_varint` lays them out: the 3 bit size field in bits
    /// 0..=2, the `size + 1` bit raw aux chain count from bit 3, and the nonce above it.
    fn varint_from_fields(size: u8, raw_aux_chain_count: u32, aux_nonce: u32) -> VarInt {
        let count_width = u32::from(size).saturating_add(1);
        VarInt(
            (u64::from(aux_nonce) << count_width.saturating_add(3)) |
                (u64::from(raw_aux_chain_count) << 3) |
                u64::from(size),
        )
    }

    /// GHSA-3qmx-q9pv-f3m4 item 3. The widest raw aux chain count field, `0b11111111`, denotes 256 chains, which
    /// `number_of_chains` cannot hold. The strict decoder rejects it rather than clamping it onto 255.
    #[test]
    fn raw_aux_chain_count_of_256_is_rejected() {
        assert_eq!(
            get_aux_chain_count(varint_from_fields(7, 0b1111_1111, 0).0, 7),
            Err(MerkleTreeParametersError::NumberOfChainsOutOfRange(256))
        );

        let varint = varint_from_fields(7, 0b1111_1111, 12_345);
        assert_eq!(
            MerkleTreeParameters::from_varint(varint),
            Err(MerkleTreeParametersError::NumberOfChainsOutOfRange(256))
        );
    }

    /// GHSA-3qmx-q9pv-f3m4 item 3. The two raw fields that used to collapse onto a single `number_of_chains` no
    /// longer do: 254 decodes to 255 chains and 255 is rejected, so the decoding is injective. The nonce is the same
    /// in both, so the raw count field is the only thing that differs.
    #[test]
    fn the_two_ambiguous_raw_aux_chain_counts_are_now_distinguishable() {
        let ambiguous = varint_from_fields(7, 0b1111_1111, 7);
        let unambiguous = varint_from_fields(7, 0b1111_1110, 7);
        let (ambiguous_raw, unambiguous_raw) = (ambiguous.0, unambiguous.0);
        assert_ne!(ambiguous, unambiguous, "the two wire encodings must differ");

        let decoded = MerkleTreeParameters::from_varint(unambiguous.clone()).unwrap();
        assert_eq!(decoded.number_of_chains(), 255);
        assert_eq!(decoded.aux_nonce(), 7);
        assert_eq!(
            MerkleTreeParameters::from_varint(ambiguous.clone()),
            Err(MerkleTreeParametersError::NumberOfChainsOutOfRange(256))
        );

        // The pre-fork decoder collapsed both onto 255. This pins that behaviour, so the gating is provably real:
        // blocks below the activation height keep decoding exactly as they always have.
        assert_eq!(
            MerkleTreeParameters::from_varint_legacy(ambiguous.clone()).number_of_chains(),
            255
        );
        assert_eq!(
            MerkleTreeParameters::from_varint_legacy(unambiguous.clone()).number_of_chains(),
            255
        );
        assert_eq!(
            MerkleTreeParameters::from_varint_legacy(ambiguous),
            MerkleTreeParameters::from_varint_legacy(unambiguous),
            "the pre-fork decoder is the non-injective one"
        );
        assert_eq!(get_aux_chain_count_legacy(ambiguous_raw, 7), 255);
        assert_eq!(get_aux_chain_count_legacy(unambiguous_raw, 7), 255);
    }

    /// Nonces chosen to hit every interesting position of the 32 bit field: both ends, both ends of each half, the
    /// bit either side of the sign boundary, a single bit in the top position, the alternating patterns and a
    /// couple of arbitrary values. Crossed with all 255 aux chain counts this is the whole encoder surface that
    /// matters for the width of the count field, which is the only thing the count varies.
    const BOUNDARY_NONCES: [u32; 14] = [
        0,
        1,
        2,
        7,
        31_214,
        0x0000_FFFF,
        0x0001_0000,
        0x5555_5555,
        0xAAAA_AAAA,
        0x7FFF_FFFF,
        0x8000_0000,
        0x8000_0001,
        u32::MAX - 1,
        u32::MAX,
    ];

    /// `to_varint` and `from_varint` are exact inverses on the strict path, for every representable aux chain count.
    ///
    /// This is the property the fork rests on: the canonicality check rejects any varint that is not a fixed point
    /// of encode-then-decode, so if the encoder could ever produce a varint that fails it, the fork would orphan
    /// blocks that honest merge miners produced with this very code. Proving the encoder is a total right inverse of
    /// the strict decoder is therefore not a nice-to-have, it is the safety argument.
    #[test]
    fn the_encoder_is_a_total_right_inverse_of_the_strict_decoder() {
        for number_of_chains in 1..=u8::MAX {
            for aux_nonce in BOUNDARY_NONCES {
                let params = MerkleTreeParameters::new(number_of_chains, aux_nonce).unwrap();
                let varint = params.to_varint();
                let decoded = MerkleTreeParameters::from_varint(varint.clone())
                    .unwrap_or_else(|e| panic!("{number_of_chains} chains, nonce {aux_nonce}: {e}"));
                assert_eq!(decoded, params);
                // And the encoding really is the canonical one, which is what the check compares against.
                assert_eq!(decoded.to_varint(), varint);
            }
        }
    }

    /// The encoder never emits the raw field that the strict decoder rejects with the specific out-of-range error,
    /// so that rejection can never orphan a block an honest merge miner produced with this code.
    #[test]
    fn the_encoder_never_emits_the_rejected_raw_aux_chain_count() {
        for number_of_chains in 1..=u8::MAX {
            let varint = MerkleTreeParameters::new(number_of_chains, 99).unwrap().to_varint();
            let size = get_decode_bits(varint.0);
            assert_ne!(
                get_raw_aux_chain_count(varint.0, size),
                0b1111_1111,
                "{number_of_chains} chains encoded to the rejected raw field"
            );
        }
    }

    /// GHSA-3qmx-q9pv-f3m4 item 3. The three collision families the canonicality check closes, each pinned by the
    /// exact pair the reviewer found colliding. Every one of these decodes to identical parameters under both
    /// decoders; only the strict decoder can tell the varints apart, and it does so by rejecting the non-canonical
    /// member of each pair.
    #[test]
    fn non_canonical_encodings_are_rejected() {
        // 1. Non-canonical width. `9` says "two chains in a two bit field"; the canonical form is `8`, "two chains in a
        //    one bit field". `0` and `1` are the same story for one chain.
        // 2. Unread high bits. Bit 50 is outside everything the decoder reads, so `24 | 1 << 50` is `24` as far as the
        //    bit readers are concerned.
        // 3. The nonce bit that is read and shifted straight back out: with a size field of 0 the nonce is read from
        //    bits 4..=36 and folded into a `u32`, so bit 36 is discarded.
        for (canonical, non_canonical, what) in [
            (0u64, 1u64, "one chain in a two bit count field"),
            (8, 9, "two chains in a two bit count field"),
            (24, 24 | 1 << 50, "a bit above everything the decoder reads"),
            (24, 24 | 1 << 36, "the nonce bit that is read and then shifted out"),
        ] {
            let accepted = MerkleTreeParameters::from_varint(VarInt(canonical))
                .unwrap_or_else(|e| panic!("{canonical} should be canonical: {e}"));

            // Both varints mean the same thing to the bit readers, which is exactly the collision.
            assert_eq!(
                MerkleTreeParameters::from_varint_legacy(VarInt(non_canonical)),
                MerkleTreeParameters::from_varint_legacy(VarInt(canonical)),
                "{what}: the pre-fork decoder is supposed to collide here"
            );
            assert_ne!(canonical, non_canonical);

            assert_eq!(
                MerkleTreeParameters::from_varint(VarInt(non_canonical)),
                Err(MerkleTreeParametersError::NonCanonicalEncoding {
                    varint: non_canonical,
                    canonical,
                }),
                "{what}: the strict decoder must reject the non-canonical member"
            );
            assert_eq!(accepted.to_varint(), VarInt(canonical));
        }
    }

    /// GHSA-3qmx-q9pv-f3m4 item 3. Injectivity demonstrated rather than asserted: enumerate a varint range, decode
    /// every value with the strict decoder, and check that no two distinct accepted varints land on the same
    /// parameters.
    ///
    /// The bound is `1 << 18`, which keeps the sweep to a few seconds even in a debug build. The wire layout puts
    /// the 3 bit size field in bits 0..=2 and the aux chain count in the `size + 1` bits above it, so 18 bits covers
    /// every one of the 8 size values crossed with the entire 8 bit count field, plus 7 bits of nonce - that is the
    /// whole of collision family 1, which is the only one
    /// that lives in the low bits and the only one an exhaustive sweep can reach. The other two families live at
    /// bit 36 and above and are unreachable by any feasible enumeration, so they are pinned by name in
    /// `non_canonical_encodings_are_rejected` instead. The same sweep over the pre-fork decoder finds a large
    /// number of collisions, which is asserted here too so the test fails if it ever stops being a real check.
    #[test]
    fn strict_decoding_is_injective_over_a_bounded_varint_range() {
        const BOUND: u64 = 1 << 18;

        let mut seen: HashMap<(u8, u32), u64> = HashMap::new();
        let mut accepted = 0u64;
        for num in 0..BOUND {
            let Ok(params) = MerkleTreeParameters::from_varint(VarInt(num)) else {
                continue;
            };
            accepted += 1;
            if let Some(previous) = seen.insert((params.number_of_chains, params.aux_nonce), num) {
                panic!(
                    "varints {previous} and {num} both decode to {} chains with nonce {}",
                    params.number_of_chains, params.aux_nonce
                );
            }
        }
        assert!(
            accepted > 100_000,
            "only {accepted} varints accepted, the sweep is not meaningful"
        );

        // The same sweep against the pre-fork decoder, which accepts everything and therefore collides heavily.
        // Without this the test above could pass simply because the strict decoder had started rejecting the world.
        let mut legacy_seen: HashMap<(u8, u32), u64> = HashMap::new();
        let mut legacy_collisions = 0u64;
        for num in 0..BOUND {
            let params = MerkleTreeParameters::from_varint_legacy(VarInt(num));
            if legacy_seen
                .insert((params.number_of_chains, params.aux_nonce), num)
                .is_some()
            {
                legacy_collisions += 1;
            }
        }
        assert!(
            legacy_collisions > 100_000,
            "the pre-fork decoder collided only {legacy_collisions} times, which does not match the finding"
        );
    }

    mod quicktest {
        use quickcheck::{Arbitrary, Gen, quickcheck};

        use crate::proof_of_work::monero_rx::MerkleTreeParameters;

        impl Arbitrary for MerkleTreeParameters {
            fn arbitrary(g: &mut Gen) -> MerkleTreeParameters {
                let mut mt = MerkleTreeParameters {
                    number_of_chains: u8::arbitrary(g),
                    aux_nonce: u32::arbitrary(g),
                };
                if mt.number_of_chains == 0 {
                    mt.number_of_chains = 1;
                };
                mt
            }
        }

        #[test]
        fn test_ser_deserialize() {
            fn varint_serialization(mt_params: MerkleTreeParameters) -> bool {
                let varint = mt_params.to_varint();
                // The strict decoder must accept everything the encoder produces, and reproduce it exactly.
                match MerkleTreeParameters::from_varint(varint) {
                    Ok(deserialize) => mt_params == deserialize,
                    Err(_) => false,
                }
            }
            quickcheck(varint_serialization as fn(MerkleTreeParameters) -> bool)
        }

        /// The pre-fork decoder must also round-trip everything the encoder produces, because that is what keeps
        /// historical blocks valid below the activation height. It differs from the strict decoder only on the raw
        /// field that the encoder never emits.
        #[test]
        fn test_legacy_ser_deserialize() {
            fn varint_serialization(mt_params: MerkleTreeParameters) -> bool {
                let varint = mt_params.to_varint();
                mt_params == MerkleTreeParameters::from_varint_legacy(varint)
            }
            quickcheck(varint_serialization as fn(MerkleTreeParameters) -> bool)
        }
    }
}
