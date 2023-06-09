//  Copyright 2022. The Tari Project
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
#![cfg(feature = "rpc")]
use std::time::Duration;

use futures::StreamExt;
use tari_comms::{
    protocol::rpc::{RpcServer, RpcServerHandle},
    transports::TcpTransport,
    CommsNode,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_test_utils::async_assert_eventually;
use tokio::time;

use crate::tests::{
    greeting_service::{GreetingClient, GreetingServer, GreetingService, StreamLargeItemsRequest},
    helpers::create_comms,
};

async fn spawn_node(signal: ShutdownSignal) -> (CommsNode, RpcServerHandle) {
    let rpc_server = RpcServer::builder()
        .with_unlimited_simultaneous_sessions()
        .finish()
        .add_service(GreetingServer::new(GreetingService::default()));

    let rpc_server_hnd = rpc_server.get_handle();
    let comms = create_comms(signal)
        .add_rpc_server(rpc_server)
        .spawn_with_transport(TcpTransport::new())
        .await
        .unwrap();

    comms
        .node_identity()
        .replace_public_address(comms.listening_address().clone());

    (comms, rpc_server_hnd)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn client_prematurely_ends_session() {
    env_logger::init();
    let shutdown = Shutdown::new();
    let (node1, _rpc_server1) = spawn_node(shutdown.to_signal()).await;
    let (node2, mut rpc_server2) = spawn_node(shutdown.to_signal()).await;

    node1
        .peer_manager()
        .add_peer(node2.node_identity().to_peer())
        .await
        .unwrap();

    let mut conn1_2 = node1
        .connectivity()
        .dial_peer(node2.node_identity().node_id().clone())
        .await
        .unwrap();

    {
        let mut client = conn1_2.connect_rpc::<GreetingClient>().await.unwrap();

        let num_sessions = rpc_server2
            .get_num_active_sessions_for(node1.node_identity().node_id().clone())
            .await
            .unwrap();
        assert_eq!(num_sessions, 1);

        let mut stream = client
            .stream_large_items(StreamLargeItemsRequest {
                id: 1,
                num_items: 100,
                item_size: 2300 * 1024,
                delay_ms: 50,
            })
            .await
            .unwrap();

        let mut count = 0;
        while let Some(r) = stream.next().await {
            count += 1;

            let data = r.unwrap();
            assert_eq!(data.len(), 2300 * 1024);
            // Prematurely drop the stream
            if count == 5 {
                log::info!("Ending the stream prematurely");
                drop(stream);
                break;
            }
        }

        // Drop stream and client
    }

    time::sleep(Duration::from_secs(1)).await;
    async_assert_eventually!(
        rpc_server2
            .get_num_active_sessions_for(node1.node_identity().node_id().clone())
            .await
            .unwrap(),
        expect = 0,
        max_attempts = 20,
        interval = Duration::from_millis(1000)
    );
}
