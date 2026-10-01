//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that
// the  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES,  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL,  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY,  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.

#![allow(clippy::indexing_slicing)]
use std::sync::Arc;

use tari_common_types::tari_address::TariAddress;
use tari_node_components::blocks::{Block, BlockHeader, NewBlockTemplate};
use tari_transaction_components::{
    key_manager::{KeyManager, TariKeyId},
    tari_amount::T,
    tari_proof_of_work::{Difficulty, PowAlgorithm},
    test_helpers::schema_to_transaction,
    transaction_components::{Transaction, WalletOutput},
    txn_schema,
};

use crate::{
    chain_storage::{BlockchainDatabase, ChainStorageError},
    proof_of_work::AchievedTargetDifficulty,
    test_helpers::{
        BlockSpec,
        blockchain::{TempDatabase, create_new_blockchain},
        create_block,
        default_coinbase_entities,
    },
};

fn setup() -> BlockchainDatabase<TempDatabase> {
    create_new_blockchain()
}

fn create_next_block(
    db: &BlockchainDatabase<TempDatabase>,
    prev_block: &Block,
    transactions: Vec<Arc<Transaction>>,
    key_manager: &KeyManager,
    script_key_id: &TariKeyId,
    wallet_payment_address: &TariAddress,
) -> (Arc<Block>, WalletOutput) {
    let rules = db.rules();
    let (block, output) = create_block(
        db,
        rules,
        prev_block,
        BlockSpec::new()
            .with_transactions(transactions.into_iter().map(|t| (*t).clone()).collect())
            .finish(),
        key_manager,
        script_key_id,
        wallet_payment_address,
        None,
    );
    let block = apply_mmr_to_block(db, block);
    (Arc::new(block), output)
}

pub fn apply_mmr_to_block(db: &BlockchainDatabase<TempDatabase>, block: Block) -> Block {
    let (mut block, mmr_roots) = db.calculate_mmr_roots(block).unwrap();
    block.header.input_mr = mmr_roots.input_mr;
    block.header.output_mr = mmr_roots.output_mr;
    block.header.output_smt_size = mmr_roots.output_smt_size;
    block.header.kernel_mr = mmr_roots.kernel_mr;
    block.header.kernel_mmr_size = mmr_roots.kernel_mmr_size;
    block.header.validator_node_mr = mmr_roots.validator_node_mr;
    block.header.validator_node_size = mmr_roots.validator_node_size;
    block
}
fn add_many_chained_blocks(
    size: usize,
    db: &BlockchainDatabase<TempDatabase>,
    key_manager: &KeyManager,
) -> (Vec<Arc<Block>>, Vec<WalletOutput>) {
    let last_header = db.fetch_last_header().unwrap();
    let mut prev_block = Arc::new(db.fetch_block(last_header.height, true).unwrap().into_block());
    let mut blocks = Vec::with_capacity(size);
    let mut outputs = Vec::with_capacity(size);
    let (script_key_id, wallet_payment_address) = default_coinbase_entities(key_manager);
    for _ in 1..=size {
        let (block, coinbase_utxo) = create_next_block(
            db,
            &prev_block,
            vec![],
            key_manager,
            &script_key_id,
            &wallet_payment_address,
        );

        db.add_block(block.clone()).unwrap().assert_added();
        prev_block = block.clone();
        blocks.push(block);
        outputs.push(coinbase_utxo);
    }
    (blocks, outputs)
}

mod fetch_blocks {

    use super::*;

    #[test]
    fn it_returns_genesis() {
        let db = setup();
        let blocks = db.fetch_blocks(0.., true).unwrap();
        assert_eq!(blocks.len(), 1);
    }

    #[tokio::test]
    async fn it_returns_all() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(4, &db, &key_manager);
        let blocks = db.fetch_blocks(.., true).unwrap();
        assert_eq!(blocks.len(), 5);
        for (i, item) in blocks.iter().enumerate().take(4 + 1) {
            assert_eq!(item.header().height, i as u64);
        }
    }

    #[tokio::test]
    async fn it_returns_one() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        let (new_blocks, _) = add_many_chained_blocks(1, &db, &key_manager);
        let blocks = db.fetch_blocks(1..=1, true).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block().hash(), new_blocks[0].hash());
    }

    #[tokio::test]
    async fn it_returns_nothing_if_asking_for_blocks_out_of_range() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(1, &db, &key_manager);
        let blocks = db.fetch_blocks(2.., true).unwrap();
        assert!(blocks.is_empty());
    }

    #[tokio::test]
    async fn it_returns_blocks_between_bounds_exclusive() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let blocks = db.fetch_blocks(3..5, true).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].header().height, 3);
        assert_eq!(blocks[1].header().height, 4);
    }

    #[tokio::test]
    async fn it_returns_blocks_between_bounds_inclusive() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let blocks = db.fetch_blocks(3..=5, true).unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].header().height, 3);
        assert_eq!(blocks[1].header().height, 4);
        assert_eq!(blocks[2].header().height, 5);
    }

    #[tokio::test]
    async fn it_returns_blocks_to_the_tip() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let blocks = db.fetch_blocks(3.., true).unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].header().height, 3);
        assert_eq!(blocks[1].header().height, 4);
        assert_eq!(blocks[2].header().height, 5);
    }

    #[tokio::test]
    async fn it_returns_blocks_from_genesis() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let blocks = db.fetch_blocks(..=3, true).unwrap();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].header().height, 0);
        assert_eq!(blocks[1].header().height, 1);
        assert_eq!(blocks[2].header().height, 2);
        assert_eq!(blocks[3].header().height, 3);
    }
}

mod fetch_headers {

    use super::*;

    #[test]
    fn it_returns_genesis() {
        let db = setup();
        let headers = db.fetch_headers(0..).unwrap();
        assert_eq!(headers.len(), 1);
        let headers = db.fetch_headers(0..0).unwrap();
        assert_eq!(headers.len(), 1);
        let headers = db.fetch_headers(0..=0).unwrap();
        assert_eq!(headers.len(), 1);
        let headers = db.fetch_headers(..).unwrap();
        assert_eq!(headers.len(), 1);
    }

    #[tokio::test]
    async fn it_returns_all() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(4, &db, &key_manager);
        let headers = db.fetch_headers(..).unwrap();
        assert_eq!(headers.len(), 5);
        for (i, item) in headers.iter().enumerate().take(4 + 1) {
            assert_eq!(item.height, i as u64);
        }
    }

    #[tokio::test]
    async fn it_returns_nothing_if_asking_for_blocks_out_of_range() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(1, &db, &key_manager);
        let headers = db.fetch_headers(2..).unwrap();
        assert!(headers.is_empty());
    }

    #[tokio::test]
    async fn it_returns_blocks_between_bounds_exclusive() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let headers = db.fetch_headers(3..5).unwrap();
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].height, 3);
        assert_eq!(headers[1].height, 4);
    }

    #[tokio::test]
    async fn it_returns_blocks_between_bounds_inclusive() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let headers = db.fetch_headers(3..=5).unwrap();
        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0].height, 3);
        assert_eq!(headers[1].height, 4);
        assert_eq!(headers[2].height, 5);
    }
    #[tokio::test]
    async fn it_returns_blocks_to_the_tip() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let headers = db.fetch_headers(3..).unwrap();
        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0].height, 3);
        assert_eq!(headers[1].height, 4);
        assert_eq!(headers[2].height, 5);
    }

    #[tokio::test]
    async fn it_returns_blocks_from_genesis() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let headers = db.fetch_headers(..=3).unwrap();
        assert_eq!(headers.len(), 4);
        assert_eq!(headers[0].height, 0);
        assert_eq!(headers[1].height, 1);
        assert_eq!(headers[2].height, 2);
        assert_eq!(headers[3].height, 3);
    }
}

mod find_headers_after_hash {
    use tari_common_types::types::FixedHash;

    use super::*;

    #[test]
    fn it_returns_none_given_empty_vec() {
        let db = setup();
        let hashes = vec![];
        assert!(db.find_headers_after_hash(hashes, 1).unwrap().is_none());
    }

    #[tokio::test]
    async fn it_returns_from_genesis() {
        let db = setup();
        let genesis_hash = db.fetch_block(0, true).unwrap().block().hash();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(1, &db, &key_manager);
        let hashes = vec![genesis_hash];
        let (index, headers) = db.find_headers_after_hash(hashes, 1).unwrap().unwrap();
        assert_eq!(index, 0);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].prev_hash, genesis_hash);
    }
    #[tokio::test]
    async fn it_returns_the_first_headers_found() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let hashes = (1..=3)
            .rev()
            .map(|i| db.fetch_block(i, true).unwrap().block().hash())
            .collect::<Vec<_>>();
        let (index, headers) = db.find_headers_after_hash(hashes, 10).unwrap().unwrap();
        assert_eq!(index, 0);
        assert_eq!(headers.len(), 2);
        assert_eq!(&headers[0], db.fetch_block(4, true).unwrap().header());
    }

    #[tokio::test]
    async fn fnit_ignores_unknown_hashes() {
        let db = setup();

        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let hashes = (2..=4)
            .map(|i| db.fetch_block(i, true).unwrap().block().hash())
            .chain(vec![FixedHash::zero(), FixedHash::zero()])
            .rev();
        let (index, headers) = db.find_headers_after_hash(hashes, 1).unwrap().unwrap();
        assert_eq!(index, 2);
        assert_eq!(headers.len(), 1);
        assert_eq!(&headers[0], db.fetch_block(5, true).unwrap().header());
    }
}

mod fetch_block_hashes_from_header_tip {

    use super::*;

    #[test]
    fn it_returns_genesis() {
        let db = setup();
        let genesis = db.fetch_tip_header().unwrap();
        let hashes = db.fetch_block_hashes_from_header_tip(10, 0).unwrap();
        assert_eq!(hashes.len(), 1);
        assert_eq!(&hashes[0], genesis.hash());
    }
    #[tokio::test]
    async fn it_returns_empty_set_for_big_offset() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        add_many_chained_blocks(5, &db, &key_manager);
        let hashes = db.fetch_block_hashes_from_header_tip(3, 6).unwrap();
        assert!(hashes.is_empty());
    }

    #[tokio::test]
    async fn it_returns_n_hashes_from_tip() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        let (blocks, _) = add_many_chained_blocks(5, &db, &key_manager);
        let hashes = db.fetch_block_hashes_from_header_tip(3, 1).unwrap();
        assert_eq!(hashes.len(), 3);
        assert_eq!(hashes[0], blocks[3].hash());
        assert_eq!(hashes[1], blocks[2].hash());
        assert_eq!(hashes[2], blocks[1].hash());
    }

    #[tokio::test]
    async fn it_returns_hashes_without_overlapping() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        let (blocks, _) = add_many_chained_blocks(3, &db, &key_manager);
        let hashes = db.fetch_block_hashes_from_header_tip(2, 0).unwrap();
        assert_eq!(hashes[0], blocks[2].hash());
        assert_eq!(hashes[1], blocks[1].hash());
        let hashes = db.fetch_block_hashes_from_header_tip(1, 2).unwrap();
        assert_eq!(hashes[0], blocks[0].hash());
    }

    #[tokio::test]
    async fn it_returns_all_hashes_from_tip() {
        let db = setup();
        let genesis = db.fetch_tip_header().unwrap();
        let key_manager = KeyManager::new_random().unwrap();
        let (blocks, _) = add_many_chained_blocks(5, &db, &key_manager);
        let hashes = db.fetch_block_hashes_from_header_tip(10, 0).unwrap();
        assert_eq!(hashes.len(), 6);
        assert_eq!(hashes[0], blocks[4].hash());
        assert_eq!(&hashes[5], genesis.hash());
    }
}

mod get_stats {
    use super::*;

    #[test]
    fn it_works_when_db_is_empty() {
        let db = setup();
        let stats = db.get_stats().unwrap();
        assert_eq!(stats.root().depth, 1);
    }
}

mod fetch_total_size_stats {

    use super::*;

    #[tokio::test]
    async fn it_measures_the_number_of_entries() {
        let db = setup();
        let genesis_output_count = db.fetch_header(0).unwrap().unwrap().output_smt_size;
        let key_manager = KeyManager::new_random().unwrap();
        let _block_and_outputs = add_many_chained_blocks(2, &db, &key_manager);
        let stats = db.fetch_total_size_stats().unwrap();
        assert_eq!(
            stats.sizes().iter().find(|s| s.name == "utxos").unwrap().num_entries,
            genesis_output_count + 2
        );
    }
}

mod prepare_new_block {
    use super::*;

    #[test]
    fn it_errors_for_genesis_block() {
        let db = setup();
        let genesis = db.fetch_block(0, true).unwrap();
        let template =
            NewBlockTemplate::from_block(genesis.block().clone(), Difficulty::min(), 5000 * T, true).unwrap();
        let err = db.prepare_new_block(template).unwrap_err();
        assert!(matches!(err, ChainStorageError::InvalidArguments { .. }));
    }

    #[test]
    fn it_errors_for_non_tip_template() {
        let db = setup();
        let genesis = db.fetch_block(0, true).unwrap();
        let next_block = BlockHeader::from_previous(genesis.header());
        let mut template =
            NewBlockTemplate::from_block(next_block.into_builder().build(), Difficulty::min(), 5000 * T, true).unwrap();
        // This would cause a panic if the sanity checks were not there
        template.header.height = 100;
        let err = db.prepare_new_block(template.clone()).unwrap_err();
        assert!(matches!(err, ChainStorageError::InvalidArguments { .. }));
        template.header.height = 1;
        template.header.prev_hash[0] += 1;
        let err = db.prepare_new_block(template).unwrap_err();
        assert!(matches!(err, ChainStorageError::InvalidArguments { .. }));
    }
    #[test]
    fn it_prepares_the_first_block() {
        let db = setup();
        let genesis = db.fetch_block(0, true).unwrap();
        let next_block = BlockHeader::from_previous(genesis.header());
        let template =
            NewBlockTemplate::from_block(next_block.into_builder().build(), Difficulty::min(), 5000 * T, true).unwrap();
        let block = db.prepare_new_block(template).unwrap();
        assert_eq!(block.header.height, 1);
    }
}

mod fetch_header_containing_kernel_mmr {

    use super::*;
    #[tokio::test]
    async fn it_returns_corresponding_header() {
        let db = setup();
        let genesis = db.fetch_block(0, true).unwrap();
        let key_manager = KeyManager::new_random().unwrap();
        let (blocks, outputs) = add_many_chained_blocks(1, &db, &key_manager);
        let num_genesis_kernels = genesis.block().body.kernels().len() as u64;

        let (txns, _) = schema_to_transaction(
            &[txn_schema!(from: vec![outputs[0].clone()], to: vec![50 * T])],
            &key_manager,
        );

        let (script_key_id, wallet_payment_address) = default_coinbase_entities(&key_manager);
        let (block, _) = create_next_block(
            &db,
            &blocks[0],
            txns,
            &key_manager,
            &script_key_id,
            &wallet_payment_address,
        );
        db.add_block(block).unwrap();
        let _block_and_outputs = add_many_chained_blocks(3, &db, &key_manager);

        let header = db.fetch_header_containing_kernel_mmr(num_genesis_kernels).unwrap();
        assert_eq!(header.height(), 1);

        for i in 1..=2 {
            let header = db.fetch_header_containing_kernel_mmr(num_genesis_kernels + i).unwrap();
            assert_eq!(header.height(), 2);
        }
        for i in 3..=5 {
            let header = db.fetch_header_containing_kernel_mmr(num_genesis_kernels + i).unwrap();
            assert_eq!(header.height(), i);
        }

        let err = db
            .fetch_header_containing_kernel_mmr(num_genesis_kernels + 6)
            .unwrap_err();
        matches!(err, ChainStorageError::ValueNotFound { .. });
    }
}

mod clear_all_pending_headers {
    use tari_node_components::blocks::ChainHeader;

    use super::*;
    use crate::blocks::BlockHeaderAccumulatedDataBuilder;

    #[tokio::test]
    async fn it_clears_no_headers() {
        let db = setup();
        assert_eq!(db.clear_all_pending_headers().unwrap(), 0);
        let key_manager = KeyManager::new_random().unwrap();
        let _block_and_outputs = add_many_chained_blocks(2, &db, &key_manager);
        db.clear_all_pending_headers().unwrap();
        let last_header = db.fetch_last_header().unwrap();
        assert_eq!(last_header.height, 2);
    }

    #[tokio::test]
    async fn it_clears_headers_after_tip() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        let _blocks_and_outputs = add_many_chained_blocks(2, &db, &key_manager);
        let prev_block = db.fetch_block(2, true).unwrap();
        let mut prev_accum = prev_block.accumulated_data().clone();
        let mut prev_header = prev_block.try_into_chain_block().unwrap().to_chain_header();
        let headers = (0..5)
            .map(|_| {
                let mut header = BlockHeader::from_previous(prev_header.header());
                header.kernel_mmr_size = header.kernel_mmr_size.saturating_add(1);
                header.output_smt_size = header.output_smt_size.saturating_add(1);
                let accum = BlockHeaderAccumulatedDataBuilder::from_previous(&prev_accum)
                    .with_hash(header.hash())
                    .with_achieved_target_difficulty(
                        AchievedTargetDifficulty::try_construct(
                            PowAlgorithm::Sha3x,
                            Difficulty::min(),
                            Difficulty::min(),
                            Difficulty::min(),
                        )
                        .unwrap(),
                    )
                    .with_total_kernel_offset(Default::default())
                    .build(db.consensus_constants().unwrap())
                    .unwrap();

                let header = ChainHeader::try_construct(header, accum.clone()).unwrap();

                prev_header = header.clone();
                prev_accum = accum;
                header
            })
            .collect();
        db.insert_valid_headers(headers).unwrap();
        let last_header = db.fetch_last_header().unwrap();
        assert_eq!(last_header.height, 7);
        let num_deleted = db.clear_all_pending_headers().unwrap();
        assert_eq!(num_deleted, 5);
        let last_header = db.fetch_last_header().unwrap();
        assert_eq!(last_header.height, 2);
    }
}

mod validator_node_merkle_root {
    use std::convert::TryFrom;

    use tari_common::configuration::Network;
    use tari_common_types::{epoch::VnEpoch, types::CompressedPublicKey};
    use tari_transaction_components::transaction_components::{OutputFeatures, ValidatorNodeSignature};

    use super::*;
    use crate::{
        blocks::genesis_block::VALIDATOR_MR_EMPTY_PLACEHOLDER_HASH,
        chain_storage::calculate_validator_node_mr,
    };
    #[tokio::test]
    async fn it_has_the_correct_genesis_merkle_root() {
        let key_manager = KeyManager::new_random().unwrap();
        let db = setup();
        let (blocks, _outputs) = add_many_chained_blocks(1, &db, &key_manager);
        assert_eq!(blocks[0].header.validator_node_mr, VALIDATOR_MR_EMPTY_PLACEHOLDER_HASH);
    }

    #[tokio::test]
    async fn it_has_the_correct_merkle_root_for_current_vn_set() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        let (blocks, outputs) = add_many_chained_blocks(1, &db, &key_manager);

        let (sk, public_key) = CompressedPublicKey::random_keypair(&mut rand::rng());
        let signature = ValidatorNodeSignature::sign_for_registration(
            &sk,
            Network::LocalNet.as_byte(),
            None,
            &public_key,
            VnEpoch::zero(),
        );
        let features =
            OutputFeatures::for_validator_node_registration(signature, public_key.clone(), None, VnEpoch::zero());
        let (tx, _outputs) = schema_to_transaction(
            &[txn_schema!(
                from: vec![outputs[0].clone()],
                to: vec![50 * T],
                features: features
            )],
            &key_manager,
        );
        let (script_key_id, wallet_payment_address) = default_coinbase_entities(&key_manager);
        let (block, _) = create_next_block(
            &db,
            &blocks[0],
            tx,
            &key_manager,
            &script_key_id,
            &wallet_payment_address,
        );
        db.add_block(block).unwrap().assert_added();

        let consts = db.consensus_constants().unwrap();
        let (_, _) = add_many_chained_blocks(usize::try_from(consts.epoch_length()).unwrap(), &db, &key_manager);

        let vn = db.get_validator_node(None, public_key.clone()).unwrap().unwrap();
        let merkle_root = calculate_validator_node_mr(&[vn]).unwrap();

        let tip = db.fetch_tip_header().unwrap();
        assert_eq!(tip.header().validator_node_mr, merkle_root);
    }

    #[tokio::test]
    async fn it_has_the_correct_merkle_root_for_current_vn_set_with_sidechain() {
        let db = setup();
        let key_manager = KeyManager::new_random().unwrap();
        let (blocks, outputs) = add_many_chained_blocks(1, &db, &key_manager);

        let (sk, public_key) = CompressedPublicKey::random_keypair(&mut rand::rng());
        let (sidechain_private, sidechain_public) = CompressedPublicKey::random_keypair(&mut rand::rng());
        let signature = ValidatorNodeSignature::sign_for_registration(
            &sk,
            Network::LocalNet.as_byte(),
            Some(&sidechain_public),
            &public_key,
            VnEpoch::zero(),
        );
        let features = OutputFeatures::for_validator_node_registration(
            signature,
            public_key.clone(),
            Some(&sidechain_private),
            VnEpoch::zero(),
        );
        let (tx, _outputs) = schema_to_transaction(
            &[txn_schema!(
                from: vec![outputs[0].clone()],
                to: vec![50 * T],
                features: features
            )],
            &key_manager,
        );
        let (script_key_id, wallet_payment_address) = default_coinbase_entities(&key_manager);
        let (block, _) = create_next_block(
            &db,
            &blocks[0],
            tx,
            &key_manager,
            &script_key_id,
            &wallet_payment_address,
        );
        db.add_block(block).unwrap().assert_added();

        let consts = db.consensus_constants().unwrap();
        let (_, _) = add_many_chained_blocks(usize::try_from(consts.epoch_length()).unwrap(), &db, &key_manager);

        let vn = db
            .get_validator_node(Some(sidechain_public.clone()), public_key.clone())
            .unwrap()
            .unwrap();
        let merkle_root = calculate_validator_node_mr(&[vn]).unwrap();

        let tip = db.fetch_tip_header().unwrap();
        assert_eq!(tip.header().validator_node_mr, merkle_root);
        assert_ne!(tip.header().validator_node_mr, VALIDATOR_MR_EMPTY_PLACEHOLDER_HASH);
    }
}

/// Chain-state checks on validator node exits (and re-registration around them). Everything that would make the
/// commit-time exit (`ValidatorNodeStore::exit` / `get_next_exit_epoch`) fail must be rejected by
/// `check_validator_node_exit` / `check_validator_node_registration` instead. The test database uses mock validators,
/// so mined blocks go straight to commit.
mod validator_node_exit {
    // Overflow in test code panics, which is the desired failure mode for a test.
    #![allow(clippy::arithmetic_side_effects)]
    use std::convert::TryFrom;

    use tari_common::configuration::Network;
    use tari_common_types::{
        epoch::VnEpoch,
        types::{CompressedCommitment, CompressedPublicKey, PrivateKey},
    };
    use tari_script::ExecutionStack;
    use tari_transaction_components::{
        aggregated_body::AggregateBody,
        consensus::ConsensusConstantsBuilder,
        tari_amount::MicroMinotari,
        transaction_components::{
            OutputFeatures,
            SpentOutput,
            TransactionInput,
            TransactionOutput,
            ValidatorNodeSignature,
        },
    };

    use super::*;
    use crate::{
        chain_storage::BlockchainBackend,
        validation::{
            ValidationError,
            aggregate_body::AggregateBodyChainLinkedValidator,
            helpers::{check_validator_node_exit, check_validator_node_registration},
        },
    };

    const NETWORK: Network = Network::LocalNet;
    const MAX_EPOCH: VnEpoch = VnEpoch(1000);

    struct Chain {
        db: BlockchainDatabase<TempDatabase>,
        key_manager: KeyManager,
        sk: PrivateKey,
        public_key: CompressedPublicKey,
        spendable: Vec<WalletOutput>,
    }

    impl Chain {
        fn new() -> Self {
            let db = setup();
            let key_manager = KeyManager::new_random().unwrap();
            let (_, spendable) = add_many_chained_blocks(1, &db, &key_manager);
            let (sk, public_key) = CompressedPublicKey::random_keypair(&mut rand::rng());
            Self {
                db,
                key_manager,
                sk,
                public_key,
                spendable,
            }
        }

        /// The epoch the next block will be validated in
        fn current_epoch(&self) -> VnEpoch {
            let height = self.db.fetch_last_header().unwrap().height + 1;
            self.db.consensus_constants().unwrap().block_height_to_epoch(height)
        }

        fn mine_epochs(&mut self, epochs: u64) {
            let length = self.db.consensus_constants().unwrap().epoch_length();
            let (_, outputs) =
                add_many_chained_blocks(usize::try_from(length * epochs).unwrap(), &self.db, &self.key_manager);
            self.spendable.extend(outputs);
        }

        /// Mines a block with one transaction carrying an output with `features`, and returns that output
        fn mine(&mut self, features: OutputFeatures) -> TransactionOutput {
            self.mine_spendable(features).0
        }

        /// As [`Self::mine`], also returning the wallet output so that it can be spent
        fn mine_spendable(&mut self, features: OutputFeatures) -> (TransactionOutput, WalletOutput) {
            let from = self.spendable.pop().unwrap();
            let (tx, outputs) = schema_to_transaction(
                &[txn_schema!(from: vec![from], to: vec![T], features: features)],
                &self.key_manager,
            );
            let output = tx[0]
                .body
                .outputs()
                .iter()
                .find(|o| o.features.sidechain_feature.is_some())
                .unwrap()
                .clone();
            let wallet_output = outputs
                .into_iter()
                .find(|o| o.features().sidechain_feature.is_some())
                .unwrap();
            self.mine_txs(tx);
            (output, wallet_output)
        }

        /// Mines a block spending `output` (worth `T`) into a plain output, leaving room for the fee
        fn spend(&mut self, output: WalletOutput) {
            let (tx, _outputs) = schema_to_transaction(
                &[txn_schema!(from: vec![output], to: vec![MicroMinotari(500_000)])],
                &self.key_manager,
            );
            self.mine_txs(tx);
        }

        fn mine_txs(&mut self, tx: Vec<Arc<Transaction>>) {
            let last_header = self.db.fetch_last_header().unwrap();
            let prev_block = self.db.fetch_block(last_header.height, true).unwrap().into_block();
            let (script_key_id, wallet_payment_address) = default_coinbase_entities(&self.key_manager);
            let (block, coinbase) = create_next_block(
                &self.db,
                &prev_block,
                tx,
                &self.key_manager,
                &script_key_id,
                &wallet_payment_address,
            );
            self.db.add_block(block).unwrap().assert_added();
            self.spendable.push(coinbase);
        }

        fn tip_height(&self) -> u64 {
            self.db.fetch_last_header().unwrap().height
        }

        /// The registered entry's commitment, if the validator node is in the registered set
        fn registered_commitment(&self) -> Option<CompressedCommitment> {
            self.db
                .db_read_access()
                .unwrap()
                .fetch_validator_node_entry(None, &self.public_key)
                .unwrap()
                .map(|entry| entry.commitment)
        }

        fn registration_features(&self) -> OutputFeatures {
            let signature = ValidatorNodeSignature::sign_for_registration(
                &self.sk,
                NETWORK.as_byte(),
                None,
                &self.public_key,
                MAX_EPOCH,
            );
            OutputFeatures::for_validator_node_registration(signature, self.public_key.clone(), None, MAX_EPOCH)
        }

        fn exit_features(&self, activation_epoch: VnEpoch) -> OutputFeatures {
            let signature =
                ValidatorNodeSignature::sign_for_exit(&self.sk, NETWORK.as_byte(), None, activation_epoch, MAX_EPOCH);
            OutputFeatures::for_validator_node_exit(signature, None, activation_epoch, MAX_EPOCH)
        }

        fn exit_output(&self, activation_epoch: VnEpoch) -> TransactionOutput {
            TransactionOutput {
                features: self.exit_features(activation_epoch),
                ..Default::default()
            }
        }

        fn registration_output(&self) -> TransactionOutput {
            TransactionOutput {
                features: self.registration_features(),
                ..Default::default()
            }
        }

        /// The registered activation epoch, if the validator node is in the registered set
        fn activation_epoch(&self) -> Option<VnEpoch> {
            self.db
                .db_read_access()
                .unwrap()
                .fetch_validator_node_entry(None, &self.public_key)
                .unwrap()
                .map(|entry| entry.activation_epoch)
        }

        fn check_exit(&self, output: &TransactionOutput) -> Result<(), ValidationError> {
            let constants = self.db.consensus_constants().unwrap().clone();
            check_validator_node_exit(
                &*self.db.db_read_access().unwrap(),
                &constants,
                output,
                self.current_epoch(),
            )
        }

        /// Runs the mempool's chain-linked validation of a body spending `output`
        fn check_spend(&self, output: &TransactionOutput) -> Result<(), ValidationError> {
            let input = TransactionInput::new_current_version(
                SpentOutput::create_from_output(output.clone()),
                ExecutionStack::default(),
                Default::default(),
            );
            let body = AggregateBody::new_unsorted(vec![input], vec![], vec![]);
            let tip = self.db.fetch_tip_header().unwrap();
            AggregateBodyChainLinkedValidator::new(self.db.rules().clone()).validate_transaction_body(
                &body,
                tip.header(),
                &*self.db.db_read_access().unwrap(),
            )
        }

        fn check_registration(&self, output: &TransactionOutput) -> Result<(), ValidationError> {
            check_validator_node_registration(&*self.db.db_read_access().unwrap(), output, self.current_epoch())
        }

        /// Registers the validator node and mines until it is active. Returns the registration output and the
        /// activation epoch.
        fn register_and_activate(&mut self) -> (TransactionOutput, VnEpoch) {
            let registration = self.mine(self.registration_features());
            self.mine_epochs(2);
            let activation_epoch = self.activation_epoch().unwrap();
            assert!(activation_epoch <= self.current_epoch());
            (registration, activation_epoch)
        }
    }

    #[test]
    fn it_accepts_an_exit_for_an_active_validator_node() {
        let mut chain = Chain::new();
        let (_, activation_epoch) = chain.register_and_activate();
        chain.check_exit(&chain.exit_output(activation_epoch)).unwrap();
    }

    #[test]
    fn it_rejects_an_exit_for_a_validator_node_that_has_not_activated_yet() {
        let mut chain = Chain::new();
        chain.mine(chain.registration_features());
        // A validator node activates in the epoch after it registers at the earliest
        let activation_epoch = chain.activation_epoch().unwrap();
        assert!(activation_epoch > chain.current_epoch());
        let err = chain.check_exit(&chain.exit_output(activation_epoch)).unwrap_err();
        // The validator node is in the registered set; it is the activation check that rejects the exit
        assert!(
            matches!(&err, ValidationError::ValidatorNodeNotRegistered { details, .. } if details.contains("only activates")),
            "{err}"
        );
    }

    #[test]
    fn it_rejects_an_exit_with_the_wrong_activation_epoch() {
        let mut chain = Chain::new();
        let (_, activation_epoch) = chain.register_and_activate();
        let wrong_epoch = activation_epoch.saturating_add(VnEpoch(1));
        let err = chain.check_exit(&chain.exit_output(wrong_epoch)).unwrap_err();
        assert!(
            matches!(err, ValidationError::ValidatorNodeExitActivationEpochMismatch { .. }),
            "{err}"
        );
    }

    #[test]
    fn it_rejects_an_exit_for_a_validator_node_already_in_the_exit_queue() {
        let mut chain = Chain::new();
        let (_, activation_epoch) = chain.register_and_activate();
        chain.mine(chain.exit_features(activation_epoch));

        // The validator is queued to exit in a future epoch, so it still counts as active (this keeps the
        // registration UTXO locked) ...
        assert!(
            chain
                .db
                .db_read_access()
                .unwrap()
                .validator_node_is_active(None, chain.current_epoch(), &chain.public_key)
                .unwrap()
        );
        // ... but a second exit would fail at commit time (it is no longer in the registered set), so it must be
        // rejected at validation.
        let err = chain.check_exit(&chain.exit_output(activation_epoch)).unwrap_err();
        assert!(
            matches!(err, ValidationError::ValidatorNodeNotRegistered { .. }),
            "{err}"
        );
    }

    #[test]
    fn it_rejects_an_exit_when_exits_are_not_permitted() {
        let mut chain = Chain::new();
        let (_, activation_epoch) = chain.register_and_activate();
        // Public networks allow zero exits per epoch. With zero, `get_next_exit_epoch` errors at commit time.
        let constants = ConsensusConstantsBuilder::new(Network::MainNet).build();
        assert_eq!(constants.vn_registration_max_exits_per_epoch(), 0);
        let err = check_validator_node_exit(
            &*chain.db.db_read_access().unwrap(),
            &constants,
            &chain.exit_output(activation_epoch),
            chain.current_epoch(),
        )
        .unwrap_err();
        assert!(matches!(err, ValidationError::ValidatorNodeExitNotPermitted), "{err}");
    }

    #[test]
    fn it_rejects_a_registration_while_an_exit_is_pending() {
        // register -> exit -> register: while the exit is queued for a future epoch, a re-registration would let a
        // second exit collide with the queued entry at commit time.
        let mut chain = Chain::new();
        let (_, activation_epoch) = chain.register_and_activate();
        chain.mine(chain.exit_features(activation_epoch));
        assert_eq!(chain.activation_epoch(), None);

        let err = chain.check_registration(&chain.registration_output()).unwrap_err();
        assert!(
            matches!(err, ValidationError::ValidatorNodeAlreadyRegistered { .. }),
            "{err}"
        );
    }

    #[test]
    fn it_allows_register_exit_register_exit_once_the_exit_has_taken_effect() {
        let mut chain = Chain::new();
        let (_, activation_epoch) = chain.register_and_activate();
        chain.mine(chain.exit_features(activation_epoch));
        chain.mine_epochs(2);

        // The exit has taken effect: the validator node may register again ...
        chain.check_registration(&chain.registration_output()).unwrap();
        chain.mine(chain.registration_features());
        chain.mine_epochs(2);
        let second_activation_epoch = chain.activation_epoch().unwrap();
        assert!(second_activation_epoch > activation_epoch);

        // ... and exit again: an exit bound to the first instance is rejected, one bound to the second is accepted
        // and commits.
        let err = chain.check_exit(&chain.exit_output(activation_epoch)).unwrap_err();
        assert!(
            matches!(err, ValidationError::ValidatorNodeExitActivationEpochMismatch { .. }),
            "{err}"
        );
        chain.check_exit(&chain.exit_output(second_activation_epoch)).unwrap();
        chain.mine(chain.exit_features(second_activation_epoch));
        assert_eq!(chain.activation_epoch(), None);
    }

    #[test]
    fn the_mempool_evaluates_validator_node_epochs_at_the_next_block_height() {
        // The tip is the last block of an epoch, so the next block (the earliest a mempool transaction can be mined
        // in) starts the following epoch. A registration whose max_epoch is the tip's epoch would be valid at the tip
        // but can never be mined, so the mempool must reject it.
        let chain = Chain::new();
        let epoch_length = chain.db.consensus_constants().unwrap().epoch_length();
        let tip_height = chain.db.fetch_last_header().unwrap().height;
        add_many_chained_blocks(
            usize::try_from(epoch_length - 1 - tip_height).unwrap(),
            &chain.db,
            &chain.key_manager,
        );
        let tip = chain.db.fetch_tip_header().unwrap();
        let tip_epoch = chain
            .db
            .consensus_constants()
            .unwrap()
            .block_height_to_epoch(tip.height());
        assert_eq!(chain.current_epoch(), tip_epoch.saturating_add(VnEpoch(1)));

        let registration = |max_epoch: VnEpoch| {
            let signature = ValidatorNodeSignature::sign_for_registration(
                &chain.sk,
                NETWORK.as_byte(),
                None,
                &chain.public_key,
                max_epoch,
            );
            let output = TransactionOutput {
                features: OutputFeatures::for_validator_node_registration(
                    signature,
                    chain.public_key.clone(),
                    None,
                    max_epoch,
                ),
                ..Default::default()
            };
            AggregateBody::new_unsorted(vec![], vec![output], vec![])
        };
        let validator = AggregateBodyChainLinkedValidator::new(chain.db.rules().clone());
        let db = chain.db.db_read_access().unwrap();

        let err = validator
            .validate_transaction_body(&registration(tip_epoch), tip.header(), &*db)
            .unwrap_err();
        assert!(
            matches!(err, ValidationError::ValidatorNodeRegistrationMaxEpoch { .. }),
            "{err}"
        );
        validator
            .validate_transaction_body(&registration(tip_epoch.saturating_add(VnEpoch(1))), tip.header(), &*db)
            .unwrap();
    }

    #[test]
    fn rewinding_an_exit_and_the_registration_restores_and_then_removes_the_entry() {
        let mut chain = Chain::new();
        let height_before = chain.tip_height();
        let (registration, activation_epoch) = chain.register_and_activate();
        let height_activated = chain.tip_height();
        chain.mine(chain.exit_features(activation_epoch));
        assert_eq!(chain.registered_commitment(), None);

        chain.db.rewind_to_height(height_activated).unwrap();
        assert_eq!(chain.registered_commitment(), Some(registration.commitment.clone()));
        chain.db.rewind_to_height(height_before).unwrap();
        assert_eq!(chain.registered_commitment(), None);
    }

    fn assert_spend_disallowed(result: Result<(), ValidationError>) {
        let err = result.unwrap_err();
        assert!(matches!(err, ValidationError::OutputSpendRuleDisallow { .. }), "{err}");
    }

    #[test]
    fn a_registration_cannot_be_spent_until_its_exit_has_taken_effect() {
        // Unmined (e.g. in the same body or still in the mempool): rejected
        let mut chain = Chain::new();
        assert_spend_disallowed(chain.check_spend(&chain.registration_output()));

        // Pending activation: rejected (a pending registration cannot be cancelled by spending it)
        let (registration, wallet_output) = chain.mine_spendable(chain.registration_features());
        assert!(chain.activation_epoch().unwrap() > chain.current_epoch());
        assert_spend_disallowed(chain.check_spend(&registration));

        // Active: rejected
        chain.mine_epochs(2);
        let activation_epoch = chain.activation_epoch().unwrap();
        assert!(activation_epoch <= chain.current_epoch());
        assert_spend_disallowed(chain.check_spend(&registration));

        // Exit queued for a later epoch: still rejected
        chain.mine(chain.exit_features(activation_epoch));
        assert_spend_disallowed(chain.check_spend(&registration));

        // Exit taken effect: the stake can be reclaimed, and the spend commits without touching the validator node set
        chain.mine_epochs(2);
        chain.check_spend(&registration).unwrap();
        chain.spend(wallet_output);
        assert_eq!(chain.activation_epoch(), None);
    }

    #[test]
    fn spending_an_old_registration_does_not_touch_a_newer_one() {
        // register R1 -> exit -> (exit takes effect) -> register R2 -> spend R1: the registered set is keyed by public
        // key only, but R2's entry was created by a different output, so R1 is spendable and R2 is untouched (and
        // locked).
        let mut chain = Chain::new();
        let (r1_output, r1) = chain.mine_spendable(chain.registration_features());
        chain.mine_epochs(2);
        let activation_epoch = chain.activation_epoch().unwrap();
        chain.mine(chain.exit_features(activation_epoch));
        chain.mine_epochs(2);
        let r2_output = chain.mine(chain.registration_features());
        assert_eq!(chain.registered_commitment(), Some(r2_output.commitment.clone()));
        // R1 is spendable while R2 is pending ...
        chain.check_spend(&r1_output).unwrap();

        // ... and stays spendable once R2 is active: the lock is specific to the registration instance, so a later
        // registration of the same validator node (e.g. a third party replaying the old registration) cannot re-lock
        // R1's stake.
        chain.mine_epochs(2);
        assert!(chain.activation_epoch().unwrap() <= chain.current_epoch());
        assert!(
            chain
                .db
                .db_read_access()
                .unwrap()
                .validator_node_is_active(None, chain.current_epoch(), &chain.public_key)
                .unwrap()
        );
        chain.check_spend(&r1_output).unwrap();
        chain.spend(r1);
        assert_eq!(chain.registered_commitment(), Some(r2_output.commitment.clone()));
        assert_spend_disallowed(chain.check_spend(&r2_output));
    }

    #[test]
    fn rewinding_a_registration_removes_its_entry() {
        let mut chain = Chain::new();
        let height_before = chain.tip_height();
        let registration = chain.mine(chain.registration_features());
        assert_eq!(chain.registered_commitment(), Some(registration.commitment));
        chain.db.rewind_to_height(height_before).unwrap();
        assert_eq!(chain.registered_commitment(), None);
    }

    #[test]
    fn rewinding_an_exit_spent_in_the_same_block_restores_the_validator_node() {
        // The exit output is also spent in the block that mines it. Applying the block ran the exit for it, so
        // rewinding the block must undo the exit too (it used to be skipped as an "immediate spend").
        let mut chain = Chain::new();
        let (_, activation_epoch) = chain.register_and_activate();
        let height_activated = chain.tip_height();
        let entry_before = chain
            .db
            .db_read_access()
            .unwrap()
            .fetch_validator_node_entry(None, &chain.public_key)
            .unwrap()
            .unwrap();

        let from = chain.spendable.pop().unwrap();
        let (mut txs, outputs) = schema_to_transaction(
            &[txn_schema!(from: vec![from], to: vec![T], features: chain.exit_features(activation_epoch))],
            &chain.key_manager,
        );
        let exit = outputs
            .into_iter()
            .find(|o| o.features().sidechain_feature.is_some())
            .unwrap();
        let (spend, _) = schema_to_transaction(
            &[txn_schema!(from: vec![exit], to: vec![MicroMinotari(500_000)])],
            &chain.key_manager,
        );
        txs.extend(spend);
        chain.mine_txs(txs);
        assert_eq!(chain.registered_commitment(), None);

        chain.db.rewind_to_height(height_activated).unwrap();
        let entry_after = chain
            .db
            .db_read_access()
            .unwrap()
            .fetch_validator_node_entry(None, &chain.public_key)
            .unwrap()
            .unwrap();
        assert_eq!(entry_after.commitment, entry_before.commitment);
        assert_eq!(entry_after.activation_epoch, entry_before.activation_epoch);
        assert_eq!(entry_after.registration_epoch, entry_before.registration_epoch);
        assert_eq!(entry_after.shard_key, entry_before.shard_key);
        // And it can exit again
        chain.check_exit(&chain.exit_output(activation_epoch)).unwrap();
    }

    #[test]
    fn the_mempool_evaluates_the_registration_spend_lock_at_the_next_block_height() {
        // The validator node is queued to exit at the start of the next epoch, and the tip is the last block of the
        // current one. At the tip the validator node still counts as active, but in the next block (the earliest the
        // spend can be mined in) its exit has taken effect, so the mempool accepts the spend.
        let mut chain = Chain::new();
        let (registration, activation_epoch) = chain.register_and_activate();
        chain.mine(chain.exit_features(activation_epoch));
        let epoch_length = chain.db.consensus_constants().unwrap().epoch_length();
        let tip_height = chain.tip_height();
        let blocks_to_epoch_end = epoch_length - 1 - tip_height % epoch_length;
        add_many_chained_blocks(
            usize::try_from(blocks_to_epoch_end).unwrap(),
            &chain.db,
            &chain.key_manager,
        );
        let tip_epoch = chain
            .db
            .consensus_constants()
            .unwrap()
            .block_height_to_epoch(chain.tip_height());
        assert!(
            chain
                .db
                .db_read_access()
                .unwrap()
                .validator_node_is_active(None, tip_epoch, &chain.public_key)
                .unwrap()
        );
        assert!(
            !chain
                .db
                .db_read_access()
                .unwrap()
                .validator_node_is_active(None, chain.current_epoch(), &chain.public_key)
                .unwrap()
        );
        chain.check_spend(&registration).unwrap();
    }

    #[test]
    fn it_still_accepts_a_registration_replay_after_exit() {
        // KNOWN RESIDUAL (accepted): once a validator node's exit has taken effect, the original registration (same
        // signature, still within its max_epoch) can be resubmitted by anyone in a new output and is accepted. The
        // registration signature covers the network, sidechain, claim key and max_epoch, but nothing that ties it to
        // a single use. A short max_epoch is the mitigation. An exit, by contrast, is bound to the registration
        // instance through its activation_epoch.
        let mut chain = Chain::new();
        let (registration, activation_epoch) = chain.register_and_activate();
        chain.mine(chain.exit_features(activation_epoch));
        chain.mine_epochs(2);
        chain.check_registration(&registration).unwrap();
    }
}
