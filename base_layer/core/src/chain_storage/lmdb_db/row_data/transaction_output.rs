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
use tari_transaction_components::transaction_components::TransactionOutput;

/// LMDB row data for a transaction output (UTXO set).
///
/// # Serialization format
///
/// Fields are serialized as an ordered tuple using bincode. **The field order must not change** —
/// reordering is a breaking schema change that requires a new DB table and migration.
///
/// | # | Field           | Type / Bincode bytes                     |
/// |---|-----------------|------------------------------------------|
/// | 0 | `output`        | `TransactionOutput` (variable length)    |
/// | 1 | `header_hash`   | 32 bytes (`FixedHash` / `[u8; 32]`)      |
/// | 2 | `hash`          | 32 bytes (`FixedHash` / `[u8; 32]`)      |
/// | 3 | `mined_height`  | 8 bytes (u64, little-endian)             |
/// | 4 | `mined_timestamp`| 8 bytes (u64, little-endian)            |
#[derive(Debug)]
pub struct TransactionOutputRowData {
    pub output: TransactionOutput,
    pub header_hash: HashOutput,
    pub hash: HashOutput,
    pub mined_height: u64,
    pub mined_timestamp: u64,
}

impl Serialize for TransactionOutputRowData {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // IMPORTANT: The serialization order of fields below is part of the LMDB schema.
        // DO NOT reorder, rename, or remove serialize_element calls — it is a breaking schema change.
        let mut tup = s.serialize_tuple(5)?;
        tup.serialize_element(&self.output)?;
        tup.serialize_element(&self.header_hash)?;
        tup.serialize_element(&self.hash)?;
        tup.serialize_element(&self.mined_height)?;
        tup.serialize_element(&self.mined_timestamp)?;
        tup.end()
    }
}

impl<'de> Deserialize<'de> for TransactionOutputRowData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct TxOutputVisitor;

        impl<'de> Visitor<'de> for TxOutputVisitor {
            type Value = TransactionOutputRowData;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a tuple of 5 elements for TransactionOutputRowData")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let output = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"5 fields"))?;
                let header_hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"5 fields"))?;
                let hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &"5 fields"))?;
                let mined_height = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &"5 fields"))?;
                let mined_timestamp = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &"5 fields"))?;
                Ok(TransactionOutputRowData {
                    output,
                    header_hash,
                    hash,
                    mined_height,
                    mined_timestamp,
                })
            }
        }

        d.deserialize_tuple(5, TxOutputVisitor)
    }
}

#[cfg(test)]
mod tests {
    use tari_common_types::types::FixedHash;
    use tari_transaction_components::transaction_components::{
        TransactionOutput,
        TransactionOutputVersion,
    };

    use super::*;

    fn make_test_output() -> TransactionOutput {
        TransactionOutput::new(
            TransactionOutputVersion::get_current_version(),
            Default::default(),
            Default::default(),
            None,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        )
    }

    #[test]
    fn round_trips_via_bincode() {
        let original = TransactionOutputRowData {
            output: make_test_output(),
            header_hash: FixedHash::from([1u8; 32]),
            hash: FixedHash::from([2u8; 32]),
            mined_height: 42,
            mined_timestamp: 1_700_000_000,
        };
        let bytes = bincode::serialize(&original).expect("serialize failed");
        let decoded: TransactionOutputRowData = bincode::deserialize(&bytes).expect("deserialize failed");
        assert_eq!(original.header_hash, decoded.header_hash);
        assert_eq!(original.hash, decoded.hash);
        assert_eq!(original.mined_height, decoded.mined_height);
        assert_eq!(original.mined_timestamp, decoded.mined_timestamp);
    }
}
