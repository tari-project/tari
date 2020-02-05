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

use crate::{
    backoff::ConstantBackoff,
    connection_manager::next::{ConnectionManager, ConnectionManagerConfig, ConnectionManagerRequester},
    noise::NoiseConfig,
    peer_manager::{NodeIdentity, PeerFeatures, PeerManager},
    protocol::Protocols,
    transports::MemoryTransport,
};
use futures::channel::mpsc;
use rand::rngs::OsRng;
use std::{sync::Arc, time::Duration};
use tari_shutdown::ShutdownSignal;
use tari_storage::HashmapDatabase;
use tokio::{runtime, sync::broadcast};

#[derive(Clone, Debug)]
pub struct TestNodeConfig {
    pub transport: MemoryTransport,
    pub dial_backoff_duration: Duration,
    pub connection_manager_config: ConnectionManagerConfig,
    pub node_identity: Arc<NodeIdentity>,
}

impl Default for TestNodeConfig {
    fn default() -> Self {
        let node_identity = Arc::new(
            NodeIdentity::random(
                &mut OsRng,
                "/memory/0".parse().unwrap(),
                PeerFeatures::COMMUNICATION_NODE,
            )
            .unwrap(),
        );

        Self {
            transport: MemoryTransport,
            connection_manager_config: ConnectionManagerConfig {
                listener_address: "/memory/0".parse().unwrap(),
                ..Default::default()
            },
            dial_backoff_duration: Duration::from_millis(200),
            node_identity,
        }
    }
}

pub fn build_connection_manager(
    executor: runtime::Handle,
    config: TestNodeConfig,
    peer_manager: Arc<PeerManager>,
    shutdown: ShutdownSignal,
) -> ConnectionManagerRequester
{
    // TODO: Once we have `comms::Builder@next` we can construct a whole "comms node" here for testing
    let noise_config = NoiseConfig::new(config.node_identity.clone());
    let (request_tx, request_rx) = mpsc::channel(10);
    let (event_tx, _) = broadcast::channel(100);

    let requester = ConnectionManagerRequester::new(request_tx, event_tx.clone());

    let connection_manager = ConnectionManager::new(
        config.connection_manager_config,
        executor.clone(),
        config.transport,
        noise_config,
        Arc::new(ConstantBackoff::new(config.dial_backoff_duration)),
        request_rx,
        config.node_identity,
        peer_manager.into(),
        Protocols::new(),
        event_tx,
        shutdown,
    );

    executor.spawn(connection_manager.run());

    requester
}

pub fn build_peer_manager() -> Arc<PeerManager> {
    Arc::new(PeerManager::new(HashmapDatabase::new()).unwrap())
}
