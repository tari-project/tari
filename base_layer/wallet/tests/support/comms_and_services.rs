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

use futures::Sink;
use std::{error::Error, sync::Arc, time::Duration};
use tari_comms::{
    builder::CommsNode,
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
};
use tari_comms_dht::{envelope::DhtMessageHeader, Dht};
use tari_p2p::{
    comms_connector::{InboundDomainConnector, PeerMessage},
    domain_message::DomainMessage,
    initialization::initialize_local_test_comms,
};
use tokio::runtime;

pub fn setup_comms_services<TSink>(
    executor: runtime::Handle,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<NodeIdentity>,
    publisher: InboundDomainConnector<TSink>,
    database_path: String,
    discovery_request_timeout: Duration,
) -> (CommsNode, Dht)
where
    TSink: Sink<Arc<PeerMessage>> + Clone + Unpin + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let (comms, dht) = initialize_local_test_comms(
        executor,
        node_identity,
        publisher,
        &database_path,
        discovery_request_timeout,
    )
    .unwrap();

    for p in peers {
        let addr = p.control_service_address();
        comms
            .peer_manager()
            .add_peer(Peer::new(
                p.public_key().clone(),
                p.node_id().clone(),
                addr.into(),
                PeerFlags::empty(),
                PeerFeatures::COMMUNICATION_NODE,
            ))
            .unwrap();
    }

    (comms, dht)
}

pub fn create_dummy_message<T>(inner: T, public_key: &CommsPublicKey) -> DomainMessage<T> {
    let peer_source = Peer::new(
        public_key.clone(),
        NodeId::from_key(public_key).unwrap(),
        Vec::<Multiaddr>::new().into(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_NODE,
    );
    DomainMessage {
        dht_header: DhtMessageHeader {
            origin_public_key: peer_source.public_key.clone(),
            origin_signature: Default::default(),
            version: Default::default(),
            message_type: Default::default(),
            flags: Default::default(),
            destination: Default::default(),
        },
        source_peer: peer_source,
        inner,
    }
}
