//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::VecDeque,
    convert::Infallible,
    future::{ready, Ready},
    sync::Arc,
    task::{Context, Poll},
};

use async_semaphore::{Semaphore, SemaphoreGuardArc};
use libp2p::{
    core::UpgradeInfo,
    futures::FutureExt,
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
    behaviour::WantList,
    error::Error,
    event::Event,
    inbound_task::inbound_sync_task,
    outbound_task::outbound_sync_task,
    proto,
    store::PeerStore,
    Config,
    EMPTY_QUEUE_SHRINK_THRESHOLD,
    MAX_MESSAGE_SIZE,
};

pub(crate) type Framed<In, Out = In> = asynchronous_codec::Framed<Stream, quick_protobuf_codec::Codec<In, Out>>;
pub(crate) type FramedOutbound = Framed<proto::WantPeers, proto::WantPeerResponse>;
pub(crate) type FramedInbound = Framed<proto::WantPeerResponse, proto::WantPeers>;

pub struct Handler<TStore> {
    peer_id: PeerId,
    protocol: StreamProtocol,
    must_request_substream: bool,
    is_complete: bool,
    failed_attempts: usize,
    current_want_list: Arc<WantList>,
    pending_events: VecDeque<Event>,
    tasks: futures_bounded::FuturesSet<Event>,
    config: Config,
    store: TStore,
    semaphore: Arc<Semaphore>,
    aquired: Option<SemaphoreGuardArc>,
}

impl<TStore: PeerStore> Handler<TStore> {
    pub fn new(
        peer_id: PeerId,
        store: TStore,
        protocol: StreamProtocol,
        config: &Config,
        want_list: WantList,
        semaphore: Arc<Semaphore>,
    ) -> Self {
        Self {
            store,
            peer_id,
            protocol,
            is_complete: false,
            failed_attempts: 0,
            current_want_list: Arc::new(want_list),
            pending_events: VecDeque::new(),
            must_request_substream: true,
            tasks: futures_bounded::FuturesSet::new(config.sync_timeout, config.max_concurrent_streams),
            config: Default::default(),
            semaphore,
            aquired: None,
        }
    }
}

impl<TStore> Handler<TStore>
where TStore: PeerStore
{
    fn on_listen_upgrade_error(&self, error: ListenUpgradeError<(), Protocol<StreamProtocol>>) {
        tracing::warn!("unexpected listen upgrade error: {:?}", error.error);
    }

    fn on_dial_upgrade_error(&mut self, error: DialUpgradeError<(), Protocol<StreamProtocol>>) {
        match error.error {
            StreamUpgradeError::Timeout => {
                self.pending_events.push_back(Event::OutboundFailure {
                    peer_id: self.peer_id,
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
                    error: Error::ProtocolNotSupported,
                });
            },
            StreamUpgradeError::Apply(_) => {},
            StreamUpgradeError::Io(e) => {
                tracing::debug!("outbound stream for request failed: {e}, retrying",);
                self.must_request_substream = true;
            },
        }
    }

    fn on_fully_negotiated_outbound(&mut self, outbound: FullyNegotiatedOutbound<Protocol<StreamProtocol>, ()>) {
        if self.current_want_list.is_empty() {
            tracing::debug!("No peers wanted, ignoring outbound stream");
            return;
        }
        let (stream, _protocol) = outbound.protocol;
        let framed = new_framed_codec(stream, MAX_MESSAGE_SIZE);
        let store = self.store.clone();
        if self.current_want_list.is_empty() {
            tracing::debug!("No peers wanted, ignoring outbound stream");
            return;
        }

        let fut = outbound_sync_task(self.peer_id, framed, store, self.current_want_list.clone()).boxed();

        if self.tasks.try_push(fut).is_err() {
            tracing::warn!("Dropping outbound peer sync because we are at capacity")
        }
    }

    fn on_fully_negotiated_inbound(&mut self, inbound: FullyNegotiatedInbound<Protocol<StreamProtocol>, ()>) {
        let (stream, _protocol) = inbound.protocol;
        let config = self.config.clone();
        let framed = new_framed_codec(stream, MAX_MESSAGE_SIZE);
        let store = self.store.clone();

        let fut = inbound_sync_task(self.peer_id, framed, store, config).boxed();

        if self.tasks.try_push(fut).is_err() {
            tracing::warn!("Dropping inbound peer sync because we are at capacity")
        }
    }
}

impl<TStore> ConnectionHandler for Handler<TStore>
where TStore: PeerStore
{
    type FromBehaviour = Arc<WantList>;
    type InboundOpenInfo = ();
    type InboundProtocol = Protocol<StreamProtocol>;
    type OutboundOpenInfo = ();
    type OutboundProtocol = Protocol<StreamProtocol>;
    type ToBehaviour = Event;

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
        // Work on the tasks
        match self.tasks.poll_unpin(cx) {
            Poll::Ready(Ok(event)) => {
                match event {
                    // Anything happens with the outbound stream, we're done
                    Event::OutboundFailure { .. } | Event::Error(_) | Event::OutboundStreamInterrupted { .. } => {
                        // Release the semaphore, and retry. If we've retried too many times, give up.
                        self.aquired = None;
                        self.failed_attempts += 1;
                        if self.failed_attempts > self.config.max_failure_retries {
                            self.is_complete = true;
                            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Event::Error(
                                Error::MaxFailedAttemptsReached,
                            )));
                        } else {
                            self.must_request_substream = true;
                        }
                    },
                    Event::PeerBatchReceived { .. } => {
                        // We're done, release the semaphore
                        self.aquired = None;
                        self.is_complete = true;
                    },
                    _ => {},
                }
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

        // If we've synced from the peer already or if the sync failed, there's nothing further to do
        if self.is_complete {
            // Ensure that the semaphore is released
            self.aquired = None;
            return Poll::Pending;
        }

        // If we do not want any peers, there's nothing further to do
        if self.current_want_list.is_empty() {
            tracing::debug!(
                "peer-sync[{}]: No peers wanted, waiting until peers are wanted",
                self.peer_id
            );
            return Poll::Pending;
        }
        tracing::debug!(
            "peer-sync[{}]: Want {} peers",
            self.peer_id,
            self.current_want_list.len()
        );

        // Otherwise, wait until another sync is complete
        if self.aquired.is_none() {
            match self.semaphore.try_acquire_arc() {
                Some(guard) => {
                    self.aquired = Some(guard);
                },
                None => {
                    return Poll::Pending;
                },
            }
        }

        tracing::debug!("peer-sync[{}]: Acquired semaphore", self.peer_id);

        // Our turn, open the substream
        if self.must_request_substream {
            let protocol = self.protocol.clone();
            self.must_request_substream = false;

            tracing::debug!("peer-sync[{}]: Requesting substream open", self.peer_id);
            return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(Protocol { protocol }, ()),
            });
        }

        Poll::Pending
    }

    fn on_behaviour_event(&mut self, want_list: Self::FromBehaviour) {
        // Sync from existing connections if there are more want-peers
        self.is_complete = !want_list.is_empty();
        self.current_want_list = want_list;
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

fn new_framed_codec<In: quick_protobuf::MessageWrite, Out: for<'a> quick_protobuf::MessageRead<'a>>(
    stream: Stream,
    max_message_length: usize,
) -> Framed<In, Out> {
    asynchronous_codec::Framed::new(stream, quick_protobuf_codec::Codec::<In, Out>::new(max_message_length))
}
