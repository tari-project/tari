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
    connection_manager::next::{ConnectionManager, ConnectionManagerRequester},
    consts::COMMS_RNG,
    multiaddr::Multiaddr,
    noise::NoiseConfig,
    peer_manager::{NodeIdentity, PeerFeatures, PeerManager},
    transports::{NoiseTransport, TcpTransport},
};
use futures::channel::mpsc;
use std::{sync::Arc, time::Duration};
use tari_shutdown::ShutdownSignal;
use tari_storage::HashmapDatabase;
use tokio::runtime::Runtime;

#[derive(Clone, Debug)]
pub struct TestNodeConfig {
    listen_address: Multiaddr,
    transport: TcpTransport,
    dial_backoff_duration: Duration,
    node_identity: Arc<NodeIdentity>,
}

impl Default for TestNodeConfig {
    fn default() -> Self {
        let node_identity = COMMS_RNG.with(|rng| {
            Arc::new(
                NodeIdentity::random(
                    &mut *rng.borrow_mut(),
                    "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
                    PeerFeatures::COMMUNICATION_NODE,
                )
                .unwrap(),
            )
        });

        Self {
            listen_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            transport: TcpTransport::default(),
            dial_backoff_duration: Duration::from_millis(200),
            node_identity,
        }
    }
}

pub fn build_connection_manager(
    runtime: Runtime,
    config: TestNodeConfig,
    shutdown: ShutdownSignal,
) -> ConnectionManagerRequester
{
    // TODO: Once we have `comms::Builder@next` we can construct a whole "comms node" here for testing
    let transport = NoiseTransport::new(TcpTransport::default(), NoiseConfig::new(config.node_identity.clone()));
    let (request_tx, request_rx) = mpsc::channel(10);
    let requester = ConnectionManagerRequester::new(request_tx);

    let peer_manager = build_peer_manager();

    let connection_manager = ConnectionManager::new(
        Default::default(),
        runtime.handle().clone(),
        transport,
        Arc::new(ConstantBackoff::new(config.dial_backoff_duration)),
        request_rx,
        peer_manager.into(),
        shutdown,
    );

    runtime.spawn(connection_manager.run());

    requester
}

pub fn build_peer_manager() -> Arc<PeerManager> {
    Arc::new(PeerManager::new(HashmapDatabase::new()).unwrap())
}
