//   Copyright 2023. The Taiji Project
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

// Re-exports
pub use taiji_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use taiji_comms::{peer_manager::Peer, CommsNode, UnspawnedCommsNode};
use taiji_contacts::contacts_service::{handle::ContactsServiceHandle, ContactsServiceInitializer};
use taiji_p2p::{
    comms_connector::pubsub_connector,
    initialization::{spawn_comms_using_transport, P2pInitializer},
    peer_seeds::SeedPeer,
    services::liveness::{LivenessConfig, LivenessInitializer},
};
use taiji_service_framework::StackBuilder;
use taiji_shutdown::ShutdownSignal;

use crate::{config::ApplicationConfig, database::connect_to_db};

pub async fn start(
    node_identity: Arc<NodeIdentity>,
    config: ApplicationConfig,
    shutdown_signal: ShutdownSignal,
) -> anyhow::Result<(ContactsServiceHandle, CommsNode)> {
    let backend = connect_to_db(config.chat_client.db_file)?;

    let (publisher, subscription_factory) = pubsub_connector(100, 50);
    let in_msg = Arc::new(subscription_factory);

    let fut = StackBuilder::new(shutdown_signal)
        .add_initializer(P2pInitializer::new(
            config.chat_client.p2p.clone(),
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

    let mut handles = fut.await.expect("Service initialization failed");

    let comms = handles
        .take_handle::<UnspawnedCommsNode>()
        .expect("P2pInitializer was not added to the stack or did not add UnspawnedCommsNode");

    let peer_manager = comms.peer_manager();

    let seed_peers = config
        .peer_seeds
        .peer_seeds
        .iter()
        .map(|s| SeedPeer::from_str(s))
        .map(|r| r.map(Peer::from))
        .collect::<Result<Vec<_>, _>>()?;

    for peer in seed_peers {
        peer_manager.add_peer(peer).await?;
    }

    let comms = spawn_comms_using_transport(comms, config.chat_client.p2p.transport.clone())
        .await
        .unwrap();
    handles.register(comms);

    let comms = handles.expect_handle::<CommsNode>();
    let contacts = handles.expect_handle::<ContactsServiceHandle>();
    Ok((contacts, comms))
}
