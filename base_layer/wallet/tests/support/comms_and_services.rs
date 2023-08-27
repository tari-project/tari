// Copyright 2019. The Taiji Project
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

use taiji_comms::{
    message::MessageTag,
    net_address::MultiaddressesWithStats,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
    CommsNode,
};
use taiji_comms_dht::{envelope::DhtMessageHeader, Dht, DhtProtocolVersion};
use taiji_p2p::{
    comms_connector::InboundDomainConnector,
    domain_message::DomainMessage,
    initialization::initialize_local_test_comms,
};
use taiji_shutdown::ShutdownSignal;

pub async fn setup_comms_services(
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    publisher: InboundDomainConnector,
    database_path: String,
    discovery_request_timeout: Duration,
    shutdown_signal: ShutdownSignal,
) -> (CommsNode, Dht) {
    let peers = peers.into_iter().map(|ni| ni.to_peer()).collect();
    let (comms, dht, _) = initialize_local_test_comms(
        node_identity,
        publisher,
        &database_path,
        discovery_request_timeout,
        peers,
        shutdown_signal,
    )
    .await
    .unwrap();

    (comms, dht)
}

pub fn create_dummy_message<T>(inner: T, public_key: &CommsPublicKey) -> DomainMessage<T> {
    let peer_source = Peer::new(
        public_key.clone(),
        NodeId::from_key(public_key),
        MultiaddressesWithStats::empty(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_NODE,
        Default::default(),
        Default::default(),
    );
    DomainMessage {
        dht_header: DhtMessageHeader {
            version: DhtProtocolVersion::latest(),
            ephemeral_public_key: None,
            message_signature: Vec::new(),
            message_type: Default::default(),
            flags: Default::default(),
            destination: Default::default(),
            message_tag: MessageTag::new(),
            expires: None,
        },
        authenticated_origin: None,
        source_peer: peer_source,
        inner,
    }
}
