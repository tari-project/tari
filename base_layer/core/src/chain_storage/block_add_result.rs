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

use std::{fmt, sync::Arc};

use tari_utilities::hex::Hex;

use crate::blocks::ChainBlock;

#[derive(Clone, Debug, PartialEq)]
pub enum BlockAddResult {
    Ok(Arc<ChainBlock>),
    BlockExists,
    OrphanBlock,
    /// Indicates the new block caused a chain reorg.
    /// This contains added blocks ordered from lowest to highest block height, and
    /// the removed blocks ordered from highest to lowest block height.
    ChainReorg {
        added: Vec<Arc<ChainBlock>>,
        removed: Vec<Arc<ChainBlock>>,
    },
}

impl BlockAddResult {
    /// Returns true if the chain was changed (i.e block added or reorged), otherwise false
    pub fn was_chain_modified(&self) -> bool {
        matches!(self, BlockAddResult::Ok(_) | BlockAddResult::ChainReorg { .. })
    }

    pub fn is_added(&self) -> bool {
        matches!(self, BlockAddResult::Ok(_))
    }

    pub fn is_chain_reorg(&self) -> bool {
        matches!(self, BlockAddResult::ChainReorg { .. })
    }

    pub fn is_orphaned(&self) -> bool {
        matches!(self, BlockAddResult::OrphanBlock)
    }

    pub fn added_blocks(&self) -> Vec<Arc<ChainBlock>> {
        match self {
            Self::ChainReorg { added, removed: _ } => added.clone(),
            Self::Ok(added) => vec![added.clone()],
            _ => vec![],
        }
    }

    pub fn removed_blocks(&self) -> Vec<Arc<ChainBlock>> {
        match self {
            Self::ChainReorg { added: _, removed } => removed.clone(),
            _ => vec![],
        }
    }

    #[cfg(test)]
    pub fn assert_added(&self) -> ChainBlock {
        match self {
            BlockAddResult::ChainReorg { added, removed } => panic!(
                "Expected added result, but was reorg ({} added, {} removed)",
                added.len(),
                removed.len()
            ),
            BlockAddResult::Ok(b) => b.as_ref().clone(),
            BlockAddResult::BlockExists => panic!("Expected added result, but was BlockExists"),
            BlockAddResult::OrphanBlock => panic!("Expected added result, but was OrphanBlock"),
        }
    }

    #[cfg(test)]
    pub fn assert_orphaned(&self) {
        assert!(self.is_orphaned(), "Result was not orphaned");
    }

    #[cfg(test)]
    pub fn assert_reorg(&self, num_added: usize, num_removed: usize) {
        match self {
            BlockAddResult::ChainReorg { added, removed } => {
                assert_eq!(num_added, added.len(), "Number of added reorged blocks was different");
                assert_eq!(
                    num_removed,
                    removed.len(),
                    "Number of removed reorged blocks was different"
                );
            },
            BlockAddResult::Ok(_) => panic!("Expected reorg result, but was Ok()"),
            BlockAddResult::BlockExists => panic!("Expected reorg result, but was BlockExists"),
            BlockAddResult::OrphanBlock => panic!("Expected reorg result, but was OrphanBlock"),
        }
    }
}

impl fmt::Display for BlockAddResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockAddResult::Ok(block) => {
                write!(f, "Block {} at height {} added", block.hash().to_hex(), block.height())
            },
            BlockAddResult::BlockExists => write!(f, "Block already exists"),
            BlockAddResult::OrphanBlock => write!(f, "Block added as orphan"),
            BlockAddResult::ChainReorg { added, removed } => write!(
                f,
                "Reorg from {} ({}) to {}, and {} blocks added  ending with {} ({})",
                removed.first().map(|r| r.height()).unwrap_or(0),
                removed
                    .first()
                    .map(|r| r.hash().to_hex())
                    .unwrap_or_else(|| "None".to_string()),
                removed.last().map(|r| r.height()).unwrap_or(0),
                added.len(),
                added.last().map(|a| a.height()).unwrap_or(0),
                added
                    .last()
                    .map(|a| a.hash().to_hex())
                    .unwrap_or_else(|| "None".to_string())
            ),
        }
    }
}
