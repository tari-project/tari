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
use tari_comms::{peer_manager::NodeIdentity, protocol::messaging::MessagingEventSender, CommsNode};
use tari_comms_dht::Dht;
use tari_p2p::{
    comms_connector::{InboundDomainConnector, PeerMessage},
    initialization::initialize_local_test_comms,
};
use tari_shutdown::ShutdownSignal;

pub async fn setup_comms_services<TSink>(
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    publisher: InboundDomainConnector<TSink>,
    data_path: &str,
    shutdown_signal: ShutdownSignal,
) -> (CommsNode, Dht, MessagingEventSender)
where
    TSink: Sink<Arc<PeerMessage>> + Clone + Unpin + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let peers = peers.into_iter().map(|ni| ni.to_peer()).collect();
    let (comms, dht, messaging_events) = initialize_local_test_comms(
        node_identity,
        publisher,
        data_path,
        Duration::from_secs(1),
        peers,
        shutdown_signal,
    )
    .await
    .unwrap();

    (comms, dht, messaging_events)
}
