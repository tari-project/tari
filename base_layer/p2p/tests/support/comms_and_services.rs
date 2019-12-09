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
use std::{error::Error, sync::Arc};
use tari_comms::{
    builder::CommsNode,
    peer_manager::{NodeIdentity, Peer, PeerFlags},
};
use tari_comms_dht::Dht;
use tari_p2p::{
    comms_connector::{InboundDomainConnector, PeerMessage},
    initialization::initialize_local_test_comms,
};
use tokio::runtime::TaskExecutor;

pub fn setup_comms_services<TSink>(
    executor: TaskExecutor,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<NodeIdentity>,
    publisher: InboundDomainConnector<TSink>,
    data_path: &str,
) -> (Arc<CommsNode>, Dht)
where
    TSink: Sink<Arc<PeerMessage>> + Clone + Unpin + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let (comms, dht) = initialize_local_test_comms(executor, node_identity, publisher, data_path)
        .map(|(comms, dht)| (Arc::new(comms), dht))
        .unwrap();

    for p in peers {
        let addr = p.control_service_address().clone();
        comms
            .peer_manager()
            .add_peer(Peer::new(
                p.public_key().clone(),
                p.node_id().clone(),
                addr.into(),
                PeerFlags::default(),
                p.features().clone(),
            ))
            .unwrap();
    }

    (comms, dht)
}
