//  Copyright 2022, The Tari Project
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

use std::convert::TryFrom;

use tari_common_types::types::PublicKey;
use tari_comms::{protocol::rpc::mock::RpcRequestMock, test_utils};
use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
use tari_dan_core::{
    fixed_hash::FixedHash,
    models::{Node, TreeNodeHash},
    services::mocks::{MockAssetProcessor, MockMempoolService},
    storage::{chain::ChainDbUnitOfWork, mocks::MockDbFactory, DbFactory},
};
use tari_test_utils::{paths::tempdir, streams::convert_mpsc_to_stream};
use tokio_stream::StreamExt;

use crate::p2p::{
    proto,
    rpc::{ValidatorNodeRpcService, ValidatorNodeRpcServiceImpl},
};

fn setup() -> (
    ValidatorNodeRpcServiceImpl<MockMempoolService, MockDbFactory, MockAssetProcessor>,
    RpcRequestMock,
    MockDbFactory,
) {
    let tmp = tempdir().unwrap();
    let peer_manager = test_utils::build_peer_manager(&tmp);
    let mock = RpcRequestMock::new(peer_manager);
    let mempool = MockMempoolService;
    let db_factory = MockDbFactory::default();
    let asset_processor = MockAssetProcessor;
    let service = ValidatorNodeRpcServiceImpl::new(mempool, db_factory.clone(), asset_processor);

    (service, mock, db_factory)
}

mod get_sidechain_blocks {

    use super::*;

    #[tokio::test]
    async fn it_fetches_matching_block() {
        let (service, mock, db_factory) = setup();
        let asset_public_key = PublicKey::default();
        let db = db_factory.get_or_create_chain_db(&asset_public_key).unwrap();
        let mut uow = db.new_unit_of_work();

        // Some random parent hash to ensure stream does not last forever
        let parent =
            TreeNodeHash::from_hex("972209d3622c1227a499fd2cfcfa75fdde547d1a21fa805522d3a1a315ebd1a3").unwrap();
        uow.add_node(TreeNodeHash::zero(), parent, 1).unwrap();
        uow.commit().unwrap();

        let req = proto::validator_node::GetSidechainBlocksRequest {
            asset_public_key: asset_public_key.to_vec(),
            start_hash: TreeNodeHash::zero().as_bytes().to_vec(),
            end_hash: vec![],
        };
        let req = mock.request_with_context(Default::default(), req);
        let mut resp = service.get_sidechain_blocks(req).await.unwrap().into_inner();
        let stream = convert_mpsc_to_stream(&mut resp).map(|r| r.unwrap());

        let responses = stream
            .collect::<Vec<proto::validator_node::GetSidechainBlocksResponse>>()
            .await;
        assert_eq!(responses.len(), 1);
        let node = Node::new(TreeNodeHash::zero(), parent, 1, false);
        let block = responses[0].block.clone();
        assert_eq!(
            Node::try_from(block.as_ref().unwrap().node.clone().unwrap()).unwrap(),
            node
        );
        assert_eq!(
            block.as_ref().unwrap().instructions.clone().unwrap().instructions.len(),
            0
        );
    }

    #[tokio::test]
    async fn it_errors_if_asset_not_found() {
        let (service, mock, _) = setup();

        let req = proto::validator_node::GetSidechainBlocksRequest {
            asset_public_key: PublicKey::default().to_vec(),
            start_hash: FixedHash::zero().as_bytes().to_vec(),
            end_hash: vec![],
        };
        let req = mock.request_with_context(Default::default(), req);
        let err = service.get_sidechain_blocks(req).await.unwrap_err();
        assert!(err.as_status_code().is_not_found());
        assert_eq!(err.details(), "Asset not found");
    }

    #[tokio::test]
    async fn it_errors_if_block_not_found() {
        let (service, mock, db_factory) = setup();
        let asset_public_key = PublicKey::default();
        let db = db_factory.get_or_create_chain_db(&asset_public_key).unwrap();
        db.new_unit_of_work().commit().unwrap();

        let req = proto::validator_node::GetSidechainBlocksRequest {
            asset_public_key: asset_public_key.to_vec(),
            start_hash: FixedHash::zero().as_bytes().to_vec(),
            end_hash: vec![],
        };
        let req = mock.request_with_context(Default::default(), req);
        let err = service.get_sidechain_blocks(req).await.unwrap_err();
        assert!(err.as_status_code().is_not_found());
        assert!(err.details().starts_with("Block not found"));
    }
}
