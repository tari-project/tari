//  Copyright 2019 The Tari Project
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

// Required to use futures::select! macro
#![recursion_limit = "256"]

/// A basic ncurses UI that sends ping and receives pong messages to a single peer using the `tari_p2p` library.
/// Press 'p' to send a ping.

#[macro_use]
extern crate lazy_static;

use clap::{App, Arg};
use cursive::{
    view::Identifiable,
    views::{Dialog, TextView},
    CbFunc,
    Cursive,
};
use futures::{
    channel::mpsc,
    stream::{FusedStream, StreamExt},
    Stream,
};
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{
    fs,
    iter,
    net::IpAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use tari_comms::{
    connection::net_address::ip::SocketAddress,
    control_service::ControlServiceConfig,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFlags},
};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{initialize_comms, CommsConfig},
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{LivenessConfig, LivenessEvent, LivenessHandle, LivenessInitializer},
    },
};
use tari_service_framework::StackBuilder;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_utilities::message_format::MessageFormat;
use tempdir::TempDir;
use tokio::runtime::Runtime;

fn load_identity(path: &str) -> NodeIdentity {
    let contents = fs::read_to_string(path).unwrap();
    NodeIdentity::from_json(contents.as_str()).unwrap()
}

pub fn random_string(len: usize) -> String {
    let mut rng = OsRng::new().unwrap();
    iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
}

fn get_lan_address() -> IpAddr {
    let interfaces = get_if_addrs::get_if_addrs().unwrap();
    interfaces
        .into_iter()
        .find(|if_addr| !if_addr.is_loopback())
        .and_then(|if_addr| Some(if_addr.ip()))
        .unwrap()
}

fn set_lan_address(identity: &NodeIdentity) {
    let addr = get_lan_address();
    let address = format!(
        "{}:{}",
        addr,
        identity.control_service_address().maybe_port().unwrap_or(0)
    )
    .parse()
    .unwrap();
    identity.set_control_service_address(address).unwrap();
}

fn main() {
    let matches = App::new("Tari comms peer to peer ping pong example")
        .version("1.0")
        .about("PingPong between two peers")
        .arg(
            Arg::with_name("node-identity")
                .value_name("FILE")
                .long("node-identity")
                .short("n")
                .help("The relative path of the node identity file to use for this node")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("peer-identity")
                .value_name("FILE")
                .long("remote-identity")
                .short("r")
                .help("The relative path of the node identity file of the other node")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("log-config")
                .value_name("FILE")
                .long("log-config")
                .short("l")
                .help("The relative path of the log config file of the other node")
                .takes_value(true)
                .default_value("base_layer/p2p/examples/example-log-config.yml"),
        )
        .arg(
            Arg::with_name("port")
                .value_name("PORT")
                .long("port")
                .short("p")
                .help("The port to use for the listener service (default:0 (os-assigned))")
                .takes_value(true)
                .default_value("0"),
        )
        .arg(
            Arg::with_name("socks proxy")
                .value_name("PROXYADDR")
                .long("proxy")
                .help("Use this proxy when connecting")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("local")
                .long("local")
                .help("Advertise local LAN address")
                .takes_value(false),
        )
        .get_matches();

    log4rs::init_file(matches.value_of("log-config").unwrap(), Default::default()).unwrap();

    let node_identity = Arc::new(load_identity(matches.value_of("node-identity").unwrap()));
    let peer_identity = load_identity(matches.value_of("peer-identity").unwrap());
    let port = matches.value_of("port").unwrap();
    let proxy = matches.value_of("socks proxy").and_then(|proxy| {
        if proxy.is_empty() {
            None
        } else {
            Some(proxy.parse::<SocketAddress>().unwrap())
        }
    });

    if matches.value_of("local").is_some() {
        set_lan_address(&node_identity);
        set_lan_address(&peer_identity);
    }

    let comms_config = CommsConfig {
        node_identity: Arc::clone(&node_identity),
        peer_connection_listening_address: format!("0.0.0.0:{}", port).parse().unwrap(),
        socks_proxy_address: proxy.clone(),
        control_service: ControlServiceConfig {
            listener_address: format!(
                "0.0.0.0:{}",
                node_identity.control_service_address().maybe_port().unwrap()
            )
            .parse()
            .unwrap(),
            socks_proxy_address: proxy,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        establish_connection_timeout: Duration::from_secs(10),
        datastore_path: TempDir::new(random_string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string(),
        peer_database_name: random_string(8),
        inbound_buffer_size: 10,
        outbound_buffer_size: 10,
        dht: Default::default(),
    };
    let rt = Runtime::new().expect("Failed to create tokio Runtime");

    let (publisher, subscription_factory) = pubsub_connector(rt.executor(), 100);
    let subscription_factory = Arc::new(subscription_factory);

    let (comms, dht) = initialize_comms(rt.executor(), comms_config, publisher).unwrap();

    let peer = Peer::new(
        peer_identity.public_key().clone(),
        peer_identity.node_id().clone(),
        peer_identity.control_service_address().into(),
        PeerFlags::empty(),
        peer_identity.features().clone(),
    );
    comms.peer_manager().add_peer(peer).unwrap();

    let fut = StackBuilder::new(rt.executor(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(LivenessInitializer::new(
            LivenessConfig {
                auto_ping_interval: Some(Duration::from_secs(5)), // None,
                enable_auto_stored_message_request: false,
                enable_auto_join: true,
                ..Default::default()
            },
            Arc::clone(&subscription_factory),
            dht.dht_requester(),
        ))
        .finish();

    let handles = rt.block_on(fut).expect("Service initialization failed");

    let mut app = setup_ui();

    // Updates the UI when pings/pongs are received
    let ui_update_signal = app.cb_sink().clone();
    let liveness_handle = handles.get_handle::<LivenessHandle>().unwrap();
    let mut shutdown = Shutdown::new();
    rt.spawn(update_ui(
        ui_update_signal,
        liveness_handle.clone(),
        shutdown.to_signal(),
    ));

    // Send pings when 'p' is pressed
    let (mut send_ping_tx, send_ping_rx) = mpsc::channel(10);
    app.add_global_callback('p', move |_| {
        let _ = send_ping_tx.start_send(());
    });

    let ui_update_signal = app.cb_sink().clone();
    let node_to_ping = peer_identity.node_id().clone();
    rt.spawn(send_ping_on_trigger(
        send_ping_rx.fuse(),
        ui_update_signal,
        liveness_handle,
        node_to_ping,
        shutdown.to_signal(),
    ));

    app.add_global_callback('q', |s| s.quit());
    app.run();

    shutdown.trigger().unwrap();
    comms.shutdown().unwrap();
    rt.shutdown_on_idle();
}

fn setup_ui() -> Cursive {
    let mut app = Cursive::default();

    app.add_layer(
        Dialog::around(TextView::new("Loading...").with_id("counter"))
            .title("PingPong")
            .button("Quit", |s| s.quit()),
    );

    app
}

#[derive(Default)]
struct UiState {
    num_pings_sent: usize,
    num_pings_recv: usize,
    num_pongs_recv: usize,
    avg_latency: u32,
}

lazy_static! {
    /// Used to keep track of the counts displayed in the UI
    /// (sent ping count, recv ping count, recv pong count)
    static ref UI_STATE: Arc<RwLock<UiState>> = Arc::new(Default::default());
}
type CursiveSignal = crossbeam_channel::Sender<Box<dyn CbFunc>>;

async fn update_ui(update_sink: CursiveSignal, mut liveness_handle: LivenessHandle, mut shutdown: ShutdownSignal) {
    let mut event_stream = liveness_handle.get_event_stream_fused();
    let ping_count = liveness_handle.get_ping_count().await.unwrap_or(0);
    let pong_count = liveness_handle.get_pong_count().await.unwrap_or(0);

    {
        let mut lock = UI_STATE.write().unwrap();
        *lock = UiState {
            num_pings_sent: lock.num_pings_sent,
            num_pings_recv: ping_count,
            num_pongs_recv: pong_count,
            avg_latency: 0,
        };
    }
    let _ = update_sink.send(Box::new(update_count));

    loop {
        ::futures::select! {
            event = event_stream.select_next_some() => {
                match &*event {
                    LivenessEvent::ReceivedPing => {
                        {
                            let mut lock = UI_STATE.write().unwrap();
                             *lock = UiState {
                                num_pings_sent: lock.num_pings_sent,
                                num_pings_recv: lock.num_pings_recv + 1,
                                num_pongs_recv: lock.num_pongs_recv,
                                avg_latency: lock.avg_latency,
                            };
                        }

                        let _ = update_sink.send(Box::new(update_count));
                    },
                    LivenessEvent::ReceivedPong(event) => {
                        {
                            let mut lock = UI_STATE.write().unwrap();
                            *lock = UiState {
                                num_pings_sent: lock.num_pings_sent,
                                num_pings_recv: lock.num_pings_recv,
                                num_pongs_recv: lock.num_pongs_recv + 1,
                                avg_latency: event.latency.unwrap_or(0),
                            };
                        }

                        let _ = update_sink.send(Box::new(update_count));
                    },
                    _ => {},
                }

            },
            _ = shutdown =>  {
                log::debug!("Ping pong example UI exiting");
                break;
            }
        }
    }
}
async fn send_ping_on_trigger(
    mut send_ping_trigger: impl Stream<Item = ()> + FusedStream + Unpin,
    update_signal: CursiveSignal,
    mut liveness_handle: LivenessHandle,
    node_to_ping: NodeId,
    mut shutdown: ShutdownSignal,
)
{
    loop {
        futures::select! {
            _ = send_ping_trigger.next() => {
                liveness_handle.send_ping(node_to_ping.clone()).await.unwrap();

                {
                    let mut lock = UI_STATE.write().unwrap();
                    *lock = UiState {
                        num_pings_sent: lock.num_pings_sent + 1,
                        num_pings_recv: lock.num_pings_recv,
                        num_pongs_recv: lock.num_pongs_recv,
                        avg_latency: lock.avg_latency,
                    };
                }
                update_signal.send(Box::new(update_count)).unwrap();
            },
            _ = shutdown => {
                break;
            }
        }
    }
}
fn update_count(s: &mut Cursive) {
    s.call_on_id("counter", move |view: &mut TextView| {
        let lock = UI_STATE.read().unwrap();
        view.set_content(format!(
            "Pings sent: {}\nPings Received: {}\nPongs Received: {}\nAvg. Latency: {}ms\n\n(p) Send ping (q) Quit",
            lock.num_pings_sent, lock.num_pings_recv, lock.num_pongs_recv, lock.avg_latency
        ));
    });
    s.set_autorefresh(true);
}
