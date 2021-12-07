//  Copyright 2020, The Tari Project
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

use crate::{
    blocks::{Block, BlockHeader, BlockHeaderAccumulatedData, ChainHeader, NewBlockTemplate},
    chain_storage::{BlockchainDatabase, ChainStorageError},
    consensus::ConsensusManager,
    crypto::tari_utilities::hex::Hex,
    proof_of_work::{AchievedTargetDifficulty, Difficulty, PowAlgorithm},
    tari_utilities::Hashable,
    test_helpers::{
        blockchain::{create_new_blockchain, TempDatabase},
        create_block,
        BlockSpec,
    },
    transactions::{
        tari_amount::T,
        test_helpers::{schema_to_transaction, TransactionSchema},
        transaction_entities::{
            output_features::OutputFeatures,
            transaction::Transaction,
            unblinded_output::UnblindedOutput,
        },
    },
    txn_schema,
};
use std::sync::Arc;
use tari_common::configuration::Network;
use tari_test_utils::unpack_enum;

fn setup() -> BlockchainDatabase<TempDatabase> {
    create_new_blockchain()
}

fn create_next_block(prev_block: &Block, transactions: Vec<Arc<Transaction>>) -> (Arc<Block>, UnblindedOutput) {
    let rules = ConsensusManager::builder(Network::LocalNet).build();
    let (block, output) = create_block(
        &rules,
        prev_block,
        BlockSpec::new()
            .with_transactions(transactions.into_iter().map(|t| (&*t).clone()).collect())
            .finish(),
    );
    (Arc::new(block), output)
}

fn add_many_chained_blocks(
    size: usize,
    db: &BlockchainDatabase<TempDatabase>,
) -> (Vec<Arc<Block>>, Vec<UnblindedOutput>) {
    let last_header = db.fetch_last_header().unwrap();
    let mut prev_block = db
        .fetch_block(last_header.height)
        .unwrap()
        .try_into_block()
        .map(Arc::new)
        .unwrap();
    let mut blocks = Vec::with_capacity(size);
    let mut outputs = Vec::with_capacity(size);
    for _ in 1..=size as u64 {
        let (block, coinbase_utxo) = create_next_block(&prev_block, vec![]);
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
        let blocks = db.fetch_blocks(0..).unwrap();
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn it_returns_all() {
        let db = setup();
        add_many_chained_blocks(4, &db);
        let blocks = db.fetch_blocks(..).unwrap();
        assert_eq!(blocks.len(), 5);
        for (i, item) in blocks.iter().enumerate().take(4 + 1) {
            assert_eq!(item.header().height, i as u64);
        }
    }

    #[test]
    fn it_returns_one() {
        let db = setup();
        let (new_blocks, _) = add_many_chained_blocks(1, &db);
        let blocks = db.fetch_blocks(1..=1).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block().hash(), new_blocks[0].hash());
    }

    #[test]
    fn it_returns_nothing_if_asking_for_blocks_out_of_range() {
        let db = setup();
        add_many_chained_blocks(1, &db);
        let blocks = db.fetch_blocks(2..).unwrap();
        assert!(blocks.is_empty());
    }

    #[test]
    fn it_returns_blocks_between_bounds_exclusive() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let blocks = db.fetch_blocks(3..5).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].header().height, 3);
        assert_eq!(blocks[1].header().height, 4);
    }

    #[test]
    fn it_returns_blocks_between_bounds_inclusive() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let blocks = db.fetch_blocks(3..=5).unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].header().height, 3);
        assert_eq!(blocks[1].header().height, 4);
        assert_eq!(blocks[2].header().height, 5);
    }

    #[test]
    fn it_returns_blocks_to_the_tip() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let blocks = db.fetch_blocks(3..).unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].header().height, 3);
        assert_eq!(blocks[1].header().height, 4);
        assert_eq!(blocks[2].header().height, 5);
    }

    #[test]
    fn it_returns_blocks_from_genesis() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let blocks = db.fetch_blocks(..=3).unwrap();
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

    #[test]
    fn it_returns_all() {
        let db = setup();
        add_many_chained_blocks(4, &db);
        let headers = db.fetch_headers(..).unwrap();
        assert_eq!(headers.len(), 5);
        for (i, item) in headers.iter().enumerate().take(4 + 1) {
            assert_eq!(item.height, i as u64);
        }
    }

    #[test]
    fn it_returns_nothing_if_asking_for_blocks_out_of_range() {
        let db = setup();
        add_many_chained_blocks(1, &db);
        let headers = db.fetch_headers(2..).unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn it_returns_blocks_between_bounds_exclusive() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let headers = db.fetch_headers(3..5).unwrap();
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].height, 3);
        assert_eq!(headers[1].height, 4);
    }

    #[test]
    fn it_returns_blocks_between_bounds_inclusive() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let headers = db.fetch_headers(3..=5).unwrap();
        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0].height, 3);
        assert_eq!(headers[1].height, 4);
        assert_eq!(headers[2].height, 5);
    }

    #[test]
    fn it_returns_blocks_to_the_tip() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let headers = db.fetch_headers(3..).unwrap();
        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0].height, 3);
        assert_eq!(headers[1].height, 4);
        assert_eq!(headers[2].height, 5);
    }

    #[test]
    fn it_returns_blocks_from_genesis() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let headers = db.fetch_headers(..=3).unwrap();
        assert_eq!(headers.len(), 4);
        assert_eq!(headers[0].height, 0);
        assert_eq!(headers[1].height, 1);
        assert_eq!(headers[2].height, 2);
        assert_eq!(headers[3].height, 3);
    }
}

mod find_headers_after_hash {
    use crate::chain_storage::ChainStorageError;

    use super::*;

    #[test]
    fn it_returns_none_given_empty_vec() {
        let db = setup();
        let hashes = vec![];
        assert!(db.find_headers_after_hash(hashes, 1).unwrap().is_none());
    }

    #[test]
    fn it_returns_from_genesis() {
        let db = setup();
        let genesis_hash = db.fetch_block(0).unwrap().block().hash();
        add_many_chained_blocks(1, &db);
        let hashes = vec![genesis_hash.clone()];
        let (index, headers) = db.find_headers_after_hash(hashes, 1).unwrap().unwrap();
        assert_eq!(index, 0);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].prev_hash, genesis_hash);
    }

    #[test]
    fn it_returns_the_first_headers_found() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let hashes = (1..=3)
            .rev()
            .map(|i| db.fetch_block(i).unwrap().block().hash())
            .collect::<Vec<_>>();
        let (index, headers) = db.find_headers_after_hash(hashes, 10).unwrap().unwrap();
        assert_eq!(index, 0);
        assert_eq!(headers.len(), 2);
        assert_eq!(&headers[0], db.fetch_block(4).unwrap().header());
    }

    #[test]
    fn it_ignores_unknown_hashes() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let hashes = (2..=4)
            .map(|i| db.fetch_block(i).unwrap().block().hash())
            .chain(vec![vec![0; 32], vec![0; 32]])
            .rev();
        let (index, headers) = db.find_headers_after_hash(hashes, 1).unwrap().unwrap();
        assert_eq!(index, 2);
        assert_eq!(headers.len(), 1);
        assert_eq!(&headers[0], db.fetch_block(5).unwrap().header());
    }

    #[test]
    fn it_errors_for_hashes_with_an_invalid_length() {
        let db = setup();
        let err = db.find_headers_after_hash(vec![vec![]], 1).unwrap_err();
        unpack_enum!(ChainStorageError::InvalidArguments { .. } = err);
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

    #[test]
    fn it_returns_empty_set_for_big_offset() {
        let db = setup();
        add_many_chained_blocks(5, &db);
        let hashes = db.fetch_block_hashes_from_header_tip(3, 6).unwrap();
        assert!(hashes.is_empty());
    }

    #[test]
    fn it_returns_n_hashes_from_tip() {
        let db = setup();
        let (blocks, _) = add_many_chained_blocks(5, &db);
        let hashes = db.fetch_block_hashes_from_header_tip(3, 1).unwrap();
        assert_eq!(hashes.len(), 3);
        assert_eq!(hashes[0], blocks[3].hash());
        assert_eq!(hashes[1], blocks[2].hash());
        assert_eq!(hashes[2], blocks[1].hash());
    }

    #[test]
    fn it_returns_hashes_without_overlapping() {
        let db = setup();
        let (blocks, _) = add_many_chained_blocks(3, &db);
        let hashes = db.fetch_block_hashes_from_header_tip(2, 0).unwrap();
        assert_eq!(hashes[0], blocks[2].hash());
        assert_eq!(hashes[1], blocks[1].hash());
        let hashes = db.fetch_block_hashes_from_header_tip(1, 2).unwrap();
        assert_eq!(hashes[0], blocks[0].hash());
    }

    #[test]
    fn it_returns_all_hashes_from_tip() {
        let db = setup();
        let genesis = db.fetch_tip_header().unwrap();
        let (blocks, _) = add_many_chained_blocks(5, &db);
        let hashes = db.fetch_block_hashes_from_header_tip(10, 0).unwrap();
        assert_eq!(hashes.len(), 6);
        assert_eq!(hashes[0], blocks[4].hash());
        assert_eq!(&hashes[5], genesis.hash());
    }
}

mod add_block {
    use super::*;

    #[test]
    fn it_does_not_allow_duplicate_commitments_in_the_utxo_set() {
        let db = setup();
        let (blocks, outputs) = add_many_chained_blocks(5, &db);

        let prev_block = blocks.last().unwrap();

        let (txns, tx_outputs) =
            schema_to_transaction(&[txn_schema!(from: vec![outputs[0].clone()], to: vec![500 * T])]);
        let mut prev_utxo = tx_outputs[0].clone();

        let (block, _) = create_next_block(prev_block, txns);
        db.add_block(block.clone()).unwrap().assert_added();

        let prev_block = block;
        let prev_output = prev_utxo.as_transaction_output(&Default::default()).unwrap();

        let (txns, _) = schema_to_transaction(&[TransactionSchema {
            from: vec![outputs[1].clone()],
            to: vec![],
            to_outputs: vec![prev_utxo.clone()],
            fee: 5.into(),
            lock_height: 0,
            features: Default::default(),
            script: tari_crypto::script![Nop],
            input_data: None,
        }]);

        let (block, _) = create_next_block(&prev_block, txns);
        let err = db.add_block(block.clone()).unwrap_err();
        unpack_enum!(ChainStorageError::KeyExists { key, table_name } = err);
        assert_eq!(table_name, "utxo_commitment_index");
        assert_eq!(key, prev_output.commitment.to_hex());
        // Check rollback
        let header = db.fetch_header(block.header.height).unwrap();
        assert!(header.is_none());

        let (txns, _) = schema_to_transaction(&[txn_schema!(from: vec![prev_utxo.clone()], to: vec![50 * T])]);
        let (block, _) = create_next_block(&prev_block, txns);
        let block = db.add_block(block).unwrap().assert_added();
        let prev_block = block.to_arc_block();

        // Different maturity so that the output hash is different in txo_hash_to_index_db
        prev_utxo.features = OutputFeatures::with_maturity(1);
        // Now we can reuse a commitment
        let (txns, _) = schema_to_transaction(&[TransactionSchema {
            from: vec![outputs[1].clone()],
            to: vec![],
            to_outputs: vec![prev_utxo],
            fee: 5.into(),
            lock_height: 0,
            features: Default::default(),
            script: tari_crypto::script![Nop],
            input_data: None,
        }]);

        let (block, _) = create_next_block(&prev_block, txns);
        db.add_block(block).unwrap().assert_added();
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

    #[test]
    fn it_measures_the_number_of_entries() {
        let db = setup();
        let _ = add_many_chained_blocks(2, &db);
        let stats = db.fetch_total_size_stats().unwrap();
        assert_eq!(
            stats.sizes().iter().find(|s| s.name == "utxos_db").unwrap().num_entries,
            4003
        );
    }
}

mod prepare_new_block {
    use super::*;

    #[test]
    fn it_errors_for_genesis_block() {
        let db = setup();
        let genesis = db.fetch_block(0).unwrap();
        let template = NewBlockTemplate::from_block(genesis.block().clone(), Difficulty::min(), 5000 * T);
        let err = db.prepare_new_block(template).unwrap_err();
        assert!(matches!(err, ChainStorageError::InvalidArguments { .. }));
    }

    #[test]
    fn it_errors_for_non_tip_template() {
        let db = setup();
        let genesis = db.fetch_block(0).unwrap();
        let next_block = BlockHeader::from_previous(genesis.header());
        let mut template = NewBlockTemplate::from_block(next_block.into_builder().build(), Difficulty::min(), 5000 * T);
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
        let genesis = db.fetch_block(0).unwrap();
        let next_block = BlockHeader::from_previous(genesis.header());
        let template = NewBlockTemplate::from_block(next_block.into_builder().build(), Difficulty::min(), 5000 * T);
        let block = db.prepare_new_block(template).unwrap();
        assert_eq!(block.header.height, 1);
    }
}

mod fetch_header_containing_utxo_mmr {
    use super::*;

    #[test]
    fn it_returns_genesis() {
        let db = setup();
        let genesis = db.fetch_block(0).unwrap();
        assert_eq!(genesis.block().body.outputs().len(), 4001);
        let mut mmr_position = 0;
        genesis.block().body.outputs().iter().for_each(|_| {
            let header = db.fetch_header_containing_utxo_mmr(mmr_position).unwrap();
            assert_eq!(header.height(), 0);
            mmr_position += 1;
        });
        let err = db.fetch_header_containing_utxo_mmr(4002).unwrap_err();
        matches!(err, ChainStorageError::ValueNotFound { .. });
    }

    #[test]
    fn it_returns_corresponding_header() {
        let db = setup();
        let genesis = db.fetch_block(0).unwrap();
        let _ = add_many_chained_blocks(5, &db);
        let num_genesis_outputs = genesis.block().body.outputs().len() as u64;

        for i in 1..=5 {
            let header = db.fetch_header_containing_utxo_mmr(num_genesis_outputs + i).unwrap();
            assert_eq!(header.height(), i);
        }
        let err = db
            .fetch_header_containing_utxo_mmr(num_genesis_outputs + 5 + 1)
            .unwrap_err();
        matches!(err, ChainStorageError::ValueNotFound { .. });
    }
}

mod fetch_header_containing_kernel_mmr {
    use super::*;

    #[test]
    fn it_returns_genesis() {
        let db = setup();
        let genesis = db.fetch_block(0).unwrap();
        assert_eq!(genesis.block().body.kernels().len(), 2);
        let mut mmr_position = 0;
        genesis.block().body.kernels().iter().for_each(|_| {
            let header = db.fetch_header_containing_kernel_mmr(mmr_position).unwrap();
            assert_eq!(header.height(), 0);
            mmr_position += 1;
        });
        let err = db.fetch_header_containing_kernel_mmr(3).unwrap_err();
        matches!(err, ChainStorageError::ValueNotFound { .. });
    }

    #[test]
    fn it_returns_corresponding_header() {
        let db = setup();
        let genesis = db.fetch_block(0).unwrap();
        let (blocks, outputs) = add_many_chained_blocks(1, &db);
        let num_genesis_kernels = genesis.block().body.kernels().len() as u64;
        let (txns, _) = schema_to_transaction(&[txn_schema!(from: vec![outputs[0].clone()], to: vec![50 * T])]);

        let (block, _) = create_next_block(&blocks[0], txns);
        db.add_block(block).unwrap();
        let _ = add_many_chained_blocks(3, &db);

        let header = db.fetch_header_containing_kernel_mmr(num_genesis_kernels).unwrap();
        assert_eq!(header.height(), 0);
        let header = db.fetch_header_containing_kernel_mmr(num_genesis_kernels + 1).unwrap();
        assert_eq!(header.height(), 1);

        for i in 2..=3 {
            let header = db.fetch_header_containing_kernel_mmr(num_genesis_kernels + i).unwrap();
            assert_eq!(header.height(), 2);
        }
        for i in 4..=6 {
            let header = db.fetch_header_containing_kernel_mmr(num_genesis_kernels + i).unwrap();
            assert_eq!(header.height(), i - 1);
        }

        let err = db
            .fetch_header_containing_kernel_mmr(num_genesis_kernels + 6 + 1)
            .unwrap_err();
        matches!(err, ChainStorageError::ValueNotFound { .. });
    }
}

mod clear_all_pending_headers {
    use super::*;

    #[test]
    fn it_clears_no_headers() {
        let db = setup();
        assert_eq!(db.clear_all_pending_headers().unwrap(), 0);
        let _ = add_many_chained_blocks(2, &db);
        db.clear_all_pending_headers().unwrap();
        let last_header = db.fetch_last_header().unwrap();
        assert_eq!(last_header.height, 2);
    }

    #[test]
    fn it_clears_headers_after_tip() {
        let db = setup();
        let _ = add_many_chained_blocks(2, &db);
        let prev_block = db.fetch_block(2).unwrap();
        let mut prev_accum = prev_block.accumulated_data.clone();
        let mut prev_block = Arc::new(prev_block.try_into_block().unwrap());
        let headers = (0..5)
            .map(|_| {
                let (block, _) = create_next_block(&prev_block, vec![]);
                let accum = BlockHeaderAccumulatedData::builder(&prev_accum)
                    .with_hash(block.hash())
                    .with_achieved_target_difficulty(
                        AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3, 0.into(), 0.into()).unwrap(),
                    )
                    .with_total_kernel_offset(Default::default())
                    .build()
                    .unwrap();

                let header = ChainHeader::try_construct(block.header.clone(), accum.clone()).unwrap();

                prev_block = block;
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
