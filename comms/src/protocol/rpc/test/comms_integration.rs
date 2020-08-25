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
    protocol::rpc::{
        test::mock::{MockRpcClient, MockRpcService},
        RpcError,
        RpcServer,
        RpcStatus,
        RpcStatusCode,
    },
    runtime,
    test_utils::node_identity::build_node_identity,
    transports::MemoryTransport,
    types::CommsDatabase,
    CommsBuilder,
};
use tari_test_utils::unpack_enum;

#[runtime::test_basic]
async fn run_service() {
    let node_identity1 = build_node_identity(Default::default());
    let rpc_service = MockRpcService::new();
    let mock_state = rpc_service.shared_state();
    let comms1 = CommsBuilder::new()
        .with_listener_address(node_identity1.public_address())
        .with_node_identity(node_identity1)
        .with_transport(MemoryTransport)
        .with_peer_storage(CommsDatabase::new())
        .add_rpc(RpcServer::new().add_service(rpc_service))
        .build()
        .unwrap()
        .spawn()
        .await
        .unwrap();

    let node_identity2 = build_node_identity(Default::default());
    let comms2 = CommsBuilder::new()
        .with_listener_address(node_identity2.public_address())
        .with_node_identity(node_identity2.clone())
        .with_transport(MemoryTransport)
        .with_peer_storage(CommsDatabase::new())
        .build()
        .unwrap()
        .add_peers(vec![comms1.node_identity().to_peer()])
        .await
        .unwrap()
        .spawn()
        .await
        .unwrap();

    let mut conn = comms2
        .connectivity()
        .dial_peer(comms1.node_identity().node_id().clone())
        .await
        .unwrap();

    let mut client = conn.connect_rpc::<MockRpcClient>().await.unwrap();

    mock_state.set_response_ok(());
    client.request_response::<_, ()>((), 0.into()).await.unwrap();
    assert_eq!(mock_state.call_count(), 1);

    mock_state.set_response_err(RpcStatus::bad_request("Insert 💾"));
    let err = client.request_response::<_, ()>((), 0.into()).await.unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    unpack_enum!(RpcStatusCode::BadRequest = status.status_code());
    assert_eq!(mock_state.call_count(), 2);
}
