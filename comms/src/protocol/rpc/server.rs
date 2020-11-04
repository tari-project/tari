//  Copyright 2020, The Tari Project
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

use super::{
    body::Body,
    message::{Request, Response},
    not_found::ProtocolServiceNotFound,
    router::Router,
    status::RpcStatus,
    RpcError,
};
use crate::{
    bounded_executor::OptionallyBoundedExecutor,
    framing,
    framing::CanonicalFraming,
    message::MessageExt,
    peer_manager::NodeId,
    proto,
    protocol::{
        rpc::{
            context::{RequestContext, RpcCommsProvider},
            message::RpcMessageFlags,
            Handshake,
            RPC_MAX_FRAME_SIZE,
        },
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
        ProtocolNotificationRx,
    },
    Bytes,
};
use futures::{AsyncRead, AsyncWrite, Sink, SinkExt, StreamExt};
use log::*;
use prost::Message;
use std::{
    io,
    time::{Duration, Instant},
};
use tari_shutdown::{OptionalShutdownSignal, ShutdownSignal};
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

#[derive(Debug, Clone)]
pub struct RpcServer {
    maximum_concurrent_sessions: Option<usize>,
    max_frame_size: usize,
    minimum_client_deadline: Duration,
    shutdown_signal: OptionalShutdownSignal,
}

impl RpcServer {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add_service<S>(self, service: S) -> Router<S, ProtocolServiceNotFound>
    where
        S: MakeService<ProtocolId, Request<Bytes>, MakeError = RpcError, Response = Response<Body>, Error = RpcStatus>
            + NamedProtocolService
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        Router::new(self, service)
    }

    pub fn maximum_concurrent_sessions(mut self, limit: usize) -> Self {
        self.maximum_concurrent_sessions = Some(limit);
        self
    }

    pub fn with_unlimited_concurrent_sessions(mut self) -> Self {
        self.maximum_concurrent_sessions = None;
        self
    }

    pub fn with_minimum_client_deadline(mut self, deadline: Duration) -> Self {
        self.minimum_client_deadline = deadline;
        self
    }

    pub fn with_shutdown_signal(mut self, shutdown_signal: ShutdownSignal) -> Self {
        self.shutdown_signal = Some(shutdown_signal).into();
        self
    }

    pub(super) async fn serve<S, TSubstream, TCommsProvider>(
        self,
        service: S,
        notifications: ProtocolNotificationRx<TSubstream>,
        comms_provider: TCommsProvider,
    ) -> Result<(), RpcError>
    where
        TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
        S: MakeService<ProtocolId, Request<Bytes>, MakeError = RpcError, Response = Response<Body>, Error = RpcStatus>
            + Send
            + 'static,
        S::Service: Send + 'static,
        S::Future: Send + 'static,
        S::Service: Send + 'static,
        <S::Service as Service<Request<Bytes>>>::Future: Send + 'static,
        TCommsProvider: RpcCommsProvider + Clone + Send + 'static,
    {
        PeerRpcServer::new(self, service, notifications, comms_provider)
            .serve()
            .await
    }
}

impl Default for RpcServer {
    fn default() -> Self {
        Self {
            maximum_concurrent_sessions: Some(100),
            max_frame_size: RPC_MAX_FRAME_SIZE,
            minimum_client_deadline: Duration::from_secs(1),
            shutdown_signal: Default::default(),
        }
    }
}

pub(super) struct PeerRpcServer<TSvc, TSubstream, TCommsProvider> {
    executor: OptionallyBoundedExecutor,
    config: RpcServer,
    service: TSvc,
    protocol_notifications: Option<ProtocolNotificationRx<TSubstream>>,
    comms_provider: TCommsProvider,
}

impl<TSvc, TSubstream, TCommsProvider> PeerRpcServer<TSvc, TSubstream, TCommsProvider>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    TSvc: MakeService<ProtocolId, Request<Bytes>, MakeError = RpcError, Response = Response<Body>, Error = RpcStatus>
        + Send
        + 'static,
    TSvc::Service: Send + 'static,
    <TSvc::Service as Service<Request<Bytes>>>::Future: Send + 'static,
    TSvc::Future: Send + 'static,
    TCommsProvider: RpcCommsProvider + Clone + Send + 'static,
{
    pub fn new(
        config: RpcServer,
        service: TSvc,
        protocol_notifications: ProtocolNotificationRx<TSubstream>,
        comms_provider: TCommsProvider,
    ) -> Self
    {
        Self {
            executor: OptionallyBoundedExecutor::from_current(config.maximum_concurrent_sessions),
            config,
            service,
            protocol_notifications: Some(protocol_notifications),
            comms_provider,
        }
    }

    pub async fn serve(mut self) -> Result<(), RpcError> {
        let mut protocol_notifs = self
            .protocol_notifications
            .take()
            .unwrap()
            .take_until(self.config.shutdown_signal.clone());

        while let Some(notif) = protocol_notifs.next().await {
            self.handle_protocol_notification(notif).await?;
        }

        debug!(
            target: LOG_TARGET,
            "Peer RPC server is shut down because the shutdown signal was triggered or the protocol notification \
             stream ended"
        );

        Ok(())
    }

    async fn handle_protocol_notification(
        &mut self,
        notification: ProtocolNotification<TSubstream>,
    ) -> Result<(), RpcError>
    {
        match notification.event {
            ProtocolEvent::NewInboundSubstream(node_id, substream) => {
                debug!(
                    target: LOG_TARGET,
                    "New client connection for protocol `{}` from peer `{}`",
                    String::from_utf8_lossy(&notification.protocol),
                    node_id
                );

                let framed = framing::canonical(substream, self.config.max_frame_size);
                match self.try_spawn_service(notification.protocol, node_id, framed).await {
                    Ok(_) => {},
                    Err(err) => {
                        debug!(target: LOG_TARGET, "Unable to spawn RPC service: {}", err);
                    },
                }
            },
        }

        Ok(())
    }

    async fn try_spawn_service(
        &mut self,
        protocol: ProtocolId,
        node_id: NodeId,
        mut framed: CanonicalFraming<TSubstream>,
    ) -> Result<(), RpcError>
    {
        if !self.executor.can_spawn() {
            debug!(
                target: LOG_TARGET,
                "Closing substream to peer `{}` because maximum number of concurrent services has been reached",
                node_id
            );
            framed.close().await?;
            return Err(RpcError::MaximumConcurrencyReached);
        }

        let service = match self.service.make_service(protocol).await {
            Ok(s) => s,
            Err(err) => {
                framed.close().await?;
                return Err(err);
            },
        };

        let mut handshake = Handshake::new(&mut framed);
        let version = handshake.perform_server_handshake().await?;
        debug!(
            target: LOG_TARGET,
            "Server negotiated RPC v{} with client node `{}`", version, node_id
        );

        let service = ActivePeerRpcService {
            config: self.config.clone(),
            node_id,
            framed: Some(framed),
            service,
            comms_provider: self.comms_provider.clone(),
            shutdown_signal: self.config.shutdown_signal.clone(),
        };

        self.executor
            .try_spawn(service.start())
            .map_err(|_| RpcError::MaximumConcurrencyReached)?;

        Ok(())
    }
}

struct ActivePeerRpcService<TSvc, TSubstream, TCommsProvider> {
    config: RpcServer,
    node_id: NodeId,
    service: TSvc,
    framed: Option<CanonicalFraming<TSubstream>>,
    comms_provider: TCommsProvider,
    shutdown_signal: OptionalShutdownSignal,
}

impl<TSvc, TSubstream, TCommsProvider> ActivePeerRpcService<TSvc, TSubstream, TCommsProvider>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin,
    TSvc: Service<Request<Bytes>, Response = Response<Body>, Error = RpcStatus>,
    TCommsProvider: RpcCommsProvider + Send + Clone + 'static,
{
    async fn start(mut self) {
        debug!(target: LOG_TARGET, "(Peer = `{}`) Rpc server started.", self.node_id);
        if let Err(err) = self.run().await {
            error!(
                target: LOG_TARGET,
                "(Peer = `{}`) Rpc server exited with an error: {}", self.node_id, err
            );
        }
        debug!(target: LOG_TARGET, "(Peer = {}) Rpc service shutdown", self.node_id);
    }

    async fn run(&mut self) -> Result<(), RpcError> {
        let (mut sink, stream) = self.framed.take().unwrap().split();
        let mut stream = stream.fuse().take_until(self.shutdown_signal.clone());

        while let Some(result) = stream.next().await {
            let start = Instant::now();
            if let Err(err) = self.handle(&mut sink, result?.freeze()).await {
                sink.close().await?;
                return Err(err);
            }
            debug!(target: LOG_TARGET, "RPC request completed in {:.0?}", start.elapsed());
        }

        sink.close().await?;
        Ok(())
    }

    fn create_request_context(&self) -> RequestContext {
        RequestContext::new(self.node_id.clone(), Box::new(self.comms_provider.clone()))
    }

    async fn handle<W>(&mut self, sink: &mut W, mut request: Bytes) -> Result<(), RpcError>
    where W: Sink<Bytes, Error = io::Error> + Unpin + ?Sized {
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
            sink.send(bad_request.to_encoded_bytes().into()).await?;
            return Ok(());
        }

        debug!(
            target: LOG_TARGET,
            "[Peer=`{}`] Got request {}", self.node_id, decoded_msg
        );

        let req = Request::with_context(self.create_request_context(), method, decoded_msg.message.into());

        let service_result = time::timeout(deadline, self.service.call(req)).await;
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
                let mut body = body.into_message();
                while let Some(r) = body.next().await {
                    let resp = match r {
                        Ok(msg) => {
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

                    sink.send(resp.to_encoded_bytes().into()).await?;
                }
            },
            Err(err) => {
                debug!(target: LOG_TARGET, "Service returned an error: {}", err);
                let resp = proto::rpc::RpcResponse {
                    request_id,
                    status: err.as_code(),
                    flags: RpcMessageFlags::FIN.bits().into(),
                    message: err.details_bytes(),
                };

                sink.send(resp.to_encoded_bytes().into()).await?;
            },
        }

        Ok(())
    }
}
