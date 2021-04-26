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

pub mod blockchain;

use crate::{
    blocks::{Block, BlockHeader},
    consensus::ConsensusManager,
    transactions::transaction::Transaction,
};

use crate::{
    chain_storage::{BlockHeaderAccumulatedData, ChainHeader},
    consensus::{ConsensusManagerBuilder, Network},
    crypto::tari_utilities::Hashable,
    proof_of_work::{sha3_difficulty, Difficulty},
    test_helpers::blockchain::TempDatabase,
    transactions::{types::CryptoFactories, CoinbaseBuilder},
    validation::{mocks::MockValidator, HeaderValidation},
};
use rand::{distributions::Alphanumeric, Rng};
use std::{iter, path::Path, sync::Arc};
use tari_comms::PeerManager;
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};

/// Create a partially constructed block using the provided set of transactions
/// is chain_block, or rename it to `create_orphan_block` and drop the prev_block argument
pub fn create_orphan_block(block_height: u64, transactions: Vec<Transaction>, consensus: &ConsensusManager) -> Block {
    create_block(
        consensus.consensus_constants(block_height).blockchain_version(),
        block_height,
        transactions,
    )
}

pub fn create_block(block_version: u16, block_height: u64, transactions: Vec<Transaction>) -> Block {
    let mut header = BlockHeader::new(block_version);
    header.height = block_height;
    if transactions.is_empty() {
        let constants = ConsensusManagerBuilder::new(Network::LocalNet).build();
        let coinbase = CoinbaseBuilder::new(CryptoFactories::default())
            .with_block_height(block_height)
            .with_fees(0.into())
            .with_nonce(0.into())
            .with_spend_key(block_height.into())
            .build_with_reward(constants.consensus_constants(block_height), 1.into())
            .unwrap();
        header.into_builder().with_transactions(vec![coinbase.0]).build()
    } else {
        header.into_builder().with_transactions(transactions).build()
    }
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
            .map(|_| rng.sample(Alphanumeric))
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

pub fn create_chain_header(
    db: &TempDatabase,
    header: BlockHeader,
    prev_accum: &BlockHeaderAccumulatedData,
) -> ChainHeader
{
    let validator = MockValidator::new(true);
    let achieved_target_diff = validator.validate(db, &header).unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(prev_accum)
        .with_hash(header.hash())
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(header.total_kernel_offset.clone())
        .build()
        .unwrap();
    ChainHeader::try_construct(header, accumulated_data).unwrap()
}
