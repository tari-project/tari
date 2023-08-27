// Copyright 2019, The Taiji Project
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

use std::{
    convert::{TryFrom, TryInto},
    mem,
};

use taiji_common_types::{chain_metadata::ChainMetadata, types::FixedHash};

use crate::proto::base_node as proto;

impl TryFrom<proto::ChainMetadata> for ChainMetadata {
    type Error = String;

    fn try_from(metadata: proto::ChainMetadata) -> Result<Self, Self::Error> {
        const ACC_DIFFICULTY_ARRAY_LEN: usize = mem::size_of::<u128>();
        if metadata.accumulated_difficulty.len() != ACC_DIFFICULTY_ARRAY_LEN {
            return Err(format!(
                "Invalid accumulated difficulty byte length. {} was expected but the actual length was {}",
                ACC_DIFFICULTY_ARRAY_LEN,
                metadata.accumulated_difficulty.len()
            ));
        }

        let mut acc_diff = [0; ACC_DIFFICULTY_ARRAY_LEN];
        acc_diff.copy_from_slice(&metadata.accumulated_difficulty[0..ACC_DIFFICULTY_ARRAY_LEN]);
        let accumulated_difficulty = u128::from_be_bytes(acc_diff);
        let height_of_longest_chain = metadata
            .height_of_longest_chain
            .ok_or_else(|| "Height of longest chain is missing".to_string())?;
        let pruning_horizon = if metadata.pruned_height == 0 {
            metadata.pruned_height
        } else {
            height_of_longest_chain.saturating_sub(metadata.pruned_height)
        };
        let hash: FixedHash = metadata
            .best_block
            .ok_or_else(|| "Best block is missing".to_string())?
            .try_into()
            .map_err(|e| format!("Malformed best block: {}", e))?;
        Ok(ChainMetadata::new(
            height_of_longest_chain,
            hash,
            pruning_horizon,
            metadata.pruned_height,
            accumulated_difficulty,
            metadata.timestamp.unwrap_or_default(),
        ))
    }
}

impl From<ChainMetadata> for proto::ChainMetadata {
    fn from(metadata: ChainMetadata) -> Self {
        let accumulated_difficulty = metadata.accumulated_difficulty().to_be_bytes().to_vec();
        Self {
            height_of_longest_chain: Some(metadata.height_of_longest_chain()),
            best_block: Some(metadata.best_block().to_vec()),
            pruned_height: metadata.pruned_height(),
            accumulated_difficulty,
            timestamp: Some(metadata.timestamp()),
        }
    }
}

impl proto::ChainMetadata {
    pub fn height_of_longest_chain(&self) -> u64 {
        self.height_of_longest_chain.unwrap_or(0)
    }
}
