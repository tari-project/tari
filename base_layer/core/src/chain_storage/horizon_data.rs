// Copyright 2021. The Tari Project
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
use tari_common_types::types::CompressedCommitment;

/// Horizon data for pruned nodes, stored in the LMDB metadata database.
///
/// # Serialization format
///
/// Fields are serialized as an ordered tuple using bincode. **The field order must not change** —
/// reordering is a breaking schema change that requires a migration.
///
/// | # | Field        | Type / Bincode bytes                        |
/// |---|--------------|---------------------------------------------|
/// | 0 | `kernel_sum` | `CompressedCommitment` (variable length)    |
/// | 1 | `utxo_sum`   | `CompressedCommitment` (variable length)    |
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HorizonData {
    kernel_sum: CompressedCommitment,
    utxo_sum: CompressedCommitment,
}

impl HorizonData {
    pub fn new(kernel_sum: CompressedCommitment, utxo_sum: CompressedCommitment) -> Self {
        HorizonData { kernel_sum, utxo_sum }
    }

    pub fn zero() -> Self {
        Default::default()
    }

    pub fn kernel_sum(&self) -> &CompressedCommitment {
        &self.kernel_sum
    }

    pub fn utxo_sum(&self) -> &CompressedCommitment {
        &self.utxo_sum
    }
}

impl Serialize for HorizonData {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // IMPORTANT: The serialization order of fields below is part of the LMDB schema.
        // DO NOT reorder, rename, or remove serialize_element calls — it is a breaking schema change.
        let mut tup = s.serialize_tuple(2)?;
        tup.serialize_element(&self.kernel_sum)?;
        tup.serialize_element(&self.utxo_sum)?;
        tup.end()
    }
}

impl<'de> Deserialize<'de> for HorizonData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct HorizonDataVisitor;

        impl<'de> Visitor<'de> for HorizonDataVisitor {
            type Value = HorizonData;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a tuple of 2 elements for HorizonData")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let kernel_sum = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"2 fields"))?;
                let utxo_sum = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"2 fields"))?;
                Ok(HorizonData { kernel_sum, utxo_sum })
            }
        }

        d.deserialize_tuple(2, HorizonDataVisitor)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn coverage_horizon_data() {
        let obj = HorizonData::zero();
        obj.kernel_sum();
        obj.utxo_sum();
        drop(obj.clone());
    }

    #[test]
    fn round_trips_via_bincode() {
        let original = HorizonData::zero();
        let bytes = bincode::serialize(&original).expect("serialize failed");
        let decoded: HorizonData = bincode::deserialize(&bytes).expect("deserialize failed");
        assert_eq!(original, decoded);
    }
}
