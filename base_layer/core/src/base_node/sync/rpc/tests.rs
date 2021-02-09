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

use super::BaseNodeSyncRpcService;
use crate::{
    blocks::{Block, BlockBuilder, BlockHeader},
    chain_storage::{BlockchainDatabase, ChainMetadata},
    test_helpers::{
        blockchain::{create_mock_blockchain_database, MockBlockchainBackend},
        create_peer_manager,
    },
};
use std::iter;
use tari_comms::protocol::rpc::mock::RpcRequestMock;
use tempfile::{tempdir, TempDir};

fn setup(
    backend: MockBlockchainBackend,
) -> (
    BaseNodeSyncRpcService<MockBlockchainBackend>,
    BlockchainDatabase<MockBlockchainBackend>,
    RpcRequestMock,
    TempDir,
) {
    let tmp = tempdir().unwrap();
    let peer_manager = create_peer_manager(&tmp);
    let request_mock = RpcRequestMock::new(peer_manager.clone());

    let db = create_mock_blockchain_database(backend);
    let service = BaseNodeSyncRpcService::new(db.clone().into());
    (service, db, request_mock, tmp)
}

fn create_mock_backend() -> MockBlockchainBackend {
    let mut backend = MockBlockchainBackend::new();
    // Expectations for BlockchainDatabase::new
    backend.expect_is_empty().times(1).returning(|| Ok(false));
    backend
        .expect_fetch_chain_metadata()
        .times(1)
        .returning(|| Ok(ChainMetadata::new(0, Vec::new(), 0, 0, 0)));
    backend
}

fn create_chained_blocks(n: usize) -> Vec<Block> {
    iter::repeat(())
        .take(n)
        .fold(Vec::with_capacity(n), |mut acc, _| match acc.last() {
            Some(prev) => {
                let header = BlockHeader::from_previous(&prev.header).unwrap();
                let block = BlockBuilder::new(0).with_header(header).build();
                acc.push(block);
                acc
            },
            None => vec![BlockBuilder::new(0).build()],
        })
}

mod sync_blocks {
    use super::*;
    use crate::{
        base_node::BaseNodeSyncService,
        blocks::BlockBuilder,
        chain_storage::{ChainMetadata, DbValue},
        proto::base_node::SyncBlocksRequest,
        tari_utilities::Hashable,
    };
    use futures::StreamExt;
    use std::ops::Bound;
    use tari_comms::protocol::rpc::RpcStatusCode;
    use tari_test_utils::unpack_enum;

    #[tokio_macros::test_basic]
    async fn it_returns_not_found_if_unknown_hash() {
        let mut backend = create_mock_backend();
        backend.expect_fetch().times(1).returning(|_| Ok(None));
        let (service, _, rpc_request_mock, _tmp) = setup(backend);
        let msg = SyncBlocksRequest {
            start_hash: vec![],
            end_hash: vec![],
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let err = service.sync_blocks(req).await.unwrap_err();
        unpack_enum!(RpcStatusCode::NotFound = err.status_code());
    }

    #[tokio_macros::test_basic]
    async fn it_sends_an_empty_response() {
        let mut backend = create_mock_backend();

        backend.expect_fetch_chain_metadata().times(1).returning(|| {
            Ok(ChainMetadata::new(
                1,
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ))
        });

        let mut block = BlockBuilder::new(0).build();
        block.header.height = 1;
        // Set both responses to return the same header, the sync rpc handler doesnt care
        let header = block.header.clone();
        backend
            .expect_fetch()
            .times(1)
            .returning(move |_| Ok(Some(DbValue::BlockHash(Box::new(header.clone())))));

        let (service, _, rpc_request_mock, _tmp) = setup(backend);
        let msg = SyncBlocksRequest {
            start_hash: block.hash(),
            end_hash: block.hash(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_blocks(req).await.unwrap();
        assert!(streaming.next().await.is_none());
    }

    #[tokio_macros::test_basic]
    async fn it_streams_blocks_until_end() {
        let mut backend = create_mock_backend();

        let blocks = create_chained_blocks(16);
        let first_block = blocks.first().unwrap();
        let first_hash = first_block.hash();
        let last_block = blocks.last().unwrap();
        let last_hash = last_block.hash();

        let first_header = first_block.header.clone();
        backend
            .expect_fetch()
            .times(1)
            .returning(move |_| Ok(Some(DbValue::BlockHash(Box::new(first_header.clone())))));

        let metadata = ChainMetadata::new(
            20,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        backend
            .expect_fetch_chain_metadata()
            .times(1)
            .returning({    let metadata =metadata.clone(); move || Ok(metadata.clone()));

        let last_header = last_block.header.clone();
        backend
            .expect_fetch()
            .times(1)
            .returning(move |_| Ok(Some(DbValue::BlockHash(Box::new(last_header.clone())))));

        backend
            .expect_fetch_chain_metadata()
            .times(3)
            .returning(move || Ok(metadata.clone()));

        fn expect_fetch_block(backend: &mut MockBlockchainBackend, block: &Block) {
            let header = block.header.clone();
            backend
                .expect_fetch()
                .times(4)
                .returning(move |_| Ok(Some(DbValue::BlockHeader(Box::new(header.clone())))));

            let kernels = block.body.kernels().clone();
            backend
                .expect_fetch_kernels_in_block()
                .times(4)
                .returning(move |_| Ok(kernels.clone()));

            let outputs = block.body.outputs().clone();
            backend
                .expect_fetch_outputs_in_block()
                .times(4)
                .returning(move |_| Ok(outputs.clone()));

            let inputs = block.body.inputs().clone();
            backend
                .expect_fetch_inputs_in_block()
                .times(4)
                .returning(move |_| Ok(inputs.clone()));
        }

        // Now expect 4 blocks to be fetched
        expect_fetch_block(&mut backend, &first_block);
        // expect_fetch_block(&mut backend, &first_block);
        // expect_fetch_block(&mut backend, &first_block);
        // expect_fetch_block(&mut backend, &first_block);

        let (service, _, rpc_request_mock, _tmp) = setup(backend);

        let msg = SyncBlocksRequest {
            start_hash: first_hash,
            end_hash: last_hash,
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let streaming = service.sync_blocks(req).await.unwrap();
        let _ = streaming.map(Result::unwrap).collect::<Vec<_>>().await;

        // assert_eq!(mock.get_call_count("get_blocks").await, 4);
        // let (start, end) = mock
        //     .pop_front_call::<(Bound<u64>, Bound<u64>)>("get_blocks")
        //     .await
        //     .unwrap();
        // unpack_enum!(Bound::Included(start) = start);
        // // Exclude block @ start hash
        // assert_eq!(start, 1);
        // unpack_enum!(Bound::Included(end) = end);
        // assert_eq!(end, 4);
        //
        // let (start, end) = mock
        //     .pop_front_call::<(Bound<u64>, Bound<u64>)>("get_blocks")
        //     .await
        //     .unwrap();
        // unpack_enum!(Bound::Included(start) = start);
        // assert_eq!(start, 5);
        // unpack_enum!(Bound::Included(end) = end);
        // assert_eq!(end, 8);
        //
        // let (start, end) = mock.pop_call::<(Bound<u64>, Bound<u64>)>("get_blocks").await.unwrap();
        // unpack_enum!(Bound::Included(start) = start);
        // assert_eq!(start, 13);
        // unpack_enum!(Bound::Included(end) = end);
        // assert_eq!(end, 15);
    }
}
