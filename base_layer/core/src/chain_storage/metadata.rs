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

use crate::{blocks::blockheader::BlockHash, proof_of_work::Difficulty};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};
use tari_utilities::hex::Hex;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainMetadata {
    /// The current chain height, or the block number of the longest valid chain, or `None` if there is no chain
    pub height_of_longest_chain: Option<u64>,
    /// The block hash of the current tip of the longest valid chain, or `None` for an empty chain
    pub best_block: Option<BlockHash>,
    /// The total accumulated difficulty, or work, on the longest valid chain since the genesis block.
    pub total_accumulated_difficulty: Difficulty,
    /// The number of blocks back from the tip that this database tracks. A value of 0 indicates that all blocks are
    /// tracked (i.e. the database is in full archival mode).
    pub pruning_horizon: u64,
}

impl ChainMetadata {
    pub fn new(height: u64, hash: BlockHash, work: Difficulty, horizon: u64) -> ChainMetadata {
        ChainMetadata {
            height_of_longest_chain: Some(height),
            best_block: Some(hash),
            total_accumulated_difficulty: work,
            pruning_horizon: horizon,
        }
    }

    /// The block height at the pruning horizon, given the chain height of the network. Typically database backends
    /// cannot provide any block data earlier than this point.
    /// Zero is returned if the blockchain still hasn't reached the pruning horizon.
    #[inline(always)]
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
}

impl Default for ChainMetadata {
    fn default() -> Self {
        ChainMetadata {
            height_of_longest_chain: None,
            best_block: None,
            total_accumulated_difficulty: Difficulty::default(),
            pruning_horizon: 2880,
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
            .unwrap_or("Empty Database".into());
        fmt.write_str(&format!("Height of longest chain : {}\n", height))?;
        fmt.write_str(&format!("Best_block : {}\n", best_block))?;
        fmt.write_str(&format!(
            "Total accumulated difficulty : {}\n",
            self.total_accumulated_difficulty
        ))?;
        fmt.write_str(&format!("Pruning horizon : {}\n", self.pruning_horizon))
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
    fn horizon_block() {
        let metadata = ChainMetadata::default();
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
