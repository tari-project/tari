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
const NUM_NODES: usize = 15;
// Must be at least 2
const NUM_WALLETS: usize = 2;

use futures::{channel::mpsc, future, StreamExt};
use lazy_static::lazy_static;
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
    transports::MemoryTransport,
    types::CommsDatabase,
    CommsBuilder,
    CommsNode,
    ConnectionManagerEvent,
    PeerConnection,
};
use tari_comms_dht::{envelope::NodeDestination, inbound::DecryptedDhtMessage, Dht, DhtBuilder};
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tari_test_utils::{paths::create_temporary_data_path, random};
use tokio::{runtime, time};
use tower::ServiceBuilder;

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

#[tokio_macros::main_basic]
async fn main() {
    env_logger::init();

    banner!(
        "Bringing up virtual network consisting of a seed node, {} nodes and {} wallets",
        NUM_NODES,
        NUM_WALLETS
    );

    let mut seed_node = make_node(PeerFeatures::COMMUNICATION_NODE, None).await;

    let mut nodes = future::join_all(
        repeat_with(|| make_node(PeerFeatures::COMMUNICATION_NODE, Some(seed_node.to_peer()))).take(NUM_NODES),
    )
    .await;

    let mut wallets = future::join_all(
        repeat_with(|| {
            make_node(
                PeerFeatures::COMMUNICATION_CLIENT,
                Some(nodes[OsRng.gen_range(0, NUM_NODES - 1)].to_peer()),
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

    let first_wallet = wallets.get(0).unwrap();
    let last_wallet = wallets.last().unwrap();

    banner!("Now, '{}' is going to try discover '{}'.", first_wallet, last_wallet);

    let start = Instant::now();
    let discovery_result = first_wallet
        .dht
        .discovery_service_requester()
        .discover_peer(
            Box::new(last_wallet.node_identity().public_key().clone()),
            None,
            NodeDestination::Unknown,
        )
        .await;

    let end = Instant::now();
    banner!("Discovery is done.");

    match discovery_result {
        Ok(peer) => {
            println!(
                "⚡️🎉😎 '{}' discovered peer '{}' ({}) in {}ms",
                first_wallet,
                get_name(&peer.node_id),
                peer,
                (end - start).as_millis()
            );
        },
        Err(err) => {
            println!(
                "💩 '{}' failed to discover '{}' after {}ms because '{:?}'",
                first_wallet,
                last_wallet,
                (end - start).as_millis(),
                err
            );
        },
    }

    banner!("We're done here. Network is shutting down...");

    shutdown_all(nodes).await;
    shutdown_all(wallets).await;
}

async fn shutdown_all(nodes: Vec<TestNode>) {
    let tasks = nodes.into_iter().map(|node| async move {
        let node_name = node.name;
        node.comms.shutdown().await;
        println!("'{}' is shut down", node_name);
    });
    future::join_all(tasks).await;
}

fn event_logger(node_name: String) -> impl FnMut(Arc<ConnectionManagerEvent>) -> Arc<ConnectionManagerEvent> {
    move |event| {
        use ConnectionManagerEvent::*;
        print!("EVENT: ");
        match &*event {
            PeerConnected(conn) => match conn.direction() {
                ConnectionDirection::Inbound => {
                    println!(
                        "'{}' got inbound connection from '{}'",
                        node_name,
                        get_name(conn.peer_node_id()),
                    );
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
    events_rx: mpsc::Receiver<Arc<ConnectionManagerEvent>>,
    _ims_rx: mpsc::Receiver<DecryptedDhtMessage>,
}

impl TestNode {
    pub fn new(
        comms: CommsNode,
        dht: Dht,
        seed_peer: Option<Peer>,
        ims_rx: mpsc::Receiver<DecryptedDhtMessage>,
    ) -> Self
    {
        let name = get_next_name();
        register_name(comms.node_identity().node_id().clone(), name.clone());

        let (events_tx, events_rx) = mpsc::channel(100);
        Self::spawn_event_monitor(&comms, events_tx);

        Self {
            name,
            seed_peer,
            comms,
            dht,
            _ims_rx: ims_rx,
            events_rx,
        }
    }

    fn spawn_event_monitor(comms: &CommsNode, events_tx: mpsc::Sender<Arc<ConnectionManagerEvent>>) {
        let subscription = comms.subscribe_connection_manager_events();
        runtime::Handle::current().spawn(
            subscription
                .filter(|r| future::ready(r.is_ok()))
                .map(Result::unwrap)
                .map(event_logger(get_name(comms.node_identity().node_id())))
                .map(Ok)
                .forward(events_tx),
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
            let event = time::timeout(Duration::from_secs(10), self.events_rx.next())
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

async fn make_node(features: PeerFeatures, seed_peer: Option<Peer>) -> TestNode {
    let node_identity = make_node_identity(features);

    let (tx, ims_rx) = mpsc::channel(1);
    let (comms, dht) = setup_comms_dht(
        node_identity,
        create_peer_storage(seed_peer.clone().into_iter().collect()),
        tx,
    )
    .await;

    TestNode::new(comms, dht, seed_peer, ims_rx)
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
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(100)))
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
