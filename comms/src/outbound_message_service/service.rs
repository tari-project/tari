// Copyright 2019 The Tari Project
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

use crate::{
    connection::PeerConnection,
    connection_manager::ConnectionManagerRequester,
    message::{Envelope, MessageExt},
    outbound_message_service::{error::OutboundServiceError, messages::OutboundMessage, OutboundServiceConfig},
    peer_manager::{NodeId, NodeIdentity},
};
use futures::{
    channel::oneshot,
    future,
    stream::{self, FuturesUnordered},
    FutureExt,
    Stream,
    StreamExt,
};
use log::*;
use std::{
    cmp,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_shutdown::ShutdownSignal;
use tokio::timer;

const LOG_TARGET: &str = "comms::outbound_message_service::worker";

/// The state of the dial request
pub struct DialState {
    /// Number of dial attempts
    attempts: usize,
    /// The node id being dialed
    node_id: NodeId,
    /// Cancel signal
    cancel_rx: Option<future::Fuse<oneshot::Receiver<()>>>,
}

impl DialState {
    /// Create a new DialState for the given NodeId
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            attempts: 1,
            cancel_rx: None,
        }
    }

    /// Set the cancel receiver for this DialState
    pub fn set_cancel_receiver(&mut self, cancel_rx: future::Fuse<oneshot::Receiver<()>>) -> &mut Self {
        self.cancel_rx = Some(cancel_rx);
        self
    }

    /// Take ownership of the cancel receiver if this DialState has ownership of one
    pub fn take_cancel_receiver(&mut self) -> Option<future::Fuse<oneshot::Receiver<()>>> {
        self.cancel_rx.take()
    }

    /// Increment the number of attempts
    pub fn inc_attempts(&mut self) -> &mut Self {
        self.attempts += 1;
        self
    }

    /// Calculates the time from now that this dial attempt should be retried.
    pub fn backoff_duration(&mut self) -> Duration {
        Duration::from_secs(self.exponential_backoff_offset())
    }

    /// Calculates the offset in seconds based on `self.attempts`.
    fn exponential_backoff_offset(&self) -> u64 {
        if self.attempts <= 1 {
            return 0;
        }
        let secs = 0.8 * (f32::powf(2.0, self.attempts as f32) - 1.0);
        cmp::max(2, secs.ceil() as u64)
    }
}

/// Responsible for dialing peers and sending queued messages
pub struct OutboundMessageService<TMsgStream> {
    config: OutboundServiceConfig,
    connection_manager: ConnectionManagerRequester,
    dial_cancel_signals: HashMap<NodeId, oneshot::Sender<()>>,
    message_stream: stream::Fuse<TMsgStream>,
    node_identity: Arc<NodeIdentity>,
    pending_connect_requests: HashMap<NodeId, Vec<OutboundMessage>>,
    active_connections: HashMap<NodeId, Arc<PeerConnection>>,
    shutdown_signal: Option<ShutdownSignal>,
}

impl<TMsgStream> OutboundMessageService<TMsgStream>
where TMsgStream: Stream<Item = OutboundMessage> + Unpin
{
    pub fn new(
        config: OutboundServiceConfig,
        message_stream: TMsgStream,
        node_identity: Arc<NodeIdentity>,
        connection_manager: ConnectionManagerRequester,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            active_connections: HashMap::with_capacity(config.max_cached_connections),
            config,
            connection_manager,
            node_identity,
            message_stream: message_stream.fuse(),
            pending_connect_requests: HashMap::new(),
            dial_cancel_signals: HashMap::new(),
            shutdown_signal: Some(shutdown_signal),
        }
    }

    pub async fn start(mut self) {
        let mut pending_connects = FuturesUnordered::new();
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("OutboundMessageService initialized without shutdown_rx")
            .fuse();
        loop {
            futures::select! {
                new_message = self.message_stream.select_next_some() => {
                    if let Some(conn) = self.get_active_connection(&new_message.peer_node_id) {
                        debug!(
                            target: LOG_TARGET,
                            "Cached connection found for NodeId={}",
                            new_message.peer_node_id
                        );
                        if let Err(err) = self.send_message(&conn, new_message).await {
                            warn!(
                                target: LOG_TARGET,
                                "Message failed to send from existing active connection",
                            );
                            // TODO: Enqueue message for resending
                        }
                    } else {
                        if let Some(mut dial_state) = self.enqueue_new_message(new_message) {
                            let (cancel_tx, cancel_rx) = oneshot::channel();
                            self.dial_cancel_signals.insert(dial_state.node_id.clone(), cancel_tx);
                            dial_state.set_cancel_receiver(cancel_rx.fuse());
                            pending_connects.push(
                                Self::connect_to(self.connection_manager.clone(), dial_state)
                            );
                        }
                    }
                },

                maybe_result = pending_connects.select_next_some() => {
                    // maybe_result could be None if the connection attempt was canceled
                    if let Some((state, result)) = maybe_result {
                        if let Some(mut state) = self.handle_connect_result(state, result).await {
                            debug!(
                                target: LOG_TARGET,
                                "[Attempt {} of {}] Failed to connect to NodeId={}",
                                state.attempts,
                                self.config.max_attempts,
                                state.node_id
                            );
                            if state.attempts >= self.config.max_attempts {
                                log_if_error!(
                                    target: LOG_TARGET,
                                    "Unable to set last connection failed because '{}'",
                                    self.connection_manager.set_last_connection_failed(state.node_id.clone()).await
                                );
                                self.dial_cancel_signals.remove(&state.node_id);
                                self.pending_connect_requests.remove(&state.node_id);
                                debug!(target: LOG_TARGET, "NodeId={} Maximum attempts reached. Discarding messages.", state.node_id);
                            } else {
                                // Should retry this connection attempt
                                state.inc_attempts();
                                pending_connects.push(
                                    Self::connect_to(self.connection_manager.clone(), state)
                                );
                            }
                        }
                    }
                },

                _guard = shutdown_signal => {
                    info!(target: LOG_TARGET, "Outbound message service shutting because the shutdown signal was received.");
                    self.cancel_pending_connection_attempts();
                    break;
                }

                complete => {
                    info!(target: LOG_TARGET, "Outbound message service shutting because the message stream ended.");
                    self.cancel_pending_connection_attempts();
                    break;
                }
            }
        }
    }

    fn get_active_connection(&mut self, node_id: &NodeId) -> Option<Arc<PeerConnection>> {
        match self.active_connections.get(node_id) {
            Some(conn) => {
                if conn.is_active() {
                    Some(Arc::clone(&conn))
                } else {
                    // Side effect: remove the inactive connection
                    self.active_connections.remove(node_id);
                    None
                }
            },
            None => None,
        }
    }

    fn cancel_pending_connection_attempts(&mut self) {
        for (_, cancel_tx) in self.dial_cancel_signals.drain() {
            let _ = cancel_tx.send(());
        }
    }

    async fn connect_to(
        mut connection_manager: ConnectionManagerRequester,
        mut state: DialState,
    ) -> Option<(DialState, Result<Arc<PeerConnection>, OutboundServiceError>)>
    {
        let mut cancel_rx = state
            .take_cancel_receiver()
            .expect("It is incorrect to attempt to connect without setting a cancel receiver");

        let offset = state.backoff_duration();
        debug!(
            target: LOG_TARGET,
            "[Attempt {}] Attempting to send message in {} second(s)",
            state.attempts,
            offset.as_secs()
        );
        let mut delay = timer::delay(Instant::now() + offset).fuse();
        futures::select! {
            _ = delay => {
                debug!(target: LOG_TARGET, "Retry delay expired. Attempting to connect...");
                let result = connection_manager
                    .dial_node(state.node_id.clone())
                    .await
                    .map_err(Into::into);
                // Put the cancel receiver back
                state.set_cancel_receiver(cancel_rx);
                Some((state, result))
            },
            _ = cancel_rx => None,
        }
    }

    /// Returns a DialState for the NodeId if a connection is not currently being attempted
    fn enqueue_new_message(&mut self, msg: OutboundMessage) -> Option<DialState> {
        trace!(
            target: LOG_TARGET,
            "{} attempt(s) in progress",
            self.pending_connect_requests.len()
        );
        match self.pending_connect_requests.get_mut(&msg.peer_node_id) {
            // Connection being attempted for peer. Add the message to the queue to be sent once connected.
            Some(msgs) => {
                debug!(
                    target: LOG_TARGET,
                    "Connection attempt already in progress for NodeId={}. {} messages waiting to be sent. ",
                    msg.peer_node_id,
                    msgs.len() + 1,
                );
                msgs.push(msg);
                None
            },

            // No connection currently being attempted for this peer.
            None => {
                let node_id = msg.peer_node_id.clone();
                debug!(
                    target: LOG_TARGET,
                    "New connection attempt required for NodeId={}.", node_id
                );
                self.pending_connect_requests.insert(node_id.clone(), vec![msg]);
                Some(DialState::new(node_id))
            },
        }
    }

    /// Handle the connection result. Returns Some(DialState) of the connection attempt failed,
    /// otherwise None is returned
    async fn handle_connect_result(
        &mut self,
        state: DialState,
        connect_result: Result<Arc<PeerConnection>, OutboundServiceError>,
    ) -> Option<DialState>
    {
        match connect_result {
            Ok(conn) => {
                log_if_error!(
                    target: LOG_TARGET,
                    "Unable to set last connection success because '{}'",
                    self.connection_manager
                        .set_last_connection_succeeded(state.node_id.clone())
                        .await
                );
                if let Err(err) = self.handle_new_connection(&state.node_id, conn).await {
                    error!(
                        target: LOG_TARGET,
                        "Error when sending messages for new connection: {:?}", err
                    );
                }
                None
            },
            Err(err) => {
                error!(target: LOG_TARGET, "Failed to connect to node: {:?}", err);
                Some(state)
            },
        }
    }

    async fn handle_new_connection(
        &mut self,
        node_id: &NodeId,
        conn: Arc<PeerConnection>,
    ) -> Result<(), OutboundServiceError>
    {
        self.dial_cancel_signals.remove(&node_id);
        self.cache_connection(node_id.clone(), Arc::clone(&conn));
        match self.pending_connect_requests.remove(node_id) {
            Some(messages) => {
                for message in messages {
                    // TODO: Error here will mean messages are discarded
                    self.send_message(&conn, message).await?;
                }
            },
            None => {
                // Shouldn't happen: no pending messages to send but we've connected to a peer?
                warn!(
                    target: LOG_TARGET,
                    "No messages to send for new connection to NodeId {}", node_id
                );
            },
        }

        Ok(())
    }

    fn cache_connection(&mut self, node_id: NodeId, conn: Arc<PeerConnection>) {
        if self.active_connections.len() + 1 > self.active_connections.capacity() {
            // Clear dead connections
            self.clear_dead_connections();
            // Still at capacity?
            if self.active_connections.len() + 1 > self.active_connections.capacity() {
                // Remove the "first" (oldest?) peer connection.
                if let Some(last) = self.active_connections.keys().next().cloned() {
                    trace!(
                        target: LOG_TARGET,
                        "Dropping recent active connection for NodeId {}",
                        last
                    );
                    self.active_connections.remove(&last);
                }
            }
        }
        self.active_connections.insert(node_id, conn);
        trace!(
            target: LOG_TARGET,
            "Recent active connection cache size: {} of {}",
            self.active_connections.len(),
            self.active_connections.capacity()
        );
    }

    fn clear_dead_connections(&mut self) {
        let mut new_hm = HashMap::with_capacity(self.active_connections.capacity());
        for (node_id, conn) in self.active_connections.drain().filter(|(_, conn)| conn.is_active()) {
            new_hm.insert(node_id, conn);
        }
        self.active_connections = new_hm;
    }

    async fn send_message(&self, conn: &PeerConnection, message: OutboundMessage) -> Result<(), OutboundServiceError> {
        let OutboundMessage { flags, body, .. } = message;
        let envelope = Envelope::construct_signed(
            self.node_identity.secret_key(),
            self.node_identity.public_key(),
            body,
            flags,
        )?;
        let frame = envelope.to_encoded_bytes()?;

        conn.send(vec![frame]).map_err(OutboundServiceError::ConnectionError)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::peer_connection,
        connection_manager::actor::ConnectionManagerRequest,
        message::MessageFlags,
        peer_manager::PeerFeatures,
        test_utils::node_id,
    };
    use futures::{channel::mpsc, stream, SinkExt};
    use prost::Message;
    use tari_shutdown::Shutdown;
    use tari_test_utils::collect_stream;
    use tokio::runtime::Runtime;

    #[test]
    fn multiple_send_in_batch() {
        // Tests sending a number of messages to 2 recipients simultaneously.
        // This checks that messages from separate requests are batched and a single dial request per
        // peer is made.
        let rt = Runtime::new().unwrap();
        let (mut new_message_tx, new_message_rx) = mpsc::unbounded();

        let (conn_man_tx, mut conn_man_rx) = mpsc::channel(2);
        let conn_manager = ConnectionManagerRequester::new(conn_man_tx);

        let node_identity = Arc::new(NodeIdentity::random_for_test(None, PeerFeatures::empty()));
        let mut shutdown = Shutdown::new();

        let service = OutboundMessageService::new(
            OutboundServiceConfig::default(),
            new_message_rx,
            node_identity,
            conn_manager,
            shutdown.to_signal(),
        );
        rt.spawn(service.start());

        let node_id1 = node_id::random();
        let node_id2 = node_id::random();

        // Send a batch of messages
        let mut messages = stream::iter(
            vec![
                (node_id1.clone(), b"A".to_vec()),
                (node_id2.clone(), b"B".to_vec()),
                (node_id1.clone(), b"C".to_vec()),
                (node_id1.clone(), b"D".to_vec()),
            ]
            .into_iter()
            .map(|(node_id, msg)| OutboundMessage::new(node_id, MessageFlags::empty(), msg)),
        );

        rt.block_on(new_message_tx.send_all(&mut messages)).unwrap();

        // There should be 2 connection requests.
        let conn_man_req1 = rt.block_on(conn_man_rx.next()).unwrap();
        let conn_man_req2 = rt.block_on(conn_man_rx.next()).unwrap();
        // Then, the stream should be empty (try_next errors on Poll::Pending)
        assert!(conn_man_rx.try_next().is_err());

        let (conn, conn_rx) = PeerConnection::new_with_connecting_state_for_test();
        let conn = Arc::new(conn);

        // Check that the dial request for node_id1 is made and, when a peer connection is passed back,
        // that peer connection is used to send the correct messages. They can happen in any order.
        vec![conn_man_req1, conn_man_req2].into_iter().for_each(|conn_man_req| {
            let conn = conn.clone();
            match conn_man_req {
                ConnectionManagerRequest::DialPeer(boxed) => {
                    let (node_id, reply_tx) = *boxed;
                    match node_id {
                        _ if node_id == node_id1 => {
                            assert!(reply_tx.send(Ok(conn)).is_ok());
                            // Check that pending messages are sent
                            let msg = conn_rx.recv_timeout(Duration::from_millis(100)).unwrap();
                            assert_send_msg(msg, b"A");

                            let msg = conn_rx.recv_timeout(Duration::from_millis(100)).unwrap();
                            assert_send_msg(msg, b"C");

                            let msg = conn_rx.recv_timeout(Duration::from_millis(100)).unwrap();
                            assert_send_msg(msg, b"D");
                        },
                        _ if node_id == node_id2 => {
                            let (conn2, rx) = PeerConnection::new_with_connecting_state_for_test();
                            assert!(reply_tx.send(Ok(Arc::new(conn2))).is_ok());
                            let msg = rx.recv_timeout(Duration::from_millis(100)).unwrap();
                            assert_send_msg(msg, b"B");
                        },
                        _ => panic!("unexpected node id in connection manager request"),
                    }
                },
                _ => panic!("unexpected connection manager request"),
            }
        });

        rt.block_on(new_message_tx.send(OutboundMessage::new(
            node_id1.clone(),
            MessageFlags::empty(),
            b"E".to_vec(),
        )))
        .unwrap();

        let msg = conn_rx.recv_timeout(Duration::from_millis(100)).unwrap();
        assert_send_msg(msg, b"E");

        // Connection should be reused, so connection manager should not receive another request to connect.
        let requests = collect_stream!(rt, conn_man_rx, take = 2, timeout = Duration::from_secs(3));
        assert!(requests.iter().all(|req| {
            match req {
                ConnectionManagerRequest::DialPeer(_) => false,
                _ => true,
            }
        }));
        assert!(requests.iter().any(|req| {
            match req {
                ConnectionManagerRequest::SetLastConnectionSucceeded(_) => true,
                _ => false,
            }
        }));

        // Abort pending connections and shutdown the service
        shutdown.trigger().unwrap();
        rt.shutdown_on_idle();
    }

    fn assert_send_msg(control_msg: peer_connection::ControlMessage, msg: &[u8]) {
        match control_msg {
            peer_connection::ControlMessage::SendMsg(mut frames) => {
                let envelope = Envelope::decode(frames.remove(0)).unwrap();
                assert_eq!(envelope.body.as_slice(), msg);
            },
            _ => panic!(),
        }
    }

    #[test]
    fn exponential_backoff_calc() {
        let mut state = DialState::new(NodeId::new());
        state.attempts = 0;
        assert_eq!(state.exponential_backoff_offset(), 0);
        state.attempts = 1;
        assert_eq!(state.exponential_backoff_offset(), 0);
        state.attempts = 2;
        assert_eq!(state.exponential_backoff_offset(), 3);
        state.attempts = 3;
        assert_eq!(state.exponential_backoff_offset(), 6);
        state.attempts = 4;
        assert_eq!(state.exponential_backoff_offset(), 12);
        state.attempts = 5;
        assert_eq!(state.exponential_backoff_offset(), 25);
        state.attempts = 6;
        assert_eq!(state.exponential_backoff_offset(), 51);
        state.attempts = 7;
        assert_eq!(state.exponential_backoff_offset(), 102);
        state.attempts = 8;
        assert_eq!(state.exponential_backoff_offset(), 204);
        state.attempts = 9;
        assert_eq!(state.exponential_backoff_offset(), 409);
        state.attempts = 10;
        assert_eq!(state.exponential_backoff_offset(), 819);
    }
}
