//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::{hash_map::Entry, HashMap},
    hash::Hash,
    sync::Arc,
    time::{Duration, Instant},
};

use libp2p::{
    autonat,
    autonat::NatStatus,
    core::ConnectedPoint,
    dcutr,
    futures::StreamExt,
    gossipsub,
    gossipsub::{IdentTopic, MessageId, TopicHash},
    identify,
    identity,
    kad::RoutingUpdate,
    mdns,
    multiaddr::Protocol,
    ping,
    relay,
    swarm::{
        dial_opts::{DialOpts, PeerCondition},
        ConnectionId,
        DialError,
        SwarmEvent,
    },
    Multiaddr,
    PeerId,
    StreamProtocol,
};
use log::*;
use rand::{prelude::IteratorRandom, rngs::OsRng};
use tari_rpc_framework::Substream;
use tari_shutdown::ShutdownSignal;
use tari_swarm::{
    messaging,
    messaging::{prost, prost::ProstCodec},
    peersync,
    substream,
    substream::{NegotiatedSubstream, ProtocolNotification, StreamId},
    TariNodeBehaviourEvent,
    TariSwarm,
};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time,
};

use crate::{
    connection::Connection,
    event::NetworkEvent,
    global_ip::GlobalIp,
    handle::NetworkingRequest,
    messaging::MessagingRequest,
    notify::Notifiers,
    relay_state::RelayState,
    BannedPeer,
    ConnectionDirection,
    MessageSpec,
    MessagingMode,
    NetworkError,
    Peer,
};

const LOG_TARGET: &str = "tari::network::service::worker";

type ReplyTx<T> = oneshot::Sender<Result<T, NetworkError>>;

const PEER_ANNOUNCE_TOPIC: &str = "peer-announce";

pub struct NetworkingWorker<TMsg>
where
    TMsg: MessageSpec,
    TMsg::Message: prost::Message + Default + Clone + 'static,
{
    keypair: identity::Keypair,
    rx_request: mpsc::Receiver<NetworkingRequest>,
    rx_msg_request: mpsc::Receiver<MessagingRequest<TMsg>>,
    tx_events: broadcast::Sender<NetworkEvent>,
    messaging_mode: MessagingMode<TMsg>,
    active_connections: HashMap<PeerId, Vec<Connection>>,
    pending_substream_requests: HashMap<StreamId, ReplyTx<NegotiatedSubstream<Substream>>>,
    pending_dial_requests: HashMap<PeerId, Vec<ReplyTx<()>>>,
    substream_notifiers: Notifiers<Substream>,
    swarm: TariSwarm<ProstCodec<TMsg::Message>>,
    // TODO: we'll replace this with a proper libp2p behaviour if needed
    ban_list: HashMap<PeerId, BannedPeer>,
    gossipsub_subscriptions: HashMap<TopicHash, mpsc::UnboundedSender<(PeerId, gossipsub::Message)>>,
    gossipsub_outbound_tx: mpsc::Sender<(IdentTopic, Vec<u8>)>,
    gossipsub_outbound_rx: Option<mpsc::Receiver<(IdentTopic, Vec<u8>)>>,
    config: crate::Config,
    relays: RelayState,
    seed_peers: Vec<Peer>,
    is_initial_bootstrap_complete: bool,
    has_sent_announce: bool,
    shutdown_signal: ShutdownSignal,
}

impl<TMsg> NetworkingWorker<TMsg>
where
    TMsg: MessageSpec,
    TMsg::Message: prost::Message + Default + Clone + 'static,
{
    pub(crate) fn new(
        keypair: identity::Keypair,
        rx_request: mpsc::Receiver<NetworkingRequest>,
        rx_msg_request: mpsc::Receiver<MessagingRequest<TMsg>>,
        tx_events: broadcast::Sender<NetworkEvent>,
        messaging_mode: MessagingMode<TMsg>,
        swarm: TariSwarm<ProstCodec<TMsg::Message>>,
        config: crate::Config,
        seed_peers: Vec<Peer>,
        known_relay_nodes: Vec<Peer>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        let (gossipsub_outbound_tx, gossipsub_outbound_rx) = mpsc::channel(100);
        Self {
            keypair,
            rx_request,
            rx_msg_request,
            tx_events,
            messaging_mode,
            substream_notifiers: Notifiers::new(),
            active_connections: HashMap::new(),
            pending_substream_requests: HashMap::new(),
            pending_dial_requests: HashMap::new(),
            relays: RelayState::new(known_relay_nodes),
            seed_peers,
            swarm,
            ban_list: HashMap::new(),
            gossipsub_subscriptions: HashMap::new(),
            gossipsub_outbound_tx,
            gossipsub_outbound_rx: Some(gossipsub_outbound_rx),
            config,
            is_initial_bootstrap_complete: false,
            has_sent_announce: false,
            shutdown_signal,
        }
    }

    pub fn add_protocol_notifier(
        &mut self,
        protocol: StreamProtocol,
        sender: mpsc::UnboundedSender<ProtocolNotification<Substream>>,
    ) {
        self.substream_notifiers.add(protocol, sender);
    }

    fn listen(&mut self) -> Result<(), NetworkError> {
        for addr in &self.config.listener_addrs {
            debug!("listening on {addr}");
            self.swarm.listen_on(addr.clone())?;
        }
        Ok(())
    }

    pub async fn run(mut self) -> Result<(), NetworkError> {
        info!(target: LOG_TARGET, "🌐 Starting networking service {:?}", self.config);
        self.add_all_seed_peers();

        self.listen()?;

        if self.config.reachability_mode.is_private() {
            self.attempt_relay_reservation();
        }

        let mut check_connections_interval = time::interval(self.config.check_connections_interval);

        self.swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&IdentTopic::new(PEER_ANNOUNCE_TOPIC))?;
        let mut gossipsub_outbound = self.gossipsub_outbound_rx.take().expect("Only taken once");

        loop {
            tokio::select! {
                Some(request) = self.rx_request.recv() => {
                    if let Err(err) = self.handle_request(request).await {
                        error!(target: LOG_TARGET, "Error handling request: {err}");
                    }
                }
                Some(request) = self.rx_msg_request.recv() => {
                    self.handle_messaging_request(request).await;
                },

                Some(event) = self.swarm.next() => {
                    if let Err(err) = self.on_swarm_event(event).await {
                        error!(target: LOG_TARGET, "🚨 Swarm event error: {}", err);
                    }
                },
                _ =  check_connections_interval.tick() => {
                    if let Err(err) = self.bootstrap().await {
                        error!(target: LOG_TARGET, "🚨 Failed to bootstrap: {}", err);
                    }
                },

                Some((topic, msg)) = gossipsub_outbound.recv() => {
                    debug!(target: LOG_TARGET, "📣 Gossip publish {topic} {} bytes", msg.len());
                    if let Err(err) = self.swarm.behaviour_mut().gossipsub.publish(topic, msg) {
                        error!(target: LOG_TARGET, "🚨 Failed to publish gossip message: {}", err);
                    }
                }

                _ = self.shutdown_signal.wait() => {
                    break;
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_request(&mut self, request: NetworkingRequest) -> Result<(), NetworkError> {
        match request {
            NetworkingRequest::DialPeer { dial_opts, reply_tx } => {
                let (tx_waiter, rx_waiter) = oneshot::channel();
                let maybe_peer_id = dial_opts.get_peer_id();
                info!(target: LOG_TARGET, "🤝 Dialing peer {:?}", dial_opts);

                match self.swarm.dial(dial_opts) {
                    Ok(_) => {
                        if let Some(peer_id) = maybe_peer_id {
                            self.pending_dial_requests.entry(peer_id).or_default().push(tx_waiter);
                        }
                        let _ignore = reply_tx.send(Ok(rx_waiter.into()));
                    },
                    Err(err) => {
                        info!(target: LOG_TARGET, "🚨 Failed to dial peer: {}",  err);
                        let _ignore = reply_tx.send(Err(err.into()));
                    },
                }
            },
            NetworkingRequest::DisconnectPeer { peer_id, reply } => {
                let _ignore = reply.send(Ok(self.swarm.disconnect_peer_id(peer_id).is_ok()));
            },
            NetworkingRequest::PublishGossip {
                topic,
                message,
                reply_tx,
            } => match self.swarm.behaviour_mut().gossipsub.publish(topic, message) {
                Ok(msg_id) => {
                    debug!(target: LOG_TARGET, "📢 Published gossipsub message: {}", msg_id);
                    let _ignore = reply_tx.send(Ok(()));
                },
                Err(err) => {
                    debug!(target: LOG_TARGET, "🚨 Failed to publish gossipsub message: {}", err);
                    let _ignore = reply_tx.send(Err(err.into()));
                },
            },
            NetworkingRequest::SubscribeTopic { topic, inbound, reply } => {
                let result = self.gossipsub_subscribe_topic(topic, inbound);
                let _ignore = reply.send(result.map(|_| self.gossipsub_outbound_tx.clone()));
            },
            NetworkingRequest::UnsubscribeTopic { topic, reply_tx } => {
                self.gossipsub_subscriptions.remove(&topic.hash());

                match self.swarm.behaviour_mut().gossipsub.unsubscribe(&topic) {
                    Ok(_) => {
                        debug!(target: LOG_TARGET, "📢 Unsubscribed from gossipsub topic: {}", topic);
                        let _ignore = reply_tx.send(Ok(()));
                    },
                    Err(err) => {
                        error!(target: LOG_TARGET, "🚨 Failed to unsubscribe from gossipsub topic: {}", err);
                        let _ignore = reply_tx.send(Err(err.into()));
                    },
                }
            },
            NetworkingRequest::IsSubscribedTopic { topic, reply_tx } => {
                let hash = topic.hash();
                let found = self.swarm.behaviour_mut().gossipsub.topics().any(|t| *t == hash);
                let _ignore = reply_tx.send(Ok(found));
            },
            NetworkingRequest::OpenSubstream {
                peer_id,
                protocol_id,
                reply_tx,
            } => {
                let stream_id = self
                    .swarm
                    .behaviour_mut()
                    .substream
                    .open_substream(peer_id, protocol_id.clone());
                self.pending_substream_requests.insert(stream_id, reply_tx);
            },
            NetworkingRequest::AddProtocolNotifier { protocols, tx_notifier } => {
                for protocol in protocols {
                    self.add_protocol_notifier(protocol.clone(), tx_notifier.clone());
                    self.swarm.behaviour_mut().substream.add_protocol(protocol);
                }
            },
            NetworkingRequest::SelectActiveConnections {
                with_peers,
                limit,
                randomize,
                exclude_peers: excluded_peers,
                reply_tx,
            } => {
                let iter = self
                    .active_connections
                    .values()
                    .flatten()
                    .filter(|c| with_peers.as_ref().map_or(true, |p| p.contains(&c.peer_id)))
                    .filter(|c| !excluded_peers.contains(&c.peer_id))
                    .cloned();
                let connections = if randomize {
                    iter.choose_multiple(&mut OsRng, limit.unwrap_or(self.active_connections.len()))
                } else if let Some(limit) = limit {
                    iter.take(limit).collect()
                } else {
                    iter.collect()
                };

                let _ignore = reply_tx.send(Ok(connections));
            },
            NetworkingRequest::GetLocalPeerInfo { reply_tx } => {
                let peer = crate::peer::PeerInfo {
                    peer_id: *self.swarm.local_peer_id(),
                    public_key: self.keypair.public(),
                    protocol_version: self.config.swarm.protocol_version.to_string(),
                    agent_version: self.config.swarm.user_agent.clone(),
                    listen_addrs: self.swarm.listeners().cloned().collect(),
                    // TODO: this isnt all the protocols, not sure if there is an easy way to get them all
                    protocols: self.swarm.behaviour_mut().substream.supported_protocols().to_vec(),
                    // observed_addr: (),
                };
                let _ignore = reply_tx.send(Ok(peer));
            },
            NetworkingRequest::SetWantPeers(peers) => {
                info!(target: LOG_TARGET, "🧭 Setting want peers to {:?}", peers);
                self.swarm.behaviour_mut().peer_sync.want_peers(peers).await?;
            },
            NetworkingRequest::AddPeer { peer, reply } => {
                info!(target: LOG_TARGET, "Adding {peer}");
                let num_addresses = peer.addresses().len();
                let peer_id = peer.to_peer_id();
                let mut failed = 0usize;
                for address in peer.addresses {
                    let update = self.swarm.behaviour_mut().kad.add_address(&peer_id, address);
                    if matches!(update, RoutingUpdate::Failed) {
                        failed += 1;
                    }
                }

                if failed == 0 {
                    let _ignore = reply.send(Ok(()));
                } else {
                    let _ignore = reply.send(Err(NetworkError::FailedToAddPeer {
                        details:
                            format!("Failed to add {failed} out of {num_addresses} address(es) to peer {peer_id}",)
                                .to_string(),
                    }));
                }
            },
            NetworkingRequest::BanPeer {
                peer_id,
                reason,
                ban_duration,
                reply,
            } => {
                info!(target: LOG_TARGET, "🎯Banning peer {peer_id} for {ban_duration:?}: {reason}");
                // TODO: mark the peer as banned and prevent connections,messages from coming through
                self.ban_list.insert(peer_id, BannedPeer {
                    peer_id,
                    banned_at: Instant::now(),
                    ban_duration,
                    ban_reason: reason,
                });
                if self.swarm.disconnect_peer_id(peer_id).is_ok() {
                    let _ignore = reply.send(Ok(true));
                } else {
                    warn!(target: LOG_TARGET, "❓️ Disconnect peer {peer_id} was not connected");
                    let _ignore = reply.send(Ok(false));
                }
                self.publish_event(NetworkEvent::PeerBanned { peer_id });
            },
            NetworkingRequest::UnbanPeer { peer_id, reply } => match self.ban_list.remove(&peer_id) {
                Some(peer) => {
                    let _ignore = reply.send(Ok(peer.is_banned()));
                    shrink_hashmap_if_required(&mut self.ban_list);
                },
                None => {
                    let _ignore = reply.send(Ok(false));
                },
            },

            NetworkingRequest::GetBannedPeer { peer_id, reply } => {
                let mut must_remove = false;
                match self.ban_list.get(&peer_id) {
                    Some(peer) => {
                        let is_banned = peer.is_banned();
                        if !is_banned {
                            must_remove = true;
                        }

                        let _ignore = reply.send(Ok(Some(peer.clone())));
                    },
                    None => {
                        let _ignore = reply.send(Ok(None));
                    },
                }
                if must_remove {
                    self.ban_list.remove(&peer_id);
                    shrink_hashmap_if_required(&mut self.ban_list);
                }
            },
            NetworkingRequest::GetBannedPeers { reply } => {
                self.ban_list.retain(|_, p| p.is_banned());
                let banned = self.ban_list.values().cloned().collect();
                let _ignore = reply.send(Ok(banned));
                shrink_hashmap_if_required(&mut self.ban_list);
            },
        }

        Ok(())
    }

    async fn handle_messaging_request(&mut self, request: MessagingRequest<TMsg>) {
        match request {
            MessagingRequest::SendMessage {
                peer,
                message,
                reply_tx,
            } => {
                match self
                    .swarm
                    .behaviour_mut()
                    .messaging
                    .as_mut()
                    .map(|m| m.send_message(peer, message))
                {
                    Some(Ok(_)) => {
                        debug!(target: LOG_TARGET, "📢 Queued message to peer {}", peer);
                        let _ignore = reply_tx.send(Ok(()));
                    },
                    Some(Err(err)) => {
                        debug!(target: LOG_TARGET, "🚨 Failed to queue message to peer {}: {}", peer, err);
                        let _ignore = reply_tx.send(Err(err.into()));
                    },
                    None => {
                        warn!(target: LOG_TARGET, "Sent message but messaging is disabled");
                        let _ignore = reply_tx.send(Err(NetworkError::MessagingDisabled));
                    },
                }
            },
            MessagingRequest::SendMulticast {
                destination,
                message,
                reply_tx,
            } => {
                let len = destination.len();
                let Some(messaging_mut) = &mut self.swarm.behaviour_mut().messaging.as_mut() else {
                    warn!(target: LOG_TARGET, "Sent multicast message but messaging is disabled");
                    let _ignore = reply_tx.send(Err(NetworkError::MessagingDisabled));
                    return;
                };

                let mut num_sent = 0;
                for peer in destination {
                    match messaging_mut.send_message(peer, message.clone()) {
                        Ok(_) => {
                            num_sent += 1;
                        },
                        Err(err) => {
                            debug!(target: LOG_TARGET, "🚨 Failed to queue message to peer {}: {}", peer, err);
                        },
                    }
                }
                debug!(target: LOG_TARGET, "📢 Queued message to {num_sent} out of {len} peers");
                let _ignore = reply_tx.send(Ok(num_sent));
            },
        }
    }

    fn gossipsub_subscribe_topic(
        &mut self,
        topic: IdentTopic,
        inbound: mpsc::UnboundedSender<(PeerId, gossipsub::Message)>,
    ) -> Result<(), NetworkError> {
        if !self.swarm.behaviour_mut().gossipsub.subscribe(&topic)? {
            warn!(target: LOG_TARGET, "Already subscribed to {topic}");
            // We'll just replace the previous channel in this case
        }

        debug!(target: LOG_TARGET, "📢 Subscribed to gossipsub topic: {}", topic);
        self.gossipsub_subscriptions.insert(topic.hash(), inbound);

        Ok(())
    }

    fn add_all_seed_peers(&mut self) {
        for peer in self.seed_peers.drain(..) {
            info!(target: LOG_TARGET, "Adding seed peer {peer}");
            let peer_id = peer.public_key.to_peer_id();
            for addr in peer.addresses {
                let update = self.swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                if matches!(update, RoutingUpdate::Failed) {
                    warn!(target: LOG_TARGET, "Failed to add seed peer {peer_id}");
                }
            }
        }

        if let Err(err) = self.swarm.behaviour_mut().kad.bootstrap() {
            error!(target: LOG_TARGET, "Error bootstrapping kad: {}", err);
        }
    }

    async fn bootstrap(&mut self) -> Result<(), NetworkError> {
        if !self.is_initial_bootstrap_complete {
            self.swarm
                .behaviour_mut()
                .peer_sync
                .add_known_local_public_addresses(self.config.known_local_public_address.clone());
        }

        if self.active_connections.len() < self.relays.num_possible_relays() {
            info!(target: LOG_TARGET, "🥾 Bootstrapping with {} known relay peers", self.relays.num_possible_relays());
            for (peer, addrs) in self.relays.possible_relays() {
                self.swarm
                    .dial(
                        DialOpts::peer_id(*peer)
                            .addresses(addrs.iter().cloned().collect())
                            .extend_addresses_through_behaviour()
                            .build(),
                    )
                    .or_else(|err| {
                        // Peer already has pending dial or established connection - OK
                        if matches!(&err, DialError::DialPeerConditionFalse(_)) {
                            Ok(())
                        } else {
                            Err(err)
                        }
                    })?;
            }
        }
        self.is_initial_bootstrap_complete = true;

        Ok(())
    }

    async fn on_swarm_event(
        &mut self,
        event: SwarmEvent<TariNodeBehaviourEvent<ProstCodec<TMsg::Message>>>,
    ) -> Result<(), NetworkError> {
        match event {
            SwarmEvent::Behaviour(event) => self.on_behaviour_event(event).await?,
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                num_established,
                concurrent_dial_errors,
                established_in,
            } => self.on_connection_established(
                peer_id,
                connection_id,
                endpoint,
                num_established.get(),
                concurrent_dial_errors.map(|c| c.len()).unwrap_or(0),
                established_in,
            )?,
            SwarmEvent::ConnectionClosed {
                peer_id,
                endpoint,
                cause,
                ..
            } => {
                info!(target: LOG_TARGET, "🔌 Connection closed: peer_id={}, endpoint={:?}, cause={:?}", peer_id, endpoint, cause);
                match self.active_connections.entry(peer_id) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().retain(|c| c.endpoint != endpoint);
                        if entry.get().is_empty() {
                            entry.remove_entry();
                        }
                    },
                    Entry::Vacant(_) => {
                        debug!(target: LOG_TARGET, "Connection closed for peer {peer_id} but this connection is not in the active connections list");
                    },
                }
                shrink_hashmap_if_required(&mut self.active_connections);

                self.publish_event(NetworkEvent::PeerDisconnected { peer_id });
            },
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer_id),
                error,
                ..
            } => {
                warn!(target: LOG_TARGET, "🚨 Outgoing connection error: peer_id={}, error={}", peer_id, error);
                let Some(waiters) = self.pending_dial_requests.remove(&peer_id) else {
                    debug!(target: LOG_TARGET, "No pending dial requests initiated by this service for peer {}", peer_id);
                    return Ok(());
                };
                shrink_hashmap_if_required(&mut self.pending_dial_requests);

                for waiter in waiters {
                    let _ignore = waiter.send(Err(NetworkError::OutgoingConnectionError(error.to_string())));
                }

                if matches!(error, DialError::NoAddresses) {
                    self.swarm
                        .behaviour_mut()
                        .peer_sync
                        .add_want_peers(Some(peer_id))
                        .await?;
                }
            },
            SwarmEvent::ExternalAddrConfirmed { address } => {
                info!(target: LOG_TARGET, "🌍️ External address confirmed: {}", address);
            },
            SwarmEvent::Dialing { peer_id, connection_id } => {
                if let Some(peer_id) = peer_id {
                    info!(target: LOG_TARGET, "🤝 Dialing peer {peer_id} for connection({connection_id})");
                } else {
                    info!(target: LOG_TARGET, "🤝 Dialing unknown peer for connection({connection_id})");
                }
            },
            e => {
                debug!(target: LOG_TARGET, "🌎️ Swarm event: {:?}", e);
            },
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn on_behaviour_event(
        &mut self,
        event: TariNodeBehaviourEvent<ProstCodec<TMsg::Message>>,
    ) -> Result<(), NetworkError> {
        use TariNodeBehaviourEvent::*;
        match event {
            Ping(ping::Event {
                peer,
                connection,
                result,
            }) => match result {
                Ok(t) => {
                    if let Some(c) = self
                        .active_connections
                        .get_mut(&peer)
                        .and_then(|c| c.iter_mut().find(|c| c.connection_id == connection))
                    {
                        c.ping_latency = Some(t);
                    }
                    debug!(target: LOG_TARGET, "🏓 Ping: peer={}, connection={}, t={:.2?}", peer, connection, t);
                },
                Err(err) => {
                    warn!(target: LOG_TARGET, "🏓 Ping failed: peer={}, connection={}, error={}", peer, connection, err);
                },
            },
            Dcutr(dcutr::Event { remote_peer_id, result }) => match result {
                Ok(_) => {
                    info!(target: LOG_TARGET, "📡 Dcutr successful: peer={}", remote_peer_id);
                },
                Err(err) => {
                    info!(target: LOG_TARGET, "📡 Dcutr failed: peer={}, error={}", remote_peer_id, err);
                },
            },
            Identify(identify::Event::Received {
                peer_id,
                info,
                connection_id,
            }) => {
                info!(target: LOG_TARGET, "👋 Received identify from {} with {} addresses on connection {}", peer_id, info.listen_addrs.len(), connection_id);
                self.on_peer_identified(peer_id, info)?;
            },
            Identify(event) => {
                debug!(target: LOG_TARGET, "ℹ️ Identify event: {:?}", event);
            },
            RelayClient(relay::client::Event::ReservationReqAccepted {
                relay_peer_id,
                renewal,
                limit,
            }) => {
                info!(
                    "🌍️ Relay accepted our reservation request: peer_id={}, renewal={:?}, limit={:?}",
                    relay_peer_id, renewal, limit
                );
            },

            RelayClient(event) => {
                info!(target: LOG_TARGET, "🌎️ RelayClient event: {:?}", event);
            },
            Relay(event) => {
                info!(target: LOG_TARGET, "ℹ️ Relay event: {:?}", event);
            },
            Gossipsub(gossipsub::Event::Message {
                message_id,
                message,
                propagation_source,
            }) => {
                info!(target: LOG_TARGET, "📢 Gossipsub message: [{topic}] {message_id} ({bytes} bytes) from {source}", topic = message.topic, bytes = message.data.len(), source = propagation_source);
                self.on_gossipsub_message(propagation_source, message_id, message)?;
            },
            Gossipsub(event) => {
                info!(target: LOG_TARGET, "ℹ️ Gossipsub event: {:?}", event);
            },
            Messaging(messaging::Event::ReceivedMessage {
                peer_id,
                message,
                length,
            }) => {
                info!(target: LOG_TARGET, "📧 Rx Messaging: peer {peer_id} ({length} bytes)");
                let _ignore = self.messaging_mode.send_message(peer_id, message);
            },
            Messaging(event) => {
                debug!(target: LOG_TARGET, "ℹ️ Messaging event: {:?}", event);
            },
            Substream(event) => {
                self.on_substream_event(event);
            },
            ConnectionLimits(_) => {
                // This is unreachable as connection-limits has no events
                info!(target: LOG_TARGET, "ℹ️ ConnectionLimits event");
            },
            Mdns(event) => {
                self.on_mdns_event(event)?;
            },
            Autonat(event) => {
                self.on_autonat_event(event)?;
            },
            PeerSync(peersync::Event::LocalPeerRecordUpdated { record }) => {
                info!(target: LOG_TARGET, "🧑‍🧑‍🧒‍🧒 Local peer record updated: {:?} announce enabled = {}, has_sent_announce = {}",record, self.config.announce, self.has_sent_announce);
                if self.config.announce && !self.has_sent_announce && record.is_signed() {
                    info!(target: LOG_TARGET, "📣 Sending local peer announce with {} address(es)", record.addresses().len());
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(IdentTopic::new(PEER_ANNOUNCE_TOPIC), record.encode_to_proto()?)?;
                    self.has_sent_announce = true;
                }
            },
            PeerSync(peersync::Event::PeerBatchReceived { new_peers, from_peer }) => {
                info!(target: LOG_TARGET, "🧑‍🧑‍🧒‍🧒 Peer batch received: from_peer={}, new_peers={}", from_peer, new_peers);
            },
            PeerSync(event) => {
                info!(target: LOG_TARGET, "ℹ️ PeerSync event: {:?}", event);
            },
            Kad(event) => {
                info!(target: LOG_TARGET, "🌐 Kad event: {:?}", event);
            },
        }

        Ok(())
    }

    fn on_gossipsub_message(
        &mut self,
        propagation_source: PeerId,
        message_id: MessageId,
        message: gossipsub::Message,
    ) -> Result<(), NetworkError> {
        let Some(sink) = self.gossipsub_subscriptions.get(&message.topic) else {
            debug!(target: LOG_TARGET, "📣 Received message {message_id} with topic {} which we are not subscribed", message.topic);
            return Ok(());
        };

        debug!(target: LOG_TARGET, "📣 RX Gossipsub: {message_id} from {propagation_source} (size: {})", message.data.len());

        if let Err(mpsc::error::SendError((_, message))) = sink.send((propagation_source, message)) {
            warn!(target: LOG_TARGET, "📣 Gossipsub sink dropped for topic {}. Removing subscription channel. The node is still subscribed (use NetworkHandle::unsubscribe_topic).", message.topic);
            // We could unsubscribe in this case, but this probably isn't very useful and this is probably a result of a
            // downstream bug.
            let _drop = self.gossipsub_subscriptions.remove(&message.topic);
        }
        Ok(())
    }

    fn on_mdns_event(&mut self, event: mdns::Event) -> Result<(), NetworkError> {
        match event {
            mdns::Event::Discovered(peers_and_addrs) => {
                for (peer, addr) in peers_and_addrs {
                    debug!(target: LOG_TARGET, "📡 mDNS discovered peer {} at {}", peer, addr);
                    self.swarm
                        .dial(DialOpts::peer_id(peer).addresses(vec![addr]).build())
                        .or_else(|err| {
                            // Peer already has pending dial or established connection - OK
                            if matches!(&err, DialError::DialPeerConditionFalse(_)) {
                                Ok(())
                            } else {
                                Err(err)
                            }
                        })?;
                }
            },
            mdns::Event::Expired(addrs_list) => {
                for (peer_id, multiaddr) in addrs_list {
                    debug!(target: LOG_TARGET, "MDNS got expired peer with ID: {peer_id:#?} and Address: {multiaddr:#?}");
                }
            },
        }
        Ok(())
    }

    fn on_autonat_event(&mut self, event: autonat::Event) -> Result<(), NetworkError> {
        use autonat::Event::*;
        match event {
            StatusChanged { old, new } => {
                if let Some(public_address) = self.swarm.behaviour().autonat.public_address() {
                    info!(target: LOG_TARGET, "🌍️ Autonat: Our public address is {public_address}");
                }

                // If we are/were "Private", let's establish a relay reservation with a known relay
                if (self.config.reachability_mode.is_private() ||
                    new == NatStatus::Private ||
                    old == NatStatus::Private) &&
                    !self.relays.has_active_relay()
                {
                    info!(target: LOG_TARGET, "🌍️ Reachability status changed to Private. Dialing relay");
                    self.attempt_relay_reservation();
                }

                info!(target: LOG_TARGET, "🌍️ Autonat status changed from {:?} to {:?}", old, new);
            },
            _ => {
                info!(target: LOG_TARGET, "🌍️ Autonat event: {:?}", event);
            },
        }

        Ok(())
    }

    fn attempt_relay_reservation(&mut self) {
        self.relays.select_random_relay();
        if let Some(relay) = self.relays.selected_relay() {
            if let Err(err) = self.swarm.dial(
                DialOpts::peer_id(relay.peer_id)
                    .addresses(relay.addresses.clone())
                    .condition(PeerCondition::NotDialing)
                    .build(),
            ) {
                if is_dial_error_caused_by_remote(&err) {
                    self.relays.clear_selected_relay();
                }
                warn!(target: LOG_TARGET, "🚨 Failed to dial relay: {}", err);
            }
        }
    }

    fn on_connection_established(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        endpoint: ConnectedPoint,
        num_established: u32,
        num_concurrent_dial_errors: usize,
        established_in: Duration,
    ) -> Result<(), NetworkError> {
        debug!(
            target: LOG_TARGET,
            "🤝 Connection established: peer_id={}, connection_id={}, endpoint={:?}, num_established={}, \
             concurrent_dial_errors={}, established_in={:?}",
            peer_id,
            connection_id,
            endpoint,
            num_established,
            num_concurrent_dial_errors,
            established_in
        );

        if let Some(relay) = self.relays.selected_relay_mut() {
            if endpoint.is_dialer() && relay.peer_id == peer_id {
                relay.dialled_address = Some(endpoint.get_remote_address().clone());
            }
        }

        let is_dialer = endpoint.is_dialer();

        self.active_connections.entry(peer_id).or_default().push(Connection {
            connection_id,
            peer_id,
            public_key: None,
            created_at: Instant::now(),
            endpoint,
            num_established,
            num_concurrent_dial_errors,
            established_in,
            ping_latency: None,
            user_agent: None,
        });

        let Some(waiters) = self.pending_dial_requests.remove(&peer_id) else {
            debug!(target: LOG_TARGET, "No pending dial requests initiated by this service for peer {}", peer_id);
            return Ok(());
        };

        for waiter in waiters {
            let _ignore = waiter.send(Ok(()));
        }

        self.publish_event(NetworkEvent::PeerConnected {
            peer_id,
            direction: if is_dialer {
                ConnectionDirection::Outbound
            } else {
                ConnectionDirection::Inbound
            },
        });
        Ok(())
    }

    fn on_peer_identified(&mut self, peer_id: PeerId, info: identify::Info) -> Result<(), NetworkError> {
        if !self.config.swarm.protocol_version.is_compatible(&info.protocol_version) {
            info!(target: LOG_TARGET, "🚨 Peer {} is using an incompatible protocol version: {}. Our version {}", peer_id, info.protocol_version, self.config.swarm.protocol_version);
            // Error can be ignored as the docs indicate that an error only occurs if there was no connection to the
            // peer.
            let _ignore = self.swarm.disconnect_peer_id(peer_id);
            return Ok(());
        }

        // Not sure if this can happen but just in case
        if *self.swarm.local_peer_id() == peer_id {
            warn!(target: LOG_TARGET, "Dialled ourselves");
            return Ok(());
        }

        let identify::Info {
            public_key,
            agent_version,
            listen_addrs,
            protocols,
            ..
        } = info;

        self.update_connected_peers(&peer_id, public_key.clone(), agent_version);

        let is_relay = protocols.iter().any(|p| *p == relay::HOP_PROTOCOL_NAME);

        let is_connected_through_relay = self
            .active_connections
            .get(&peer_id)
            .map(|conns| {
                conns
                    .iter()
                    .any(|c| c.endpoint.is_dialer() && is_through_relay_address(c.endpoint.get_remote_address()))
            })
            .unwrap_or(false);

        for address in listen_addrs {
            if is_p2p_address(&address) && address.is_global_ip() {
                // If the peer has a p2p-circuit address, immediately upgrade to a direct connection (DCUtR /
                // hole-punching) if we're connected to them through a relay
                if is_connected_through_relay {
                    info!(target: LOG_TARGET, "📡 Peer {} has a p2p-circuit address. Upgrading to DCUtR", peer_id);
                    // Ignore as connection failures are logged in events, or an error here is because the peer is
                    // already connected/being dialled
                    let _ignore = self
                        .swarm
                        .dial(DialOpts::peer_id(peer_id).addresses(vec![address.clone()]).build());
                } else if is_relay && !is_through_relay_address(&address) {
                    // Otherwise, if the peer advertises as a relay we'll add them
                    info!(target: LOG_TARGET, "📡 Adding peer {peer_id} {address} as a relay");
                    self.relays.add_possible_relay(peer_id, address.clone());
                } else {
                    // Nothing to do
                }
            }

            let update = self.swarm.behaviour_mut().kad.add_address(&peer_id, address);
            if matches!(update, RoutingUpdate::Failed) {
                warn!(
                    target: LOG_TARGET,
                    "⚠️ Failed to add peer {peer_id} to routing table on connect",
                )
            }
        }

        // If this peer is the selected relay that was dialled previously, listen on the circuit address
        // Note we only select a relay if autonat says we are not publicly accessible.
        if is_relay {
            self.establish_relay_circuit_on_connect(&peer_id);
        }

        self.publish_event(NetworkEvent::NewIdentifiedPeer {
            peer_id,
            public_key,
            supported_protocols: protocols,
        });
        Ok(())
    }

    fn update_connected_peers(&mut self, peer_id: &PeerId, public_key: identity::PublicKey, agent_version: String) {
        let Some(conns_mut) = self.active_connections.get_mut(peer_id) else {
            return;
        };

        let user_agent = Arc::new(agent_version);
        for conn_mut in conns_mut {
            conn_mut.user_agent = Some(user_agent.clone());
            conn_mut.public_key = Some(public_key.clone());
        }
    }

    /// Establishes a relay circuit for the given peer if it is the selected relay peer. Returns true if the circuit
    /// was established from this call.
    fn establish_relay_circuit_on_connect(&mut self, peer_id: &PeerId) -> bool {
        let Some(relay) = self.relays.selected_relay() else {
            return false;
        };

        // If the peer we've connected with is the selected relay that we previously dialled, then continue
        if relay.peer_id != *peer_id {
            return false;
        }

        // If we've already established a circuit with the relay, there's nothing to do here
        if relay.is_circuit_established {
            return false;
        }

        // Check if we've got a confirmed address for the relay
        let Some(dialled_address) = relay.dialled_address.as_ref() else {
            return false;
        };

        let circuit_addr = dialled_address.clone().with(Protocol::P2pCircuit);

        match self.swarm.listen_on(circuit_addr.clone()) {
            Ok(id) => {
                self.swarm
                    .behaviour_mut()
                    .peer_sync
                    .add_known_local_public_addresses(vec![circuit_addr]);
                info!(target: LOG_TARGET, "🌍️ Peer {peer_id} is a relay. Listening (id={id:?}) for circuit connections");
                let Some(relay_mut) = self.relays.selected_relay_mut() else {
                    // unreachable
                    return false;
                };
                relay_mut.is_circuit_established = true;
                true
            },
            Err(e) => {
                // failed to establish a circuit, reset to try another relay
                self.relays.clear_selected_relay();
                error!(target: LOG_TARGET, "Local node failed to listen on relay address. Error: {e}");
                false
            },
        }
    }

    fn on_substream_event(&mut self, event: substream::Event) {
        use substream::Event::*;
        match event {
            SubstreamOpen {
                peer_id,
                stream_id,
                stream,
                protocol,
            } => {
                info!(target: LOG_TARGET, "📥 substream open: peer_id={}, stream_id={}, protocol={}", peer_id, stream_id, protocol);
                let Some(reply) = self.pending_substream_requests.remove(&stream_id) else {
                    debug!(target: LOG_TARGET, "No pending requests for subtream protocol {protocol} for peer {peer_id}");
                    return;
                };
                shrink_hashmap_if_required(&mut self.pending_substream_requests);

                let _ignore = reply.send(Ok(NegotiatedSubstream::new(peer_id, protocol, stream)));
            },
            InboundSubstreamOpen { notification } => {
                debug!(target: LOG_TARGET, "📥 Inbound substream open: protocol={}", notification.protocol);
                self.substream_notifiers.notify(notification);
            },
            InboundFailure {
                peer_id,
                stream_id,
                error,
            } => {
                debug!(target: LOG_TARGET, "Inbound substream failed from peer {peer_id} with stream id {stream_id}: {error}");
            },
            OutboundFailure {
                error,
                stream_id,
                peer_id,
                ..
            } => {
                debug!(target: LOG_TARGET, "Outbound substream failed with peer {peer_id}, stream {stream_id}: {error}");
                if let Some(waiting_reply) = self.pending_substream_requests.remove(&stream_id) {
                    let _ignore = waiting_reply.send(Err(NetworkError::FailedToOpenSubstream(error)));
                }
            },
            Error(_) => {},
        }
    }

    fn publish_event(&mut self, event: NetworkEvent) {
        if let Ok(num) = self.tx_events.send(event) {
            debug!(target: LOG_TARGET, "📢 Published networking event to {num} subscribers");
        }
    }
}

fn is_p2p_address(address: &Multiaddr) -> bool {
    address.iter().any(|p| matches!(p, Protocol::P2p(_)))
}

fn is_through_relay_address(address: &Multiaddr) -> bool {
    let mut found_p2p_circuit = false;
    for protocol in address {
        if !found_p2p_circuit {
            if let Protocol::P2pCircuit = protocol {
                found_p2p_circuit = true;
                continue;
            }
            continue;
        }
        // Once we found a p2p-circuit protocol, this is followed by /p2p/<peer_id>
        return matches!(protocol, Protocol::P2p(_));
    }

    false
}

fn is_dial_error_caused_by_remote(err: &DialError) -> bool {
    !matches!(
        err,
        DialError::DialPeerConditionFalse(_) | DialError::Aborted | DialError::LocalPeerId { .. }
    )
}

fn shrink_hashmap_if_required<K, V>(map: &mut HashMap<K, V>)
where K: Eq + Hash {
    const HASHMAP_EXCESS_ENTRIES_SHRINK_THRESHOLD: usize = 50;
    if map.len() + HASHMAP_EXCESS_ENTRIES_SHRINK_THRESHOLD < map.capacity() {
        map.shrink_to_fit();
    }
}
