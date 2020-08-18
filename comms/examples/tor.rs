use bytes::Bytes;
use chrono::Utc;
use futures::{
    channel::{mpsc, mpsc::SendError},
    SinkExt,
    StreamExt,
};
use rand::{rngs::OsRng, thread_rng, RngCore};
use std::{collections::HashMap, convert::identity, env, net::SocketAddr, path::Path, process, sync::Arc};
use tari_comms::{
    message::{InboundMessage, OutboundMessage},
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, NodeIdentityError, Peer, PeerFeatures, PeerManagerError},
    pipeline,
    pipeline::SinkService,
    tor,
    CommsBuilder,
    CommsBuilderError,
    CommsNode,
};
use tari_crypto::tari_utilities::message_format::MessageFormat;
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tempfile::Builder;
use thiserror::Error;
use tokio::{runtime, task};

// Tor example for tari_comms.
//
// _Note:_ A running tor proxy with `ControlPort` set is required for this example to work.

#[derive(Debug, Error)]
enum Error {
    #[error("Invalid control port address provided. USAGE: tor_example 'Tor_multiaddress'.")]
    InvalidArgControlPortAddress,
    #[error("NodeIdentityError: {0}")]
    NodeIdentityError(#[from] NodeIdentityError),
    #[error("HiddenServiceBuilderError: {0}")]
    HiddenServiceBuilderError(#[from] tor::HiddenServiceBuilderError),
    #[error("CommsBuilderError: {0}")]
    CommsBuilderError(#[from] CommsBuilderError),
    #[error("PeerManagerError: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Failed to send message")]
    SendError(#[from] SendError),
    #[error("JoinError: {0}")]
    JoinError(#[from] task::JoinError),
    #[error("{0}")]
    Custom(String),
}

#[tokio_macros::main]
async fn main() {
    env_logger::init();
    if let Err(err) = run().await {
        eprintln!("{error:?}: {error}", error = err);
        process::exit(1);
    }
}

fn load_tor_identity<P: AsRef<Path>>(path: P) -> tor::TorIdentity {
    let contents = std::fs::read_to_string(path).unwrap();
    tor::TorIdentity::from_json(&contents).unwrap()
}

async fn run() -> Result<(), Error> {
    let mut args_iter = env::args().skip(1);
    let control_port_addr = args_iter
        .next()
        .unwrap_or("/ip4/127.0.0.1/tcp/9051".to_string())
        .parse::<Multiaddr>()
        .map_err(|_| Error::InvalidArgControlPortAddress)?;

    let tor_identity1 = args_iter.next().map(load_tor_identity);
    let tor_identity2 = args_iter.next().map(load_tor_identity);

    println!("Starting comms nodes...",);

    let temp_dir1 = Builder::new().prefix("tor-example1").tempdir().unwrap();
    let (comms_node1, inbound_rx1, mut outbound_tx1) = setup_node_with_tor(
        control_port_addr.clone(),
        temp_dir1.as_ref(),
        (9098u16, "127.0.0.1:0".parse::<SocketAddr>().unwrap()),
        tor_identity1,
    )
    .await?;

    let temp_dir2 = Builder::new().prefix("tor-example2").tempdir().unwrap();
    let (comms_node2, inbound_rx2, outbound_tx2) = setup_node_with_tor(
        control_port_addr,
        temp_dir2.as_ref(),
        (9099u16, "127.0.0.1:0".parse::<SocketAddr>().unwrap()),
        tor_identity2,
    )
    .await?;

    let node_identity1 = comms_node1.node_identity();
    let node_identity2 = comms_node2.node_identity();

    println!("Comms nodes started!");
    println!(
        "Node 1 is '{}' with address '{}' (local_listening_addr='{}')",
        node_identity1.node_id().short_str(),
        node_identity1.public_address(),
        comms_node1.listening_address(),
    );
    println!(
        "Node 2 is '{}' with address '{}' (local_listening_addr='{}')",
        node_identity2.node_id().short_str(),
        node_identity2.public_address(),
        comms_node2.listening_address(),
    );

    // Let's add node 2 as a peer to node 1
    comms_node1
        .peer_manager()
        .add_peer(Peer::new(
            node_identity2.public_key().clone(),
            node_identity2.node_id().clone(),
            vec![node_identity2.public_address()].into(),
            Default::default(),
            PeerFeatures::COMMUNICATION_CLIENT,
            &[],
            Default::default(),
        ))
        .await?;

    // This kicks things off
    outbound_tx1
        .send(OutboundMessage::new(
            comms_node2.node_identity().node_id().clone(),
            Bytes::from_static(b"START"),
        ))
        .await?;

    let executor = runtime::Handle::current();

    println!("Starting ping pong between nodes over tor. This may take a few moments to begin.");
    let handle1 = executor.spawn(start_ping_ponger(
        comms_node2.node_identity().node_id().clone(),
        inbound_rx1,
        outbound_tx1,
    ));
    let handle2 = executor.spawn(start_ping_ponger(
        comms_node1.node_identity().node_id().clone(),
        inbound_rx2,
        outbound_tx2,
    ));

    tokio::signal::ctrl_c().await.expect("ctrl-c failed");

    println!("Tor example is shutting down...");
    comms_node1.shutdown().await;
    comms_node2.shutdown().await;

    handle1.await??;
    handle2.await??;

    Ok(())
}

async fn setup_node_with_tor<P: Into<tor::PortMapping>>(
    control_port_addr: Multiaddr,
    database_path: &Path,
    port_mapping: P,
    tor_identity: Option<tor::TorIdentity>,
) -> Result<(CommsNode, mpsc::Receiver<InboundMessage>, mpsc::Sender<OutboundMessage>), Error>
{
    let datastore = LMDBBuilder::new()
        .set_path(database_path.to_str().unwrap())
        .set_environment_size(50)
        .set_max_number_of_databases(1)
        .add_database("peerdb", lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&"peerdb").unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    let (inbound_tx, inbound_rx) = mpsc::channel(10);
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let mut hs_builder = tor::HiddenServiceBuilder::new()
        .with_port_mapping(port_mapping)
        .with_control_server_address(control_port_addr);

    if let Some(ident) = tor_identity {
        hs_builder = hs_builder.with_tor_identity(ident);
    }

    let hs_controller = hs_builder.finish().await?;

    let node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        PeerFeatures::COMMUNICATION_CLIENT,
    )?);

    let comms_node = CommsBuilder::new()
        .with_node_identity(node_identity)
        .configure_from_hidden_service(hs_controller)
        .await
        .unwrap()
        .with_peer_storage(peer_database)
        .build()
        .unwrap();

    let comms_node = comms_node
        .with_messaging_pipeline(
            pipeline::Builder::new()
            // Outbound messages will be forwarded "as is" to outbound messaging
            .with_outbound_pipeline(outbound_rx, identity)
            .max_concurrent_inbound_tasks(1)
            // Inbound messages will be forwarded "as is" to inbound_tx
            .with_inbound_pipeline(SinkService::new(inbound_tx))
            .finish(),
        )
        .spawn()
        .await?;

    println!(
        "Tor hidden service created with address '{}'",
        comms_node.node_identity().public_address()
    );

    Ok((comms_node, inbound_rx, outbound_tx))
}

async fn start_ping_ponger(
    dest_node_id: NodeId,
    mut inbound_rx: mpsc::Receiver<InboundMessage>,
    mut outbound_tx: mpsc::Sender<OutboundMessage>,
) -> Result<usize, Error>
{
    let mut inflight_pings = HashMap::new();
    let mut counter = 0;
    while let Some(msg) = inbound_rx.next().await {
        counter += 1;

        let msg_str = String::from_utf8_lossy(&msg.body);
        println!("Received '{}' from '{}'", msg_str, msg.source_peer.short_str());

        let mut msg_parts = msg_str.split(' ');
        match msg_parts.next() {
            Some("START") => {
                println!("\n-----------------------------------");
                let id = thread_rng().next_u64();
                inflight_pings.insert(id, Utc::now().naive_utc());
                let msg = make_msg(&dest_node_id, format!("PING {}", id));
                outbound_tx.send(msg).await?;
            },
            Some("PING") => {
                let id = msg_parts
                    .next()
                    .ok_or(Error::Custom("Received PING without id".to_string()))?;

                let msg_str = format!("PONG {}", id);
                let msg = make_msg(&dest_node_id, msg_str);

                outbound_tx.send(msg).await?;
            },

            Some("PONG") => {
                let id = msg_parts
                    .next()
                    .ok_or(Error::Custom("Received PING without id".to_string()))?;

                id.parse::<u64>()
                    .ok()
                    .and_then(|id_num| inflight_pings.remove(&id_num))
                    .and_then(|ts| Some((Utc::now().naive_utc().signed_duration_since(ts)).num_milliseconds()))
                    .and_then(|latency| {
                        println!("Latency: {}ms", latency);
                        Some(latency)
                    });

                println!("-----------------------------------");
                let new_id = thread_rng().next_u64();
                inflight_pings.insert(new_id, Utc::now().naive_utc());
                let msg = make_msg(&dest_node_id, format!("PING {}", new_id));
                outbound_tx.send(msg).await?;
            },
            msg => {
                return Err(Error::Custom(format!("Received invalid message '{:?}'", msg)));
            },
        }
    }

    Ok(counter)
}

fn make_msg(node_id: &NodeId, msg: String) -> OutboundMessage {
    let msg = Bytes::copy_from_slice(msg.as_bytes());
    OutboundMessage::new(node_id.clone(), msg)
}
