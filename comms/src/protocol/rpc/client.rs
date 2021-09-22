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

use super::message::RpcMethod;
use crate::{
    framing::CanonicalFraming,
    message::MessageExt,
    proto,
    protocol::{
        rpc::{
            body::ClientStreaming,
            message::{BaseRequest, RpcMessageFlags},
            Handshake,
            NamedProtocolService,
            Response,
            RpcError,
            RpcStatus,
            RPC_CHUNKING_MAX_CHUNKS,
        },
        ProtocolId,
    },
    runtime::task,
    Substream,
};
use bytes::Bytes;
use futures::{
    future::{BoxFuture, Either},
    task::{Context, Poll},
    FutureExt,
    SinkExt,
    StreamExt,
};
use log::*;
use prost::Message;
use std::{
    borrow::Cow,
    convert::TryFrom,
    fmt,
    future::Future,
    marker::PhantomData,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    time,
};
use tower::{Service, ServiceExt};
use tracing::{event, span, Instrument, Level};

const LOG_TARGET: &str = "comms::rpc::client";

#[derive(Clone)]
pub struct RpcClient {
    connector: ClientConnector,
}

impl RpcClient {
    /// Create a new RpcClient using the given framed substream and perform the RPC handshake.
    pub async fn connect(
        config: RpcClientConfig,
        framed: CanonicalFraming<Substream>,
        protocol_name: ProtocolId,
    ) -> Result<Self, RpcError> {
        let (request_tx, request_rx) = mpsc::channel(1);
        let shutdown = Shutdown::new();
        let shutdown_signal = shutdown.to_signal();
        let connector = ClientConnector::new(request_tx, shutdown);
        let (ready_tx, ready_rx) = oneshot::channel();
        let tracing_id = tracing::Span::current().id();
        task::spawn({
            let span = span!(Level::TRACE, "start_rpc_worker");
            span.follows_from(tracing_id);

            RpcClientWorker::new(config, request_rx, framed, ready_tx, protocol_name, shutdown_signal)
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
    pub fn get_last_request_latency(&mut self) -> impl Future<Output = Result<Option<Duration>, RpcError>> + '_ {
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
        let svc = self.connector.ready_and().await?;
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
    _client: PhantomData<TClient>,
}

impl<TClient> Default for RpcClientBuilder<TClient> {
    fn default() -> Self {
        Self {
            config: Default::default(),
            protocol_id: None,
            _client: PhantomData,
        }
    }
}

impl<TClient> RpcClientBuilder<TClient>
where TClient: From<RpcClient> + NamedProtocolService
{
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

    /// Negotiates and establishes a session to the peer's RPC service
    pub async fn connect(self, framed: CanonicalFraming<Substream>) -> Result<TClient, RpcError> {
        RpcClient::connect(
            self.config,
            framed,
            self.protocol_id.as_ref().cloned().unwrap_or_default(),
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
        }
    }
}

#[derive(Clone)]
pub struct ClientConnector {
    inner: mpsc::Sender<ClientRequest>,
    shutdown: Arc<Mutex<Shutdown>>,
}

impl ClientConnector {
    pub(self) fn new(sender: mpsc::Sender<ClientRequest>, shutdown: Shutdown) -> Self {
        Self {
            inner: sender,
            shutdown: Arc::new(Mutex::new(shutdown)),
        }
    }

    pub async fn close(&mut self) {
        let mut lock = self.shutdown.lock().await;
        lock.trigger();
    }

    pub async fn get_last_request_latency(&mut self) -> Result<Option<Duration>, RpcError> {
        let (reply, reply_rx) = oneshot::channel();
        self.inner
            .send(ClientRequest::GetLastRequestLatency(reply))
            .await
            .map_err(|_| RpcError::ClientClosed)?;

        reply_rx.await.map_err(|_| RpcError::RequestCancelled)
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

struct RpcClientWorker {
    config: RpcClientConfig,
    request_rx: mpsc::Receiver<ClientRequest>,
    framed: CanonicalFraming<Substream>,
    // Request ids are limited to u16::MAX because varint encoding is used over the wire and the magnitude of the value
    // sent determines the byte size. A u16 will be more than enough for the purpose
    next_request_id: u16,
    ready_tx: Option<oneshot::Sender<Result<(), RpcError>>>,
    last_request_latency: Option<Duration>,
    protocol_id: ProtocolId,
    shutdown_signal: ShutdownSignal,
}

impl RpcClientWorker {
    pub(self) fn new(
        config: RpcClientConfig,
        request_rx: mpsc::Receiver<ClientRequest>,
        framed: CanonicalFraming<Substream>,
        ready_tx: oneshot::Sender<Result<(), RpcError>>,
        protocol_id: ProtocolId,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            config,
            request_rx,
            framed,
            next_request_id: 0,
            ready_tx: Some(ready_tx),
            last_request_latency: None,
            protocol_id,
            shutdown_signal,
        }
    }

    fn protocol_name(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.protocol_id)
    }

    fn stream_id(&self) -> yamux::StreamId {
        self.framed.get_ref().id()
    }

    #[tracing::instrument(name = "rpc_client_worker run", skip(self), fields(next_request_id = self.next_request_id))]
    async fn run(mut self) {
        debug!(
            target: LOG_TARGET,
            "(stream={}) Performing client handshake for '{}'",
            self.stream_id(),
            self.protocol_name()
        );
        let start = Instant::now();
        let mut handshake = Handshake::new(&mut self.framed).with_timeout(self.config.handshake_timeout());
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
                self.last_request_latency = Some(latency);
                if let Some(r) = self.ready_tx.take() {
                    let _ = r.send(Ok(()));
                }
            },
            Err(err) => {
                if let Some(r) = self.ready_tx.take() {
                    let _ = r.send(Err(err.into()));
                }

                return;
            },
        }

        loop {
            tokio::select! {
                biased;
                _ = &mut self.shutdown_signal => {
                    break;
                }
                req = self.request_rx.recv() => {
                    match req {
                        Some(req) => {
                            if let Err(err) = self.handle_request(req).await {
                                error!(target: LOG_TARGET, "(stream={}) Unexpected error: {}. Worker is terminating.", self.stream_id(), err);
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        if let Err(err) = self.framed.close().await {
            debug!(
                target: LOG_TARGET,
                "(stream={}) IO Error when closing substream: {}",
                self.stream_id(),
                err
            );
        }

        debug!(
            target: LOG_TARGET,
            "(stream={}) RpcClientWorker ({}) terminated.",
            self.stream_id(),
            self.protocol_name()
        );
    }

    async fn handle_request(&mut self, req: ClientRequest) -> Result<(), RpcError> {
        use ClientRequest::*;
        match req {
            SendRequest { request, reply } => {
                self.do_request_response(request, reply).await?;
            },
            GetLastRequestLatency(reply) => {
                let _ = reply.send(self.last_request_latency);
            },
            SendPing(reply) => {
                self.do_ping_pong(reply).await?;
            },
        }
        Ok(())
    }

    async fn do_ping_pong(&mut self, reply: oneshot::Sender<Result<Duration, RpcStatus>>) -> Result<(), RpcError> {
        let ack = proto::rpc::RpcRequest {
            flags: RpcMessageFlags::ACK.bits() as u32,
            deadline: self.config.deadline.map(|t| t.as_secs()).unwrap_or(0),
            ..Default::default()
        };

        let start = Instant::now();
        self.framed.send(ack.to_encoded_bytes().into()).await?;

        debug!(
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
                let _ = reply.send(Err(RpcStatus::timed_out("Response timed out")));
                return Ok(());
            },
            Err(err) => return Err(err),
        };

        let status = RpcStatus::from(&resp);
        if !status.is_ok() {
            let _ = reply.send(Err(status.clone()));
            return Err(status.into());
        }

        let resp_flags = RpcMessageFlags::from_bits_truncate(resp.flags as u8);
        if !resp_flags.contains(RpcMessageFlags::ACK) {
            warn!(
                target: LOG_TARGET,
                "(stream={}) Invalid ping response {:?}",
                self.stream_id(),
                resp
            );
            let _ = reply.send(Err(RpcStatus::protocol_error(format!(
                "Received invalid ping response on protocol '{}'",
                self.protocol_name()
            ))));
            return Err(RpcError::InvalidPingResponse);
        }

        let _ = reply.send(Ok(start.elapsed()));
        Ok(())
    }

    #[tracing::instrument(name = "rpc_do_request_response", skip(self, reply, request), fields(request_method = ?request.method, request_size = request.message.len()))]
    async fn do_request_response(
        &mut self,
        request: BaseRequest<Bytes>,
        reply: oneshot::Sender<mpsc::Receiver<Result<Response<Bytes>, RpcStatus>>>,
    ) -> Result<(), RpcError> {
        let request_id = self.next_request_id();
        let method = request.method.into();
        let req = proto::rpc::RpcRequest {
            request_id: request_id as u32,
            method,
            deadline: self.config.deadline.map(|t| t.as_secs()).unwrap_or(0),
            flags: 0,
            payload: request.message.to_vec(),
        };

        debug!(target: LOG_TARGET, "Sending request: {}", req);

        let start = Instant::now();
        if reply.is_closed() {
            event!(Level::WARN, "Client request was cancelled before request was sent");
            warn!(
                target: LOG_TARGET,
                "Client request was cancelled before request was sent"
            );
        }
        self.framed.send(req.to_encoded_bytes().into()).await?;

        let (response_tx, response_rx) = mpsc::channel(10);
        if let Err(mut rx) = reply.send(response_rx) {
            event!(Level::WARN, "Client request was cancelled after request was sent");
            warn!(
                target: LOG_TARGET,
                "Client request was cancelled after request was sent"
            );
            rx.close();
            // RPC is strictly request/response
            // If the client drops the RpcClient request at this point after the, we have two options:
            // 1. Obey the protocol: receive the response
            // 2. Error out and immediately close the session (seems brittle and may be unexpected)
            // Option 1 has the disadvantage when receiving large/many streamed responses, however if all client handles
            // have been dropped, then read_reply will exit early the stream will close and the server-side
            // can exit early
        }

        loop {
            let resp = match self.read_response(request_id).await {
                Ok(resp) => {
                    let latency = start.elapsed();
                    event!(Level::TRACE, "Message received");
                    trace!(
                        target: LOG_TARGET,
                        "Received response ({} byte(s)) from request #{} (protocol = {}, method={}) in {:.0?}",
                        resp.payload.len(),
                        request_id,
                        self.protocol_name(),
                        method,
                        latency
                    );
                    self.last_request_latency = Some(latency);
                    resp
                },
                Err(RpcError::ReplyTimeout) => {
                    debug!(
                        target: LOG_TARGET,
                        "Request {} (method={}) timed out after {:.0?}",
                        request_id,
                        method,
                        start.elapsed()
                    );
                    event!(Level::ERROR, "Response timed out");
                    if !response_tx.is_closed() {
                        let _ = response_tx.send(Err(RpcStatus::timed_out("Response timed out"))).await;
                    }
                    break;
                },
                Err(RpcError::ClientClosed) => {
                    debug!(
                        target: LOG_TARGET,
                        "Request {} (method={}) was closed after {:.0?} (read_reply)",
                        request_id,
                        method,
                        start.elapsed()
                    );
                    self.request_rx.close();
                    break;
                },
                Err(err) => {
                    event!(
                        Level::WARN,
                        "Request {} (method={}) returned an error after {:.0?}: {}",
                        request_id,
                        method,
                        start.elapsed(),
                        err
                    );
                    return Err(err);
                },
            };

            match Self::convert_to_result(resp) {
                Ok(Ok(resp)) => {
                    // The consumer may drop the receiver before all responses are received.
                    // We just ignore that as we still want obey the protocol and receive messages until the FIN flag or
                    // the connection is dropped
                    let is_finished = resp.is_finished();
                    if response_tx.is_closed() {
                        warn!(
                            target: LOG_TARGET,
                            "(stream={}) Response receiver was dropped before the response/stream could complete for \
                             protocol {}, the stream will continue until completed",
                            self.framed.get_ref().id(),
                            self.protocol_name()
                        );
                    } else {
                        let _ = response_tx.send(Ok(resp)).await;
                    }
                    if is_finished {
                        break;
                    }
                },
                Ok(Err(err)) => {
                    debug!(target: LOG_TARGET, "Remote service returned error: {}", err);
                    if !response_tx.is_closed() {
                        let _ = response_tx.send(Err(err)).await;
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

    async fn read_response(&mut self, request_id: u16) -> Result<proto::rpc::RpcResponse, RpcError> {
        let mut reader = RpcResponseReader::new(&mut self.framed, self.config, request_id);
        let resp = reader.read_response().await?;
        Ok(resp)
    }

    fn next_request_id(&mut self) -> u16 {
        let next_id = self.next_request_id;
        // request_id is allowed to wrap around back to 0
        self.next_request_id = self.next_request_id.checked_add(1).unwrap_or(0);
        next_id
    }

    fn convert_to_result(resp: proto::rpc::RpcResponse) -> Result<Result<Response<Bytes>, RpcStatus>, RpcError> {
        let status = RpcStatus::from(&resp);
        if !status.is_ok() {
            return Ok(Err(status));
        }

        let resp = Response {
            flags: resp.flags(),
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
    GetLastRequestLatency(oneshot::Sender<Option<Duration>>),
    SendPing(oneshot::Sender<Result<Duration, RpcStatus>>),
}

struct RpcResponseReader<'a> {
    framed: &'a mut CanonicalFraming<Substream>,
    config: RpcClientConfig,
    request_id: u16,
}
impl<'a> RpcResponseReader<'a> {
    pub fn new(framed: &'a mut CanonicalFraming<Substream>, config: RpcClientConfig, request_id: u16) -> Self {
        Self {
            framed,
            config,
            request_id,
        }
    }

    pub async fn read_response(&mut self) -> Result<proto::rpc::RpcResponse, RpcError> {
        let mut resp = self.next().await?;
        self.check_response(&resp)?;
        let mut chunk_count = 1;
        let mut last_chunk_flags = RpcMessageFlags::from_bits_truncate(resp.flags as u8);
        let mut last_chunk_size = resp.payload.len();
        loop {
            trace!(
                target: LOG_TARGET,
                "Chunk {} received (flags={:?}, {} bytes, {} total)",
                chunk_count,
                last_chunk_flags,
                last_chunk_size,
                resp.payload.len()
            );
            if !last_chunk_flags.is_more() {
                return Ok(resp);
            }

            if chunk_count >= RPC_CHUNKING_MAX_CHUNKS {
                return Err(RpcError::ExceededMaxChunkCount {
                    expected: RPC_CHUNKING_MAX_CHUNKS,
                });
            }

            let msg = self.next().await?;
            last_chunk_flags = RpcMessageFlags::from_bits_truncate(msg.flags as u8);
            last_chunk_size = msg.payload.len();
            self.check_response(&resp)?;
            resp.payload.extend(msg.payload);
            chunk_count += 1;
        }
    }

    pub async fn read_ack(&mut self) -> Result<proto::rpc::RpcResponse, RpcError> {
        let resp = self.next().await?;
        Ok(resp)
    }

    fn check_response(&self, resp: &proto::rpc::RpcResponse) -> Result<(), RpcError> {
        let resp_id = u16::try_from(resp.request_id)
            .map_err(|_| RpcStatus::protocol_error(format!("invalid request_id: must be less than {}", u16::MAX)))?;

        let flags = RpcMessageFlags::from_bits_truncate(resp.flags as u8);
        if flags.contains(RpcMessageFlags::ACK) {
            return Err(RpcError::UnexpectedAckResponse);
        }

        if resp_id != self.request_id {
            return Err(RpcError::ResponseIdDidNotMatchRequest {
                expected: self.request_id,
                actual: resp.request_id as u16,
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
