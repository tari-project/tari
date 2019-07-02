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

use std::{sync::Arc, time::Duration};
use tari_comms::{
    connection::{net_address::NetAddressWithStats, NetAddress, NetAddressesWithStats},
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    peer_manager::{peer::PeerFlags, NodeId, NodeIdentity, Peer},
    types::CommsPublicKey,
    CommsBuilder,
};
use tari_p2p::{
    services::{ServiceExecutor, ServiceRegistry},
    tari_message::{NetMessage, TariMessageType},
};
use tari_storage::lmdb_store::LMDBDatabase;
use tari_wallet::text_message_service::{TextMessageService, TextMessageServiceApi};

fn create_peer(public_key: CommsPublicKey, net_address: NetAddress) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key).unwrap(),
        NetAddressesWithStats::new(vec![NetAddressWithStats::new(net_address.clone())]),
        PeerFlags::empty(),
    )
}

pub fn setup_text_message_service(
    node_identity: NodeIdentity,
    peers: Vec<NodeIdentity>,
    peer_database: LMDBDatabase,
) -> (ServiceExecutor, Arc<TextMessageServiceApi>)
{
    let tms = TextMessageService::new(node_identity.identity.public_key.clone());
    let tms_api = tms.get_api();

    let services = ServiceRegistry::new().register(tms);

    let comms = CommsBuilder::new()
        .with_routes(services.build_comms_routes())
        .with_node_identity(node_identity.clone())
        .with_peer_storage(peer_database)
        .configure_peer_connections(PeerConnectionConfig {
            host: "127.0.0.1".parse().unwrap(),
            ..Default::default()
        })
        .configure_control_service(ControlServiceConfig {
            socks_proxy_address: None,
            listener_address: node_identity.control_service_address.clone(),
            accept_message_type: TariMessageType::new(NetMessage::Accept),
            requested_outbound_connection_timeout: Duration::from_millis(5000),
        })
        .build()
        .unwrap()
        .start()
        .unwrap();

    for p in peers {
        comms
            .peer_manager()
            .add_peer(create_peer(p.identity.public_key.clone(), p.control_service_address))
            .unwrap();
    }

    (ServiceExecutor::execute(Arc::new(comms), services), tms_api)
}
