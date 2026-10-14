// Copyright 2026, The Tari Project
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

//! Tests for the listener's inbound admission limits: liveness admission, the inbound handshake deadline, the
//! per-IP handshake cap, refusing sockets when the handshake slots are full and the established inbound cap.

use std::{
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures::{Stream, StreamExt};
use multiaddr::Multiaddr;
use tari_shutdown::Shutdown;
use tari_test_utils::unpack_enum;
use tari_utilities::hex;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, oneshot},
    time,
    time::Instant,
};

use crate::{
    backoff::ConstantBackoff,
    connection_manager::{
        ConnectionManagerConfig,
        ConnectionManagerError,
        PeerConnection,
        SelfLivenessCheck,
        SelfLivenessStatus,
        dialer::{Dialer, DialerRequest},
        listener::{ListenerSharedState, PeerListener},
        manager::ConnectionManagerEvent,
        self_liveness::LivenessTokens,
        wire_mode::LIVENESS_WIRE_MODE,
    },
    memsocket::MemorySocket,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    noise::NoiseConfig,
    peer_manager::{NodeIdentity, PeerFeatures},
    test_utils::{build_peer_manager, node_identity::build_node_identity},
    transports::{MemoryTransport, Transport},
    types::TransportProtocol,
};

/// A memory transport whose inbound connections all appear to come from `source`, so that tests can exercise the
/// listener's IP-based rules without real sockets.
#[derive(Clone)]
struct FixedSourceTransport {
    source: Multiaddr,
    /// The number of dials made through this transport (and its clones)
    dials: Arc<AtomicUsize>,
}

impl FixedSourceTransport {
    fn new(source: &str) -> Self {
        Self {
            source: source.parse().unwrap(),
            dials: Arc::new(AtomicUsize::new(0)),
        }
    }
}

type FixedSourceListener = Pin<Box<dyn Stream<Item = io::Result<(MemorySocket, Multiaddr)>> + Send>>;

#[crate::async_trait]
impl Transport for FixedSourceTransport {
    type Error = io::Error;
    type Listener = FixedSourceListener;
    type Output = MemorySocket;

    async fn listen(&self, addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
        let (listener, bound_addr) = MemoryTransport.listen(addr).await?;
        let source = self.source.clone();
        let listener = listener.map(move |result| result.map(|(socket, _addr)| (socket, source.clone())));
        Ok((Box::pin(listener), bound_addr))
    }

    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
        self.dials.fetch_add(1, Ordering::SeqCst);
        MemoryTransport.dial(addr).await
    }

    fn supported_protocols(&self) -> Vec<TransportProtocol> {
        vec![TransportProtocol::Memory]
    }
}

const CLEARNET_SOURCE: &str = "/ip4/203.0.113.7/tcp/18189";
const LOOPBACK_SOURCE: &str = "/ip4/127.0.0.1/tcp/18189";

struct TestListener {
    address: Multiaddr,
    node_identity: Arc<NodeIdentity>,
    events: mpsc::Receiver<ConnectionManagerEvent>,
    liveness_tokens: LivenessTokens,
    shared_state: ListenerSharedState,
    transport: FixedSourceTransport,
}

impl TestListener {
    /// Waits until no inbound future is running. A completed dial does not mean the listener side has finished.
    async fn wait_for_no_handshakes_in_flight(&self) {
        for _ in 0..200 {
            if self.shared_state.handshakes_in_flight() == 0 {
                return;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
        panic!("inbound handshakes never finished");
    }
}

async fn spawn_listener(
    config: ConnectionManagerConfig,
    transport: FixedSourceTransport,
    shutdown: &Shutdown,
) -> TestListener {
    spawn_listener_with_shared_state(config, transport, shutdown, None, 100).await
}

/// Spawns a listener. If `shared_state` is given the listener uses it, as the main and auxiliary listeners of a node
/// share theirs. `event_channel_size` is the capacity of the listener's event channel; a small one that the test does
/// not drain stalls the listener after a connection is established.
async fn spawn_listener_with_shared_state(
    config: ConnectionManagerConfig,
    transport: FixedSourceTransport,
    shutdown: &Shutdown,
    shared_state: Option<ListenerSharedState>,
    event_channel_size: usize,
) -> TestListener {
    let (event_tx, events) = mpsc::channel(event_channel_size);
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_manager = build_peer_manager(&node_identity.to_peer()).unwrap();
    let mut listener = PeerListener::new(
        config,
        "/memory/0".parse().unwrap(),
        transport.clone(),
        NoiseConfig::new(node_identity.clone()),
        event_tx,
        peer_manager,
        node_identity.clone(),
        shutdown.to_signal(),
    );
    if let Some(shared_state) = shared_state {
        listener.set_shared_state(shared_state);
    }
    let liveness_tokens = listener.liveness_tokens();
    let shared_state = listener.shared_state();
    let address = listener.listen().await.unwrap();
    TestListener {
        address,
        node_identity,
        events,
        liveness_tokens,
        shared_state,
        transport,
    }
}

/// Dials the listener with a fresh identity and returns the established outbound connection.
async fn dial_listener(listener: &TestListener, shutdown: &Shutdown) -> Result<PeerConnection, ConnectionManagerError> {
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_manager = build_peer_manager(&node_identity.to_peer()).unwrap();
    let (event_tx, _event_rx) = mpsc::channel(100);
    let (request_tx, request_rx) = mpsc::channel(1);
    let dialer = Dialer::new(
        ConnectionManagerConfig::default(),
        node_identity.clone(),
        peer_manager,
        MemoryTransport,
        NoiseConfig::new(node_identity),
        ConstantBackoff::new(Duration::from_millis(100)),
        request_rx,
        event_tx,
        shutdown.to_signal(),
    );
    tokio::spawn(dialer.run());

    let mut peer = listener.node_identity.to_peer();
    peer.addresses =
        MultiaddressesWithStats::from_addresses_with_source(vec![listener.address.clone()], &PeerAddressSource::Config);
    peer.set_id_for_test(1);
    let (reply_tx, reply_rx) = oneshot::channel();
    request_tx
        .send(DialerRequest::Dial(Box::new(peer), Some(reply_tx)))
        .await
        .unwrap();
    reply_rx.await.unwrap()
}

/// Opens a raw socket to the listener and writes `bytes`.
async fn connect_and_send(listener: &TestListener, bytes: &[u8]) -> MemorySocket {
    let mut socket = listener.transport.dial(&listener.address).await.unwrap();
    if !bytes.is_empty() {
        socket.write_all(bytes).await.unwrap();
    }
    socket
}

/// Reads until the listener closes the socket and returns everything it sent.
async fn read_until_closed(socket: &mut MemorySocket) -> Vec<u8> {
    let mut buf = Vec::new();
    time::timeout(Duration::from_secs(60), socket.read_to_end(&mut buf))
        .await
        .expect("listener did not close the socket")
        .unwrap_or(0);
    buf
}

/// True if the listener has not closed the socket (nothing to read and no EOF within a short wait).
async fn is_still_open(socket: &mut MemorySocket) -> bool {
    let mut buf = [0u8; 1];
    time::timeout(Duration::from_millis(200), socket.read(&mut buf))
        .await
        .is_err()
}

/// Reads one echoed line and checks it is `line`.
async fn expect_echo(socket: &mut MemorySocket, line: &str) {
    let expected = format!("{line}\n").into_bytes();
    let mut buf = vec![0u8; expected.len()];
    time::timeout(Duration::from_secs(10), socket.read_exact(&mut buf))
        .await
        .expect("no echo")
        .unwrap();
    assert_eq!(buf, expected);
}

/// Waits until `count` checks have reported Live, panicking if any reports Unreachable.
async fn wait_for_live_checks(liveness_watch: &mut tokio::sync::watch::Receiver<SelfLivenessStatus>, count: usize) {
    let mut live = 0;
    while live < count {
        time::timeout(Duration::from_secs(120), liveness_watch.changed())
            .await
            .expect("self-check stopped reporting")
            .unwrap();
        match *liveness_watch.borrow_and_update() {
            SelfLivenessStatus::Live(_) => live = live.saturating_add(1),
            SelfLivenessStatus::Unreachable => panic!("self-check reported unreachable"),
            _ => {},
        }
    }
}

fn liveness_line(line: &str) -> Vec<u8> {
    let mut bytes = vec![LIVENESS_WIRE_MODE];
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');
    bytes
}

#[tokio::test]
async fn liveness_flood_from_non_allowlisted_ip_is_refused() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        self_liveness_self_check_interval: Some(Duration::from_millis(100)),
        liveness_max_sessions: 10,
        liveness_cidr_allowlist: vec!["127.0.0.1/32".parse().unwrap()],
        // Isolate the liveness admission rules from the per-IP handshake cap
        max_inbound_handshakes_per_ip: 1000,
        ..Default::default()
    };
    let listener = spawn_listener(config, FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown).await;

    let mut liveness_watch = SelfLivenessCheck::spawn(
        listener.transport.clone(),
        vec![listener.address.clone()],
        Duration::from_millis(100),
        listener.liveness_tokens.clone(),
        shutdown.to_signal(),
    );

    let mut sockets = Vec::new();
    for _ in 0..50 {
        sockets.push(connect_and_send(&listener, &liveness_line("junk")).await);
    }
    for mut socket in sockets {
        let received = read_until_closed(&mut socket).await;
        assert!(received.is_empty(), "a non-allowlisted liveness connection got an echo");
    }

    // The self-check arrives from the same non-allowlisted address and is admitted by its token
    time::timeout(
        Duration::from_secs(10),
        liveness_watch.wait_for(|status| matches!(status, SelfLivenessStatus::Live(_))),
    )
    .await
    .expect("self-check never became live")
    .unwrap();

    shutdown.trigger();
}

#[tokio::test]
async fn liveness_self_check_requires_matching_token() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        liveness_max_sessions: 10,
        liveness_cidr_allowlist: vec![],
        ..Default::default()
    };
    let listener = spawn_listener(config, FixedSourceTransport::new(LOOPBACK_SOURCE), &shutdown).await;

    // A well-formed token that was never registered is refused
    let random_token = hex::to_hex(&[7u8; 32]);
    let mut socket = connect_and_send(&listener, &liveness_line(&random_token)).await;
    assert!(read_until_closed(&mut socket).await.is_empty());

    // A registered token is consumed and starts a self-check session that keeps echoing
    let token = [9u8; 32];
    listener.liveness_tokens.lock().unwrap().insert(token);
    let token_hex = hex::to_hex(&token);
    let mut session = connect_and_send(&listener, &liveness_line(&token_hex)).await;
    expect_echo(&mut session, &token_hex).await;
    assert!(listener.liveness_tokens.lock().unwrap().is_empty());
    session.write_all(b"next check\n").await.unwrap();
    expect_echo(&mut session, "next check").await;

    // Replaying the consumed token is refused
    let mut socket = connect_and_send(&listener, &liveness_line(&token_hex)).await;
    assert!(read_until_closed(&mut socket).await.is_empty());

    shutdown.trigger();
}

#[tokio::test]
async fn liveness_session_count_never_underflows() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        liveness_max_sessions: 0,
        liveness_cidr_allowlist: vec!["127.0.0.1/32".parse().unwrap()],
        ..Default::default()
    };
    let listener = spawn_listener(config, FixedSourceTransport::new(LOOPBACK_SOURCE), &shutdown).await;

    // With no permits, every allow-listed connection is refused. A wrapping counter would start admitting them.
    for _ in 0..5 {
        let mut socket = connect_and_send(&listener, &liveness_line("hello")).await;
        assert!(read_until_closed(&mut socket).await.is_empty());
    }

    // The self-check path does not need a permit
    let token = [3u8; 32];
    listener.liveness_tokens.lock().unwrap().insert(token);
    let token_hex = hex::to_hex(&token);
    let mut self_check = connect_and_send(&listener, &liveness_line(&token_hex)).await;
    expect_echo(&mut self_check, &token_hex).await;

    // ...and still nothing else is admitted afterwards
    let mut socket = connect_and_send(&listener, &liveness_line("hello")).await;
    assert!(read_until_closed(&mut socket).await.is_empty());

    // With one permit: a second concurrent session is refused, and the permit comes back when the first ends
    let config = ConnectionManagerConfig {
        liveness_max_sessions: 1,
        liveness_cidr_allowlist: vec!["127.0.0.1/32".parse().unwrap()],
        ..Default::default()
    };
    let listener = spawn_listener(config, FixedSourceTransport::new(LOOPBACK_SOURCE), &shutdown).await;
    let mut first = connect_and_send(&listener, &liveness_line("first")).await;
    let mut buf = [0u8; 6];
    first.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"first\n");

    let mut second = connect_and_send(&listener, &liveness_line("second")).await;
    assert!(read_until_closed(&mut second).await.is_empty());

    drop(first);
    let mut third = None;
    for _ in 0..50 {
        let mut socket = connect_and_send(&listener, &liveness_line("third")).await;
        let mut buf = [0u8; 6];
        if socket.read_exact(&mut buf).await.is_ok() {
            assert_eq!(&buf, b"third\n");
            third = Some(socket);
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert!(third.is_some(), "the liveness permit was never returned");

    shutdown.trigger();
}

#[tokio::test(start_paused = true)]
async fn liveness_session_times_out_when_idle() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        liveness_max_sessions: 1,
        liveness_cidr_allowlist: vec!["127.0.0.1/32".parse().unwrap()],
        ..Default::default()
    };
    let listener = spawn_listener(config, FixedSourceTransport::new(LOOPBACK_SOURCE), &shutdown).await;

    let mut socket = connect_and_send(&listener, &liveness_line("hello")).await;
    let mut buf = [0u8; 6];
    socket.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"hello\n");

    // Send nothing more: the session is closed once the 30s idle timeout passes
    let start = Instant::now();
    assert!(read_until_closed(&mut socket).await.is_empty());
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_secs(30), "closed early after {elapsed:?}");
    assert!(elapsed < Duration::from_secs(31), "closed late after {elapsed:?}");

    shutdown.trigger();
}

#[tokio::test(start_paused = true)]
async fn silent_inbound_sockets_do_not_block_accept() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        max_simultaneous_inbound_connects: 3,
        // Make the 15s inbound handshake deadline, not the first byte timeout, the bound that frees the slots
        time_to_first_byte: Duration::from_secs(60),
        ..Default::default()
    };
    let listener = spawn_listener(config, FixedSourceTransport::new(LOOPBACK_SOURCE), &shutdown).await;
    let start = Instant::now();

    let mut silent = Vec::new();
    for _ in 0..3 {
        silent.push(connect_and_send(&listener, &[]).await);
    }
    for socket in &mut silent {
        assert!(is_still_open(socket).await);
    }

    // All slots are held: a new connection is closed straight away instead of waiting for a slot
    let probe_start = Instant::now();
    let mut probe = connect_and_send(&listener, &[]).await;
    assert!(read_until_closed(&mut probe).await.is_empty());
    assert!(probe_start.elapsed() < Duration::from_secs(1));

    // Once the deadline has closed the silent sockets, an honest peer connects
    time::sleep_until(start + Duration::from_secs(16)).await;
    for socket in &mut silent {
        assert!(read_until_closed(socket).await.is_empty());
    }
    let conn = dial_listener(&listener, &shutdown).await.unwrap();
    assert!(conn.is_connected());

    shutdown.trigger();
}

#[tokio::test(start_paused = true)]
async fn inbound_handshake_deadline_cuts_stalled_identity_exchange() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        // Leave the wire byte stage plenty of time so that the identity stage starts late enough for its own 10s
        // read timeout to end after the 15s deadline
        time_to_first_byte: Duration::from_secs(10),
        ..Default::default()
    };
    let network_byte = config.network_info.network_wire_byte;
    let mut listener = spawn_listener(config, FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown).await;
    let start = Instant::now();

    let mut socket = connect_and_send(&listener, &[]).await;
    time::sleep(Duration::from_secs(8)).await;
    socket.write_all(&[network_byte]).await.unwrap();
    let client_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let mut noise_socket = NoiseConfig::new(client_identity)
        .upgrade_socket(socket, crate::connection_manager::ConnectionDirection::Outbound)
        .await
        .unwrap();

    // Noise is done; never send our identity. Read the listener's identity frame and wait for the close.
    let mut buf = [0u8; 1024];
    loop {
        match noise_socket.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {},
        }
    }
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_secs(15), "closed early after {elapsed:?}");
    assert!(elapsed < Duration::from_secs(16), "closed late after {elapsed:?}");

    let event = time::timeout(Duration::from_secs(5), listener.events.recv())
        .await
        .unwrap()
        .unwrap();
    unpack_enum!(
        ConnectionManagerEvent::PeerInboundConnectFailed(ConnectionManagerError::InboundHandshakeTimeout) = event
    );

    shutdown.trigger();
}

#[tokio::test(start_paused = true)]
async fn per_ip_handshake_cap_applies_to_clearnet_only() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        max_inbound_handshakes_per_ip: 4,
        ..Default::default()
    };

    let clearnet = spawn_listener(config.clone(), FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown).await;
    let mut sockets = Vec::new();
    for _ in 0..5 {
        sockets.push(connect_and_send(&clearnet, &[]).await);
    }
    let mut fifth = sockets.pop().unwrap();
    assert!(read_until_closed(&mut fifth).await.is_empty());
    for socket in &mut sockets {
        assert!(
            is_still_open(socket).await,
            "one of the first four clearnet handshakes was refused"
        );
    }

    let loopback = spawn_listener(config, FixedSourceTransport::new(LOOPBACK_SOURCE), &shutdown).await;
    let mut sockets = Vec::new();
    for _ in 0..5 {
        sockets.push(connect_and_send(&loopback, &[]).await);
    }
    for socket in &mut sockets {
        assert!(is_still_open(socket).await, "a loopback handshake was refused");
    }

    shutdown.trigger();
}

#[tokio::test]
async fn inbound_connection_cap_refuses_before_noise() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        max_inbound_connections: 2,
        ..Default::default()
    };
    let network_byte = config.network_info.network_wire_byte;
    let mut listener = spawn_listener(config, FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown).await;

    // Hold the listener side of each connection, as the connectivity manager would
    let mut inbound_conns = Vec::new();
    let mut conn1 = dial_listener(&listener, &shutdown).await.unwrap();
    let event = listener.events.recv().await.unwrap();
    unpack_enum!(ConnectionManagerEvent::PeerConnected(inbound1) = event);
    inbound_conns.push(inbound1);
    listener.wait_for_no_handshakes_in_flight().await;
    let _conn2 = dial_listener(&listener, &shutdown).await.unwrap();
    let event = listener.events.recv().await.unwrap();
    unpack_enum!(ConnectionManagerEvent::PeerConnected(inbound2) = event);
    inbound_conns.push(inbound2);
    listener.wait_for_no_handshakes_in_flight().await;
    assert_eq!(listener.shared_state.established_inbound(), 2);

    // The third is closed before the listener reads anything, so the noise handshake never starts. Had it been
    // admitted it would only be closed by the 3s noise receive timeout.
    let start = Instant::now();
    let mut socket = connect_and_send(&listener, &[]).await;
    let _result = socket.write_all(&[network_byte]).await;
    assert!(read_until_closed(&mut socket).await.is_empty());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(listener.shared_state.established_inbound(), 2);

    // Closing an inbound connection frees its place
    conn1.disconnect(crate::Minimized::No, "unit test").await.unwrap();
    for _ in 0..100 {
        if listener.shared_state.established_inbound() == 1 {
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(listener.shared_state.established_inbound(), 1);
    let conn3 = dial_listener(&listener, &shutdown).await.unwrap();
    assert!(conn3.is_connected());

    shutdown.trigger();
}

#[test]
fn zero_listener_limits_are_rejected() {
    assert!(ConnectionManagerConfig::default().validate().is_ok());

    let configs = [
        ConnectionManagerConfig {
            inbound_handshake_timeout: Duration::ZERO,
            ..Default::default()
        },
        ConnectionManagerConfig {
            max_inbound_connections: 0,
            ..Default::default()
        },
        ConnectionManagerConfig {
            max_inbound_handshakes_per_ip: 0,
            ..Default::default()
        },
        ConnectionManagerConfig {
            max_inbound_connections_per_ip: 0,
            ..Default::default()
        },
    ];
    for config in configs {
        let err = config.validate().unwrap_err();
        unpack_enum!(ConnectionManagerError::InvalidConfig(_details) = err);
    }
}

#[tokio::test]
async fn per_source_inbound_cap_refuses_before_noise() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        max_inbound_connections_per_ip: 2,
        ..Default::default()
    };
    let network_byte = config.network_info.network_wire_byte;
    let mut listener = spawn_listener(config, FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown).await;

    // Hold the listener side of each connection, as the connectivity manager would
    let mut inbound_conns = Vec::new();
    let mut conn1 = dial_listener(&listener, &shutdown).await.unwrap();
    let event = listener.events.recv().await.unwrap();
    unpack_enum!(ConnectionManagerEvent::PeerConnected(inbound1) = event);
    inbound_conns.push(inbound1);
    listener.wait_for_no_handshakes_in_flight().await;
    let _conn2 = dial_listener(&listener, &shutdown).await.unwrap();
    let event = listener.events.recv().await.unwrap();
    unpack_enum!(ConnectionManagerEvent::PeerConnected(inbound2) = event);
    inbound_conns.push(inbound2);
    listener.wait_for_no_handshakes_in_flight().await;

    // No handshakes are in flight, so only the established count for the source can refuse the third
    let start = Instant::now();
    let mut socket = connect_and_send(&listener, &[]).await;
    let _result = socket.write_all(&[network_byte]).await;
    assert!(read_until_closed(&mut socket).await.is_empty());
    assert!(start.elapsed() < Duration::from_secs(1));

    // Closing one of the source's connections frees its place
    conn1.disconnect(crate::Minimized::No, "unit test").await.unwrap();
    for _ in 0..100 {
        if listener.shared_state.established_inbound() == 1 {
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(listener.shared_state.established_inbound(), 1);
    let conn3 = dial_listener(&listener, &shutdown).await.unwrap();
    assert!(conn3.is_connected());

    shutdown.trigger();
}

#[tokio::test(start_paused = true)]
async fn per_ip_caps_exempt_mapped_loopback_and_configured_cidrs() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        max_inbound_handshakes_per_ip: 1,
        max_inbound_connections_per_ip: 1,
        ..Default::default()
    };

    // Control: without an exemption the second handshake from the source is refused
    let plain = spawn_listener(config.clone(), FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown).await;
    let mut first = connect_and_send(&plain, &[]).await;
    let mut second = connect_and_send(&plain, &[]).await;
    assert!(read_until_closed(&mut second).await.is_empty());
    assert!(is_still_open(&mut first).await);

    // IPv4-mapped loopback is treated as loopback
    let mapped_loopback = spawn_listener(
        config.clone(),
        FixedSourceTransport::new("/ip6/::ffff:127.0.0.1/tcp/18189"),
        &shutdown,
    )
    .await;
    let mut sockets = Vec::new();
    for _ in 0..3 {
        sockets.push(connect_and_send(&mapped_loopback, &[]).await);
    }
    for socket in &mut sockets {
        assert!(is_still_open(socket).await, "a mapped loopback handshake was refused");
    }

    // A configured exempt CIDR (e.g. a remote Tor forwarder) is exempt, including in IPv4-mapped form
    let exempt_config = ConnectionManagerConfig {
        listener_per_ip_exempt_cidrs: vec!["203.0.113.0/24".parse().unwrap()],
        ..config
    };
    let exempt = spawn_listener(
        exempt_config,
        FixedSourceTransport::new("/ip6/::ffff:203.0.113.7/tcp/18189"),
        &shutdown,
    )
    .await;
    let mut sockets = Vec::new();
    for _ in 0..3 {
        sockets.push(connect_and_send(&exempt, &[]).await);
    }
    for socket in &mut sockets {
        assert!(
            is_still_open(socket).await,
            "a handshake from an exempt CIDR was refused"
        );
    }

    shutdown.trigger();
}

#[tokio::test]
async fn liveness_sessions_are_shared_between_listeners() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        liveness_max_sessions: 1,
        liveness_cidr_allowlist: vec!["127.0.0.1/32".parse().unwrap()],
        ..Default::default()
    };
    let main = spawn_listener(config.clone(), FixedSourceTransport::new(LOOPBACK_SOURCE), &shutdown).await;
    let aux = spawn_listener_with_shared_state(
        config,
        FixedSourceTransport::new(LOOPBACK_SOURCE),
        &shutdown,
        Some(main.shared_state.with_own_counts()),
        100,
    )
    .await;

    let mut session = connect_and_send(&main, &liveness_line("main")).await;
    let mut buf = [0u8; 5];
    session.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"main\n");

    // The one node-wide permit is taken, so the other listener refuses
    let mut refused = connect_and_send(&aux, &liveness_line("aux")).await;
    assert!(read_until_closed(&mut refused).await.is_empty());

    shutdown.trigger();
}

#[tokio::test]
async fn established_connection_is_not_also_counted_in_flight() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        max_inbound_connections_per_ip: 3,
        ..Default::default()
    };
    // The test never drains the single-slot event channel: the first PeerConnected fills it and every later
    // connection's task stays parked publishing its event after it is established
    let listener =
        spawn_listener_with_shared_state(config, FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown, None, 1).await;

    let _conn1 = dial_listener(&listener, &shutdown).await.unwrap();
    let _conn2 = dial_listener(&listener, &shutdown).await.unwrap();
    for _ in 0..100 {
        if listener.shared_state.established_inbound() == 2 {
            break;
        }
        time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(listener.shared_state.established_inbound(), 2);
    // The second connection's task is still running, but it counts as established only
    assert_eq!(listener.shared_state.handshakes_in_flight(), 0);

    // So the source has used two of its three places, and a third connection is admitted
    let conn3 = dial_listener(&listener, &shutdown).await.unwrap();
    assert!(conn3.is_connected());

    shutdown.trigger();
}

#[tokio::test(start_paused = true)]
async fn main_listener_inbound_cap_does_not_refuse_aux_listener() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        max_inbound_connections: 1,
        ..Default::default()
    };
    let main = spawn_listener(config.clone(), FixedSourceTransport::new(LOOPBACK_SOURCE), &shutdown).await;
    let aux = spawn_listener_with_shared_state(
        config,
        FixedSourceTransport::new(LOOPBACK_SOURCE),
        &shutdown,
        Some(main.shared_state.with_own_counts()),
        100,
    )
    .await;

    // Fill the main listener's cap
    let mut held = connect_and_send(&main, &[]).await;
    let mut refused = connect_and_send(&main, &[]).await;
    assert!(read_until_closed(&mut refused).await.is_empty());
    assert!(is_still_open(&mut held).await);

    // The auxiliary listener has its own count
    let mut aux_socket = connect_and_send(&aux, &[]).await;
    assert!(
        is_still_open(&mut aux_socket).await,
        "the aux listener was refused by the main listener's cap"
    );
    assert_eq!(main.shared_state.handshakes_in_flight(), 1);
    assert_eq!(aux.shared_state.handshakes_in_flight(), 1);

    shutdown.trigger();
}

#[tokio::test(start_paused = true)]
async fn ipv4_mapped_cidrs_in_config_match_ipv4_sources() {
    let shutdown = Shutdown::new();

    // An IPv4-mapped liveness allowlist entry admits the IPv4 source
    let config = ConnectionManagerConfig {
        liveness_max_sessions: 1,
        liveness_cidr_allowlist: vec!["::ffff:127.0.0.1/128".parse().unwrap()],
        ..Default::default()
    };
    let listener = spawn_listener(config, FixedSourceTransport::new(LOOPBACK_SOURCE), &shutdown).await;
    let mut session = connect_and_send(&listener, &liveness_line("hello")).await;
    let mut buf = [0u8; 6];
    session.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"hello\n");

    // An IPv4-mapped exempt entry exempts the IPv4 source from the per-source limits
    let config = ConnectionManagerConfig {
        max_inbound_handshakes_per_ip: 1,
        listener_per_ip_exempt_cidrs: vec!["::ffff:203.0.113.0/120".parse().unwrap()],
        ..Default::default()
    };
    let listener = spawn_listener(config, FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown).await;
    let mut sockets = Vec::new();
    for _ in 0..3 {
        sockets.push(connect_and_send(&listener, &[]).await);
    }
    for socket in &mut sockets {
        assert!(
            is_still_open(socket).await,
            "a handshake from an exempt mapped CIDR was refused"
        );
    }

    shutdown.trigger();
}

#[tokio::test(start_paused = true)]
async fn self_check_reuses_one_connection_across_checks() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        liveness_cidr_allowlist: vec![],
        ..Default::default()
    };
    let listener = spawn_listener(config, FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown).await;

    let mut liveness_watch = SelfLivenessCheck::spawn(
        listener.transport.clone(),
        vec![listener.address.clone()],
        Duration::from_secs(5),
        listener.liveness_tokens.clone(),
        shutdown.to_signal(),
    );

    // Five checks, all on the first connection
    wait_for_live_checks(&mut liveness_watch, 5).await;
    assert_eq!(listener.transport.dials.load(Ordering::SeqCst), 1);
    assert!(listener.liveness_tokens.lock().unwrap().is_empty());

    shutdown.trigger();
}

#[tokio::test(start_paused = true)]
async fn self_check_redials_when_the_listener_would_end_the_session() {
    let shutdown = Shutdown::new();
    let listener = spawn_listener(
        ConnectionManagerConfig::default(),
        FixedSourceTransport::new(CLEARNET_SOURCE),
        &shutdown,
    )
    .await;

    // An interval longer than the listener's idle timeout: each check finds its session idled out and re-dials with a
    // fresh token, which is not a failure
    let mut liveness_watch = SelfLivenessCheck::spawn(
        listener.transport.clone(),
        vec![listener.address.clone()],
        Duration::from_secs(40),
        listener.liveness_tokens.clone(),
        shutdown.to_signal(),
    );
    wait_for_live_checks(&mut liveness_watch, 3).await;
    assert_eq!(listener.transport.dials.load(Ordering::SeqCst), 3);

    shutdown.trigger();
}

#[tokio::test]
async fn only_one_self_check_session_is_admitted_at_a_time() {
    let shutdown = Shutdown::new();
    let config = ConnectionManagerConfig {
        liveness_cidr_allowlist: vec![],
        ..Default::default()
    };
    let main = spawn_listener(config.clone(), FixedSourceTransport::new(CLEARNET_SOURCE), &shutdown).await;
    // The permit is node-wide, so a token presented to the aux listener is refused too
    let aux = spawn_listener_with_shared_state(
        config,
        FixedSourceTransport::new(CLEARNET_SOURCE),
        &shutdown,
        Some(main.shared_state.with_own_counts()),
        100,
    )
    .await;

    let first_token = hex::to_hex(&[1u8; 32]);
    let second_token = hex::to_hex(&[2u8; 32]);
    main.liveness_tokens.lock().unwrap().insert([1u8; 32]);
    main.liveness_tokens.lock().unwrap().insert([2u8; 32]);

    let mut first = connect_and_send(&main, &liveness_line(&first_token)).await;
    expect_echo(&mut first, &first_token).await;

    // A second token session while the first is open is refused, and its token is consumed
    let mut second = connect_and_send(&aux, &liveness_line(&second_token)).await;
    assert!(read_until_closed(&mut second).await.is_empty());
    assert!(main.liveness_tokens.lock().unwrap().is_empty());

    // Once the first session closes, a new token session is admitted
    drop(first);
    let mut admitted = false;
    for i in 0..50u8 {
        let token = [i.saturating_add(10); 32];
        main.liveness_tokens.lock().unwrap().insert(token);
        let token_hex = hex::to_hex(&token);
        let mut socket = connect_and_send(&main, &liveness_line(&token_hex)).await;
        let mut buf = vec![0u8; token_hex.len().saturating_add(1)];
        if socket.read_exact(&mut buf).await.is_ok() {
            assert_eq!(buf, format!("{token_hex}\n").into_bytes());
            admitted = true;
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert!(admitted, "the self-check permit was never returned");

    shutdown.trigger();
}
