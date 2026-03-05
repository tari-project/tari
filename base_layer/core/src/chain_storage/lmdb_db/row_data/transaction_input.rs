// Copyright 2025. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::fmt;

use serde::{
    de::{self, SeqAccess, Visitor},
    ser::SerializeTuple,
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use tari_common_types::types::HashOutput;
use tari_transaction_components::transaction_components::TransactionInput;

/// LMDB row data for a spent transaction input.
///
/// # Serialization format
///
/// Fields are serialized as an ordered tuple using bincode. **The field order must not change** —
/// reordering is a breaking schema change that requires a new DB table and migration.
///
/// | # | Field             | Type / Bincode bytes                    |
/// |---|-------------------|-----------------------------------------|
/// | 0 | `input`           | `TransactionInput` (variable length)    |
/// | 1 | `header_hash`     | 32 bytes (`FixedHash` / `[u8; 32]`)     |
/// | 2 | `spent_timestamp` | 8 bytes (u64, little-endian)            |
/// | 3 | `spent_height`    | 8 bytes (u64, little-endian)            |
/// | 4 | `hash`            | 32 bytes (`FixedHash` / `[u8; 32]`)     |
#[derive(Debug)]
pub struct TransactionInputRowData {
    pub input: TransactionInput,
    pub header_hash: HashOutput,
    pub spent_timestamp: u64,
    pub spent_height: u64,
    pub hash: HashOutput,
}

impl Serialize for TransactionInputRowData {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // IMPORTANT: The serialization order of fields below is part of the LMDB schema.
        // DO NOT reorder, rename, or remove serialize_element calls — it is a breaking schema change.
        let mut tup = s.serialize_tuple(5)?;
        tup.serialize_element(&self.input)?;
        tup.serialize_element(&self.header_hash)?;
        tup.serialize_element(&self.spent_timestamp)?;
        tup.serialize_element(&self.spent_height)?;
        tup.serialize_element(&self.hash)?;
        tup.end()
    }
}

impl<'de> Deserialize<'de> for TransactionInputRowData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct TxInputVisitor;

        impl<'de> Visitor<'de> for TxInputVisitor {
            type Value = TransactionInputRowData;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a tuple of 5 elements for TransactionInputRowData")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let input = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"5 fields"))?;
                let header_hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"5 fields"))?;
                let spent_timestamp = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &"5 fields"))?;
                let spent_height = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &"5 fields"))?;
                let hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &"5 fields"))?;
                Ok(TransactionInputRowData {
                    input,
                    header_hash,
                    spent_timestamp,
                    spent_height,
                    hash,
                })
            }
        }

        d.deserialize_tuple(5, TxInputVisitor)
    }
}

/// Reference version of `TransactionInputRowData` — used when inserting to avoid cloning the input.
///
/// This struct must mirror the serialization order of `TransactionInputRowData` exactly.
///
/// | # | Field             | Type / Bincode bytes                    |
/// |---|-------------------|-----------------------------------------|
/// | 0 | `input`           | `&TransactionInput` (variable length)   |
/// | 1 | `header_hash`     | 32 bytes (`FixedHash` / `[u8; 32]`)     |
/// | 2 | `spent_timestamp` | 8 bytes (u64, little-endian)            |
/// | 3 | `spent_height`    | 8 bytes (u64, little-endian)            |
/// | 4 | `hash`            | 32 bytes (`FixedHash` / `[u8; 32]`)     |
#[derive(Debug)]
pub struct TransactionInputRowDataRef<'a> {
    pub input: &'a TransactionInput,
    #[allow(clippy::ptr_arg)]
    pub header_hash: &'a HashOutput,
    pub spent_timestamp: u64,
    pub spent_height: u64,
    #[allow(clippy::ptr_arg)]
    pub hash: &'a HashOutput,
}

impl Serialize for TransactionInputRowDataRef<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // IMPORTANT: The serialization order of fields below is part of the LMDB schema.
        // DO NOT reorder, rename, or remove serialize_element calls — it is a breaking schema change.
        // This MUST produce bytes identical to TransactionInputRowData serialization.
        let mut tup = s.serialize_tuple(5)?;
        tup.serialize_element(self.input)?;
        tup.serialize_element(self.header_hash)?;
        tup.serialize_element(&self.spent_timestamp)?;
        tup.serialize_element(&self.spent_height)?;
        tup.serialize_element(self.hash)?;
        tup.end()
    }
}

#[cfg(test)]
mod tests {
    use tari_common_types::types::{ComAndPubSignature, FixedHash};
    use tari_transaction_components::transaction_components::{SpentOutput, TransactionInput, TransactionInputVersion};

    use super::*;

    fn make_test_input() -> TransactionInput {
        TransactionInput::new(
            TransactionInputVersion::get_current_version(),
            SpentOutput::OutputHash(FixedHash::zero()),
            Default::default(),
            ComAndPubSignature::default(),
        )
    }

    #[test]
    fn round_trips_via_bincode() {
        let original = TransactionInputRowData {
            input: make_test_input(),
            header_hash: FixedHash::from([1u8; 32]),
            spent_timestamp: 1_700_000_001,
            spent_height: 99,
            hash: FixedHash::from([3u8; 32]),
        };
        let bytes = bincode::serialize(&original).expect("serialize failed");
        let decoded: TransactionInputRowData = bincode::deserialize(&bytes).expect("deserialize failed");
        assert_eq!(original.header_hash, decoded.header_hash);
        assert_eq!(original.spent_timestamp, decoded.spent_timestamp);
        assert_eq!(original.spent_height, decoded.spent_height);
        assert_eq!(original.hash, decoded.hash);
    }

    #[test]
    fn ref_produces_same_bytes_as_owned() {
        let input = make_test_input();
        let header_hash = FixedHash::from([1u8; 32]);
        let hash = FixedHash::from([3u8; 32]);

        let owned = TransactionInputRowData {
            input: input.clone(),
            header_hash,
            spent_timestamp: 1_700_000_001,
            spent_height: 99,
            hash,
        };
        let by_ref = TransactionInputRowDataRef {
            input: &input,
            header_hash: &header_hash,
            spent_timestamp: 1_700_000_001,
            spent_height: 99,
            hash: &hash,
        };

        let owned_bytes = bincode::serialize(&owned).expect("serialize owned failed");
        let ref_bytes = bincode::serialize(&by_ref).expect("serialize ref failed");
        assert_eq!(owned_bytes, ref_bytes, "owned and ref must produce identical bytes");
    }
}
