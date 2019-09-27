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

#[derive(Debug, Clone)]
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

    /// The block height at the pruning horizon. Typically database backends cannot provide any block data earlier
    /// than this point.
    ///
    /// #Returns
    ///
    /// * `None`, if the chain is still empty
    /// * `h`, the block number of the first block stored in the chain
    #[inline(always)]
    pub fn horizon_block(&self) -> Option<u64> {
        if self.height_of_longest_chain.is_none() {
            return None;
        }
        match self.pruning_horizon {
            0 => Some(0u64),
            horizon => match self.height_of_longest_chain.unwrap().checked_sub(horizon) {
                None => Some(0u64),
                Some(v) => Some(v as u64),
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

#[cfg(test)]
mod test {
    use super::ChainMetadata;

    #[test]
    fn horizon_block_on_default() {
        let metadata = ChainMetadata::default();
        assert_eq!(metadata.horizon_block(), None);
    }

    #[test]
    fn horizon_block() {
        let mut metadata = ChainMetadata::default();
        metadata.height_of_longest_chain = Some(0);
        assert_eq!(metadata.horizon_block(), Some(0));
        metadata.height_of_longest_chain = Some(100);
        assert_eq!(metadata.horizon_block(), Some(0));
        metadata.height_of_longest_chain = Some(2880);
        assert_eq!(metadata.horizon_block(), Some(0));
        metadata.height_of_longest_chain = Some(2881);
        assert_eq!(metadata.horizon_block(), Some(1));
    }

    #[test]
    fn archival_node() {
        let mut metadata = ChainMetadata::default();
        metadata.archival_mode();
        // Chain is still empty
        assert_eq!(metadata.horizon_block(), None);
        // When pruning horizon is zero, the horizon block is always 0, the genesis block
        metadata.height_of_longest_chain = Some(0);
        assert_eq!(metadata.horizon_block(), Some(0));
        metadata.height_of_longest_chain = Some(100);
        assert_eq!(metadata.horizon_block(), Some(0));
        metadata.height_of_longest_chain = Some(2881);
        assert_eq!(metadata.horizon_block(), Some(0));
    }
}
