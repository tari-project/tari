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

use primitive_types::U512;
use serde::{Deserialize, Serialize};
use tari_utilities::hex::Hex;

use crate::types::{BlockHash, FixedHash};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct ChainMetadata {
    /// The current chain height, or the block number of the longest valid chain, or `None` if there is no chain
    height_of_longest_chain: u64,
    /// The block hash of the current tip of the longest valid chain
    best_block: BlockHash,
    /// The configured number of blocks back from the tip that this database tracks. A value of 0 indicates that
    /// pruning mode is disabled and the node will keep full blocks from the time it was set. If pruning horizon
    /// was previously enabled, previously pruned blocks will remain pruned. If set from initial sync, full blocks
    /// are preserved from genesis (i.e. the database is in full archival mode).
    pruning_horizon: u64,
    /// The height of the pruning horizon. This indicates from what height a full block can be provided
    /// (exclusive). If `pruned_height` is equal to the `height_of_longest_chain` no blocks can be
    /// provided. Archival nodes wil always have an `pruned_height` of zero.
    pruned_height: u64,
    /// The total accumulated proof of work of the longest chain
    accumulated_difficulty: U512,
    /// Timestamp of the tip block in the longest valid chain
    timestamp: u64,
}

impl ChainMetadata {
    pub fn new(
        height: u64,
        hash: BlockHash,
        pruning_horizon: u64,
        pruned_height: u64,
        accumulated_difficulty: U512,
        timestamp: u64,
    ) -> ChainMetadata {
        ChainMetadata {
            height_of_longest_chain: height,
            best_block: hash,
            pruning_horizon,
            pruned_height,
            accumulated_difficulty,
            timestamp,
        }
    }

    pub fn empty() -> ChainMetadata {
        ChainMetadata {
            height_of_longest_chain: 0,
            best_block: FixedHash::zero(),
            pruning_horizon: 0,
            pruned_height: 0,
            accumulated_difficulty: 0.into(),
            timestamp: 0,
        }
    }

    /// The block height at the pruning horizon, given the chain height of the network. Typically database backends
    /// cannot provide any block data earlier than this point.
    /// Zero is returned if the blockchain still hasn't reached the pruning horizon.
    pub fn horizon_block_height(&self, chain_tip: u64) -> u64 {
        match self.pruning_horizon {
            0 => 0,
            horizon => chain_tip.saturating_sub(horizon),
        }
    }

    /// Set the pruning horizon to indicate that the chain is in archival mode (i.e. a pruning horizon of zero)
    pub fn archival_mode(&mut self) {
        self.pruning_horizon = 0;
    }

    /// Set the pruning horizon
    pub fn set_pruning_horizon(&mut self, pruning_horizon: u64) {
        self.pruning_horizon = pruning_horizon;
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
    pub fn height_of_longest_chain(&self) -> u64 {
        self.height_of_longest_chain
    }

    /// The height of the pruning horizon. This indicates from what height a full block can be provided
    /// (exclusive). If `pruned_height` is equal to the `height_of_longest_chain` no blocks can be
    /// provided. Archival nodes wil always have an `pruned_height` of zero.
    pub fn pruned_height(&self) -> u64 {
        self.pruned_height
    }

    pub fn accumulated_difficulty(&self) -> U512 {
        self.accumulated_difficulty
    }

    pub fn best_block(&self) -> &BlockHash {
        &self.best_block
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

impl Display for ChainMetadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let height = self.height_of_longest_chain;
        let best_block = self.best_block.to_hex();
        let accumulated_difficulty = self.accumulated_difficulty;
        writeln!(f, "Height of longest chain: {}", height)?;
        writeln!(f, "Total accumulated difficulty: {}", accumulated_difficulty)?;
        writeln!(f, "Best block: {}", best_block)?;
        writeln!(f, "Pruning horizon: {}", self.pruning_horizon)?;
        writeln!(f, "Effective pruned height: {}", self.pruned_height)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::ChainMetadata;

    #[test]
    fn horizon_block_on_default() {
        let metadata = ChainMetadata::empty();
        assert_eq!(metadata.horizon_block_height(0), 0);
    }

    #[test]
    fn pruned_mode() {
        let mut metadata = ChainMetadata::empty();
        assert!(!metadata.is_pruned_node());
        assert!(metadata.is_archival_node());
        metadata.set_pruning_horizon(2880);
        assert!(metadata.is_pruned_node());
        assert!(!metadata.is_archival_node());
        assert_eq!(metadata.horizon_block_height(0), 0);
        assert_eq!(metadata.horizon_block_height(100), 0);
        assert_eq!(metadata.horizon_block_height(2880), 0);
        assert_eq!(metadata.horizon_block_height(2881), 1);
    }

    #[test]
    fn archival_node() {
        let mut metadata = ChainMetadata::empty();
        metadata.archival_mode();
        // Chain is still empty
        assert_eq!(metadata.horizon_block_height(0), 0);
        // When pruning horizon is zero, the horizon block is always 0, the genesis block
        assert_eq!(metadata.horizon_block_height(0), 0);
        assert_eq!(metadata.horizon_block_height(100), 0);
        assert_eq!(metadata.horizon_block_height(2881), 0);
    }
}
