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
    services::liveness::{handle::LivenessEventSender, peer_pool::PeerPool, LivenessEvent, PongEvent},
    tari_message::TariMessageType,
};
use futures::{future::Either, pin_mut, stream::StreamExt, Stream};
use log::*;
use std::{cmp, sync::Arc, time::Instant};
use tari_comms::{
    connection_manager::ConnectionManagerRequester,
    peer_manager::NodeId,
    types::CommsPublicKey,
    ConnectionManagerEvent,
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

/// Service responsible for testing Liveness of Peers.
pub struct LivenessService<THandleStream, TPingStream> {
    config: LivenessConfig,
    request_rx: Option<THandleStream>,
    ping_stream: Option<TPingStream>,
    state: LivenessState,
    dht_requester: DhtRequester,
    oms_handle: OutboundMessageRequester,
    event_publisher: LivenessEventSender,
    connection_manager: ConnectionManagerRequester,
    shutdown_signal: Option<ShutdownSignal>,
    neighbours: PeerPool,
    random_peers: PeerPool,
    active_pool: PeerPool,
}

impl<THandleStream, TPingStream> LivenessService<THandleStream, TPingStream>
where
    TPingStream: Stream<Item = DomainMessage<PingPongMessage>>,
    THandleStream: Stream<Item = RequestContext<LivenessRequest, Result<LivenessResponse, LivenessError>>>,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: LivenessConfig,
        request_rx: THandleStream,
        ping_stream: TPingStream,
        state: LivenessState,
        dht_requester: DhtRequester,
        connection_manager: ConnectionManagerRequester,
        oms_handle: OutboundMessageRequester,
        event_publisher: LivenessEventSender,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            request_rx: Some(request_rx),
            ping_stream: Some(ping_stream),
            state,
            dht_requester,
            oms_handle,
            connection_manager,
            event_publisher,
            shutdown_signal: Some(shutdown_signal),
            neighbours: PeerPool::new(config.refresh_neighbours_interval),
            random_peers: PeerPool::new(config.refresh_random_pool_interval),
            active_pool: PeerPool::new(config.refresh_neighbours_interval),
            config,
        }
    }

    pub async fn run(mut self) {
        info!(target: LOG_TARGET, "Liveness service started");
        debug!(target: LOG_TARGET, "Config = {:?}", self.config);
        let ping_stream = self.ping_stream.take().expect("ping_stream cannot be None").fuse();
        pin_mut!(ping_stream);

        let request_stream = self.request_rx.take().expect("ping_stream cannot be None").fuse();
        pin_mut!(request_stream);

        let mut ping_tick = match self.config.auto_ping_interval {
            Some(interval) => Either::Left(time::interval_at((Instant::now() + interval).into(), interval)),
            None => Either::Right(futures::stream::iter(Vec::new())),
        }
        .fuse();

        let mut connection_manager_events = self.connection_manager.get_event_subscription().fuse();

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

                event = connection_manager_events.select_next_some() => {
                    if let Ok(event) = event {
                        let _ = self.handle_connection_manager_event(&*event).await.or_else(|err| {
                            error!(target: LOG_TARGET, "Error when handling connection manager event: {:?}", err);
                            Err(err)
                        });
                    }
                },

                _ = ping_tick.select_next_some() => {
                    let _ = self.ping_active_pool().await.or_else(|err| {
                        error!(target: LOG_TARGET, "Error when pinging peers: {:?}", err);
                        Err(err)
                    });
                    let _ = self.ping_monitored_node_ids().await.or_else(|err| {
                        error!(target: LOG_TARGET, "Error when pinging monitored nodes: {:?}", err);
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
        let DomainMessage::<_> {
            source_peer,
            inner: ping_pong_msg,
            ..
        } = msg;
        let node_id = source_peer.node_id;
        let public_key = source_peer.public_key;

        match ping_pong_msg.kind().ok_or_else(|| LivenessError::InvalidPingPongType)? {
            PingPong::Ping => {
                self.state.inc_pings_received();
                trace!(target: LOG_TARGET, "Received ping from peer '{}'", node_id.short_str());
                self.send_pong(ping_pong_msg.nonce, public_key).await.unwrap();
                self.state.inc_pongs_sent();

                self.publish_event(LivenessEvent::ReceivedPing);
            },
            PingPong::Pong => {
                if !self.state.is_inflight(ping_pong_msg.nonce) {
                    warn!(
                        target: LOG_TARGET,
                        "Received Pong that was not requested from '{}'. Ignoring it.",
                        node_id.short_str()
                    );
                    return Ok(());
                }

                let is_neighbour = self.neighbours.contains(&node_id);
                self.refresh_peer_pools_if_stale().await?;
                let maybe_latency = self.state.record_pong(ping_pong_msg.nonce);
                let is_monitored = self.state.is_monitored_node_id(&node_id);

                trace!(
                    target: LOG_TARGET,
                    "Received pong from peer '{}'. {} {} {}",
                    node_id.short_str(),
                    maybe_latency.map(|ms| format!("Latency: {}ms", ms)).unwrap_or_default(),
                    if is_neighbour { "(neighbouring)" } else { "" },
                    if is_monitored { "(monitored)" } else { "" },
                );
                let pong_event = PongEvent::new(
                    node_id,
                    maybe_latency,
                    ping_pong_msg.metadata.into(),
                    is_neighbour,
                    is_monitored,
                );

                self.publish_event(LivenessEvent::ReceivedPong(Box::new(pong_event)));
            },
        }
        Ok(())
    }

    async fn handle_connection_manager_event(&mut self, event: &ConnectionManagerEvent) -> Result<(), LivenessError> {
        use ConnectionManagerEvent::*;
        match event {
            PeerDisconnected(node_id) | PeerConnectFailed(node_id, _) => {
                self.replace_failed_peer_if_required(node_id).await?;
            },
            _ => {},
        }

        Ok(())
    }

    async fn send_ping(&mut self, node_id: NodeId) -> Result<(), LivenessError> {
        let msg = PingPongMessage::ping();
        self.state.add_inflight_ping(msg.nonce, &node_id);
        trace!(target: LOG_TARGET, "Sending ping to peer '{}'", node_id.short_str());
        if self.neighbours.contains(&node_id) {
            trace!(
                target: LOG_TARGET,
                "Peer '{}' is a neighbouring peer",
                node_id.short_str()
            );
        }
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
            AddNodeId(node_id) => {
                self.state.add_node_id(&node_id);
                self.send_ping(node_id.clone()).await?;
                Ok(LivenessResponse::NodeIdAdded)
            },
            RemoveNodeId(node_id) => {
                self.state.remove_node_id(&node_id);
                Ok(LivenessResponse::NodeIdRemoved)
            },
            GetNodeIdStats(node_id) => self
                .state
                .get_node_id_stats(&node_id)
                .map(LivenessResponse::NodeIdStats),
            ClearNodeIds => {
                self.state.clear_node_ids();
                Ok(LivenessResponse::NodeIdsCleared)
            },
            GetBestMonitoredNodeId => Ok(LivenessResponse::BestMonitoredNodeId(self.state.get_best_node_id()?)),
        }
    }

    async fn replace_failed_peer_if_required(&mut self, node_id: &NodeId) -> Result<(), LivenessError> {
        if self.neighbours.contains(node_id) {
            self.refresh_neighbour_pool().await?;
            return Ok(());
        }

        if self.should_include_random_peers() && self.random_peers.contains(node_id) {
            // Replace the peer in the random peer pool with another random peer
            let excluded = self
                .neighbours
                .node_ids()
                .into_iter()
                .chain(vec![node_id])
                .cloned()
                .collect();

            if let Some(peer) = self
                .dht_requester
                .select_peers(BroadcastStrategy::Random(1, excluded))
                .await?
                .pop()
            {
                self.random_peers.remove(node_id);
                self.random_peers.push(peer.node_id)
            }
        }

        Ok(())
    }

    fn should_include_random_peers(&self) -> bool {
        self.config.random_peer_selection_ratio > 0.0
    }

    async fn refresh_peer_pools_if_stale(&mut self) -> Result<(), LivenessError> {
        let is_stale = self.neighbours.is_stale();
        if is_stale {
            self.refresh_neighbour_pool().await?;
        }

        if self.should_include_random_peers() && self.random_peers.is_stale() {
            self.refresh_random_peer_pool().await?;
        }

        if is_stale {
            self.refresh_active_peer_pool();

            info!(
                target: LOG_TARGET,
                "Selected {} active peers liveness neighbourhood out of a pool of {} neighbouring peers and {} random \
                 peers",
                self.active_pool.len(),
                self.neighbours.len(),
                self.random_peers.len()
            );
        }

        Ok(())
    }

    fn refresh_active_peer_pool(&mut self) {
        let rand_peer_ratio = 1.0f32.min(0.0f32.max(self.config.random_peer_selection_ratio));
        let desired_neighbours = (self.neighbours.len() as f32 * (1.0 - rand_peer_ratio)).ceil() as usize;
        let desired_random = (self.neighbours.len() as f32 * rand_peer_ratio).ceil() as usize;

        let num_random = cmp::min(desired_random, self.random_peers.len());
        let num_neighbours = self.neighbours.len() - num_random;
        debug!(
            target: LOG_TARGET,
            "Adding {} neighbouring peers (wanted = {}) and {} randomly selected (wanted = {}) peer(s) to active peer \
             pool",
            num_neighbours,
            desired_neighbours,
            num_random,
            desired_random
        );

        let mut active_node_ids = self.neighbours.sample(num_neighbours);
        active_node_ids.extend(self.random_peers.sample(num_random));
        self.active_pool
            .set_node_ids(active_node_ids.into_iter().cloned().collect());
        self.state.set_num_active_peers(self.active_pool.len());
    }

    async fn refresh_random_peer_pool(&mut self) -> Result<(), LivenessError> {
        let excluded = self.neighbours.node_ids().into_iter().cloned().collect();

        // Select a pool of random peers the same length as neighbouring peers
        let random_peers = self
            .dht_requester
            .select_peers(BroadcastStrategy::Random(self.neighbours.len(), excluded))
            .await?;

        if random_peers.is_empty() {
            warn!(target: LOG_TARGET, "No random peers selected for this round of pings");
        }
        let new_node_ids = random_peers.into_iter().map(|p| p.node_id).collect::<Vec<_>>();
        let removed = new_node_ids
            .iter()
            .filter(|n| self.random_peers.contains(*n))
            .collect::<Vec<_>>();
        debug!(target: LOG_TARGET, "Removed {} random peer(s)", removed.len());
        for node_id in removed {
            if let Err(err) = self.connection_manager.disconnect_peer(node_id.clone()).await {
                error!(target: LOG_TARGET, "Failed to disconnect peer: {:?}", err);
            }
        }

        self.random_peers.set_node_ids(new_node_ids);

        Ok(())
    }

    async fn refresh_neighbour_pool(&mut self) -> Result<(), LivenessError> {
        let neighbours = self
            .dht_requester
            .select_peers(BroadcastStrategy::Neighbours(Vec::new(), false))
            .await?;

        debug!(
            target: LOG_TARGET,
            "Setting active peers ({} peer(s))",
            neighbours.len()
        );
        self.neighbours
            .set_node_ids(neighbours.into_iter().map(|p| p.node_id).collect());

        Ok(())
    }

    async fn ping_active_pool(&mut self) -> Result<(), LivenessError> {
        self.refresh_peer_pools_if_stale().await?;
        let node_ids = self.active_pool.node_ids();
        let len_peers = node_ids.len();
        trace!(target: LOG_TARGET, "Sending liveness ping to {} peer(s)", len_peers);

        for node_id in node_ids {
            let msg = PingPongMessage::ping();
            self.state.add_inflight_ping(msg.nonce, &node_id);
            self.oms_handle
                .send_direct_node_id(
                    node_id.clone(),
                    OutboundEncryption::None,
                    OutboundDomainMessage::new(TariMessageType::PingPong, msg),
                )
                .await?;
        }

        self.publish_event(LivenessEvent::BroadcastedNeighbourPings(len_peers));

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

            self.publish_event(LivenessEvent::BroadcastedMonitoredNodeIdPings(num_nodes));
        }
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
    use super::*;
    use crate::{
        proto::liveness::MetadataKey,
        services::liveness::{handle::LivenessHandle, state::Metadata},
    };
    use futures::{channel::mpsc, stream, FutureExt};
    use rand::rngs::OsRng;
    use std::time::Duration;
    use tari_comms::{
        multiaddr::Multiaddr,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    };
    use tari_comms_dht::{
        envelope::{DhtMessageHeader, DhtMessageType, Network},
        outbound::{DhtOutboundRequest, SendMessageResponse},
        DhtRequest,
    };
    use tari_crypto::keys::PublicKey;
    use tari_service_framework::reply_channel;
    use tari_shutdown::Shutdown;
    use tokio::{sync::broadcast, task, time::delay_for};

    #[tokio_macros::test_basic]
    async fn get_ping_pong_count() {
        let state = LivenessState::new();
        state.inc_pings_received();
        state.inc_pongs_received();
        state.inc_pongs_received();

        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        let (outbound_tx, _) = mpsc::channel(10);
        let oms_handle = OutboundMessageRequester::new(outbound_tx);

        // Setup liveness service
        let (sender_service, receiver) = reply_channel::unbounded();
        let (publisher, _) = broadcast::channel(200);

        let mut liveness_handle = LivenessHandle::new(sender_service, publisher.clone());

        let (dht_tx, _) = mpsc::channel(10);
        let dht_requester = DhtRequester::new(dht_tx);

        let (tx, _) = mpsc::channel(0);
        let (event_tx, _) = broadcast::channel(1);
        let connection_manager = ConnectionManagerRequester::new(tx, event_tx);

        let shutdown = Shutdown::new();
        let service = LivenessService::new(
            Default::default(),
            receiver,
            stream::empty(),
            state,
            dht_requester,
            connection_manager,
            oms_handle,
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

    #[tokio_macros::test]
    async fn send_ping() {
        let state = LivenessState::new();

        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        let (outbound_tx, mut outbound_rx) = mpsc::channel(10);
        let oms_handle = OutboundMessageRequester::new(outbound_tx);

        // Setup liveness service
        let (sender_service, receiver) = reply_channel::unbounded();
        let (publisher, _) = broadcast::channel(200);
        let mut liveness_handle = LivenessHandle::new(sender_service, publisher.clone());

        let (dht_tx, _) = mpsc::channel(10);
        let dht_requester = DhtRequester::new(dht_tx);

        let (tx, _) = mpsc::channel(0);
        let (event_tx, _) = broadcast::channel(1);
        let connection_manager = ConnectionManagerRequester::new(tx, event_tx);

        let shutdown = Shutdown::new();
        let service = LivenessService::new(
            Default::default(),
            receiver,
            stream::empty(),
            state,
            dht_requester,
            connection_manager,
            oms_handle,
            publisher,
            shutdown.to_signal(),
        );

        // Run the LivenessService
        task::spawn(service.run());

        let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rngs::OsRng);
        let node_id = NodeId::from_key(&pk).unwrap();
        // Receive outbound request
        task::spawn(async move {
            match outbound_rx.select_next_some().await {
                DhtOutboundRequest::SendMessage(_, _, reply_tx) => {
                    reply_tx.send(SendMessageResponse::Queued(vec![].into())).unwrap();
                },
            }
        });

        let _res = liveness_handle.send_ping(node_id).await.unwrap();
    }

    fn create_dummy_message<T>(inner: T) -> DomainMessage<T> {
        let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let source_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk).unwrap(),
            Vec::<Multiaddr>::new().into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            &[],
        );
        DomainMessage {
            dht_header: DhtMessageHeader {
                version: 0,
                destination: Default::default(),
                origin_mac: Vec::new(),
                ephemeral_public_key: None,
                message_type: DhtMessageType::None,
                network: Network::LocalTest,
                flags: Default::default(),
            },
            authenticated_origin: None,
            source_peer,
            inner,
        }
    }

    #[tokio_macros::test]
    async fn handle_message_ping() {
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
        let (publisher, _) = broadcast::channel(200);

        let (tx, _) = mpsc::channel(0);
        let (event_tx, _) = broadcast::channel(1);
        let connection_manager = ConnectionManagerRequester::new(tx, event_tx);
        let shutdown = Shutdown::new();
        let service = LivenessService::new(
            Default::default(),
            stream::empty(),
            pingpong_stream,
            state,
            dht_requester,
            connection_manager,
            oms_handle,
            publisher,
            shutdown.to_signal(),
        );

        task::spawn(service.run());

        // Test oms got request to send message
        unwrap_oms_send_msg!(outbound_rx.select_next_some().await);
    }

    #[tokio_macros::test_basic]
    async fn handle_message_pong() {
        let mut state = LivenessState::new();

        let (outbound_tx, _) = mpsc::channel(10);
        let oms_handle = OutboundMessageRequester::new(outbound_tx);

        let mut metadata = Metadata::new();
        metadata.insert(MetadataKey::ChainMetadata, b"dummy-data".to_vec());
        let msg = create_dummy_message(PingPongMessage::pong_with_metadata(123, metadata.clone()));
        let peer = msg.source_peer.clone();

        state.add_inflight_ping(msg.inner.nonce, &msg.source_peer.node_id);
        // A stream which emits an inflight pong message and an unexpected one
        let malicious_msg = create_dummy_message(PingPongMessage::pong_with_metadata(321, metadata));
        let pingpong_stream = stream::iter(vec![msg, malicious_msg]);

        let (dht_tx, mut dht_rx) = mpsc::channel(10);
        let dht_requester = DhtRequester::new(dht_tx);

        // TODO: create mock
        task::spawn(async move {
            use DhtRequest::*;
            while let Some(req) = dht_rx.next().await {
                match req {
                    SelectPeers(_, reply_tx) => {
                        reply_tx.send(vec![peer.clone()]).unwrap();
                    },
                    _ => panic!("unexpected request {:?}", req),
                }
            }
        });

        let (tx, _) = mpsc::channel(0);
        let (event_tx, _) = broadcast::channel(1);
        let connection_manager = ConnectionManagerRequester::new(tx, event_tx);

        // Setup liveness service
        let (publisher, _) = broadcast::channel(200);
        let mut shutdown = Shutdown::new();
        let service = LivenessService::new(
            Default::default(),
            stream::empty(),
            pingpong_stream,
            state,
            dht_requester,
            connection_manager,
            oms_handle,
            publisher.clone(),
            shutdown.to_signal(),
        );

        task::spawn(service.run());

        // Listen for the pong event
        let subscriber = publisher.subscribe();

        let event = time::timeout(Duration::from_secs(10), subscriber.fuse().select_next_some())
            .await
            .unwrap()
            .unwrap();

        match &*event {
            LivenessEvent::ReceivedPong(event) => {
                assert_eq!(event.metadata.get(MetadataKey::ChainMetadata).unwrap(), b"dummy-data");
            },
            _ => panic!("Unexpected event"),
        }

        shutdown.trigger().unwrap();

        // No further events (malicious_msg was ignored)
        let mut subscriber = publisher.subscribe().fuse();

        let mut delay = delay_for(Duration::from_secs(10)).fuse();
        let mut count: i32 = 0;
        loop {
            futures::select! {
                _ = subscriber.select_next_some() => {
                    count+=1;
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(count, 0);
    }
}
