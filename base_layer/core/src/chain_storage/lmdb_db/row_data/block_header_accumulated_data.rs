use primitive_types::U512;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{HashOutput, PrivateKey};

use crate::{
    blocks::{BlockAccumulatedData, BlockHeaderAccumulatedData},
    proof_of_work::{AccumulatedDifficulty, Difficulty},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct LmdbRowBlockHeaderAccumulatedDataV1 {
    pub hash: HashOutput,
    pub total_kernel_offset: PrivateKey,
    pub achieved_difficulty: Difficulty,
    pub total_accumulated_difficulty: U512,
    accumulated_monero_randomx_difficulty: AccumulatedDifficulty,
    accumulated_tari_randomx_difficulty: AccumulatedDifficulty,
    accumulated_sha3x_difficulty: AccumulatedDifficulty,
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
            accumulated_cuckaroo_difficulty: Difficulty::min(),
            target_difficulty: data.target_difficulty,
        }
    }
}

impl From<&BlockHeaderAccumulatedData> for LmdbRowBlockHeaderAccumulatedDataV1 {
    fn from(data: &BlockHeaderAccumulatedData) -> Self {
        LmdbRowBlockHeaderAccumulatedDataV1 {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset,
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
    accumulated_monero_randomx_difficulty: AccumulatedDifficulty,
    accumulated_tari_randomx_difficulty: AccumulatedDifficulty,
    accumulated_sha3x_difficulty: AccumulatedDifficulty,
    accumulated_cuckaroo_difficulty: AccumulatedDifficulty,
    pub target_difficulty: Difficulty,
}

impl From<LmdbRowBlockHeaderAccumulatedDataV2> for BlockHeaderAccumulatedData {
    fn from(data: LmdbRowBlockHeaderAccumulatedDataV2) -> Self {
        BlockHeaderAccumulatedData {
            hash: data.hash,
            total_kernel_offset: data.total_kernel_offset,
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
            total_kernel_offset: data.total_kernel_offset,
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
