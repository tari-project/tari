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
    base_node::service::blockchain_state::{create_blockchain_state_service_mock, BlockchainStateMockState},
    blocks::{Block, BlockBuilder, BlockHeader},
    test_helpers::create_peer_manager,
};
use std::iter;
use tari_comms::protocol::rpc::mock::RpcRequestMock;
use tempfile::{tempdir, TempDir};

fn setup() -> (
    BaseNodeSyncRpcService,
    BlockchainStateMockState,
    RpcRequestMock,
    TempDir,
) {
    let tmp = tempdir().unwrap();
    let peer_manager = create_peer_manager(&tmp);
    let request_mock = RpcRequestMock::new(peer_manager.clone());
    let (handle, state) = create_blockchain_state_service_mock();
    let service = BaseNodeSyncRpcService::new(handle);
    (service, state, request_mock, tmp)
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
        proto::generated::base_node::SyncBlocksRequest,
        tari_utilities::Hashable,
    };
    use futures::StreamExt;
    use std::ops::Bound;
    use tari_common_types::chain_metadata::ChainMetadata;
    use tari_comms::protocol::rpc::RpcStatusCode;
    use tari_test_utils::unpack_enum;

    #[tokio_macros::test_basic]
    async fn it_returns_not_found_if_unknown_hash() {
        let (service, mock, rpc_request_mock, _tmp) = setup();
        mock.set_get_block_header_by_hash(None).await;
        let msg = SyncBlocksRequest {
            start_hash: vec![],
            count: 1,
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let err = service.sync_blocks(req).await.unwrap_err();
        unpack_enum!(RpcStatusCode::NotFound = err.status_code());
    }

    #[tokio_macros::test_basic]
    async fn it_sends_an_empty_response() {
        let (service, mock, rpc_request_mock, _tmp) = setup();
        let block = BlockBuilder::new(0).build();
        mock.set_get_block_header_by_hash(Some(block.header.clone())).await;
        mock.set_get_blocks_response(vec![]).await;
        let msg = SyncBlocksRequest {
            start_hash: block.hash(),
            count: 1,
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_blocks(req).await.unwrap();
        assert!(streaming.next().await.is_none());
    }

    #[tokio_macros::test_basic]
    async fn it_streams_blocks_until_count() {
        let (service, mock, rpc_request_mock, _tmp) = setup();
        let metadata = ChainMetadata::new(20, Vec::new(), 0, 0, 0);
        mock.set_get_chain_metadata_response(metadata).await;
        let blocks = create_chained_blocks(10);
        let first_hash = blocks[0].hash();
        mock.set_get_block_header_by_hash(Some(blocks[0].header.clone())).await;
        mock.set_get_blocks_response(blocks).await;

        let msg = SyncBlocksRequest {
            start_hash: first_hash,
            count: 15,
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let streaming = service.sync_blocks(req).await.unwrap();
        let blocks = streaming.map(Result::unwrap).collect::<Vec<_>>().await;
        // Since we have a mocked out service backend, this will always be 10 * 2 - we only what to test what the RPC
        // service is passing to the blockchain state backend.
        assert_eq!(blocks.len(), 20);

        assert_eq!(mock.get_call_count("get_blocks").await, 2);
        let (start, end) = mock
            .pop_front_call::<(Bound<u64>, Bound<u64>)>("get_blocks")
            .await
            .unwrap();
        unpack_enum!(Bound::Included(start) = start);
        assert_eq!(start, 0);
        unpack_enum!(Bound::Included(end) = end);
        assert_eq!(end, 9);

        let (start, end) = mock
            .pop_front_call::<(Bound<u64>, Bound<u64>)>("get_blocks")
            .await
            .unwrap();
        unpack_enum!(Bound::Included(start) = start);
        assert_eq!(start, 10);
        unpack_enum!(Bound::Included(end) = end);
        assert_eq!(end, 15);
    }

    #[tokio_macros::test_basic]
    async fn it_streams_blocks_until_the_end() {
        let (service, mock, rpc_request_mock, _tmp) = setup();
        let metadata = ChainMetadata::new(20, Vec::new(), 0, 0, 0);
        mock.set_get_chain_metadata_response(metadata).await;
        let blocks = create_chained_blocks(10);
        let first_hash = blocks[0].hash();
        mock.set_get_block_header_by_hash(Some(blocks[0].header.clone())).await;
        // Remember: this response is sent back regardless of what is requested. So we have to test what bounds are sent
        mock.set_get_blocks_response(blocks).await;

        let msg = SyncBlocksRequest {
            start_hash: first_hash,
            count: 0,
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let streaming = service.sync_blocks(req).await.unwrap();
        let _ = streaming.map(Result::unwrap).collect::<Vec<_>>().await;

        // assert_eq!(mock.get_call_count("get_blocks").await, 3);
        let (start, end) = mock
            .pop_front_call::<(Bound<u64>, Bound<u64>)>("get_blocks")
            .await
            .unwrap();
        unpack_enum!(Bound::Included(start) = start);
        assert_eq!(start, 0);
        unpack_enum!(Bound::Included(end) = end);
        assert_eq!(end, 9);

        let (start, end) = mock
            .pop_front_call::<(Bound<u64>, Bound<u64>)>("get_blocks")
            .await
            .unwrap();
        unpack_enum!(Bound::Included(start) = start);
        assert_eq!(start, 10);
        unpack_enum!(Bound::Included(end) = end);
        assert_eq!(end, 19);

        let (start, end) = mock
            .pop_front_call::<(Bound<u64>, Bound<u64>)>("get_blocks")
            .await
            .unwrap();
        unpack_enum!(Bound::Included(start) = start);
        assert_eq!(start, 20);
        unpack_enum!(Bound::Included(end) = end);
        assert_eq!(end, 20);
    }

    #[tokio_macros::test_basic]
    async fn it_does_not_crash_if_overflowed_u64() {
        let (service, mock, rpc_request_mock, _tmp) = setup();
        let blocks = create_chained_blocks(1);
        mock.set_get_block_header_by_hash(Some(blocks[0].header.clone())).await;
        // Remember: this response is sent back regardless of what is requested. So we have to test what bounds are sent
        mock.set_get_blocks_response(blocks).await;
        let metadata = ChainMetadata::new(20, Vec::new(), 0, 0, 0);
        mock.set_get_chain_metadata_response(metadata).await;

        let msg = SyncBlocksRequest {
            start_hash: vec![],
            count: u64::MAX,
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let _ = service.sync_blocks(req).await.unwrap();
    }
}
