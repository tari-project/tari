// Copyright 2019, The Tari Project
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

use std::{
    collections::{BTreeMap, HashMap},
    convert::TryInto,
    future::Future,
    io::{Error, ErrorKind},
    net::{IpAddr, Ipv6Addr},
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::FutureExt;
use log::*;
use tari_shutdown::{ShutdownSignal, oneshot_trigger, oneshot_trigger::OneshotTrigger};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{Semaphore, mpsc},
    time,
};
use tokio_stream::StreamExt;
use tracing::{Instrument, Level, span};

use super::{
    ConnectionManagerConfig,
    ConnectionManagerEvent,
    PeerConnectionInfo,
    common,
    direction::ConnectionDirection,
    error::ConnectionManagerError,
    peer_connection::{self, PeerConnection},
};
#[cfg(feature = "metrics")]
use crate::connection_manager::metrics;
use crate::{
    Minimized,
    PeerManager,
    bounded_executor::BoundedExecutor,
    connection_manager::{
        self_liveness::{LIVENESS_FIRST_LINE_TIMEOUT, LivenessTokens, SelfLivenessSession, take_liveness_token},
        wire_mode::{LIVENESS_WIRE_MODE, WireMode},
    },
    multiaddr::{Multiaddr, Protocol},
    multiplexing::{WriteStallSocket, Yamux},
    noise::NoiseConfig,
    peer_manager::NodeIdentity,
    protocol::ProtocolId,
    transports::Transport,
    utils::{
        cidr::normalise_ipv4_mapped_cidrs,
        mpsc::{SEND_OR_DROP_TIMEOUT, send_or_drop},
    },
};

const LOG_TARGET: &str = "comms::connection_manager::listener";

/// Tells the connection manager that an inbound connection from `peer_addr` failed, unless it is too busy to take it
/// (see [send_or_drop]). The peer's identity is not known (its upgrade failed), only its address.
async fn notify_inbound_connect_failed(
    conn_man_notifier: &mpsc::Sender<ConnectionManagerEvent>,
    err: ConnectionManagerError,
    peer_addr: &Multiaddr,
) {
    let event = ConnectionManagerEvent::PeerInboundConnectFailed(err);
    if send_or_drop(
        conn_man_notifier,
        event,
        "listener_inbound_connect_failed",
        Some(peer_addr),
    )
    .await
    .is_err()
    {
        debug!(
            target: LOG_TARGET,
            "Dropped the PeerInboundConnectFailed event for the peer at {peer_addr} because the connection manager was \
             busy"
        );
    }
}

/// Closes a connection whose `PeerConnected` event was dropped, so that nothing else knows of it.
///
/// Waits for the connection actor's reply for at most [SEND_OR_DROP_TIMEOUT]: the actor may itself be waiting on the
/// busy connection manager (to report a violation, say), and the caller holds a listener permit. If it does not reply
/// in time, the connection is still torn down once the actor is free: the disconnect request is already queued (this is
/// the only handle, so its request channel is otherwise empty), and dropping the handle closes that channel, which
/// also ends the actor.
pub(super) async fn close_unannounced_connection(mut conn: PeerConnection) {
    debug!(
        target: LOG_TARGET,
        "Dropped the PeerConnected event for peer `{}` ({}) because the connection manager was busy; closing the \
         connection",
        conn.peer_node_id(),
        conn.address()
    );
    let disconnect = conn.disconnect_silent(Minimized::No, "listener: PeerConnected event dropped");
    if time::timeout(SEND_OR_DROP_TIMEOUT, disconnect).await.is_err() {
        debug!(
            target: LOG_TARGET,
            "Connection to peer `{}` did not confirm its disconnect within {:.0?}; it closes once its actor is free",
            conn.peer_node_id(),
            SEND_OR_DROP_TIMEOUT
        );
    }
}

/// How often, at most, the listener logs a warning summarising the inbound connections it refused.
const REFUSAL_WARN_INTERVAL: Duration = Duration::from_secs(60);

/// Inbound connection counts, in total and per source bucket (see [per_source_key]). "In flight" covers every
/// running inbound future: handshakes and liveness sessions.
#[derive(Debug, Default)]
struct InboundCounts {
    in_flight: usize,
    established: usize,
    in_flight_per_source: HashMap<IpAddr, usize>,
    established_per_source: HashMap<IpAddr, usize>,
}

fn increment_source(map: &mut HashMap<IpAddr, usize>, source: IpAddr) {
    let count = map.get(&source).copied().unwrap_or(0);
    map.insert(source, count.saturating_add(1));
}

fn decrement_source(map: &mut HashMap<IpAddr, usize>, source: IpAddr) {
    let remaining = map.get(&source).copied().unwrap_or(0).saturating_sub(1);
    if remaining == 0 {
        map.remove(&source);
    } else {
        map.insert(source, remaining);
    }
}

impl InboundCounts {
    /// Admits one more inbound socket from `source` (None for exempt sources) if every inbound limit allows it, and
    /// counts it as in flight. Otherwise returns the metric reason it is refused.
    fn try_admit(&mut self, source: Option<IpAddr>, config: &ConnectionManagerConfig) -> Result<(), &'static str> {
        if self.established.saturating_add(self.in_flight) >= config.max_inbound_connections {
            return Err("inbound_cap");
        }

        if let Some(source) = source {
            let established_for_source = self.established_per_source.get(&source).copied().unwrap_or(0);
            let in_flight_for_source = self.in_flight_per_source.get(&source).copied().unwrap_or(0);
            if established_for_source.saturating_add(in_flight_for_source) >= config.max_inbound_connections_per_ip {
                return Err("inbound_cap_per_ip");
            }
            if in_flight_for_source >= config.max_inbound_handshakes_per_ip {
                return Err("per_ip");
            }
            increment_source(&mut self.in_flight_per_source, source);
        }

        self.in_flight = self.in_flight.saturating_add(1);
        Ok(())
    }
}

/// Counts inbound connections refused before the handshake so that they can be summarised in a rate-limited warning.
#[derive(Debug, Default)]
struct RefusalLog {
    last_warn: Option<time::Instant>,
    counts: BTreeMap<&'static str, usize>,
}

impl RefusalLog {
    /// Counts a refusal. Returns a summary of the refusals counted since the last summary, at most once per
    /// [REFUSAL_WARN_INTERVAL].
    fn record(&mut self, reason: &'static str, now: time::Instant) -> Option<String> {
        let count = self.counts.get(reason).copied().unwrap_or(0);
        self.counts.insert(reason, count.saturating_add(1));
        if let Some(last_warn) = self.last_warn &&
            now.saturating_duration_since(last_warn) < REFUSAL_WARN_INTERVAL
        {
            return None;
        }
        self.last_warn = Some(now);
        let mut parts = Vec::new();
        for (reason, count) in &self.counts {
            parts.push(format!("{reason}: {count}"));
        }
        self.counts.clear();
        Some(parts.join(", "))
    }
}

/// The number of liveness sessions the listeners allow: `liveness_max_sessions`, but never more than half of
/// `max_simultaneous_inbound_connects`, because each session holds a handshake slot for its whole lifetime.
pub(crate) fn effective_liveness_sessions(config: &ConnectionManagerConfig) -> usize {
    let slot_bound = config.max_simultaneous_inbound_connects.saturating_div(2);
    config.liveness_max_sessions.min(slot_bound).min(Semaphore::MAX_PERMITS)
}

/// Returns the source IP address of `peer_addr` in canonical form (IPv4-mapped IPv6 becomes IPv4), or None for a
/// non-IP address. No DNS lookup is done.
fn source_ip(peer_addr: &Multiaddr) -> Option<IpAddr> {
    match peer_addr.iter().next() {
        Some(Protocol::Ip4(ip)) => Some(IpAddr::V4(ip)),
        Some(Protocol::Ip6(ip)) => Some(IpAddr::V6(ip).to_canonical()),
        _ => None,
    }
}

/// True if `ip` (a canonical source address, see [source_ip]) is in any of `cidrs`. An IPv4 source is also matched in
/// its IPv4-mapped IPv6 form, so IPv6 CIDRs that cover `::ffff:0:0/96` (e.g. `::/0`) still match IPv4 peers on a
/// dual-stack listener.
fn source_in_cidrs(ip: IpAddr, cidrs: &[cidr::AnyIpCidr]) -> bool {
    let mapped = match ip {
        IpAddr::V4(ipv4) => Some(IpAddr::V6(ipv4.to_ipv6_mapped())),
        IpAddr::V6(_) => None,
    };
    for cidr in cidrs {
        if cidr.contains(&ip) {
            return true;
        }
        if let Some(mapped) = mapped &&
            cidr.contains(&mapped)
        {
            return true;
        }
    }
    false
}

/// Returns the bucket that the per-source inbound limits count `peer_addr` against: the IPv4 address, or the IPv6
/// /64 prefix. Returns None for exempt sources: loopback (Tor inbound forwarded over loopback), addresses in
/// `exempt_cidrs` (e.g. a remote Tor forwarder or proxy) and non-IP addresses (e.g. memory transport).
fn per_source_key(peer_addr: &Multiaddr, exempt_cidrs: &[cidr::AnyIpCidr]) -> Option<IpAddr> {
    const IPV6_SLASH_64_MASK: u128 = 0xffff_ffff_ffff_ffff_0000_0000_0000_0000;
    let ip = source_ip(peer_addr)?;
    if ip.is_loopback() {
        return None;
    }
    if source_in_cidrs(ip, exempt_cidrs) {
        return None;
    }
    match ip {
        IpAddr::V4(_) => Some(ip),
        IpAddr::V6(ip) => Some(IpAddr::V6(Ipv6Addr::from(u128::from(ip) & IPV6_SLASH_64_MASK))),
    }
}

/// State a listener shares with the other listeners of its node: the self-check liveness tokens, the liveness session
/// permits, the single self-check session permit and the refusal log are node-wide. The inbound connection counts
/// belong to one listener (see [ListenerSharedState::with_own_counts]), so each listener has its own inbound limits.
#[derive(Debug, Clone)]
pub(crate) struct ListenerSharedState {
    pub liveness_tokens: LivenessTokens,
    liveness_sessions: Arc<Semaphore>,
    /// One permit: the self-check only ever needs one session, so a captured token cannot buy more than one
    /// permit-free session.
    self_check_sessions: Arc<Semaphore>,
    counts: Arc<Mutex<InboundCounts>>,
    refusals: Arc<Mutex<RefusalLog>>,
}

impl ListenerSharedState {
    pub fn new(config: &ConnectionManagerConfig) -> Self {
        Self {
            liveness_tokens: LivenessTokens::default(),
            liveness_sessions: Arc::new(Semaphore::new(effective_liveness_sessions(config))),
            self_check_sessions: Arc::new(Semaphore::new(1)),
            counts: Arc::new(Mutex::new(InboundCounts::default())),
            refusals: Arc::new(Mutex::new(RefusalLog::default())),
        }
    }

    /// The same node-wide liveness state and refusal log, with fresh inbound connection counts for another listener.
    pub fn with_own_counts(&self) -> Self {
        Self {
            liveness_tokens: self.liveness_tokens.clone(),
            liveness_sessions: self.liveness_sessions.clone(),
            self_check_sessions: self.self_check_sessions.clone(),
            counts: Arc::new(Mutex::new(InboundCounts::default())),
            refusals: self.refusals.clone(),
        }
    }
}

#[cfg(test)]
impl ListenerSharedState {
    /// The number of inbound futures (handshakes and liveness sessions) currently running.
    pub(crate) fn handshakes_in_flight(&self) -> usize {
        self.counts.lock().expect("inbound counts lock poisoned").in_flight
    }

    /// The number of established inbound connections counted against `max_inbound_connections`.
    pub(crate) fn established_inbound(&self) -> usize {
        self.counts.lock().expect("inbound counts lock poisoned").established
    }
}

/// Held by an inbound future for its whole lifetime. Until the connection is established, it counts the connection
/// as in flight; dropping it releases the in-flight slot (and the per-source slot, if the source counts towards one)
/// on every exit path.
struct InboundHandshakeGuard {
    counts: Arc<Mutex<InboundCounts>>,
    source: Option<IpAddr>,
    /// Set once the count has moved to an [EstablishedInboundGuard], after which dropping this guard does nothing.
    established: bool,
}

impl InboundHandshakeGuard {
    fn new(counts: Arc<Mutex<InboundCounts>>, source: Option<IpAddr>) -> Self {
        Self {
            counts,
            source,
            established: false,
        }
    }

    /// Moves the connection from in flight to established, under one lock so it is never counted twice. The
    /// connection stays counted as established for as long as the returned guard lives. Call at most once.
    fn establish(&mut self) -> EstablishedInboundGuard {
        let mut counts = self.counts.lock().expect("inbound counts lock poisoned");
        counts.in_flight = counts.in_flight.saturating_sub(1);
        counts.established = counts.established.saturating_add(1);
        if let Some(source) = self.source {
            decrement_source(&mut counts.in_flight_per_source, source);
            increment_source(&mut counts.established_per_source, source);
        }
        drop(counts);
        self.established = true;
        EstablishedInboundGuard {
            counts: self.counts.clone(),
            source: self.source,
        }
    }
}

impl Drop for InboundHandshakeGuard {
    fn drop(&mut self) {
        if self.established {
            return;
        }
        let mut counts = self.counts.lock().expect("inbound counts lock poisoned");
        counts.in_flight = counts.in_flight.saturating_sub(1);
        if let Some(source) = self.source {
            decrement_source(&mut counts.in_flight_per_source, source);
        }
    }
}

/// Counts one established inbound connection, in total and for its source. It is stored in the connection's actor,
/// so the count follows the actor's lifetime and is released however the connection ends.
#[derive(Debug)]
pub(crate) struct EstablishedInboundGuard {
    counts: Arc<Mutex<InboundCounts>>,
    source: Option<IpAddr>,
}

impl Drop for EstablishedInboundGuard {
    fn drop(&mut self) {
        let mut counts = self.counts.lock().expect("inbound counts lock poisoned");
        counts.established = counts.established.saturating_sub(1);
        if let Some(source) = self.source {
            decrement_source(&mut counts.established_per_source, source);
        }
    }
}

/// Listens on the given transport for peer connections and notifies when a new inbound peer connection is established.
pub struct PeerListener<TTransport> {
    config: ConnectionManagerConfig,
    bind_address: Multiaddr,
    bounded_executor: BoundedExecutor,
    conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
    shutdown_signal: ShutdownSignal,
    transport: TTransport,
    noise_config: NoiseConfig,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    our_supported_protocols: Arc<Vec<ProtocolId>>,
    shared_state: ListenerSharedState,
    on_listening: OneshotTrigger<Result<Multiaddr, ConnectionManagerError>>,
}

impl<TTransport> PeerListener<TTransport>
where
    TTransport: Transport + Clone + Send + Sync + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
{
    pub fn new(
        mut config: ConnectionManagerConfig,
        bind_address: Multiaddr,
        transport: TTransport,
        noise_config: NoiseConfig,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        // Source addresses are compared in canonical form, so IPv4-mapped CIDRs must be in IPv4 form to match
        normalise_ipv4_mapped_cidrs(&mut config.liveness_cidr_allowlist);
        normalise_ipv4_mapped_cidrs(&mut config.listener_per_ip_exempt_cidrs);
        Self {
            transport,
            bind_address,
            noise_config,
            conn_man_notifier,
            peer_manager,
            node_identity,
            shutdown_signal,
            our_supported_protocols: Arc::new(Vec::new()),
            bounded_executor: BoundedExecutor::new(config.max_simultaneous_inbound_connects),
            shared_state: ListenerSharedState::new(&config),
            config,
            on_listening: oneshot_trigger::channel(),
        }
    }

    /// Returns a future that resolves once the listener has either succeeded (`Ok(bind_addr)`) or failed (`Err(...)`)
    /// in binding the listener socket
    // This returns an impl Future and is not async because we want to exclude &self from the future so that it has a
    // 'static lifetime as well as to flatten the oneshot result for ergonomics
    pub fn on_listening(&self) -> impl Future<Output = Result<Multiaddr, ConnectionManagerError>> + 'static {
        let signal = self.on_listening.to_signal();
        signal.map(|r| r.ok_or(ConnectionManagerError::ListenerOneshotCancelled)?)
    }

    /// Set the supported protocols of this node to send to peers during the peer identity exchange
    pub fn set_supported_protocols(&mut self, our_supported_protocols: Vec<ProtocolId>) -> &mut Self {
        self.our_supported_protocols = Arc::new(our_supported_protocols);
        self
    }

    /// Share the self-check liveness tokens, liveness session permits and inbound connection counts with other
    /// listeners of this node.
    pub(crate) fn set_shared_state(&mut self, shared_state: ListenerSharedState) -> &mut Self {
        self.shared_state = shared_state;
        self
    }

    /// The tokens this listener accepts for the node's own liveness self-check.
    #[cfg(test)]
    pub(crate) fn liveness_tokens(&self) -> LivenessTokens {
        self.shared_state.liveness_tokens.clone()
    }

    /// The shared state, so tests can observe the inbound counts and share it with another listener.
    #[cfg(test)]
    pub(crate) fn shared_state(&self) -> ListenerSharedState {
        self.shared_state.clone()
    }

    pub async fn listen(self) -> Result<Multiaddr, ConnectionManagerError> {
        let on_listening = self.on_listening();
        tokio::spawn(self.run());
        on_listening.await
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self.shutdown_signal.clone();

        match self.bind().await {
            Ok((mut inbound, address)) => {
                info!(target: LOG_TARGET, "Listening for peer connections on '{address}'");

                self.on_listening.broadcast(Ok(address));

                loop {
                    tokio::select! {
                        biased;

                        _ = &mut shutdown_signal => {
                            info!(target: LOG_TARGET, "PeerListener is shutting down because the shutdown signal was triggered");
                            break;
                        },
                        Some(inbound_result) = inbound.next() => {
                            if let Some((socket, peer_addr)) = log_if_error!(target: LOG_TARGET, inbound_result, "Inbound connection failed because '{error}'",) {
                                self.spawn_listen_task(socket, peer_addr);
                            }
                        },
                    }
                }
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "PeerListener was unable to start because '{err}'");
                self.on_listening.broadcast(Err(err));
            },
        }
    }

    async fn read_wire_format(
        socket: &mut TTransport::Output,
        time_to_first_byte: Duration,
    ) -> Result<WireMode, Error> {
        let mut buf = [0u8; 1];
        match time::timeout(time_to_first_byte, socket.read_exact(&mut buf)).await {
            Ok(result) => match result {
                Ok(_) => match buf[0].try_into().ok() {
                    Some(wf) => Ok(wf),
                    None => {
                        warn!(target: LOG_TARGET, "Invalid wire format byte '{}'", buf[0]);
                        Err(ErrorKind::InvalidData.into())
                    },
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to read wire format byte due to error: {err}"
                    );
                    Err(err)
                },
            },
            Err(elapsed) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to read wire format byte within timeout of {time_to_first_byte:#?}. {elapsed}"
                );
                Err(elapsed.into())
            },
        }
    }

    fn is_address_in_liveness_cidr_range(addr: &Multiaddr, allowlist: &[cidr::AnyIpCidr]) -> bool {
        match source_ip(addr) {
            Some(ip) => source_in_cidrs(ip, allowlist),
            None => {
                warn!(
                    target: LOG_TARGET,
                    "Peer address '{addr}' is invalid for liveness checks. It must be an TCP/IP address."
                );
                false
            },
        }
    }

    /// Counts and logs an inbound socket refused before the handshake: a `debug!` for each, a metric, and a `warn!`
    /// summarising the refusals at most once per [REFUSAL_WARN_INTERVAL].
    fn record_refusal(&self, peer_addr: &Multiaddr, reason: &'static str) {
        debug!(
            target: LOG_TARGET,
            "Refusing inbound connection from '{peer_addr}' before the handshake ({reason})"
        );
        #[cfg(feature = "metrics")]
        metrics::inbound_handshake_rejected(reason).inc();
        let summary = self
            .shared_state
            .refusals
            .lock()
            .expect("refusal log lock poisoned")
            .record(reason, time::Instant::now());
        if let Some(summary) = summary {
            warn!(
                target: LOG_TARGET,
                "Refusing inbound connections because inbound limits are reached ({summary}). If these are \
                 legitimate peers, raise `max_inbound_connections`, `max_inbound_connections_per_ip` \
                 or `listener_max_handshakes_per_ip`, or add a proxy that inbound connections arrive from to \
                 `listener_per_ip_exempt_cidrs`."
            );
        }
    }

    /// Admits a freshly accepted socket against the inbound connection caps and the per-source handshake cap,
    /// before anything is read from it. On success the returned guard holds the in-flight slots until it is dropped.
    /// Returns None, after logging and counting the refusal, if the socket must be closed.
    fn try_admit_inbound(&self, peer_addr: &Multiaddr) -> Option<InboundHandshakeGuard> {
        let source = per_source_key(peer_addr, &self.config.listener_per_ip_exempt_cidrs);
        let result = self
            .shared_state
            .counts
            .lock()
            .expect("inbound counts lock poisoned")
            .try_admit(source, &self.config);
        match result {
            Ok(()) => Some(InboundHandshakeGuard::new(self.shared_state.counts.clone(), source)),
            Err(reason) => {
                self.record_refusal(peer_addr, reason);
                None
            },
        }
    }

    /// Handles a connection that sent the liveness wire byte.
    ///
    /// A first line that is one of this node's self-check tokens starts a self-check echo session; this needs no
    /// allowlist or session permit because only this node could know the token. Any other first line is only served
    /// if the source is allow-listed and a session permit is free, in which case it is echoed back and the session
    /// carries on as a plain echo server.
    async fn handle_liveness_connection(
        socket: TTransport::Output,
        peer_addr: &Multiaddr,
        config: &ConnectionManagerConfig,
        liveness_tokens: &LivenessTokens,
        liveness_sessions: Arc<Semaphore>,
        self_check_sessions: Arc<Semaphore>,
        shutdown_signal: ShutdownSignal,
    ) {
        let mut session = SelfLivenessSession::new(socket);
        let first_line = match session.read_line(LIVENESS_FIRST_LINE_TIMEOUT).await {
            Some(line) => line,
            None => {
                debug!(
                    target: LOG_TARGET,
                    "Liveness connection from '{peer_addr}' did not send a valid first line"
                );
                return;
            },
        };

        if take_liveness_token(liveness_tokens, &first_line) {
            // This node's own self-check: it ignores the allowlist and the liveness session limit, but only one
            // self-check session may be open node-wide, and it has the same idle timeout and lifetime as any other
            // liveness session. The token is consumed either way.
            let self_check_permit = match self_check_sessions.try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    debug!(
                        target: LOG_TARGET,
                        "Refusing self-check liveness session from '{peer_addr}': one is already open"
                    );
                    return;
                },
            };
            debug!(target: LOG_TARGET, "Self-check liveness session from '{peer_addr}'");
            Self::run_liveness_session(session, first_line, peer_addr, shutdown_signal).await;
            drop(self_check_permit);
            return;
        }

        if !Self::is_address_in_liveness_cidr_range(peer_addr, &config.liveness_cidr_allowlist) {
            debug!(
                target: LOG_TARGET,
                "Liveness session not permitted for peer address '{peer_addr}'"
            );
            return;
        }

        let permit = match liveness_sessions.try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                debug!(
                    target: LOG_TARGET,
                    "No liveness sessions available for peer address '{peer_addr}'"
                );
                return;
            },
        };

        debug!(target: LOG_TARGET, "Started liveness session for '{peer_addr}'");
        Self::run_liveness_session(session, first_line, peer_addr, shutdown_signal).await;
        drop(permit);
    }

    /// Echoes `first_line`, then keeps echoing until the session ends or the node shuts down.
    async fn run_liveness_session(
        mut session: SelfLivenessSession<TTransport::Output>,
        first_line: String,
        peer_addr: &Multiaddr,
        shutdown_signal: ShutdownSignal,
    ) {
        if let Err(err) = session.send_line(first_line).await {
            debug!(target: LOG_TARGET, "Liveness session for '{peer_addr}' failed: {err}");
            return;
        }
        tokio::select! {
            result = session.run() => {
                if let Err(err) = result {
                    debug!(target: LOG_TARGET, "Liveness session for '{peer_addr}' failed: {err}");
                }
            },
            _ = shutdown_signal => {},
        }
        debug!(target: LOG_TARGET, "Liveness session for '{peer_addr}' ended");
    }

    fn spawn_listen_task(&self, mut socket: TTransport::Output, peer_addr: Multiaddr) {
        // Checked before anything is read, so a refused socket costs no handshake work. Dropping the socket closes it.
        let Some(mut handshake_guard) = self.try_admit_inbound(&peer_addr) else {
            return;
        };

        let node_identity = self.node_identity.clone();
        let peer_manager = self.peer_manager.clone();
        let conn_man_notifier = self.conn_man_notifier.clone();
        let noise_config = self.noise_config.clone();
        let config = self.config.clone();
        let our_supported_protocols = self.our_supported_protocols.clone();
        let liveness_tokens = self.shared_state.liveness_tokens.clone();
        let liveness_sessions = self.shared_state.liveness_sessions.clone();
        let self_check_sessions = self.shared_state.self_check_sessions.clone();
        let shutdown_signal = self.shutdown_signal.clone();
        let peer_addr_for_log = peer_addr.clone();

        let span = span!(Level::TRACE, "connection_mann::listener::inbound_task",);
        let inbound_fut = async move {
            #[cfg(feature = "metrics")]
            metrics::pending_connections(ConnectionDirection::Inbound).inc();
            // The inbound handshake timeout bounds the unauthenticated stages only: the wire byte, noise and the
            // identity exchange. Liveness sessions are bounded by their own timeouts instead. Dropping a stage on
            // expiry closes the socket.
            let started = time::Instant::now();
            let wire_mode_result = time::timeout(
                config.inbound_handshake_timeout,
                Self::read_wire_format(&mut socket, config.time_to_first_byte),
            )
            .await;
            match wire_mode_result {
                Ok(Ok(WireMode::Comms(byte))) if byte == config.network_info.network_wire_byte => {
                    let this_node_id_str = node_identity.node_id().short_str();
                    let handshake_budget = config.inbound_handshake_timeout.saturating_sub(started.elapsed());
                    let result = Self::perform_socket_upgrade_procedure(
                        &node_identity,
                        &peer_manager,
                        noise_config.clone(),
                        conn_man_notifier.clone(),
                        socket,
                        peer_addr.clone(),
                        our_supported_protocols,
                        &mut handshake_guard,
                        handshake_budget,
                        &config,
                    )
                    .await;
                    Self::publish_inbound_result(&conn_man_notifier, &this_node_id_str, &peer_addr, result).await;
                },
                Ok(Ok(WireMode::Comms(byte))) => {
                    debug!(
                        target: LOG_TARGET,
                        "Peer at address '{}' sent invalid wire format byte. Expected {:x?} got: {:x?} ",
                        peer_addr,
                        config.network_info.network_wire_byte,
                        byte,
                    );
                    let _result = socket.shutdown().await;
                },
                Ok(Ok(WireMode::Liveness)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Connection at address '{peer_addr}' requested liveness session"
                    );
                    Self::handle_liveness_connection(
                        socket,
                        &peer_addr,
                        &config,
                        &liveness_tokens,
                        liveness_sessions,
                        self_check_sessions,
                        shutdown_signal,
                    )
                    .await;
                },
                Ok(Err(err)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Peer at address '{}' failed to send its wire format. Expected network byte {:x?} or liveness \
                         byte {:x?} not received. Error: {}",
                        peer_addr,
                        config.network_info.network_wire_byte,
                        LIVENESS_WIRE_MODE,
                        err
                    );
                },
                Err(_elapsed) => {
                    let this_node_id_str = node_identity.node_id().short_str();
                    let result = Err(ConnectionManagerError::InboundHandshakeTimeout);
                    Self::publish_inbound_result(&conn_man_notifier, &this_node_id_str, &peer_addr, result).await;
                },
            }

            #[cfg(feature = "metrics")]
            metrics::pending_connections(ConnectionDirection::Inbound).dec();
            drop(handshake_guard);
        }
        .instrument(span);

        // Never wait for a slot: a full executor means the socket is closed straight away, so silent sockets cannot
        // stall the accept loop
        if self.bounded_executor.try_spawn(inbound_fut).is_err() {
            self.record_refusal(&peer_addr_for_log, "slots_full");
        }
    }

    /// Publishes the outcome of an inbound upgrade to the connection manager.
    /// Hands the result of an inbound upgrade to the connection manager. Both sends are bounded so that a busy
    /// connection manager cannot park this task (and the listener permit it holds). The connection manager never learns
    /// of a connection whose `PeerConnected` event was dropped, so nothing would use or close it: it is closed here.
    async fn publish_inbound_result(
        conn_man_notifier: &mpsc::Sender<ConnectionManagerEvent>,
        this_node_id_str: &str,
        peer_addr: &Multiaddr,
        result: Result<PeerConnection, ConnectionManagerError>,
    ) {
        match result {
            Ok(peer_conn) => {
                let peer = peer_conn.peer_node_id().clone();
                let event = ConnectionManagerEvent::PeerConnected(peer_conn.into());
                let result = send_or_drop(conn_man_notifier, event, "listener_peer_connected", Some(&peer)).await;
                if let Err(err) = result &&
                    let ConnectionManagerEvent::PeerConnected(conn) = err.into_inner()
                {
                    close_unannounced_connection(*conn).await;
                }
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "[ThisNode={this_node_id_str}] Peer connection upgrade failed for peer because '{err:?}'"
                );
                notify_inbound_connect_failed(conn_man_notifier, err, peer_addr).await;
            },
        }
    }

    async fn perform_socket_upgrade_procedure(
        node_identity: &NodeIdentity,
        peer_manager: &PeerManager,
        noise_config: NoiseConfig,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        socket: TTransport::Output,
        peer_addr: Multiaddr,
        our_supported_protocols: Arc<Vec<ProtocolId>>,
        handshake_guard: &mut InboundHandshakeGuard,
        handshake_budget: Duration,
        config: &ConnectionManagerConfig,
    ) -> Result<PeerConnection, ConnectionManagerError> {
        const CONNECTION_DIRECTION: ConnectionDirection = ConnectionDirection::Inbound;
        trace!(
            target: LOG_TARGET,
            "Listen - starting noise protocol upgrade for peer at address '{peer_addr}'"
        );

        // `handshake_budget` is what is left of the inbound handshake timeout. It bounds the noise upgrade and the
        // identity exchange only; the peer database calls and the multiplexer upgrade are not cut by it.
        let timer = time::Instant::now();
        // Write stalls are measured on the transport, under noise
        let noise_result = time::timeout(
            handshake_budget,
            noise_config.upgrade_socket(WriteStallSocket::new(socket), CONNECTION_DIRECTION),
        )
        .await
        .map_err(|_elapsed| ConnectionManagerError::InboundHandshakeTimeout)?;
        let mut noise_socket = noise_result.map_err(|err| {
            debug!(
                target: LOG_TARGET,
                "Listen - failed to upgrade noise: {} on address: {} ({})",
                node_identity.node_id(),
                peer_addr,
                err
            );
            err
        })?;

        let authenticated_public_key = noise_socket
            .get_remote_public_key()
            .ok_or(ConnectionManagerError::InvalidStaticPublicKey)?;
        let latency = timer.elapsed();
        let identity_budget = handshake_budget.saturating_sub(latency);

        trace!(
            target: LOG_TARGET,
            "Listen - noise socket upgrade completed in {latency:.2?} with public key '{authenticated_public_key}'"
        );

        // Check if we know the peer and if it is banned
        let known_peer = common::find_unbanned_peer(peer_manager, &authenticated_public_key).await?;

        trace!(
            target: LOG_TARGET,
            "Listen - starting peer identity exchange for peer with public key '{authenticated_public_key}'"

        );

        let peer_identity_result = time::timeout(
            identity_budget,
            common::perform_identity_exchange(
                &mut noise_socket,
                node_identity,
                &*our_supported_protocols,
                config.network_info.clone(),
            ),
        )
        .await
        .map_err(|_elapsed| ConnectionManagerError::InboundHandshakeTimeout)?;

        let peer_identity =
            common::ban_on_offence(peer_manager, &authenticated_public_key, peer_identity_result).await?;

        let valid_peer_identity_result = common::validate_peer_identity_message(
            &config.peer_validation_config,
            &authenticated_public_key,
            peer_identity,
        );

        let valid_peer_identity =
            common::ban_on_offence(peer_manager, &authenticated_public_key, valid_peer_identity_result).await?;

        let peer = common::create_or_update_peer_from_validated_peer_identity(
            known_peer,
            authenticated_public_key.clone(),
            &valid_peer_identity,
        );

        let peer_connection_info = PeerConnectionInfo::new(
            Some(authenticated_public_key.clone()),
            Some(valid_peer_identity.claim.features),
            Some(valid_peer_identity.metadata.user_agent.clone()),
        );
        let write_stall = noise_socket.get_ref().write_stall();
        let muxer = Yamux::upgrade_connection_with_write_stall(
            noise_socket,
            CONNECTION_DIRECTION,
            peer_connection_info,
            write_stall,
        )
        .map_err(|err| ConnectionManagerError::YamuxUpgradeFailure(err.to_string()))?;

        let conn = peer_connection::create(
            muxer,
            peer_addr,
            peer.node_id.clone(),
            peer.features,
            CONNECTION_DIRECTION,
            conn_man_notifier,
            our_supported_protocols,
            valid_peer_identity.metadata.supported_protocols,
            Some(handshake_guard.establish()),
        );

        peer_manager.add_or_update_peer(peer).await?;

        Ok(conn)
    }

    async fn bind(&mut self) -> Result<(TTransport::Listener, Multiaddr), ConnectionManagerError> {
        let bind_address = self.bind_address.clone();
        debug!(target: LOG_TARGET, "Attempting to listen on {bind_address}");
        self.transport
            .listen(&bind_address)
            .await
            .map_err(|err| ConnectionManagerError::ListenerError {
                address: bind_address.to_string(),
                details: err.to_string(),
            })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn key(addr: &str, exempt_cidrs: &[&str]) -> Option<IpAddr> {
        let mut cidrs = Vec::new();
        for cidr in exempt_cidrs {
            cidrs.push(cidr.parse::<cidr::AnyIpCidr>().unwrap());
        }
        per_source_key(&addr.parse().unwrap(), &cidrs)
    }

    fn ip(ip: &str) -> Option<IpAddr> {
        Some(ip.parse().unwrap())
    }

    #[test]
    fn per_source_key_canonicalises_before_exempting() {
        assert_eq!(key("/ip4/127.0.0.1/tcp/1", &[]), None);
        assert_eq!(key("/ip6/::1/tcp/1", &[]), None);
        // IPv4-mapped loopback is loopback
        assert_eq!(key("/ip6/::ffff:127.0.0.1/tcp/1", &[]), None);
        // IPv4-mapped addresses share the bucket of the IPv4 address
        assert_eq!(key("/ip6/::ffff:203.0.113.7/tcp/1", &[]), ip("203.0.113.7"));
        assert_eq!(key("/ip4/203.0.113.7/tcp/1", &[]), ip("203.0.113.7"));
        // Configured exemptions, matched against the canonical address
        assert_eq!(key("/ip4/10.1.2.3/tcp/1", &["10.0.0.0/8"]), None);
        assert_eq!(key("/ip6/::ffff:10.1.2.3/tcp/1", &["10.0.0.0/8"]), None);
        assert_eq!(key("/ip6/2001:db8::1/tcp/1", &["2001:db8::/32"]), None);
        assert_eq!(key("/ip4/11.1.2.3/tcp/1", &["10.0.0.0/8"]), ip("11.1.2.3"));
        // Non-IP sources are exempt
        assert_eq!(key("/memory/1", &[]), None);
    }

    #[test]
    fn wide_ipv6_cidrs_match_ipv4_sources() {
        // `::ffff:0:0/95` has a host bit set and is rejected by the parser; `::fffe:0:0/95` is the /95 that covers
        // the IPv4-mapped range
        assert!("::ffff:0:0/95".parse::<cidr::AnyIpCidr>().is_err());
        for cidr in ["::/0", "::fffe:0:0/95"] {
            // Per-source exemption, for an IPv4 and an IPv4-mapped source
            assert_eq!(key("/ip4/203.0.113.7/tcp/1", &[cidr]), None, "{cidr}");
            assert_eq!(key("/ip6/::ffff:203.0.113.7/tcp/1", &[cidr]), None, "{cidr}");

            // Liveness allowlist, for an IPv4 and an IPv4-mapped source
            let allowlist = vec![cidr.parse::<cidr::AnyIpCidr>().unwrap()];
            for addr in ["/ip4/203.0.113.7/tcp/1", "/ip6/::ffff:203.0.113.7/tcp/1"] {
                assert!(
                    PeerListener::<crate::transports::MemoryTransport>::is_address_in_liveness_cidr_range(
                        &addr.parse().unwrap(),
                        &allowlist
                    ),
                    "{cidr} {addr}"
                );
            }
        }
        // A /95 that does not cover the IPv4-mapped range does not match
        assert_eq!(key("/ip4/203.0.113.7/tcp/1", &["::fffc:0:0/95"]), ip("203.0.113.7"));
        // A native IPv6 source is still bucketed normally
        assert_eq!(key("/ip6/2001:db8::1/tcp/1", &["::fffe:0:0/95"]), ip("2001:db8::"));
    }

    #[test]
    fn ipv6_sources_are_bucketed_by_slash_64() {
        assert_eq!(key("/ip6/2001:db8::1/tcp/1", &[]), ip("2001:db8::"));
        assert_eq!(key("/ip6/2001:db8::ffff:1:2:3/tcp/1", &[]), ip("2001:db8::"));
        assert_eq!(key("/ip6/2001:db8:0:1::1/tcp/1", &[]), ip("2001:db8:0:1::"));

        // Addresses in one /64 share the per-source handshake limit; another /64 does not
        let config = ConnectionManagerConfig {
            max_inbound_handshakes_per_ip: 4,
            ..Default::default()
        };
        let mut counts = InboundCounts::default();
        for addr in [
            "/ip6/2001:db8::1/tcp/1",
            "/ip6/2001:db8::2/tcp/1",
            "/ip6/2001:db8::3/tcp/1",
            "/ip6/2001:db8::4/tcp/1",
        ] {
            assert_eq!(counts.try_admit(key(addr, &[]), &config), Ok(()));
        }
        assert_eq!(
            counts.try_admit(key("/ip6/2001:db8::ffff:1/tcp/1", &[]), &config),
            Err("per_ip")
        );
        assert_eq!(
            counts.try_admit(key("/ip6/2001:db8:0:1::1/tcp/1", &[]), &config),
            Ok(())
        );
    }

    #[test]
    fn per_source_inbound_cap_counts_established_and_in_flight() {
        let config = ConnectionManagerConfig {
            max_inbound_connections_per_ip: 3,
            max_inbound_handshakes_per_ip: 4,
            ..Default::default()
        };
        let counts = Arc::new(Mutex::new(InboundCounts::default()));
        let source = ip("203.0.113.7");

        // Two established connections and one handshake from the source fill its limit of three
        let mut established = Vec::new();
        for _ in 0..2 {
            counts.lock().unwrap().try_admit(source, &config).unwrap();
            let mut guard = InboundHandshakeGuard::new(counts.clone(), source);
            established.push(guard.establish());
        }
        counts.lock().unwrap().try_admit(source, &config).unwrap();
        let _in_flight = InboundHandshakeGuard::new(counts.clone(), source);
        assert_eq!(
            counts.lock().unwrap().try_admit(source, &config),
            Err("inbound_cap_per_ip")
        );

        // Other and exempt sources are unaffected
        assert_eq!(counts.lock().unwrap().try_admit(ip("203.0.113.8"), &config), Ok(()));
        assert_eq!(counts.lock().unwrap().try_admit(None, &config), Ok(()));

        // Closing an established connection frees its place for the source
        established.pop();
        assert_eq!(counts.lock().unwrap().try_admit(source, &config), Ok(()));
        assert_eq!(counts.lock().unwrap().established, 1);
    }

    #[test]
    fn liveness_sessions_cannot_take_every_handshake_slot() {
        let config = ConnectionManagerConfig {
            liveness_max_sessions: 100,
            max_simultaneous_inbound_connects: 10,
            ..Default::default()
        };
        assert_eq!(effective_liveness_sessions(&config), 5);

        let config = ConnectionManagerConfig {
            liveness_max_sessions: 2,
            max_simultaneous_inbound_connects: 100,
            ..Default::default()
        };
        assert_eq!(effective_liveness_sessions(&config), 2);

        let config = ConnectionManagerConfig {
            liveness_max_sessions: 1,
            max_simultaneous_inbound_connects: 1,
            ..Default::default()
        };
        assert_eq!(effective_liveness_sessions(&config), 0);
    }

    #[test]
    fn refusal_warnings_are_rate_limited() {
        let mut log = RefusalLog::default();
        let start = time::Instant::now();

        // The first refusal is reported straight away
        assert_eq!(log.record("per_ip", start), Some("per_ip: 1".to_string()));

        // Refusals within the interval are only counted
        assert_eq!(log.record("per_ip", start + Duration::from_secs(1)), None);
        assert_eq!(log.record("inbound_cap", start + Duration::from_secs(2)), None);
        assert_eq!(log.record("per_ip", start + Duration::from_secs(59)), None);

        // The next one after the interval reports everything counted since the last report
        assert_eq!(
            log.record("slots_full", start + Duration::from_secs(60)),
            Some("inbound_cap: 1, per_ip: 2, slots_full: 1".to_string())
        );
        assert_eq!(log.record("per_ip", start + Duration::from_secs(61)), None);
    }
}
