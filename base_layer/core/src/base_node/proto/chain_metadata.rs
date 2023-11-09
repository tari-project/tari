// Copyright 2019, The Tari Project
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

use std::convert::{TryFrom, TryInto};

use primitive_types::U512;
use tari_common_types::{chain_metadata::ChainMetadata, types::FixedHash};

use crate::proto::base_node as proto;

impl TryFrom<proto::ChainMetadata> for ChainMetadata {
    type Error = String;

    fn try_from(metadata: proto::ChainMetadata) -> Result<Self, Self::Error> {
        if metadata.accumulated_difficulty.len() != 64 {
            return Err(format!(
                "Invalid accumulated difficulty byte length. {} was expected but the actual length was {}",
                64,
                metadata.accumulated_difficulty.len()
            ));
        }

        let accumulated_difficulty = U512::from_big_endian(&metadata.accumulated_difficulty);
        let height_of_longest_chain = metadata.height_of_longest_chain;

        let pruning_horizon = if metadata.pruned_height == 0 {
            metadata.pruned_height
        } else {
            height_of_longest_chain.saturating_sub(metadata.pruned_height)
        };

        if metadata.best_block.is_empty() {
            return Err("Best block is missing".to_string());
        }
        let hash: FixedHash = metadata
            .best_block
            .try_into()
            .map_err(|e| format!("Malformed best block: {}", e))?;
        Ok(ChainMetadata::new(
            height_of_longest_chain,
            hash,
            pruning_horizon,
            metadata.pruned_height,
            accumulated_difficulty,
            metadata.timestamp,
        ))
    }
}

impl From<ChainMetadata> for proto::ChainMetadata {
    fn from(metadata: ChainMetadata) -> Self {
        let mut accumulated_difficulty = [0u8; 64];
        metadata
            .accumulated_difficulty()
            .to_big_endian(&mut accumulated_difficulty);
        Self {
            height_of_longest_chain: metadata.height_of_longest_chain(),
            best_block: metadata.best_block().to_vec(),
            pruned_height: metadata.pruned_height(),
            accumulated_difficulty: accumulated_difficulty.to_vec(),
            timestamp: metadata.timestamp(),
        }
    }
}

impl proto::ChainMetadata {
    pub fn height_of_longest_chain(&self) -> u64 {
        self.height_of_longest_chain
    }
}
