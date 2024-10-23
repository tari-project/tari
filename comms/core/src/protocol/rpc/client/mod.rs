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

pub mod pool;

#[cfg(test)]
mod tests;

#[cfg(feature = "metrics")]
mod metrics;

use std::{
    borrow::Cow,
    convert::TryFrom,
    fmt,
    future::Future,
    marker::PhantomData,
    sync::Arc,
    time::{Duration, Instant},
};

use bytes::Bytes;
use futures::{
    future,
    future::{BoxFuture, Either},
    task::{Context, Poll},
    FutureExt,
    SinkExt,
    StreamExt,
};
use log::*;
use prost::Message;
use tari_shutdown::{
    oneshot_trigger::{OneshotSignal, OneshotTrigger},
    Shutdown,
    ShutdownSignal,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc, oneshot, watch, Mutex},
    time,
};
use tower::{Service, ServiceExt};
use tracing::{span, Instrument, Level};

use super::message::RpcMethod;
use crate::{
    framing::CanonicalFraming,
    message::MessageExt,
    peer_manager::NodeId,
    proto,
    protocol::{
        rpc,
        rpc::{
            body::ClientStreaming,
            message::{BaseRequest, RpcMessageFlags},
            Handshake,
            NamedProtocolService,
            Response,
            RpcError,
            RpcServerError,
            RpcStatus,
        },
        ProtocolId,
    },
    stream_id,
    stream_id::StreamId,
};

const LOG_TARGET: &str = "comms::rpc::client";

#[derive(Clone)]
pub struct RpcClient {
    connector: ClientConnector,
}

impl RpcClient {
    pub fn builder<T>() -> RpcClientBuilder<T>
    where T: NamedProtocolService {
        RpcClientBuilder::new().with_protocol_id(T::PROTOCOL_NAME.into())
    }

    /// Create a new RpcClient using the given framed substream and perform the RPC handshake.
    pub async fn connect<TSubstream>(
        config: RpcClientConfig,
        node_id: NodeId,
        framed: CanonicalFraming<TSubstream>,
        protocol_name: ProtocolId,
        drop_receiver: Option<OneshotTrigger<NodeId>>,
    ) -> Result<Self, RpcError>
    where
        TSubstream: AsyncRead + AsyncWrite + Unpin + Send + StreamId + 'static,
    {
        trace!(target: LOG_TARGET,"connect to {:?} with {:?}", node_id, config);
        let (request_tx, request_rx) = mpsc::channel(1);
        let shutdown = Shutdown::new();
        let shutdown_signal = shutdown.to_signal();
        let (last_request_latency_tx, last_request_latency_rx) = watch::channel(None);
        let connector = ClientConnector::new(request_tx, last_request_latency_rx, shutdown);
        let (ready_tx, ready_rx) = oneshot::channel();
        let tracing_id = tracing::Span::current().id();
        let drop_signal = if let Some(val) = drop_receiver.as_ref() {
            val.to_signal()
        } else {
            OneshotTrigger::<NodeId>::new().to_signal()
        };
        tokio::spawn({
            let span = span!(Level::TRACE, "start_rpc_worker");
            span.follows_from(tracing_id);

            RpcClientWorker::new(
                config,
                node_id,
                request_rx,
                last_request_latency_tx,
                framed,
                ready_tx,
                protocol_name,
                shutdown_signal,
                drop_signal,
            )
            .run()
            .instrument(span)
        });
        ready_rx
            .await
            .expect("ready_rx oneshot is never dropped without a reply")?;
        Ok(Self { connector })
    }

    /// Perform a single request and single response
    pub async fn request_response<T, R, M>(&mut self, request: T, method: M) -> Result<R, RpcError>
    where
        T: prost::Message,
        R: prost::Message + Default + std::fmt::Debug,
        M: Into<RpcMethod>,
    {
        let req_bytes = request.to_encoded_bytes();
        let request = BaseRequest::new(method.into(), req_bytes.into());

        let mut resp = self.call_inner(request).await?;
        let resp = resp.recv().await.ok_or(RpcError::ServerClosedRequest)??;
        let resp = R::decode(resp.into_message())?;

        Ok(resp)
    }

    /// Perform a single request and streaming response
    pub async fn server_streaming<T, M, R>(&mut self, request: T, method: M) -> Result<ClientStreaming<R>, RpcError>
    where
        T: prost::Message,
        R: prost::Message + Default,
        M: Into<RpcMethod>,
    {
        let req_bytes = request.to_encoded_bytes();
        let request = BaseRequest::new(method.into(), req_bytes.into());

        let resp = self.call_inner(request).await?;

        Ok(ClientStreaming::new(resp))
    }

    /// Close the RPC session. Any subsequent calls will error.
    pub async fn close(&mut self) {
        self.connector.close().await;
    }

    pub fn is_connected(&self) -> bool {
        self.connector.is_connected()
    }

    /// Return the latency of the last request
    pub fn get_last_request_latency(&mut self) -> Option<Duration> {
        self.connector.get_last_request_latency()
    }

    /// Sends a ping and returns the latency
    pub fn ping(&mut self) -> impl Future<Output = Result<Duration, RpcError>> + '_ {
        self.connector.send_ping()
    }

    async fn call_inner(
        &mut self,
        request: BaseRequest<Bytes>,
    ) -> Result<mpsc::Receiver<Result<Response<Bytes>, RpcStatus>>, RpcError> {
        let svc = self.connector.ready().await?;
        let resp = svc.call(request).await?;
        Ok(resp)
    }
}

impl fmt::Debug for RpcClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RpcClient {{ inner: ... }}")
    }
}

#[derive(Debug, Clone)]
pub struct RpcClientBuilder<TClient> {
    config: RpcClientConfig,
    protocol_id: Option<ProtocolId>,
    node_id: Option<NodeId>,
    drop_receiver: Option<OneshotTrigger<NodeId>>,
    _client: PhantomData<TClient>,
}

impl<TClient> Default for RpcClientBuilder<TClient> {
    fn default() -> Self {
        Self {
            config: Default::default(),
            protocol_id: None,
            node_id: None,
            drop_receiver: None,
            _client: PhantomData,
        }
    }
}

impl<TClient> RpcClientBuilder<TClient> {
    pub fn new() -> Self {
        Default::default()
    }

    /// The deadline to send to the peer when performing a request.
    /// If this deadline is exceeded, the server SHOULD abandon the request.
    /// The client will return a timeout error if the deadline plus the grace period is exceeded.
    ///
    /// _Note: That is the deadline is set too low, the responding peer MAY immediately reject the request.
    ///
    /// Default: 100s
    pub fn with_deadline(mut self, timeout: Duration) -> Self {
        self.config.deadline = Some(timeout);
        self
    }

    /// Sets the grace period to allow after the configured deadline before giving up and timing out.
    /// This configuration should be set to comfortably account for the latency experienced during requests.
    ///
    /// Default: 10 seconds
    pub fn with_deadline_grace_period(mut self, timeout: Duration) -> Self {
        self.config.deadline_grace_period = timeout;
        self
    }

    /// Set the length of time that the client will wait for a response in the RPC handshake before returning a timeout
    /// error.
    /// Default: 15 seconds
    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.config.handshake_timeout = timeout;
        self
    }

    /// Set the protocol ID associated with this client. This is used for logging purposes only.
    pub fn with_protocol_id(mut self, protocol_id: ProtocolId) -> Self {
        self.protocol_id = Some(protocol_id);
        self
    }

    /// Set the node_id for logging/metrics purposes
    pub fn with_node_id(mut self, node_id: NodeId) -> Self {
        self.node_id = Some(node_id);
        self
    }

    /// Old RPC connections will be dropped when a new connection is established.
    pub fn with_drop_old_connections(mut self, drop_old_connections: bool) -> Self {
        self.config.drop_old_connections = drop_old_connections;
        self
    }

    /// Set the drop receiver to be used to trigger the client to close
    pub fn with_drop_receiver(mut self, drop_receiver: OneshotTrigger<NodeId>) -> Self {
        self.drop_receiver = Some(drop_receiver);
        self
    }
}

impl<TClient> RpcClientBuilder<TClient>
where TClient: From<RpcClient> + NamedProtocolService
{
    /// Negotiates and establishes a session to the peer's RPC service
    pub async fn connect<TSubstream>(self, framed: CanonicalFraming<TSubstream>) -> Result<TClient, RpcError>
    where TSubstream: AsyncRead + AsyncWrite + Unpin + Send + StreamId + 'static {
        RpcClient::connect(
            self.config,
            self.node_id.unwrap_or_default(),
            framed,
            self.protocol_id
                .as_ref()
                .cloned()
                .unwrap_or_else(|| ProtocolId::from_static(TClient::PROTOCOL_NAME)),
            self.drop_receiver,
        )
        .await
        .map(Into::into)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RpcClientConfig {
    pub deadline: Option<Duration>,
    pub deadline_grace_period: Duration,
    pub handshake_timeout: Duration,
    pub drop_old_connections: bool,
}

impl RpcClientConfig {
    /// Returns the timeout including the configured grace period
    pub fn timeout_with_grace_period(&self) -> Option<Duration> {
        self.deadline.map(|d| d + self.deadline_grace_period)
    }

    /// Returns the handshake timeout
    pub fn handshake_timeout(&self) -> Duration {
        self.handshake_timeout
    }
}

impl Default for RpcClientConfig {
    fn default() -> Self {
        Self {
            deadline: Some(Duration::from_secs(120)),
            deadline_grace_period: Duration::from_secs(60),
            handshake_timeout: Duration::from_secs(90),
            drop_old_connections: false,
        }
    }
}

#[derive(Clone)]
pub struct ClientConnector {
    inner: mpsc::Sender<ClientRequest>,
    last_request_latency_rx: watch::Receiver<Option<Duration>>,
    shutdown: Arc<Mutex<Shutdown>>,
}

impl ClientConnector {
    pub(self) fn new(
        sender: mpsc::Sender<ClientRequest>,
        last_request_latency_rx: watch::Receiver<Option<Duration>>,
        shutdown: Shutdown,
    ) -> Self {
        Self {
            inner: sender,
            last_request_latency_rx,
            shutdown: Arc::new(Mutex::new(shutdown)),
        }
    }

    pub async fn close(&mut self) {
        let mut lock = self.shutdown.lock().await;
        lock.trigger();
    }

    pub fn get_last_request_latency(&mut self) -> Option<Duration> {
        *self.last_request_latency_rx.borrow()
    }

    pub async fn send_ping(&mut self) -> Result<Duration, RpcError> {
        let (reply, reply_rx) = oneshot::channel();
        self.inner
            .send(ClientRequest::SendPing(reply))
            .await
            .map_err(|_| RpcError::ClientClosed)?;

        let latency = reply_rx.await.map_err(|_| RpcError::RequestCancelled)??;
        Ok(latency)
    }

    pub fn is_connected(&self) -> bool {
        !self.inner.is_closed()
    }
}

impl fmt::Debug for ClientConnector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClientConnector {{ inner: ... }}")
    }
}

impl Service<BaseRequest<Bytes>> for ClientConnector {
    type Error = RpcError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = mpsc::Receiver<Result<Response<Bytes>, RpcStatus>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: BaseRequest<Bytes>) -> Self::Future {
        let (reply, reply_rx) = oneshot::channel();
        let inner = self.inner.clone();
        async move {
            inner
                .send(ClientRequest::SendRequest { request, reply })
                .await
                .map_err(|_| RpcError::ClientClosed)?;

            reply_rx.await.map_err(|_| RpcError::RequestCancelled)
        }
        .boxed()
    }
}

struct RpcClientWorker<TSubstream> {
    config: RpcClientConfig,
    node_id: NodeId,
    request_rx: mpsc::Receiver<ClientRequest>,
    last_request_latency_tx: watch::Sender<Option<Duration>>,
    framed: CanonicalFraming<TSubstream>,
    // Request ids are limited to u16::MAX because varint encoding is used over the wire and the magnitude of the value
    // sent determines the byte size. A u16 will be more than enough for the purpose
    next_request_id: u16,
    ready_tx: Option<oneshot::Sender<Result<(), RpcError>>>,
    protocol_id: ProtocolId,
    shutdown_signal: ShutdownSignal,
    drop_signal: OneshotSignal<NodeId>,
}

impl<TSubstream> RpcClientWorker<TSubstream>
where TSubstream: AsyncRead + AsyncWrite + Unpin + Send + StreamId
{
    pub(self) fn new(
        config: RpcClientConfig,
        node_id: NodeId,
        request_rx: mpsc::Receiver<ClientRequest>,
        last_request_latency_tx: watch::Sender<Option<Duration>>,
        framed: CanonicalFraming<TSubstream>,
        ready_tx: oneshot::Sender<Result<(), RpcError>>,
        protocol_id: ProtocolId,
        shutdown_signal: ShutdownSignal,
        drop_signal: OneshotSignal<NodeId>,
    ) -> Self {
        Self {
            config,
            node_id,
            request_rx,
            framed,
            next_request_id: 0,
            ready_tx: Some(ready_tx),
            last_request_latency_tx,
            protocol_id,
            shutdown_signal,
            drop_signal,
        }
    }

    fn protocol_name(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.protocol_id)
    }

    fn stream_id(&self) -> stream_id::Id {
        self.framed.stream_id()
    }

    async fn run(mut self) {
        debug!(
            target: LOG_TARGET,
            "(stream={}) Performing client handshake for '{}'",
            self.stream_id(),
            self.protocol_name()
        );
        let start = Instant::now();
        let mut handshake = Handshake::new(&mut self.framed)
            .with_timeout(self.config.handshake_timeout())
            .with_drop_old_connections(self.config.drop_old_connections);
        match handshake.perform_client_handshake().await {
            Ok(_) => {
                let latency = start.elapsed();
                debug!(
                    target: LOG_TARGET,
                    "(stream={}) RPC Session ({}) negotiation completed. Latency: {:.0?}",
                    self.stream_id(),
                    self.protocol_name(),
                    latency
                );
                let _ = self.last_request_latency_tx.send(Some(latency));
                if let Some(r) = self.ready_tx.take() {
                    let _result = r.send(Ok(()));
                }
                #[cfg(feature = "metrics")]
                metrics::handshake_counter(&self.node_id, &self.protocol_id).inc();
            },
            Err(err) => {
                #[cfg(feature = "metrics")]
                metrics::handshake_errors(&self.node_id, &self.protocol_id).inc();
                if let Some(r) = self.ready_tx.take() {
                    let _result = r.send(Err(err.into()));
                }

                return;
            },
        }

        #[cfg(feature = "metrics")]
        metrics::num_sessions(&self.node_id, &self.protocol_id).inc();
        loop {
            tokio::select! {
                // Check the futures in the order they are listed
                biased;
                _ = &mut self.shutdown_signal => {
                    break;
                }
                node_id = &mut self.drop_signal => {
                    debug!(
                        target: LOG_TARGET, "(stream={}) Peer '{}' connection has dropped. Worker is terminating.",
                        self.stream_id(), node_id.unwrap_or_default()
                    );
                    break;
                }
                req = self.request_rx.recv() => {
                    match req {
                        Some(req) => {
                            if let Err(err) = self.handle_request(req).await {
                                #[cfg(feature = "metrics")]
                                metrics::client_errors(&self.node_id, &self.protocol_id).inc();
                                error!(
                                    target: LOG_TARGET,
                                    "(stream={}) Unexpected error: {}. Worker is terminating.",
                                    self.stream_id(), err
                                );
                                break;
                            }
                        }
                        None => {
                            debug!(
                                target: LOG_TARGET,
                                "(stream={}) Request channel closed. Worker is terminating.",
                                self.stream_id()
                            );
                            break
                        },
                    }
                }
            }
        }
        #[cfg(feature = "metrics")]
        metrics::num_sessions(&self.node_id, &self.protocol_id).dec();

        if let Err(err) = self.framed.close().await {
            debug!(
                target: LOG_TARGET,
                "(stream: {}, peer: {}) IO Error when closing substream: {}",
                self.stream_id(),
                self.node_id,
                err
            );
        }

        debug!(
            target: LOG_TARGET,
            "(stream: {}, peer: {}) RpcClientWorker ({}) terminated.",
            self.stream_id(),
            self.node_id,
            self.protocol_name()
        );
    }

    async fn handle_request(&mut self, req: ClientRequest) -> Result<(), RpcError> {
        use ClientRequest::{SendPing, SendRequest};
        match req {
            SendRequest { request, reply } => {
                self.do_request_response(request, reply).await?;
            },
            SendPing(reply) => {
                self.do_ping_pong(reply).await?;
            },
        }
        Ok(())
    }

    async fn do_ping_pong(&mut self, reply: oneshot::Sender<Result<Duration, RpcStatus>>) -> Result<(), RpcError> {
        let ack = proto::rpc::RpcRequest {
            flags: u32::from(RpcMessageFlags::ACK.bits()),
            deadline: self.config.deadline.map(|t| t.as_secs()).unwrap_or(0),
            ..Default::default()
        };

        let start = Instant::now();
        self.framed.send(ack.to_encoded_bytes().into()).await?;

        trace!(
            target: LOG_TARGET,
            "(stream={}) Ping (protocol {}) sent in {:.2?}",
            self.stream_id(),
            self.protocol_name(),
            start.elapsed()
        );
        let mut reader = RpcResponseReader::new(&mut self.framed, self.config, 0);
        let resp = match reader.read_ack().await {
            Ok(resp) => resp,
            Err(RpcError::ReplyTimeout) => {
                debug!(
                    target: LOG_TARGET,
                    "(stream={}) Ping timed out after {:.0?}",
                    self.stream_id(),
                    start.elapsed()
                );
                #[cfg(feature = "metrics")]
                metrics::client_timeouts(&self.node_id, &self.protocol_id).inc();
                let _result = reply.send(Err(RpcStatus::timed_out("Response timed out")));
                return Ok(());
            },
            Err(err) => return Err(err),
        };

        let status = RpcStatus::from(&resp);
        if !status.is_ok() {
            let _result = reply.send(Err(status.clone()));
            return Err(status.into());
        }

        let resp_flags =
            RpcMessageFlags::from_bits(u8::try_from(resp.flags).map_err(|_| {
                RpcStatus::protocol_error(&format!("invalid message flag: must be less than {}", u8::MAX))
            })?)
            .ok_or(RpcStatus::protocol_error(&format!(
                "invalid message flag, does not match any flags ({})",
                resp.flags
            )))?;
        if !resp_flags.contains(RpcMessageFlags::ACK) {
            warn!(
                target: LOG_TARGET,
                "(stream={}) Invalid ping response {:?}",
                self.stream_id(),
                resp
            );
            let _result = reply.send(Err(RpcStatus::protocol_error(&format!(
                "Received invalid ping response on protocol '{}'",
                self.protocol_name()
            ))));
            return Err(RpcError::InvalidPingResponse);
        }

        let _result = reply.send(Ok(start.elapsed()));
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn do_request_response(
        &mut self,
        request: BaseRequest<Bytes>,
        reply: oneshot::Sender<mpsc::Receiver<Result<Response<Bytes>, RpcStatus>>>,
    ) -> Result<(), RpcError> {
        #[cfg(feature = "metrics")]
        metrics::outbound_request_bytes(&self.node_id, &self.protocol_id).observe(request.get_ref().len() as f64);

        let request_id = self.next_request_id();
        let method = request.method.into();
        let req = proto::rpc::RpcRequest {
            request_id: u32::from(request_id),
            method,
            deadline: self.config.deadline.map(|t| t.as_secs()).unwrap_or(0),
            flags: 0,
            payload: request.message.to_vec(),
        };

        trace!(target: LOG_TARGET, "Sending request: {}", req);

        if reply.is_closed() {
            warn!(
                target: LOG_TARGET,
                "Client request was cancelled before request was sent"
            );
        }

        let (response_tx, response_rx) = mpsc::channel(5);
        if let Err(mut rx) = reply.send(response_rx) {
            warn!(
                target: LOG_TARGET,
                "Client request was cancelled after request was sent. This means that you are making an RPC request \
                 and then immediately dropping the response! (protocol = {})",
                self.protocol_name(),
            );
            rx.close();
            return Ok(());
        }

        #[cfg(feature = "metrics")]
        let latency = metrics::request_response_latency(&self.node_id, &self.protocol_id);
        #[cfg(feature = "metrics")]
        let mut metrics_timer = Some(latency.start_timer());

        let timer = Instant::now();
        if let Err(err) = self.send_request(req).await {
            warn!(target: LOG_TARGET, "{}", err);
            #[cfg(feature = "metrics")]
            metrics::client_errors(&self.node_id, &self.protocol_id).inc();
            let _result = response_tx.send(Err(err.into())).await;
            return Ok(());
        }
        let partial_latency = timer.elapsed();

        loop {
            if self.shutdown_signal.is_triggered() {
                debug!(
                    target: LOG_TARGET,
                    "[peer: {}, protocol: {}, stream_id: {}, req_id: {}] Client connector closed. Quitting stream \
                     early",
                    self.node_id,
                    self.protocol_name(),
                    self.stream_id(),
                    request_id
                );
                break;
            }

            // Check if the response receiver has been dropped while receiving messages
            let resp_result = {
                let resp_fut = self.read_response(request_id);
                tokio::pin!(resp_fut);
                let closed_fut = response_tx.closed();
                tokio::pin!(closed_fut);
                match future::select(resp_fut, closed_fut).await {
                    Either::Left((r, _)) => Some(r),
                    Either::Right(_) => None,
                }
            };
            let resp_result = match resp_result {
                Some(r) => r,
                None => {
                    self.premature_close(request_id, method).await?;
                    break;
                },
            };

            let resp = match resp_result {
                Ok((resp, time_to_first_msg)) => {
                    if let Some(t) = time_to_first_msg {
                        let _ = self.last_request_latency_tx.send(Some(partial_latency + t));
                    }
                    trace!(
                        target: LOG_TARGET,
                        "Received response ({} byte(s)) from request #{} (protocol = {}, method={})",
                        resp.payload.len(),
                        request_id,
                        self.protocol_name(),
                        method,
                    );

                    #[cfg(feature = "metrics")]
                    if let Some(t) = metrics_timer.take() {
                        t.observe_duration();
                    }
                    resp
                },
                Err(RpcError::ReplyTimeout) => {
                    debug!(
                        target: LOG_TARGET,
                        "Request {} (method={}) timed out", request_id, method,
                    );
                    #[cfg(feature = "metrics")]
                    metrics::client_timeouts(&self.node_id, &self.protocol_id).inc();
                    if response_tx.is_closed() {
                        self.premature_close(request_id, method).await?;
                    } else {
                        let _result = response_tx.send(Err(RpcStatus::timed_out("Response timed out"))).await;
                    }
                    break;
                },
                Err(RpcError::ClientClosed) => {
                    debug!(
                        target: LOG_TARGET,
                        "Request {} (method={}) was closed (read_reply)", request_id, method,
                    );
                    self.request_rx.close();
                    break;
                },
                Err(err) => {
                    return Err(err);
                },
            };

            match Self::convert_to_result(resp) {
                Ok(Ok(resp)) => {
                    let is_finished = resp.is_finished();
                    // The consumer may drop the receiver before all responses are received.
                    // We handle this by sending a 'FIN' message to the server.
                    if response_tx.is_closed() {
                        self.premature_close(request_id, method).await?;
                        break;
                    } else {
                        let _result = response_tx.send(Ok(resp)).await;
                    }
                    if is_finished {
                        break;
                    }
                },
                Ok(Err(err)) => {
                    debug!(target: LOG_TARGET, "Remote service returned error: {}", err);
                    if !response_tx.is_closed() {
                        let _result = response_tx.send(Err(err)).await;
                    }
                    break;
                },
                Err(err @ RpcError::ResponseIdDidNotMatchRequest { .. }) |
                Err(err @ RpcError::UnexpectedAckResponse) => {
                    warn!(target: LOG_TARGET, "{}", err);
                    // Ignore the response, this can happen when there is excessive latency. The server sends back a
                    // reply before the deadline but it is only received after the client has timed
                    // out
                    continue;
                },
                Err(err) => return Err(err),
            }
        }

        Ok(())
    }

    async fn premature_close(&mut self, request_id: u16, method: u32) -> Result<(), RpcError> {
        warn!(
            target: LOG_TARGET,
            "(stream={}) Response receiver was dropped before the response/stream could complete for protocol {}, \
             interrupting the stream. ",
            self.stream_id(),
            self.protocol_name()
        );
        let req = proto::rpc::RpcRequest {
            request_id: u32::from(request_id),
            method,
            flags: RpcMessageFlags::FIN.bits().into(),
            deadline: self.config.deadline.map(|d| d.as_secs()).unwrap_or(0),
            ..Default::default()
        };

        // If we cannot set FIN quickly, just exit
        if let Ok(res) = time::timeout(Duration::from_secs(2), self.send_request(req)).await {
            res?;
        }
        Ok(())
    }

    async fn send_request(&mut self, req: proto::rpc::RpcRequest) -> Result<(), RpcError> {
        let payload = req.to_encoded_bytes();
        if payload.len() > rpc::max_request_size() {
            return Err(RpcError::MaxRequestSizeExceeded {
                got: payload.len(),
                expected: rpc::max_request_size(),
            });
        }
        self.framed.send(payload.into()).await?;
        Ok(())
    }

    async fn read_response(
        &mut self,
        request_id: u16,
    ) -> Result<(proto::rpc::RpcResponse, Option<Duration>), RpcError> {
        let stream_id = self.stream_id();
        let protocol_name = self.protocol_name().to_string();

        let mut reader = RpcResponseReader::new(&mut self.framed, self.config, request_id);
        let mut num_ignored = 0;
        let resp = loop {
            match reader.read_response().await {
                Ok(resp) => {
                    trace!(
                        target: LOG_TARGET,
                        "(stream: {}, {}) Received body len = {}",
                        stream_id,
                        protocol_name,
                        reader.bytes_read()
                    );
                    #[cfg(feature = "metrics")]
                    metrics::inbound_response_bytes(&self.node_id, &self.protocol_id)
                        .observe(reader.bytes_read() as f64);
                    let time_to_first_msg = reader.time_to_first_msg();
                    break (resp, time_to_first_msg);
                },
                Err(RpcError::ResponseIdDidNotMatchRequest { actual, expected })
                    if actual.wrapping_add(1) == request_id =>
                {
                    warn!(
                        target: LOG_TARGET,
                        "Possible delayed response received for previous request {}", actual
                    );
                    num_ignored += 1;

                    // Be lenient for a number of messages that may have been buffered to come through for the previous
                    // request.
                    const MAX_ALLOWED_IGNORED: usize = 20;
                    if num_ignored > MAX_ALLOWED_IGNORED {
                        return Err(RpcError::ResponseIdDidNotMatchRequest { actual, expected });
                    }
                    continue;
                },
                Err(err) => return Err(err),
            }
        };
        Ok(resp)
    }

    fn next_request_id(&mut self) -> u16 {
        let mut next_id = self.next_request_id;
        // request_id is allowed to wrap around back to 0
        self.next_request_id = self.next_request_id.wrapping_add(1);
        // We dont want request id of zero because that is the default for varint on protobuf, so it is possible for the
        // entire message to be zero bytes (WriteZero IO error)
        if next_id == 0 {
            next_id += 1;
            self.next_request_id += 1;
        }
        next_id
    }

    fn convert_to_result(resp: proto::rpc::RpcResponse) -> Result<Result<Response<Bytes>, RpcStatus>, RpcError> {
        let status = RpcStatus::from(&resp);
        if !status.is_ok() {
            return Ok(Err(status));
        }
        let flags = match resp.flags() {
            Ok(flags) => flags,
            Err(e) => return Ok(Err(RpcError::ServerError(RpcServerError::ProtocolError(e)).into())),
        };
        let resp = Response {
            flags,
            payload: resp.payload.into(),
        };

        Ok(Ok(resp))
    }
}

pub enum ClientRequest {
    SendRequest {
        request: BaseRequest<Bytes>,
        reply: oneshot::Sender<mpsc::Receiver<Result<Response<Bytes>, RpcStatus>>>,
    },
    SendPing(oneshot::Sender<Result<Duration, RpcStatus>>),
}

struct RpcResponseReader<'a, TSubstream> {
    framed: &'a mut CanonicalFraming<TSubstream>,
    config: RpcClientConfig,
    request_id: u16,
    bytes_read: usize,
    time_to_first_msg: Option<Duration>,
}

impl<'a, TSubstream> RpcResponseReader<'a, TSubstream>
where TSubstream: AsyncRead + AsyncWrite + Unpin
{
    pub fn new(framed: &'a mut CanonicalFraming<TSubstream>, config: RpcClientConfig, request_id: u16) -> Self {
        Self {
            framed,
            config,
            request_id,
            bytes_read: 0,
            time_to_first_msg: None,
        }
    }

    pub fn bytes_read(&self) -> usize {
        self.bytes_read
    }

    pub fn time_to_first_msg(&self) -> Option<Duration> {
        self.time_to_first_msg
    }

    pub async fn read_response(&mut self) -> Result<proto::rpc::RpcResponse, RpcError> {
        let timer = Instant::now();
        let resp = self.next().await?;
        self.time_to_first_msg = Some(timer.elapsed());
        self.check_response(&resp)?;
        self.bytes_read = resp.payload.len();
        trace!(
            target: LOG_TARGET,
            "Received {} bytes in {:.2?}",
            resp.payload.len(),
            self.time_to_first_msg.unwrap_or_default()
        );
        Ok(resp)
    }

    pub async fn read_ack(&mut self) -> Result<proto::rpc::RpcResponse, RpcError> {
        let resp = self.next().await?;
        Ok(resp)
    }

    fn check_response(&self, resp: &proto::rpc::RpcResponse) -> Result<(), RpcError> {
        let resp_id = u16::try_from(resp.request_id)
            .map_err(|_| RpcStatus::protocol_error(&format!("invalid request_id: must be less than {}", u16::MAX)))?;

        let flags =
            RpcMessageFlags::from_bits(u8::try_from(resp.flags).map_err(|_| {
                RpcStatus::protocol_error(&format!("invalid message flag: must be less than {}", u8::MAX))
            })?)
            .ok_or(RpcStatus::protocol_error(&format!(
                "invalid message flag, does not match any flags ({})",
                resp.flags
            )))?;
        if flags.contains(RpcMessageFlags::ACK) {
            return Err(RpcError::UnexpectedAckResponse);
        }

        if resp_id != self.request_id {
            return Err(RpcError::ResponseIdDidNotMatchRequest {
                expected: self.request_id,
                actual: u16::try_from(resp.request_id).map_err(|_| {
                    RpcStatus::protocol_error(&format!("invalid request_id: must be less than {}", u16::MAX))
                })?,
            });
        }

        Ok(())
    }

    async fn next(&mut self) -> Result<proto::rpc::RpcResponse, RpcError> {
        // Wait until the timeout, allowing an extra grace period to account for latency
        let next_msg_fut = match self.config.timeout_with_grace_period() {
            Some(timeout) => Either::Left(time::timeout(timeout, self.framed.next())),
            None => Either::Right(self.framed.next().map(Ok)),
        };

        match next_msg_fut.await {
            Ok(Some(Ok(resp))) => Ok(proto::rpc::RpcResponse::decode(resp)?),
            Ok(Some(Err(err))) => Err(err.into()),
            Ok(None) => Err(RpcError::ServerClosedRequest),
            Err(_) => Err(RpcError::ReplyTimeout),
        }
    }
}
