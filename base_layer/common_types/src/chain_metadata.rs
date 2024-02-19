// Copyright 2019. The Tari Project
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

use std::fmt::{Display, Error, Formatter};

use primitive_types::U256;
use serde::{Deserialize, Serialize};

use crate::types::BlockHash;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct ChainMetadata {
    /// The current chain height, or the block number of the longest valid chain
    best_block_height: u64,
    /// The block hash of the current tip of the longest valid chain
    best_block_hash: BlockHash,
    /// The configured number of blocks back from the tip that this database tracks. A value of 0 indicates that
    /// pruning mode is disabled and the node will keep full blocks from the time it was set. If pruning horizon
    /// was previously enabled, previously pruned blocks will remain pruned. If set from initial sync, full blocks
    /// are preserved from genesis (i.e. the database is in full archival mode).
    pruning_horizon: u64,
    /// The height of the pruning horizon. This indicates from what height a full block can be provided
    /// (exclusive). If `pruned_height` is equal to the `best_block_height` no blocks can be
    /// provided. Archival nodes wil always have a `pruned_height` of zero.
    pruned_height: u64,
    /// The total accumulated proof of work of the longest chain
    accumulated_target_difficulty: U256,
    /// Timestamp of the tip block in the longest valid chain
    timestamp: u64,
}
#[derive(Debug, thiserror::Error)]
pub enum ChainMetaDataError {
    #[error("Pruning Height is higher than the Best Block height")]
    PruningHeightAboveBestBlock,
    #[error("The total accumulated difficulty is zero")]
    AccumulatedDifficultyZero,
}

impl ChainMetadata {
    pub fn new(
        best_block_height: u64,
        best_block_hash: BlockHash,
        pruning_horizon: u64,
        pruned_height: u64,
        accumulated_target_difficulty: U256,
        timestamp: u64,
    ) -> Result<ChainMetadata, ChainMetaDataError> {
        let chain_meta_data = ChainMetadata {
            best_block_height,
            best_block_hash,
            pruning_horizon,
            pruned_height,
            accumulated_target_difficulty,
            timestamp,
        };
        if chain_meta_data.accumulated_target_difficulty == 0.into() {
            return Err(ChainMetaDataError::AccumulatedDifficultyZero);
        };
        if chain_meta_data.pruned_height > chain_meta_data.best_block_height {
            return Err(ChainMetaDataError::PruningHeightAboveBestBlock);
        };
        Ok(chain_meta_data)
    }

    /// The block height at the pruning horizon, given the chain height of the network. Typically database backends
    /// cannot provide any block data earlier than this point.
    /// Zero is returned if the blockchain still hasn't reached the pruning horizon.
    pub fn pruned_height_at_given_chain_tip(&self, chain_tip: u64) -> u64 {
        match self.pruning_horizon {
            0 => 0,
            pruning_horizon => chain_tip.saturating_sub(pruning_horizon),
        }
    }

    /// The configured number of blocks back from the tip that this database tracks. A value of 0 indicates that
    /// pruning mode is disabled and the node will keep full blocks from the time it was set. If pruning horizon
    /// was previously enabled, previously pruned blocks will remain pruned. If set from initial sync, full blocks
    /// are preserved from genesis (i.e. the database is in full archival mode).
    pub fn pruning_horizon(&self) -> u64 {
        self.pruning_horizon
    }

    /// Check if the node is an archival node based on its pruning horizon.
    pub fn is_archival_node(&self) -> bool {
        self.pruning_horizon == 0
    }

    /// Check if the node is a pruned node based on its pruning horizon.
    pub fn is_pruned_node(&self) -> bool {
        self.pruning_horizon != 0
    }

    /// Returns the height of longest chain.
    pub fn best_block_height(&self) -> u64 {
        self.best_block_height
    }

    /// The height of the pruning horizon. This indicates from what height a full block can be provided
    /// (exclusive). If `pruned_height` is equal to the `best_block_height` no blocks can be
    /// provided. Archival nodes wil always have a `pruned_height` of zero.
    pub fn pruned_height(&self) -> u64 {
        self.pruned_height
    }

    pub fn accumulated_target_difficulty(&self) -> U256 {
        self.accumulated_target_difficulty
    }

    pub fn best_block_hash(&self) -> &BlockHash {
        &self.best_block_hash
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

impl Display for ChainMetadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        writeln!(f, "Best block height: {}", self.best_block_height)?;
        writeln!(f, "Total accumulated difficulty: {}", self.accumulated_target_difficulty)?;
        writeln!(f, "Best block hash: {}", self.best_block_hash)?;
        writeln!(f, "Pruning horizon: {}", self.pruning_horizon)?;
        writeln!(f, "Pruned height: {}", self.pruned_height)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::ChainMetadata;

    #[test]
    fn horizon_block_on_default() {
        let metadata = ChainMetadata {
            best_block_height: 0,
            best_block_hash: Default::default(),
            pruning_horizon: 0,
            pruned_height: 0,
            accumulated_target_difficulty: Default::default(),
            timestamp: 0,
        };
        assert_eq!(metadata.pruned_height_at_given_chain_tip(0), 0);
    }

    #[test]
    fn pruned_mode() {
        let mut metadata = ChainMetadata {
            best_block_height: 0,
            best_block_hash: Default::default(),
            pruning_horizon: 0,
            pruned_height: 0,
            accumulated_target_difficulty: Default::default(),
            timestamp: 0,
        };
        assert!(!metadata.is_pruned_node());
        assert!(metadata.is_archival_node());
        metadata.pruning_horizon = 2880;
        assert!(metadata.is_pruned_node());
        assert!(!metadata.is_archival_node());
        assert_eq!(metadata.pruned_height_at_given_chain_tip(0), 0);
        assert_eq!(metadata.pruned_height_at_given_chain_tip(100), 0);
        assert_eq!(metadata.pruned_height_at_given_chain_tip(2880), 0);
        assert_eq!(metadata.pruned_height_at_given_chain_tip(2881), 1);
    }

    #[test]
    fn archival_node() {
        let metadata = ChainMetadata {
            best_block_height: 0,
            best_block_hash: Default::default(),
            pruning_horizon: 0,
            pruned_height: 0,
            accumulated_target_difficulty: Default::default(),
            timestamp: 0,
        };
        // Chain is still empty
        assert_eq!(metadata.pruned_height_at_given_chain_tip(0), 0);
        // When pruning horizon is zero, the horizon block is always 0, the genesis block
        assert_eq!(metadata.pruned_height_at_given_chain_tip(0), 0);
        assert_eq!(metadata.pruned_height_at_given_chain_tip(100), 0);
        assert_eq!(metadata.pruned_height_at_given_chain_tip(2881), 0);
    }
}
