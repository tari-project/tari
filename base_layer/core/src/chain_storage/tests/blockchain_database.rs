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
    blocks::Block,
    chain_storage::BlockchainDatabase,
    tari_utilities::Hashable,
    test_helpers::{
        blockchain::{create_new_blockchain, TempDatabase},
        create_block,
    },
};
use std::sync::Arc;
use tari_test_utils::unpack_enum;

fn setup() -> BlockchainDatabase<TempDatabase> {
    create_new_blockchain()
}

fn add_many_chained_blocks(size: usize, db: &BlockchainDatabase<TempDatabase>) -> Vec<Arc<Block>> {
    let mut prev_block = Arc::new(db.fetch_block(0).unwrap().try_into_block().unwrap());
    let mut blocks = Vec::with_capacity(size);
    for i in 1..=size as u64 {
        let mut block = create_block(1, i, vec![]);
        block.header.prev_hash = prev_block.hash().clone();
        block.header.output_mmr_size = prev_block.header.output_mmr_size + block.body.outputs().len() as u64;
        block.header.kernel_mmr_size = prev_block.header.kernel_mmr_size + block.body.kernels().len() as u64;
        let block = Arc::new(block);
        prev_block = block.clone();
        db.add_block(block.clone()).unwrap().assert_added();
        blocks.push(block);
    }
    blocks
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
        let new_blocks = add_many_chained_blocks(1, &db);
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
    use super::*;
    use crate::chain_storage::ChainStorageError;

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
        unpack_enum!(ChainStorageError::InvalidArguments {..} = err);
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
        let blocks = add_many_chained_blocks(5, &db);
        let hashes = db.fetch_block_hashes_from_header_tip(3, 1).unwrap();
        assert_eq!(hashes.len(), 3);
        assert_eq!(hashes[0], blocks[3].hash());
        assert_eq!(hashes[1], blocks[2].hash());
        assert_eq!(hashes[2], blocks[1].hash());
    }

    #[test]
    fn it_returns_hashes_without_overlapping() {
        let db = setup();
        let blocks = add_many_chained_blocks(3, &db);
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
        let blocks = add_many_chained_blocks(5, &db);
        let hashes = db.fetch_block_hashes_from_header_tip(10, 0).unwrap();
        assert_eq!(hashes.len(), 6);
        assert_eq!(hashes[0], blocks[4].hash());
        assert_eq!(&hashes[5], genesis.hash());
    }
}
