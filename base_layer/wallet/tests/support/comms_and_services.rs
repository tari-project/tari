// Copyright 2019. The Tari Project
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

use crate::support::utils::random_string;
use futures::Sink;
use rand::rngs::OsRng;
use std::{error::Error, sync::Arc, time::Duration};
use tari_comms::{
    builder::CommsNode,
    connection::NetAddress,
    control_service::ControlServiceConfig,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
};
use tari_comms_dht::Dht;
use tari_crypto::keys::PublicKey;
use tari_p2p::{
    comms_connector::{InboundDomainConnector, PeerMessage},
    domain_message::DomainMessage,
    initialization::{initialize_comms, CommsConfig},
};
use tempdir::TempDir;
use tokio::runtime::TaskExecutor;

pub fn setup_comms_services<TSink>(
    executor: TaskExecutor,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<NodeIdentity>,
    publisher: InboundDomainConnector<TSink>,
) -> (CommsNode, Dht)
where
    TSink: Sink<Arc<PeerMessage>> + Clone + Unpin + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let comms_config = CommsConfig {
        node_identity: Arc::clone(&node_identity),
        host: "127.0.0.1".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: node_identity.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        datastore_path: TempDir::new(random_string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string(),
        establish_connection_timeout: Duration::from_secs(3),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
    };

    let (comms, dht) = initialize_comms(executor, comms_config, publisher).unwrap();

    for p in peers {
        let addr = p.control_service_address();
        comms
            .peer_manager()
            .add_peer(Peer::new(
                p.public_key().clone(),
                p.node_id().clone(),
                addr.into(),
                PeerFlags::empty(),
                PeerFeatures::empty(),
            ))
            .unwrap();
    }

    (comms, dht)
}

pub fn create_dummy_message<T>(inner: T) -> DomainMessage<T> {
    let mut rng = OsRng::new().unwrap();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
    let peer_source = Peer::new(
        pk.clone(),
        NodeId::from_key(&pk).unwrap(),
        Vec::<NetAddress>::new().into(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_NODE,
    );
    DomainMessage {
        origin_pubkey: peer_source.public_key.clone(),
        source_peer: peer_source,
        inner,
    }
}
