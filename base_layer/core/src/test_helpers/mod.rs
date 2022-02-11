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

//! Common test helper functions that are small and useful enough to be included in the main crate, rather than the
//! integration test folder.

use std::{iter, path::Path, sync::Arc};

pub use block_spec::{BlockSpec, BlockSpecs};
use rand::{distributions::Alphanumeric, Rng};
use tari_common::configuration::Network;
use tari_comms::PeerManager;
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tari_utilities::Hashable;

use crate::{
    blocks::{Block, BlockHeader, BlockHeaderAccumulatedData, ChainHeader},
    consensus::{ConsensusConstants, ConsensusManager},
    proof_of_work::{sha3_difficulty, AchievedTargetDifficulty, Difficulty},
    transactions::{
        transaction_components::{Transaction, UnblindedOutput},
        CoinbaseBuilder,
        CryptoFactories,
    },
};

#[macro_use]
mod block_spec;
pub mod blockchain;

pub fn create_consensus_rules() -> ConsensusManager {
    ConsensusManager::builder(Network::LocalNet).build()
}

pub fn create_consensus_constants(height: u64) -> ConsensusConstants {
    create_consensus_rules().consensus_constants(height).clone()
}

/// Create a partially constructed block using the provided set of transactions
/// is chain_block, or rename it to `create_orphan_block` and drop the prev_block argument
pub fn create_orphan_block(block_height: u64, transactions: Vec<Transaction>, consensus: &ConsensusManager) -> Block {
    let mut header = BlockHeader::new(consensus.consensus_constants(block_height).blockchain_version());
    header.height = block_height;
    header.into_builder().with_transactions(transactions).build()
}

pub fn create_block(rules: &ConsensusManager, prev_block: &Block, spec: BlockSpec) -> (Block, UnblindedOutput) {
    let mut header = BlockHeader::from_previous(&prev_block.header);
    let block_height = spec.height_override.unwrap_or(prev_block.header.height + 1);
    header.height = block_height;
    // header.prev_hash = prev_block.hash();
    let reward = spec.reward_override.unwrap_or_else(|| {
        rules.calculate_coinbase_and_fees(
            header.height,
            &spec
                .transactions
                .iter()
                .map(|tx| tx.body.kernels().clone())
                .flatten()
                .collect::<Vec<_>>(),
        )
    });

    let (coinbase, coinbase_output) = CoinbaseBuilder::new(CryptoFactories::default())
        .with_block_height(header.height)
        .with_fees(0.into())
        .with_nonce(0.into())
        .with_spend_key(block_height.into())
        .build_with_reward(rules.consensus_constants(block_height), reward)
        .unwrap();

    let mut block = header
        .into_builder()
        .with_transactions(
            Some(coinbase)
                .filter(|_| !spec.skip_coinbase)
                .into_iter()
                .chain(spec.transactions)
                .collect(),
        )
        .build();

    // Keep times constant in case we need a particular target difficulty
    block.header.timestamp = prev_block.header.timestamp.increase(spec.block_time);
    block.header.output_mmr_size = prev_block.header.output_mmr_size + block.body.outputs().len() as u64;
    block.header.kernel_mmr_size = prev_block.header.kernel_mmr_size + block.body.kernels().len() as u64;
    (block, coinbase_output)
}

pub fn mine_to_difficulty(mut block: Block, difficulty: Difficulty) -> Result<Block, String> {
    // When starting from the same nonce, in tests it becomes common to mine the same block more than once without the
    // hash changing. This introduces the required entropy
    block.header.nonce = rand::thread_rng().gen();
    for _i in 0..10000 {
        if sha3_difficulty(&block.header) == difficulty {
            return Ok(block);
        }
        block.header.nonce += 1;
    }
    Err("Could not mine to difficulty in 10000 iterations".to_string())
}

pub fn create_peer_manager<P: AsRef<Path>>(data_path: P) -> Arc<PeerManager> {
    let peer_database_name = {
        let mut rng = rand::thread_rng();
        iter::repeat(())
            .map(|_| rng.sample(Alphanumeric) as char)
            .take(8)
            .collect::<String>()
    };
    std::fs::create_dir_all(&data_path).unwrap();
    let datastore = LMDBBuilder::new()
        .set_path(data_path)
        .set_env_config(Default::default())
        .set_max_number_of_databases(1)
        .add_database(&peer_database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&peer_database_name).unwrap();
    Arc::new(PeerManager::new(LMDBWrapper::new(Arc::new(peer_database)), None).unwrap())
}

pub fn create_chain_header(header: BlockHeader, prev_accum: &BlockHeaderAccumulatedData) -> ChainHeader {
    let achieved_target_diff = AchievedTargetDifficulty::try_construct(header.pow_algo(), 1.into(), 1.into()).unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(prev_accum)
        .with_hash(header.hash())
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(header.total_kernel_offset.clone())
        .build()
        .unwrap();
    ChainHeader::try_construct(header, accumulated_data).unwrap()
}
