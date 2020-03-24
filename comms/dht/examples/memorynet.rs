// Copyright 2020, The Tari Project
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

//! # MemoryNet
//!
//! This example runs a small in-memory network.
//! It's primary purpose is to test and debug the behaviour of the DHT.
//!
//! The following happens:
//! 1. A single "seed node", `NUM_NODES` "base nodes" and `NUM_WALLETS` "wallets" are generated and started.
//! 1. All "base nodes" join the network via the "seed node"
//! 1. All "wallets" join the network via a random "base node"
//! 1. The first "wallet" in the list attempts to discover the last "wallet" in the list
//!
//! The suggested way to run this is:
//!
//! `RUST_BACKTRACE=1 RUST_LOG=trace cargo run --example memorynet 2> /tmp/debug.log`

// Size of network
const NUM_NODES: usize = 40;
// Must be at least 2
const NUM_WALLETS: usize = 8;

mod memory_net;

use futures::{channel::mpsc, future, StreamExt};
use lazy_static::lazy_static;
use memory_net::DrainBurst;
use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    fmt,
    iter::repeat_with,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tari_comms::{
    backoff::ConstantBackoff,
    connection_manager::ConnectionDirection,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerStorage},
    pipeline,
    pipeline::SinkService,
    protocol::messaging::MessagingEvent,
    transports::MemoryTransport,
    types::CommsDatabase,
    CommsBuilder,
    CommsNode,
    ConnectionManagerEvent,
    PeerConnection,
};
use tari_comms_dht::{envelope::NodeDestination, inbound::DecryptedDhtMessage, Dht, DhtBuilder};
use tari_crypto::tari_utilities::ByteArray;
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tari_test_utils::{paths::create_temporary_data_path, random};
use tokio::{runtime, time};
use tower::ServiceBuilder;

type MessagingEventRx = mpsc::UnboundedReceiver<(NodeId, NodeId)>;
type MessagingEventTx = mpsc::UnboundedSender<(NodeId, NodeId)>;

macro_rules! banner {
    ($($arg: tt)*) => {
        println!();
        println!("----------------------------------------------------------");
        println!($($arg)*);
        println!("----------------------------------------------------------");
        println!();
    }
}

const NAMES: &[&str] = &[
    "Alice", "Bob", "Carol", "Charlie", "Dave", "Eve", "Isaac", "Ivan", "Justin", "Mallory", "Marvin", "Mallet",
    "Matilda", "Oscar", "Pat", "Peggy", "Vanna", "Plod", "Steve", "Trent", "Trudy", "Walter", "Zoe",
];

lazy_static! {
    static ref NAME_MAP: Mutex<HashMap<NodeId, String>> = Mutex::new(HashMap::new());
    static ref NAME_POS: Mutex<usize> = Mutex::new(0);
}

fn register_name(node_id: NodeId, name: String) {
    NAME_MAP.lock().unwrap().insert(node_id, name);
}

fn get_name(node_id: &NodeId) -> String {
    NAME_MAP
        .lock()
        .unwrap()
        .get(node_id)
        .map(|name| format!("{} ({})", name, node_id.short_str()))
        .unwrap_or_else(|| format!("NoName ({})", node_id.short_str()))
}

fn get_short_name(node_id: &NodeId) -> String {
    NAME_MAP
        .lock()
        .unwrap()
        .get(node_id)
        .map(|name| format!("{}", name))
        .unwrap_or_else(|| format!("NoName ({})", node_id.short_str()))
}

fn get_next_name() -> String {
    let pos = {
        let mut i = NAME_POS.lock().unwrap();
        *i = *i + 1;
        *i
    };
    if pos > NAMES.len() {
        format!("Node {}", pos - NAMES.len())
    } else {
        NAMES[pos - 1].to_owned()
    }
}

#[tokio_macros::main]
async fn main() {
    env_logger::init();

    banner!(
        "Bringing up virtual network consisting of a seed node, {} nodes and {} wallets",
        NUM_NODES,
        NUM_WALLETS
    );

    let (messaging_events_tx, mut messaging_events_rx) = mpsc::unbounded();

    let mut seed_node = make_node(PeerFeatures::COMMUNICATION_NODE, None, messaging_events_tx.clone()).await;

    let mut nodes = future::join_all(
        repeat_with(|| {
            make_node(
                PeerFeatures::COMMUNICATION_NODE,
                Some(seed_node.to_peer()),
                messaging_events_tx.clone(),
            )
        })
        .take(NUM_NODES),
    )
    .await;

    let mut wallets = future::join_all(
        repeat_with(|| {
            make_node(
                PeerFeatures::COMMUNICATION_CLIENT,
                Some(nodes[OsRng.gen_range(0, NUM_NODES - 1)].to_peer()),
                messaging_events_tx.clone(),
            )
        })
        .take(NUM_WALLETS),
    )
    .await;

    for node in nodes.iter_mut() {
        println!(
            "Node '{}' is joining the network via the seed node '{}'",
            node, seed_node
        );
        node.dht.dht_requester().send_join().await.unwrap();

        seed_node.expect_peer_connection(&node.get_node_id()).await.unwrap();
        println!();
    }

    take_a_break().await;

    peer_list_summary(&nodes).await;

    banner!(
        "Now, {} wallets are going to join from a random base node.",
        NUM_WALLETS
    );

    for wallet in wallets.iter_mut() {
        println!(
            "Wallet '{}' is joining the network via node '{}'",
            wallet,
            get_name(&wallet.seed_peer.as_ref().unwrap().node_id)
        );
        wallet.dht.dht_requester().send_join().await.unwrap();
        let seed_node_id = &wallet.seed_peer.as_ref().unwrap().node_id;
        nodes
            .iter_mut()
            .find(|n| &n.get_node_id() == seed_node_id)
            .expect("node must exist")
            .expect_peer_connection(&wallet.get_node_id())
            .await
            .unwrap();
        println!();
    }

    peer_list_summary(&wallets).await;

    drain_messaging_events(&mut messaging_events_rx, false).await;
    take_a_break().await;
    drain_messaging_events(&mut messaging_events_rx, false).await;

    discovery(&wallets, &mut messaging_events_rx, false, false).await;

    take_a_break().await;
    drain_messaging_events(&mut messaging_events_rx, true).await;

    discovery(&wallets, &mut messaging_events_rx, true, false).await;

    take_a_break().await;
    drain_messaging_events(&mut messaging_events_rx, true).await;

    discovery(&wallets, &mut messaging_events_rx, false, true).await;

    banner!("That's it folks! Network is shutting down...");

    shutdown_all(nodes).await;
    shutdown_all(wallets).await;
}

async fn shutdown_all(nodes: Vec<TestNode>) {
    let tasks = nodes.into_iter().map(|node| node.comms.shutdown());
    future::join_all(tasks).await;
}

async fn discovery(
    wallets: &[TestNode],
    messaging_events_rx: &mut MessagingEventRx,
    use_network_region: bool,
    use_destination_node_id: bool,
)
{
    let mut successes = 0;
    for i in 0..wallets.len() - 1 {
        let wallet1 = wallets.get(i).unwrap();
        let wallet2 = wallets.get(i + 1).unwrap();

        banner!("Now, '{}' is going to try discover '{}'.", wallet1, wallet2);

        let mut destination = NodeDestination::Unknown;
        if use_network_region {
            let mut new_node_id = [0; 13];
            let node_id = wallet2.get_node_id();
            let buf = &mut new_node_id[..10];
            buf.copy_from_slice(&node_id.as_bytes()[..10]);
            let regional_node_id = NodeId::from_bytes(&new_node_id).unwrap();
            destination = NodeDestination::NodeId(Box::new(regional_node_id));
        }

        let mut node_id_dest = None;
        if use_destination_node_id {
            node_id_dest = Some(wallet2.get_node_id());
        }

        let start = Instant::now();
        let discovery_result = wallet1
            .dht
            .discovery_service_requester()
            .discover_peer(
                Box::new(wallet2.node_identity().public_key().clone()),
                node_id_dest,
                destination,
            )
            .await;

        let end = Instant::now();
        banner!("Discovery is done.");

        match discovery_result {
            Ok(peer) => {
                successes += 1;
                println!(
                    "⚡️🎉😎 '{}' discovered peer '{}' ({}) in {}ms",
                    wallet1,
                    get_name(&peer.node_id),
                    peer,
                    (end - start).as_millis()
                );

                println!();
                time::delay_for(Duration::from_secs(5)).await;
                drain_messaging_events(messaging_events_rx, false).await;
            },
            Err(err) => {
                println!(
                    "💩 '{}' failed to discover '{}' after {}ms because '{:?}'",
                    wallet1,
                    wallet2,
                    (end - start).as_millis(),
                    err
                );

                println!();
                time::delay_for(Duration::from_secs(5)).await;
                drain_messaging_events(messaging_events_rx, true).await;
            },
        }
    }

    banner!(
        "✨ The set of discoveries succeeded {}% of the time.",
        (successes as f32 / wallets.len() as f32) * 100.0
    );
}

async fn peer_list_summary(network: &[TestNode]) {
    for node in network {
        let peers = node.comms.peer_manager().all().await.unwrap();
        println!("-----------------------------------------");
        println!("{} knows {} peer(s):", node, peers.len());
        println!(
            "  {}",
            peers
                .iter()
                .map(|p| &p.node_id)
                .map(get_name)
                .collect::<Vec<_>>()
                .join("\n")
        );
        println!("-----------------------------------------");
    }
}

async fn drain_messaging_events(messaging_rx: &mut MessagingEventRx, show_logs: bool) {
    let drain_fut = DrainBurst::new(messaging_rx);
    if show_logs {
        let messages = drain_fut.await;
        let num_messages = messages.len();
        let mut node_id_buf = Vec::new();
        let mut last_from_node = None;
        for (from_node, to_node) in &messages {
            match &last_from_node {
                Some(node_id) if *node_id == from_node => {
                    node_id_buf.push(to_node);
                },
                Some(_) => {
                    println!(
                        "📨 {} sent {} messages to {}️",
                        get_short_name(last_from_node.take().unwrap()),
                        node_id_buf.len(),
                        node_id_buf.drain(..).map(get_short_name).collect::<Vec<_>>().join(", ")
                    );
                },
                None => {
                    last_from_node = Some(from_node);
                    node_id_buf.push(to_node)
                },
            }
        }
        println!("{} messages sent between nodes", num_messages);
    } else {
        let _ = drain_fut.await;
    }
}

fn connection_manager_logger(
    node_id: NodeId,
) -> impl FnMut(Arc<ConnectionManagerEvent>) -> Arc<ConnectionManagerEvent> {
    let node_name = get_name(&node_id);
    move |event| {
        use ConnectionManagerEvent::*;
        print!("EVENT: ");
        match &*event {
            PeerConnected(conn) => match conn.direction() {
                ConnectionDirection::Inbound => {
                    // println!(
                    //     "'{}' got inbound connection from '{}'",
                    //     node_name,
                    //     get_name(conn.peer_node_id()),
                    // );
                },
                ConnectionDirection::Outbound => {
                    println!("'{}' connected to '{}'", node_name, get_name(conn.peer_node_id()),);
                },
            },
            PeerDisconnected(node_id) => {
                println!("'{}' disconnected from '{}'", get_name(node_id), node_name);
            },
            PeerConnectFailed(node_id, err) => {
                println!(
                    "'{}' failed to connect to '{}' because '{:?}'",
                    get_name(node_id),
                    node_name,
                    err
                );
            },
            PeerConnectWillClose(_, node_id, direction) => {
                println!(
                    "'{}' will disconnect {} connection to '{}'",
                    get_name(node_id),
                    direction,
                    node_name,
                );
            },
            PeerInboundConnectFailed(err) => {
                println!(
                    "'{}' failed to accept inbound connection because '{:?}'",
                    node_name, err
                );
            },
            Listening(_) | ListenFailed(_) => unreachable!(),
            NewInboundSubstream(node_id, protocol, _) => {
                println!(
                    "'{}' negotiated protocol '{}' to '{}'",
                    get_name(node_id),
                    String::from_utf8_lossy(protocol),
                    node_name
                );
            },
        }
        event
    }
}

struct TestNode {
    name: String,
    comms: CommsNode,
    seed_peer: Option<Peer>,
    dht: Dht,
    conn_man_events_rx: mpsc::Receiver<Arc<ConnectionManagerEvent>>,
    _ims_rx: mpsc::Receiver<DecryptedDhtMessage>,
}

impl TestNode {
    pub fn new(
        comms: CommsNode,
        dht: Dht,
        seed_peer: Option<Peer>,
        ims_rx: mpsc::Receiver<DecryptedDhtMessage>,
        messaging_events_tx: MessagingEventTx,
    ) -> Self
    {
        let name = get_next_name();
        register_name(comms.node_identity().node_id().clone(), name.clone());

        let (conn_man_events_tx, events_rx) = mpsc::channel(100);
        Self::spawn_event_monitor(&comms, conn_man_events_tx, messaging_events_tx);

        Self {
            name,
            seed_peer,
            comms,
            dht,
            _ims_rx: ims_rx,
            conn_man_events_rx: events_rx,
        }
    }

    fn spawn_event_monitor(
        comms: &CommsNode,
        events_tx: mpsc::Sender<Arc<ConnectionManagerEvent>>,
        messaging_events_tx: MessagingEventTx,
    )
    {
        let conn_man_event_sub = comms.subscribe_connection_manager_events();
        let messaging_events = comms.subscribe_messaging_events();
        let spawner = runtime::Handle::current();

        spawner.spawn(
            conn_man_event_sub
                .filter(|r| future::ready(r.is_ok()))
                .map(Result::unwrap)
                .map(connection_manager_logger(comms.node_identity().node_id().clone()))
                .map(Ok)
                .forward(events_tx),
        );

        let node_id = comms.node_identity().node_id().clone();

        spawner.spawn(
            messaging_events
                .filter(|r| future::ready(r.is_ok()))
                .map(Result::unwrap)
                .filter_map(move |event| {
                    use MessagingEvent::*;
                    future::ready(match &*event {
                        MessageReceived(peer_node_id, _) => Some((Clone::clone(&**peer_node_id), node_id.clone())),
                        _ => None,
                    })
                })
                .map(Ok)
                .forward(messaging_events_tx),
        );
    }

    #[inline]
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        self.comms.node_identity()
    }

    #[inline]
    pub fn get_node_id(&self) -> NodeId {
        self.node_identity().node_id().clone()
    }

    #[inline]
    pub fn to_peer(&self) -> Peer {
        self.comms.node_identity().to_peer()
    }

    pub async fn expect_peer_connection(&mut self, node_id: &NodeId) -> Option<PeerConnection> {
        use ConnectionManagerEvent::*;
        loop {
            let event = time::timeout(Duration::from_secs(10), self.conn_man_events_rx.next())
                .await
                .ok()??;

            match &*event {
                PeerConnected(conn) if conn.peer_node_id() == node_id => {
                    break Some(conn.clone());
                },
                _ => {},
            }
        }
    }
}

impl fmt::Display for TestNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

fn make_node_identity(features: PeerFeatures) -> Arc<NodeIdentity> {
    let port = MemoryTransport::acquire_next_memsocket_port();
    Arc::new(NodeIdentity::random(&mut OsRng, format!("/memory/{}", port).parse().unwrap(), features).unwrap())
}

fn create_peer_storage(peers: Vec<Peer>) -> CommsDatabase {
    let database_name = random::string(8);
    let datastore = LMDBBuilder::new()
        .set_path(create_temporary_data_path().to_str().unwrap())
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(&database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(&database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));
    let mut storage = PeerStorage::new_indexed(peer_database).unwrap();
    for peer in peers {
        storage.add_peer(peer).unwrap();
    }

    storage.into()
}

async fn make_node(features: PeerFeatures, seed_peer: Option<Peer>, messaging_events_tx: MessagingEventTx) -> TestNode {
    let node_identity = make_node_identity(features);

    let (tx, ims_rx) = mpsc::channel(1);
    let (comms, dht) = setup_comms_dht(
        node_identity,
        create_peer_storage(seed_peer.clone().into_iter().collect()),
        tx,
    )
    .await;

    TestNode::new(comms, dht, seed_peer, ims_rx, messaging_events_tx)
}

async fn setup_comms_dht(
    node_identity: Arc<NodeIdentity>,
    storage: CommsDatabase,
    inbound_tx: mpsc::Sender<DecryptedDhtMessage>,
) -> (CommsNode, Dht)
{
    // Create inbound and outbound channels
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let comms = CommsBuilder::new()
        .allow_test_addresses()
        // In this case the listener address and the public address are the same (/memory/...)
        .with_listener_address(node_identity.public_address())
        .with_transport(MemoryTransport)
        .with_node_identity(node_identity)
        .with_peer_storage(storage)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(1000)))
        .build()
        .unwrap();

    let dht = DhtBuilder::new(
        comms.node_identity(),
        comms.peer_manager(),
        outbound_tx,
        comms.shutdown_signal(),
    )
    .local_test()
    .with_discovery_timeout(Duration::from_secs(60))
    .with_num_neighbouring_nodes(8)
    .finish();

    let dht_outbound_layer = dht.outbound_middleware_layer();

    let comms = comms
        .with_messaging_pipeline(
            pipeline::Builder::new()
                .outbound_buffer_size(10)
                .with_outbound_pipeline(outbound_rx, |sink| {
                    ServiceBuilder::new().layer(dht_outbound_layer).service(sink)
                })
                .max_concurrent_inbound_tasks(5)
                .with_inbound_pipeline(
                    ServiceBuilder::new()
                        .layer(dht.inbound_middleware_layer())
                        .service(SinkService::new(inbound_tx)),
                )
                .finish(),
        )
        .spawn()
        .await
        .unwrap();

    (comms, dht)
}

async fn take_a_break() {
    banner!("Taking a break for a few seconds to let things settle...");
    time::delay_for(Duration::from_millis(NUM_NODES as u64 * 500)).await;
}
