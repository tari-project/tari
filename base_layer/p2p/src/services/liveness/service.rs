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

use super::{
    config::LivenessConfig,
    error::LivenessError,
    message::{PingPong, PingPongMessage},
    state::LivenessState,
    LivenessRequest,
    LivenessResponse,
    LOG_TARGET,
};
use crate::{
    domain_message::DomainMessage,
    services::liveness::{neighbours::Neighbours, LivenessEvent, PongEvent},
    tari_message::TariMessageType,
};
use futures::{pin_mut, stream::StreamExt, task::Context, SinkExt, Stream};
use log::*;
use std::{pin::Pin, task::Poll, time::Instant};
use tari_broadcast_channel::Publisher;
use tari_comms::{
    peer_manager::{NodeId, Peer},
    types::CommsPublicKey,
};
use tari_comms_dht::{
    broadcast_strategy::BroadcastStrategy,
    domain_message::OutboundDomainMessage,
    outbound::{DhtOutboundError, OutboundEncryption, OutboundMessageRequester},
    DhtRequester,
};
use tari_service_framework::RequestContext;
use tari_shutdown::ShutdownSignal;
use tokio::time;

/// Service responsible for testing Liveness for Peers.
///
/// Very basic global ping and pong counter stats are implemented. In future,
/// peer latency and availability stats will be added.
pub struct LivenessService<THandleStream, TPingStream> {
    config: LivenessConfig,
    request_rx: Option<THandleStream>,
    ping_stream: Option<TPingStream>,
    state: LivenessState,
    dht_requester: DhtRequester,
    oms_handle: OutboundMessageRequester,
    event_publisher: Publisher<LivenessEvent>,
    shutdown_signal: Option<ShutdownSignal>,
    neighbours: Neighbours,
}

impl<THandleStream, TPingStream> LivenessService<THandleStream, TPingStream> {
    pub fn new(
        config: LivenessConfig,
        request_rx: THandleStream,
        ping_stream: TPingStream,
        state: LivenessState,
        dht_requester: DhtRequester,
        oms_handle: OutboundMessageRequester,
        event_publisher: Publisher<LivenessEvent>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            request_rx: Some(request_rx),
            ping_stream: Some(ping_stream),
            state,
            dht_requester,
            oms_handle,
            event_publisher,
            shutdown_signal: Some(shutdown_signal),
            neighbours: Neighbours::new(config.refresh_neighbours_interval),
            config,
        }
    }
}

impl<THandleStream, TPingStream> LivenessService<THandleStream, TPingStream>
where
    TPingStream: Stream<Item = DomainMessage<PingPongMessage>>,
    THandleStream: Stream<Item = RequestContext<LivenessRequest, Result<LivenessResponse, LivenessError>>>,
{
    pub async fn run(mut self) {
        let ping_stream = self.ping_stream.take().expect("ping_stream cannot be None").fuse();
        pin_mut!(ping_stream);

        let request_stream = self.request_rx.take().expect("ping_stream cannot be None").fuse();
        pin_mut!(request_stream);

        let mut ping_tick = match self.config.auto_ping_interval {
            Some(interval) => EitherStream::Left(time::interval_at((Instant::now() + interval).into(), interval)),
            None => EitherStream::Right(futures::stream::iter(Vec::new())),
        }
        .fuse();

        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Liveness service initialized without shutdown signal");

        loop {
            futures::select! {
                // Requests from the handle
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },

                _ = ping_tick.select_next_some() => {
                        let _ = self.ping_neighbours().await.or_else(|err| {
                            error!(target: LOG_TARGET, "Error when pinging neighbours: {}", err);
                            Err(err)
                        });
                        let _ = self.ping_monitored_node_ids().await.or_else(|err| {
                            error!(target: LOG_TARGET, "Error when pinging monitored nodes: {}", err);
                            Err(err)
                        });
                },
                // Incoming messages from the Comms layer
                msg = ping_stream.select_next_some() => {
                    let _ = self.handle_incoming_message(msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming PingPong message: {:?}", err);
                        Err(err)
                    });
                },
                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "Liveness service shutting down because the shutdown signal was received");
                    break;
                }
            }
        }
    }

    async fn handle_incoming_message(&mut self, msg: DomainMessage<PingPongMessage>) -> Result<(), LivenessError> {
        let inner_msg = msg.inner();
        match inner_msg.kind().ok_or_else(|| LivenessError::InvalidPingPongType)? {
            PingPong::Ping => {
                self.state.inc_pings_received();
                self.send_pong(msg.inner.nonce, msg.source_peer.public_key.clone())
                    .await
                    .unwrap();
                self.state.inc_pongs_sent();

                self.publish_event(LivenessEvent::ReceivedPing).await?;
            },
            PingPong::Pong => {
                let maybe_latency = self.state.record_pong(inner_msg.nonce);
                trace!(target: LOG_TARGET, "Recorded latency: {:?}", maybe_latency);
                let is_neighbour = self.neighbours.peers().contains(&msg.source_peer);
                let is_monitored = self.state.is_monitored_node_id(&msg.source_peer.node_id);
                let pong_event = PongEvent::new(
                    msg.source_peer.node_id.clone(),
                    maybe_latency,
                    msg.inner.metadata.into(),
                    is_neighbour,
                    is_monitored,
                );

                self.publish_event(LivenessEvent::ReceivedPong(Box::new(pong_event)))
                    .await?;
            },
        }
        Ok(())
    }

    async fn send_pong(&mut self, nonce: u64, dest: CommsPublicKey) -> Result<(), LivenessError> {
        let msg = PingPongMessage::pong_with_metadata(nonce, self.state.pong_metadata().clone());
        self.oms_handle
            .send_direct(
                dest,
                OutboundEncryption::None,
                OutboundDomainMessage::new(TariMessageType::PingPong, msg),
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn handle_request(&mut self, request: LivenessRequest) -> Result<LivenessResponse, LivenessError> {
        use LivenessRequest::*;
        match request {
            SendPing(node_id) => {
                self.send_ping(node_id).await?;
                self.state.inc_pings_sent();
                Ok(LivenessResponse::Ok)
            },
            GetPingCount => {
                let ping_count = self.get_ping_count();
                Ok(LivenessResponse::Count(ping_count))
            },
            GetPongCount => {
                let pong_count = self.get_pong_count();
                Ok(LivenessResponse::Count(pong_count))
            },
            GetAvgLatency(node_id) => {
                let latency = self.state.get_avg_latency_ms(&node_id);
                Ok(LivenessResponse::AvgLatency(latency))
            },
            SetPongMetadata(key, value) => {
                self.state.set_pong_metadata_entry(key, value);
                Ok(LivenessResponse::Ok)
            },
            GetNumActiveNeighbours => {
                let num_active_neighbours = self.state.num_active_neighbours();
                Ok(LivenessResponse::NumActiveNeighbours(num_active_neighbours))
            },
            AddNodeId(node_id) => {
                self.state.add_node_id(&node_id);
                self.send_ping(node_id.clone()).await?;
                Ok(LivenessResponse::NodeIdAdded)
            },
            GetNodeIdStats(node_id) => self
                .state
                .get_node_id_stats(&node_id)
                .map(LivenessResponse::NodeIdStats),
        }
    }

    async fn send_ping(&mut self, node_id: NodeId) -> Result<(), LivenessError> {
        let msg = PingPongMessage::ping();
        self.state.add_inflight_ping(msg.nonce, &node_id);
        self.oms_handle
            .send_direct_node_id(
                node_id,
                OutboundEncryption::None,
                OutboundDomainMessage::new(TariMessageType::PingPong, msg),
            )
            .await
            .map_err(Into::<DhtOutboundError>::into)?;

        Ok(())
    }

    async fn update_neighbours_if_stale(&mut self) -> Result<&[Peer], LivenessError> {
        if self.neighbours.is_fresh() {
            return Ok(self.neighbours.peers());
        }

        let peers = self
            .dht_requester
            .select_peers(BroadcastStrategy::Neighbours(Vec::new()))
            .await?;

        self.state.set_num_active_neighbours(peers.len());
        self.neighbours.set_peers(peers);

        Ok(self.neighbours.peers())
    }

    async fn ping_neighbours(&mut self) -> Result<(), LivenessError> {
        self.update_neighbours_if_stale().await?;
        let peers = self.neighbours.peers();
        let len_peers = peers.len();
        trace!(
            target: LOG_TARGET,
            "Sending liveness ping to {} neighbour(s)",
            len_peers
        );

        for peer in peers {
            let msg = PingPongMessage::ping();
            self.state.add_inflight_ping(msg.nonce, &peer.node_id);
            self.oms_handle
                .send_direct(
                    peer.public_key.clone(),
                    OutboundEncryption::None,
                    OutboundDomainMessage::new(TariMessageType::PingPong, msg),
                )
                .await?;
        }

        self.publish_event(LivenessEvent::BroadcastedNeighbourPings(len_peers))
            .await?;

        Ok(())
    }

    async fn ping_monitored_node_ids(&mut self) -> Result<(), LivenessError> {
        let num_nodes = self.state.get_num_monitored_nodes();
        if num_nodes > 0 {
            trace!(
                target: LOG_TARGET,
                "Sending liveness ping to {} monitored nodes",
                num_nodes,
            );
            for node_id in self.state.get_monitored_node_ids() {
                let msg = PingPongMessage::ping();
                self.state.add_inflight_ping(msg.nonce, &node_id);
                self.oms_handle
                    .send_direct_node_id(
                        node_id,
                        OutboundEncryption::None,
                        OutboundDomainMessage::new(TariMessageType::PingPong, msg),
                    )
                    .await
                    .map_err(Into::<DhtOutboundError>::into)?;
            }

            self.publish_event(LivenessEvent::BroadcastedMonitoredNodeIdPings(num_nodes))
                .await?;
        }
        Ok(())
    }

    async fn publish_event(&mut self, event: LivenessEvent) -> Result<(), LivenessError> {
        self.event_publisher
            .send(event)
            .await
            .map_err(|_| LivenessError::EventStreamError)
    }

    fn get_ping_count(&self) -> usize {
        self.state.pings_received()
    }

    fn get_pong_count(&self) -> usize {
        self.state.pongs_received()
    }
}

// Unfortunately, `stream::Either` doesn't exist yet in futures-0.3.0
enum EitherStream<A, B> {
    Left(A),
    Right(B),
}

impl<A, B> Stream for EitherStream<A, B>
where
    A: Stream + Unpin,
    B: Stream<Item = A::Item> + Unpin,
{
    type Item = A::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match &mut *self {
            EitherStream::Left(stream) => {
                pin_mut!(stream);
                stream.poll_next(cx)
            },
            EitherStream::Right(stream) => {
                pin_mut!(stream);
                stream.poll_next(cx)
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        proto::liveness::MetadataKey,
        services::liveness::{handle::LivenessHandle, state::Metadata},
    };
    use futures::{channel::mpsc, stream};
    use rand::rngs::OsRng;
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    };
    use tari_broadcast_channel as broadcast_channel;
    use tari_comms::{
        multiaddr::Multiaddr,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    };
    use tari_comms_dht::{
        envelope::{DhtMessageHeader, DhtMessageType, Network},
        outbound::{DhtOutboundRequest, SendMessageResponse},
    };
    use tari_crypto::keys::PublicKey;
    use tari_service_framework::reply_channel;
    use tari_shutdown::Shutdown;
    use tari_test_utils::runtime;

    #[test]
    fn get_ping_pong_count() {
        runtime::test_async(|rt| {
            let state = LivenessState::new();
            state.inc_pings_received();
            state.inc_pongs_received();
            state.inc_pongs_received();

            // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
            let (outbound_tx, _) = mpsc::channel(10);
            let oms_handle = OutboundMessageRequester::new(outbound_tx);

            // Setup liveness service
            let (sender_service, receiver) = reply_channel::unbounded();
            let (publisher, subscriber) = broadcast_channel::bounded(100);
            let mut liveness_handle = LivenessHandle::new(sender_service, subscriber);

            let (dht_tx, _) = mpsc::channel(10);
            let dht_requester = DhtRequester::new(dht_tx);

            let mut shutdown = Shutdown::new();
            let service = LivenessService::new(
                Default::default(),
                receiver,
                stream::empty(),
                state,
                dht_requester,
                oms_handle,
                publisher,
                shutdown.to_signal(),
            );

            // Run the service
            rt.spawn(service.run());

            let res = rt.block_on(liveness_handle.get_ping_count()).unwrap();
            assert_eq!(res, 1);

            let res = rt.block_on(liveness_handle.get_pong_count()).unwrap();
            assert_eq!(res, 2);

            shutdown.trigger().unwrap();
        });
    }

    #[test]
    fn send_ping() {
        runtime::test_async(|rt| {
            let state = LivenessState::new();

            // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
            let (outbound_tx, mut outbound_rx) = mpsc::channel(10);
            let oms_handle = OutboundMessageRequester::new(outbound_tx);

            // Setup liveness service
            let (sender_service, receiver) = reply_channel::unbounded();
            let (publisher, subscriber) = broadcast_channel::bounded(100);
            let mut liveness_handle = LivenessHandle::new(sender_service, subscriber);

            let (dht_tx, _) = mpsc::channel(10);
            let dht_requester = DhtRequester::new(dht_tx);

            let mut shutdown = Shutdown::new();
            let service = LivenessService::new(
                Default::default(),
                receiver,
                stream::empty(),
                state,
                dht_requester,
                oms_handle,
                publisher,
                shutdown.to_signal(),
            );

            // Run the LivenessService
            rt.spawn(service.run());

            let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rngs::OsRng);
            let node_id = NodeId::from_key(&pk).unwrap();
            // Receive outbound request
            rt.spawn(async move {
                match outbound_rx.select_next_some().await {
                    DhtOutboundRequest::SendMessage(_, _, reply_tx) => {
                        reply_tx.send(SendMessageResponse::Queued(vec![])).unwrap();
                    },
                }
            });

            let _res = rt.block_on(liveness_handle.send_ping(node_id)).unwrap();

            shutdown.trigger().unwrap();
        })
    }

    fn create_dummy_message<T>(inner: T) -> DomainMessage<T> {
        let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let source_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk).unwrap(),
            Vec::<Multiaddr>::new().into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        DomainMessage {
            dht_header: DhtMessageHeader::new(
                Default::default(),
                DhtMessageType::None,
                None,
                Network::LocalTest,
                Default::default(),
            ),
            source_peer,
            inner,
        }
    }

    #[test]
    fn handle_message_ping() {
        runtime::test_async(|rt| {
            let state = LivenessState::new();

            // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
            let (outbound_tx, mut outbound_rx) = mpsc::channel(10);
            let oms_handle = OutboundMessageRequester::new(outbound_tx);

            let msg = create_dummy_message(PingPongMessage::ping());
            // A stream which emits one message and then closes
            let pingpong_stream = stream::iter(std::iter::once(msg));

            let (dht_tx, _) = mpsc::channel(10);
            let dht_requester = DhtRequester::new(dht_tx);
            // Setup liveness service
            let (publisher, _subscriber) = broadcast_channel::bounded(100);
            let mut shutdown = Shutdown::new();
            let service = LivenessService::new(
                Default::default(),
                stream::empty(),
                pingpong_stream,
                state,
                dht_requester,
                oms_handle,
                publisher,
                shutdown.to_signal(),
            );

            rt.spawn(service.run());

            rt.spawn(async move {
                // Test oms got request to send message
                unwrap_oms_send_msg!(outbound_rx.select_next_some().await);
                shutdown.trigger().unwrap();
            });
        });
    }

    #[test]
    fn handle_message_pong() {
        runtime::test_async(|rt| {
            rt.spawn(async {
                let mut state = LivenessState::new();

                let (outbound_tx, _) = mpsc::channel(10);
                let oms_handle = OutboundMessageRequester::new(outbound_tx);

                let mut metadata = Metadata::new();
                metadata.insert(MetadataKey::ChainMetadata, b"dummy-data".to_vec());
                let msg = create_dummy_message(PingPongMessage::pong_with_metadata(123, metadata));

                state.add_inflight_ping(msg.inner.nonce, &msg.source_peer.node_id);
                // A stream which emits one message and then closes
                let pingpong_stream = stream::iter(std::iter::once(msg));

                let (dht_tx, _) = mpsc::channel(10);
                let dht_requester = DhtRequester::new(dht_tx);
                // Setup liveness service
                let (publisher, subscriber) = broadcast_channel::bounded(100);
                let mut shutdown = Shutdown::new();
                let service = LivenessService::new(
                    Default::default(),
                    stream::empty(),
                    pingpong_stream,
                    state,
                    dht_requester,
                    oms_handle,
                    publisher,
                    shutdown.to_signal(),
                );

                // Create a flag that gets flipped when the subscribed event is received
                let received_event = Arc::new(AtomicBool::new(false));
                let rec_event_clone = received_event.clone();

                // Listen for the pong event
                futures::join!(
                    async move {
                        let event = time::timeout(Duration::from_secs(10), subscriber.fuse().select_next_some())
                            .await
                            .unwrap();

                        match &*event {
                            LivenessEvent::ReceivedPong(event) => {
                                rec_event_clone.store(true, Ordering::SeqCst);
                                assert_eq!(event.metadata.get(MetadataKey::ChainMetadata).unwrap(), b"dummy-data");
                            },
                            _ => panic!("Unexpected event"),
                        }

                        shutdown.trigger().unwrap();
                    },
                    service.run()
                );

                assert_eq!(received_event.load(Ordering::SeqCst), true);
            });
        });
    }
}
