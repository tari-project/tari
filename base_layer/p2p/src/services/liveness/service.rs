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

use std::{collections::HashSet, iter, sync::Arc, time::Instant};

use futures::{future::Either, pin_mut, stream::StreamExt, Stream};
use log::*;
use tari_network::{identity::PeerId, NetworkHandle, NetworkingService, OutboundMessager, OutboundMessaging};
use tari_service_framework::reply_channel::RequestContext;
use tari_shutdown::ShutdownSignal;
use tokio::{sync::mpsc, time, time::MissedTickBehavior};
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
    message::{DomainMessage, TariNodeMessageSpec},
    proto::message::TariMessage,
    services::liveness::{handle::LivenessEventSender, LivenessEvent, PingPongEvent},
};

/// Service responsible for testing Liveness of Peers.
pub struct LivenessService<THandleStream> {
    config: LivenessConfig,
    request_rx: Option<THandleStream>,
    ping_stream: Option<mpsc::UnboundedReceiver<DomainMessage<TariMessage>>>,
    state: LivenessState,
    network: NetworkHandle,
    outbound_messaging: OutboundMessaging<TariNodeMessageSpec>,
    event_publisher: LivenessEventSender,
    shutdown_signal: ShutdownSignal,
    monitored_peers: HashSet<PeerId>,
}

impl<TRequestStream> LivenessService<TRequestStream>
where TRequestStream: Stream<Item = RequestContext<LivenessRequest, Result<LivenessResponse, LivenessError>>>
{
    pub fn new(
        config: LivenessConfig,
        request_rx: TRequestStream,
        ping_stream: mpsc::UnboundedReceiver<DomainMessage<TariMessage>>,
        state: LivenessState,
        network: NetworkHandle,
        outbound_messaging: OutboundMessaging<TariNodeMessageSpec>,
        event_publisher: LivenessEventSender,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            request_rx: Some(request_rx),
            ping_stream: Some(ping_stream),
            state,
            network,
            outbound_messaging,
            event_publisher,
            shutdown_signal,
            config: config.clone(),
            monitored_peers: config.monitored_peers.into_iter().collect(),
        }
    }

    pub async fn run(mut self) {
        debug!(target: LOG_TARGET, "Liveness service started");
        debug!(target: LOG_TARGET, "Config = {:?}", self.config);
        let mut ping_stream = self.ping_stream.take().expect("ping_stream cannot be None");

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
                    let _result = reply_tx.send(self.handle_request(request).await);
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
                Some(msg) = ping_stream.recv() => {
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

    async fn handle_incoming_message(&mut self, msg: DomainMessage<TariMessage>) -> Result<(), LivenessError> {
        let DomainMessage::<_> {
            source_peer_id,
            header,
            payload,
            ..
        } = msg;

        let ping_pong_msg = payload
            .into_ping_pong()
            .expect("Dispatch should only send PingPong messages to the Liveness service");

        let message_tag = header.message_tag;

        match ping_pong_msg.kind().ok_or(LivenessError::InvalidPingPongType)? {
            PingPong::Ping => {
                self.state.inc_pings_received();
                self.send_pong(ping_pong_msg.nonce, source_peer_id).await?;
                self.state.inc_pongs_sent();

                debug!(
                    target: LOG_TARGET,
                    "Received ping from peer '{}' (Trace: {})",
                    source_peer_id,
                    message_tag,
                );

                let ping_event = PingPongEvent::new(source_peer_id, None, ping_pong_msg.metadata.into());
                self.publish_event(LivenessEvent::ReceivedPing(Box::new(ping_event)));
            },
            PingPong::Pong => {
                if !self.state.is_inflight(ping_pong_msg.nonce) {
                    debug!(
                        target: LOG_TARGET,
                        "Received Pong that was not requested from '{}'. Ignoring it. (Trace: {})",
                        source_peer_id,
                        message_tag,
                    );
                    return Ok(());
                }

                let maybe_latency = self.state.record_pong(ping_pong_msg.nonce, &source_peer_id);
                debug!(
                    target: LOG_TARGET,
                    "Received pong from peer '{}' (Latency: {}, Trace: {})",
                    source_peer_id,
                    maybe_latency
                        .map(|latency| format!("{:.2?}", latency))
                        .unwrap_or_else(|| "None".to_string()),
                    message_tag,
                );

                let pong_event = PingPongEvent::new(source_peer_id, maybe_latency, ping_pong_msg.metadata.into());
                self.publish_event(LivenessEvent::ReceivedPong(Box::new(pong_event)));
            },
        }
        Ok(())
    }

    async fn send_ping(&mut self, peer_id: PeerId) -> Result<(), LivenessError> {
        let msg = PingPongMessage::ping_with_metadata(self.state.metadata().clone());
        self.state.add_inflight_ping(msg.nonce, peer_id);
        debug!(target: LOG_TARGET, "Sending ping to peer '{}'", peer_id);
        self.outbound_messaging.send_message(peer_id, msg).await?;
        Ok(())
    }

    async fn send_pong(&mut self, nonce: u64, dest: PeerId) -> Result<(), LivenessError> {
        let msg = PingPongMessage::pong_with_metadata(nonce, self.state.metadata().clone());
        self.outbound_messaging.send_message(dest, msg).await?;
        Ok(())
    }

    async fn handle_request(&mut self, request: LivenessRequest) -> Result<LivenessResponse, LivenessError> {
        #[allow(clippy::enum_glob_use)]
        use LivenessRequest::*;
        match request {
            SendPing(peer_id) => {
                self.send_ping(peer_id).await?;
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
            AddMonitoredPeer(peer_id) => {
                self.monitored_peers.insert(peer_id);
                Ok(LivenessResponse::Ok)
            },
            RemoveMonitoredPeer(peer_id) => {
                self.monitored_peers.remove(&peer_id);
                Ok(LivenessResponse::Ok)
            },
        }
    }

    async fn start_ping_round(&mut self) -> Result<(), LivenessError> {
        let selected_conns = self
            .network
            .select_active_connections(None, Some(self.config.num_peers_per_round), true, Default::default())
            .await?;

        if selected_conns.is_empty() && self.monitored_peers.is_empty() {
            debug!(
                target: LOG_TARGET,
                "Cannot broadcast pings because there are no broadcast peers available"
            )
        }

        let mut count = 0usize;
        let iter = selected_conns
            .into_iter()
            .map(|conn| conn.peer_id)
            .chain(self.monitored_peers.iter().copied());

        for peer_id in iter {
            let msg = PingPongMessage::ping_with_metadata(self.state.metadata().clone());
            self.state.add_inflight_ping(msg.nonce, peer_id);
            self.outbound_messaging.send_message(peer_id, msg).await?;
            count += 1;
        }

        self.publish_event(LivenessEvent::PingRoundBroadcast(count));

        Ok(())
    }

    async fn disconnect_failed_peers(&mut self) -> Result<(), LivenessError> {
        let max_allowed_ping_failures = self.config.max_allowed_ping_failures;
        for peer_id in self
            .state
            .failed_pings_iter()
            .filter(|(_, n)| **n > max_allowed_ping_failures)
            .map(|(node_id, _)| node_id)
        {
            if self.network.disconnect_peer(*peer_id).await? {
                debug!(
                    target: LOG_TARGET,
                    "Disconnected peer {} that failed {} rounds of pings", peer_id, max_allowed_ping_failures
                );
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

// #[cfg(test)]
// mod test {
//     use std::time::Duration;
//
//     use futures::stream;
//     use rand::rngs::OsRng;
//     use tari_comms::{
//         message::MessageTag,
//         net_address::MultiaddressesWithStats,
//         peer_manager::{Peer, PeerFeatures, PeerFlags},
//         test_utils::mocks::create_connectivity_mock,
//         types::CommsDatabase,
//     };
//     use tari_comms_dht::{
//         envelope::{DhtMessageHeader, DhtMessageType},
//         outbound::{DhtOutboundRequest, MessageSendState, SendMessageResponse},
//         DhtProtocolVersion,
//     };
//     use tari_crypto::keys::PublicKey;
//     use tari_service_framework::reply_channel;
//     use tari_shutdown::Shutdown;
//     use tari_storage::lmdb_store::{LMDBBuilder, LMDBConfig};
//     use tari_test_utils::{paths::create_temporary_data_path, random};
//     use tokio::{
//         sync::{broadcast, mpsc, oneshot},
//         task,
//     };
//
//     use super::*;
//     use crate::{
//         proto::liveness::MetadataKey,
//         services::liveness::{handle::LivenessHandle, state::Metadata},
//     };
//
//     pub fn build_peer_manager() -> Arc<PeerManager> {
//         let database_name = random::string(8);
//         let path = create_temporary_data_path();
//         let datastore = LMDBBuilder::new()
//             .set_path(path.to_str().unwrap())
//             .set_env_config(LMDBConfig::default())
//             .set_max_number_of_databases(1)
//             .add_database(&database_name, lmdb_zero::db::CREATE)
//             .build()
//             .unwrap();
//
//         let peer_database = datastore.get_handle(&database_name).unwrap();
//
//         PeerManager::new(CommsDatabase::new(Arc::new(peer_database)), None)
//             .map(Arc::new)
//             .unwrap()
//     }
//
//     #[tokio::test]
//     async fn get_ping_pong_count() {
//         let mut state = LivenessState::new();
//         state.inc_pings_received();
//         state.inc_pongs_received();
//         state.inc_pongs_received();
//
//         let (network, mock) = create_connectivity_mock();
//         mock.spawn();
//
//         // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
//         let (outbound_tx, _) = mpsc::channel(10);
//         let outbound_messaging = OutboundMessageRequester::new(outbound_tx);
//
//         // Setup liveness service
//         let (sender_service, receiver) = reply_channel::unbounded();
//         let (publisher, _) = broadcast::channel(200);
//
//         let mut liveness_handle = LivenessHandle::new(sender_service, publisher.clone());
//
//         let shutdown = Shutdown::new();
//         let service = LivenessService::new(
//             Default::default(),
//             receiver,
//             stream::empty(),
//             state,
//             network,
//             outbound_messaging,
//             publisher,
//             shutdown.to_signal(),
//             build_peer_manager(),
//         );
//
//         // Run the service
//         task::spawn(service.run());
//
//         let res = liveness_handle.get_ping_count().await.unwrap();
//         assert_eq!(res, 1);
//
//         let res = liveness_handle.get_pong_count().await.unwrap();
//         assert_eq!(res, 2);
//     }
//
//     #[tokio::test]
//     async fn send_ping() {
//         let (network, mock) = create_connectivity_mock();
//         mock.spawn();
//         // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
//         let (outbound_tx, mut outbound_rx) = mpsc::channel(10);
//         let outbound_messaging = OutboundMessageRequester::new(outbound_tx);
//
//         // Setup liveness service
//         let (sender_service, receiver) = reply_channel::unbounded();
//         let (publisher, _) = broadcast::channel(200);
//         let mut liveness_handle = LivenessHandle::new(sender_service, publisher.clone());
//
//         let shutdown = Shutdown::new();
//         let service = LivenessService::new(
//             Default::default(),
//             receiver,
//             stream::empty(),
//             LivenessState::default(),
//             network,
//             outbound_messaging,
//             publisher,
//             shutdown.to_signal(),
//             build_peer_manager(),
//         );
//
//         // Run the LivenessService
//         task::spawn(service.run());
//
//         let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rngs::OsRng);
//         let node_id = NodeId::from_key(&pk);
//         // Receive outbound request
//         task::spawn(async move {
//             #[allow(clippy::single_match)]
//             match outbound_rx.recv().await {
//                 Some(DhtOutboundRequest::SendMessage(_, _, reply_tx)) => {
//                     let (_, rx) = oneshot::channel();
//                     reply_tx
//                         .send(SendMessageResponse::Queued(
//                             vec![MessageSendState::new(MessageTag::new(), rx)].into(),
//                         ))
//                         .unwrap();
//                 },
//                 None => {},
//             }
//         });
//
//         liveness_handle.send_ping(node_id).await.unwrap();
//     }
//
//     fn create_dummy_message<T>(inner: T) -> DomainMessage<Result<T, prost::DecodeError>> {
//         let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
//         let source_peer = Peer::new(
//             pk.clone(),
//             NodeId::from_key(&pk),
//             MultiaddressesWithStats::empty(),
//             PeerFlags::empty(),
//             PeerFeatures::COMMUNICATION_NODE,
//             Default::default(),
//             Default::default(),
//         );
//         DomainMessage {
//             dht_header: DhtMessageHeader {
//                 version: DhtProtocolVersion::latest(),
//                 destination: Default::default(),
//                 message_signature: Vec::new(),
//                 ephemeral_public_key: None,
//                 message_type: DhtMessageType::None,
//                 flags: Default::default(),
//                 message_tag: MessageTag::new(),
//                 expires: None,
//             },
//             authenticated_origin: None,
//             source_peer,
//             inner: Ok(inner),
//         }
//     }
//
//     #[tokio::test]
//     async fn handle_message_ping() {
//         let state = LivenessState::new();
//
//         let (network, mock) = create_connectivity_mock();
//         mock.spawn();
//         // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
//         let (outbound_tx, mut outbound_rx) = mpsc::channel(10);
//         let outbound_messaging = OutboundMessageRequester::new(outbound_tx);
//
//         let metadata = Metadata::new();
//         let msg = create_dummy_message(PingPongMessage::ping_with_metadata(metadata));
//         // A stream which emits one message and then closes
//         let pingpong_stream = stream::iter(std::iter::once(msg));
//
//         // Setup liveness service
//         let (publisher, _) = broadcast::channel(200);
//
//         let shutdown = Shutdown::new();
//
//         let service = LivenessService::new(
//             Default::default(),
//             stream::empty(),
//             pingpong_stream,
//             state,
//             network,
//             outbound_messaging,
//             publisher,
//             shutdown.to_signal(),
//             build_peer_manager(),
//         );
//
//         task::spawn(service.run());
//
//         // Test oms got request to send message
//         unwrap_oms_send_msg!(outbound_rx.recv().await.unwrap());
//     }
//
//     #[tokio::test]
//     async fn handle_message_pong() {
//         let mut state = LivenessState::new();
//
//         let (network, mock) = create_connectivity_mock();
//         mock.spawn();
//         let (outbound_tx, _) = mpsc::channel(10);
//         let outbound_messaging = OutboundMessageRequester::new(outbound_tx);
//
//         let mut metadata = Metadata::new();
//         metadata.insert(MetadataKey::ChainMetadata, b"dummy-data".to_vec());
//         let msg = create_dummy_message(PingPongMessage::pong_with_metadata(123, metadata.clone()));
//
//         state.add_inflight_ping(
//             msg.inner.as_ref().map(|i| i.nonce).unwrap(),
//             msg.source_peer.node_id.clone(),
//         );
//
//         // A stream which emits an inflight pong message and an unexpected one
//         let malicious_msg = create_dummy_message(PingPongMessage::pong_with_metadata(321, metadata));
//         let pingpong_stream = stream::iter(vec![msg, malicious_msg]);
//
//         // Setup liveness service
//         let (publisher, _) = broadcast::channel(200);
//         let mut shutdown = Shutdown::new();
//         let service = LivenessService::new(
//             Default::default(),
//             stream::empty(),
//             pingpong_stream,
//             state,
//             network,
//             outbound_messaging,
//             publisher.clone(),
//             shutdown.to_signal(),
//             build_peer_manager(),
//         );
//
//         task::spawn(service.run());
//
//         // Listen for the pong event
//         let mut subscriber = publisher.subscribe();
//
//         let event = time::timeout(Duration::from_secs(10), subscriber.recv())
//             .await
//             .unwrap()
//             .unwrap();
//
//         match &*event {
//             LivenessEvent::ReceivedPong(event) => {
//                 assert_eq!(event.metadata.get(MetadataKey::ChainMetadata).unwrap(), b"dummy-data");
//             },
//             _ => panic!("Unexpected event"),
//         }
//
//         shutdown.trigger();
//
//         // No further events (malicious_msg was ignored)
//         let mut subscriber = publisher.subscribe();
//         drop(publisher);
//         let msg = subscriber.recv().await;
//         assert!(msg.is_err());
//     }
// }
