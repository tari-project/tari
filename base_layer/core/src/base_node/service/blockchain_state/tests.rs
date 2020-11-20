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
    base_node::service::blockchain_state::{service::BlockchainStateService, BlockchainStateServiceHandle},
    chain_storage::BlockchainDatabase,
    tari_utilities::Hashable,
    test_helpers::{
        blockchain::{create_new_blockchain, create_store, TempDatabase},
        create_block,
    },
    transactions::types::HashDigest,
};
use futures::channel::mpsc;
use tokio::task;

fn setup() -> (BlockchainStateServiceHandle, BlockchainDatabase<TempDatabase>) {
    let db = create_new_blockchain();
    let (request_tx, request_rx) = mpsc::channel(1);
    let service = BlockchainStateService::new(db.clone(), request_rx);
    task::spawn(service.run());
    (BlockchainStateServiceHandle::new(request_tx), db)
}

fn add_many_chained_blocks(size: usize, db: &BlockchainDatabase<TempDatabase>) {
    let mut prev_block_hash = db.fetch_block(0).unwrap().block.hash();
    for i in 1..=size as u64 {
        let mut block = create_block(1, i, vec![]);
        block.header.prev_hash = prev_block_hash.clone();
        prev_block_hash = block.hash();
        db.add_block(block.into()).unwrap();
    }
}

mod get_blocks {
    use super::*;

    #[tokio_macros::test_basic]
    async fn it_returns_genesis() {
        let (mut state_service, _) = setup();
        let blocks = state_service.get_blocks(0..).await.unwrap();
        assert_eq!(blocks.len(), 1);
    }

    #[tokio_macros::test_basic]
    async fn it_returns_all() {
        let (mut state_service, db) = setup();
        add_many_chained_blocks(4, &db);
        let blocks = state_service.get_blocks(..).await.unwrap();
        assert_eq!(blocks.len(), 5);
        for i in 0..=4 {
            assert_eq!(blocks[i].header.height, i as u64);
        }
    }

    #[tokio_macros::test_basic]
    async fn it_returns_nothing_if_asking_for_blocks_out_of_range() {
        let (mut state_service, db) = setup();
        add_many_chained_blocks(1, &db);
        let blocks = state_service.get_blocks(2..).await.unwrap();
        assert!(blocks.is_empty());
    }

    #[tokio_macros::test_basic]
    async fn it_returns_blocks_between_bounds() {
        let (mut state_service, db) = setup();
        add_many_chained_blocks(5, &db);
        let blocks = state_service.get_blocks(3..5).await.unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].header.height, 3);
        assert_eq!(blocks[1].header.height, 4);
        assert_eq!(blocks[2].header.height, 5);
    }

    #[tokio_macros::test_basic]
    async fn it_returns_blocks_to_the_tip() {
        let (mut state_service, db) = setup();
        add_many_chained_blocks(5, &db);
        let blocks = state_service.get_blocks(3..).await.unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].header.height, 3);
        assert_eq!(blocks[1].header.height, 4);
        assert_eq!(blocks[2].header.height, 5);
    }

    #[tokio_macros::test_basic]
    async fn it_returns_blocks_from_genesis() {
        let (mut state_service, db) = setup();
        add_many_chained_blocks(5, &db);
        let blocks = state_service.get_blocks(..3).await.unwrap();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].header.height, 0);
        assert_eq!(blocks[1].header.height, 1);
        assert_eq!(blocks[2].header.height, 2);
        assert_eq!(blocks[3].header.height, 3);
    }
}

mod find_headers_after_hash {
    use super::*;

    #[tokio_macros::test_basic]
    async fn it_returns_none_given_empty_vec() {
        let (mut state_service, _) = setup();
        let hashes = vec![];
        assert!(state_service
            .find_headers_after_hash(hashes, 1)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio_macros::test_basic]
    async fn it_returns_from_genesis() {
        let (mut state_service, db) = setup();
        let genesis_hash = db.fetch_block(0).unwrap().block.hash();
        add_many_chained_blocks(1, &db);
        let hashes = vec![genesis_hash.clone()];
        let (index, headers) = state_service.find_headers_after_hash(hashes, 1).await.unwrap().unwrap();
        assert_eq!(index, 0);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].prev_hash, genesis_hash);
    }

    #[tokio_macros::test_basic]
    async fn it_returns_the_first_headers_found() {
        let (mut state_service, db) = setup();
        add_many_chained_blocks(5, &db);
        let hashes = (1..=3).map(|i| db.fetch_block(i).unwrap().block.hash()).rev();
        let (index, headers) = state_service
            .find_headers_after_hash(hashes, 10)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(index, 0);
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], db.fetch_block(4).unwrap().block.header);
    }

    #[tokio_macros::test_basic]
    async fn it_ignores_unknown_hashes() {
        let (mut state_service, db) = setup();
        add_many_chained_blocks(5, &db);
        let hashes = (2..=4)
            .map(|i| db.fetch_block(i).unwrap().block.hash())
            .chain(vec![Default::default(), Default::default()])
            .rev();
        let (index, headers) = state_service.find_headers_after_hash(hashes, 1).await.unwrap().unwrap();
        assert_eq!(index, 2);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0], db.fetch_block(5).unwrap().block.header);
    }
}
