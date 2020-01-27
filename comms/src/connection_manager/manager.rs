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

use super::dialer::DialerRequest;
use crate::{
    backoff::Backoff,
    connection_manager::{
        dialer::Dialer,
        error::ConnectionManagerError,
        listener::PeerListener,
        peer_connection::PeerConnection,
        requester::ConnectionManagerRequest,
    },
    noise::NoiseConfig,
    peer_manager::{AsyncPeerManager, NodeId, NodeIdentity},
    protocol::{ProtocolEvent, ProtocolId, ProtocolNotifier},
    transports::Transport,
    types::DEFAULT_LISTENER_ADDRESS,
};
use futures::{
    channel::{mpsc, oneshot},
    stream::Fuse,
    AsyncRead,
    AsyncWrite,
    StreamExt,
};
use log::*;
use multiaddr::Multiaddr;
use std::{collections::HashMap, sync::Arc};
use tari_shutdown::ShutdownSignal;
use tokio::runtime;

const LOG_TARGET: &str = "comms::connection_manager::manager";

const EVENT_CHANNEL_SIZE: usize = 32;
const ESTABLISHER_CHANNEL_SIZE: usize = 32;

#[derive(Debug)]
pub enum ConnectionManagerEvent {
    // Peer connection
    PeerConnected(Box<PeerConnection>),
    PeerDisconnected(Box<NodeId>),
    PeerConnectFailed(Box<NodeId>, ConnectionManagerError),
    PeerInboundConnectFailed(ConnectionManagerError),

    // Listener
    Listening(Multiaddr),
    ListenFailed(ConnectionManagerError),

    // Substreams
    NewInboundSubstream(Box<NodeId>, ProtocolId, yamux::Stream),
}

#[derive(Debug, Clone)]
pub struct ConnectionManagerConfig {
    /// The address to listen on for incoming connections. This address must be supported by the transport.
    /// Default: DEFAULT_LISTENER_ADDRESS constant
    pub listener_address: Multiaddr,
    /// The number of dial attempts to make before giving up. Default: 3
    pub max_dial_attempts: usize,
    /// The maximum number of connection tasks that will be spawned at the same time. Once this limit is reached, peers
    /// attempting to connect will have to wait for another connection attempt to complete. Default: 20
    pub max_simultaneous_inbound_connects: usize,
}

impl Default for ConnectionManagerConfig {
    fn default() -> Self {
        Self {
            listener_address: DEFAULT_LISTENER_ADDRESS
                .parse()
                .expect("DEFAULT_LISTENER_ADDRESS is malformed"),
            max_dial_attempts: 3,
            max_simultaneous_inbound_connects: 20,
        }
    }
}

pub struct ConnectionManager<TTransport, TBackoff> {
    executor: runtime::Handle,
    request_rx: Fuse<mpsc::Receiver<ConnectionManagerRequest>>,
    event_rx: Fuse<mpsc::Receiver<ConnectionManagerEvent>>,
    dialer_tx: mpsc::Sender<DialerRequest>,
    dialer: Option<Dialer<TTransport, TBackoff>>,
    listener: Option<PeerListener<TTransport>>,
    peer_manager: AsyncPeerManager,
    active_connections: HashMap<NodeId, PeerConnection>,
    shutdown_signal: Option<ShutdownSignal>,
    protocol_notifier: ProtocolNotifier<yamux::Stream>,
}

impl<TTransport, TBackoff> ConnectionManager<TTransport, TBackoff>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TBackoff: Backoff + Send + Sync + 'static,
{
    pub fn new(
        config: ConnectionManagerConfig,
        executor: runtime::Handle,
        transport: TTransport,
        noise_config: NoiseConfig,
        backoff: Arc<TBackoff>,
        request_rx: mpsc::Receiver<ConnectionManagerRequest>,
        node_identity: Arc<NodeIdentity>,
        peer_manager: AsyncPeerManager,
        protocol_notifier: ProtocolNotifier<yamux::Stream>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_SIZE);

        let (establisher_tx, establisher_rx) = mpsc::channel(ESTABLISHER_CHANNEL_SIZE);

        let supported_protocols = protocol_notifier.get_supported_protocols();

        let listener = PeerListener::new(
            executor.clone(),
            config.clone(),
            transport.clone(),
            noise_config.clone(),
            event_tx.clone(),
            peer_manager.clone(),
            Arc::clone(&node_identity),
            supported_protocols.clone(),
            shutdown_signal.clone(),
        );

        let dialer = Dialer::new(
            executor.clone(),
            config,
            Arc::clone(&node_identity),
            peer_manager.clone(),
            transport,
            noise_config,
            backoff,
            establisher_rx,
            event_tx,
            supported_protocols,
            shutdown_signal.clone(),
        );

        Self {
            executor,
            shutdown_signal: Some(shutdown_signal),
            request_rx: request_rx.fuse(),
            peer_manager,
            protocol_notifier,
            event_rx: event_rx.fuse(),
            dialer_tx: establisher_tx,
            dialer: Some(dialer),
            listener: Some(listener),
            active_connections: Default::default(),
        }
    }

    pub async fn run(mut self) {
        let mut shutdown = self
            .shutdown_signal
            .take()
            .expect("ConnectionManager initialized without a shutdown");

        self.run_listener();
        self.run_dialer();

        debug!(target: LOG_TARGET, "Connection manager started");
        loop {
            futures::select! {
                event = self.event_rx.select_next_some() => {
                    self.handle_event(event).await;
                },

                request = self.request_rx.select_next_some() => {
                    self.handle_request(request).await;
                },

                _ = shutdown => {
                    info!(target: LOG_TARGET, "ConnectionManager is shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
    }

    fn run_listener(&mut self) {
        let listener = self
            .listener
            .take()
            .expect("ConnnectionManager initialized without a Listener");

        self.executor.spawn(listener.run());
    }

    fn run_dialer(&mut self) {
        let dialer = self
            .dialer
            .take()
            .expect("ConnnectionManager initialized without an Establisher");

        self.executor.spawn(dialer.run());
    }

    async fn handle_request(&mut self, request: ConnectionManagerRequest) {
        use ConnectionManagerRequest::*;
        match request {
            DialPeer(node_id, reply_tx) => match self.get_active_connection(&node_id) {
                Some(conn) => {
                    log_if_error_fmt!(
                        target: LOG_TARGET,
                        reply_tx.send(Ok(conn.clone())),
                        "Failed to send reply for dial request for peer '{}'",
                        node_id.short_str()
                    );
                },
                None => self.dial_peer(node_id, reply_tx).await,
            },
        }
    }

    async fn handle_event(&mut self, event: ConnectionManagerEvent) {
        use ConnectionManagerEvent::*;

        match event {
            NewInboundSubstream(_peer_public_key, protocol, stream) => {
                log_if_error!(
                    target: LOG_TARGET,
                    self.protocol_notifier
                        .notify(&protocol, ProtocolEvent::NewSubstream(123, stream))
                        .await,
                    "Error sending NewSubstream notification because '{error}'",
                );
            },
            _ => {},
        }
    }

    fn get_active_connection(&self, node_id: &NodeId) -> Option<&PeerConnection> {
        self.active_connections.get(node_id)
    }

    async fn dial_peer(
        &mut self,
        node_id: NodeId,
        reply_tx: oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>,
    )
    {
        match self.peer_manager.find_by_node_id(&node_id).await {
            Ok(peer) => {
                if let Err(err) = self.dialer_tx.try_send(DialerRequest::Dial(Box::new(peer), reply_tx)) {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send request to establisher because '{}'", err
                    );

                    if let DialerRequest::Dial(_, reply_tx) = err.into_inner() {
                        log_if_error_fmt!(
                            target: LOG_TARGET,
                            reply_tx.send(Err(ConnectionManagerError::EstablisherChannelError)),
                            "Failed to send dial peer result for peer '{}'",
                            node_id.short_str()
                        );
                    }
                }
            },
            Err(err) => {
                error!(target: LOG_TARGET, "Failed to fetch peer to dial because '{}'", err);
                log_if_error_fmt!(
                    level: warn,
                    target: LOG_TARGET,
                    reply_tx.send(Err(ConnectionManagerError::PeerManagerError(err))),
                    "Failed to send error reply when dialing peer '{}'",
                    node_id.short_str()
                );
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        backoff::ConstantBackoff,
        connection_manager::requester::ConnectionManagerRequester,
        noise::NoiseConfig,
        peer_manager::{PeerFeatures, PeerManagerError},
        test_utils::{node_identity::build_node_identity, test_node::build_peer_manager},
        transports::MemoryTransport,
    };
    use std::time::Duration;
    use tari_shutdown::Shutdown;
    use tari_test_utils::unpack_enum;
    use tokio::runtime::Handle;

    #[tokio_macros::test_basic]
    async fn connect_to_nonexistent_peer() {
        let rt_handle = Handle::current();
        let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let noise_config = NoiseConfig::new(node_identity.clone());
        let (request_tx, request_rx) = mpsc::channel(1);
        let mut requester = ConnectionManagerRequester::new(request_tx);
        let mut shutdown = Shutdown::new();

        let peer_manager = build_peer_manager();

        let connection_manager = ConnectionManager::new(
            Default::default(),
            rt_handle.clone(),
            MemoryTransport,
            noise_config,
            Arc::new(ConstantBackoff::new(Duration::from_secs(1))),
            request_rx,
            node_identity,
            peer_manager.into(),
            ProtocolNotifier::new(),
            shutdown.to_signal(),
        );

        rt_handle.spawn(connection_manager.run());

        let result = requester.dial_peer(NodeId::default()).await;
        unpack_enum!(Result::Err(err) = result);
        match err {
            ConnectionManagerError::PeerManagerError(PeerManagerError::PeerNotFoundError) => {},
            _ => panic!(
                "Unexpected error. Expected \
                 `ConnectionManagerError::PeerManagerError(PeerManagerError::PeerNotFoundError)`"
            ),
        }

        shutdown.trigger().unwrap();
    }
}
