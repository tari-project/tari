//  Copyright 2022, The Tari Project
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

use std::{collections::VecDeque, fmt, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{
    de::{self, SeqAccess, Visitor},
    ser::SerializeTuple,
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use tari_common_types::types::HashOutput;
use tari_node_components::blocks::ChainBlock;

/// A record of a chain reorganisation, stored in LMDB.
///
/// # Serialization format
///
/// Fields are serialized as an ordered tuple using bincode. **The field order must not change** —
/// reordering is a breaking schema change that requires a migration.
///
/// | # | Field                | Type / Bincode bytes                      |
/// |---|----------------------|-------------------------------------------|
/// | 0 | `new_height`         | 8 bytes (u64, little-endian)              |
/// | 1 | `new_hash`           | 32 bytes (`FixedHash` / `[u8; 32]`)       |
/// | 2 | `prev_height`        | 8 bytes (u64, little-endian)              |
/// | 3 | `prev_hash`          | 32 bytes (`FixedHash` / `[u8; 32]`)       |
/// | 4 | `num_blocks_added`   | 8 bytes (u64, little-endian)              |
/// | 5 | `num_blocks_removed` | 8 bytes (u64, little-endian)              |
/// | 6 | `local_time`         | `DateTime<Utc>` (variable length)         |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reorg {
    pub new_height: u64,
    pub new_hash: HashOutput,
    pub prev_height: u64,
    pub prev_hash: HashOutput,
    pub num_blocks_added: u64,
    pub num_blocks_removed: u64,
    pub local_time: DateTime<Utc>,
}

impl Reorg {
    pub fn from_reorged_blocks(added: &VecDeque<Arc<ChainBlock>>, removed: &[Arc<ChainBlock>]) -> Self {
        // Expects blocks to be ordered sequentially highest height to lowest (as in rewind_to_height)
        Self {
            new_height: added.front().map(|b| b.header().height).unwrap_or_default(),
            new_hash: added.front().map(|b| *b.hash()).unwrap_or_default(),
            prev_height: removed.first().map(|b| b.header().height).unwrap_or_default(),
            prev_hash: removed.first().map(|b| *b.hash()).unwrap_or_default(),
            num_blocks_added: added.len() as u64,
            num_blocks_removed: removed.len() as u64,
            local_time: Utc::now(),
        }
    }
}

impl Serialize for Reorg {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // IMPORTANT: The serialization order of fields below is part of the LMDB schema.
        // DO NOT reorder, rename, or remove serialize_element calls — it is a breaking schema change.
        let mut tup = s.serialize_tuple(7)?;
        tup.serialize_element(&self.new_height)?;
        tup.serialize_element(&self.new_hash)?;
        tup.serialize_element(&self.prev_height)?;
        tup.serialize_element(&self.prev_hash)?;
        tup.serialize_element(&self.num_blocks_added)?;
        tup.serialize_element(&self.num_blocks_removed)?;
        tup.serialize_element(&self.local_time)?;
        tup.end()
    }
}

impl<'de> Deserialize<'de> for Reorg {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ReorgVisitor;

        impl<'de> Visitor<'de> for ReorgVisitor {
            type Value = Reorg;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a tuple of 7 elements for Reorg")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let new_height = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"7 fields"))?;
                let new_hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"7 fields"))?;
                let prev_height = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &"7 fields"))?;
                let prev_hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &"7 fields"))?;
                let num_blocks_added = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &"7 fields"))?;
                let num_blocks_removed = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(5, &"7 fields"))?;
                let local_time = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(6, &"7 fields"))?;
                Ok(Reorg {
                    new_height,
                    new_hash,
                    prev_height,
                    prev_hash,
                    num_blocks_added,
                    num_blocks_removed,
                    local_time,
                })
            }
        }

        d.deserialize_tuple(7, ReorgVisitor)
    }
}
