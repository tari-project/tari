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
use std::convert::TryFrom;
use tari_common_types::chain_metadata::ChainMetadata;

impl TryFrom<proto::ChainMetadata> for ChainMetadata {
    type Error = String;

    fn try_from(metadata: proto::ChainMetadata) -> Result<Self, Self::Error> {
        let accumulated_difficulty = {
            let mut accumulated_difficulty_array = [0; 16];
            accumulated_difficulty_array.copy_from_slice(&metadata.accumulated_difficulty[0..16]);
            u128::from_be_bytes(accumulated_difficulty_array)
        };

        Ok(ChainMetadata::new(
            metadata
                .height_of_longest_chain
                .ok_or_else(|| "Height of longest chain is missing".to_string())?,
            metadata.best_block.ok_or_else(|| "Best block is missing".to_string())?,
            metadata.pruning_horizon,
            metadata.effective_pruned_height,
            accumulated_difficulty,
        ))
    }
}

impl From<ChainMetadata> for proto::ChainMetadata {
    fn from(metadata: ChainMetadata) -> Self {
        let accumulated_difficulty = metadata.accumulated_difficulty().to_be_bytes().to_vec();
        Self {
            height_of_longest_chain: Some(metadata.height_of_longest_chain()),
            best_block: Some(metadata.best_block().clone()),
            pruning_horizon: metadata.pruning_horizon(),
            effective_pruned_height: metadata.effective_pruned_height(),
            accumulated_difficulty,
        }
    }
}
