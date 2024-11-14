//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use libp2p::{
    core::{transport::PortUse, Endpoint},
    futures::executor::block_on,
    identity::Keypair,
    swarm::{
        ConnectionClosed,
        ConnectionDenied,
        ConnectionId,
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

use crate::{
    error::Error,
    event::Event,
    handler::{Handler, HandlerAction},
    store::PeerStore,
    Config,
    LocalPeerRecord,
    SignedPeerRecord,
};

/// Internal threshold for when to shrink the capacity
/// of empty queues. If the capacity of an empty queue
/// exceeds this threshold, the associated memory is
/// released.
pub const EMPTY_QUEUE_SHRINK_THRESHOLD: usize = 100;
pub const DEFAULT_PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/tari/peersync/0.0.1");

pub(crate) type WantList = HashSet<PeerId>;

pub struct Behaviour<TPeerStore: PeerStore> {
    protocol: StreamProtocol,
    config: Config,
    pending_events: VecDeque<ToSwarm<Event, THandlerInEvent<Self>>>,
    local_peer_record: LocalPeerRecord,
    peer_store: TPeerStore,
    want_peers: HashSet<PeerId>,
    remaining_want_peers: HashSet<PeerId>,
    pending_syncs: VecDeque<PeerId>,
    pending_tasks: futures_bounded::FuturesSet<Event>,
    active_outbound_connections: HashMap<PeerId, ConnectionId>,
    /// Ensures that only one sync task can occur at once
    sync_semaphore: Arc<async_semaphore::Semaphore>,
}

impl<TPeerStore> Behaviour<TPeerStore>
where TPeerStore: PeerStore
{
    pub fn new(keypair: Keypair, store: TPeerStore, config: Config) -> Self {
        Self::with_custom_protocol(keypair, DEFAULT_PROTOCOL_NAME, store, config)
    }

    fn with_custom_protocol(
        keypair: Keypair,
        protocol: StreamProtocol,
        peer_store: TPeerStore,
        config: Config,
    ) -> Self {
        Self {
            local_peer_record: LocalPeerRecord::new(Arc::new(keypair)),
            protocol,
            config,
            pending_events: VecDeque::new(),
            peer_store,
            want_peers: HashSet::new(),
            active_outbound_connections: HashMap::new(),
            remaining_want_peers: HashSet::new(),
            pending_syncs: VecDeque::new(),
            pending_tasks: futures_bounded::FuturesSet::new(Duration::from_secs(30), 1024),
            sync_semaphore: Arc::new(async_semaphore::Semaphore::new(1)),
        }
    }

    pub async fn validate_and_add_peer_record(&mut self, peer: SignedPeerRecord) -> Result<(), Error> {
        if !peer.is_valid() {
            return Err(Error::InvalidSignedPeer {
                peer_id: peer.to_peer_id(),
                details: "Peer signature failed validation".to_string(),
            });
        }
        self.store()
            .put_if_newer(peer)
            .await
            .map_err(|e| Error::StoreError(e.to_string()))
    }

    pub fn local_peer_record(&self) -> &LocalPeerRecord {
        &self.local_peer_record
    }

    pub fn add_known_local_public_addresses(&mut self, addrs: Vec<Multiaddr>) {
        if addrs.is_empty() {
            return;
        }

        let mut is_any_new = false;
        for addr in addrs {
            is_any_new |= self.local_peer_record.add_address(addr.clone());
        }

        if is_any_new {
            self.handle_update_local_record();
        }
    }

    pub async fn want_peers<I: IntoIterator<Item = PeerId>>(&mut self, peers: I) -> Result<(), Error> {
        self.want_peers.clear();
        self.want_peers.extend(peers);
        shrink_hash_set_if_required(&mut self.want_peers);
        if self.want_peers.is_empty() {
            self.remaining_want_peers.clear();
            shrink_hash_set_if_required(&mut self.remaining_want_peers);
            return Ok(());
        }

        // None - no more to add, we've already added them above
        self.add_want_peers(None).await?;
        Ok(())
    }

    pub async fn add_want_peers<I: IntoIterator<Item = PeerId>>(&mut self, peers: I) -> Result<(), Error> {
        let local_peer_id = self.local_peer_record.to_peer_id();
        self.want_peers
            .extend(peers.into_iter().filter(|id| *id != local_peer_id));
        self.remaining_want_peers = self
            .store()
            .difference(&self.want_peers)
            .await
            .map_err(|e| Error::StoreError(e.to_string()))?;
        tracing::debug!("Remaining want peers: {:?}", self.remaining_want_peers);
        if !self.remaining_want_peers.is_empty() {
            let list = Arc::new(self.remaining_want_peers.clone());
            // Notify all handlers
            self.pending_events.reserve(self.remaining_want_peers.len());
            for (peer_id, conn_id) in &self.active_outbound_connections {
                self.pending_events.push_back(ToSwarm::NotifyHandler {
                    peer_id: *peer_id,
                    handler: NotifyHandler::One(*conn_id),
                    event: HandlerAction::WantPeers(list.clone()),
                });
            }
        }
        Ok(())
    }

    pub fn store(&self) -> &TPeerStore {
        &self.peer_store
    }

    fn on_connection_closed(&mut self, ConnectionClosed { peer_id, .. }: ConnectionClosed) {
        self.active_outbound_connections.remove(&peer_id);
        if let Some(pos) = self.pending_syncs.iter().position(|p| *p == peer_id) {
            self.pending_syncs.remove(pos);
            self.pending_events
                .push_back(ToSwarm::GenerateEvent(Event::InboundFailure {
                    peer_id,
                    error: Error::ConnectionClosed,
                }));
        }
    }

    fn handle_update_local_record(&mut self) {
        let store = self.peer_store.clone();
        let local_peer_record = self.local_peer_record.clone();
        let local_peer_rec = SignedPeerRecord::from(local_peer_record);
        let local_rec = Arc::new(local_peer_rec.clone());
        let task = async move {
            match store.put(local_peer_rec).await {
                Ok(_) => Event::LocalPeerRecordUpdated,
                Err(err) => {
                    tracing::error!("Failed to add local peer record to store: {}", err);
                    Event::Error(Error::StoreError(err.to_string()))
                },
            }
        };
        match self.pending_tasks.try_push(task) {
            Ok(()) => {
                self.pending_events.reserve(self.active_outbound_connections.len());
                for (peer_id, conn_id) in &self.active_outbound_connections {
                    self.pending_events.push_back(ToSwarm::NotifyHandler {
                        peer_id: *peer_id,
                        handler: NotifyHandler::One(*conn_id),
                        event: HandlerAction::PushLocalRecord(local_rec.clone()),
                    });
                }
            },
            Err(_) => {
                self.pending_events.push_back(ToSwarm::GenerateEvent(Event::Error(
                    Error::ExceededMaxNumberOfPendingTasks,
                )));
            },
        }
    }
}

impl<TPeerStore> NetworkBehaviour for Behaviour<TPeerStore>
where TPeerStore: PeerStore
{
    type ConnectionHandler = Handler<TPeerStore>;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer_id: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let handler = Handler::new(
            peer_id,
            self.peer_store.clone(),
            self.protocol.clone(),
            &self.config,
            self.remaining_want_peers.clone(),
            self.sync_semaphore.clone(),
            None,
        );
        Ok(handler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _remote_addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        tracing::debug!("outbound connection to peer {}", peer);
        let handler = Handler::new(
            peer,
            self.peer_store.clone(),
            self.protocol.clone(),
            &self.config,
            self.remaining_want_peers.clone(),
            self.sync_semaphore.clone(),
            Some(Arc::new(self.local_peer_record.clone().into())),
        );
        self.active_outbound_connections.insert(peer, connection_id);
        Ok(handler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(_) => {},
            FromSwarm::ConnectionClosed(connection_closed) => self.on_connection_closed(connection_closed),
            FromSwarm::AddressChange(_) => {},
            FromSwarm::ExternalAddrConfirmed(addr_confirmed) => {
                if self.local_peer_record.add_address(addr_confirmed.addr.clone()) {
                    self.handle_update_local_record();
                    self.pending_events
                        .push_back(ToSwarm::GenerateEvent(Event::LocalPeerRecordUpdated));
                }
            },
            FromSwarm::ExternalAddrExpired(addr_expired) => {
                if self.local_peer_record.remove_address(addr_expired.addr) {
                    self.handle_update_local_record();
                    self.pending_events
                        .push_back(ToSwarm::GenerateEvent(Event::LocalPeerRecordUpdated));
                }
            },
            _ => {},
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match &event {
            Event::InboundFailure { .. } => {},
            Event::OutboundFailure { .. } => {},
            Event::PeerBatchReceived { new_peers, .. } => {
                if *new_peers > 0 {
                    match block_on(self.store().difference(&self.want_peers)) {
                        Ok(peers) => {
                            self.remaining_want_peers = peers;
                        },
                        Err(err) => {
                            tracing::error!("Failed to get peer from store: {}", err);
                        },
                    }
                }
            },
            Event::InboundStreamInterrupted { .. } => {},
            Event::OutboundStreamInterrupted { .. } => {},
            Event::InboundRequestCompleted { .. } => {},
            Event::LocalPeerRecordUpdated { .. } => {},
            Event::Error(_) => {},
        }

        self.pending_events.push_back(ToSwarm::GenerateEvent(event));
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }
        if self.pending_events.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
            self.pending_events.shrink_to_fit();
        }

        match self.pending_tasks.poll_unpin(cx) {
            Poll::Ready(Ok(event)) => {
                return Poll::Ready(ToSwarm::GenerateEvent(event));
            },
            Poll::Ready(Err(_)) => {
                tracing::error!("Internal task timed out");
            },
            Poll::Pending => {},
        }

        Poll::Pending
    }

    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let peer_id = match maybe_peer {
            Some(peer_id) => peer_id,
            None => return Ok(vec![]),
        };

        match block_on(self.peer_store.get(&peer_id)) {
            Ok(maybe_peer) => Ok(maybe_peer.map(|peer| peer.addresses).unwrap_or_default()),
            Err(err) => {
                tracing::error!("Failed to get peer from store: {}", err);
                Ok(vec![])
            },
        }
    }
}

fn shrink_hash_set_if_required<T: Eq + std::hash::Hash>(set: &mut HashSet<T>) {
    if set.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
        set.shrink_to_fit();
    }
}
