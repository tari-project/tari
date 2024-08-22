// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{sync::Arc, time::Duration};

use rand::rngs::OsRng;
use tari_shutdown::ShutdownSignal;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
};

use crate::{
    backoff::ConstantBackoff,
    connection_manager::{ConnectionManager, ConnectionManagerConfig, ConnectionManagerRequester},
    multiplexing::Substream,
    peer_manager::{NodeIdentity, PeerFeatures, PeerManager},
    peer_validator::PeerValidatorConfig,
    protocol::Protocols,
    transports::Transport,
};

#[derive(Clone, Debug)]
pub struct TestNodeConfig {
    pub dial_backoff_duration: Duration,
    pub connection_manager_config: ConnectionManagerConfig,
    pub node_identity: Arc<NodeIdentity>,
}

impl Default for TestNodeConfig {
    fn default() -> Self {
        let node_identity = Arc::new(NodeIdentity::random(
            &mut OsRng,
            "/memory/0".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        ));

        Self {
            connection_manager_config: ConnectionManagerConfig {
                peer_validation_config: PeerValidatorConfig {
                    allow_test_addresses: true,
                    ..Default::default()
                },
                listener_address: "/memory/0".parse().unwrap(),
                ..Default::default()
            },
            dial_backoff_duration: Duration::from_millis(200),
            node_identity,
        }
    }
}

pub fn build_connection_manager<TTransport>(
    config: TestNodeConfig,
    transport: TTransport,
    peer_manager: Arc<PeerManager>,
    protocols: Protocols<Substream>,
    shutdown: ShutdownSignal,
) -> ConnectionManagerRequester
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
{
    let (request_tx, request_rx) = mpsc::channel(10);
    let (event_tx, _) = broadcast::channel(100);

    let requester = ConnectionManagerRequester::new(request_tx, event_tx.clone());

    let mut connection_manager = ConnectionManager::new(
        config.connection_manager_config,
        transport,
        ConstantBackoff::new(config.dial_backoff_duration),
        request_rx,
        config.node_identity,
        peer_manager,
        event_tx,
        shutdown,
        &[1, 1, 1, 1]
    );
    connection_manager.add_protocols(protocols);

    connection_manager.spawn();

    requester
}
