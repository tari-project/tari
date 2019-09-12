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
    outbound_message_service::{
        error::OutboundServiceError,
        messages::OutboundMessage,
        service::OutboundServiceConfig,
    },
    peer_manager::NodeId,
};
use futures::{
    channel::oneshot,
    future::Fuse,
    stream::{FusedStream, FuturesUnordered},
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
use tokio::timer;

const LOG_TARGET: &'static str = "comms::outbound_message_service::worker";

/// The state of the dial request
pub struct DialState {
    /// Number of dial attempts
    attempts: usize,
    /// The node id being dialed
    node_id: NodeId,
    /// Cancel signal
    cancel_rx: Option<Fuse<oneshot::Receiver<()>>>,
}

impl DialState {
    /// Create a new DialState for the given NodeId
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            attempts: 0,
            cancel_rx: None,
        }
    }

    /// Set the cancel receiver for this DialState
    pub fn set_cancel_receiver(&mut self, cancel_rx: Fuse<oneshot::Receiver<()>>) -> &mut Self {
        self.cancel_rx = Some(cancel_rx);
        self
    }

    /// Take ownership of the cancel receiver if this DialState has ownership of one
    pub fn take_cancel_receiver(&mut self) -> Option<Fuse<oneshot::Receiver<()>>> {
        self.cancel_rx.take()
    }

    /// Increment the number of attempts
    pub fn inc_attempts(&mut self) -> &mut Self {
        self.attempts += 1;
        self
    }

    /// Calculates the time from now that this dial attempt should be retried.
    pub fn backoff_duration(&mut self) -> Duration {
        Duration::from_millis(self.exponential_backoff_offset())
    }

    /// Calculates the offset in seconds based on `self.attempts`.
    fn exponential_backoff_offset(&self) -> u64 {
        if self.attempts == 0 {
            return 0;
        }
        let secs = 0.5 * (f32::powf(2.0, self.attempts as f32) - 1.0);
        cmp::max(2, secs.ceil() as u64)
    }
}

/// Responsible for dialing peers and sending queued messages
pub struct OutboundMessageWorker<TMsgStream> {
    config: OutboundServiceConfig,
    connection_manager: ConnectionManagerRequester,
    incoming_message_stream: TMsgStream,
    pending_connect_requests: HashMap<NodeId, Vec<OutboundMessage>>,
    shutdown_rx: Option<oneshot::Receiver<()>>,
    dial_cancel_signals: HashMap<NodeId, oneshot::Sender<()>>,
}

impl<TMsgStream> OutboundMessageWorker<TMsgStream>
where TMsgStream: Stream<Item = Vec<OutboundMessage>> + FusedStream + Unpin
{
    pub fn new(
        config: OutboundServiceConfig,
        incoming_message_stream: TMsgStream,
        connection_manager: ConnectionManagerRequester,
        shutdown_rx: oneshot::Receiver<()>,
    ) -> Self
    {
        Self {
            config,
            connection_manager,
            incoming_message_stream,
            pending_connect_requests: HashMap::new(),
            shutdown_rx: Some(shutdown_rx),
            dial_cancel_signals: HashMap::new(),
        }
    }

    pub async fn start(mut self) {
        let mut pending_connects = FuturesUnordered::new();
        let mut shutdown_rx = self
            .shutdown_rx
            .take()
            .expect("OutboundMessageActor initialized without shutdown_rx")
            .fuse();
        loop {
            futures::select! {
                new_messages = self.incoming_message_stream.select_next_some() => {
                    let pending_connect_states = self.enqueue_new_messages(new_messages)
                        .into_iter()
                        // Wrap node ids to connect in a DialState
                        .map(DialState::new);

                    for mut state in pending_connect_states {
                        let (cancel_tx, cancel_rx) = oneshot::channel();
                        self.dial_cancel_signals.insert(state.node_id.clone(), cancel_tx);
                        state.set_cancel_receiver(cancel_rx.fuse());
                        pending_connects.push(
                            Self::connect_to(self.connection_manager.clone(), state)
                        );
                    }
                },

                maybe_result = pending_connects.select_next_some() => {
                    // maybe_result could be None if the connection attempt was canceled
                    if let Some((state, result)) = maybe_result {
                        if let Some(mut state) = self.handle_connect_result(state, result) {
                            if state.attempts >= self.config.max_attempts {
                                warn!(target: LOG_TARGET, "Failed to connect to NodeId={}", state.node_id);
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

                _ = shutdown_rx => {
                    info!(target: LOG_TARGET, "Outbound message service received shutdown signal.");
                    self.cancel_connection_attempts();
                    break;
                },

                complete => break,
            }
        }
    }

    fn cancel_connection_attempts(&mut self) {
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
        let mut delay = timer::delay(Instant::now() + offset).fuse();
        futures::select! {
            _ = delay => {
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

    fn enqueue_new_messages(&mut self, batch: Vec<OutboundMessage>) -> Vec<NodeId> {
        let mut pending_node_ids = Vec::new();
        for msg in batch {
            match self.pending_connect_requests.get_mut(msg.destination_node_id()) {
                // Connection being attempted for peer. Add the message to the queue to be sent once connected.
                Some(msgs) => msgs.push(msg),

                // No connection currently being attempted for this peer.
                None => {
                    let node_id = msg.destination_node_id().clone();
                    self.pending_connect_requests.insert(node_id.clone(), vec![msg]);
                    pending_node_ids.push(node_id);
                },
            }
        }

        pending_node_ids
    }

    fn handle_connect_result(
        &mut self,
        state: DialState,
        connect_result: Result<Arc<PeerConnection>, OutboundServiceError>,
    ) -> Option<DialState>
    {
        match connect_result {
            Ok(conn) => {
                if let Err(err) = self.handle_new_connection(&state.node_id, conn) {
                    error!(
                        target: LOG_TARGET,
                        "Error when sending messages for new connection: {:?}", err
                    );
                }
                None
            },
            Err(err) => {
                error!(target: LOG_TARGET, "Failed to connect to node: {}", err);
                Some(state)
            },
        }
    }

    fn handle_new_connection(
        &mut self,
        node_id: &NodeId,
        conn: Arc<PeerConnection>,
    ) -> Result<(), OutboundServiceError>
    {
        self.dial_cancel_signals.remove(node_id);
        match self.pending_connect_requests.remove(node_id) {
            Some(messages) => {
                for message in messages {
                    conn.send(message.message_frames().clone())
                        .map_err(OutboundServiceError::ConnectionError)?;
                }
            },
            None => {
                // This should never happen
                warn!(
                    target: LOG_TARGET,
                    "No messages to send for new connection to NodeId {}", node_id
                );
            },
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::peer_connection,
        connection_manager::actor::ConnectionManagerRequest,
        test_utils::node_id,
    };
    use futures::{channel::mpsc, SinkExt};
    use tokio::runtime::Runtime;

    #[test]
    fn multiple_send_in_batch() {
        // Tests sending 3 messages to 2 recipients simultaneously.
        // This checks that messages from separate requests are batched and a single dial request per
        // peer is made.
        let rt = Runtime::new().unwrap();
        let (mut new_message_tx, new_message_rx) = mpsc::unbounded();

        let (conn_man_tx, mut conn_man_rx) = mpsc::channel(1);
        let conn_manager = ConnectionManagerRequester::new(conn_man_tx);

        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let service = OutboundMessageWorker::new(
            OutboundServiceConfig::default(),
            new_message_rx,
            conn_manager,
            shutdown_rx,
        );
        rt.spawn(service.start());

        let node_id1 = node_id::random();
        let node_id2 = node_id::random();

        // Send a batch of messages
        let messages = vec![
            OutboundMessage::new(node_id1.clone(), vec![b"A".to_vec()]),
            OutboundMessage::new(node_id2.clone(), vec![b"B".to_vec()]),
        ];
        rt.block_on(new_message_tx.send(messages)).unwrap();
        // Another message for node_id1
        let messages = vec![OutboundMessage::new(node_id1.clone(), vec![b"C".to_vec()])];
        rt.block_on(new_message_tx.send(messages)).unwrap();

        // There should be 2 connection requests.
        let conn_man_req1 = rt.block_on(conn_man_rx.next()).unwrap();
        // This should be followed by the next connection message
        let conn_man_req2 = rt.block_on(conn_man_rx.next()).unwrap();
        // Then, the stream should be empty (try_next errors on Poll::Pending)
        assert!(conn_man_rx.try_next().is_err());

        // Check that the dial request for node_id1 is made and, when a peer connection is passed back,
        // that peer connection is used to send the correct messages
        match conn_man_req1 {
            ConnectionManagerRequest::DialPeer(boxed) => {
                let (node_id, reply_tx) = *boxed;
                assert_eq!(node_id, node_id1);
                let (conn, rx) = PeerConnection::new_with_connecting_state_for_test();
                assert!(reply_tx.send(Ok(Arc::new(conn))).is_ok());
                // Check that pending messages are sent
                let msg = rx.recv_timeout(Duration::from_millis(100)).unwrap();
                match msg {
                    peer_connection::ControlMessage::SendMsg(frames) => {
                        assert_eq!(&frames[0], b"A");
                    },
                    _ => panic!(),
                }
                let msg = rx.recv_timeout(Duration::from_millis(100)).unwrap();
                match msg {
                    peer_connection::ControlMessage::SendMsg(frames) => {
                        assert_eq!(&frames[0], b"C");
                    },
                    _ => panic!(),
                }
            },
        }

        // Check that the dial request for node_id2 is made
        match conn_man_req2 {
            ConnectionManagerRequest::DialPeer(boxed) => {
                let (node_id, _) = *boxed;
                assert_eq!(node_id, node_id2);
            },
        }

        // Abort pending connections and shutdown the service
        shutdown_tx.send(()).unwrap();
        rt.shutdown_on_idle();
    }

    #[test]
    fn exponential_backoff_calc() {
        let mut state = DialState::new(NodeId::new());
        state.attempts = 0;
        assert_eq!(state.exponential_backoff_offset(), 0);
        state.attempts = 1;
        assert_eq!(state.exponential_backoff_offset(), 2);
        state.attempts = 2;
        assert_eq!(state.exponential_backoff_offset(), 2);
        state.attempts = 3;
        assert_eq!(state.exponential_backoff_offset(), 4);
        state.attempts = 4;
        assert_eq!(state.exponential_backoff_offset(), 8);
        state.attempts = 5;
        assert_eq!(state.exponential_backoff_offset(), 16);
        state.attempts = 6;
        assert_eq!(state.exponential_backoff_offset(), 32);
        state.attempts = 7;
        assert_eq!(state.exponential_backoff_offset(), 64);
        state.attempts = 8;
        assert_eq!(state.exponential_backoff_offset(), 128);
        state.attempts = 9;
        assert_eq!(state.exponential_backoff_offset(), 256);
        state.attempts = 10;
        assert_eq!(state.exponential_backoff_offset(), 512);
    }
}
