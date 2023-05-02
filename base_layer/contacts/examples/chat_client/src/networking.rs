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

use std::{path::PathBuf, sync::Arc, time::Duration};

// Re-exports
pub use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use tari_comms::{peer_manager::Peer, CommsNode, UnspawnedCommsNode};
use tari_contacts::contacts_service::{handle::ContactsServiceHandle, ContactsServiceInitializer};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{spawn_comms_using_transport, P2pInitializer},
    services::liveness::{LivenessConfig, LivenessInitializer},
    Network,
    P2pConfig,
    PeerSeedsConfig,
};
use tari_service_framework::StackBuilder;
use tari_shutdown::ShutdownSignal;

use crate::database::connect_to_db;

pub async fn start(
    node_identity: Arc<NodeIdentity>,
    config: P2pConfig,
    seed_peers: Vec<Peer>,
    network: Network,
    db_path: PathBuf,
    shutdown_signal: ShutdownSignal,
) -> anyhow::Result<(ContactsServiceHandle, CommsNode)> {
    let backend = connect_to_db(db_path)?;

    let (publisher, subscription_factory) = pubsub_connector(100, 50);
    let in_msg = Arc::new(subscription_factory);

    let seed_config = PeerSeedsConfig {
        peer_seeds: seed_peers
            .iter()
            .map(|p| format!("{}::{}", p.public_key, p.addresses.best().unwrap().address()))
            .collect::<Vec<String>>()
            .into(),
        ..PeerSeedsConfig::default()
    };

    let fut = StackBuilder::new(shutdown_signal)
        .add_initializer(P2pInitializer::new(
            config.clone(),
            seed_config,
            network,
            node_identity,
            publisher,
        ))
        .add_initializer(LivenessInitializer::new(
            LivenessConfig {
                auto_ping_interval: Some(Duration::from_secs(1)),
                num_peers_per_round: 0,       // No random peers
                max_allowed_ping_failures: 0, // Peer with failed ping-pong will never be removed
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
    for peer in seed_peers {
        peer_manager.add_peer(peer).await?;
    }

    let comms = spawn_comms_using_transport(comms, config.transport.clone())
        .await
        .unwrap();
    handles.register(comms);

    let comms = handles.expect_handle::<CommsNode>();
    let contacts = handles.expect_handle::<ContactsServiceHandle>();
    Ok((contacts, comms))
}
