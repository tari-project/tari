//  Copyright 2021, The Tari Project
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

// mod chunking;

mod error;
pub use error::RpcServerError;

mod handle;
pub use handle::RpcServerHandle;
use handle::RpcServerRequest;

#[cfg(feature = "metrics")]
mod metrics;

pub mod mock;

mod early_close;
mod request_codec;
mod router;

use std::{
    borrow::Cow,
    cmp,
    collections::HashMap,
    convert::TryFrom,
    future::Future,
    io,
    io::ErrorKind,
    pin::Pin,
    sync::Arc,
    task::Poll,
    time::{Duration, Instant},
};

use futures::{SinkExt, StreamExt, future, future::BoxFuture, stream::FuturesUnordered};
use log::*;
use prost::Message;
use request_codec::{RequestCodec, RequestFrame};
use router::Router;
use tokio::{sync::mpsc, task::JoinSet, time};
use tokio_stream::Stream;
use tokio_util::codec::{Framed, FramedParts};
use tower::{Service, make::MakeService};
use tracing::{Instrument, Level, debug, error, instrument, span, trace, warn};

use super::{
    Handshake,
    RPC_MAX_FRAME_SIZE,
    body::Body,
    context::{RequestContext, RpcCommsProvider},
    error::HandshakeRejectReason,
    message::{Request, Response, RpcMessageFlags},
    not_found::ProtocolServiceNotFound,
    status::RpcStatus,
};
use crate::{
    Bytes,
    BytesMut,
    Substream,
    bounded_executor::BoundedExecutor,
    framing,
    framing::CanonicalFraming,
    message::MessageExt,
    peer_manager::NodeId,
    proto,
    protocol::{
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
        ProtocolNotificationRx,
        rpc,
        rpc::{
            body::BodyBytes,
            message::{RpcMethod, RpcResponse},
            server::early_close::EarlyClose,
        },
    },
    stream_id::{Id, StreamId},
};

const LOG_TARGET: &str = "comms::rpc::server";

pub trait NamedProtocolService {
    const PROTOCOL_NAME: &'static [u8];

    /// Default implementation that returns a pointer to the static protocol name.
    fn as_protocol_name(&self) -> &'static [u8] {
        Self::PROTOCOL_NAME
    }
}

pub struct RpcServer {
    builder: RpcServerBuilder,
    request_tx: mpsc::Sender<RpcServerRequest>,
    request_rx: mpsc::Receiver<RpcServerRequest>,
}

impl RpcServer {
    pub fn new() -> Self {
        Self::builder().finish()
    }

    pub fn builder() -> RpcServerBuilder {
        RpcServerBuilder::new()
    }

    pub fn add_service<S>(self, service: S) -> Router<S, ProtocolServiceNotFound>
    where
        S: MakeService<
                ProtocolId,
                Request<Bytes>,
                MakeError = RpcServerError,
                Response = Response<Body>,
                Error = RpcStatus,
            > + NamedProtocolService
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        Router::new(self, service)
    }

    pub fn get_handle(&self) -> RpcServerHandle {
        RpcServerHandle::new(self.request_tx.clone())
    }

    pub(super) async fn serve<S, TCommsProvider>(
        self,
        service: S,
        notifications: ProtocolNotificationRx<Substream>,
        comms_provider: TCommsProvider,
    ) -> Result<(), RpcServerError>
    where
        S: MakeService<
                ProtocolId,
                Request<Bytes>,
                MakeError = RpcServerError,
                Response = Response<Body>,
                Error = RpcStatus,
            > + Send
            + 'static,
        S::Service: Send + 'static,
        S::Future: Send + 'static,
        S::Service: Send + 'static,
        <S::Service as Service<Request<Bytes>>>::Future: Send + 'static,
        TCommsProvider: RpcCommsProvider + Clone + Send + 'static,
    {
        PeerRpcServer::new(self.builder, service, notifications, comms_provider, self.request_rx)
            .serve()
            .await
    }
}

impl Default for RpcServer {
    fn default() -> Self {
        Self::new()
    }
}

/// The default maximum time a session may sit without receiving a request before it is closed. This
/// is well above any client's polling interval, so it only ever reclaims sessions whose peer is
/// gone.
const DEFAULT_IDLE_SESSION_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// The default ceiling applied to the deadline a client asks for. The largest deadline any client
/// in this workspace sets is the 240s block-sync deadline (sized for full blocks over Tor), so this
/// leaves generous headroom for an operator who raises it while still bounding how long a single
/// request can occupy a session. It matches [`DEFAULT_IDLE_SESSION_TIMEOUT`] deliberately: a
/// request in flight should not be able to hold a session slot for longer than an idle one may sit.
const DEFAULT_MAXIMUM_CLIENT_DEADLINE: Duration = Duration::from_secs(10 * 60);

/// How long the server will spend trying to shut a substream down gracefully before dropping it.
///
/// A close is a handful of bytes, so this only ever elapses when the peer is not reading. It is
/// deliberately short and not client-controlled: unlike a request deadline there is no legitimate
/// reason for teardown to take long, and the whole point is to stop a stalled peer from holding the
/// session's executor permit.
const SESSION_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a session that was told to stop (culled, or closed for its peer) spends trying to shut its substream down
/// gracefully before dropping it. Kept well under a second so that a culled session's slot is free within a second
/// even if its peer is not reading.
const STOPPED_SESSION_CLOSE_TIMEOUT: Duration = Duration::from_millis(500);

/// The longest frame read while a request's service call runs, to see whether the client abandoned it with a FIN. A FIN
/// request has no payload, so it is at most 29 bytes: a tag byte for each of its four integer fields, three u32
/// varints (5 bytes each) and a u64 varint (10 bytes). This leaves room for a client that sets a short payload as well.
const MAX_FIN_REQUEST_SIZE: usize = 128;

#[derive(Clone)]
pub struct RpcServerBuilder {
    maximum_simultaneous_sessions: Option<usize>,
    maximum_sessions_per_client: Option<usize>,
    minimum_client_deadline: Duration,
    maximum_client_deadline: Duration,
    handshake_timeout: Duration,
    maximum_pending_handshakes: usize,
    maximum_pending_handshakes_per_client: usize,
    idle_session_timeout: Option<Duration>,
    cull_oldest_peer_rpc_connection_on_full: bool,
}

impl RpcServerBuilder {
    fn new() -> Self {
        Default::default()
    }

    pub fn with_maximum_simultaneous_sessions(mut self, limit: usize) -> Self {
        self.maximum_simultaneous_sessions = Some(cmp::min(limit, BoundedExecutor::max_theoretical_tasks()));
        self
    }

    pub fn with_unlimited_simultaneous_sessions(mut self) -> Self {
        self.maximum_simultaneous_sessions = None;
        self
    }

    pub fn with_maximum_sessions_per_client(mut self, limit: usize) -> Self {
        self.maximum_sessions_per_client = Some(cmp::min(limit, BoundedExecutor::max_theoretical_tasks()));
        self
    }

    pub fn with_cull_oldest_peer_rpc_connection_on_full(mut self, cull: bool) -> Self {
        self.cull_oldest_peer_rpc_connection_on_full = cull;
        self
    }

    pub fn with_unlimited_sessions_per_client(mut self) -> Self {
        self.maximum_sessions_per_client = None;
        self
    }

    pub fn with_minimum_client_deadline(mut self, deadline: Duration) -> Self {
        self.minimum_client_deadline = deadline;
        self
    }

    /// Cap the deadline a client is allowed to ask for. The deadline arrives on the wire as a u64
    /// of seconds and bounds both the service call and each message of a streaming response, so
    /// without a ceiling a peer can simply ask for `u64::MAX` and switch off the server's own
    /// timeouts for that request.
    ///
    /// The request is clamped rather than rejected outright: a peer asking for more than the
    /// ceiling is still served, and only sees a difference if the work actually runs past the
    /// ceiling - in which case it gets a prompt `Timeout` status instead of the longer wait it
    /// asked for.
    ///
    /// Note this bounds a single service call and the gap between two messages of a stream. It
    /// does not bound a stream's total duration, which is deliberate: `sync_blocks` streams an
    /// entire initial block download in one request and a total budget would cut it short.
    pub fn with_maximum_client_deadline(mut self, deadline: Duration) -> Self {
        self.maximum_client_deadline = deadline;
        self
    }

    /// The deadline this server will actually apply for a client that asked for `requested`.
    ///
    /// Floored at `minimum_client_deadline` so that a `maximum_client_deadline` misconfigured below
    /// it cannot silently reduce every request's deadline to nothing - which would leave the node
    /// up but aborting every RPC call it is asked to serve.
    fn clamp_client_deadline(&self, requested: Duration) -> Duration {
        let capped = cmp::min(requested, self.maximum_client_deadline);
        cmp::max(capped, self.minimum_client_deadline)
    }

    /// Limit the handshakes in progress at once across all peers. Handshakes run off the accept loop, so a peer that
    /// opens a substream and never sends its handshake only holds one of these until the handshake times out. A new
    /// substream over the limit is dropped without a reply.
    pub fn with_maximum_pending_handshakes(mut self, limit: usize) -> Self {
        self.maximum_pending_handshakes = limit;
        self
    }

    /// Limit the handshakes in progress at once for a single peer. A new substream over the limit is dropped without a
    /// reply.
    pub fn with_maximum_pending_handshakes_per_client(mut self, limit: usize) -> Self {
        self.maximum_pending_handshakes_per_client = limit;
        self
    }

    /// How long a client has to complete the handshake on a new substream
    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    /// Close a session that has not received a request for `timeout`. A session holds a slot in the
    /// global session limit for as long as its substream is open, and a peer that goes away without
    /// closing the substream (common over Tor) never releases it. Only idle time counts; a session
    /// servicing requests is never closed.
    pub fn with_idle_session_timeout(mut self, timeout: Duration) -> Self {
        self.idle_session_timeout = Some(timeout);
        self
    }

    /// Allow sessions to stay open indefinitely while idle. See [`Self::with_idle_session_timeout`]
    /// for why this is generally a bad idea outside of tests.
    pub fn with_no_idle_session_timeout(mut self) -> Self {
        self.idle_session_timeout = None;
        self
    }

    pub fn finish(self) -> RpcServer {
        let (request_tx, request_rx) = mpsc::channel(10);
        RpcServer {
            builder: self,
            request_tx,
            request_rx,
        }
    }
}

/// The default limit on RPC sessions open at once across all peers. Each session may have one request decoding at a
/// time, so this bounds how much decode memory the server can be made to hold. Use
/// [RpcServerBuilder::with_unlimited_simultaneous_sessions] to opt out.
const DEFAULT_MAXIMUM_SIMULTANEOUS_SESSIONS: usize = 100;

/// The default limit on RPC sessions a single peer may have open at once. Use
/// [RpcServerBuilder::with_unlimited_sessions_per_client] to opt out.
const DEFAULT_MAXIMUM_SESSIONS_PER_CLIENT: usize = 10;

/// The default time a client has to send its handshake, and the server to send its reply. A handshake is two frames of
/// a few bytes, but negotiation over a congested Tor circuit can cost a round trip; this also bounds how long a
/// substream that never sends one holds a pending-handshake slot.
const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// The default limit on handshakes in progress at once, across all peers
const DEFAULT_MAXIMUM_PENDING_HANDSHAKES: usize = 32;

/// The default limit on handshakes in progress at once for a single peer. One server serves several protocols, and a
/// client may open a few sessions within one round trip (e.g. discovery, sync on connect, rebootstrap and wallet
/// pools), so a peer with more than this many unfinished handshakes is not an ordinary client.
const DEFAULT_MAXIMUM_PENDING_HANDSHAKES_PER_CLIENT: usize = 4;

/// The shortest interval between two warnings about the same kind of handshake problem
const HANDSHAKE_WARNING_INTERVAL: Duration = Duration::from_secs(60);

impl Default for RpcServerBuilder {
    fn default() -> Self {
        Self {
            maximum_simultaneous_sessions: Some(DEFAULT_MAXIMUM_SIMULTANEOUS_SESSIONS),
            maximum_sessions_per_client: Some(DEFAULT_MAXIMUM_SESSIONS_PER_CLIENT),
            minimum_client_deadline: Duration::from_secs(1),
            maximum_client_deadline: DEFAULT_MAXIMUM_CLIENT_DEADLINE,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
            maximum_pending_handshakes: DEFAULT_MAXIMUM_PENDING_HANDSHAKES,
            maximum_pending_handshakes_per_client: DEFAULT_MAXIMUM_PENDING_HANDSHAKES_PER_CLIENT,
            idle_session_timeout: Some(DEFAULT_IDLE_SESSION_TIMEOUT),
            cull_oldest_peer_rpc_connection_on_full: false,
        }
    }
}

pub(super) struct PeerRpcServer<TSvc, TCommsProvider>
where TSvc: MakeService<ProtocolId, Request<Bytes>>
{
    executor: BoundedExecutor,
    config: RpcServerBuilder,
    service: TSvc,
    protocol_notifications: Option<ProtocolNotificationRx<Substream>>,
    comms_provider: TCommsProvider,
    request_rx: mpsc::Receiver<RpcServerRequest>,
    sessions: HashMap<NodeId, Vec<SessionInfo>>,
    /// The id given to the next session. Unique for the life of the server, unlike yamux stream ids, which every
    /// connection reuses.
    next_session_id: u64,
    /// Resolves to the peer and session id of each session once its task ends, however it ends (a panic included), so
    /// that its session is always released
    tasks: FuturesUnordered<BoxFuture<'static, (NodeId, u64)>>,
    /// Handshakes in progress. Dropping the set (when `serve` returns) aborts them, so none outlives the server.
    handshakes: JoinSet<HandshakeOutcome<<TSvc as MakeService<ProtocolId, Request<Bytes>>>::Service>>,
    /// The peer of each handshake task, and whether the task will accept the session, so its pending slot and session
    /// reservation are released however the task ends
    handshake_peers: HashMap<tokio::task::Id, (NodeId, bool)>,
    /// Pending handshakes per peer
    pending_handshakes: HashMap<NodeId, usize>,
    /// Handshakes per peer that will accept the session if the client completes them. Each holds a reservation
    /// against the per-client session limit, so an accepted handshake is never refused a session afterwards.
    accepted_pending: HashMap<NodeId, usize>,
    /// All handshakes that will accept the session, each holding a reservation against the global session limit
    accepted_pending_total: usize,
    /// Replies to substreams over a pending-handshake limit. Bounded like `handshakes`, and aborted with the server.
    capacity_rejections: JoinSet<()>,
    capacity_warning: RateLimitedWarning,
    timeout_warning: RateLimitedWarning,
    session_limit_warning: RateLimitedWarning,
}

/// Lets a warning through at most once per [HANDSHAKE_WARNING_INTERVAL], counting the ones it holds back
#[derive(Default)]
struct RateLimitedWarning {
    last: Option<Instant>,
    suppressed: usize,
}

impl RateLimitedWarning {
    /// Returns the number of occurrences since the last warning if one should be logged now
    fn occurred(&mut self) -> Option<usize> {
        let now = Instant::now();
        if self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < HANDSHAKE_WARNING_INTERVAL)
        {
            self.suppressed = self.suppressed.saturating_add(1);
            return None;
        }
        self.last = Some(now);
        let count = self.suppressed.saturating_add(1);
        self.suppressed = 0;
        Some(count)
    }
}

/// What a handshake task returns to the accept loop
struct HandshakeOutcome<S> {
    protocol: ProtocolId,
    node_id: NodeId,
    /// The service, the substream and the negotiated version, ready to start a session (the acceptance is not sent
    /// yet); or why the session was not started
    result: Result<(S, CanonicalFraming<Substream>, u32), RpcServerError>,
}

/// What the accept loop decided before handing a substream to a handshake task
enum SessionDecision<S> {
    /// Perform the handshake, then start a session with this service
    Accept(S),
    /// Reply with this rejection, and report this error
    Reject(HandshakeRejectReason, RpcServerError),
}

struct SessionInfo {
    pub(crate) peer_watch: tokio::sync::watch::Sender<()>,
    pub(crate) session_id: u64,
    /// For logging only: stream ids are reused across connections, so they do not identify a session
    pub(crate) stream_id: Id,
}

impl<TSvc, TCommsProvider> PeerRpcServer<TSvc, TCommsProvider>
where
    TSvc: MakeService<
            ProtocolId,
            Request<Bytes>,
            MakeError = RpcServerError,
            Response = Response<Body>,
            Error = RpcStatus,
        > + Send
        + 'static,
    TSvc::Service: Send + 'static,
    <TSvc::Service as Service<Request<Bytes>>>::Future: Send + 'static,
    TSvc::Future: Send + 'static,
    TCommsProvider: RpcCommsProvider + Clone + Send + 'static,
{
    fn new(
        config: RpcServerBuilder,
        service: TSvc,
        protocol_notifications: ProtocolNotificationRx<Substream>,
        comms_provider: TCommsProvider,
        request_rx: mpsc::Receiver<RpcServerRequest>,
    ) -> Self {
        Self {
            executor: match config.maximum_simultaneous_sessions {
                Some(usize::MAX) => BoundedExecutor::allow_maximum(),
                Some(num) => BoundedExecutor::new(num),
                None => BoundedExecutor::allow_maximum(),
            },
            config,
            service,
            protocol_notifications: Some(protocol_notifications),
            comms_provider,
            request_rx,
            sessions: HashMap::new(),
            next_session_id: 0,
            tasks: FuturesUnordered::new(),
            handshakes: JoinSet::new(),
            handshake_peers: HashMap::new(),
            pending_handshakes: HashMap::new(),
            accepted_pending: HashMap::new(),
            accepted_pending_total: 0,
            capacity_rejections: JoinSet::new(),
            capacity_warning: RateLimitedWarning::default(),
            timeout_warning: RateLimitedWarning::default(),
            session_limit_warning: RateLimitedWarning::default(),
        }
    }

    pub async fn serve(mut self) -> Result<(), RpcServerError> {
        let mut protocol_notifs = self
            .protocol_notifications
            .take()
            .expect("PeerRpcServer initialized without protocol_notifications");

        loop {
            tokio::select! {
                maybe_notif = protocol_notifs.recv() => {
                    match maybe_notif {
                        Some(notif) => self.handle_protocol_notification(notif).await?,
                        // No more protocol notifications to come, so we're done
                        None => break,
                    }
                }

                Some((node_id, session_id)) = self.tasks.next() => {
                    self.on_session_complete(&node_id, session_id);
                },

                Some(joined) = self.handshakes.join_next_with_id() => {
                    self.on_handshake_finished(joined);
                },

                Some(_) = self.capacity_rejections.join_next() => {},

                Some(req) = self.request_rx.recv() => {
                     self.handle_request(req).await;
                },
            }
        }

        debug!(
            target: LOG_TARGET,
            "Peer RPC server is shut down because the protocol notification stream ended"
        );

        Ok(())
    }

    async fn handle_request(&mut self, req: RpcServerRequest) {
        #[allow(clippy::enum_glob_use)]
        use RpcServerRequest::*;
        match req {
            GetNumActiveSessions(reply) => {
                let max_sessions = self
                    .config
                    .maximum_simultaneous_sessions
                    .unwrap_or_else(BoundedExecutor::max_theoretical_tasks);
                let num_active = max_sessions.saturating_sub(self.executor.num_available());
                let _ = reply.send(num_active);
            },
            GetNumActiveSessionsForPeer(node_id, reply) => {
                let num_active = self.sessions.get(&node_id).map(|v| v.len()).unwrap_or(0);
                let _ = reply.send(num_active);
            },
            CloseAllSessionsForPeer(node_id, reply) => {
                let num_closed = self.close_all_sessions(&node_id);
                let _ = reply.send(num_closed);
            },
        }
    }

    async fn handle_protocol_notification(
        &mut self,
        notification: ProtocolNotification<Substream>,
    ) -> Result<(), RpcServerError> {
        match notification.event {
            ProtocolEvent::NewInboundSubstream(node_id, substream) => {
                trace!(
                    target: LOG_TARGET,
                    "New client connection for protocol `{}` from peer `{}`",
                    String::from_utf8_lossy(&notification.protocol),
                    node_id
                );

                let framed = framing::canonical(substream, RPC_MAX_FRAME_SIZE);

                // Over either pending-handshake limit the session is refused with an explicit rejection, which a
                // client pool treats as "use an existing session" rather than a failure to connect
                let pending_for_peer = self.pending_handshakes.get(&node_id).copied().unwrap_or(0);
                if pending_for_peer >= self.config.maximum_pending_handshakes_per_client {
                    trace!(
                        target: LOG_TARGET,
                        "Rejecting RPC substream from peer `{}`: it already has {} handshake(s) in progress",
                        node_id,
                        pending_for_peer
                    );
                    self.reject_at_capacity(&notification.protocol, framed);
                    return Ok(());
                }
                if self.handshakes.len() >= self.config.maximum_pending_handshakes {
                    trace!(
                        target: LOG_TARGET,
                        "Rejecting RPC substream from peer `{}`: {} handshake(s) already in progress",
                        node_id,
                        self.handshakes.len()
                    );
                    self.reject_at_capacity(&notification.protocol, framed);
                    return Ok(());
                }

                let decision = self.decide_session(notification.protocol.clone(), &node_id).await;
                self.spawn_handshake(notification.protocol, node_id, framed, decision);
            },
        }

        Ok(())
    }

    /// Refuses a substream over a pending-handshake limit with an explicit rejection, sent from a short-lived task so
    /// the accept loop never waits on the peer. The replies are bounded like the handshakes themselves: past that,
    /// the substream is dropped without one.
    fn reject_at_capacity(&mut self, protocol: &ProtocolId, mut framed: CanonicalFraming<Substream>) {
        #[cfg(feature = "metrics")]
        metrics::handshake_capacity_rejection_counter(protocol).inc();
        #[cfg(not(feature = "metrics"))]
        let _ = protocol;
        if let Some(count) = self.capacity_warning.occurred() {
            warn!(
                target: LOG_TARGET,
                "Refused {count} RPC substream(s) because too many handshakes were in progress (at most {} per peer, \
                 {} in total)",
                self.config.maximum_pending_handshakes_per_client,
                self.config.maximum_pending_handshakes
            );
        }
        if self.capacity_rejections.len() >= self.config.maximum_pending_handshakes {
            return;
        }
        let timeout = self.config.handshake_timeout;
        self.capacity_rejections.spawn(async move {
            // Best effort: the session is refused either way
            let _result = Handshake::new(&mut framed)
                .with_timeout(timeout)
                .reject_with_reason(HandshakeRejectReason::NoServerSessionsAvailable(
                    "Too many handshakes in progress",
                ))
                .await;
        });
    }

    /// Checks whether a session for `protocol` with `node_id` can be started, before any handshake I/O
    async fn decide_session(&mut self, protocol: ProtocolId, node_id: &NodeId) -> SessionDecision<TSvc::Service> {
        let service = match self.service.make_service(protocol).await {
            Ok(service) => service,
            Err(err) => {
                trace!(
                    target: LOG_TARGET,
                    "Rejecting RPC session request for peer `{}` because {}",
                    node_id,
                    HandshakeRejectReason::ProtocolNotSupported
                );
                return SessionDecision::Reject(HandshakeRejectReason::ProtocolNotSupported, err);
            },
        };

        // The per-peer limit is enforced before the global one so that culling still happens when
        // the server is sitting on its global limit - that is precisely when reclaiming a slot from
        // a peer that is over its own quota matters. Doing the global check first made
        // `cull_oldest_peer_rpc_connection_on_full` unreachable exactly when it was needed.
        //
        // Both limits count the handshakes already accepted (reserved) as well as the running sessions: an accepted
        // handshake must never be refused a session afterwards, because the client would only see the substream close
        // on its first request (`ServerClosedRequest`, which it blames on the server) instead of an explicit rejection.
        let accepted_for_peer = self.accepted_pending.get(node_id).copied().unwrap_or(0);
        if let Err(err) = self.new_session_possible_for(node_id, accepted_for_peer) {
            return SessionDecision::Reject(
                HandshakeRejectReason::NoServerSessionsAvailable("Maximum sessions for client"),
                err,
            );
        }

        if self.executor.num_available() <= self.accepted_pending_total {
            let msg = format!("Used all {} sessions", self.executor.max_available());
            trace!(
                target: LOG_TARGET,
                "Rejecting RPC session request for peer `{}` because {}",
                node_id,
                HandshakeRejectReason::NoServerSessionsAvailable("Cannot spawn more sessions")
            );
            // A session culled above only releases its slot once that task winds down, so this
            // request is still rejected. The client's next attempt is the one that gets in.
            return SessionDecision::Reject(
                HandshakeRejectReason::NoServerSessionsAvailable("Cannot spawn more sessions"),
                RpcServerError::MaximumSessionsReached(msg),
            );
        }

        SessionDecision::Accept(service)
    }

    /// Performs the handshake (or sends the rejection) on its own task, so a slow or silent client cannot stall the
    /// accept loop. The task is bounded by the handshake timeout, and holds a pending-handshake slot until it ends.
    fn spawn_handshake(
        &mut self,
        protocol: ProtocolId,
        node_id: NodeId,
        mut framed: CanonicalFraming<Substream>,
        decision: SessionDecision<TSvc::Service>,
    ) {
        let timeout = self.config.handshake_timeout;
        let task_node_id = node_id.clone();
        let accepted = matches!(decision, SessionDecision::Accept(_));
        let handle = self.handshakes.spawn(async move {
            let result = match decision {
                SessionDecision::Reject(reason, err) => {
                    // Best effort: the session is refused either way
                    let _result = Handshake::new(&mut framed)
                        .with_timeout(timeout)
                        .reject_with_reason(reason)
                        .await;
                    Err(err)
                },
                SessionDecision::Accept(service) => {
                    // Only the client's half of the handshake happens here. The acceptance is sent by the session task
                    // once the session is registered (see `start_session`), so a client never learns it was accepted
                    // before the server counts its session, and an accepted session is never dropped afterwards.
                    match Handshake::new(&mut framed)
                        .with_timeout(timeout)
                        .receive_client_handshake()
                        .await
                    {
                        Ok(version) => {
                            debug!(
                                target: LOG_TARGET,
                                "Server negotiated RPC v{version} with client node `{task_node_id}`"
                            );
                            Ok((service, framed, version))
                        },
                        Err(err) => Err(err.into()),
                    }
                },
            };
            HandshakeOutcome {
                protocol,
                node_id: task_node_id,
                result,
            }
        });
        self.handshake_peers.insert(handle.id(), (node_id.clone(), accepted));
        if accepted {
            let reserved = self.accepted_pending.entry(node_id.clone()).or_insert(0);
            *reserved = reserved.saturating_add(1);
            self.accepted_pending_total = self.accepted_pending_total.saturating_add(1);
        }
        let pending = self.pending_handshakes.entry(node_id).or_insert(0);
        *pending = pending.saturating_add(1);
    }

    /// Releases the pending-handshake slot of a finished handshake task, and starts the session if it succeeded
    fn on_handshake_finished(
        &mut self,
        joined: Result<(tokio::task::Id, HandshakeOutcome<TSvc::Service>), tokio::task::JoinError>,
    ) {
        let task_id = match &joined {
            Ok((id, _)) => *id,
            Err(err) => err.id(),
        };
        if let Some((node_id, accepted)) = self.handshake_peers.remove(&task_id) {
            decrement(&mut self.pending_handshakes, &node_id);
            if accepted {
                decrement(&mut self.accepted_pending, &node_id);
                self.accepted_pending_total = self.accepted_pending_total.saturating_sub(1);
            }
        }

        let outcome = match joined {
            Ok((_, outcome)) => outcome,
            Err(err) => {
                error!(target: LOG_TARGET, "RPC handshake task failed: {err}");
                return;
            },
        };
        let HandshakeOutcome {
            protocol,
            node_id,
            result,
        } = outcome;
        let (service, framed, version) = match result {
            Ok(accepted) => accepted,
            Err(err) => {
                self.log_handshake_failure(&protocol, &err);
                return;
            },
        };
        // The session was reserved when the handshake was accepted, so this cannot fail unless that bookkeeping is
        // wrong. The substream is dropped if it does.
        if let Err(err) = self.start_session(protocol, &node_id, service, framed, version) {
            warn!(
                target: LOG_TARGET,
                "BUG: an accepted RPC handshake from peer `{node_id}` could not start its session: {err}"
            );
        }
    }

    fn log_handshake_failure(&mut self, protocol: &ProtocolId, err: &RpcServerError) {
        #[cfg(not(feature = "metrics"))]
        let _ = protocol;
        if matches!(err, RpcServerError::HandshakeError(rpc::RpcHandshakeError::TimedOut)) {
            #[cfg(feature = "metrics")]
            metrics::handshake_timeout_counter(protocol).inc();
            if let Some(count) = self.timeout_warning.occurred() {
                warn!(
                    target: LOG_TARGET,
                    "{count} RPC handshake(s) timed out after {:.0?} without a handshake from the client",
                    self.config.handshake_timeout
                );
            }
        }
        match err {
            err @ RpcServerError::HandshakeError(_) => {
                trace!(target: LOG_TARGET, "Handshake error: {}", err);
                #[cfg(feature = "metrics")]
                metrics::handshake_error_counter(protocol).inc();
            },
            err => {
                trace!(target: LOG_TARGET, "Unable to spawn RPC service: {}", err);
            },
        }
    }

    /// Checks that `node_id` can have one more session, counting its running sessions and the `reserved` sessions of
    /// its accepted handshakes. In cull mode, culls its oldest running sessions to make room, but only when that is
    /// enough: a request is never refused after sessions were culled for it. Returns the peer's running sessions.
    fn new_session_possible_for(&mut self, node_id: &NodeId, reserved: usize) -> Result<usize, RpcServerError> {
        let max = match self.config.maximum_sessions_per_client {
            Some(max) if max > 0 => max,
            Some(_) | None => return Ok(self.sessions.get(node_id).map_or(0, Vec::len)),
        };
        let running = self.sessions.get(node_id).map_or(0, Vec::len);
        let in_use = running.saturating_add(reserved);
        if in_use < max {
            return Ok(running);
        }
        // Reservations cannot be culled, so culling only helps if they leave room for this request
        if self.config.cull_oldest_peer_rpc_connection_on_full && reserved < max {
            let num_to_remove = in_use.saturating_sub(max).saturating_add(1);
            if let Some(session_info) = self.sessions.get_mut(node_id) {
                for _ in 0..num_to_remove {
                    let info = session_info.remove(0);
                    info!(target: LOG_TARGET, "Culling oldest RPC session for peer `{node_id}`");
                    let _ = info.peer_watch.send(());
                }
                let running = session_info.len();
                if session_info.is_empty() {
                    self.sessions.remove(node_id);
                }
                return Ok(running);
            }
        }
        trace!(
            target: LOG_TARGET,
            "Maximum RPC sessions for peer {node_id} met or exceeded. Max: {max}, running: {running}, accepted \
             handshakes: {reserved}"
        );
        if let Some(count) = self.session_limit_warning.occurred() {
            warn!(
                target: LOG_TARGET,
                "Refused {count} RPC session(s) because the peer was at its limit of {max} session(s)"
            );
        }
        Err(RpcServerError::MaxSessionsPerClientReached {
            node_id: node_id.clone(),
            max_sessions: max,
        })
    }

    fn close_all_sessions(&mut self, node_id: &NodeId) -> usize {
        let mut count = 0usize;
        if let Some(session_info) = self.sessions.get_mut(node_id) {
            for info in session_info.iter_mut() {
                count = count.saturating_add(1);
                info!(
                    target: LOG_TARGET,
                    "Closing RPC session {} (stream id {}) for peer `{}`", info.session_id, info.stream_id, node_id
                );
                let _ = info.peer_watch.send(());
            }
            self.sessions.remove(node_id);
        }
        count
    }

    fn on_session_complete(&mut self, node_id: &NodeId, session_id: u64) {
        if let Some(session_info) = self.sessions.get_mut(node_id) {
            if let Some(info) = session_info.iter_mut().find(|info| info.session_id == session_id) {
                info!(
                    target: LOG_TARGET,
                    "Session {session_id} complete for {node_id} (stream id {})", info.stream_id
                );
                let _ = info.peer_watch.send(());
            };
            session_info.retain(|info| info.session_id != session_id);
            if session_info.is_empty() {
                self.sessions.remove(node_id);
            }
        }
    }

    /// Registers and starts a session on a substream whose client handshake was received with `version`. Its session
    /// was reserved when the handshake was accepted; the limits are checked again only as a defensive invariant (if
    /// that fails, the substream is dropped before the client was told it was accepted).
    ///
    /// The session task sends the acceptance as its first action. The session is registered here, on the accept loop,
    /// before that task is spawned, so by the time the client is told it was accepted the session is already counted.
    /// If the acceptance cannot be sent within the handshake timeout, the session ends at once and releases its slot.
    fn start_session(
        &mut self,
        protocol: ProtocolId,
        node_id: &NodeId,
        service: TSvc::Service,
        mut framed: CanonicalFraming<Substream>,
        version: u32,
    ) -> Result<(), RpcServerError> {
        // Its reservation was released when the handshake finished
        let num_sessions = self.new_session_possible_for(node_id, 0)?;
        if !self.executor.can_spawn() {
            return Err(RpcServerError::MaximumSessionsReached(format!(
                "Used all {} sessions",
                self.executor.max_available()
            )));
        }

        info!(
            target: LOG_TARGET,
            "NEW SESSION for {node_id} ({num_sessions} currently active) "
        );

        let stream_id = framed.stream_id();
        let session_id = self.next_session_id;
        self.next_session_id = self.next_session_id.wrapping_add(1);
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(());
        let config = self.config.clone();
        let comms_provider = self.comms_provider.clone();
        let session_node_id = node_id.clone();

        let node_id_clone = node_id.clone();
        let handle = self
            .executor
            .try_spawn(async move {
                // The session is registered (and can be culled or closed) from here on, so a stop ends it even while
                // the acceptance is waiting on a peer that does not read. The substream is dropped
                // without a close. The clone has its own record of what it has seen, so a stop after
                // the acceptance still reaches the session below; a stop sent before this task first
                // runs is seen here (as is a dropped sender).
                let mut accept_stop_rx = stop_rx.clone();
                let mut handshake = Handshake::new(&mut framed).with_timeout(config.handshake_timeout);
                let accepted = tokio::select! {
                    biased;
                    _ = accept_stop_rx.changed() => {
                        debug!(
                            target: LOG_TARGET,
                            "RPC session for `{node_id_clone}` was stopped before its acceptance was sent"
                        );
                        return (node_id_clone, session_id);
                    },
                    result = handshake.accept_version(version) => result,
                };
                if let Err(err) = accepted {
                    debug!(
                        target: LOG_TARGET,
                        "Could not send the RPC handshake acceptance to `{node_id_clone}`; ending the session: {err}"
                    );
                    return (node_id_clone, session_id);
                }
                let service = ActivePeerRpcService::new(
                    config,
                    protocol,
                    session_node_id,
                    service,
                    framed,
                    comms_provider,
                    stop_rx,
                );
                // Counted until the session ends, however it ends (a panic included)
                #[cfg(feature = "metrics")]
                let _num_sessions = metrics::ActiveSession::new(&service.protocol);
                service.start().await;
                info!(target: LOG_TARGET, "END OF SESSION for {node_id_clone} ");

                (node_id_clone, session_id)
            })
            .map_err(|e| RpcServerError::MaximumSessionsReached(format!("{e:?}")))?;

        let task_node_id = node_id.clone();
        self.tasks.push(Box::pin(async move {
            if let Err(err) = handle.await {
                error!(target: LOG_TARGET, "RPC session {session_id} for peer `{task_node_id}` failed: {err}");
            }
            (task_node_id, session_id)
        }));
        let mut peer_stop = vec![SessionInfo {
            peer_watch: stop_tx,
            session_id,
            stream_id,
        }];
        self.sessions
            .entry(node_id.clone())
            .and_modify(|entry| entry.append(&mut peer_stop))
            .or_insert(peer_stop);
        if let Some(info) = self.sessions.get(node_id) {
            info!(
                target: LOG_TARGET,
                "NEW SESSION {session_id} created for {node_id} (stream id {stream_id}, {} active) ", info.len()
            );
        }

        Ok(())
    }
}

/// Decrements `node_id`'s count, removing it at zero
fn decrement(counts: &mut HashMap<NodeId, usize>, node_id: &NodeId) {
    if let Some(count) = counts.get_mut(node_id) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            counts.remove(node_id);
        }
    }
}

/// A session's substream, framed for reading requests (see [RequestCodec]) and writing responses
type SessionFraming = EarlyClose<Framed<Substream, RequestCodec>>;

struct ActivePeerRpcService<TSvc, TCommsProvider> {
    config: RpcServerBuilder,
    protocol: ProtocolId,
    node_id: NodeId,
    service: TSvc,
    framed: SessionFraming,
    /// A request that arrived while the previous one was being served, to be handled next
    buffered_request: Option<RequestFrame>,
    comms_provider: TCommsProvider,
    logging_context_string: Arc<String>,
    /// Changes (or closes) when the session must stop: it was culled, closed for its peer, or the server is gone
    stop_rx: tokio::sync::watch::Receiver<()>,
    /// Looked up once per session rather than for every streamed message.
    #[cfg(feature = "metrics")]
    outbound_response_bytes: tari_metrics::Histogram,
}

impl<TSvc, TCommsProvider> ActivePeerRpcService<TSvc, TCommsProvider>
where
    TSvc: Service<Request<Bytes>, Response = Response<Body>, Error = RpcStatus>,
    TCommsProvider: RpcCommsProvider + Send + Clone + 'static,
{
    pub(self) fn new(
        config: RpcServerBuilder,
        protocol: ProtocolId,
        node_id: NodeId,
        service: TSvc,
        framed: CanonicalFraming<Substream>,
        comms_provider: TCommsProvider,
        stop_rx: tokio::sync::watch::Receiver<()>,
    ) -> Self {
        Self {
            logging_context_string: Arc::new(format!(
                "stream_id: {}, peer: {}, protocol: {}, stream_id: {}",
                framed.stream_id(),
                node_id,
                framed.stream_id(),
                String::from_utf8_lossy(&protocol)
            )),

            #[cfg(feature = "metrics")]
            outbound_response_bytes: metrics::outbound_response_bytes(&protocol),
            config,
            protocol,
            node_id,
            service,
            framed: EarlyClose::new(into_session_framing(framed)),
            buffered_request: None,
            comms_provider,
            stop_rx,
        }
    }

    async fn start(mut self) {
        debug!(
            target: LOG_TARGET,
            "({}) Rpc server started.", self.logging_context_string,
        );
        if let Err(err) = self.run().await {
            #[cfg(feature = "metrics")]
            metrics::error_counter(&self.protocol, &err).inc();
            let level = match &err {
                RpcServerError::Io(e) => err_to_log_level(e),
                RpcServerError::EarlyClose(e) => e.io().map(err_to_log_level).unwrap_or(log::Level::Error),
                _ => log::Level::Error,
            };
            log!(
                target: LOG_TARGET,
                level,
                "({}) Rpc server exited with an error: {}",
                self.logging_context_string,
                err
            );
        }
    }

    /// Write one frame to the peer, giving up after `deadline`.
    ///
    /// Every write here goes into a yamux window the peer controls, so a peer that simply stops
    /// reading can park any unbounded `send`. Route all of them through this so the bound is
    /// structural rather than something to remember at each call site.
    ///
    /// Also gives up as soon as the session is told to stop, with `SessionStopped`.
    async fn send_with_deadline(&mut self, msg: Bytes, deadline: Duration) -> Result<(), RpcServerError> {
        let mut stop_rx = self.stop_rx.clone();
        let result = tokio::select! {
            biased;
            _ = stop_rx.changed() => return Err(RpcServerError::SessionStopped),
            result = time::timeout(deadline, self.framed.send(msg)) => result,
        };
        match result {
            Ok(result) => result.map_err(Into::into),
            Err(_elapsed) => {
                debug!(
                    target: LOG_TARGET,
                    "({}) Peer did not accept a response within the deadline ({:.0?}). Aborting the session.",
                    self.logging_context_string,
                    deadline
                );

                #[cfg(feature = "metrics")]
                metrics::error_counter(&self.protocol, &RpcServerError::WriteStreamExceededDeadline).inc();
                Err(RpcServerError::WriteStreamExceededDeadline)
            },
        }
    }

    /// Shut the substream down, giving up after `timeout` (normally [`SESSION_CLOSE_TIMEOUT`]) and letting it drop.
    ///
    /// `EarlyClose::poll_close` falls through to `check_for_early_close`, which returns `Pending`
    /// for a peer that is merely silent - so closing a substream whose window the peer has filled
    /// and stopped draining never completes. That is worse than one stuck task: the session holds a
    /// `BoundedExecutor` permit until `start()` returns, and nothing can reclaim it. The idle timer
    /// is not running (we are not in the select), and culling only signals `stop_rx`, which this
    /// task is no longer watching. One peer could walk the global session limit down to zero and
    /// lock every other peer out of the node.
    ///
    /// A session that has been told to stop (before or during the close) never spends longer than
    /// [`STOPPED_SESSION_CLOSE_TIMEOUT`] on it, so it releases its slot within a second on every path.
    async fn close_framed(&mut self, timeout: Duration) -> Result<(), RpcServerError> {
        let timeout = if self.stop_requested() {
            cmp::min(timeout, STOPPED_SESSION_CLOSE_TIMEOUT)
        } else {
            timeout
        };
        let mut stop_rx = self.stop_rx.clone();
        let result = tokio::select! {
            biased;
            result = time::timeout(timeout, self.framed.close()) => result,
            _ = stop_rx.changed(), if timeout > STOPPED_SESSION_CLOSE_TIMEOUT => {
                debug!(
                    target: LOG_TARGET,
                    "({}) Stop signal received while closing the substream. Dropping it.",
                    self.logging_context_string
                );
                return Ok(());
            },
        };
        match result {
            Ok(result) => result.map_err(Into::into),
            Err(_elapsed) => {
                debug!(
                    target: LOG_TARGET,
                    "({}) Peer did not accept the substream close within {:.0?}. Dropping it.",
                    self.logging_context_string,
                    timeout
                );
                Ok(())
            },
        }
    }

    /// Whether the session has been told to stop. The server drops the sender right after signalling (or when it shuts
    /// down), so a closed channel counts as a stop too; that also covers a signal this receiver has already seen.
    fn stop_requested(&self) -> bool {
        self.stop_rx.has_changed().unwrap_or(true)
    }

    #[allow(clippy::too_many_lines)]
    async fn run(&mut self) -> Result<(), RpcServerError> {
        // Only the time spent waiting for the next request is bounded - handling a request holds
        // this loop and cannot trip the timer. Without this, a session (and the slot it occupies in
        // the global session limit) is held for as long as the substream stays open, which for a
        // peer that has gone away without closing it is forever.
        let idle_timeout = self.config.idle_session_timeout;
        let idle_sleep = time::sleep(idle_timeout.unwrap_or(DEFAULT_IDLE_SESSION_TIMEOUT));
        tokio::pin!(idle_sleep);

        loop {
            let next = match self.buffered_request.take() {
                // A request that arrived during the last service call: one that was not read yet is read now
                Some(RequestFrame::Unread) | None => tokio::select! {
                    _ = self.stop_rx.changed() => {
                        debug!(target: LOG_TARGET, "({}) Stop signal received, closing substream.", self.logging_context_string);
                        self.close_framed(STOPPED_SESSION_CLOSE_TIMEOUT).await?;
                        return Ok(());
                    }
                    () = &mut idle_sleep, if idle_timeout.is_some() => {
                        debug!(
                            target: LOG_TARGET,
                            "({}) Closing session idle for {:.0?}.",
                            self.logging_context_string,
                            idle_timeout.unwrap_or_default()
                        );
                        break;
                    }
                    result = self.framed.next() => result,
                },
                Some(frame) => Some(Ok(frame)),
            };

            let served = match next {
                Some(Ok(RequestFrame::Request(frame))) => {
                    #[cfg(feature = "metrics")]
                    metrics::inbound_requests_bytes(&self.protocol).observe(frame.len() as f64);

                    let start = Instant::now();
                    let was_large = frame.len() > MAX_IDLE_READ_BUFFER_CAPACITY;
                    let result = self.handle_request(frame.freeze()).await;
                    // The request (and every copy of it) is gone by now
                    release_large_read_buffer(self.framed.get_mut().read_buffer_mut(), was_large);
                    let elapsed = start.elapsed();
                    trace!(
                        target: LOG_TARGET,
                        "({}) RPC request completed in {:.0?}{}",
                        self.logging_context_string,
                        elapsed,
                        if elapsed.as_secs() > 5 { " (LONG REQUEST)" } else { "" }
                    );
                    result
                },
                Some(Ok(RequestFrame::Oversized { request_id, len })) => {
                    self.reject_oversized_request(request_id, len).await.map(|()| false)
                },
                // Only produced in peek mode, which is only on during a service call
                Some(Ok(RequestFrame::Unread)) => Err(RpcServerError::ProtocolError(
                    "BUG: a frame was left unread outside a service call".to_string(),
                )),
                Some(Err(err)) => {
                    if let Err(err) = self.close_framed(SESSION_CLOSE_TIMEOUT).await {
                        error!(
                            target: LOG_TARGET,
                            "({}) Failed to close substream after socket error: {}", self.logging_context_string, err
                        );
                    }
                    return Err(err.into());
                },
                None => break,
            };

            match served {
                // Only a request that reached the service counts as activity. An ACK (a liveness ping), a FIN or a
                // rejected request does not: otherwise a peer could hold its session slots forever with pings alone.
                Ok(true) => {
                    if let Some(timeout) = idle_timeout {
                        idle_sleep.as_mut().reset(
                            time::Instant::now()
                                .checked_add(timeout)
                                .unwrap_or_else(time::Instant::now),
                        );
                    }
                },
                Ok(false) => {},
                Err(RpcServerError::SessionStopped) => {
                    debug!(
                        target: LOG_TARGET,
                        "({}) Stop signal received while handling a request, closing substream.",
                        self.logging_context_string
                    );
                    self.close_framed(STOPPED_SESSION_CLOSE_TIMEOUT).await?;
                    return Ok(());
                },
                Err(RpcServerError::StreamClosedByRemote) => {
                    debug!(
                        target: LOG_TARGET,
                        "({}) Client closed the substream while its request was being handled.",
                        self.logging_context_string
                    );
                    // The client is gone, so this close has nobody to wait for
                    self.close_framed(STOPPED_SESSION_CLOSE_TIMEOUT).await?;
                    return Ok(());
                },
                Err(err) => {
                    // A write deadline means the peer stopped reading, so the window is
                    // already full and a graceful close cannot land - skip straight to
                    // dropping the substream rather than burning the close timeout.
                    let close_result = if matches!(err, RpcServerError::WriteStreamExceededDeadline) {
                        Ok(())
                    } else {
                        self.close_framed(SESSION_CLOSE_TIMEOUT).await
                    };
                    if let Err(err) = close_result {
                        let level = err.early_close_io().map(err_to_log_level).unwrap_or(log::Level::Error);

                        log!(
                            target: LOG_TARGET,
                            level,
                            "({}) Failed to close substream after socket error: {}",
                            self.logging_context_string,
                            err,
                        );
                    }
                    let level = match &err {
                        RpcServerError::Io(e) => err_to_log_level(e),
                        err => err.early_close_io().map(err_to_log_level).unwrap_or(log::Level::Error),
                    };
                    log!(
                        target: LOG_TARGET,
                        level,
                        "(peer: {}, protocol: {}) Failed to handle request: {}",
                        self.node_id,
                        self.protocol_name(),
                        err
                    );
                    return Err(err);
                },
            }
        }

        self.close_framed(SESSION_CLOSE_TIMEOUT).await?;
        Ok(())
    }

    /// Replies `BadRequest` to a request over [rpc::RPC_MAX_REQUEST_SIZE], which was not read. No ban: a peer running a
    /// version with a larger request cap may send such a request honestly.
    async fn reject_oversized_request(&mut self, request_id: u32, len: usize) -> Result<(), RpcServerError> {
        debug!(
            target: LOG_TARGET,
            "({}) Rejecting a {} byte request, larger than the maximum of {} bytes",
            self.logging_context_string,
            len,
            rpc::max_request_size()
        );
        #[cfg(feature = "metrics")]
        metrics::status_error_counter(&self.protocol, super::RpcStatusCode::BadRequest).inc();
        let rejection = oversized_request_response(request_id, len);
        self.send_with_deadline(rejection.to_encoded_bytes().into(), SESSION_CLOSE_TIMEOUT)
            .await
    }

    /// Handles one request. Returns whether it reached the service, which is what counts as activity for the idle
    /// timer.
    #[allow(clippy::too_many_lines)]
    #[instrument(name = "rpc::server::handle_req", level="trace", skip(self, request), err, fields(request_size = request.len ()))]
    async fn handle_request(&mut self, mut request: Bytes) -> Result<bool, RpcServerError> {
        let decoded_msg = proto::rpc::RpcRequest::decode(&mut request)?;
        // The payload was copied out, so release the frame now rather than hold it through the service call
        drop(request);

        let request_id = decoded_msg.request_id;
        let method = RpcMethod::from(decoded_msg.method);
        let requested_deadline = Duration::from_secs(decoded_msg.deadline);

        // The client side deadline MUST be greater or equal to the minimum_client_deadline
        if requested_deadline < self.config.minimum_client_deadline {
            debug!(
                target: LOG_TARGET,
                "({}) Client has an invalid deadline. {}", self.logging_context_string, decoded_msg
            );
            // Let the client know that they have disobeyed the spec
            let status = RpcStatus::bad_request(&format!(
                "Invalid deadline ({:.0?}). The deadline MUST be greater than {:.0?}.",
                requested_deadline, self.config.minimum_client_deadline,
            ));
            let bad_request = proto::rpc::RpcResponse {
                request_id,
                status: status.as_code(),
                flags: RpcMessageFlags::FIN.bits().into(),
                payload: status.to_details_bytes(),
            };
            #[cfg(feature = "metrics")]
            metrics::status_error_counter(&self.protocol, status.as_status_code()).inc();
            // The client's own deadline was rejected as invalid, so there is nothing to honour
            // here - bound this on the teardown timeout instead.
            self.send_with_deadline(bad_request.to_encoded_bytes().into(), SESSION_CLOSE_TIMEOUT)
                .await?;
            return Ok(false);
        }

        // ...and it is capped from above, because the deadline governs both the service call and
        // each message of a streaming response. An uncapped `u64` of seconds lets a peer disable
        // those timeouts outright and hold a session slot for as long as it likes. Clamp instead of
        // rejecting so a peer configured above our ceiling still works.
        let deadline = self.config.clamp_client_deadline(requested_deadline);
        if deadline < requested_deadline {
            debug!(
                target: LOG_TARGET,
                "({}) Client requested a deadline of {:.0?}, capping it at {:.0?}.",
                self.logging_context_string,
                requested_deadline,
                deadline
            );
        }

        let msg_flags = RpcMessageFlags::from_bits(u8::try_from(decoded_msg.flags).map_err(|_| {
            RpcServerError::ProtocolError(format!("invalid message flag: must be less than {}", u8::MAX))
        })?)
        .ok_or(RpcServerError::ProtocolError(format!(
            "invalid message flag, does not match any flags ({})",
            decoded_msg.flags
        )))?;

        if msg_flags.contains(RpcMessageFlags::FIN) {
            debug!(target: LOG_TARGET, "({}) Client sent FIN.", self.logging_context_string);
            return Ok(false);
        }
        if msg_flags.contains(RpcMessageFlags::ACK) {
            debug!(
                target: LOG_TARGET,
                "({}) sending ACK response.", self.logging_context_string
            );
            let ack = proto::rpc::RpcResponse {
                request_id,
                status: RpcStatus::ok().as_code(),
                flags: RpcMessageFlags::ACK.bits().into(),
                ..Default::default()
            };
            self.send_with_deadline(ack.to_encoded_bytes().into(), deadline).await?;
            return Ok(false);
        }

        trace!(
            target: LOG_TARGET,
            "({}) Request: {}, Method: {}",
            self.logging_context_string,
            decoded_msg,
            method.id()
        );

        let req = Request::with_context(
            self.create_request_context(request_id),
            method,
            decoded_msg.payload.into(),
        );

        // A clone, so that the call (which lives on while this function sends the response) does not borrow `self`
        let logging_context = self.logging_context_string.clone();
        let service_call = log_timing(&logging_context, request_id, "service call", self.service.call(req));
        // The call is dropped (cancelling it) only if the session is told to stop. If the client gives up on the
        // request (a FIN) or hangs up, the call is still driven to the end (or its deadline) and its result discarded:
        // dropping it would not stop work it handed to other tasks (e.g. blocking database calls), so a client
        // repeating request-then-FIN could otherwise start any number of them at once. This way a session never has
        // more than one call running.
        //
        // Meanwhile only frames small enough to be a FIN are read: a longer one (a pipelined request) is left on the
        // substream until this request is done, so the client's flow control, not the session, holds it.
        let mut stop_rx = self.stop_rx.clone();
        let call = time::timeout(deadline, service_call);
        tokio::pin!(call);
        self.framed
            .get_mut()
            .codec_mut()
            .set_peek_limit(Some(MAX_FIN_REQUEST_SIZE));
        let selected = tokio::select! {
            biased;
            _ = stop_rx.changed() => Err(RpcServerError::SessionStopped),
            reason = wait_for_client_to_give_up(&mut self.framed, &mut self.buffered_request) => Err(reason),
            result = &mut call => Ok(result),
        };
        self.framed.get_mut().codec_mut().set_peek_limit(None);
        let service_result = match selected {
            Ok(result) => result,
            Err(RpcServerError::SessionStopped) => return Err(RpcServerError::SessionStopped),
            Err(reason) => {
                debug!(
                    target: LOG_TARGET,
                    "({}) Client gave up on its request while it was being handled ({}). Finishing the call without \
                     replying.",
                    self.logging_context_string,
                    reason
                );
                // The substream is not read meanwhile
                tokio::select! {
                    biased;
                    _ = stop_rx.changed() => return Err(RpcServerError::SessionStopped),
                    _ = &mut call => {},
                }
                return match reason {
                    // The session carries on with the next request. An abandoned call is not activity.
                    RpcServerError::ClientInterruptedStream => Ok(false),
                    // The client hung up: the session ends
                    reason => Err(reason),
                };
            },
        };
        let service_result = match service_result {
            Ok(v) => v,
            Err(_) => {
                warn!(
                    target: LOG_TARGET,
                    "{} RPC service was not able to complete within the deadline ({:.0?}). Request aborted",
                    self.logging_context_string,
                    deadline,
                );

                #[cfg(feature = "metrics")]
                metrics::error_counter(&self.protocol, &RpcServerError::ServiceCallExceededDeadline).inc();

                // Tell the client, rather than going silent and leaving it to unwind on its own
                // timeout. This matters whenever the deadline we applied is shorter than the one the
                // client asked for - a clamped request would otherwise look identical to a hung
                // server for the remainder of the client's own (longer) deadline.
                let status = RpcStatus::timed_out("RPC service did not complete within the deadline");
                let timed_out = proto::rpc::RpcResponse {
                    request_id,
                    status: status.as_code(),
                    flags: RpcMessageFlags::FIN.bits().into(),
                    payload: status.to_details_bytes(),
                };
                #[cfg(feature = "metrics")]
                metrics::status_error_counter(&self.protocol, status.as_status_code()).inc();
                self.send_with_deadline(timed_out.to_encoded_bytes().into(), deadline)
                    .await?;
                return Ok(true);
            },
        };

        match service_result {
            Ok(body) => {
                self.process_body(request_id, deadline, body).await?;
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "{} Service returned an error: {}", self.logging_context_string, err
                );
                let resp = proto::rpc::RpcResponse {
                    request_id,
                    status: err.as_code(),
                    flags: RpcMessageFlags::FIN.bits().into(),
                    payload: err.to_details_bytes(),
                };

                #[cfg(feature = "metrics")]
                metrics::status_error_counter(&self.protocol, err.as_status_code()).inc();
                self.send_with_deadline(resp.to_encoded_bytes().into(), deadline)
                    .await?;
            },
        }

        Ok(true)
    }

    fn protocol_name(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.protocol)
    }

    #[allow(clippy::too_many_lines)]
    async fn process_body(
        &mut self,
        request_id: u32,
        deadline: Duration,
        body: Response<Body>,
    ) -> Result<(), RpcServerError> {
        trace!(target: LOG_TARGET, "Service call succeeded");

        #[cfg(feature = "metrics")]
        let protocol = self.protocol.clone();
        let mut stream = body
            .into_message()
            .map(|result| into_response(request_id, result))
            .map(move |mut message| {
                if message.payload.len() > rpc::max_response_payload_size() {
                    message = message.exceeded_message_size();
                }
                #[cfg(feature = "metrics")]
                if !message.status.is_ok() {
                    metrics::status_error_counter(&protocol, message.status).inc();
                }
                message.to_encoded_bytes()
            });

        let logging_context = self.logging_context_string.clone();
        let mut stop_rx = self.stop_rx.clone();
        // The next item, pulled from the stream and encoded while the previous one was being sent. `Some(None)` means
        // the stream ended during that send.
        let mut prefetched: Option<Option<Bytes>> = None;
        loop {
            let next = match prefetched.take() {
                Some(Some(msg)) => {
                    // Give an interruption that arrived during the previous send the same chance it has in the
                    // select below. The prefetched item is dropped unsent.
                    if let Err(err) = self.check_interruptions().await {
                        match err {
                            err @ RpcServerError::ClientInterruptedStream => {
                                debug!(target: LOG_TARGET, "Stream was interrupted by client: {}", err);
                                break;
                            },
                            err => {
                                error!(target: LOG_TARGET, "Stream was interrupted: {}", err);
                                return Err(err);
                            },
                        }
                    }
                    Some(msg)
                },
                Some(None) => None,
                None => {
                    let next_item = log_timing(&logging_context, request_id, "message read", stream.next());
                    let timeout = time::sleep(deadline);

                    tokio::select! {
                        // The session was told to stop: drop the stream (and whatever it is waiting on)
                        _ = stop_rx.changed() => return Err(RpcServerError::SessionStopped),
                        // Check if the client interrupted the outgoing stream
                        Err(err) = self.check_interruptions() => {
                            match err {
                                err @ RpcServerError::ClientInterruptedStream => {
                                    debug!(target: LOG_TARGET, "Stream was interrupted by client: {}", err);
                                    break;
                                },
                                err => {
                                    error!(target: LOG_TARGET, "Stream was interrupted: {}", err);
                                    return Err(err);
                                },
                            }
                        },
                        msg = next_item => msg,

                        _ = timeout => {
                             debug!(
                                target: LOG_TARGET,
                                "({}) Failed to return result within client deadline ({:.0?})",
                                self.logging_context_string,
                                deadline
                            );

                            #[cfg(feature = "metrics")]
                            metrics::error_counter(
                                &self.protocol,
                                &RpcServerError::ReadStreamExceededDeadline,
                            )
                            .inc();
                            break;
                        }
                    } // end select!
                },
            };

            let Some(msg) = next else {
                trace!(target: LOG_TARGET, "{} Request complete", self.logging_context_string,);
                break;
            };

            #[cfg(feature = "metrics")]
            self.outbound_response_bytes.observe(msg.len() as f64);
            trace!(
                target: LOG_TARGET,
                "({}) Sending body len = {}",
                self.logging_context_string,
                msg.len()
            );

            // Bounded because this sits outside the `timeout` branch above: it is the yamux window that backs up
            // here, so a peer that opens a stream and then stops draining it parks this task - and the session slot
            // it holds - with no timer running anywhere. The outer `run()` loop cannot rescue it either; its idle
            // timer does not tick while a request is being handled. On elapse the peer is mid-frame, so the
            // substream can no longer be trusted for a subsequent request and the session ends.
            let send = self.send_with_deadline(msg, deadline);
            tokio::pin!(send);
            // While the send is in flight, pull and encode the next item. At most one item is held, so a session whose
            // peer stops reading holds one extra encoded item, for no longer than the send deadline. If the send
            // finishes first, the pending pull is dropped (a stream loses nothing when `next()` is cancelled) and the
            // top of the loop waits for it as before, under a fresh per-item deadline.
            loop {
                tokio::select! {
                    biased;
                    result = &mut send => {
                        result?;
                        break;
                    },
                    next = stream.next(), if prefetched.is_none() => {
                        prefetched = Some(next);
                    },
                }
            }
        } // end loop
        Ok(())
    }

    async fn check_interruptions(&mut self) -> Result<(), RpcServerError> {
        // A request that arrived during the service call is handled after this one, so this does not read past it.
        // The substream can still be read while a send waits for the peer: `EarlyClose` checks for a message from the
        // client then, and one ends the session (`UnexpectedMessage`), as it always has for a client that sends while
        // it is being answered.
        if self.buffered_request.is_some() {
            return Ok(());
        }
        let check = future::poll_fn(|cx| match Pin::new(&mut self.framed).poll_next(cx) {
            Poll::Ready(Some(Ok(RequestFrame::Oversized { len, .. }))) => {
                // An oversized message is never read; a client may only send a FIN here, which is tiny
                debug!(
                    target: LOG_TARGET,
                    "Ignoring a {} byte message received during a streaming response (maximum {} bytes)",
                    len,
                    rpc::max_request_size()
                );
                Poll::Ready(None)
            },
            // Only produced in peek mode, which is off here
            Poll::Ready(Some(Ok(RequestFrame::Unread))) => Poll::Ready(None),
            Poll::Ready(Some(Ok(RequestFrame::Request(mut msg)))) => {
                let decoded_msg = match proto::rpc::RpcRequest::decode(&mut msg) {
                    Ok(msg) => msg,
                    Err(err) => {
                        error!(target: LOG_TARGET, "Client send MALFORMED response: {}", err);
                        return Poll::Ready(Some(RpcServerError::UnexpectedIncomingMessageMalformed));
                    },
                };
                let u8_bits = match u8::try_from(decoded_msg.flags) {
                    Ok(bits) => bits,
                    Err(err) => {
                        error!(target: LOG_TARGET, "Client send MALFORMED flags: {}", err);
                        return Poll::Ready(Some(RpcServerError::ProtocolError(format!(
                            "invalid message flag: must be less than {}",
                            u8::MAX
                        ))));
                    },
                };

                let msg_flags = match RpcMessageFlags::from_bits(u8_bits) {
                    Some(flags) => flags,
                    None => {
                        error!(target: LOG_TARGET, "Client send MALFORMED flags: {}", u8_bits);
                        return Poll::Ready(Some(RpcServerError::ProtocolError(format!(
                            "invalid message flag, does not match any flags ({u8_bits})"
                        ))));
                    },
                };
                if msg_flags.is_fin() {
                    Poll::Ready(Some(RpcServerError::ClientInterruptedStream))
                } else {
                    Poll::Ready(Some(RpcServerError::UnexpectedIncomingMessage(decoded_msg)))
                }
            },
            Poll::Ready(Some(Err(err))) if err.kind() == io::ErrorKind::WouldBlock => Poll::Ready(None),
            Poll::Ready(Some(Err(err))) => Poll::Ready(Some(RpcServerError::from(err))),
            Poll::Ready(None) => Poll::Ready(Some(RpcServerError::StreamClosedByRemote)),
            Poll::Pending => Poll::Ready(None),
        })
        .await;
        match check {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    fn create_request_context(&self, request_id: u32) -> RequestContext {
        RequestContext::new(request_id, self.node_id.clone(), Box::new(self.comms_provider.clone()))
    }
}

fn is_trace_enabled() -> bool {
    log_enabled!(target: LOG_TARGET, log::Level::Trace) || tracing::enabled!(target: LOG_TARGET, Level::TRACE)
}

async fn log_timing<R, F: Future<Output = R>>(context_str: &str, request_id: u32, tag: &str, fut: F) -> R {
    // Called for every streamed message, so only pay for the span and the timing when they will be logged
    if !is_trace_enabled() {
        return fut.await;
    }
    let t = Instant::now();
    let span = span!(Level::TRACE, "rpc::internal::timing", request_id, tag);
    let ret = fut.instrument(span).await;
    let elapsed = t.elapsed();
    trace!(
        target: LOG_TARGET,
        "({}) RPC TIMING(REQ_ID={}): '{}' took {:.2}s{}",
        context_str,
        request_id,
        tag,
        elapsed.as_secs_f32(),
        if elapsed.as_secs() >= 5 { " (SLOW)" } else { "" }
    );
    ret
}

/// The flags of an `RpcRequest`. Decoding this skips over the other fields, including the payload, without copying
/// them.
#[derive(Clone, PartialEq, prost::Message)]
struct RpcRequestFlags {
    #[prost(uint32, tag = "3")]
    flags: u32,
}

/// Whether `frame` is a request with the FIN flag set, with which a client abandons its request. Anything that does not
/// decode as one is not.
fn is_fin(frame: &BytesMut) -> bool {
    match RpcRequestFlags::decode(frame.as_ref()) {
        Ok(request) => u8::try_from(request.flags)
            .ok()
            .and_then(RpcMessageFlags::from_bits)
            .is_some_and(|flags| flags.is_fin()),
        Err(_) => false,
    }
}

/// Waits, while a request's service call runs, for the client to give up on it, and returns why: it hung up
/// (`StreamClosedByRemote`, or the read error), or it sent a FIN (`ClientInterruptedStream`). The call is not cancelled
/// for either (see `ActivePeerRpcService::handle_request`), only its result discarded.
///
/// Any other frame is a request pipelined behind this one. A small one is kept in `buffered`; a larger one is not read
/// at all (the codec is in peek mode, see `RequestCodec::set_peek_limit`), only noted in `buffered` as
/// [RequestFrame::Unread]. Either way the session handles it next, and this function does not read again, so nothing
/// is lost or reordered; from then on only the service call's own deadline (and the stop signal) can end the call
/// early. The session may still read the substream before then while it waits to send a response: see
/// `ActivePeerRpcService::check_interruptions`.
async fn wait_for_client_to_give_up(
    framed: &mut SessionFraming,
    buffered: &mut Option<RequestFrame>,
) -> RpcServerError {
    if buffered.is_none() {
        match framed.next().await {
            None => return RpcServerError::StreamClosedByRemote,
            Some(Err(err)) => return err.into(),
            Some(Ok(RequestFrame::Request(frame))) if is_fin(&frame) => return RpcServerError::ClientInterruptedStream,
            Some(Ok(frame)) => *buffered = Some(frame),
        }
    }
    future::pending().await
}

/// A read buffer is given a fresh allocation once it has more than this capacity
const MAX_IDLE_READ_BUFFER_CAPACITY: usize = 64 * 1024;

/// The capacity of a fresh read buffer, as `Framed` starts with
const READ_BUFFER_INITIAL_CAPACITY: usize = 8 * 1024;

/// A read buffer that grew for a large request keeps that allocation for the rest of the session: each frame is split
/// off the front of it and the rest reuses the same allocation. Once such a request has been handled
/// (`after_large_request`), this moves what is left (the start of the next request, if any) into a fresh, small
/// buffer, so an idle session does not pin up to the request limit.
///
/// The decision is taken from the request's size, not from `buffer.capacity()`: after a frame is split off, that is
/// only the free space behind the frame, which is small when the frame filled its allocation, while the rest of the
/// buffer still shares (and would reclaim, on the next read) the whole allocation.
fn release_large_read_buffer(buffer: &mut BytesMut, after_large_request: bool) {
    if !after_large_request && buffer.capacity() <= MAX_IDLE_READ_BUFFER_CAPACITY {
        return;
    }
    let mut fresh = BytesMut::with_capacity(cmp::max(buffer.len(), READ_BUFFER_INITIAL_CAPACITY));
    fresh.extend_from_slice(buffer);
    *buffer = fresh;
}

/// Switches a substream from the canonical framing used for the handshake to the session's request framing. The
/// handshake has read exactly one frame and nothing since, so the codec is between frames, and any bytes of a request
/// the client sent straight after its handshake are carried over.
fn into_session_framing(framed: CanonicalFraming<Substream>) -> Framed<Substream, RequestCodec> {
    let parts = framed.into_parts();
    let codec = RequestCodec::new(parts.codec, rpc::max_request_size());
    let mut session_parts = FramedParts::new::<Bytes>(parts.io, codec);
    session_parts.read_buf = parts.read_buf;
    session_parts.write_buf = parts.write_buf;
    Framed::from_parts(session_parts)
}

/// The `BadRequest` response to a request of `len` bytes, over [rpc::RPC_MAX_REQUEST_SIZE]. It carries the request's
/// `request_id` so that the client can match the response.
fn oversized_request_response(request_id: u32, len: usize) -> proto::rpc::RpcResponse {
    let status = RpcStatus::bad_request(&format!(
        "The request size exceeded the maximum allowed request size. Max = {} bytes, Got = {} bytes",
        rpc::max_request_size(),
        len
    ));
    proto::rpc::RpcResponse {
        request_id,
        status: status.as_code(),
        flags: RpcMessageFlags::FIN.bits().into(),
        payload: status.to_details_bytes(),
    }
}

fn into_response(request_id: u32, result: Result<BodyBytes, RpcStatus>) -> RpcResponse {
    match result {
        Ok(msg) => {
            let mut flags = RpcMessageFlags::empty();
            if msg.is_finished() {
                flags |= RpcMessageFlags::FIN;
            }
            RpcResponse {
                request_id,
                status: RpcStatus::ok().as_status_code(),
                flags,
                payload: msg.into_bytes().unwrap_or_else(Bytes::new),
            }
        },
        Err(err) => {
            debug!(target: LOG_TARGET, "Body contained an error: {}", err);
            RpcResponse {
                request_id,
                status: err.as_status_code(),
                flags: RpcMessageFlags::FIN,
                payload: Bytes::from(err.to_details_bytes()),
            }
        },
    }
}

fn err_to_log_level(err: &io::Error) -> log::Level {
    match err.kind() {
        ErrorKind::ConnectionReset |
        ErrorKind::ConnectionAborted |
        ErrorKind::BrokenPipe |
        ErrorKind::WriteZero |
        ErrorKind::UnexpectedEof => log::Level::Debug,
        _ => log::Level::Error,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn client_deadline_is_capped_from_above() {
        let builder = RpcServerBuilder::new();
        assert_eq!(builder.maximum_client_deadline, DEFAULT_MAXIMUM_CLIENT_DEADLINE);

        // Anything at or below the ceiling is honoured as asked for.
        assert_eq!(
            builder.clamp_client_deadline(Duration::from_secs(1)),
            Duration::from_secs(1)
        );
        assert_eq!(
            builder.clamp_client_deadline(DEFAULT_MAXIMUM_CLIENT_DEADLINE),
            DEFAULT_MAXIMUM_CLIENT_DEADLINE
        );
        // The 240s block-sync deadline must survive untouched, or syncing full blocks over Tor
        // would start being cut short by peers.
        assert_eq!(
            builder.clamp_client_deadline(Duration::from_secs(240)),
            Duration::from_secs(240)
        );

        // Above it, the server's own limit applies...
        assert_eq!(
            builder.clamp_client_deadline(DEFAULT_MAXIMUM_CLIENT_DEADLINE + Duration::from_secs(1)),
            DEFAULT_MAXIMUM_CLIENT_DEADLINE
        );
        // ...including the case this exists for: a peer asking for an effectively infinite deadline
        // to switch off the service-call and streaming timeouts.
        assert_eq!(
            builder.clamp_client_deadline(Duration::from_secs(u64::MAX)),
            DEFAULT_MAXIMUM_CLIENT_DEADLINE
        );
    }

    #[test]
    fn maximum_client_deadline_is_configurable() {
        let builder = RpcServerBuilder::new().with_maximum_client_deadline(Duration::from_secs(30));
        assert_eq!(
            builder.clamp_client_deadline(Duration::from_secs(u64::MAX)),
            Duration::from_secs(30)
        );
        assert_eq!(
            builder.clamp_client_deadline(Duration::from_secs(5)),
            Duration::from_secs(5)
        );
    }

    /// After a large request, the read buffer goes back to a small allocation, keeping any bytes of the next request
    #[test]
    fn a_large_request_does_not_pin_its_read_buffer() {
        use tokio_util::codec::{Decoder, LengthDelimitedCodec};

        fn with_prefix(body: &[u8]) -> Vec<u8> {
            let mut frame = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
            frame.extend_from_slice(body);
            frame
        }

        let mut codec = RequestCodec::new(LengthDelimitedCodec::new(), rpc::max_request_size());
        let mut buffer = BytesMut::new();
        buffer.extend_from_slice(&with_prefix(&vec![1; 4 * 1024 * 1024]));
        // The start of the next (small) request has arrived too
        let small = with_prefix(&[2; 10]);
        buffer.extend_from_slice(small.get(..7).unwrap());

        let Some(RequestFrame::Request(large)) = codec.decode(&mut buffer).unwrap() else {
            panic!("expected a request");
        };
        drop(large);
        assert!(buffer.capacity() > MAX_IDLE_READ_BUFFER_CAPACITY);
        release_large_read_buffer(&mut buffer, true);
        assert!(buffer.capacity() <= MAX_IDLE_READ_BUFFER_CAPACITY);

        buffer.extend_from_slice(small.get(7..).unwrap());
        let Some(RequestFrame::Request(frame)) = codec.decode(&mut buffer).unwrap() else {
            panic!("expected a request");
        };
        assert_eq!(frame.as_ref(), &[2; 10]);
        release_large_read_buffer(&mut buffer, false);
        assert!(buffer.capacity() <= MAX_IDLE_READ_BUFFER_CAPACITY);
    }

    /// A request that fills its allocation almost exactly leaves little free space behind it, but the allocation is
    /// still released: the rest of the buffer shares it and would reclaim all of it on the next read
    #[test]
    fn a_request_filling_its_allocation_does_not_pin_it() {
        use tokio_util::codec::{Decoder, LengthDelimitedCodec};

        let body_len = (1 << 20) - 4 - 10;
        let mut frame = u32::try_from(body_len).unwrap().to_be_bytes().to_vec();
        frame.extend_from_slice(&vec![1; body_len]);

        // Read the way `Framed` reads: make room for at least one byte, then fill the free space
        let mut buffer = BytesMut::new();
        let mut remaining = frame.as_slice();
        while !remaining.is_empty() {
            buffer.reserve(1);
            let free = buffer.capacity().saturating_sub(buffer.len());
            let (now, later) = remaining.split_at(cmp::min(free, remaining.len()));
            buffer.extend_from_slice(now);
            remaining = later;
        }

        let mut codec = RequestCodec::new(LengthDelimitedCodec::new(), rpc::max_request_size());
        let Some(RequestFrame::Request(request)) = codec.decode(&mut buffer).unwrap() else {
            panic!("expected a request");
        };
        assert_eq!(request.len(), body_len);
        drop(request);
        // Little is free behind the request, so its capacity alone does not show the allocation
        assert!(buffer.capacity() <= MAX_IDLE_READ_BUFFER_CAPACITY);

        release_large_read_buffer(&mut buffer, true);
        // The next reads fill the free space and make room for more, as `Framed` does, without reclaiming the large
        // allocation
        let free = buffer.capacity().saturating_sub(buffer.len());
        buffer.extend_from_slice(&vec![2; free]);
        buffer.reserve(1);
        assert!(
            buffer.capacity() <= MAX_IDLE_READ_BUFFER_CAPACITY,
            "{} bytes still allocated",
            buffer.capacity()
        );
    }

    #[test]
    fn a_maximum_below_the_minimum_does_not_starve_requests() {
        // An operator setting this to zero (or anything under the minimum) would otherwise leave
        // the node up while aborting every request it is asked to serve.
        let builder = RpcServerBuilder::new()
            .with_minimum_client_deadline(Duration::from_secs(1))
            .with_maximum_client_deadline(Duration::from_secs(0));

        assert_eq!(
            builder.clamp_client_deadline(Duration::from_secs(0)),
            Duration::from_secs(1)
        );
        assert_eq!(
            builder.clamp_client_deadline(Duration::from_secs(120)),
            Duration::from_secs(1)
        );
        assert_eq!(
            builder.clamp_client_deadline(Duration::from_secs(u64::MAX)),
            Duration::from_secs(1)
        );
    }
}
