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

use futures::{future, stream::FuturesUnordered, SinkExt, StreamExt};
use log::*;
use prost::Message;
use router::Router;
use tokio::{sync::mpsc, task::JoinHandle, time};
use tokio_stream::Stream;
use tower::{make::MakeService, Service};
use tracing::{debug, error, instrument, span, trace, warn, Instrument, Level};

use super::{
    body::Body,
    context::{RequestContext, RpcCommsProvider},
    error::HandshakeRejectReason,
    message::{Request, Response, RpcMessageFlags},
    not_found::ProtocolServiceNotFound,
    status::RpcStatus,
    Handshake,
    RPC_MAX_FRAME_SIZE,
};
use crate::{
    bounded_executor::BoundedExecutor,
    framing,
    framing::CanonicalFraming,
    message::MessageExt,
    peer_manager::NodeId,
    proto,
    protocol::{
        rpc,
        rpc::{
            body::BodyBytes,
            message::{RpcMethod, RpcResponse},
            server::early_close::EarlyClose,
        },
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
        ProtocolNotificationRx,
    },
    stream_id::StreamId,
    Bytes,
    Substream,
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

#[derive(Clone)]
pub struct RpcServerBuilder {
    maximum_simultaneous_sessions: Option<usize>,
    maximum_sessions_per_client: Option<usize>,
    minimum_client_deadline: Duration,
    handshake_timeout: Duration,
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

    pub fn with_unlimited_sessions_per_client(mut self) -> Self {
        self.maximum_sessions_per_client = None;
        self
    }

    pub fn with_minimum_client_deadline(mut self, deadline: Duration) -> Self {
        self.minimum_client_deadline = deadline;
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

impl Default for RpcServerBuilder {
    fn default() -> Self {
        Self {
            maximum_simultaneous_sessions: None,
            maximum_sessions_per_client: None,
            minimum_client_deadline: Duration::from_secs(1),
            handshake_timeout: Duration::from_secs(15),
        }
    }
}

pub(super) struct PeerRpcServer<TSvc, TCommsProvider> {
    executor: BoundedExecutor,
    config: RpcServerBuilder,
    service: TSvc,
    protocol_notifications: Option<ProtocolNotificationRx<Substream>>,
    comms_provider: TCommsProvider,
    request_rx: mpsc::Receiver<RpcServerRequest>,
    sessions: HashMap<NodeId, usize>,
    tasks: FuturesUnordered<JoinHandle<NodeId>>,
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
            tasks: FuturesUnordered::new(),
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

                Some(Ok(node_id)) = self.tasks.next() => {
                    self.on_session_complete(&node_id);
                },

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

    async fn handle_request(&self, req: RpcServerRequest) {
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
                let num_active = self.sessions.get(&node_id).copied().unwrap_or(0);
                let _ = reply.send(num_active);
            },
        }
    }

    async fn handle_protocol_notification(
        &mut self,
        notification: ProtocolNotification<Substream>,
    ) -> Result<(), RpcServerError> {
        match notification.event {
            ProtocolEvent::NewInboundSubstream(node_id, substream) => {
                debug!(
                    target: LOG_TARGET,
                    "New client connection for protocol `{}` from peer `{}`",
                    String::from_utf8_lossy(&notification.protocol),
                    node_id
                );

                let framed = framing::canonical(substream, RPC_MAX_FRAME_SIZE);
                match self
                    .try_initiate_service(notification.protocol.clone(), &node_id, framed)
                    .await
                {
                    Ok(_) => {},
                    Err(err @ RpcServerError::HandshakeError(_)) => {
                        debug!(target: LOG_TARGET, "Handshake error: {}", err);
                        #[cfg(feature = "metrics")]
                        metrics::handshake_error_counter(&node_id, &notification.protocol).inc();
                    },
                    Err(err) => {
                        debug!(target: LOG_TARGET, "Unable to spawn RPC service: {}", err);
                    },
                }
            },
        }

        Ok(())
    }

    fn new_session_for(&mut self, node_id: NodeId) -> Result<usize, RpcServerError> {
        let count = self.sessions.entry(node_id.clone()).or_insert(0);
        match self.config.maximum_sessions_per_client {
            Some(max) if max > 0 => {
                debug_assert!(*count <= max);
                if *count >= max {
                    return Err(RpcServerError::MaxSessionsPerClientReached {
                        node_id,
                        max_sessions: max,
                    });
                }
            },
            Some(_) | None => {},
        }

        *count += 1;
        Ok(*count)
    }

    fn on_session_complete(&mut self, node_id: &NodeId) {
        info!(target: LOG_TARGET, "Session complete for {}", node_id);
        if let Some(v) = self.sessions.get_mut(node_id) {
            *v -= 1;
            if *v == 0 {
                self.sessions.remove(node_id);
            }
        }
    }

    async fn try_initiate_service(
        &mut self,
        protocol: ProtocolId,
        node_id: &NodeId,
        mut framed: CanonicalFraming<Substream>,
    ) -> Result<(), RpcServerError> {
        let mut handshake = Handshake::new(&mut framed).with_timeout(self.config.handshake_timeout);

        if !self.executor.can_spawn() {
            let msg = format!("Used all {} sessions", self.executor.max_available());
            debug!(
                target: LOG_TARGET,
                "Rejecting RPC session request for peer `{}` because {}",
                node_id,
                HandshakeRejectReason::NoServerSessionsAvailable("Cannot spawn more sessions")
            );
            handshake
                .reject_with_reason(HandshakeRejectReason::NoServerSessionsAvailable(
                    "Cannot spawn more sessions",
                ))
                .await?;
            return Err(RpcServerError::MaximumSessionsReached(msg));
        }

        let service = match self.service.make_service(protocol.clone()).await {
            Ok(s) => s,
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Rejecting RPC session request for peer `{}` because {}",
                    node_id,
                    HandshakeRejectReason::ProtocolNotSupported
                );
                handshake
                    .reject_with_reason(HandshakeRejectReason::ProtocolNotSupported)
                    .await?;
                return Err(err);
            },
        };

        match self.new_session_for(node_id.clone()) {
            Ok(num_sessions) => {
                info!(
                    target: LOG_TARGET,
                    "NEW SESSION for {} ({} active) ", node_id, num_sessions
                );
            },

            Err(err) => {
                handshake
                    .reject_with_reason(HandshakeRejectReason::NoServerSessionsAvailable(
                        "Maximum sessions for client",
                    ))
                    .await?;
                return Err(err);
            },
        }

        let version = handshake.perform_server_handshake().await?;
        debug!(
            target: LOG_TARGET,
            "Server negotiated RPC v{} with client node `{}`", version, node_id
        );

        let service = ActivePeerRpcService::new(
            self.config.clone(),
            protocol,
            node_id.clone(),
            service,
            framed,
            self.comms_provider.clone(),
        );

        let node_id = node_id.clone();
        let handle = self
            .executor
            .try_spawn(async move {
                #[cfg(feature = "metrics")]
                let num_sessions = metrics::num_sessions(&node_id, &service.protocol);
                #[cfg(feature = "metrics")]
                num_sessions.inc();
                service.start().await;
                info!(target: LOG_TARGET, "END OF SESSION for {} ", node_id,);
                #[cfg(feature = "metrics")]
                num_sessions.dec();

                node_id
            })
            .map_err(|e| RpcServerError::MaximumSessionsReached(format!("{:?}", e)))?;

        self.tasks.push(handle);

        Ok(())
    }
}

struct ActivePeerRpcService<TSvc, TCommsProvider> {
    config: RpcServerBuilder,
    protocol: ProtocolId,
    node_id: NodeId,
    service: TSvc,
    framed: EarlyClose<CanonicalFraming<Substream>>,
    comms_provider: TCommsProvider,
    logging_context_string: Arc<String>,
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
    ) -> Self {
        Self {
            logging_context_string: Arc::new(format!(
                "stream_id: {}, peer: {}, protocol: {}",
                framed.stream_id(),
                node_id,
                String::from_utf8_lossy(&protocol)
            )),

            config,
            protocol,
            node_id,
            service,
            framed: EarlyClose::new(framed),
            comms_provider,
        }
    }

    async fn start(mut self) {
        debug!(
            target: LOG_TARGET,
            "({}) Rpc server started.", self.logging_context_string,
        );
        if let Err(err) = self.run().await {
            #[cfg(feature = "metrics")]
            metrics::error_counter(&self.node_id, &self.protocol, &err).inc();
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

    async fn run(&mut self) -> Result<(), RpcServerError> {
        while let Some(result) = self.framed.next().await {
            match result {
                Ok(frame) => {
                    #[cfg(feature = "metrics")]
                    metrics::inbound_requests_bytes(&self.node_id, &self.protocol).observe(frame.len() as f64);

                    let start = Instant::now();

                    if let Err(err) = self.handle_request(frame.freeze()).await {
                        if let Err(err) = self.framed.close().await {
                            let level = err.io().map(err_to_log_level).unwrap_or(log::Level::Error);

                            log!(
                                target: LOG_TARGET,
                                level,
                                "({}) Failed to close substream after socket error: {}",
                                self.logging_context_string,
                                err,
                            );
                        }
                        let level = err.early_close_io().map(err_to_log_level).unwrap_or(log::Level::Error);
                        log!(
                            target: LOG_TARGET,
                            level,
                            "(peer: {}, protocol: {}) Failed to handle request: {}",
                            self.node_id,
                            self.protocol_name(),
                            err
                        );
                        return Err(err);
                    }
                    let elapsed = start.elapsed();
                    trace!(
                        target: LOG_TARGET,
                        "({}) RPC request completed in {:.0?}{}",
                        self.logging_context_string,
                        elapsed,
                        if elapsed.as_secs() > 5 { " (LONG REQUEST)" } else { "" }
                    );
                },
                Err(err) => {
                    if let Err(err) = self.framed.close().await {
                        error!(
                            target: LOG_TARGET,
                            "({}) Failed to close substream after socket error: {}", self.logging_context_string, err
                        );
                    }
                    return Err(err.into());
                },
            }
        }

        self.framed.close().await?;
        Ok(())
    }

    #[instrument(name = "rpc::server::handle_req", level="trace", skip(self, request), err, fields(request_size = request.len ()))]
    async fn handle_request(&mut self, mut request: Bytes) -> Result<(), RpcServerError> {
        let decoded_msg = proto::rpc::RpcRequest::decode(&mut request)?;

        let request_id = decoded_msg.request_id;
        let method = RpcMethod::from(decoded_msg.method);
        let deadline = Duration::from_secs(decoded_msg.deadline);

        // The client side deadline MUST be greater or equal to the minimum_client_deadline
        if deadline < self.config.minimum_client_deadline {
            debug!(
                target: LOG_TARGET,
                "({}) Client has an invalid deadline. {}", self.logging_context_string, decoded_msg
            );
            // Let the client know that they have disobeyed the spec
            let status = RpcStatus::bad_request(&format!(
                "Invalid deadline ({:.0?}). The deadline MUST be greater than {:.0?}.",
                self.node_id, deadline,
            ));
            let bad_request = proto::rpc::RpcResponse {
                request_id,
                status: status.as_code(),
                flags: RpcMessageFlags::FIN.bits().into(),
                payload: status.to_details_bytes(),
            };
            #[cfg(feature = "metrics")]
            metrics::status_error_counter(&self.node_id, &self.protocol, status.as_status_code()).inc();
            self.framed.send(bad_request.to_encoded_bytes().into()).await?;
            return Ok(());
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
            return Ok(());
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
            self.framed.send(ack.to_encoded_bytes().into()).await?;
            return Ok(());
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

        let service_call = log_timing(
            self.logging_context_string.clone(),
            request_id,
            "service call",
            self.service.call(req),
        );
        let service_result = time::timeout(deadline, service_call).await;
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
                metrics::error_counter(
                    &self.node_id,
                    &self.protocol,
                    &RpcServerError::ServiceCallExceededDeadline,
                )
                .inc();
                return Ok(());
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
                metrics::status_error_counter(&self.node_id, &self.protocol, err.as_status_code()).inc();
                self.framed.send(resp.to_encoded_bytes().into()).await?;
            },
        }

        Ok(())
    }

    fn protocol_name(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.protocol)
    }

    async fn process_body(
        &mut self,
        request_id: u32,
        deadline: Duration,
        body: Response<Body>,
    ) -> Result<(), RpcServerError> {
        trace!(target: LOG_TARGET, "Service call succeeded");

        #[cfg(feature = "metrics")]
        let node_id = self.node_id.clone();
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
                    metrics::status_error_counter(&node_id, &protocol, message.status).inc();
                }
                message.to_proto()
            })
            .map(|resp| Bytes::from(resp.to_encoded_bytes()));

        loop {
            let next_item = log_timing(
                self.logging_context_string.clone(),
                request_id,
                "message read",
                stream.next(),
            );
            let timeout = time::sleep(deadline);

            tokio::select! {
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
                msg = next_item => {
                     match msg {
                         Some(msg) => {
                            #[cfg(feature = "metrics")]
                            metrics::outbound_response_bytes(&self.node_id, &self.protocol).observe(msg.len() as f64);
                            trace!(
                                target: LOG_TARGET,
                                "({}) Sending body len = {}",
                                self.logging_context_string,
                                msg.len()
                            );

                            self.framed.send(msg).await?;
                        },
                        None => {
                            trace!(target: LOG_TARGET, "{} Request complete", self.logging_context_string,);
                            break;
                        },
                    }
                },

                _ = timeout => {
                     debug!(
                        target: LOG_TARGET,
                        "({}) Failed to return result within client deadline ({:.0?})",
                        self.logging_context_string,
                        deadline
                    );

                    #[cfg(feature = "metrics")]
                    metrics::error_counter(
                        &self.node_id,
                        &self.protocol,
                        &RpcServerError::ReadStreamExceededDeadline,
                    )
                    .inc();
                    break;
                }
            } // end select!
        } // end loop
        Ok(())
    }

    async fn check_interruptions(&mut self) -> Result<(), RpcServerError> {
        let check = future::poll_fn(|cx| match Pin::new(&mut self.framed).poll_next(cx) {
            Poll::Ready(Some(Ok(mut msg))) => {
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
                            "invalid message flag, does not match any flags ({})",
                            u8_bits
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

async fn log_timing<R, F: Future<Output = R>>(context_str: Arc<String>, request_id: u32, tag: &str, fut: F) -> R {
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
