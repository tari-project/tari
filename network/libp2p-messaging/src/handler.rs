//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::VecDeque,
    convert::Infallible,
    future::{ready, Ready},
    io,
    task::{Context, Poll},
    time::Duration,
};

use libp2p::{
    core::UpgradeInfo,
    futures::{channel::mpsc, FutureExt, SinkExt, StreamExt},
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

use crate::{
    codec::Codec,
    error::Error,
    event::Event,
    stream::MessageStream,
    Config,
    MessageId,
    EMPTY_QUEUE_SHRINK_THRESHOLD,
};

pub struct Handler<TCodec: Codec> {
    peer_id: PeerId,
    protocol: StreamProtocol,
    requested_stream: Option<MessageStream<TCodec::Message>>,
    pending_stream: Option<MessageStream<TCodec::Message>>,
    pending_events: VecDeque<Event<TCodec::Message>>,
    pending_events_sender: mpsc::Sender<Event<TCodec::Message>>,
    pending_events_receiver: mpsc::Receiver<Event<TCodec::Message>>,
    codec: TCodec,
    tasks: futures_bounded::FuturesSet<Event<TCodec::Message>>,
}

impl<TCodec: Codec> Handler<TCodec> {
    pub fn new(peer_id: PeerId, protocol: StreamProtocol, config: &Config) -> Self {
        let (pending_events_sender, pending_events_receiver) = mpsc::channel(20);
        Self {
            peer_id,
            protocol,
            requested_stream: None,
            pending_stream: None,
            pending_events: VecDeque::new(),
            codec: TCodec::default(),
            pending_events_sender,
            pending_events_receiver,
            tasks: futures_bounded::FuturesSet::new(
                Duration::from_secs(10000 * 24 * 60 * 60),
                config.max_concurrent_streams_per_peer,
            ),
        }
    }
}

impl<TCodec> Handler<TCodec>
where TCodec: Codec + Send + Clone + 'static
{
    fn on_listen_upgrade_error(&self, error: ListenUpgradeError<(), Protocol<StreamProtocol>>) {
        tracing::warn!("unexpected listen upgrade error: {:?}", error.error);
    }

    fn on_dial_upgrade_error(&mut self, error: DialUpgradeError<(), Protocol<StreamProtocol>>) {
        let stream = self
            .requested_stream
            .take()
            .expect("negotiated a stream without a requested stream");

        match error.error {
            StreamUpgradeError::Timeout => {
                self.pending_events.push_back(Event::OutboundFailure {
                    peer_id: self.peer_id,
                    stream_id: stream.stream_id(),
                    error: Error::DialUpgradeError,
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
                    stream_id: stream.stream_id(),
                    error: Error::ProtocolNotSupported,
                });
            },
            StreamUpgradeError::Apply(_) => {},
            StreamUpgradeError::Io(e) => {
                tracing::debug!(
                    "outbound stream for request {} failed: {e}, retrying",
                    stream.stream_id()
                );
                self.requested_stream = Some(stream);
            },
        }
    }

    fn on_fully_negotiated_outbound(&mut self, outbound: FullyNegotiatedOutbound<Protocol<StreamProtocol>, ()>) {
        let codec = self.codec.clone();
        let (mut peer_stream, _protocol) = outbound.protocol;

        let mut msg_stream = self
            .requested_stream
            .take()
            .expect("negotiated outbound stream without a requested stream");

        let mut events = self.pending_events_sender.clone();

        self.pending_events.push_back(Event::OutboundStreamOpened {
            peer_id: self.peer_id,
            stream_id: msg_stream.stream_id(),
        });

        let fut = async move {
            let mut message_id = MessageId::default();
            let stream_id = msg_stream.stream_id();
            let peer_id = *msg_stream.peer_id();
            loop {
                let Some(msg) = msg_stream.recv().await else {
                    break Event::StreamClosed { peer_id, stream_id };
                };

                match codec.encode_to(&mut peer_stream, msg).await {
                    Ok(()) => {
                        events
                            .send(Event::MessageSent { message_id, stream_id })
                            .await
                            .expect("Can never be closed because receiver is held in this instance");
                    },
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        break Event::StreamClosed { peer_id, stream_id };
                    },
                    Err(e) => break Event::Error(Error::CodecError(e)),
                }
                message_id = message_id.wrapping_add(1);
            }
        }
        .boxed();

        if self.tasks.try_push(fut).is_err() {
            tracing::warn!("Dropping outbound stream because we are at capacity")
        }
    }

    fn on_fully_negotiated_inbound(&mut self, inbound: FullyNegotiatedInbound<Protocol<StreamProtocol>, ()>) {
        let codec = self.codec.clone();
        let peer_id = self.peer_id;
        let (mut stream, _protocol) = inbound.protocol;
        let mut events = self.pending_events_sender.clone();

        self.pending_events.push_back(Event::InboundStreamOpened { peer_id });

        let fut = async move {
            loop {
                // TODO: read timeout
                match codec.decode_from(&mut stream).await {
                    Ok((length, msg)) => {
                        events
                            .send(Event::ReceivedMessage {
                                peer_id,
                                message: msg,
                                length,
                            })
                            .await
                            .expect("Can never be closed because receiver is held in this instance");
                        // TODO
                        // Event::ReceivedMessage { peer_id, message }
                    },
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        break Event::InboundStreamClosed { peer_id };
                    },
                    Err(e) => {
                        break Event::Error(Error::CodecError(e));
                    },
                }
            }
        }
        .boxed();

        if self.tasks.try_push(fut).is_err() {
            tracing::warn!("Dropping inbound stream because we are at capacity")
        }
    }
}

impl<TCodec> ConnectionHandler for Handler<TCodec>
where TCodec: Codec + Send + Clone + 'static
{
    type FromBehaviour = MessageStream<TCodec::Message>;
    type InboundOpenInfo = ();
    type InboundProtocol = Protocol<StreamProtocol>;
    type OutboundOpenInfo = ();
    type OutboundProtocol = Protocol<StreamProtocol>;
    type ToBehaviour = Event<TCodec::Message>;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(
            Protocol {
                protocol: self.protocol.clone(),
            },
            (),
        )
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>> {
        match self.tasks.poll_unpin(cx) {
            Poll::Ready(Ok(event)) => {
                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(event));
            },
            Poll::Ready(Err(err)) => {
                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Event::Error(Error::Timeout(
                    err,
                ))));
            },
            Poll::Pending => {},
        }

        // Drain pending events that were produced by handler
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(event));
        }
        if self.pending_events.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
            self.pending_events.shrink_to_fit();
        }

        // Emit pending events produced by handler tasks
        if let Poll::Ready(Some(event)) = self.pending_events_receiver.poll_next_unpin(cx) {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(event));
        }

        // Open outbound stream.
        if let Some(stream) = self.pending_stream.take() {
            self.requested_stream = Some(stream);

            return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(
                    Protocol {
                        protocol: self.protocol.clone(),
                    },
                    (),
                ),
            });
        }

        Poll::Pending
    }

    fn on_behaviour_event(&mut self, stream: Self::FromBehaviour) {
        self.pending_stream = Some(stream);
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

pub struct Protocol<P> {
    pub(crate) protocol: P,
}

impl<P> UpgradeInfo for Protocol<P>
where P: AsRef<str> + Clone
{
    type Info = P;
    type InfoIter = std::option::IntoIter<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        Some(self.protocol.clone()).into_iter()
    }
}

impl<P> InboundUpgrade<Stream> for Protocol<P>
where P: AsRef<str> + Clone
{
    type Error = Infallible;
    type Future = Ready<Result<Self::Output, Self::Error>>;
    type Output = (Stream, P);

    fn upgrade_inbound(self, io: Stream, protocol: Self::Info) -> Self::Future {
        ready(Ok((io, protocol)))
    }
}

impl<P> OutboundUpgrade<Stream> for Protocol<P>
where P: AsRef<str> + Clone
{
    type Error = Infallible;
    type Future = Ready<Result<Self::Output, Self::Error>>;
    type Output = (Stream, P);

    fn upgrade_outbound(self, io: Stream, protocol: Self::Info) -> Self::Future {
        ready(Ok((io, protocol)))
    }
}
