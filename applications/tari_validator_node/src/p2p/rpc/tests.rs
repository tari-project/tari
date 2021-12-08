//  Copyright 2021, The Tari Project
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

use tari_comms::protocol::rpc::mock::RpcRequestMock;
use tari_core::test_helpers::create_peer_manager;
use tari_dan_core::services::mocks::{create_mempool_mock, MockMempoolService};
use tari_test_utils::paths::temp_tari_path;

use super::ValidatorNodeRpcService;
use crate::{
    p2p::{proto::validator_node as proto, rpc::ValidatorNodeRpcServiceImpl},
    services::mocks::{create_mempool_mock, MockMempoolService},
};

fn setup() -> (ValidatorNodeRpcServiceImpl<MockMempoolService>, RpcRequestMock) {
    let mempool = create_mempool_mock();
    let peer_manager = create_peer_manager(temp_tari_path());
    let mock = RpcRequestMock::new(peer_manager);
    let service_impl = ValidatorNodeRpcServiceImpl::new(mempool);
    (service_impl, mock)
}

#[tokio::test]
async fn it_works() {
    let (service_impl, req_mock) = setup();
    let msg = proto::SubmitInstructionRequest {
        asset_public_key: vec![0; 32],
        ..Default::default()
    };
    let request = req_mock.request_no_context(msg);
    let response = service_impl.submit_instruction(request).await.unwrap();
    assert_eq!(response.into_message().status, proto::Status::Accepted as i32);
}
