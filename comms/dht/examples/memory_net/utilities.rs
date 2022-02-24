// Copyright 2020. The Tari Project
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
#![allow(clippy::mutex_atomic)]

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use futures::future;
use lazy_static::lazy_static;
use rand::{rngs::OsRng, Rng};
use tari_comms::{
    backoff::ConstantBackoff,
    connection_manager::{ConnectionDirection, ConnectionManagerEvent},
    connectivity::ConnectivitySelection,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerStorage},
    pipeline,
    pipeline::SinkService,
    protocol::{
        messaging::{MessagingEvent, MessagingEventReceiver, MessagingEventSender, MessagingProtocolExtension},
        rpc::RpcServer,
    },
    transports::MemoryTransport,
    types::CommsDatabase,
    CommsBuilder,
    CommsNode,
    PeerConnection,
};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    inbound::DecryptedDhtMessage,
    outbound::OutboundEncryption,
    store_forward::SafConfig,
    Dht,
    DhtConfig,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_storage::{
    lmdb_store::{LMDBBuilder, LMDBConfig},
    LMDBWrapper,
};
use tari_test_utils::{paths::create_temporary_data_path, random, streams::convert_unbounded_mpsc_to_stream};
use tokio::{
    runtime,
    sync::{broadcast, mpsc},
    task,
    time,
};
use tower::ServiceBuilder;

use crate::memory_net::DrainBurst;

pub type NodeEventRx = mpsc::UnboundedReceiver<(NodeId, NodeId)>;
pub type NodeEventTx = mpsc::UnboundedSender<(NodeId, NodeId)>;

#[macro_export]
macro_rules! banner {
    ($($arg: tt)*) => {
        println!();
        println!("----------------------------------------------------------");
        println!($($arg)*);
        println!("----------------------------------------------------------");
        println!();
    }
}

lazy_static! {
    static ref NAME_MAP: Mutex<HashMap<NodeId, String>> = Mutex::new(HashMap::new());
    static ref NAME_POS: Mutex<usize> = Mutex::new(0);
}

pub fn register_name(node_id: NodeId, name: String) {
    NAME_MAP.lock().unwrap().insert(node_id, name);
}

pub fn get_name(node_id: &NodeId) -> String {
    NAME_MAP
        .lock()
        .unwrap()
        .get(node_id)
        .map(|name| format!("{} ({})", name, node_id.short_str()))
        .unwrap_or_else(|| format!("NoName ({})", node_id.short_str()))
}

pub fn get_short_name(node_id: &NodeId) -> String {
    NAME_MAP
        .lock()
        .unwrap()
        .get(node_id)
        .map(|name| name.to_string())
        .unwrap_or_else(|| format!("NoName ({})", node_id.short_str()))
}

pub fn get_next_name() -> String {
    let pos = {
        let mut i = NAME_POS.lock().unwrap();
        *i += 1;
        *i - 1
    };

    format!("Node{}", pos)
}

pub async fn shutdown_all(nodes: Vec<TestNode>) {
    let tasks = nodes.into_iter().map(|node| node.shutdown());
    future::join_all(tasks).await;
}

pub async fn discovery(wallets: &[TestNode], messaging_events_rx: &mut NodeEventRx) -> (usize, usize, usize) {
    let mut successes = 0;
    let mut total_messages = 0;
    let mut total_time = Duration::from_secs(0);
    for i in 0..wallets.len() - 1 {
        let wallet1 = wallets.get(i).unwrap();
        let wallet2 = wallets.get(i + 1).unwrap();

        banner!("🌎 '{}' is going to try discover '{}'.", wallet1, wallet2);

        let start = Instant::now();
        let discovery_result = wallet1
            .dht
            .discovery_service_requester()
            .discover_peer(
                wallet2.node_identity().public_key().clone(),
                wallet2.node_identity().node_id().clone().into(),
            )
            .await;

        match discovery_result {
            Ok(peer) => {
                successes += 1;
                total_time += start.elapsed();
                banner!(
                    "⚡️🎉😎 '{}' discovered peer '{}' ({}) in {:.2?}",
                    wallet1,
                    get_name(&peer.node_id),
                    peer,
                    start.elapsed()
                );

                time::sleep(Duration::from_secs(5)).await;
                total_messages += drain_messaging_events(messaging_events_rx, false).await;
            },
            Err(err) => {
                banner!(
                    "💩 '{}' failed to discover '{}' after {:.2?} because '{}'",
                    wallet1,
                    wallet2,
                    start.elapsed(),
                    err
                );

                time::sleep(Duration::from_secs(5)).await;
                total_messages += drain_messaging_events(messaging_events_rx, false).await;
            },
        }
    }

    banner!(
        "✨ The set of discoveries succeeded {} out of {} times and took a total of {:.1}s with {} messages sent.",
        successes,
        wallets.len() - 1,
        total_time.as_secs_f32(),
        total_messages
    );
    (total_messages, successes, wallets.len() - 1)
}

pub async fn network_peer_list_stats(nodes: &[TestNode], wallets: &[TestNode]) {
    let mut stats = HashMap::<String, usize>::with_capacity(wallets.len());
    for wallet in wallets {
        let mut num_known = 0;
        for node in nodes {
            if node
                .comms
                .peer_manager()
                .exists_node_id(wallet.node_identity().node_id())
                .await
            {
                num_known += 1;
            }
        }
        stats.insert(get_name(wallet.node_identity().node_id()), num_known);
    }

    let mut avg = Vec::with_capacity(wallets.len());
    for (n, v) in stats {
        let perc = v as f32 / nodes.len() as f32;
        avg.push(perc);
        println!(
            "{} is known by {} out of {} nodes ({:.2}%)",
            n,
            v,
            nodes.len(),
            perc * 100.0
        );
    }
    println!(
        "Average {:.2}%",
        avg.into_iter().sum::<f32>() / wallets.len() as f32 * 100.0
    );
}

pub async fn network_connectivity_stats(nodes: &[TestNode], wallets: &[TestNode], quiet_mode: bool) {
    pub async fn display(nodes: &[TestNode], quiet_mode: bool) -> (usize, usize) {
        let mut total = 0;
        let mut avg = Vec::new();
        for node in nodes {
            let conns = node.comms.connectivity().get_active_connections().await.unwrap();
            total += conns.len();
            avg.push(conns.len());

            println!("{} connected to {} nodes", node, conns.len());
            if !quiet_mode {
                for c in conns {
                    println!("  {} ({})", get_name(c.peer_node_id()), c.direction());
                }
            }
        }
        (total, avg.into_iter().sum())
    }
    let (mut total, mut avg) = display(nodes, quiet_mode).await;
    let (t, a) = display(wallets, quiet_mode).await;
    total += t;
    avg += a;
    println!(
        "{} total connections on the network. ({} per peer on average)",
        total,
        avg / (wallets.len() + nodes.len())
    );
}

pub async fn do_network_wide_propagation(nodes: &mut [TestNode], origin_node_index: Option<usize>) -> (usize, usize) {
    let random_node = match origin_node_index {
        Some(n) if n < nodes.len() => &nodes[n],
        Some(_) | None => &nodes[OsRng.gen_range(0..nodes.len() - 1)],
    };

    let random_node_id = random_node.comms.node_identity().node_id().clone();
    const PUBLIC_MESSAGE: &str = "This is something you're all interested in!";

    banner!("🌎 {} is going to broadcast a message to the network", random_node);
    let send_states = random_node
        .dht
        .outbound_requester()
        .broadcast(
            NodeDestination::Unknown,
            OutboundEncryption::ClearText,
            vec![],
            OutboundDomainMessage::new(0i32, PUBLIC_MESSAGE.to_string()),
        )
        .await
        .unwrap();
    let num_connections = random_node
        .comms
        .connectivity()
        .get_active_connections()
        .await
        .unwrap()
        .len();
    let (success, failed) = send_states.wait_all().await;
    println!(
        "🦠 {} broadcast to {}/{} peer(s) ({} connection(s))",
        random_node.name,
        success.len(),
        success.len() + failed.len(),
        num_connections
    );

    let start_global = Instant::now();
    // Spawn task for each peer that will read the message and propagate it on
    let tasks = nodes
        .iter_mut()
        .filter(|n| n.comms.node_identity().node_id() != &random_node_id)
        .enumerate()
        .map(|(idx, node)| {
            let mut outbound_req = node.dht.outbound_requester();
            let mut connectivity = node.comms.connectivity();
            let mut ims_rx = node.ims_rx.take().unwrap();
            let start = Instant::now();
            let start_global = start_global;
            let node_name = node.name.clone();

            task::spawn(async move {
                let result = time::timeout(Duration::from_secs(30), ims_rx.recv()).await;
                let mut is_success = false;
                match result {
                    Ok(Some(msg)) => {
                        let public_msg = msg
                            .decryption_result
                            .unwrap()
                            .decode_part::<String>(1)
                            .unwrap()
                            .unwrap();
                        println!(
                            "📬 {} got public message '{}' (t={:.0?})",
                            node_name,
                            public_msg,
                            start_global.elapsed(),
                        );
                        is_success = true;
                        let send_states = outbound_req
                            .propagate(
                                NodeDestination::Unknown,
                                OutboundEncryption::ClearText,
                                vec![msg.source_peer.node_id.clone()],
                                OutboundDomainMessage::new(0i32, public_msg),
                            )
                            .await
                            .unwrap();
                        let num_connections = connectivity.get_active_connections().await.unwrap().len();
                        let (success, failed) = send_states.wait_all().await;
                        println!(
                            "🦠 {} propagated to {}/{} peer(s) ({} connection(s))",
                            node_name,
                            success.len(),
                            success.len() + failed.len(),
                            num_connections
                        );
                    },
                    Err(_) | Ok(None) => {
                        banner!(
                            "💩 {} failed to receive network message after {:.2?}",
                            node_name,
                            start.elapsed(),
                        );
                    },
                }

                (idx, ims_rx, is_success)
            })
        });

    // Put the ims_rxs back
    let ims_rxs = future::join_all(tasks).await;
    let mut num_successes = 0;
    for ims in ims_rxs {
        let (idx, ims_rx, is_success) = ims.unwrap();
        nodes[idx].ims_rx = Some(ims_rx);
        if is_success {
            num_successes += 1;
        }
    }

    banner!(
        "🙌 Finished propagation test. {} out of {} nodes received the message",
        num_successes,
        nodes.len() - 1
    );
    (num_successes, nodes.len() - 1)
}

pub async fn do_store_and_forward_message_propagation(
    wallet: TestNode,
    wallets: &[TestNode],
    nodes: &[TestNode],
    messaging_tx: NodeEventTx,
    messaging_rx: &mut NodeEventRx,
    num_neighbouring_nodes: usize,
    num_random_nodes: usize,
    propagation_factor: usize,
    quiet_mode: bool,
) -> (usize, TestNode, usize, usize) {
    banner!(
        "{} chosen at random to be receive messages from other nodes using store and forward",
        wallet,
    );
    let wallets_peers = wallet.comms.peer_manager().all().await.unwrap();
    let node_identity = wallet.comms.node_identity().clone();

    let neighbours = wallet
        .comms
        .connectivity()
        .select_connections(ConnectivitySelection::closest_to(
            wallet.node_identity().node_id().clone(),
            num_neighbouring_nodes,
            vec![],
        ))
        .await
        .unwrap()
        .into_iter()
        .filter_map(
            // If a node is not found in the node list it must be the seed node - should probably assert that this is
            // the case
            |p| nodes.iter().find(|n| n.node_identity().node_id() == p.peer_node_id()),
        )
        .collect::<Vec<_>>();

    let neighbour_subs = neighbours.iter().map(|n| n.messaging_events.subscribe());

    banner!(
        "{} has {} neighbours ({})",
        wallet,
        neighbours.len(),
        neighbours
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    banner!("😴 {} is going offline", wallet);
    wallet.shutdown().await;

    banner!(
        "🎤 All other wallets are going to attempt to broadcast messages to {} ({})",
        get_name(node_identity.node_id()),
        node_identity.public_key(),
    );

    let start = Instant::now();
    for wallet in wallets {
        let secret_message = format!("My name is wiki wiki {}", wallet);
        let send_states = wallet
            .dht
            .outbound_requester()
            .closest_broadcast(
                node_identity.node_id().clone(),
                OutboundEncryption::encrypt_for(node_identity.public_key().clone()),
                vec![],
                OutboundDomainMessage::new(123i32, secret_message.clone()),
            )
            .await
            .unwrap();
        let (success, failed) = send_states.wait_all().await;
        println!(
            "{} sent {}/{} messages",
            wallet,
            success.len(),
            success.len() + failed.len(),
        );
    }

    for (idx, mut s) in neighbour_subs.into_iter().enumerate() {
        let neighbour = neighbours[idx].name.clone();
        task::spawn(async move {
            let msg = time::timeout(Duration::from_secs(2), s.recv()).await;
            match msg {
                Ok(Ok(evt)) => {
                    if let MessagingEvent::MessageReceived(_, tag) = &*evt {
                        println!("{} received propagated SAF message ({})", neighbour, tag);
                    }
                },
                Ok(Err(err)) => {
                    println!("{}", err);
                },
                Err(_) => println!("{} did not receive the SAF message", neighbour),
            }
        });
    }

    banner!("⏰ Waiting a few seconds for messages to propagate around the network...");
    time::sleep(Duration::from_secs(5)).await;

    let mut total_messages = drain_messaging_events(messaging_rx, false).await;

    banner!("🤓 {} is coming back online", get_name(node_identity.node_id()));
    let (tx, ims_rx) = mpsc::channel(1);
    let shutdown = Shutdown::new();
    let (comms, dht, messaging_events) = setup_comms_dht(
        node_identity,
        create_peer_storage(),
        tx,
        num_neighbouring_nodes,
        num_random_nodes,
        propagation_factor,
        wallets_peers,
        true,
        shutdown.to_signal(),
    )
    .await;
    let mut wallet = TestNode::new(
        comms,
        dht,
        vec![],
        ims_rx,
        messaging_tx,
        messaging_events,
        quiet_mode,
        shutdown,
    );
    let mut connectivity = wallet.comms.connectivity();

    connectivity
        .wait_for_connectivity(Duration::from_secs(10))
        .await
        .unwrap();
    take_a_break(nodes.len()).await;
    let connections = wallet.comms.connectivity().get_active_connections().await.unwrap();
    println!(
        "{} has {} connections to {}",
        wallet,
        connections.len(),
        connections
            .iter()
            .map(|p| get_name(p.peer_node_id()))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut num_msgs = 0;
    let mut succeeded = 0;
    loop {
        let result = time::timeout(Duration::from_secs(10), wallet.ims_rx.as_mut().unwrap().recv()).await;
        num_msgs += 1;
        match result {
            Ok(msg) => {
                let msg = msg.unwrap();
                let secret_msg = msg
                    .decryption_result
                    .unwrap()
                    .decode_part::<String>(1)
                    .unwrap()
                    .unwrap();
                banner!(
                    "🎉 Wallet {} received propagated message '{}' from store and forward in {:.2?}",
                    wallet,
                    secret_msg,
                    start.elapsed()
                );
                succeeded += 1;
            },
            Err(err) => {
                banner!(
                    "💩 Failed to receive message after {:.0?} using store and forward '{}'",
                    start.elapsed(),
                    err
                );
            },
        };

        if num_msgs == wallets.len() {
            break;
        }
    }

    total_messages += drain_messaging_events(messaging_rx, false).await;

    (total_messages, wallet, succeeded, num_msgs)
}

pub async fn drain_messaging_events(messaging_rx: &mut NodeEventRx, show_logs: bool) -> usize {
    let stream = convert_unbounded_mpsc_to_stream(messaging_rx);
    tokio::pin!(stream);
    let drain_fut = DrainBurst::new(&mut stream);
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
                        "📨 {} sent {} messages to {}.️",
                        get_short_name(last_from_node.take().unwrap()),
                        node_id_buf.len(),
                        node_id_buf.drain(..).map(get_short_name).collect::<Vec<_>>().join(", ")
                    );

                    last_from_node = Some(from_node);
                    node_id_buf.push(to_node)
                },
                None => {
                    last_from_node = Some(from_node);
                    node_id_buf.push(to_node)
                },
            }
        }
        println!("{} messages sent between nodes", num_messages);
        num_messages
    } else {
        let len = drain_fut.await.len();
        println!("📨 {} messages exchanged", len);
        len
    }
}

fn connection_manager_logger(
    node_id: NodeId,
    quiet_mode: bool,
) -> impl FnMut(Arc<ConnectionManagerEvent>) -> Arc<ConnectionManagerEvent> {
    let node_name = get_name(&node_id);
    move |event| {
        if quiet_mode {
            return event;
        }
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
            PeerDisconnected(_, node_id) => {
                println!("'{}' disconnected from '{}'", get_name(node_id), node_name);
            },
            PeerConnectFailed(node_id, err) => {
                println!(
                    "'{}' failed to connect to '{}' because '{:?}'",
                    node_name,
                    get_name(node_id),
                    err
                );
            },
            PeerInboundConnectFailed(err) => {
                println!(
                    "'{}' failed to accept inbound connection because '{:?}'",
                    node_name, err
                );
            },
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

pub struct TestNode {
    pub name: String,
    pub comms: CommsNode,
    pub seed_peers: Vec<Peer>,
    pub dht: Dht,
    pub conn_man_events_rx: mpsc::Receiver<Arc<ConnectionManagerEvent>>,
    pub ims_rx: Option<mpsc::Receiver<DecryptedDhtMessage>>,
    pub messaging_events: MessagingEventSender,
    pub shutdown: Shutdown,
}

impl TestNode {
    pub fn new(
        comms: CommsNode,
        dht: Dht,
        seed_peers: Vec<Peer>,
        ims_rx: mpsc::Receiver<DecryptedDhtMessage>,
        node_messsage_tx: NodeEventTx,
        messaging_events: MessagingEventSender,
        quiet_mode: bool,
        shutdown: Shutdown,
    ) -> Self {
        let name = get_next_name();
        register_name(comms.node_identity().node_id().clone(), name.clone());

        let (conn_man_events_tx, events_rx) = mpsc::channel(100);
        Self::spawn_event_monitor(
            &comms,
            messaging_events.subscribe(),
            conn_man_events_tx,
            node_messsage_tx,
            quiet_mode,
        );

        Self {
            name,
            seed_peers,
            comms,
            dht,
            ims_rx: Some(ims_rx),
            conn_man_events_rx: events_rx,
            messaging_events,
            shutdown,
        }
    }

    fn spawn_event_monitor(
        comms: &CommsNode,
        mut messaging_events: MessagingEventReceiver,
        events_tx: mpsc::Sender<Arc<ConnectionManagerEvent>>,
        messaging_events_tx: NodeEventTx,
        quiet_mode: bool,
    ) {
        let mut conn_man_event_sub = comms.subscribe_connection_manager_events();
        let executor = runtime::Handle::current();

        let node_id = comms.node_identity().node_id().clone();
        executor.spawn(async move {
            let mut logger = connection_manager_logger(node_id, quiet_mode);
            loop {
                match conn_man_event_sub.recv().await {
                    Ok(event) => {
                        let _ = events_tx.send(logger(event)).await;
                    },
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(err) => log::error!("{}", err),
                }
            }
        });

        let node_id = comms.node_identity().node_id().clone();
        executor.spawn(async move {
            loop {
                let event = messaging_events.recv().await;
                use MessagingEvent::*;
                match event.as_deref() {
                    Ok(MessageReceived(peer_node_id, _)) => {
                        messaging_events_tx
                            .send((Clone::clone(&*peer_node_id), node_id.clone()))
                            .unwrap();
                    },
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    },
                    _ => {},
                }
            }
        });
    }

    #[inline]
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        self.comms.node_identity()
    }

    #[inline]
    pub fn to_peer(&self) -> Peer {
        self.comms.node_identity().to_peer()
    }

    #[allow(dead_code)]
    pub async fn expect_peer_connection(&mut self, node_id: &NodeId) -> Option<PeerConnection> {
        if let Some(conn) = self.comms.connectivity().get_connection(node_id.clone()).await.unwrap() {
            return Some(conn);
        }
        use ConnectionManagerEvent::*;
        loop {
            let event = time::timeout(Duration::from_secs(30), self.conn_man_events_rx.recv())
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

    pub async fn shutdown(mut self) {
        self.shutdown.trigger();
        self.comms.wait_until_shutdown().await;
    }
}

impl AsRef<TestNode> for TestNode {
    fn as_ref(&self) -> &TestNode {
        self
    }
}

impl fmt::Display for TestNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} ({})",
            self.name,
            self.comms.node_identity().node_id().short_str()
        )
    }
}

pub fn make_node_identity(features: PeerFeatures) -> Arc<NodeIdentity> {
    let port = MemoryTransport::acquire_next_memsocket_port();
    Arc::new(NodeIdentity::random(
        &mut OsRng,
        format!("/memory/{}", port).parse().unwrap(),
        features,
    ))
}

fn create_peer_storage() -> CommsDatabase {
    let database_name = random::string(8);
    let datastore = LMDBBuilder::new()
        .set_path(create_temporary_data_path().to_str().unwrap())
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(1)
        .add_database(&database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(&database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));
    PeerStorage::new_indexed(peer_database).unwrap().into()
}

pub async fn make_node(
    features: PeerFeatures,
    peer_identities: Vec<Arc<NodeIdentity>>,
    node_event_tx: NodeEventTx,
    num_neighbouring_nodes: usize,
    num_random_nodes: usize,
    propagation_factor: usize,
    quiet_mode: bool,
) -> TestNode {
    let node_identity = make_node_identity(features);
    make_node_from_node_identities(
        node_identity,
        peer_identities,
        node_event_tx,
        num_neighbouring_nodes,
        num_random_nodes,
        propagation_factor,
        quiet_mode,
    )
    .await
}

pub async fn make_node_from_node_identities(
    node_identity: Arc<NodeIdentity>,
    peer_identities: Vec<Arc<NodeIdentity>>,
    node_events_tx: NodeEventTx,
    num_neighbouring_nodes: usize,
    num_random_nodes: usize,
    propagation_factor: usize,
    quiet_mode: bool,
) -> TestNode {
    let (tx, ims_rx) = mpsc::channel(1);
    let seed_peers = peer_identities.iter().map(|n| n.to_peer()).collect::<Vec<_>>();
    let shutdown = Shutdown::new();
    let (comms, dht, messaging_events) = setup_comms_dht(
        node_identity,
        create_peer_storage(),
        tx,
        num_neighbouring_nodes,
        num_random_nodes,
        propagation_factor,
        seed_peers.clone(),
        false,
        shutdown.to_signal(),
    )
    .await;

    TestNode::new(
        comms,
        dht,
        seed_peers,
        ims_rx,
        node_events_tx,
        messaging_events,
        quiet_mode,
        shutdown,
    )
}

async fn setup_comms_dht(
    node_identity: Arc<NodeIdentity>,
    storage: CommsDatabase,
    inbound_tx: mpsc::Sender<DecryptedDhtMessage>,
    num_neighbouring_nodes: usize,
    num_random_nodes: usize,
    propagation_factor: usize,
    seed_peers: Vec<Peer>,
    saf_auto_request: bool,
    shutdown_signal: ShutdownSignal,
) -> (CommsNode, Dht, MessagingEventSender) {
    // Create inbound and outbound channels
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let comms = CommsBuilder::new()
        .allow_test_addresses()
        // In this case the listener address and the public address are the same (/memory/...)
        .with_listener_address(node_identity.public_address())
        .with_shutdown_signal(shutdown_signal)
        .with_node_identity(node_identity)
        .with_min_connectivity(1)
        .with_peer_storage(storage,None)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(1000)))
        .build()
        .unwrap();
    for peer in seed_peers {
        comms.peer_manager().add_peer(peer).await.unwrap();
    }

    let dht = Dht::builder()
        .with_config(DhtConfig {
            saf_config: SafConfig {
                auto_request: saf_auto_request,
                ..Default::default()
            },
            auto_join: false,
            discovery_request_timeout: Duration::from_secs(15),
            num_neighbouring_nodes,
            num_random_nodes,
            propagation_factor,
            network_discovery: Default::default(),
            ..DhtConfig::default_local_test()
        })
        .with_outbound_sender(outbound_tx)
        .build(
            comms.node_identity(),
            comms.peer_manager(),
            comms.connectivity(),
            comms.shutdown_signal(),
        )
        .await
        .unwrap();

    let dht_outbound_layer = dht.outbound_middleware_layer();
    let pipeline = pipeline::Builder::new()
        .outbound_buffer_size(10)
        .with_outbound_pipeline(outbound_rx, |sink| {
            ServiceBuilder::new().layer(dht_outbound_layer).service(sink)
        })
        .max_concurrent_inbound_tasks(10)
        .with_inbound_pipeline(
            ServiceBuilder::new()
                .layer(dht.inbound_middleware_layer())
                .service(SinkService::new(inbound_tx)),
        )
        .build();

    let (messaging_events_tx, _) = broadcast::channel(100);
    let comms = comms
        .add_rpc_server(RpcServer::new().add_service(dht.rpc_service()))
        .add_protocol_extension(MessagingProtocolExtension::new(messaging_events_tx.clone(), pipeline))
        .spawn_with_transport(MemoryTransport)
        .await
        .unwrap();

    (comms, dht, messaging_events_tx)
}

pub async fn take_a_break(num_nodes: usize) {
    banner!("Taking a break for a few seconds to let things settle...");
    time::sleep(Duration::from_millis(num_nodes as u64 * 100)).await;
}
