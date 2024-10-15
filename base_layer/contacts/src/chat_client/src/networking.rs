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

use tari_contacts::contacts_service::{handle::ContactsServiceHandle, ContactsServiceInitializer};
use tari_network::{identity, NetworkHandle, Peer};
use tari_p2p::{
    connector::InboundMessaging,
    initialization::P2pInitializer,
    message::TariNodeMessageSpec,
    peer_seeds::SeedPeer,
    services::liveness::{LivenessConfig, LivenessInitializer},
    Dispatcher,
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
    node_identity: Arc<identity::Keypair>,
    config: ApplicationConfig,
    shutdown_signal: ShutdownSignal,
    user_agent: String,
) -> Result<(ContactsServiceHandle, NetworkHandle), NetworkingError> {
    create_chat_storage(&config.chat_client.db_file)?;
    let backend = connect_to_db(config.chat_client.db_file)?;

    let dispatcher = Dispatcher::new();

    let p2p_config = config.chat_client.p2p.clone();

    let fut = StackBuilder::new(shutdown_signal)
        .add_initializer(P2pInitializer::new(
            p2p_config.clone(),
            user_agent,
            config.peer_seeds.clone(),
            config.chat_client.network,
            node_identity,
        ))
        .add_initializer(LivenessInitializer::new(
            LivenessConfig {
                auto_ping_interval: Some(config.chat_client.metadata_auto_ping_interval),
                ..Default::default()
            },
            dispatcher.clone(),
        ))
        .add_initializer(ContactsServiceInitializer::new(
            backend,
            dispatcher.clone(),
            Duration::from_secs(5),
            2,
        ))
        .build();

    let mut handles = fut.await?;

    let inbound = handles
        .take_handle::<InboundMessaging<TariNodeMessageSpec>>()
        .expect("Inbound messaging not registered");
    dispatcher.spawn(inbound);

    let network = handles.expect_handle::<NetworkHandle>();

    let seed_peers = config
        .peer_seeds
        .peer_seeds
        .iter()
        .map(|s| SeedPeer::from_str(s))
        .map(|r| r.map(Peer::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| NetworkingError::PeerSeeds(e.to_string()))?;

    for peer in seed_peers {
        network.add_peer(peer).await?;
    }

    let contacts = handles.expect_handle::<ContactsServiceHandle>();
    Ok((contacts, network))
}
