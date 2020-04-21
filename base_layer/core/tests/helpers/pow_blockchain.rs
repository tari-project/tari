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

use super::block_builders::chain_block;
use tari_core::{
    blocks::Block,
    chain_storage::{BlockchainDatabase, MemoryDatabase},
    consensus::ConsensusConstants,
    proof_of_work::{lwma_diff::LinearWeightedMovingAverage, Difficulty, DifficultyAdjustment, PowAlgorithm},
    transactions::types::HashDigest,
};

pub fn create_test_pow_blockchain(
    db: &BlockchainDatabase<MemoryDatabase<HashDigest>>,
    mut pow_algos: Vec<PowAlgorithm>,
    consensus_constants: &ConsensusConstants,
)
{
    // Remove the first as it will be replaced by the genesis block
    pow_algos.remove(0);
    let block0 = db.fetch_block(0).unwrap().block().clone();
    append_to_pow_blockchain(db, block0, pow_algos, consensus_constants);
}

pub fn append_to_pow_blockchain(
    db: &BlockchainDatabase<MemoryDatabase<HashDigest>>,
    chain_tip: Block,
    pow_algos: Vec<PowAlgorithm>,
    consensus: &ConsensusConstants,
)
{
    let mut prev_block = chain_tip;
    for pow_algo in pow_algos {
        let new_block = chain_block(&prev_block, Vec::new(), consensus);
        let mut new_block = db.calculate_mmr_roots(new_block).unwrap();
        new_block.header.timestamp = prev_block
            .header
            .timestamp
            .increase(consensus.get_target_block_interval());
        new_block.header.pow.pow_algo = pow_algo;
        db.add_block(new_block.clone()).unwrap();
        prev_block = new_block;
    }
}

// Calculated the accumulated difficulty for the selected blocks in the blockchain db.
pub fn calculate_accumulated_difficulty(
    db: &BlockchainDatabase<MemoryDatabase<HashDigest>>,
    heights: Vec<u64>,
    consensus_constants: &ConsensusConstants,
) -> Difficulty
{
    let mut lwma = LinearWeightedMovingAverage::new(
        consensus_constants.get_difficulty_block_window() as usize,
        consensus_constants.get_diff_target_block_interval(),
        consensus_constants.min_pow_difficulty(),
        consensus_constants.get_difficulty_max_block_interval(),
    );
    for height in heights {
        let header = db.fetch_header(height).unwrap();

        lwma.add(header.timestamp, lwma.get_difficulty()).unwrap();
    }
    lwma.get_difficulty()
}
