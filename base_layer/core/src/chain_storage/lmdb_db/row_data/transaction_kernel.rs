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
use tari_transaction_components::transaction_components::TransactionKernel;

/// LMDB row data for a transaction kernel.
///
/// # Serialization format
///
/// Fields are serialized as an ordered tuple using bincode. **The field order must not change** —
/// reordering is a breaking schema change that requires a new DB table and migration.
///
/// | # | Field          | Type / Bincode bytes                    |
/// |---|----------------|-----------------------------------------|
/// | 0 | `kernel`       | `TransactionKernel` (variable length)   |
/// | 1 | `header_hash`  | 32 bytes (`FixedHash` / `[u8; 32]`)     |
/// | 2 | `mmr_position` | 8 bytes (u64, little-endian)            |
/// | 3 | `hash`         | 32 bytes (`FixedHash` / `[u8; 32]`)     |
#[derive(Debug)]
pub struct TransactionKernelRowData {
    pub kernel: TransactionKernel,
    pub header_hash: HashOutput,
    pub mmr_position: u64,
    pub hash: HashOutput,
}

impl Serialize for TransactionKernelRowData {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // IMPORTANT: The serialization order of fields below is part of the LMDB schema.
        // DO NOT reorder, rename, or remove serialize_element calls — it is a breaking schema change.
        let mut tup = s.serialize_tuple(4)?;
        tup.serialize_element(&self.kernel)?;
        tup.serialize_element(&self.header_hash)?;
        tup.serialize_element(&self.mmr_position)?;
        tup.serialize_element(&self.hash)?;
        tup.end()
    }
}

impl<'de> Deserialize<'de> for TransactionKernelRowData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct TxKernelVisitor;

        impl<'de> Visitor<'de> for TxKernelVisitor {
            type Value = TransactionKernelRowData;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a tuple of 4 elements for TransactionKernelRowData")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let kernel = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"4 fields"))?;
                let header_hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"4 fields"))?;
                let mmr_position = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &"4 fields"))?;
                let hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &"4 fields"))?;
                Ok(TransactionKernelRowData {
                    kernel,
                    header_hash,
                    mmr_position,
                    hash,
                })
            }
        }

        d.deserialize_tuple(4, TxKernelVisitor)
    }
}

#[cfg(test)]
mod tests {
    use tari_common_types::types::{CompressedCommitment, CompressedSignature, FixedHash};
    use tari_transaction_components::{
        transaction_components::{KernelFeatures, TransactionKernelVersion},
        MicroMinotari,
    };

    use super::*;

    fn make_test_kernel() -> TransactionKernel {
        TransactionKernel::new(
            TransactionKernelVersion::get_current_version(),
            KernelFeatures::default(),
            MicroMinotari::zero(),
            0,
            CompressedCommitment::default(),
            CompressedSignature::default(),
            None,
        )
    }

    #[test]
    fn round_trips_via_bincode() {
        let original = TransactionKernelRowData {
            kernel: make_test_kernel(),
            header_hash: FixedHash::from([1u8; 32]),
            mmr_position: 7,
            hash: FixedHash::from([5u8; 32]),
        };
        let bytes = bincode::serialize(&original).expect("serialize failed");
        let decoded: TransactionKernelRowData = bincode::deserialize(&bytes).expect("deserialize failed");
        assert_eq!(original.header_hash, decoded.header_hash);
        assert_eq!(original.mmr_position, decoded.mmr_position);
        assert_eq!(original.hash, decoded.hash);
    }
}
