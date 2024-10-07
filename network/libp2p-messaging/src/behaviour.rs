//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    cmp,
    collections::{HashMap, VecDeque},
    task::{Context, Poll},
};

use libp2p::{
    core::{transport::PortUse, Endpoint},
    swarm::{
        dial_opts::DialOpts,
        AddressChange,
        ConnectionClosed,
        ConnectionDenied,
        ConnectionHandler,
        ConnectionId,
        DialFailure,
        FromSwarm,
        NetworkBehaviour,
        NotifyHandler,
        THandler,
        THandlerInEvent,
        THandlerOutEvent,
        ToSwarm,
    },
    Multiaddr,
    PeerId,
    StreamProtocol,
};
use smallvec::SmallVec;

use crate::{
    codec::Codec,
    error::Error,
    event::Event,
    handler::Handler,
    stream,
    stream::{MessageSink, MessageStream, StreamId},
    Config,
};

/// Internal threshold for when to shrink the capacity
/// of empty queues. If the capacity of an empty queue
/// exceeds this threshold, the associated memory is
/// released.
pub const EMPTY_QUEUE_SHRINK_THRESHOLD: usize = 100;

type MessageSinkAndStream<TMsg> = (MessageSink<TMsg>, MessageStream<TMsg>);

#[derive(Debug)]
pub struct Behaviour<TCodec>
where TCodec: Codec + Send + Clone + 'static
{
    protocol: StreamProtocol,
    config: Config,
    pending_events: VecDeque<ToSwarm<Event<TCodec::Message>, THandlerInEvent<Self>>>,
    /// The currently connected peers, their pending outbound and inbound responses and their known,
    /// reachable addresses, if any.
    connected: HashMap<PeerId, Connections<TCodec::Message>>,
    pending_outbound_dials: HashMap<PeerId, MessageSinkAndStream<TCodec::Message>>,
    next_outbound_stream_id: StreamId,
}

impl<TCodec> Behaviour<TCodec>
where TCodec: Codec + Send + Clone + 'static
{
    pub fn new(protocol: StreamProtocol, config: Config) -> Self {
        Self {
            protocol,
            config,
            pending_events: VecDeque::new(),
            connected: HashMap::new(),
            next_outbound_stream_id: StreamId::default(),
            pending_outbound_dials: HashMap::default(),
        }
    }

    pub fn send_message(&mut self, peer_id: PeerId, message: TCodec::Message) -> Result<(), Error> {
        self.obtain_message_channel(peer_id).send(message)?;
        Ok(())
    }

    pub fn obtain_message_channel(&mut self, peer_id: PeerId) -> MessageSink<TCodec::Message> {
        let stream_id = self.next_outbound_stream_id;

        self.clear_closed_connections();
        match self.get_connections(&peer_id) {
            Some(connections) => {
                // Return a currently active stream
                if let Some(sink) = connections.next_active_sink() {
                    tracing::debug!("return a currently active stream {}", sink.stream_id());
                    return sink.clone();
                }

                // Otherwise, return a pending stream
                if let Some(sink) = connections.next_pending_sink() {
                    tracing::debug!("return a pending stream {}", sink.stream_id());
                    return sink.clone();
                }

                // Otherwise, create a new stream
                let (sink, stream) = stream::channel(stream_id, peer_id);
                let ix = (stream_id as usize) % connections.connections.len();
                let conn_mut = &mut connections.connections[ix];
                conn_mut.stream_id = Some(stream_id);
                assert!(conn_mut.pending_sink.is_none());
                assert!(conn_mut.message_sink.is_none());
                conn_mut.pending_sink = Some(sink.clone());

                let conn_id = conn_mut.id;
                tracing::debug!("create a new stream {peer_id} {stream_id}");
                self.pending_events.push_back(ToSwarm::NotifyHandler {
                    peer_id,
                    handler: NotifyHandler::One(conn_id),
                    event: stream,
                });

                // Can't use next_outbound_stream_id() above because of multiple mutable borrows
                self.next_outbound_stream_id();

                sink
            },
            None => match self.pending_outbound_dials.get(&peer_id) {
                Some((sink, _)) => {
                    tracing::debug!("return a pending outbound dial {}", sink.stream_id());
                    sink.clone()
                },
                None => {
                    let stream_id = self.next_outbound_stream_id();
                    tracing::debug!("create a new outbound dial {stream_id}");
                    let (sink, stream) = stream::channel(stream_id, peer_id);

                    self.pending_events.push_back(ToSwarm::Dial {
                        opts: DialOpts::peer_id(peer_id).build(),
                    });

                    self.pending_outbound_dials.insert(peer_id, (sink.clone(), stream));
                    sink
                },
            },
        }
    }

    fn clear_closed_connections(&mut self) {
        for connections in self.connected.values_mut() {
            connections.clear_closed_connections();
        }
        self.connected.retain(|_, connections| !connections.is_empty());

        // Shrink the capacity of empty queues if they exceed the threshold.
        if self.connected.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
            self.connected.shrink_to_fit();
        }
    }

    fn next_outbound_stream_id(&mut self) -> StreamId {
        let stream_id = self.next_outbound_stream_id;
        self.next_outbound_stream_id = self.next_outbound_stream_id.wrapping_add(1);
        stream_id
    }

    fn on_connection_closed(
        &mut self,
        ConnectionClosed {
            peer_id,
            connection_id,
            remaining_established,
            ..
        }: ConnectionClosed,
    ) {
        let connections = self
            .connected
            .get_mut(&peer_id)
            .expect("Expected some established connection to peer before closing.");

        let connection = connections
            .connections
            .iter()
            .position(|c| c.id == connection_id)
            .map(|p: usize| connections.connections.remove(p))
            .expect("Expected connection to be established before closing.");

        debug_assert_eq!(connections.is_empty(), remaining_established == 0);
        if connections.is_empty() {
            self.connected.remove(&peer_id);
        }

        if let Some(sink) = connection.pending_sink {
            self.pending_events
                .push_back(ToSwarm::GenerateEvent(Event::InboundFailure {
                    peer_id,
                    stream_id: sink.stream_id(),
                    error: Error::ConnectionClosed,
                }));
        }
    }

    fn on_address_change(&mut self, address_change: AddressChange) {
        let AddressChange {
            peer_id,
            connection_id,
            new,
            ..
        } = address_change;
        if let Some(connections) = self.connected.get_mut(&peer_id) {
            for connection in &mut connections.connections {
                if connection.id == connection_id {
                    connection.remote_address = Some(new.get_remote_address().clone());
                    return;
                }
            }
        }
    }

    fn on_dial_failure(&mut self, DialFailure { peer_id, .. }: DialFailure) {
        if let Some(peer) = peer_id {
            // If there are pending outgoing messages when a dial failure occurs,
            // it is implied that we are not connected to the peer, since pending
            // outgoing messages are drained when a connection is established and
            // only created when a peer is not connected when a request is made.
            // Thus these requests must be considered failed, even if there is
            // another, concurrent dialing attempt ongoing.
            if let Some((_sink, stream)) = self.pending_outbound_dials.remove(&peer) {
                self.pending_events
                    .push_back(ToSwarm::GenerateEvent(Event::OutboundFailure {
                        peer_id: peer,
                        stream_id: stream.stream_id(),
                        error: Error::DialFailure,
                    }));
            }
        }
    }

    fn on_connection_established(
        &mut self,
        handler: &mut Handler<TCodec>,
        peer_id: PeerId,
        connection_id: ConnectionId,
        remote_address: Option<Multiaddr>,
    ) {
        let mut connection = Connection::new(connection_id, remote_address);

        if let Some((sink, stream)) = self.pending_outbound_dials.remove(&peer_id) {
            connection.stream_id = Some(stream.stream_id());
            connection.pending_sink = Some(sink);
            handler.on_behaviour_event(stream);
        }

        self.connected.entry(peer_id).or_default().push(connection);
    }

    fn get_connections(&mut self, peer_id: &PeerId) -> Option<&mut Connections<TCodec::Message>> {
        self.connected.get_mut(peer_id).filter(|c| !c.is_empty())
    }
}

impl<TCodec> NetworkBehaviour for Behaviour<TCodec>
where TCodec: Codec + Send + Clone + 'static
{
    type ConnectionHandler = Handler<TCodec>;
    type ToSwarm = Event<TCodec::Message>;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let mut handler = Handler::<TCodec>::new(peer, self.protocol.clone(), &self.config);
        self.on_connection_established(&mut handler, peer, connection_id, Some(remote_addr.clone()));

        Ok(handler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        remote_addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let mut handler = Handler::new(peer, self.protocol.clone(), &self.config);
        self.on_connection_established(&mut handler, peer, connection_id, Some(remote_addr.clone()));
        Ok(handler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(_) => {},
            FromSwarm::ConnectionClosed(connection_closed) => self.on_connection_closed(connection_closed),
            FromSwarm::AddressChange(address_change) => self.on_address_change(address_change),
            FromSwarm::DialFailure(dial_failure) => self.on_dial_failure(dial_failure),
            _ => {},
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match &event {
            Event::InboundFailure { stream_id, .. } |
            Event::OutboundFailure { stream_id, .. } |
            Event::StreamClosed { stream_id, .. } => {
                if let Some(connections) = self.connected.get_mut(&peer_id) {
                    for connection in &mut connections.connections {
                        if connection.stream_id == Some(*stream_id) {
                            connection.stream_id = None;
                            connection.pending_sink = None;
                            connection.message_sink = None;
                            break;
                        }
                    }
                }
            },
            Event::OutboundStreamOpened { stream_id, .. } => {
                if let Some(connections) = self.connected.get_mut(&peer_id) {
                    for connection in &mut connections.connections {
                        if connection.stream_id == Some(*stream_id) {
                            connection.message_sink = connection.pending_sink.take();
                            break;
                        }
                    }
                }
            },
            _ => {},
        }
        self.pending_events.push_back(ToSwarm::GenerateEvent(event));
    }

    fn poll(&mut self, _cx: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }
        if self.pending_events.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
            self.pending_events.shrink_to_fit();
        }

        Poll::Pending
    }
}

/// Internal information tracked for an established connection.
#[derive(Debug)]
struct Connection<TMsg> {
    id: ConnectionId,
    stream_id: Option<StreamId>,
    remote_address: Option<Multiaddr>,
    pending_sink: Option<MessageSink<TMsg>>,
    message_sink: Option<MessageSink<TMsg>>,
}

impl<TMsg> Connection<TMsg> {
    fn new(id: ConnectionId, remote_address: Option<Multiaddr>) -> Self {
        Self {
            id,
            remote_address,
            stream_id: None,
            pending_sink: None,
            message_sink: None,
        }
    }
}

#[derive(Debug)]
struct Connections<TMsg> {
    last_selected_index: usize,
    connections: SmallVec<Connection<TMsg>, 2>,
}

impl<TMsg> Connections<TMsg> {
    pub(self) fn new() -> Self {
        Self {
            last_selected_index: 0,
            connections: SmallVec::new(),
        }
    }

    pub(self) fn push(&mut self, connection: Connection<TMsg>) {
        self.connections.push(connection);
    }

    pub(self) fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    pub(self) fn next_active_sink(&mut self) -> Option<&MessageSink<TMsg>> {
        let initial_last_selected = cmp::min(self.last_selected_index, self.connections.len() - 1);
        let (last_index, sink) = cycle_once(self.connections.len(), initial_last_selected, |i| {
            let conn = &self.connections[i];
            conn.message_sink.as_ref()
        })?;

        self.last_selected_index = last_index;
        Some(sink)
    }

    pub(self) fn next_pending_sink(&mut self) -> Option<&MessageSink<TMsg>> {
        let initial_last_selected = cmp::min(self.last_selected_index, self.connections.len() - 1);
        let (last_index, sink) = cycle_once(self.connections.len(), initial_last_selected, |i| {
            let conn = &self.connections[i];
            conn.pending_sink.as_ref()
        })?;

        self.last_selected_index = last_index;
        Some(sink)
    }

    pub(self) fn clear_closed_connections(&mut self) {
        self.connections.retain(|c| {
            c.message_sink.as_ref().map_or(true, |s| !s.is_closed()) &&
                c.pending_sink.as_ref().map_or(true, |s| !s.is_closed())
        });
    }
}

impl<TMsg> Default for Connections<TMsg> {
    fn default() -> Self {
        Self::new()
    }
}

fn cycle_once<T, F>(n: usize, start: usize, mut f: F) -> Option<(usize, T)>
where F: FnMut(usize) -> Option<T> {
    let mut did_wrap = false;
    let mut i = start;
    if n == 0 {
        return None;
    }
    if i >= n {
        return None;
    }

    loop {
        // Did we find a value?
        if let Some(t) = f(i) {
            return Some((i, t));
        }

        // Are we back at where we started?
        if did_wrap && i == start {
            return None;
        }

        i = (i + 1) % n;
        // Did we wrap around?
        if i == 0 {
            did_wrap = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_once_works() {
        assert_eq!(cycle_once(0, 0, |_| -> Option<()> { panic!() }), None);
        assert_eq!(cycle_once(1, 0, Some), Some((0, 0)));
        assert_eq!(cycle_once(0, 1, Some), None);
        assert_eq!(cycle_once(10, 2, |_| None::<()>), None);
        assert_eq!(
            cycle_once(10, 2, |i| {
                if i == 5 {
                    Some(())
                } else {
                    None
                }
            }),
            Some((5, ()))
        );
        assert_eq!(
            cycle_once(10, 2, |i| {
                if i == 1 {
                    Some(())
                } else {
                    None
                }
            }),
            Some((1, ()))
        );
    }
}
