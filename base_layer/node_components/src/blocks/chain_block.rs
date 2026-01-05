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

use std::{fmt, fmt::Display, sync::Arc};

use tari_common_types::types::HashOutput;
use tari_transaction_components::aggregated_body::AggregateBody;

use crate::blocks::{Block, BlockHeader, BlockHeaderAccumulatedData};

/// A block linked to a chain.
/// A ChainBlock MUST have the same or stronger guarantees than `ChainHeader`
#[derive(Debug, Clone, PartialEq)]
pub struct ChainBlock {
    accumulated_data: BlockHeaderAccumulatedData,
    block: Arc<Block>,
}

impl Display for ChainBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.accumulated_data)?;
        writeln!(f, "{}", self.block)?;
        Ok(())
    }
}

impl ChainBlock {
    /// Attempts to construct a `ChainBlock` from a `Block` and associate `BlockHeaderAccumulatedData`. Returns None if
    /// the Block and the BlockHeaderAccumulatedData do not correspond (i.e have different hashes)
    pub fn try_construct(block: Arc<Block>, accumulated_data: BlockHeaderAccumulatedData) -> Option<Self> {
        if accumulated_data.hash != block.hash() {
            return None;
        }

        Some(Self {
            accumulated_data,
            block,
        })
    }

    pub fn height(&self) -> u64 {
        self.block.header.height
    }

    pub fn hash(&self) -> &HashOutput {
        &self.accumulated_data.hash
    }

    /// Returns a reference to the inner block
    pub fn block(&self) -> &Block {
        &self.block
    }

    /// Returns a reference to the inner block's header
    pub fn header(&self) -> &BlockHeader {
        &self.block.header
    }

    /// Returns the inner block wrapped in an atomically reference counted (ARC) pointer. This call is cheap and does
    /// not copy the block in memory.
    pub fn to_arc_block(&self) -> Arc<Block> {
        self.block.clone()
    }

    pub fn accumulated_data(&self) -> &BlockHeaderAccumulatedData {
        &self.accumulated_data
    }

    pub fn to_chain_header(&self) -> ChainHeader {
        // NOTE: Panic is impossible, a ChainBlock cannot be constructed if inconsistencies between the header and
        // accum data exist
        ChainHeader::try_construct(self.block.header.clone(), self.accumulated_data.clone()).unwrap()
    }
}

/// A block linked to a chain.
/// A ChainHeader guarantees (i.e cannot be constructed) that the block and accumulated data correspond by hash.
#[derive(Debug, Clone, PartialEq)]
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

    pub fn height(&self) -> u64 {
        self.header.height
    }

    pub fn timestamp(&self) -> u64 {
        self.header.timestamp.as_u64()
    }

    pub fn hash(&self) -> &HashOutput {
        &self.accumulated_data.hash
    }

    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    pub fn accumulated_data(&self) -> &BlockHeaderAccumulatedData {
        &self.accumulated_data
    }

    pub fn into_parts(self) -> (BlockHeader, BlockHeaderAccumulatedData) {
        (self.header, self.accumulated_data)
    }

    pub fn into_header(self) -> BlockHeader {
        self.header
    }

    pub fn upgrade_to_chain_block(self, body: AggregateBody) -> ChainBlock {
        // NOTE: Panic cannot occur because a ChainBlock has the same guarantees as ChainHeader
        ChainBlock::try_construct(Arc::new(Block::new(self.header, body)), self.accumulated_data).unwrap()
    }
}
