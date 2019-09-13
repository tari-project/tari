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
    executor::{block_on, ThreadPool},
    join,
    stream::StreamExt,
    task::SpawnExt,
    Stream,
};
use futures_timer::Interval;
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
    initialization::{initialize_comms, CommsConfig},
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{LivenessHandle, LivenessInitializer, LivenessRequest, LivenessResponse},
    },
};
use tari_service_framework::StackBuilder;
use tari_utilities::message_format::MessageFormat;
use tempdir::TempDir;
use tokio::runtime::Runtime;
use tower_service::Service;

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

    let node_identity = load_identity(matches.value_of("node-identity").unwrap());
    let peer_identity = load_identity(matches.value_of("peer-identity").unwrap());

    let comms_config = CommsConfig {
        public_key: node_identity.identity.public_key.clone(),
        host: "0.0.0.0".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: node_identity.control_service_address().unwrap(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        secret_key: node_identity.secret_key.clone(),
        public_address: node_identity.control_service_address().unwrap(),
        datastore_path: TempDir::new(random_string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string(),
        peer_database_name: random_string(8),
    };
    let rt = Runtime::new().expect("Failed to create tokio Runtime");
    // TODO: Use only tokio runtime
    let mut thread_pool = ThreadPool::new().expect("Could not start Futures ThreadPool");

    let comms = initialize_comms(rt.executor(), comms_config).unwrap();
    let peer = Peer::new(
        peer_identity.identity.public_key.clone(),
        peer_identity.identity.node_id.clone(),
        peer_identity.control_service_address().unwrap().into(),
        PeerFlags::empty(),
    );
    comms.peer_manager().add_peer(peer).unwrap();

    let comms = Arc::new(comms);

    let fut = StackBuilder::new(&mut thread_pool)
        .add_initializer(CommsOutboundServiceInitializer::new(comms.outbound_message_service()))
        .add_initializer(LivenessInitializer::new(Arc::clone(&comms)))
        .finish();

    let handles = block_on(fut).expect("Service initialization failed");

    let mut app = setup_ui();

    // Updates the UI when pings/pongs are received
    let ui_update_signal = app.cb_sink().clone();
    let liveness_handle = handles.get_handle::<LivenessHandle>().unwrap();
    thread_pool
        .spawn(update_ui(ui_update_signal, liveness_handle.clone()))
        .unwrap();

    // Send pings when 'p' is pressed
    let (mut send_ping_tx, send_ping_rx) = mpsc::channel(10);
    app.add_global_callback('p', move |_| {
        let _ = send_ping_tx.start_send(());
    });

    let ui_update_signal = app.cb_sink().clone();
    let pk_to_ping = peer_identity.identity.public_key.clone();
    thread_pool
        .spawn(send_ping_on_trigger(
            send_ping_rx,
            ui_update_signal,
            liveness_handle,
            pk_to_ping,
        ))
        .unwrap();

    app.add_global_callback('q', |s| s.quit());
    app.run();
    let comms = Arc::try_unwrap(comms).map_err(|_| ()).unwrap();

    comms.shutdown().unwrap();
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

async fn update_ui(update_sink: CursiveSignal, mut liveness_handle: LivenessHandle) {
    // TODO: This should ideally be a stream of events which we subscribe to here
    let mut interval = Interval::new(Duration::from_millis(100));
    while let Some(_) = interval.next().await {
        let (ping_count, pong_count) = join!(
            liveness_handle.call(LivenessRequest::GetPingCount),
            liveness_handle.call(LivenessRequest::GetPongCount)
        );

        match (ping_count.unwrap(), pong_count.unwrap()) {
            (Ok(LivenessResponse::Count(num_pings)), Ok(LivenessResponse::Count(num_pongs))) => {
                {
                    let mut lock = COUNTER_STATE.write().unwrap();
                    *lock = (lock.0, num_pings, num_pongs);
                }
                let _ = update_sink.send(Box::new(update_count));
            },
            _ => {},
        }
    }
}

async fn send_ping_on_trigger(
    mut send_ping_trigger: impl Stream<Item = ()> + Unpin,
    update_signal: CursiveSignal,
    mut liveness_handle: LivenessHandle,
    pk_to_ping: CommsPublicKey,
)
{
    while let Some(_) = send_ping_trigger.next().await {
        liveness_handle
            .call(LivenessRequest::SendPing(pk_to_ping.clone()))
            .await
            .unwrap()
            .unwrap();

        {
            let mut lock = COUNTER_STATE.write().unwrap();
            *lock = (lock.0 + 1, lock.1, lock.2);
        }
        update_signal.send(Box::new(update_count)).unwrap();
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
