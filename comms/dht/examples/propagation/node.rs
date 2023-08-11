//  Copyright 2022, The Tari Project
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

use std::{path::Path, sync::Arc, time::Duration};

use rand::rngs::OsRng;
use tari_comms::{
    backoff::ConstantBackoff,
    peer_manager::PeerFeatures,
    pipeline,
    pipeline::SinkService,
    protocol::{messaging::MessagingProtocolExtension, NodeNetworkInfo},
    tor,
    tor::TorIdentity,
    CommsBuilder,
    CommsNode,
    NodeIdentity,
};
use tari_comms_dht::{inbound::DecryptedDhtMessage, DbConnectionUrl, Dht};
use tari_shutdown::ShutdownSignal;
use tari_storage::{
    lmdb_store::{LMDBBuilder, LMDBConfig},
    LMDBWrapper,
};
use tokio::sync::{broadcast, mpsc};
use tower::ServiceBuilder;

use crate::parse_from_short_str;

pub const TOR_CONTROL_PORT_ADDR: &str = "/ip4/127.0.0.1/tcp/9051";

pub async fn create<P: AsRef<Path>>(
    node_identity: Option<Arc<NodeIdentity>>,
    database_path: P,
    tor_identity: Option<TorIdentity>,
    onion_port: u16,
    seed_peers: &[&str],
    shutdown_signal: ShutdownSignal,
) -> anyhow::Result<(CommsNode, Dht, mpsc::Receiver<DecryptedDhtMessage>)> {
    let datastore = LMDBBuilder::new()
        .set_path(database_path.as_ref())
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(1)
        .add_database("peerdb", lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle("peerdb").unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    let node_identity = node_identity.unwrap_or_else(|| {
        Arc::new(NodeIdentity::random_multiple_addresses(
            &mut OsRng,
            vec![],
            Default::default(),
        ))
    });

    let builder = CommsBuilder::new()
        .allow_test_addresses()
        .with_network_byte(0x25)
        .with_shutdown_signal(shutdown_signal)
        .with_node_info(NodeNetworkInfo {
            major_version: 0,
            minor_version: 0,
            network_byte: 0x25,
            user_agent: "/tari/propagator/0.0.1".to_string(),
        })
        .with_node_identity(node_identity.clone())
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(500)))
        .with_peer_storage(peer_database, None)
        .with_listener_liveness_max_sessions(10)
        .disable_connection_reaping();

    let (inbound_tx, inbound_rx) = mpsc::channel(1);
    let (outbound_tx, outbound_rx) = mpsc::channel(1);
    let (event_tx, _) = broadcast::channel(1);

    let mut hs_builder = tor::HiddenServiceBuilder::new()
        .with_port_mapping(onion_port)
        .with_control_server_address(TOR_CONTROL_PORT_ADDR.parse().unwrap());

    if let Some(tor_identity) = tor_identity {
        println!("Set tor identity from file");
        hs_builder = hs_builder.with_tor_identity(tor_identity);
    }

    let mut hs_ctl = hs_builder.build()?;
    let transport = hs_ctl.initialize_transport().await?;

    let comms_node = builder.with_listener_address(hs_ctl.proxied_address()).build()?;

    let dht = tari_comms_dht::Dht::builder()
        .with_database_url(DbConnectionUrl::File(database_path.as_ref().join("dht.sqlite")))
        .set_auto_store_and_forward_requests(false)
        .with_outbound_sender(outbound_tx)
        .enable_auto_join()
        .build(
            node_identity.clone(),
            comms_node.peer_manager(),
            comms_node.connectivity(),
            comms_node.shutdown_signal(),
        )
        .await?;

    let peer_manager = comms_node.peer_manager();
    for peer in seed_peers {
        peer_manager
            .add_peer(parse_from_short_str(peer, PeerFeatures::COMMUNICATION_NODE).unwrap())
            .await?;
    }

    let dht_outbound_layer = dht.outbound_middleware_layer();
    let comms_node = comms_node
        .with_hidden_service_controller(hs_ctl)
        .add_protocol_extension(MessagingProtocolExtension::new(
            event_tx,
            pipeline::Builder::new()
                .with_inbound_pipeline(
                    ServiceBuilder::new()
                        .layer(dht.inbound_middleware_layer())
                        .service(SinkService::new(inbound_tx)),
                )
                .max_concurrent_inbound_tasks(1)
                .max_concurrent_outbound_tasks(1)
                .with_outbound_pipeline(outbound_rx, |sink| {
                    ServiceBuilder::new().layer(dht_outbound_layer).service(sink)
                })
                .build(),
        ))
        .spawn_with_transport(transport)
        .await?;

    Ok((comms_node, dht, inbound_rx))
}
