//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{str::FromStr, sync::Arc, time::Duration};

use log::trace;
use minotari_app_utilities::{identity_management, identity_management::load_from_json};
// Re-exports
pub use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use tari_comms::{peer_manager::Peer, tor::TorIdentity, CommsNode, UnspawnedCommsNode};
use tari_contacts::contacts_service::{handle::ContactsServiceHandle, ContactsServiceInitializer};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{spawn_comms_using_transport, P2pInitializer},
    peer_seeds::SeedPeer,
    services::liveness::{LivenessConfig, LivenessInitializer},
    TransportType,
};
use tari_service_framework::StackBuilder;
use tari_shutdown::ShutdownSignal;

use crate::{
    config::ApplicationConfig,
    database::{connect_to_db, create_chat_storage},
    error::NetworkingError,
};

const LOG_TARGET: &str = "contacts::chat_client::networking";

pub async fn start(
    node_identity: Arc<NodeIdentity>,
    config: ApplicationConfig,
    shutdown_signal: ShutdownSignal,
) -> Result<(ContactsServiceHandle, CommsNode), NetworkingError> {
    create_chat_storage(&config.chat_client.db_file)?;
    let backend = connect_to_db(config.chat_client.db_file)?;

    let (publisher, subscription_factory) = pubsub_connector(100);
    let in_msg = Arc::new(subscription_factory);

    let mut p2p_config = config.chat_client.p2p.clone();

    let tor_identity = load_from_json(&config.chat_client.tor_identity_file)?;
    p2p_config.transport.tor.identity = tor_identity.clone();
    trace!(target: LOG_TARGET, "loaded chat tor identity {:?}", tor_identity);

    let fut = StackBuilder::new(shutdown_signal)
        .add_initializer(P2pInitializer::new(
            p2p_config.clone(),
            config.peer_seeds.clone(),
            config.chat_client.network,
            node_identity,
            publisher,
        ))
        .add_initializer(LivenessInitializer::new(
            LivenessConfig {
                auto_ping_interval: Some(config.chat_client.metadata_auto_ping_interval),
                ..Default::default()
            },
            in_msg.clone(),
        ))
        .add_initializer(ContactsServiceInitializer::new(
            backend,
            in_msg,
            Duration::from_secs(5),
            2,
        ))
        .build();

    let mut handles = fut.await?;

    let comms = handles
        .take_handle::<UnspawnedCommsNode>()
        .ok_or(NetworkingError::CommsSpawnError)?;

    let peer_manager = comms.peer_manager();

    let seed_peers = config
        .peer_seeds
        .peer_seeds
        .iter()
        .map(|s| SeedPeer::from_str(s))
        .map(|r| r.map(Peer::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| NetworkingError::PeerSeeds(e.to_string()))?;

    for peer in seed_peers {
        peer_manager.add_peer(peer).await?;
    }
    let comms = if p2p_config.transport.transport_type == TransportType::Tor {
        let path = config.chat_client.tor_identity_file.clone();
        let node_id = comms.node_identity();
        let after_comms = move |identity: TorIdentity| {
            let _result = identity_management::save_as_json(&path, &identity);
            let address: Multiaddr = format!("/onion3/{}:{}", identity.service_id, identity.onion_port)
                .parse()
                .expect("Should be able to create address");
            trace!(target: LOG_TARGET, "resave the chat tor identity {:?}", identity);
            if !node_id.public_addresses().contains(&address) {
                node_id.add_public_address(address);
            }
        };
        spawn_comms_using_transport(comms, p2p_config.transport.clone(), after_comms).await?
    } else {
        let after_comms = |_identity| {};
        spawn_comms_using_transport(comms, p2p_config.transport.clone(), after_comms).await?
    };
    // changed by comms during initialization when using tor.
    match p2p_config.transport.transport_type {
        TransportType::Tcp => {}, // Do not overwrite TCP public_address in the base_node_id!
        _ => {
            identity_management::save_as_json(&config.chat_client.identity_file, &*comms.node_identity())?;
            trace!(target: LOG_TARGET, "save chat identity file");
        },
    };

    handles.register(comms);

    let comms = handles.expect_handle::<CommsNode>();
    let contacts = handles.expect_handle::<ContactsServiceHandle>();
    Ok((contacts, comms))
}
