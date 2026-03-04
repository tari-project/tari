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

use std::fmt;

use primitive_types::U512;
use serde::{
    de::{self, SeqAccess, Visitor},
    ser::SerializeTuple,
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use tari_common_types::types::{HashOutput, PrivateKey};
use tari_node_components::blocks::BlockHeaderAccumulatedData;
use tari_transaction_components::tari_proof_of_work::{AccumulatedDifficulty, Difficulty};

/// LMDB row data for block header accumulated data — schema version 1.
///
/// # Serialization format
///
/// Fields are serialized as an ordered tuple using bincode. **The field order must not change** —
/// reordering is a breaking schema change that requires a new DB table and migration.
///
/// | # | Field                                  | Bincode bytes                                |
/// |---|----------------------------------------|----------------------------------------------|
/// | 0 | `hash`                                 | 32 bytes (FixedHash / `[u8; 32]`)            |
/// | 1 | `total_kernel_offset`                  | 32 bytes (PrivateKey / RistrettoSecretKey)   |
/// | 2 | `achieved_difficulty`                  | 8 bytes (u64, little-endian)                 |
/// | 3 | `total_accumulated_difficulty`         | 8-byte length prefix + 64 bytes (U512 LE)   |
/// | 4 | `accumulated_monero_randomx_difficulty`| 16 bytes (u128, little-endian)               |
/// | 5 | `accumulated_tari_randomx_difficulty`  | 16 bytes (u128, little-endian)               |
/// | 6 | `accumulated_sha3x_difficulty`         | 16 bytes (u128, little-endian)               |
/// | 7 | `target_difficulty`                    | 8 bytes (u64, little-endian)                 |
#[derive(Debug)]
pub struct LmdbRowBlockHeaderAccumulatedDataV1 {
    pub hash: HashOutput,
    pub total_kernel_offset: PrivateKey,
    pub achieved_difficulty: Difficulty,
    pub total_accumulated_difficulty: U512,
    pub accumulated_monero_randomx_difficulty: AccumulatedDifficulty,
    pub accumulated_tari_randomx_difficulty: AccumulatedDifficulty,
    pub accumulated_sha3x_difficulty: AccumulatedDifficulty,
    pub target_difficulty: Difficulty,
}

impl Serialize for LmdbRowBlockHeaderAccumulatedDataV1 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // IMPORTANT: The serialization order of fields below is part of the LMDB schema.
        // DO NOT reorder, rename, or remove serialize_element calls — it is a breaking schema change.
        //
        // `U512` is from an external crate (`primitive_types`). We explicitly serialize it as 64
        // little-endian bytes to avoid depending on that crate's own serde implementation, which
        // may change across versions. The format (8-byte length prefix + 64 raw bytes) is
        // binary-compatible with the previous `#[derive(Serialize)]` output.
        let mut tup = s.serialize_tuple(8)?;
        tup.serialize_element(&self.hash)?;
        tup.serialize_element(&self.total_kernel_offset)?;
        tup.serialize_element(&self.achieved_difficulty)?;
        {
            let mut le_bytes = [0u8; 64];
            self.total_accumulated_difficulty.to_little_endian(&mut le_bytes);
            tup.serialize_element(le_bytes.as_slice())?;
        }
        tup.serialize_element(&self.accumulated_monero_randomx_difficulty)?;
        tup.serialize_element(&self.accumulated_tari_randomx_difficulty)?;
        tup.serialize_element(&self.accumulated_sha3x_difficulty)?;
        tup.serialize_element(&self.target_difficulty)?;
        tup.end()
    }
}

impl<'de> Deserialize<'de> for LmdbRowBlockHeaderAccumulatedDataV1 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V1Visitor;

        impl<'de> Visitor<'de> for V1Visitor {
            type Value = LmdbRowBlockHeaderAccumulatedDataV1;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a tuple of 8 elements for LmdbRowBlockHeaderAccumulatedDataV1")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"8 fields"))?;
                let total_kernel_offset = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"8 fields"))?;
                let achieved_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &"8 fields"))?;
                let le_bytes: Vec<u8> = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &"8 fields"))?;
                if le_bytes.len() != 64 {
                    return Err(de::Error::custom(format!(
                        "expected 64 bytes for U512, got {}",
                        le_bytes.len()
                    )));
                }
                let total_accumulated_difficulty = U512::from_little_endian(&le_bytes);
                let accumulated_monero_randomx_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &"8 fields"))?;
                let accumulated_tari_randomx_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(5, &"8 fields"))?;
                let accumulated_sha3x_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(6, &"8 fields"))?;
                let target_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(7, &"8 fields"))?;
                Ok(LmdbRowBlockHeaderAccumulatedDataV1 {
                    hash,
                    total_kernel_offset,
                    achieved_difficulty,
                    total_accumulated_difficulty,
                    accumulated_monero_randomx_difficulty,
                    accumulated_tari_randomx_difficulty,
                    accumulated_sha3x_difficulty,
                    target_difficulty,
                })
            }
        }

        d.deserialize_tuple(8, V1Visitor)
    }
}

impl From<LmdbRowBlockHeaderAccumulatedDataV1> for BlockHeaderAccumulatedData {
    fn from(data: LmdbRowBlockHeaderAccumulatedDataV1) -> Self {
        BlockHeaderAccumulatedData {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset,
            achieved_difficulty: data.achieved_difficulty,
            total_accumulated_difficulty: data.total_accumulated_difficulty,
            accumulated_monero_randomx_difficulty: data.accumulated_monero_randomx_difficulty,
            accumulated_tari_randomx_difficulty: data.accumulated_tari_randomx_difficulty,
            accumulated_sha3x_difficulty: data.accumulated_sha3x_difficulty,
            accumulated_cuckaroo_difficulty: AccumulatedDifficulty::min(),
            target_difficulty: data.target_difficulty,
        }
    }
}

impl From<&BlockHeaderAccumulatedData> for LmdbRowBlockHeaderAccumulatedDataV1 {
    fn from(data: &BlockHeaderAccumulatedData) -> Self {
        LmdbRowBlockHeaderAccumulatedDataV1 {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset.clone(),
            achieved_difficulty: data.achieved_difficulty,
            total_accumulated_difficulty: data.total_accumulated_difficulty,
            accumulated_monero_randomx_difficulty: data.accumulated_monero_randomx_difficulty,
            accumulated_tari_randomx_difficulty: data.accumulated_tari_randomx_difficulty,
            accumulated_sha3x_difficulty: data.accumulated_sha3x_difficulty,
            target_difficulty: data.target_difficulty,
        }
    }
}

/// LMDB row data for block header accumulated data — schema version 2.
///
/// # Serialization format
///
/// Fields are serialized as an ordered tuple using bincode. **The field order must not change** —
/// reordering is a breaking schema change that requires a new DB table and migration.
///
/// | # | Field                                  | Bincode bytes                                |
/// |---|----------------------------------------|----------------------------------------------|
/// | 0 | `hash`                                 | 32 bytes (FixedHash / `[u8; 32]`)            |
/// | 1 | `total_kernel_offset`                  | 32 bytes (PrivateKey / RistrettoSecretKey)   |
/// | 2 | `achieved_difficulty`                  | 8 bytes (u64, little-endian)                 |
/// | 3 | `total_accumulated_difficulty`         | 8-byte length prefix + 64 bytes (U512 LE)   |
/// | 4 | `accumulated_monero_randomx_difficulty`| 16 bytes (u128, little-endian)               |
/// | 5 | `accumulated_tari_randomx_difficulty`  | 16 bytes (u128, little-endian)               |
/// | 6 | `accumulated_sha3x_difficulty`         | 16 bytes (u128, little-endian)               |
/// | 7 | `accumulated_cuckaroo_difficulty`      | 16 bytes (u128, little-endian)               |
/// | 8 | `target_difficulty`                    | 8 bytes (u64, little-endian)                 |
#[derive(Debug)]
pub struct LmdbRowBlockHeaderAccumulatedDataV2 {
    pub hash: HashOutput,
    pub total_kernel_offset: PrivateKey,
    pub achieved_difficulty: Difficulty,
    pub total_accumulated_difficulty: U512,
    pub accumulated_monero_randomx_difficulty: AccumulatedDifficulty,
    pub accumulated_tari_randomx_difficulty: AccumulatedDifficulty,
    pub accumulated_sha3x_difficulty: AccumulatedDifficulty,
    pub accumulated_cuckaroo_difficulty: AccumulatedDifficulty,
    pub target_difficulty: Difficulty,
}

impl Serialize for LmdbRowBlockHeaderAccumulatedDataV2 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // IMPORTANT: The serialization order of fields below is part of the LMDB schema.
        // DO NOT reorder, rename, or remove serialize_element calls — it is a breaking schema change.
        //
        // `U512` is from an external crate (`primitive_types`). We explicitly serialize it as 64
        // little-endian bytes to avoid depending on that crate's own serde implementation, which
        // may change across versions. The format (8-byte length prefix + 64 raw bytes) is
        // binary-compatible with the previous `#[derive(Serialize)]` output.
        let mut tup = s.serialize_tuple(9)?;
        tup.serialize_element(&self.hash)?;
        tup.serialize_element(&self.total_kernel_offset)?;
        tup.serialize_element(&self.achieved_difficulty)?;
        {
            let mut le_bytes = [0u8; 64];
            self.total_accumulated_difficulty.to_little_endian(&mut le_bytes);
            tup.serialize_element(le_bytes.as_slice())?;
        }
        tup.serialize_element(&self.accumulated_monero_randomx_difficulty)?;
        tup.serialize_element(&self.accumulated_tari_randomx_difficulty)?;
        tup.serialize_element(&self.accumulated_sha3x_difficulty)?;
        tup.serialize_element(&self.accumulated_cuckaroo_difficulty)?;
        tup.serialize_element(&self.target_difficulty)?;
        tup.end()
    }
}

impl<'de> Deserialize<'de> for LmdbRowBlockHeaderAccumulatedDataV2 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V2Visitor;

        impl<'de> Visitor<'de> for V2Visitor {
            type Value = LmdbRowBlockHeaderAccumulatedDataV2;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a tuple of 9 elements for LmdbRowBlockHeaderAccumulatedDataV2")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"9 fields"))?;
                let total_kernel_offset = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"9 fields"))?;
                let achieved_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &"9 fields"))?;
                let le_bytes: Vec<u8> = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &"9 fields"))?;
                if le_bytes.len() != 64 {
                    return Err(de::Error::custom(format!(
                        "expected 64 bytes for U512, got {}",
                        le_bytes.len()
                    )));
                }
                let total_accumulated_difficulty = U512::from_little_endian(&le_bytes);
                let accumulated_monero_randomx_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &"9 fields"))?;
                let accumulated_tari_randomx_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(5, &"9 fields"))?;
                let accumulated_sha3x_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(6, &"9 fields"))?;
                let accumulated_cuckaroo_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(7, &"9 fields"))?;
                let target_difficulty = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(8, &"9 fields"))?;
                Ok(LmdbRowBlockHeaderAccumulatedDataV2 {
                    hash,
                    total_kernel_offset,
                    achieved_difficulty,
                    total_accumulated_difficulty,
                    accumulated_monero_randomx_difficulty,
                    accumulated_tari_randomx_difficulty,
                    accumulated_sha3x_difficulty,
                    accumulated_cuckaroo_difficulty,
                    target_difficulty,
                })
            }
        }

        d.deserialize_tuple(9, V2Visitor)
    }
}

impl From<LmdbRowBlockHeaderAccumulatedDataV2> for BlockHeaderAccumulatedData {
    fn from(data: LmdbRowBlockHeaderAccumulatedDataV2) -> Self {
        BlockHeaderAccumulatedData {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset.clone(),
            achieved_difficulty: data.achieved_difficulty,
            total_accumulated_difficulty: data.total_accumulated_difficulty,
            accumulated_monero_randomx_difficulty: data.accumulated_monero_randomx_difficulty,
            accumulated_tari_randomx_difficulty: data.accumulated_tari_randomx_difficulty,
            accumulated_sha3x_difficulty: data.accumulated_sha3x_difficulty,
            accumulated_cuckaroo_difficulty: data.accumulated_cuckaroo_difficulty,
            target_difficulty: data.target_difficulty,
        }
    }
}

impl From<&BlockHeaderAccumulatedData> for LmdbRowBlockHeaderAccumulatedDataV2 {
    fn from(data: &BlockHeaderAccumulatedData) -> Self {
        LmdbRowBlockHeaderAccumulatedDataV2 {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset.clone(),
            achieved_difficulty: data.achieved_difficulty,
            total_accumulated_difficulty: data.total_accumulated_difficulty,
            accumulated_monero_randomx_difficulty: data.accumulated_monero_randomx_difficulty,
            accumulated_tari_randomx_difficulty: data.accumulated_tari_randomx_difficulty,
            accumulated_sha3x_difficulty: data.accumulated_sha3x_difficulty,
            accumulated_cuckaroo_difficulty: data.accumulated_cuckaroo_difficulty,
            target_difficulty: data.target_difficulty,
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;
    use tari_crypto::keys::SecretKey;

    use super::*;

    fn make_test_v1() -> LmdbRowBlockHeaderAccumulatedDataV1 {
        LmdbRowBlockHeaderAccumulatedDataV1 {
            hash: HashOutput::from([1u8; 32]),
            total_kernel_offset: PrivateKey::random(&mut OsRng),
            achieved_difficulty: Difficulty::from_u64(12345).unwrap(),
            total_accumulated_difficulty: U512::from_dec_str("99999999999999999999999999").unwrap(),
            accumulated_monero_randomx_difficulty: AccumulatedDifficulty::from_u128(111).unwrap(),
            accumulated_tari_randomx_difficulty: AccumulatedDifficulty::from_u128(222).unwrap(),
            accumulated_sha3x_difficulty: AccumulatedDifficulty::from_u128(333).unwrap(),
            target_difficulty: Difficulty::from_u64(9999).unwrap(),
        }
    }

    fn make_test_v2() -> LmdbRowBlockHeaderAccumulatedDataV2 {
        LmdbRowBlockHeaderAccumulatedDataV2 {
            hash: HashOutput::from([2u8; 32]),
            total_kernel_offset: PrivateKey::random(&mut OsRng),
            achieved_difficulty: Difficulty::from_u64(54321).unwrap(),
            total_accumulated_difficulty: U512::from_dec_str("88888888888888888888888888").unwrap(),
            accumulated_monero_randomx_difficulty: AccumulatedDifficulty::from_u128(444).unwrap(),
            accumulated_tari_randomx_difficulty: AccumulatedDifficulty::from_u128(555).unwrap(),
            accumulated_sha3x_difficulty: AccumulatedDifficulty::from_u128(666).unwrap(),
            accumulated_cuckaroo_difficulty: AccumulatedDifficulty::from_u128(777).unwrap(),
            target_difficulty: Difficulty::from_u64(7777).unwrap(),
        }
    }

    #[test]
    fn v1_round_trips_via_bincode() {
        let original = make_test_v1();
        let bytes = bincode::serialize(&original).expect("serialization failed");
        let decoded: LmdbRowBlockHeaderAccumulatedDataV1 =
            bincode::deserialize(&bytes).expect("deserialization failed");
        assert_eq!(original.hash, decoded.hash);
        assert_eq!(original.total_kernel_offset, decoded.total_kernel_offset);
        assert_eq!(original.achieved_difficulty, decoded.achieved_difficulty);
        assert_eq!(
            original.total_accumulated_difficulty,
            decoded.total_accumulated_difficulty
        );
        assert_eq!(
            original.accumulated_monero_randomx_difficulty,
            decoded.accumulated_monero_randomx_difficulty
        );
        assert_eq!(
            original.accumulated_tari_randomx_difficulty,
            decoded.accumulated_tari_randomx_difficulty
        );
        assert_eq!(
            original.accumulated_sha3x_difficulty,
            decoded.accumulated_sha3x_difficulty
        );
        assert_eq!(original.target_difficulty, decoded.target_difficulty);
    }

    #[test]
    fn v2_round_trips_via_bincode() {
        let original = make_test_v2();
        let bytes = bincode::serialize(&original).expect("serialization failed");
        let decoded: LmdbRowBlockHeaderAccumulatedDataV2 =
            bincode::deserialize(&bytes).expect("deserialization failed");
        assert_eq!(original.hash, decoded.hash);
        assert_eq!(original.total_kernel_offset, decoded.total_kernel_offset);
        assert_eq!(original.achieved_difficulty, decoded.achieved_difficulty);
        assert_eq!(
            original.total_accumulated_difficulty,
            decoded.total_accumulated_difficulty
        );
        assert_eq!(
            original.accumulated_monero_randomx_difficulty,
            decoded.accumulated_monero_randomx_difficulty
        );
        assert_eq!(
            original.accumulated_tari_randomx_difficulty,
            decoded.accumulated_tari_randomx_difficulty
        );
        assert_eq!(
            original.accumulated_sha3x_difficulty,
            decoded.accumulated_sha3x_difficulty
        );
        assert_eq!(
            original.accumulated_cuckaroo_difficulty,
            decoded.accumulated_cuckaroo_difficulty
        );
        assert_eq!(original.target_difficulty, decoded.target_difficulty);
    }

    /// Verify that `U512` is serialized as a length-prefixed 64-byte little-endian byte slice.
    /// This test pins the on-disk binary format so accidental changes to U512 serialization are caught.
    #[test]
    fn u512_is_serialized_as_64_le_bytes_with_length_prefix() {
        // Construct a V1 row where total_accumulated_difficulty is set to a known value.
        let row = LmdbRowBlockHeaderAccumulatedDataV1 {
            hash: HashOutput::from([0u8; 32]),
            total_kernel_offset: PrivateKey::random(&mut OsRng),
            achieved_difficulty: Difficulty::min(),
            // U512 = 1  →  LE bytes = [0x01, 0x00, 0x00, ..., 0x00] (64 bytes)
            total_accumulated_difficulty: U512::one(),
            accumulated_monero_randomx_difficulty: AccumulatedDifficulty::min(),
            accumulated_tari_randomx_difficulty: AccumulatedDifficulty::min(),
            accumulated_sha3x_difficulty: AccumulatedDifficulty::min(),
            target_difficulty: Difficulty::min(),
        };

        let bytes = bincode::serialize(&row).unwrap();
        // Layout (bincode):
        //   hash (FixedHash transparent over [u8;32]):  32 bytes (fixed-size array, no length prefix)
        //   total_kernel_offset (RistrettoSecretKey):   8 bytes length (=32) + 32 bytes = 40 bytes
        //     Note: RistrettoSecretKey::serialize calls serialize_bytes(), adding an 8-byte length prefix.
        //   achieved_difficulty (Difficulty(u64)):       8 bytes
        //   total_accumulated_difficulty (U512):         8 bytes length (=64) + 64 bytes = 72 bytes
        //   ... followed by the three AccumulatedDifficulty fields and target_difficulty
        let hash_size = 32;
        // RistrettoSecretKey::serialize uses serialize_bytes (8-byte length + 32 raw bytes = 40):
        let key_size = 8 + 32;
        let difficulty_size = 8;
        let u512_offset = hash_size + key_size + difficulty_size; // = 80
        let length_prefix = u64::from_le_bytes(bytes[u512_offset..u512_offset + 8].try_into().unwrap());
        assert_eq!(length_prefix, 64, "U512 length prefix must be 64");

        let u512_data = &bytes[u512_offset + 8..u512_offset + 8 + 64];
        // U512::one() in little-endian: first byte is 1, rest are 0
        assert_eq!(u512_data[0], 1, "first LE byte of U512(1) must be 1");
        assert!(
            u512_data[1..].iter().all(|&b| b == 0),
            "remaining LE bytes of U512(1) must be 0"
        );

        // Also verify round-trip
        let decoded: LmdbRowBlockHeaderAccumulatedDataV1 = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.total_accumulated_difficulty, U512::one());
    }
}
