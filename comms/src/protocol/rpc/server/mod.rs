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

mod error;
pub use error::RpcServerError;

mod handle;
pub use handle::RpcServerHandle;
use handle::RpcServerRequest;

pub mod mock;

mod router;
use router::Router;

use super::{
    body::Body,
    context::{RequestContext, RpcCommsProvider},
    error::HandshakeRejectReason,
    message::{Request, Response, RpcMessageFlags},
    not_found::ProtocolServiceNotFound,
    status::RpcStatus,
    Handshake,
    RpcStatusCode,
    RPC_MAX_FRAME_SIZE,
};
use crate::{
    bounded_executor::BoundedExecutor,
    framing,
    framing::CanonicalFraming,
    message::MessageExt,
    peer_manager::NodeId,
    proto,
    protocol::{ProtocolEvent, ProtocolId, ProtocolNotification, ProtocolNotificationRx},
    Bytes,
};
use futures::{channel::mpsc, AsyncRead, AsyncWrite, SinkExt, StreamExt};
use log::*;
use prost::Message;
use std::{
    borrow::Cow,
    future::Future,
    time::{Duration, Instant},
};
use tokio::time;
use tower::Service;
use tower_make::MakeService;

const LOG_TARGET: &str = "comms::rpc";

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

    pub(super) async fn serve<S, TSubstream, TCommsProvider>(
        self,
        service: S,
        notifications: ProtocolNotificationRx<TSubstream>,
        comms_provider: TCommsProvider,
    ) -> Result<(), RpcServerError>
    where
        TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
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
    minimum_client_deadline: Duration,
    handshake_timeout: Duration,
}

impl RpcServerBuilder {
    fn new() -> Self {
        Default::default()
    }

    pub fn with_maximum_simultaneous_sessions(mut self, limit: usize) -> Self {
        self.maximum_simultaneous_sessions = Some(limit);
        self
    }

    pub fn with_unlimited_simultaneous_sessions(mut self) -> Self {
        self.maximum_simultaneous_sessions = None;
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
            maximum_simultaneous_sessions: Some(1000),
            minimum_client_deadline: Duration::from_secs(1),
            handshake_timeout: Duration::from_secs(15),
        }
    }
}

pub(super) struct PeerRpcServer<TSvc, TSubstream, TCommsProvider> {
    executor: BoundedExecutor,
    config: RpcServerBuilder,
    service: TSvc,
    protocol_notifications: Option<ProtocolNotificationRx<TSubstream>>,
    comms_provider: TCommsProvider,
    request_rx: Option<mpsc::Receiver<RpcServerRequest>>,
}

impl<TSvc, TSubstream, TCommsProvider> PeerRpcServer<TSvc, TSubstream, TCommsProvider>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
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
        protocol_notifications: ProtocolNotificationRx<TSubstream>,
        comms_provider: TCommsProvider,
        request_rx: mpsc::Receiver<RpcServerRequest>,
    ) -> Self {
        Self {
            executor: match config.maximum_simultaneous_sessions {
                Some(num) => BoundedExecutor::from_current(num),
                None => BoundedExecutor::allow_maximum(),
            },
            config,
            service,
            protocol_notifications: Some(protocol_notifications),
            comms_provider,
            request_rx: Some(request_rx),
        }
    }

    pub async fn serve(mut self) -> Result<(), RpcServerError> {
        let mut protocol_notifs = self
            .protocol_notifications
            .take()
            .expect("PeerRpcServer initialized without protocol_notifications");

        let mut requests = self
            .request_rx
            .take()
            .expect("PeerRpcServer initialized without request_rx");

        loop {
            futures::select! {
                 maybe_notif = protocol_notifs.next() => {
                     match maybe_notif {
                         Some(notif) => self.handle_protocol_notification(notif).await?,
                         // No more protocol notifications to come, so we're done
                         None => break,
                     }
                 }

                 req = requests.select_next_some() => {
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
        }
    }

    async fn handle_protocol_notification(
        &mut self,
        notification: ProtocolNotification<TSubstream>,
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
                match self.try_initiate_service(notification.protocol, node_id, framed).await {
                    Ok(_) => {},
                    Err(err) => {
                        debug!(target: LOG_TARGET, "Unable to spawn RPC service: {}", err);
                    },
                }
            },
        }

        Ok(())
    }

    async fn try_initiate_service(
        &mut self,
        protocol: ProtocolId,
        node_id: NodeId,
        mut framed: CanonicalFraming<TSubstream>,
    ) -> Result<(), RpcServerError> {
        let mut handshake = Handshake::new(&mut framed).with_timeout(self.config.handshake_timeout);

        if !self.executor.can_spawn() {
            debug!(
                target: LOG_TARGET,
                "Rejecting RPC session request for peer `{}` because {}",
                node_id,
                HandshakeRejectReason::NoSessionsAvailable
            );
            handshake
                .reject_with_reason(HandshakeRejectReason::NoSessionsAvailable)
                .await?;
            return Err(RpcServerError::MaximumSessionsReached);
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

        let version = handshake.perform_server_handshake().await?;
        debug!(
            target: LOG_TARGET,
            "Server negotiated RPC v{} with client node `{}`", version, node_id
        );

        let service = ActivePeerRpcService {
            config: self.config.clone(),
            protocol,
            node_id: node_id.clone(),
            framed,
            service,
            comms_provider: self.comms_provider.clone(),
        };

        self.executor
            .try_spawn(service.start())
            .map_err(|_| RpcServerError::MaximumSessionsReached)?;

        Ok(())
    }
}

struct ActivePeerRpcService<TSvc, TSubstream, TCommsProvider> {
    config: RpcServerBuilder,
    protocol: ProtocolId,
    node_id: NodeId,
    service: TSvc,
    framed: CanonicalFraming<TSubstream>,
    comms_provider: TCommsProvider,
}

impl<TSvc, TSubstream, TCommsProvider> ActivePeerRpcService<TSvc, TSubstream, TCommsProvider>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin,
    TSvc: Service<Request<Bytes>, Response = Response<Body>, Error = RpcStatus>,
    TCommsProvider: RpcCommsProvider + Send + Clone + 'static,
{
    async fn start(mut self) {
        debug!(
            target: LOG_TARGET,
            "(Peer = `{}`) Rpc server ({}) started.",
            self.node_id,
            self.protocol_name()
        );
        if let Err(err) = self.run().await {
            error!(
                target: LOG_TARGET,
                "(Peer = `{}`) Rpc server ({}) exited with an error: {}",
                self.node_id,
                self.protocol_name(),
                err
            );
        }
        debug!(
            target: LOG_TARGET,
            "(Peer = {}) Rpc service ({}) shutdown",
            self.node_id,
            self.protocol_name()
        );
    }

    fn protocol_name(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.protocol)
    }

    async fn run(&mut self) -> Result<(), RpcServerError> {
        while let Some(result) = self.framed.next().await {
            let start = Instant::now();
            if let Err(err) = self.handle(result?.freeze()).await {
                self.framed.close().await?;
                return Err(err);
            }
            let elapsed = start.elapsed();
            debug!(
                target: LOG_TARGET,
                "RPC ({}) request completed in {:.0?}{}",
                self.protocol_name(),
                elapsed,
                if elapsed.as_secs() > 5 { " (LONG REQUEST)" } else { "" }
            );
        }

        self.framed.close().await?;
        Ok(())
    }

    async fn handle(&mut self, mut request: Bytes) -> Result<(), RpcServerError> {
        let decoded_msg = proto::rpc::RpcRequest::decode(&mut request)?;

        let request_id = decoded_msg.request_id;
        let method = decoded_msg.method.into();
        let deadline = Duration::from_secs(decoded_msg.deadline);

        // The client side deadline MUST be greater or equal to the minimum_client_deadline
        if deadline < self.config.minimum_client_deadline {
            debug!(
                target: LOG_TARGET,
                "[Peer=`{}`] Client has an invalid deadline. {}", self.node_id, decoded_msg
            );
            // Let the client know that they have disobeyed the spec
            let status = RpcStatus::bad_request(format!(
                "Invalid deadline ({:.0?}). The deadline MUST be greater than {:.0?}.",
                self.node_id, deadline,
            ));
            let bad_request = proto::rpc::RpcResponse {
                request_id,
                status: status.as_code(),
                flags: RpcMessageFlags::FIN.bits().into(),
                message: status.details_bytes(),
            };
            self.framed.send(bad_request.to_encoded_bytes().into()).await?;
            return Ok(());
        }

        let msg_flags = RpcMessageFlags::from_bits_truncate(decoded_msg.flags as u8);
        if msg_flags.contains(RpcMessageFlags::ACK) {
            debug!(
                target: LOG_TARGET,
                "[Peer=`{}` {}] sending ACK response.",
                self.node_id,
                self.protocol_name()
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

        debug!(
            target: LOG_TARGET,
            "[Peer=`{}`] Got request {}", self.node_id, decoded_msg
        );

        let req = Request::with_context(
            self.create_request_context(request_id),
            method,
            decoded_msg.message.into(),
        );

        let service_call = log_timing(request_id, "service call", self.service.call(req));
        let service_result = time::timeout(deadline, service_call).await;
        let service_result = match service_result {
            Ok(v) => v,
            Err(_) => {
                warn!(
                    target: LOG_TARGET,
                    "RPC service was not able to complete within the deadline ({:.0?}). Request aborted.", deadline
                );
                return Ok(());
            },
        };

        match service_result {
            Ok(body) => {
                // This is the most basic way we can push responses back to the peer. Keeping this here for reference
                // and possible future evaluation
                //
                // body.into_message()
                //     .map(|msg| match msg {
                //         Ok(msg) => {
                //             trace!(target: LOG_TARGET, "Sending body len = {}", msg.len());
                //             let mut flags = RpcMessageFlags::empty();
                //             if msg.is_finished() {
                //                 flags |= RpcMessageFlags::FIN;
                //             }
                //             proto::rpc::RpcResponse {
                //                 request_id,
                //                 status: RpcStatus::ok().as_code(),
                //                 flags: flags.bits().into(),
                //                 message: msg.into(),
                //             }
                //         },
                //         Err(err) => {
                //             debug!(target: LOG_TARGET, "Body contained an error: {}", err);
                //             proto::rpc::RpcResponse {
                //                 request_id,
                //                 status: err.as_code(),
                //                 flags: RpcMessageFlags::FIN.bits().into(),
                //                 message: err.details().as_bytes().to_vec(),
                //             }
                //         },
                //     })
                //     .map(|resp| Ok(resp.to_encoded_bytes().into()))
                //     .forward(PreventClose::new(sink))
                //     .await?;

                let mut message = body.into_message();
                loop {
                    let msg_read = log_timing(request_id, "message read", message.next());
                    match time::timeout(deadline, msg_read).await {
                        Ok(Some(msg)) => {
                            let resp = match msg {
                                Ok(msg) => {
                                    trace!(target: LOG_TARGET, "Sending body len = {}", msg.len());
                                    let mut flags = RpcMessageFlags::empty();
                                    if msg.is_finished() {
                                        flags |= RpcMessageFlags::FIN;
                                    }
                                    proto::rpc::RpcResponse {
                                        request_id,
                                        status: RpcStatus::ok().as_code(),
                                        flags: flags.bits().into(),
                                        message: msg.into(),
                                    }
                                },
                                Err(err) => {
                                    debug!(target: LOG_TARGET, "Body contained an error: {}", err);
                                    proto::rpc::RpcResponse {
                                        request_id,
                                        status: err.as_code(),
                                        flags: RpcMessageFlags::FIN.bits().into(),
                                        message: err.details().as_bytes().to_vec(),
                                    }
                                },
                            };

                            let is_valid =
                                log_timing(request_id, "transmit", self.send_response(request_id, resp)).await?;

                            if !is_valid {
                                break;
                            }
                        },
                        Ok(None) => break,
                        Err(_) => {
                            debug!(
                                target: LOG_TARGET,
                                "Failed to return result within client deadline ({:.0?})", deadline
                            );

                            break;
                        },
                    }
                } // end loop
            },
            Err(err) => {
                debug!(target: LOG_TARGET, "Service returned an error: {}", err);
                let resp = proto::rpc::RpcResponse {
                    request_id,
                    status: err.as_code(),
                    flags: RpcMessageFlags::FIN.bits().into(),
                    message: err.details_bytes(),
                };

                self.framed.send(resp.to_encoded_bytes().into()).await?;
            },
        }

        Ok(())
    }

    /// Sends an RpcResponse on the given Sink. If the size of the message exceeds the RPC_MAX_FRAME_SIZE, an error is
    /// returned to the client and false is returned from this function, otherwise the message is sent and true is
    /// returned
    async fn send_response(&mut self, request_id: u32, resp: proto::rpc::RpcResponse) -> Result<bool, RpcServerError> {
        match resp.to_encoded_bytes() {
            buf if buf.len() > RPC_MAX_FRAME_SIZE => {
                let msg = format!(
                    "This node tried to return a message that exceeds the maximum frame size. Max = {:.4} MiB, Got = \
                     {:.4} MiB",
                    RPC_MAX_FRAME_SIZE as f32 / (1024.0 * 1024.0),
                    buf.len() as f32 / (1024.0 * 1024.0)
                );
                warn!(target: LOG_TARGET, "{}", msg);
                self.framed
                    .send(
                        proto::rpc::RpcResponse {
                            request_id,
                            status: RpcStatusCode::MalformedResponse as u32,
                            flags: RpcMessageFlags::FIN.bits().into(),
                            message: msg.as_bytes().to_vec(),
                        }
                        .to_encoded_bytes()
                        .into(),
                    )
                    .await?;
                Ok(false)
            },
            buf => {
                self.framed.send(buf.into()).await?;
                Ok(true)
            },
        }
    }

    fn create_request_context(&self, request_id: u32) -> RequestContext {
        RequestContext::new(request_id, self.node_id.clone(), Box::new(self.comms_provider.clone()))
    }
}

async fn log_timing<R, F: Future<Output = R>>(request_id: u32, tag: &str, fut: F) -> R {
    let t = Instant::now();
    let ret = fut.await;
    let elapsed = t.elapsed();
    trace!(
        target: LOG_TARGET,
        "RPC TIMING(REQ_ID={}): '{}' took {:.2}s{}",
        request_id,
        tag,
        elapsed.as_secs_f32(),
        if elapsed.as_secs() >= 5 { " (SLOW)" } else { "" }
    );
    ret
}
