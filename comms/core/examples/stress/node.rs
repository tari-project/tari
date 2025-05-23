//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    convert,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use rand::rngs::OsRng;
use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};
use tari_comms::{
    backoff::ConstantBackoff,
    message::{InboundMessage, OutboundMessage},
    multiaddr::Multiaddr,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{
        database::{PeerDatabaseSql, MIGRATIONS},
        NodeId,
        Peer,
        PeerFeatures,
        PeerFlags,
    },
    pipeline,
    pipeline::SinkService,
    protocol::{messaging::MessagingProtocolExtension, ProtocolId, ProtocolNotification, Protocols},
    tor,
    tor::TorIdentity,
    transports::{predicate::FalsePredicate, SocksConfig, TcpWithTorTransport},
    types::CommsPublicKey,
    CommsBuilder,
    CommsNode,
    NodeIdentity,
    Substream,
};
use tari_shutdown::ShutdownSignal;
use tokio::sync::{broadcast, mpsc};

use super::{error::Error, STRESS_PROTOCOL_NAME, TOR_CONTROL_PORT_ADDR, TOR_SOCKS_ADDR};

static MSG_PROTOCOL_ID: ProtocolId = ProtocolId::from_static(b"example/msg/1.0");

pub fn create_test_peer() -> Peer {
    let mut rng = rand::rngs::OsRng;
    let (_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
    let node_id = NodeId::from_key(&pk);
    let addresses = MultiaddressesWithStats::from_addresses_with_source(
        vec!["/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap()],
        &PeerAddressSource::Config,
    );
    Peer::new(
        pk,
        node_id,
        addresses,
        PeerFlags::default(),
        PeerFeatures::empty(),
        Default::default(),
        Default::default(),
    )
}

pub async fn create(
    node_identity: Option<Arc<NodeIdentity>>,
    database_path: &Path,
    public_ip: Option<Ipv4Addr>,
    port: u16,
    tor_identity: Option<TorIdentity>,
    is_tcp: bool,
    shutdown_signal: ShutdownSignal,
) -> Result<
    (
        CommsNode,
        mpsc::Receiver<ProtocolNotification<Substream>>,
        mpsc::Receiver<InboundMessage>,
        mpsc::UnboundedSender<OutboundMessage>,
    ),
    Error,
> {
    let database_url = DbConnectionUrl::File(PathBuf::from(database_path).join("peers.db"));
    let db_connection = DbConnection::connect_and_migrate(&database_url, MIGRATIONS, Some(5))?;
    let this_node = if let Some(node) = node_identity.as_ref() {
        node.to_peer()
    } else {
        create_test_peer()
    };
    let peer_database = PeerDatabaseSql::new(db_connection, &this_node)?;

    let mut protocols = Protocols::new();
    let (proto_notif_tx, proto_notif_rx) = mpsc::channel(1);
    protocols.add(&[STRESS_PROTOCOL_NAME.clone()], &proto_notif_tx);

    let public_addr = format!(
        "/ip4/{}/tcp/{}",
        public_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "0.0.0.0".to_string()),
        port
    )
    .parse::<Multiaddr>()
    .unwrap();
    let node_identity = node_identity
        .inspect(|ni| {
            ni.add_public_address(public_addr.clone());
        })
        .unwrap_or_else(|| Arc::new(NodeIdentity::random(&mut OsRng, public_addr, Default::default())));

    let listener_addr = format!("/ip4/0.0.0.0/tcp/{}", port).parse().unwrap();

    let builder = CommsBuilder::new()
        .allow_test_addresses()
        .with_shutdown_signal(shutdown_signal)
        .with_node_identity(node_identity.clone())
        .with_dial_backoff(ConstantBackoff::new(Duration::from_secs(0)))
        .with_peer_storage(peer_database)
        .with_listener_liveness_max_sessions(10)
        .disable_connection_reaping();

    let (inbound_tx, inbound_rx) = mpsc::channel(100);
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
    let (event_tx, _) = broadcast::channel(1);

    let comms_node = if is_tcp {
        builder
            .with_listener_address(listener_addr)
            .build()?
            .add_protocol_extensions(protocols.into())
            .add_protocol_extension(MessagingProtocolExtension::new(
                MSG_PROTOCOL_ID.clone(),
                event_tx,
                pipeline::Builder::new()
                    .with_inbound_pipeline(SinkService::new(inbound_tx))
                    .max_concurrent_inbound_tasks(100)
                    .with_outbound_pipeline(outbound_rx, convert::identity)
                    .build(),
            ))
            .spawn_with_transport(TcpWithTorTransport::with_tor_socks_proxy(SocksConfig {
                proxy_address: TOR_SOCKS_ADDR.parse().unwrap(),
                authentication: Default::default(),
                proxy_bypass_predicate: Arc::new(FalsePredicate::new()),
            }))
            .await
            .unwrap()
    } else {
        let mut hs_builder = tor::HiddenServiceBuilder::new()
            .with_port_mapping(port)
            .with_control_server_address(TOR_CONTROL_PORT_ADDR.parse().unwrap());

        if let Some(tor_identity) = tor_identity {
            println!("Set tor identity from file");
            hs_builder = hs_builder.with_tor_identity(tor_identity);
        }

        let mut hs_ctl = hs_builder.build()?;
        let transport = hs_ctl.initialize_transport().await?;

        builder
            .with_listener_address(hs_ctl.proxied_address())
            .build()?
            .with_hidden_service_controller(hs_ctl)
            .add_protocol_extensions(protocols.into())
            .add_protocol_extension(MessagingProtocolExtension::new(
                MSG_PROTOCOL_ID.clone(),
                event_tx,
                pipeline::Builder::new()
                    .with_inbound_pipeline(SinkService::new(inbound_tx))
                    .max_concurrent_inbound_tasks(100)
                    .with_outbound_pipeline(outbound_rx, convert::identity)
                    .build(),
            ))
            .spawn_with_transport(transport)
            .await
            .unwrap()
    };

    Ok((comms_node, proto_notif_rx, inbound_rx, outbound_tx))
}
