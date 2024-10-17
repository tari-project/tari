//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::VecDeque,
    convert::Infallible,
    future::{ready, Ready},
    task::{Context, Poll},
};

use libp2p::{
    core::UpgradeInfo,
    swarm::{
        handler::{
            ConnectionEvent,
            DialUpgradeError,
            FullyNegotiatedInbound,
            FullyNegotiatedOutbound,
            ListenUpgradeError,
        },
        ConnectionHandler,
        ConnectionHandlerEvent,
        StreamUpgradeError,
        SubstreamProtocol,
    },
    InboundUpgrade,
    OutboundUpgrade,
    PeerId,
    Stream,
    StreamProtocol,
};
use smallvec::SmallVec;

use crate::{
    error::Error,
    event::Event,
    stream::OpenStreamRequest,
    FromBehaviourEvent,
    ProtocolEvent,
    ProtocolNotification,
    StreamId,
    EMPTY_QUEUE_SHRINK_THRESHOLD,
};

pub struct Handler {
    peer_id: PeerId,
    protocols: Protocols<StreamProtocol>,
    requested_streams: VecDeque<OpenStreamRequest>,
    pending_behaviour_events: VecDeque<FromBehaviourEvent>,
    pending_events: VecDeque<Event>,
}

impl Handler {
    pub fn new(peer_id: PeerId, protocols: SmallVec<[StreamProtocol; 32]>) -> Self {
        Self {
            peer_id,
            protocols: Protocols::new(protocols),
            requested_streams: VecDeque::new(),
            pending_behaviour_events: VecDeque::new(),
            pending_events: VecDeque::new(),
        }
    }
}

impl Handler {
    fn on_listen_upgrade_error(&self, error: ListenUpgradeError<(), Protocols<StreamProtocol>>) {
        tracing::warn!("unexpected listen upgrade error: {:?}", error.error);
    }

    fn on_dial_upgrade_error(&mut self, error: DialUpgradeError<StreamId, ChosenProtocol<StreamProtocol>>) {
        let stream = self
            .requested_streams
            .pop_front()
            .expect("negotiated a stream without a pending request");

        match error.error {
            StreamUpgradeError::Timeout => {
                self.pending_events.push_back(Event::OutboundFailure {
                    peer_id: self.peer_id,
                    protocol: stream.protocol().clone(),
                    stream_id: stream.stream_id(),
                    error: Error::ProtocolNegotiationTimeout,
                });
            },
            StreamUpgradeError::NegotiationFailed => {
                // The remote merely doesn't support the protocol(s) we requested.
                // This is no reason to close the connection, which may
                // successfully communicate with other protocols already.
                // An event is reported to permit user code to react to the fact that
                // the remote peer does not support the requested protocol(s).
                self.pending_events.push_back(Event::OutboundFailure {
                    peer_id: self.peer_id,
                    protocol: stream.protocol().clone(),
                    stream_id: stream.stream_id(),
                    error: Error::ProtocolNotSupported,
                });
            },
            StreamUpgradeError::Apply(_infallible) => {},
            StreamUpgradeError::Io(e) => {
                tracing::debug!(
                    "outbound stream for request {} failed: {e}, retrying",
                    stream.stream_id()
                );
                self.requested_streams.push_back(stream);
            },
        }
    }

    fn on_fully_negotiated_outbound(
        &mut self,
        outbound: FullyNegotiatedOutbound<ChosenProtocol<StreamProtocol>, StreamId>,
    ) {
        let (stream, protocol) = outbound.protocol;
        let stream_id = outbound.info;
        // Requested stream succeeded, remove it from the pending list.
        let _request = self.requested_streams.pop_front();

        self.pending_events.push_back(Event::SubstreamOpen {
            peer_id: self.peer_id,
            stream_id,
            stream,
            protocol,
        });
    }

    fn on_fully_negotiated_inbound(&mut self, inbound: FullyNegotiatedInbound<Protocols<StreamProtocol>, ()>) {
        let peer_id = self.peer_id;
        let (stream, protocol) = inbound.protocol;

        self.pending_events.push_back(Event::InboundSubstreamOpen {
            notification: ProtocolNotification::new(protocol, ProtocolEvent::NewInboundSubstream {
                peer_id,
                substream: stream,
            }),
        });
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = FromBehaviourEvent;
    type InboundOpenInfo = ();
    type InboundProtocol = Protocols<StreamProtocol>;
    type OutboundOpenInfo = StreamId;
    type OutboundProtocol = ChosenProtocol<StreamProtocol>;
    type ToBehaviour = Event;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(self.protocols.clone(), ())
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>> {
        // Drain pending events that were produced by handler
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(event));
        }
        if self.pending_events.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
            self.pending_events.shrink_to_fit();
        }

        // Emit outbound streams.
        if let Some(event) = self.pending_behaviour_events.pop_front() {
            match event {
                FromBehaviourEvent::OpenRpcSessionRequest(stream) => {
                    let protocol = stream.protocol().clone();
                    let stream_id = stream.stream_id();
                    self.requested_streams.push_back(stream);

                    return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                        protocol: SubstreamProtocol::new(ChosenProtocol { protocol }, stream_id),
                    });
                },
                FromBehaviourEvent::AddSupportedProtocol(added) => {
                    if !self.protocols.protocols.contains(&added) {
                        self.protocols.protocols.push(added);
                    }
                },
            }
        }

        debug_assert!(self.pending_behaviour_events.is_empty());

        if self.pending_behaviour_events.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
            self.pending_behaviour_events.shrink_to_fit();
        }

        Poll::Pending
    }

    fn on_behaviour_event(&mut self, stream: Self::FromBehaviour) {
        self.pending_behaviour_events.push_back(stream);
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(fully_negotiated_inbound) => {
                self.on_fully_negotiated_inbound(fully_negotiated_inbound)
            },
            ConnectionEvent::FullyNegotiatedOutbound(fully_negotiated_outbound) => {
                self.on_fully_negotiated_outbound(fully_negotiated_outbound)
            },
            ConnectionEvent::DialUpgradeError(dial_upgrade_error) => self.on_dial_upgrade_error(dial_upgrade_error),
            ConnectionEvent::ListenUpgradeError(listen_upgrade_error) => {
                self.on_listen_upgrade_error(listen_upgrade_error)
            },
            _ => {},
        }
    }
}

#[derive(Clone)]
pub struct Protocols<P> {
    protocols: SmallVec<[P; 32]>,
}

impl<P> Protocols<P> {
    pub fn new(protocols: SmallVec<[P; 32]>) -> Self {
        Self { protocols }
    }
}

impl<P> UpgradeInfo for Protocols<P>
where P: AsRef<str> + Clone
{
    type Info = P;
    type InfoIter = smallvec::IntoIter<[Self::Info; 32]>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.protocols.clone().into_iter()
    }
}

impl<P> InboundUpgrade<Stream> for Protocols<P>
where P: AsRef<str> + Clone
{
    type Error = Infallible;
    type Future = Ready<Result<Self::Output, Self::Error>>;
    type Output = (Stream, P);

    fn upgrade_inbound(self, io: Stream, protocol: Self::Info) -> Self::Future {
        ready(Ok((io, protocol)))
    }
}

#[derive(Clone)]
pub struct ChosenProtocol<P> {
    protocol: P,
}

impl<P> UpgradeInfo for ChosenProtocol<P>
where P: AsRef<str> + Clone
{
    type Info = P;
    type InfoIter = std::option::IntoIter<P>;

    fn protocol_info(&self) -> Self::InfoIter {
        Some(self.protocol.clone()).into_iter()
    }
}

impl<P> OutboundUpgrade<Stream> for ChosenProtocol<P>
where P: AsRef<str> + Clone
{
    type Error = Infallible;
    type Future = Ready<Result<Self::Output, Self::Error>>;
    type Output = (Stream, P);

    fn upgrade_outbound(self, io: Stream, protocol: Self::Info) -> Self::Future {
        ready(Ok((io, protocol)))
    }
}
