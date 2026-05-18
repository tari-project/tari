//  Copyright 2025, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use primitive_types::U512;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{HashOutput, PrivateKey};
use tari_node_components::blocks::BlockHeaderAccumulatedData;
use tari_transaction_components::tari_proof_of_work::{AccumulatedDifficulty, Difficulty};

#[derive(Serialize, Deserialize, Debug)]
pub struct LmdbRowBlockHeaderAccumulatedDataV1 {
    pub hash: HashOutput,
    pub total_kernel_offset: PrivateKey,
    pub achieved_difficulty: Difficulty,
    pub total_accumulated_difficulty: U512,
    pub accumulated_monero_randomx_difficulty: AccumulatedDifficulty,
    pub accumulated_tari_randomx_difficulty: AccumulatedDifficulty,
    pub accumulated_sha3x_difficulty: AccumulatedDifficulty,
    pub target_difficulty: Difficulty,
}

impl From<LmdbRowBlockHeaderAccumulatedDataV1> for BlockHeaderAccumulatedData {
    fn from(data: LmdbRowBlockHeaderAccumulatedDataV1) -> Self {
        BlockHeaderAccumulatedData {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset,
            achieved_difficulty: data.achieved_difficulty,
            total_accumulated_difficulty: data.total_accumulated_difficulty,
            accumulated_monero_randomx_difficulty: data.accumulated_monero_randomx_difficulty,
            accumulated_tari_randomx_difficulty: data.accumulated_tari_randomx_difficulty,
            accumulated_sha3x_difficulty: data.accumulated_sha3x_difficulty,
            accumulated_cuckaroo_difficulty: AccumulatedDifficulty::min(),
            target_difficulty: data.target_difficulty,
        }
    }
}

impl From<&BlockHeaderAccumulatedData> for LmdbRowBlockHeaderAccumulatedDataV1 {
    fn from(data: &BlockHeaderAccumulatedData) -> Self {
        LmdbRowBlockHeaderAccumulatedDataV1 {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset.clone(),
            achieved_difficulty: data.achieved_difficulty,
            total_accumulated_difficulty: data.total_accumulated_difficulty,
            accumulated_monero_randomx_difficulty: data.accumulated_monero_randomx_difficulty,
            accumulated_tari_randomx_difficulty: data.accumulated_tari_randomx_difficulty,
            accumulated_sha3x_difficulty: data.accumulated_sha3x_difficulty,
            target_difficulty: data.target_difficulty,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LmdbRowBlockHeaderAccumulatedDataV2 {
    pub hash: HashOutput,
    pub total_kernel_offset: PrivateKey,
    pub achieved_difficulty: Difficulty,
    pub total_accumulated_difficulty: U512,
    pub accumulated_monero_randomx_difficulty: AccumulatedDifficulty,
    pub accumulated_tari_randomx_difficulty: AccumulatedDifficulty,
    pub accumulated_sha3x_difficulty: AccumulatedDifficulty,
    pub accumulated_cuckaroo_difficulty: AccumulatedDifficulty,
    pub target_difficulty: Difficulty,
}

impl From<LmdbRowBlockHeaderAccumulatedDataV2> for BlockHeaderAccumulatedData {
    fn from(data: LmdbRowBlockHeaderAccumulatedDataV2) -> Self {
        BlockHeaderAccumulatedData {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset.clone(),
            achieved_difficulty: data.achieved_difficulty,
            total_accumulated_difficulty: data.total_accumulated_difficulty,
            accumulated_monero_randomx_difficulty: data.accumulated_monero_randomx_difficulty,
            accumulated_tari_randomx_difficulty: data.accumulated_tari_randomx_difficulty,
            accumulated_sha3x_difficulty: data.accumulated_sha3x_difficulty,
            accumulated_cuckaroo_difficulty: data.accumulated_cuckaroo_difficulty,
            target_difficulty: data.target_difficulty,
        }
    }
}

impl From<&BlockHeaderAccumulatedData> for LmdbRowBlockHeaderAccumulatedDataV2 {
    fn from(data: &BlockHeaderAccumulatedData) -> Self {
        LmdbRowBlockHeaderAccumulatedDataV2 {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset.clone(),
            achieved_difficulty: data.achieved_difficulty,
            total_accumulated_difficulty: data.total_accumulated_difficulty,
            accumulated_monero_randomx_difficulty: data.accumulated_monero_randomx_difficulty,
            accumulated_tari_randomx_difficulty: data.accumulated_tari_randomx_difficulty,
            accumulated_sha3x_difficulty: data.accumulated_sha3x_difficulty,
            accumulated_cuckaroo_difficulty: data.accumulated_cuckaroo_difficulty,
            target_difficulty: data.target_difficulty,
        }
    }
}
