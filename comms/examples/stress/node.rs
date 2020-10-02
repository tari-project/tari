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

use super::{error::Error, STRESS_PROTOCOL_NAME, TOR_CONTROL_PORT_ADDR, TOR_SOCKS_ADDR};
use futures::channel::mpsc;
use rand::rngs::OsRng;
use std::{convert, net::Ipv4Addr, path::Path, sync::Arc, time::Duration};
use tari_comms::{
    backoff::ConstantBackoff,
    message::{InboundMessage, OutboundMessage},
    multiaddr::Multiaddr,
    pipeline,
    pipeline::SinkService,
    protocol::{messaging::MessagingProtocolExtension, ProtocolNotification, Protocols},
    tor,
    tor::{HsFlags, TorIdentity},
    transports::{SocksConfig, TcpWithTorTransport},
    CommsBuilder,
    CommsNode,
    NodeIdentity,
    Substream,
};
use tari_storage::{
    lmdb_store::{LMDBBuilder, LMDBConfig},
    LMDBWrapper,
};
use tokio::sync::broadcast;

pub async fn create(
    node_identity: Option<Arc<NodeIdentity>>,
    database_path: &Path,
    public_ip: Option<Ipv4Addr>,
    port: u16,
    tor_identity: Option<TorIdentity>,
    is_tcp: bool,
) -> Result<
    (
        CommsNode,
        mpsc::Receiver<ProtocolNotification<Substream>>,
        mpsc::Receiver<InboundMessage>,
        mpsc::Sender<OutboundMessage>,
    ),
    Error,
>
{
    let datastore = LMDBBuilder::new()
        .set_path(database_path.to_str().unwrap())
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(1)
        .add_database("peerdb", lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&"peerdb").unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    let mut protocols = Protocols::new();
    let (proto_notif_tx, proto_notif_rx) = mpsc::channel(1);
    protocols.add(&[STRESS_PROTOCOL_NAME.clone()], proto_notif_tx);

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
        .map(|ni| {
            ni.set_public_address(public_addr.clone());
            ni
        })
        .unwrap_or_else(|| Arc::new(NodeIdentity::random(&mut OsRng, public_addr, Default::default()).unwrap()));

    let listener_addr = format!("/ip4/0.0.0.0/tcp/{}", port).parse().unwrap();

    let builder = CommsBuilder::new()
        .allow_test_addresses()
        .with_node_identity(node_identity.clone())
        .with_dial_backoff(ConstantBackoff::new(Duration::from_secs(0)))
        .with_peer_storage(peer_database)
        .with_listener_liveness_max_sessions(10)
        .disable_connection_reaping();

    let (inbound_tx, inbound_rx) = mpsc::channel(100);
    let (outbound_tx, outbound_rx) = mpsc::channel(100);
    let (event_tx, _) = broadcast::channel(1);

    let comms_node = if is_tcp {
        builder
            .with_listener_address(listener_addr)
            .build()?
            .add_protocol_extensions(protocols.into())
            .add_protocol_extension(MessagingProtocolExtension::new(
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
            }))
            .await
            .unwrap()
    } else {
        let mut hs_builder = tor::HiddenServiceBuilder::new()
            .with_hs_flags(HsFlags::DETACH)
            .with_port_mapping(port)
            .with_control_server_address(TOR_CONTROL_PORT_ADDR.parse().unwrap());

        if let Some(tor_identity) = tor_identity {
            println!("Set tor identity from file");
            hs_builder = hs_builder.with_tor_identity(tor_identity);
        }

        let mut hs_ctl = hs_builder.build().await?;
        let transport = hs_ctl.initialize_transport().await?;

        builder
            .with_listener_address(hs_ctl.proxied_address())
            .build()?
            .add_protocol_extensions(protocols.into())
            .add_protocol_extension(MessagingProtocolExtension::new(
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
