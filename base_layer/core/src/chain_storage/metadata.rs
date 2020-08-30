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

use crate::{proof_of_work::Difficulty, tari_utilities::hex::Hex, transactions::types::BlockHash};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainMetadata {
    /// The current chain height, or the block number of the longest valid chain, or `None` if there is no chain
    pub height_of_longest_chain: Option<u64>,
    /// The block hash of the current tip of the longest valid chain, or `None` for an empty chain
    pub best_block: Option<BlockHash>,
    /// The number of blocks back from the tip that this database tracks. A value of 0 indicates that all blocks are
    /// tracked (i.e. the database is in full archival mode).
    pub pruning_horizon: u64,
    /// The effective height of the pruning horizon. This indicates from what height a full block can be provided
    /// (exclusive). If `effective_pruned_height` is equal to the `height_of_longest_chain` no blocks can be
    /// provided. Archival nodes wil always have an `effective_pruned_height` of zero.
    pub effective_pruned_height: u64,
    /// The geometric mean of the proof of work of the longest chain, none if the chain is empty
    pub accumulated_difficulty: Option<Difficulty>,
}

impl ChainMetadata {
    pub fn new(
        height: u64,
        hash: BlockHash,
        pruning_horizon: u64,
        effective_pruned_height: u64,
        accumulated_difficulty: Difficulty,
    ) -> ChainMetadata
    {
        ChainMetadata {
            height_of_longest_chain: Some(height),
            best_block: Some(hash),
            pruning_horizon,
            effective_pruned_height,
            accumulated_difficulty: Some(accumulated_difficulty),
        }
    }

    /// The block height at the pruning horizon, given the chain height of the network. Typically database backends
    /// cannot provide any block data earlier than this point.
    /// Zero is returned if the blockchain still hasn't reached the pruning horizon.
    pub fn horizon_block(&self, chain_tip: u64) -> u64 {
        match self.pruning_horizon {
            0 => 0,
            horizon => match chain_tip.checked_sub(horizon) {
                None => 0,
                Some(h) => h,
            },
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

    /// Check if the node is an archival node based on its pruning horizon.
    pub fn is_archival_node(&self) -> bool {
        self.pruning_horizon == 0
    }

    /// Check if the node is a pruned node based on its pruning horizon.
    pub fn is_pruned_node(&self) -> bool {
        self.pruning_horizon != 0
    }

    /// Returns the height of longest chain.
    #[inline]
    pub fn height_of_longest_chain(&self) -> u64 {
        self.height_of_longest_chain.unwrap_or_default()
    }
}

impl Default for ChainMetadata {
    fn default() -> Self {
        ChainMetadata {
            height_of_longest_chain: None,
            best_block: None,
            pruning_horizon: 0,
            effective_pruned_height: 0,
            accumulated_difficulty: None,
        }
    }
}

impl Display for ChainMetadata {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        let height = self.height_of_longest_chain.unwrap_or(0);
        let best_block = self
            .best_block
            .clone()
            .map(|b| b.to_hex())
            .unwrap_or_else(|| "None".into());
        let accumulated_difficulty = self.accumulated_difficulty.unwrap_or_else(|| 0.into());
        fmt.write_str(&format!("Height of longest chain : {}\n", height))?;
        fmt.write_str(&format!(
            "Geometric mean of longest chain : {}\n",
            accumulated_difficulty
        ))?;
        fmt.write_str(&format!("Best block : {}\n", best_block))?;
        fmt.write_str(&format!("Pruning horizon : {}\n", self.pruning_horizon))?;
        fmt.write_str(&format!("Effective pruned height : {}\n", self.effective_pruned_height))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InProgressHorizonSyncState {
    pub metadata: ChainMetadata,
    pub initial_kernel_checkpoint_count: u64,
    pub initial_utxo_checkpoint_count: u64,
    pub initial_rangeproof_checkpoint_count: u64,
}

impl InProgressHorizonSyncState {
    pub fn new_with_metadata(metadata: ChainMetadata) -> Self {
        Self {
            metadata,
            initial_kernel_checkpoint_count: 0,
            initial_utxo_checkpoint_count: 0,
            initial_rangeproof_checkpoint_count: 0,
        }
    }
}

impl Display for InProgressHorizonSyncState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "metadata = {}, #kernel checkpoints = ({}), #UTXO checkpoints = ({}), #range proof checkpoints = ({})",
            self.metadata,
            self.initial_kernel_checkpoint_count,
            self.initial_utxo_checkpoint_count,
            self.initial_rangeproof_checkpoint_count,
        )
    }
}

#[cfg(test)]
mod test {
    use super::ChainMetadata;

    #[test]
    fn horizon_block_on_default() {
        let metadata = ChainMetadata::default();
        assert_eq!(metadata.horizon_block(0), 0);
    }

    #[test]
    fn pruned_mode() {
        let mut metadata = ChainMetadata::default();
        assert_eq!(metadata.is_pruned_node(), false);
        assert_eq!(metadata.is_archival_node(), true);
        metadata.set_pruning_horizon(2880);
        assert_eq!(metadata.is_pruned_node(), true);
        assert_eq!(metadata.is_archival_node(), false);
        assert_eq!(metadata.horizon_block(0), 0);
        assert_eq!(metadata.horizon_block(100), 0);
        assert_eq!(metadata.horizon_block(2880), 0);
        assert_eq!(metadata.horizon_block(2881), 1);
    }

    #[test]
    fn archival_node() {
        let mut metadata = ChainMetadata::default();
        metadata.archival_mode();
        // Chain is still empty
        assert_eq!(metadata.horizon_block(0), 0);
        // When pruning horizon is zero, the horizon block is always 0, the genesis block
        assert_eq!(metadata.horizon_block(0), 0);
        assert_eq!(metadata.horizon_block(100), 0);
        assert_eq!(metadata.horizon_block(2881), 0);
    }
}
