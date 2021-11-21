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
    base_node::BaseNodeSyncService,
    chain_storage::BlockchainDatabase,
    proto::base_node::SyncBlocksRequest,
    test_helpers::{
        blockchain::{create_main_chain, create_new_blockchain, TempDatabase},
        create_peer_manager,
    },
};
use futures::StreamExt;
use tari_comms::protocol::rpc::{mock::RpcRequestMock, RpcStatusCode};
use tari_test_utils::{streams::convert_mpsc_to_stream, unpack_enum};
use tempfile::{tempdir, TempDir};

fn setup() -> (
    BaseNodeSyncRpcService<TempDatabase>,
    BlockchainDatabase<TempDatabase>,
    RpcRequestMock,
    TempDir,
) {
    let tmp = tempdir().unwrap();
    let peer_manager = create_peer_manager(&tmp);
    let request_mock = RpcRequestMock::new(peer_manager);

    let db = create_new_blockchain();
    let service = BaseNodeSyncRpcService::new(db.clone().into());
    (service, db, request_mock, tmp)
}

mod sync_blocks {
    use super::*;

    #[tokio::test]
    async fn it_returns_not_found_if_unknown_hash() {
        let (service, _, rpc_request_mock, _tmp) = setup();
        let msg = SyncBlocksRequest {
            start_hash: vec![0; 32],
            end_hash: vec![0; 32],
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let err = service.sync_blocks(req).await.unwrap_err();
        unpack_enum!(RpcStatusCode::NotFound = err.as_status_code());
    }

    #[tokio::test]
    async fn it_sends_an_empty_response() {
        let (service, db, rpc_request_mock, _tmp) = setup();

        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"]));

        let block = chain.get("A").unwrap();
        let msg = SyncBlocksRequest {
            start_hash: block.hash().clone(),
            end_hash: block.hash().clone(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_blocks(req).await.unwrap();
        assert!(streaming.next().await.is_none());
    }

    #[tokio::test]
    async fn it_streams_blocks_until_end() {
        let (service, db, rpc_request_mock, _tmp) = setup();

        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"], ["D->C"], ["E->D"]));

        let first_block = chain.get("A").unwrap();
        let last_block = chain.get("E").unwrap();

        let msg = SyncBlocksRequest {
            start_hash: first_block.hash().clone(),
            end_hash: last_block.hash().clone(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_blocks(req).await.unwrap().into_inner();
        let blocks = convert_mpsc_to_stream(&mut streaming)
            .map(|block| block.unwrap())
            .collect::<Vec<_>>()
            .await;

        assert_eq!(blocks.len(), 4);
        blocks.iter().zip(["B", "C", "D", "E"]).for_each(|(block, name)| {
            assert_eq!(*chain.get(name).unwrap().hash(), block.hash);
        });
    }
}
