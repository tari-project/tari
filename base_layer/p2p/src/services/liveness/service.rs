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

use std::{iter, sync::Arc, time::Instant};

use futures::{future::Either, pin_mut, stream::StreamExt, Stream};
use log::*;
use tari_comms::{
    connectivity::{ConnectivityRequester, ConnectivitySelection},
    peer_manager::NodeId,
    types::CommsPublicKey,
};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    outbound::{DhtOutboundError, OutboundMessageRequester},
};
use tari_service_framework::reply_channel::RequestContext;
use tari_shutdown::ShutdownSignal;
use tokio::{sync::RwLock, time, time::MissedTickBehavior};
use tokio_stream::wrappers;

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
    services::liveness::{handle::LivenessEventSender, LivenessEvent, PingPongEvent},
    tari_message::TariMessageType,
};

/// Service responsible for testing Liveness of Peers.
pub struct LivenessService<THandleStream, TPingStream> {
    config: LivenessConfig,
    request_rx: Option<THandleStream>,
    ping_stream: Option<TPingStream>,
    state: LivenessState,
    connectivity: ConnectivityRequester,
    outbound_messaging: OutboundMessageRequester,
    event_publisher: LivenessEventSender,
    shutdown_signal: ShutdownSignal,
    monitored_peers: Arc<RwLock<Vec<NodeId>>>,
}

impl<TRequestStream, TPingStream> LivenessService<TRequestStream, TPingStream>
where
    TPingStream: Stream<Item = DomainMessage<PingPongMessage>>,
    TRequestStream: Stream<Item = RequestContext<LivenessRequest, Result<LivenessResponse, LivenessError>>>,
{
    pub fn new(
        config: LivenessConfig,
        request_rx: TRequestStream,
        ping_stream: TPingStream,
        state: LivenessState,
        connectivity: ConnectivityRequester,
        outbound_messaging: OutboundMessageRequester,
        event_publisher: LivenessEventSender,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            request_rx: Some(request_rx),
            ping_stream: Some(ping_stream),
            state,
            connectivity,
            outbound_messaging,
            event_publisher,
            shutdown_signal,
            config: config.clone(),
            monitored_peers: Arc::new(RwLock::new(config.monitored_peers)),
        }
    }

    pub async fn run(mut self) {
        debug!(target: LOG_TARGET, "Liveness service started");
        debug!(target: LOG_TARGET, "Config = {:?}", self.config);
        let ping_stream = self.ping_stream.take().expect("ping_stream cannot be None").fuse();
        pin_mut!(ping_stream);

        let request_stream = self.request_rx.take().expect("ping_stream cannot be None").fuse();
        pin_mut!(request_stream);

        let mut ping_tick = match self.config.auto_ping_interval {
            Some(interval) => {
                let mut interval = time::interval_at((Instant::now() + interval).into(), interval);
                interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
                Either::Left(wrappers::IntervalStream::new(interval))
            },
            None => Either::Right(futures::stream::iter(iter::empty())),
        };

        loop {
            tokio::select! {
                // Requests from the handle
                Some(request_context) = request_stream.next() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await);
                },

                // Tick events
                Some(_) = ping_tick.next() => {
                    if let Err(err) = self.start_ping_round().await {
                        warn!(target: LOG_TARGET, "Error when pinging peers: {}", err);
                    }
                    if self.config.max_allowed_ping_failures > 0 {
                        if let Err(err) = self.disconnect_failed_peers().await {
                            error!(target: LOG_TARGET, "Error occurred while disconnecting failed peers: {}", err);
                        }
                    }
                },

                // Incoming messages from the Comms layer
                Some(msg) = ping_stream.next() => {
                    if let Err(err) = self.handle_incoming_message(msg).await {
                        warn!(target: LOG_TARGET, "Failed to handle incoming PingPong message: {}", err);
                    }
                },

                _ = self.shutdown_signal.wait() => {
                    info!(target: LOG_TARGET, "Liveness service shutting down because the shutdown signal was received");
                    break;
                }
            }
        }
    }

    async fn handle_incoming_message(&mut self, msg: DomainMessage<PingPongMessage>) -> Result<(), LivenessError> {
        let DomainMessage::<_> {
            source_peer,
            dht_header,
            inner: ping_pong_msg,
            ..
        } = msg;
        let node_id = source_peer.node_id;
        let public_key = source_peer.public_key;
        let message_tag = dht_header.message_tag;

        match ping_pong_msg.kind().ok_or(LivenessError::InvalidPingPongType)? {
            PingPong::Ping => {
                self.state.inc_pings_received();
                self.send_pong(ping_pong_msg.nonce, public_key).await.unwrap();
                self.state.inc_pongs_sent();

                debug!(
                    target: LOG_TARGET,
                    "Received ping from peer '{}' with useragent '{}' (Trace: {})",
                    node_id.short_str(),
                    source_peer.user_agent,
                    message_tag,
                );

                let ping_event = PingPongEvent::new(node_id, None, ping_pong_msg.metadata.into());
                self.publish_event(LivenessEvent::ReceivedPing(Box::new(ping_event)));
            },
            PingPong::Pong => {
                if !self.state.is_inflight(ping_pong_msg.nonce) {
                    debug!(
                        target: LOG_TARGET,
                        "Received Pong that was not requested from '{}' with useragent {}. Ignoring it. (Trace: {})",
                        node_id.short_str(),
                        source_peer.user_agent,
                        message_tag,
                    );
                    return Ok(());
                }

                let maybe_latency = self.state.record_pong(ping_pong_msg.nonce, &node_id);
                debug!(
                    target: LOG_TARGET,
                    "Received pong from peer '{}' with useragent '{}'. {} (Trace: {})",
                    node_id.short_str(),
                    source_peer.user_agent,
                    maybe_latency
                        .map(|latency| format!("Latency: {:.2?}", latency))
                        .unwrap_or_default(),
                    message_tag,
                );

                let pong_event = PingPongEvent::new(node_id, maybe_latency, ping_pong_msg.metadata.into());
                self.publish_event(LivenessEvent::ReceivedPong(Box::new(pong_event)));
            },
        }
        Ok(())
    }

    async fn send_ping(&mut self, node_id: NodeId) -> Result<(), LivenessError> {
        let msg = PingPongMessage::ping_with_metadata(self.state.metadata().clone());
        self.state.add_inflight_ping(msg.nonce, node_id.clone());
        debug!(target: LOG_TARGET, "Sending ping to peer '{}'", node_id.short_str(),);

        self.outbound_messaging
            .send_direct_node_id(node_id, OutboundDomainMessage::new(TariMessageType::PingPong, msg))
            .await
            .map_err(Into::<DhtOutboundError>::into)?;

        Ok(())
    }

    async fn send_pong(&mut self, nonce: u64, dest: CommsPublicKey) -> Result<(), LivenessError> {
        let msg = PingPongMessage::pong_with_metadata(nonce, self.state.metadata().clone());
        self.outbound_messaging
            .send_direct(dest, OutboundDomainMessage::new(TariMessageType::PingPong, msg))
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
                let latency = self.state.get_avg_latency(&node_id);
                Ok(LivenessResponse::AvgLatency(latency))
            },
            GetNetworkAvgLatency => {
                let latency = self.state.get_network_avg_latency();
                Ok(LivenessResponse::AvgLatency(latency))
            },
            SetMetadataEntry(key, value) => {
                self.state.set_metadata_entry(key, value);
                Ok(LivenessResponse::Ok)
            },
            AddMonitoredPeer(node_id) => {
                let node_id_exists = { self.monitored_peers.read().await.iter().any(|val| val == &node_id) };
                if !node_id_exists {
                    self.monitored_peers.write().await.push(node_id.clone());
                }
                Ok(LivenessResponse::Ok)
            },
            RemoveMonitoredPeer(node_id) => {
                let node_id_exists = { self.monitored_peers.read().await.iter().position(|val| *val == node_id) };
                if let Some(pos) = node_id_exists {
                    self.monitored_peers.write().await.swap_remove(pos);
                }
                Ok(LivenessResponse::Ok)
            },
        }
    }

    async fn start_ping_round(&mut self) -> Result<(), LivenessError> {
        let monitored_peers = { self.monitored_peers.read().await.clone() };
        let selected_peers = self
            .connectivity
            .select_connections(ConnectivitySelection::random_nodes(
                self.config.num_peers_per_round,
                Default::default(),
            ))
            .await?
            .into_iter()
            .map(|c| c.peer_node_id().clone())
            .chain(monitored_peers)
            .collect::<Vec<_>>();

        if selected_peers.is_empty() {
            info!(
                target: LOG_TARGET,
                "Cannot broadcast pings because there are no broadcast peers available"
            )
        }

        let len_peers = selected_peers.len();
        debug!(target: LOG_TARGET, "Sending liveness ping to {} peer(s)", len_peers);

        for peer in selected_peers {
            let msg = PingPongMessage::ping_with_metadata(self.state.metadata().clone());
            self.state.add_inflight_ping(msg.nonce, peer.clone());
            self.outbound_messaging
                .send_direct_node_id(peer, OutboundDomainMessage::new(TariMessageType::PingPong, msg))
                .await?;
        }

        self.publish_event(LivenessEvent::PingRoundBroadcast(len_peers));

        Ok(())
    }

    async fn disconnect_failed_peers(&mut self) -> Result<(), LivenessError> {
        let max_allowed_ping_failures = self.config.max_allowed_ping_failures;
        for node_id in self
            .state
            .failed_pings_iter()
            .filter(|(_, n)| **n > max_allowed_ping_failures)
            .map(|(node_id, _)| node_id)
        {
            if let Some(mut conn) = self.connectivity.get_connection(node_id.clone()).await? {
                debug!(
                    target: LOG_TARGET,
                    "Disconnecting peer {} that failed {} rounds of pings", node_id, max_allowed_ping_failures
                );
                conn.disconnect().await?;
            }
        }
        self.state.clear_failed_pings();
        Ok(())
    }

    fn publish_event(&mut self, event: LivenessEvent) {
        let _ = self.event_publisher.send(Arc::new(event)).map_err(|_| {
            trace!(
                target: LOG_TARGET,
                "Could not publish LivenessEvent as there are no subscribers"
            )
        });
    }

    fn get_ping_count(&self) -> usize {
        self.state.pings_received()
    }

    fn get_pong_count(&self) -> usize {
        self.state.pongs_received()
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use futures::stream;
    use rand::rngs::OsRng;
    use tari_comms::{
        message::MessageTag,
        multiaddr::Multiaddr,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
        test_utils::mocks::create_connectivity_mock,
    };
    use tari_comms_dht::{
        envelope::{DhtMessageHeader, DhtMessageType},
        outbound::{DhtOutboundRequest, MessageSendState, SendMessageResponse},
        DhtProtocolVersion,
    };
    use tari_crypto::keys::PublicKey;
    use tari_service_framework::reply_channel;
    use tari_shutdown::Shutdown;
    use tokio::{
        sync::{broadcast, mpsc, oneshot},
        task,
    };

    use super::*;
    use crate::{
        proto::liveness::MetadataKey,
        services::liveness::{handle::LivenessHandle, state::Metadata},
    };

    #[tokio::test]
    async fn get_ping_pong_count() {
        let mut state = LivenessState::new();
        state.inc_pings_received();
        state.inc_pongs_received();
        state.inc_pongs_received();

        let (connectivity, mock) = create_connectivity_mock();
        mock.spawn();

        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        let (outbound_tx, _) = mpsc::channel(10);
        let outbound_messaging = OutboundMessageRequester::new(outbound_tx);

        // Setup liveness service
        let (sender_service, receiver) = reply_channel::unbounded();
        let (publisher, _) = broadcast::channel(200);

        let mut liveness_handle = LivenessHandle::new(sender_service, publisher.clone());

        let shutdown = Shutdown::new();
        let service = LivenessService::new(
            Default::default(),
            receiver,
            stream::empty(),
            state,
            connectivity,
            outbound_messaging,
            publisher,
            shutdown.to_signal(),
        );

        // Run the service
        task::spawn(service.run());

        let res = liveness_handle.get_ping_count().await.unwrap();
        assert_eq!(res, 1);

        let res = liveness_handle.get_pong_count().await.unwrap();
        assert_eq!(res, 2);
    }

    #[tokio::test]
    async fn send_ping() {
        let (connectivity, mock) = create_connectivity_mock();
        mock.spawn();
        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        let (outbound_tx, mut outbound_rx) = mpsc::channel(10);
        let outbound_messaging = OutboundMessageRequester::new(outbound_tx);

        // Setup liveness service
        let (sender_service, receiver) = reply_channel::unbounded();
        let (publisher, _) = broadcast::channel(200);
        let mut liveness_handle = LivenessHandle::new(sender_service, publisher.clone());

        let shutdown = Shutdown::new();
        let service = LivenessService::new(
            Default::default(),
            receiver,
            stream::empty(),
            LivenessState::default(),
            connectivity,
            outbound_messaging,
            publisher,
            shutdown.to_signal(),
        );

        // Run the LivenessService
        task::spawn(service.run());

        let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rngs::OsRng);
        let node_id = NodeId::from_key(&pk);
        // Receive outbound request
        task::spawn(async move {
            #[allow(clippy::single_match)]
            match outbound_rx.recv().await {
                Some(DhtOutboundRequest::SendMessage(_, _, reply_tx)) => {
                    let (_, rx) = oneshot::channel();
                    reply_tx
                        .send(SendMessageResponse::Queued(
                            vec![MessageSendState::new(MessageTag::new(), rx)].into(),
                        ))
                        .unwrap();
                },
                None => {},
            }
        });

        let _res = liveness_handle.send_ping(node_id).await.unwrap();
    }

    fn create_dummy_message<T>(inner: T) -> DomainMessage<T> {
        let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let source_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk),
            Vec::<Multiaddr>::new().into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            Default::default(),
            Default::default(),
        );
        DomainMessage {
            dht_header: DhtMessageHeader {
                version: DhtProtocolVersion::latest(),
                destination: Default::default(),
                origin_mac: Vec::new(),
                ephemeral_public_key: None,
                message_type: DhtMessageType::None,
                flags: Default::default(),
                message_tag: MessageTag::new(),
                expires: None,
            },
            authenticated_origin: None,
            source_peer,
            inner,
        }
    }

    #[tokio::test]
    async fn handle_message_ping() {
        let state = LivenessState::new();

        let (connectivity, mock) = create_connectivity_mock();
        mock.spawn();
        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        let (outbound_tx, mut outbound_rx) = mpsc::channel(10);
        let outbound_messaging = OutboundMessageRequester::new(outbound_tx);

        let metadata = Metadata::new();
        let msg = create_dummy_message(PingPongMessage::ping_with_metadata(metadata));
        // A stream which emits one message and then closes
        let pingpong_stream = stream::iter(std::iter::once(msg));

        // Setup liveness service
        let (publisher, _) = broadcast::channel(200);

        let shutdown = Shutdown::new();
        let service = LivenessService::new(
            Default::default(),
            stream::empty(),
            pingpong_stream,
            state,
            connectivity,
            outbound_messaging,
            publisher,
            shutdown.to_signal(),
        );

        task::spawn(service.run());

        // Test oms got request to send message
        unwrap_oms_send_msg!(outbound_rx.recv().await.unwrap());
    }

    #[tokio::test]
    async fn handle_message_pong() {
        let mut state = LivenessState::new();

        let (connectivity, mock) = create_connectivity_mock();
        mock.spawn();
        let (outbound_tx, _) = mpsc::channel(10);
        let outbound_messaging = OutboundMessageRequester::new(outbound_tx);

        let mut metadata = Metadata::new();
        metadata.insert(MetadataKey::ChainMetadata, b"dummy-data".to_vec());
        let msg = create_dummy_message(PingPongMessage::pong_with_metadata(123, metadata.clone()));

        state.add_inflight_ping(msg.inner.nonce, msg.source_peer.node_id.clone());
        // A stream which emits an inflight pong message and an unexpected one
        let malicious_msg = create_dummy_message(PingPongMessage::pong_with_metadata(321, metadata));
        let pingpong_stream = stream::iter(vec![msg, malicious_msg]);

        // Setup liveness service
        let (publisher, _) = broadcast::channel(200);
        let mut shutdown = Shutdown::new();
        let service = LivenessService::new(
            Default::default(),
            stream::empty(),
            pingpong_stream,
            state,
            connectivity,
            outbound_messaging,
            publisher.clone(),
            shutdown.to_signal(),
        );

        task::spawn(service.run());

        // Listen for the pong event
        let mut subscriber = publisher.subscribe();

        let event = time::timeout(Duration::from_secs(10), subscriber.recv())
            .await
            .unwrap()
            .unwrap();

        match &*event {
            LivenessEvent::ReceivedPong(event) => {
                assert_eq!(event.metadata.get(MetadataKey::ChainMetadata).unwrap(), b"dummy-data");
            },
            _ => panic!("Unexpected event"),
        }

        shutdown.trigger();

        // No further events (malicious_msg was ignored)
        let mut subscriber = publisher.subscribe();
        drop(publisher);
        let msg = subscriber.recv().await;
        assert!(msg.is_err());
    }
}
