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

use tari_common_types::types::FixedHash;
use tari_node_components::blocks::ChainBlock;
use tari_utilities::hex::Hex;

use crate::{chain_storage::ChainStorageError, validation::ValidationError};

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

/// What `add_block` did: the net change to the main chain, and every held block that failed body validation on the way.
///
/// A reorg can fail on a block other than the one being added, a held orphan that the new block linked to the main
/// chain. The node then keeps the strongest valid chain it holds and tries the next strongest orphan tip, so a single
/// call can both change the chain and reject blocks.
#[derive(Debug)]
pub struct AddBlockOutcome {
    /// The net change from the tip the call started at, over every reorg it attempted
    pub result: BlockAddResult,
    /// The blocks that failed body validation, in the order they failed
    pub rejected: Vec<RejectedBlock>,
}

/// A block that failed body validation during a reorg. It, and every orphan built on it, has been dropped.
#[derive(Debug)]
pub struct RejectedBlock {
    pub hash: FixedHash,
    pub height: u64,
    pub error: ValidationError,
    /// Whether the peer that sent the block being added is at fault. It is if the block that failed is the block being
    /// added, or an ancestor of it whose body is the one its header commits to: an honest peer never holds a chain
    /// built on a block that is invalid as mined. It is not for a descendant of the block being added (which a peer
    /// can hold back and send us first), nor for any block whose body was never checked against its header.
    pub blame_sender: bool,
}

impl RejectedBlock {
    /// The error to report this rejection with, when `candidate_hash` is the block being added. The block being added
    /// is reported with its own validation error, as it always was; an ancestor is reported by its own hash.
    pub fn into_error(self, candidate_hash: FixedHash) -> ChainStorageError {
        if self.hash == candidate_hash {
            ChainStorageError::ValidationError { source: self.error }
        } else {
            ChainStorageError::AncestorBlockInvalid {
                candidate: candidate_hash,
                hash: self.hash,
                height: self.height,
                source: self.error,
            }
        }
    }
}

impl fmt::Display for BlockAddResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockAddResult::Ok(block) => {
                write!(f, "Block {} at height {} added", block.hash(), block.height())
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
