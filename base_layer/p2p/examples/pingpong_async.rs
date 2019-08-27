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
    Cursive,
};
use futures::{future, sync::mpsc, Future, Sink, Stream};
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{
    fs,
    iter,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use stream_cancel::{StreamExt, Tripwire};
use tari_comms::{
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, Peer, PeerFlags, PeerNodeIdentity},
};
use tari_p2p::{
    initialization::{initialize_comms, CommsConfig},
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{LivenessHandle, LivenessInitializer, LivenessRequest, LivenessResponse},
        ServiceHandles,
        ServiceName,
    },
};
use tari_service_framework::StackBuilder;
use tari_utilities::message_format::MessageFormat;
use tempdir::TempDir;
use tokio::{runtime::Runtime, timer::Interval};
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

    let comms = initialize_comms(comms_config).unwrap();
    let peer = Peer::new(
        peer_identity.identity.public_key.clone(),
        peer_identity.identity.node_id.clone(),
        peer_identity.control_service_address().unwrap().into(),
        PeerFlags::empty(),
    );
    comms.peer_manager().add_peer(peer).unwrap();

    let mut rt = Runtime::new().unwrap();

    let initialize = StackBuilder::new()
        .add_initializer(CommsOutboundServiceInitializer::new(comms.outbound_message_service()))
        .add_initializer(LivenessInitializer::new(Arc::clone(&comms)))
        .finish();

    let handles = rt.block_on(initialize).unwrap();

    run_ui(peer_identity.identity, handles);

    let comms = Arc::try_unwrap(comms).map_err(|_| ()).unwrap();

    comms.shutdown().unwrap();
}

lazy_static! {
    /// Used to keep track of the counts displayed in the UI
    /// (sent ping count, recv ping count, recv pong count)
    static ref COUNTER_STATE: Arc<RwLock<(usize, usize, usize)>> = Arc::new(RwLock::new((0, 0, 0)));
}

fn run_ui(peer_identity: PeerNodeIdentity, handles: Arc<ServiceHandles>) {
    tokio::run(future::lazy(move || {
        let mut app = Cursive::default();

        app.add_layer(
            Dialog::around(TextView::new("Loading...").with_id("counter"))
                .title("PingPong")
                .button("Quit", |s| s.quit()),
        );

        let (trigger, trip_wire) = Tripwire::new();

        let update_sink = app.cb_sink().clone();

        let mut liveness_handle = handles.get_handle::<LivenessHandle>(ServiceName::Liveness).unwrap();
        let update_ui_stream = Interval::new(Instant::now(), Duration::from_millis(100))
            .take_until(trip_wire.clone())
            .map_err(|_| ())
            .for_each(move |_| {
                let update_inner = update_sink.clone();
                liveness_handle
                    .call(LivenessRequest::GetPingCount)
                    .join(liveness_handle.call(LivenessRequest::GetPongCount))
                    .and_then(move |(pings, pongs)| {
                        match (pings, pongs) {
                            (Ok(LivenessResponse::Count(num_pings)), Ok(LivenessResponse::Count(num_pongs))) => {
                                {
                                    let mut lock = COUNTER_STATE.write().unwrap();
                                    *lock = (lock.0, num_pings, num_pongs);
                                }
                                let _ = update_inner.send(Box::new(update_count));
                            },
                            _ => {},
                        }

                        future::ok(())
                    })
                    .map_err(|_| ())
            });

        tokio::spawn(update_ui_stream);

        let ping_update_cb = app.cb_sink().clone();
        let mut liveness_handle = handles.get_handle::<LivenessHandle>(ServiceName::Liveness).unwrap();
        let pk_to_ping = peer_identity.public_key.clone();

        let (mut send_ping_tx, send_ping_rx) = mpsc::channel(10);
        app.add_global_callback('p', move |_| {
            let _ = send_ping_tx.start_send(());
        });

        let p_stream = send_ping_rx.take_until(trip_wire).for_each(move |_| {
            let ping_update_inner = ping_update_cb.clone();
            liveness_handle
                .call(LivenessRequest::SendPing(pk_to_ping.clone()))
                .map_err(|_| ())
                .and_then(move |_| {
                    {
                        let mut lock = COUNTER_STATE.write().unwrap();
                        *lock = (lock.0 + 1, lock.1, lock.2);
                    }
                    ping_update_inner.send(Box::new(update_count)).unwrap();
                    future::ok(())
                })
        });

        tokio::spawn(p_stream);

        app.add_global_callback('q', |s| s.quit());

        app.run();

        // Signal UI streams to stop
        trigger.cancel();
        future::ok(())
    }));
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
