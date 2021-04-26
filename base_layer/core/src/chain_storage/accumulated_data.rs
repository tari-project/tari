//  Copyright 2020, The Tari Project
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

use crate::{
    blocks::{Block, BlockHeader},
    chain_storage::ChainStorageError,
    proof_of_work::{AchievedTargetDifficulty, Difficulty, PowAlgorithm},
    tari_utilities::Hashable,
    transactions::{
        aggregated_body::AggregateBody,
        types::{BlindingFactor, Commitment, HashOutput},
    },
};
use croaring::Bitmap;
use log::*;
use num_format::{Locale, ToFormattedString};
use serde::{
    de,
    de::{MapAccess, SeqAccess, Visitor},
    ser::SerializeStruct,
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use std::{
    fmt,
    fmt::{Display, Formatter},
    sync::Arc,
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_mmr::{pruned_hashset::PrunedHashSet, ArrayLike};

const LOG_TARGET: &str = "c::bn::acc_data";

#[derive(Debug)]
// Helper struct to serialize and deserialize Bitmap
pub struct DeletedBitmap {
    pub(super) deleted: Bitmap,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockAccumulatedData {
    pub(super) kernels: PrunedHashSet,
    pub(super) outputs: PrunedHashSet,
    pub(super) deleted: DeletedBitmap,
    pub(super) range_proofs: PrunedHashSet,
    pub(super) kernel_sum: Commitment,
}

impl BlockAccumulatedData {
    pub fn new(
        kernels: PrunedHashSet,
        outputs: PrunedHashSet,
        range_proofs: PrunedHashSet,
        deleted: Bitmap,
        total_kernel_sum: Commitment,
    ) -> Self
    {
        Self {
            kernels,
            outputs,
            range_proofs,
            deleted: DeletedBitmap { deleted },
            kernel_sum: total_kernel_sum,
        }
    }

    #[inline(always)]
    pub fn deleted(&self) -> &Bitmap {
        &self.deleted.deleted
    }

    pub fn dissolve(self) -> (PrunedHashSet, PrunedHashSet, PrunedHashSet, Bitmap) {
        (self.kernels, self.outputs, self.range_proofs, self.deleted.deleted)
    }

    pub fn kernel_sum(&self) -> &Commitment {
        &self.kernel_sum
    }
}

impl Default for BlockAccumulatedData {
    fn default() -> Self {
        Self {
            kernels: Default::default(),
            outputs: Default::default(),
            deleted: DeletedBitmap {
                deleted: Bitmap::create(),
            },
            range_proofs: Default::default(),
            kernel_sum: Default::default(),
        }
    }
}

impl Display for BlockAccumulatedData {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{} output(s), {} spent, {} kernel(s), {} rangeproof(s)",
            self.outputs.len().unwrap_or(0),
            self.deleted.deleted.cardinality(),
            self.kernels.len().unwrap_or(0),
            self.range_proofs.len().unwrap_or(0)
        )
    }
}

impl Serialize for DeletedBitmap {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where S: Serializer {
        let mut s = serializer.serialize_struct("DeletedBitmap", 1)?;
        s.serialize_field("deleted", &self.deleted.serialize())?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for DeletedBitmap {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where D: Deserializer<'de> {
        const FIELDS: &[&str] = &["deleted"];

        deserializer.deserialize_struct("DeletedBitmap", FIELDS, DeletedBitmapVisitor)
    }
}

struct DeletedBitmapVisitor;

impl<'de> Visitor<'de> for DeletedBitmapVisitor {
    type Value = DeletedBitmap;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("`deleted`")
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
    where V: SeqAccess<'de> {
        let deleted: Vec<u8> = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(2, &self))?;
        Ok(DeletedBitmap {
            deleted: Bitmap::deserialize(&deleted),
        })
    }

    fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
    where V: MapAccess<'de> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Deleted,
        };
        let mut deleted = None;
        while let Some(key) = map.next_key()? {
            match key {
                Field::Deleted => {
                    if deleted.is_some() {
                        return Err(de::Error::duplicate_field("deleted"));
                    }
                    deleted = Some(map.next_value()?);
                },
            }
        }
        let deleted: Vec<u8> = deleted.ok_or_else(|| de::Error::missing_field("deleted"))?;

        Ok(DeletedBitmap {
            deleted: Bitmap::deserialize(&deleted),
        })
    }
}

pub struct BlockHeaderAccumulatedDataBuilder<'a> {
    previous_accum: &'a BlockHeaderAccumulatedData,
    hash: Option<HashOutput>,
    current_total_kernel_offset: Option<BlindingFactor>,
    current_achieved_target: Option<AchievedTargetDifficulty>,
}

impl<'a> BlockHeaderAccumulatedDataBuilder<'a> {
    pub fn from_previous(previous_accum: &'a BlockHeaderAccumulatedData) -> Self {
        Self {
            previous_accum,
            hash: None,
            current_total_kernel_offset: None,
            current_achieved_target: None,
        }
    }
}

impl BlockHeaderAccumulatedDataBuilder<'_> {
    pub fn with_hash(mut self, hash: HashOutput) -> Self {
        self.hash = Some(hash);
        self
    }

    pub fn with_total_kernel_offset(mut self, current_offset: BlindingFactor) -> Self {
        self.current_total_kernel_offset = Some(current_offset);
        self
    }

    pub fn with_achieved_target_difficulty(mut self, achieved_target: AchievedTargetDifficulty) -> Self {
        self.current_achieved_target = Some(achieved_target);
        self
    }

    pub fn build(self) -> Result<BlockHeaderAccumulatedData, ChainStorageError> {
        let previous_accum = self.previous_accum;
        let hash = self
            .hash
            .ok_or_else(|| ChainStorageError::InvalidOperation("hash not provided".to_string()))?;

        if hash == self.previous_accum.hash {
            return Err(ChainStorageError::InvalidOperation(
                "Hash was set to the same hash that is contained in previous accumulated data".to_string(),
            ));
        }

        let achieved_target = self.current_achieved_target.ok_or_else(|| {
            ChainStorageError::InvalidOperation("Current achieved difficulty not provided".to_string())
        })?;

        let (monero_diff, blake_diff) = match achieved_target.pow_algo() {
            PowAlgorithm::Monero => (
                previous_accum.accumulated_monero_difficulty + achieved_target.achieved(),
                previous_accum.accumulated_blake_difficulty,
            ),
            PowAlgorithm::Blake => unimplemented!(),
            PowAlgorithm::Sha3 => (
                previous_accum.accumulated_monero_difficulty,
                previous_accum.accumulated_blake_difficulty + achieved_target.achieved(),
            ),
        };

        let total_kernel_offset = self
            .current_total_kernel_offset
            .map(|offset| &previous_accum.total_kernel_offset + offset)
            .ok_or_else(|| ChainStorageError::InvalidOperation("total_kernel_offset not provided".to_string()))?;

        let result = BlockHeaderAccumulatedData {
            hash,
            total_kernel_offset,
            achieved_difficulty: achieved_target.achieved(),
            total_accumulated_difficulty: monero_diff.as_u64() as u128 * blake_diff.as_u64() as u128,
            accumulated_monero_difficulty: monero_diff,
            accumulated_blake_difficulty: blake_diff,
            target_difficulty: achieved_target.target(),
        };
        trace!(
            target: LOG_TARGET,
            "Calculated: Tot_acc_diff {}, Monero {}, SHA3 {}",
            result.total_accumulated_difficulty.to_formatted_string(&Locale::en),
            result.accumulated_monero_difficulty,
            result.accumulated_blake_difficulty,
        );
        Ok(result)
    }
}

// TODO: Find a better name and move into `core::blocks` mod
#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct BlockHeaderAccumulatedData {
    pub hash: HashOutput,
    pub total_kernel_offset: BlindingFactor,
    pub achieved_difficulty: Difficulty,
    pub total_accumulated_difficulty: u128,
    /// The total accumulated difficulty for each proof of work algorithms for all blocks since Genesis,
    /// but not including this block, tracked separately.
    pub accumulated_monero_difficulty: Difficulty,
    // TODO: Rename #testnetreset
    pub accumulated_blake_difficulty: Difficulty,
    /// The target difficulty for solving the current block using the specified proof of work algorithm.
    pub target_difficulty: Difficulty,
}

impl BlockHeaderAccumulatedData {
    pub fn builder(previous: &BlockHeaderAccumulatedData) -> BlockHeaderAccumulatedDataBuilder<'_> {
        BlockHeaderAccumulatedDataBuilder::from_previous(previous)
    }
}

impl Display for BlockHeaderAccumulatedData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Hash: {}", self.hash.to_hex())?;
        writeln!(f, "Achieved difficulty: {}", self.achieved_difficulty)?;
        writeln!(f, "Total accumulated difficulty: {}", self.total_accumulated_difficulty)?;
        writeln!(
            f,
            "Accumulated monero difficulty: {}",
            self.accumulated_monero_difficulty
        )?;
        writeln!(f, "Accumulated sha3 difficulty: {}", self.accumulated_blake_difficulty)?;
        writeln!(f, "Target difficulty: {}", self.target_difficulty)?;
        Ok(())
    }
}

/// A block linked to a chain.
/// A ChainHeader guarantees (i.e cannot be constructed) that the block and accumulated data correspond by hash.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ChainHeader {
    header: BlockHeader,
    accumulated_data: BlockHeaderAccumulatedData,
}

impl Display for ChainHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.header)?;
        writeln!(f, "{}", self.accumulated_data)?;
        Ok(())
    }
}

impl ChainHeader {
    /// Attempts to construct a `ChainHeader` from a `BlockHeader` and associate `BlockHeaderAccumulatedData`. Returns
    /// None if the Block and the BlockHeaderAccumulatedData do not correspond (i.e have different hashes)
    pub fn try_construct(header: BlockHeader, accumulated_data: BlockHeaderAccumulatedData) -> Option<Self> {
        if accumulated_data.hash != header.hash() {
            return None;
        }

        Some(Self {
            header,
            accumulated_data,
        })
    }

    #[inline]
    pub fn height(&self) -> u64 {
        self.header.height
    }

    #[inline]
    pub fn hash(&self) -> &HashOutput {
        &self.accumulated_data.hash
    }

    #[inline]
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    #[inline]
    pub fn accumulated_data(&self) -> &BlockHeaderAccumulatedData {
        &self.accumulated_data
    }

    #[inline]
    pub fn into_parts(self) -> (BlockHeader, BlockHeaderAccumulatedData) {
        (self.header, self.accumulated_data)
    }

    #[inline]
    pub fn into_header(self) -> BlockHeader {
        self.header
    }

    pub fn upgrade_to_chain_block(self, body: AggregateBody) -> ChainBlock {
        // NOTE: Panic cannot occur because a ChainBlock has the same guarantees as ChainHeader
        ChainBlock::try_construct(Arc::new(Block::new(self.header, body)), self.accumulated_data).unwrap()
    }
}

/// A block linked to a chain.
/// A ChainBlock MUST have the same or stronger guarantees than `ChainHeader`
#[derive(Debug, Clone, PartialEq)]
pub struct ChainBlock {
    accumulated_data: BlockHeaderAccumulatedData,
    block: Arc<Block>,
}

impl ChainBlock {
    /// Attempts to construct a `ChainBlock` from a `Block` and associate `BlockHeaderAccumulatedData`. Returns None if
    /// the Block and the BlockHeaderAccumulatedData do not correspond (i.e have different hashes)
    pub fn try_construct(block: Arc<Block>, accumulated_data: BlockHeaderAccumulatedData) -> Option<Self> {
        if accumulated_data.hash != block.hash() {
            return None;
        }

        Some(Self {
            block,
            accumulated_data,
        })
    }

    #[inline]
    pub fn height(&self) -> u64 {
        self.block.header.height
    }

    #[inline]
    pub fn hash(&self) -> &HashOutput {
        &self.accumulated_data.hash
    }

    /// Returns a reference to the inner block
    #[inline]
    pub fn block(&self) -> &Block {
        &self.block
    }

    /// Returns a reference to the inner block's header
    #[inline]
    pub fn header(&self) -> &BlockHeader {
        &self.block.header
    }

    /// Returns the inner block wrapped in an atomically reference counted (ARC) pointer. This call is cheap and does
    /// not copy the block in memory.
    #[inline]
    pub fn to_arc_block(&self) -> Arc<Block> {
        self.block.clone()
    }

    #[inline]
    pub fn accumulated_data(&self) -> &BlockHeaderAccumulatedData {
        &self.accumulated_data
    }

    pub fn to_chain_header(&self) -> ChainHeader {
        // NOTE: Panic is impossible, a ChainBlock cannot be constructed if inconsistencies between the header and
        // accum data exist
        ChainHeader::try_construct(self.block.header.clone(), self.accumulated_data.clone()).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::blocks::genesis_block::get_ridcully_genesis_block;

    mod chain_block {
        use super::*;

        #[test]
        fn it_converts_to_a_chain_header() {
            let genesis = get_ridcully_genesis_block();
            let header = genesis.to_chain_header();
            assert_eq!(header.header(), genesis.header());
            assert_eq!(header.accumulated_data(), genesis.accumulated_data());
        }

        #[test]
        fn it_provides_guarantees_about_data_integrity() {
            let mut genesis = get_ridcully_genesis_block();
            // Mess with the header, only possible using the non-public fields
            genesis.block = Arc::new({
                let mut b = (*genesis.block).clone();
                b.header.height = 1;
                b
            });
            assert!(ChainBlock::try_construct(genesis.to_arc_block(), genesis.accumulated_data().clone()).is_none());
            assert!(ChainHeader::try_construct(genesis.header().clone(), genesis.accumulated_data().clone()).is_none());

            genesis.block = Arc::new({
                let mut b = (*genesis.block).clone();
                b.header.height = 0;
                b
            });
            ChainBlock::try_construct(genesis.to_arc_block(), genesis.accumulated_data().clone()).unwrap();
            ChainHeader::try_construct(genesis.header().clone(), genesis.accumulated_data().clone()).unwrap();
        }
    }
}
