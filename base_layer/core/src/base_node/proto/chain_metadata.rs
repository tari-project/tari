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

use super::base_node as proto;
use crate::chain_storage::ChainMetadata;

impl From<proto::ChainMetadata> for ChainMetadata {
    fn from(metadata: proto::ChainMetadata) -> Self {
        let accumulated_difficulty = if metadata.accumulated_difficulty.len() == 16 {
            let mut accumulated_difficulty_array = [0; 16];
            accumulated_difficulty_array.copy_from_slice(&metadata.accumulated_difficulty[0..16]);
            Some(u128::from_be_bytes(accumulated_difficulty_array))
        } else {
            None
        };

        Self {
            height_of_longest_chain: metadata.height_of_longest_chain,
            best_block: metadata.best_block,
            pruning_horizon: metadata.pruning_horizon,
            effective_pruned_height: metadata.effective_pruned_height,
            accumulated_difficulty,
        }
    }
}

impl From<ChainMetadata> for proto::ChainMetadata {
    fn from(metadata: ChainMetadata) -> Self {
        let accumulated_difficulty = match metadata.accumulated_difficulty {
            None => Vec::new(),
            Some(v) => v.to_be_bytes().to_vec(),
        };
        Self {
            height_of_longest_chain: metadata.height_of_longest_chain,
            best_block: metadata.best_block,
            pruning_horizon: metadata.pruning_horizon,
            effective_pruned_height: metadata.effective_pruned_height,
            accumulated_difficulty,
        }
    }
}
