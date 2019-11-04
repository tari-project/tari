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
    sync::{Arc, RwLock},
    time::Duration,
};
use tari_comms::{
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, Peer, PeerFlags},
    types::CommsPublicKey,
};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{initialize_comms, CommsConfig},
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{
            handle::{LivenessEvent, LivenessHandle},
            LivenessConfig,
            LivenessInitializer,
        },
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
                .long("peer-identity")
                .short("p")
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
        .get_matches();

    log4rs::init_file(matches.value_of("log-config").unwrap(), Default::default()).unwrap();

    let node_identity = Arc::new(load_identity(matches.value_of("node-identity").unwrap()));
    let peer_identity = load_identity(matches.value_of("peer-identity").unwrap());

    let comms_config = CommsConfig {
        node_identity: Arc::clone(&node_identity),
        host: "0.0.0.0".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: node_identity.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        establish_connection_timeout: Duration::from_secs(2),
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
                auto_ping_interval: Some(Duration::from_secs(10)),
            },
            Arc::clone(&subscription_factory),
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
    let pk_to_ping = peer_identity.public_key().clone();
    rt.spawn(send_ping_on_trigger(
        send_ping_rx.fuse(),
        ui_update_signal,
        liveness_handle,
        pk_to_ping,
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
lazy_static! {
    /// Used to keep track of the counts displayed in the UI
    /// (sent ping count, recv ping count, recv pong count)
    static ref COUNTER_STATE: Arc<RwLock<(usize, usize, usize)>> = Arc::new(RwLock::new((0, 0, 0)));
}
type CursiveSignal = crossbeam_channel::Sender<Box<dyn CbFunc>>;

async fn update_ui(update_sink: CursiveSignal, mut liveness_handle: LivenessHandle, mut shutdown: ShutdownSignal) {
    let mut event_stream = liveness_handle.get_event_stream_fused();
    let ping_count = liveness_handle.get_ping_count().await.unwrap_or(0);
    let pong_count = liveness_handle.get_pong_count().await.unwrap_or(0);

    {
        let mut lock = COUNTER_STATE.write().unwrap();
        *lock = (lock.0, ping_count, pong_count);
    }
    let _ = update_sink.send(Box::new(update_count));

    loop {
        ::futures::select! {
            event = event_stream.select_next_some() => {
                match *event {
                    LivenessEvent::ReceivedPing => {
                        {
                            let mut lock = COUNTER_STATE.write().unwrap();
                            *lock = (lock.0, lock.1 + 1, lock.2);
                        }
                    },
                    LivenessEvent::ReceivedPong => {
                        {
                            let mut lock = COUNTER_STATE.write().unwrap();
                            *lock = (lock.0, lock.1, lock.2 + 1);
                        }
                    },
                }

                let _ = update_sink.send(Box::new(update_count));
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
    pk_to_ping: CommsPublicKey,
    mut shutdown: ShutdownSignal,
)
{
    loop {
        futures::select! {
            _ = send_ping_trigger.next() => {
                liveness_handle.send_ping(pk_to_ping.clone()).await.unwrap();

                {
                    let mut lock = COUNTER_STATE.write().unwrap();
                    *lock = (lock.0 + 1, lock.1, lock.2);
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
        let lock = COUNTER_STATE.read().unwrap();
        view.set_content(format!(
            "Pings sent: {}\nPings Received: {}\nPongs Received: {}\n\n(p) Send ping (q) Quit",
            lock.0, lock.1, lock.2
        ));
    });
    s.set_autorefresh(true);
}
