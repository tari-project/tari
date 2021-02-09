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

use crate::helpers::block_builders::chain_block_with_new_coinbase;
use monero::{blockdata::block::Block as MoneroBlock, consensus::deserialize};
use std::{collections::HashMap, sync::Arc};
use tari_core::{
    blocks::{Block, BlockHeaderValidationError},
    chain_storage::{BlockchainDatabase, BlockchainDatabaseConfig, ChainStorageError, Validators},
    consensus::{
        consensus_constants::PowAlgorithmConstants,
        ConsensusConstantsBuilder,
        ConsensusManagerBuilder,
        Network,
    },
    proof_of_work::{
        monero_rx,
        monero_rx::MoneroData,
        randomx_factory::{RandomXConfig, RandomXFactory},
        PowAlgorithm,
    },
    test_helpers::blockchain::{create_store_with_consensus_and_validators, create_test_db},
    transactions::types::CryptoFactories,
    validation::{
        block_validators::{BodyOnlyValidator, OrphanBlockValidator},
        header_validator::HeaderValidator,
        mocks::MockValidator,
        ValidationError,
    },
};

mod helpers;

#[test]
fn test_genesis_block() {
    let factories = CryptoFactories::default();
    let network = Network::Stibbons;
    let rules = ConsensusManagerBuilder::new(network).build();
    let backend = create_test_db();
    let rx = RandomXFactory::new(RandomXConfig::default(), 1);
    let validators = Validators::new(
        BodyOnlyValidator::default(),
        HeaderValidator::new(rules.clone(), rx),
        OrphanBlockValidator::new(rules.clone(), factories),
    );
    let db = BlockchainDatabase::new(backend, &rules, validators, BlockchainDatabaseConfig::default(), false).unwrap();
    let block = rules.get_genesis_block();
    match db.add_block(block.block.into()).unwrap_err() {
        ChainStorageError::ValidationError { source } => match source {
            ValidationError::ValidatingGenesis => (),
            _ => panic!("Failed because incorrect validation error was received"),
        },
        _ => panic!("Failed because incorrect ChainStorageError was received"),
    }
}

#[test]
fn test_monero_blocks() {
    // Create temporary test folder
    let seed1 = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
    let seed2 = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad98".to_string();

    let factories = CryptoFactories::default();
    let network = Network::Stibbons;
    let mut algos = HashMap::new();
    algos.insert(PowAlgorithm::Sha3, PowAlgorithmConstants {
        max_target_time: 1800,
        min_difficulty: 1.into(),
        max_difficulty: 1.into(),
        target_time: 300,
    });
    algos.insert(PowAlgorithm::Monero, PowAlgorithmConstants {
        max_target_time: 1200,
        min_difficulty: 1.into(),
        max_difficulty: 1.into(),
        target_time: 200,
    });
    let cc = ConsensusConstantsBuilder::new(network)
        .with_max_randomx_seed_height(1)
        .with_proof_of_work(algos)
        .build();
    let cm = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(cc)
        .build();
    let header_validator = HeaderValidator::new(cm.clone(), RandomXFactory::default());
    let db = create_store_with_consensus_and_validators(
        &cm,
        Validators::new(MockValidator::new(true), header_validator, MockValidator::new(true)),
    );
    let block_0 = db.fetch_block(0).unwrap().try_into_chain_block().unwrap();
    let (block_1_t, _) = chain_block_with_new_coinbase(&block_0, vec![], &cm, &factories);
    let mut block_1 = db.prepare_block_merkle_roots(block_1_t).unwrap();

    // Now we have block 1, lets add monero data to it
    add_monero_data(&mut block_1, seed1.clone());
    let cb_1 = db.add_block(Arc::new(block_1.clone())).unwrap().assert_added();
    // Now lets add a second faulty block using the same seed hash
    let (block_2_t, _) = chain_block_with_new_coinbase(&cb_1, vec![], &cm, &factories);
    let mut block_2 = db.prepare_block_merkle_roots(block_2_t).unwrap();

    add_monero_data(&mut block_2, seed1.clone());
    let cb_2 = db.add_block(Arc::new(block_2.clone())).unwrap().assert_added();
    // Now lets add a third faulty block using the same seed hash. This should fail.
    let (block_3_t, _) = chain_block_with_new_coinbase(&cb_2, vec![], &cm, &factories);
    let mut block_3 = db.prepare_block_merkle_roots(block_3_t).unwrap();
    let mut block_3_broken = block_3.clone();
    add_monero_data(&mut block_3_broken, seed1.clone());
    match db.add_block(Arc::new(block_3_broken.clone())) {
        Err(ChainStorageError::ValidationError {
            source: ValidationError::BlockHeaderError(BlockHeaderValidationError::OldSeedHash),
        }) => (),
        Err(e) => {
            panic!("Failed due to other error:{}", e);
        },
        Ok(res) => {
            panic!("Block add unexpectedly succeeded with result: {}", res);
        },
    };

    // now lets fix the seed, and try again
    add_monero_data(&mut block_3, seed2.clone());
    db.add_block(Arc::new(block_3.clone())).unwrap().assert_added();
}

fn add_monero_data(tblock: &mut Block, seed_hash: String) {
    let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
    let bytes = hex::decode(blocktemplate_blob).unwrap();
    let mut mblock = deserialize::<MoneroBlock>(&bytes[..]).unwrap();
    let hash = tblock.header.merged_mining_hash();
    monero_rx::append_merge_mining_tag(&mut mblock, hash).unwrap();
    let hashes = monero_rx::create_ordered_transaction_hashes_from_block(&mblock);
    let root = monero_rx::tree_hash(&hashes).unwrap();
    let monero_data = MoneroData {
        header: mblock.header,
        key: seed_hash,
        count: hashes.len() as u16,
        transaction_root: root.to_fixed_bytes(),
        transaction_hashes: hashes.into_iter().map(|h| h.to_fixed_bytes()).collect(),
        coinbase_tx: mblock.miner_tx,
    };
    let serialized = bincode::serialize(&monero_data).unwrap();
    tblock.header.pow.pow_algo = PowAlgorithm::Monero;
    tblock.header.pow.pow_data = serialized;
}
