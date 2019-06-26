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
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{
    fs,
    iter,
    sync::{
        mpsc::{channel, RecvTimeoutError},
        Arc,
        RwLock,
    },
    thread,
    time::Duration,
};
use tari_comms::{
    control_service::ControlServiceConfig,
    peer_manager::{peer_storage::PeerStorage, NodeIdentity, Peer, PeerFlags, PeerNodeIdentity},
    types::{CommsDataStore, CommsPublicKey},
};
use tari_p2p::{
    initialization::{initialize_comms, CommsConfig},
    ping_pong::{PingPongService, PingPongServiceApi},
    services::{ServiceExecutor, ServiceRegistry},
    tari_message::{NetMessage, TariMessageType},
};
use tari_storage::{keyvalue_store::DataStore, lmdb::LMDBBuilder};
use tari_utilities::message_format::MessageFormat;
use tempdir::TempDir;

fn load_identity(path: &str) -> NodeIdentity {
    let contents = fs::read_to_string(path).unwrap();
    NodeIdentity::from_json(contents.as_str()).unwrap()
}

pub fn random_string(len: usize) -> String {
    let mut rng = OsRng::new().unwrap();
    iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
}

fn datastore_with_peer(identity: NodeIdentity) -> CommsDataStore {
    let tmp_dir = TempDir::new(random_string(8).as_str()).unwrap();
    let dummy_db = random_string(8);
    let mut store = LMDBBuilder::new()
        .set_path(tmp_dir.path().to_str().unwrap())
        .add_database(&dummy_db)
        .build()
        .unwrap();

    store.connect(&dummy_db).unwrap();
    let mut storage = PeerStorage::with_datastore(store).unwrap();
    let peer = Peer::new(
        identity.identity.public_key,
        identity.identity.node_id,
        identity.control_service_address.into(),
        PeerFlags::empty(),
    );
    storage.add_peer(peer).unwrap();

    storage.into_datastore().unwrap()
}

fn main() {
    let matches = App::new("Peer file generator")
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
        .get_matches();

    let node_identity = load_identity(matches.value_of("node-identity").unwrap());
    let peer_identity = load_identity(matches.value_of("peer-identity").unwrap());

    let peer_store = datastore_with_peer(peer_identity.clone());

    let comms_config = CommsConfig {
        public_key: node_identity.identity.public_key.clone(),
        host: "0.0.0.0".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            accept_message_type: TariMessageType::from(NetMessage::Accept),
            listener_address: node_identity.control_service_address.clone(),
            socks_proxy_address: None,
            requested_outbound_connection_timeout: Duration::from_millis(2000),
        },
        secret_key: node_identity.secret_key.clone(),
    };

    let pingpong_service = PingPongService::new();
    let pingpong = pingpong_service.get_api();
    let services = ServiceRegistry::new().register(pingpong_service);

    let comms = initialize_comms(comms_config, services.build_comms_routes(), Some(peer_store)).unwrap();
    let services = ServiceExecutor::execute(comms, services);

    run_ui(services, peer_identity.identity, pingpong);
}

lazy_static! {
    /// Used to keep track of the counts displayed in the UI
    /// (sent ping count, recv ping count, recv pong count)
    static ref COUNTER_STATE: Arc<RwLock<(usize, usize, usize)>> = Arc::new(RwLock::new((0, 0, 0)));
}

fn run_ui(
    services: ServiceExecutor,
    peer_identity: PeerNodeIdentity<CommsPublicKey>,
    pingpong_api: Arc<PingPongServiceApi>,
)
{
    let mut app = Cursive::default();

    app.add_layer(
        Dialog::around(TextView::new("Loading...").with_id("counter"))
            .title("PingPong")
            .button("Quit", |s| s.quit()),
    );

    let update_sink = app.cb_sink().clone();

    let inner_api = pingpong_api.clone();
    let (shutdown_tx, shutdown_rx) = channel();
    let app_handle = thread::spawn(move || loop {
        {
            let mut lock = COUNTER_STATE.write().unwrap();
            *lock = (
                lock.0,
                pingpong_api.ping_count().unwrap(),
                pingpong_api.pong_count().unwrap(),
            );
        }
        update_sink.send(Box::new(update_count)).unwrap();
        match shutdown_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(_) => break,
            Err(RecvTimeoutError::Timeout) => {},
            Err(RecvTimeoutError::Disconnected) => break,
        }
    });

    let ping_update_cb = app.cb_sink().clone();

    let pk_to_ping = peer_identity.public_key.clone();
    app.add_global_callback('p', move |_| {
        inner_api.ping(pk_to_ping.clone()).unwrap();
        {
            let mut lock = COUNTER_STATE.write().unwrap();
            *lock = (lock.0 + 1, lock.1, lock.2);
        }
        ping_update_cb.send(Box::new(update_count)).unwrap();
    });
    app.add_global_callback('q', |s| s.quit());

    app.run();

    shutdown_tx.send(()).unwrap();
    services.shutdown().unwrap();
    services.join_timeout(Duration::from_millis(1000)).unwrap();
    app_handle.join().unwrap();
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
