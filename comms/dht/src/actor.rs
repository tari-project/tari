// Copyright 2019, The Tari Project
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

//! Actor for DHT functionality.
//!
//! The DhtActor is responsible for sending a join request on startup
//! and furnishing [DhtRequest]s.
//!
//! [DhtRequest]: ./enum.DhtRequest.html

use std::{cmp, fmt, fmt::Display, sync::Arc, time::Instant};

use chrono::{DateTime, Utc};
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use log::*;
use tari_comms::{
    connection_manager::ConnectionManagerError,
    connectivity::{ConnectivityError, ConnectivityRequester, ConnectivitySelection},
    peer_manager::{NodeId, NodeIdentity, PeerFeatures, PeerManager, PeerManagerError, PeerQuery, PeerQuerySortBy},
    types::CommsPublicKey,
    PeerConnection,
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_shutdown::ShutdownSignal;
use tari_utilities::message_format::{MessageFormat, MessageFormatError};
use thiserror::Error;
use tokio::{
    sync::{mpsc, oneshot},
    task,
    time,
    time::MissedTickBehavior,
};

use crate::{
    broadcast_strategy::{BroadcastClosestRequest, BroadcastStrategy},
    dedup::DedupCacheDatabase,
    discovery::DhtDiscoveryError,
    outbound::{DhtOutboundError, OutboundMessageRequester, SendMessageParams},
    proto::{dht::JoinMessage, envelope::DhtMessageType},
    storage::{DbConnection, DhtDatabase, DhtMetadataKey, StorageError},
    DhtConfig,
    DhtDiscoveryRequester,
};

const LOG_TARGET: &str = "comms::dht::actor";

#[derive(Debug, Error)]
pub enum DhtActorError {
    #[error("MPSC channel is disconnected")]
    ChannelDisconnected,
    #[error("Reply sender canceled the request")]
    ReplyCanceled,
    #[error("PeerManagerError: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Failed to broadcast join message: {0}")]
    FailedToBroadcastJoinMessage(DhtOutboundError),
    #[error("DiscoveryError: {0}")]
    DiscoveryError(#[from] DhtDiscoveryError),
    #[error("StorageError: {0}")]
    StorageError(#[from] StorageError),
    #[error("StoredValueFailedToDeserialize: {0}")]
    StoredValueFailedToDeserialize(MessageFormatError),
    #[error("FailedToSerializeValue: {0}")]
    FailedToSerializeValue(MessageFormatError),
    #[error("ConnectivityError: {0}")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("Connectivity event stream closed")]
    ConnectivityEventStreamClosed,
}

impl<T> From<mpsc::error::SendError<T>> for DhtActorError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        DhtActorError::ChannelDisconnected
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum DhtRequest {
    /// Send a Join request to the network
    SendJoin,
    /// Inserts a message signature to the msg hash cache. This operation replies with the number of times this message
    /// has previously been seen (hit count)
    MsgHashCacheInsert {
        message_hash: Vec<u8>,
        received_from: CommsPublicKey,
        reply_tx: oneshot::Sender<u32>,
    },
    GetMsgHashHitCount(Vec<u8>, oneshot::Sender<u32>),
    /// Fetch selected peers according to the broadcast strategy
    SelectPeers(BroadcastStrategy, oneshot::Sender<Vec<NodeId>>),
    GetMetadata(DhtMetadataKey, oneshot::Sender<Result<Option<Vec<u8>>, DhtActorError>>),
    SetMetadata(DhtMetadataKey, Vec<u8>, oneshot::Sender<Result<(), DhtActorError>>),
    DialDiscoverPeer {
        public_key: CommsPublicKey,
        reply: oneshot::Sender<Result<PeerConnection, DhtActorError>>,
    },
}

impl Display for DhtRequest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use DhtRequest::*;
        match self {
            SendJoin => write!(f, "SendJoin"),
            MsgHashCacheInsert {
                message_hash,
                received_from,
                ..
            } => write!(
                f,
                "MsgHashCacheInsert(message hash: {}, received from: {})",
                message_hash.to_hex(),
                received_from.to_hex(),
            ),
            GetMsgHashHitCount(hash, _) => write!(f, "GetMsgHashHitCount({})", hash.to_hex()),
            SelectPeers(s, _) => write!(f, "SelectPeers (Strategy={})", s),
            GetMetadata(key, _) => write!(f, "GetMetadata (key={})", key),
            SetMetadata(key, value, _) => {
                write!(f, "SetMetadata (key={}, value={} bytes)", key, value.len())
            },
            DialDiscoverPeer { public_key, .. } => write!(f, "DialDiscoverPeer(public_key={})", public_key),
        }
    }
}

#[derive(Clone)]
pub struct DhtRequester {
    sender: mpsc::Sender<DhtRequest>,
}

impl DhtRequester {
    pub fn new(sender: mpsc::Sender<DhtRequest>) -> Self {
        Self { sender }
    }

    pub async fn send_join(&mut self) -> Result<(), DhtActorError> {
        self.sender.send(DhtRequest::SendJoin).await.map_err(Into::into)
    }

    pub async fn select_peers(&mut self, broadcast_strategy: BroadcastStrategy) -> Result<Vec<NodeId>, DhtActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(DhtRequest::SelectPeers(broadcast_strategy, reply_tx))
            .await?;
        reply_rx.await.map_err(|_| DhtActorError::ReplyCanceled)
    }

    pub async fn add_message_to_dedup_cache(
        &mut self,
        message_hash: Vec<u8>,
        received_from: CommsPublicKey,
    ) -> Result<u32, DhtActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(DhtRequest::MsgHashCacheInsert {
                message_hash,
                received_from,
                reply_tx,
            })
            .await?;

        reply_rx.await.map_err(|_| DhtActorError::ReplyCanceled)
    }

    pub async fn get_message_cache_hit_count(&mut self, message_hash: Vec<u8>) -> Result<u32, DhtActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(DhtRequest::GetMsgHashHitCount(message_hash, reply_tx))
            .await?;

        reply_rx.await.map_err(|_| DhtActorError::ReplyCanceled)
    }

    pub async fn get_metadata<T: MessageFormat>(&mut self, key: DhtMetadataKey) -> Result<Option<T>, DhtActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender.send(DhtRequest::GetMetadata(key, reply_tx)).await?;
        match reply_rx.await.map_err(|_| DhtActorError::ReplyCanceled)?? {
            Some(bytes) => T::from_binary(&bytes)
                .map(Some)
                .map_err(DhtActorError::StoredValueFailedToDeserialize),
            None => Ok(None),
        }
    }

    pub async fn set_metadata<T: MessageFormat>(&mut self, key: DhtMetadataKey, value: T) -> Result<(), DhtActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let bytes = value.to_binary().map_err(DhtActorError::FailedToSerializeValue)?;
        self.sender.send(DhtRequest::SetMetadata(key, bytes, reply_tx)).await?;
        reply_rx.await.map_err(|_| DhtActorError::ReplyCanceled)?
    }

    /// Attempt to dial a peer. If the peer is not known, a discovery will be initiated. If discovery succeeds, a
    /// connection to the peer will be returned.
    pub async fn dial_or_discover_peer(&mut self, public_key: CommsPublicKey) -> Result<PeerConnection, DhtActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(DhtRequest::DialDiscoverPeer {
                public_key,
                reply: reply_tx,
            })
            .await?;
        reply_rx.await.map_err(|_| DhtActorError::ReplyCanceled)?
    }
}

pub struct DhtActor {
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    database: DhtDatabase,
    outbound_requester: OutboundMessageRequester,
    connectivity: ConnectivityRequester,
    config: Arc<DhtConfig>,
    discovery: DhtDiscoveryRequester,
    shutdown_signal: ShutdownSignal,
    request_rx: mpsc::Receiver<DhtRequest>,
    msg_hash_dedup_cache: DedupCacheDatabase,
}

impl DhtActor {
    pub fn new(
        config: Arc<DhtConfig>,
        conn: DbConnection,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        connectivity: ConnectivityRequester,
        outbound_requester: OutboundMessageRequester,
        request_rx: mpsc::Receiver<DhtRequest>,
        discovery: DhtDiscoveryRequester,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        debug!(
            target: LOG_TARGET,
            "Message dedup cache will be trimmed to capacity every {}s",
            config.dedup_cache_trim_interval.as_secs() as f64 +
                config.dedup_cache_trim_interval.subsec_nanos() as f64 * 1e-9
        );
        Self {
            msg_hash_dedup_cache: DedupCacheDatabase::new(conn.clone(), config.dedup_cache_capacity),
            config,
            database: DhtDatabase::new(conn),
            outbound_requester,
            peer_manager,
            connectivity,
            node_identity,
            discovery,
            shutdown_signal,
            request_rx,
        }
    }

    pub fn spawn(self) {
        task::spawn(async move {
            if let Err(err) = self.run().await {
                error!(target: LOG_TARGET, "DhtActor failed to start with error: {:?}", err);
            }
        });
    }

    async fn run(mut self) -> Result<(), DhtActorError> {
        let offline_ts = self
            .database
            .get_metadata_value::<DateTime<Utc>>(DhtMetadataKey::OfflineTimestamp)
            .ok()
            .flatten();
        debug!(
            target: LOG_TARGET,
            "DhtActor started. {}",
            offline_ts
                .map(|dt| format!("Dht has been offline since '{}'", dt))
                .unwrap_or_else(String::new)
        );

        let mut pending_jobs = FuturesUnordered::new();

        let mut dedup_cache_trim_ticker = time::interval(self.config.dedup_cache_trim_interval);
        dedup_cache_trim_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                Some(request) = self.request_rx.recv() => {
                    trace!(target: LOG_TARGET, "DhtActor received request: {}", request);
                    pending_jobs.push(self.request_handler(request));
                },

                Some(result) = pending_jobs.next() => {
                    if let Err(err) = result {
                        debug!(target: LOG_TARGET, "Error when handling DHT request message. {}", err);
                    }
                },

                _ = dedup_cache_trim_ticker.tick() => {
                    if let Err(err) = self.msg_hash_dedup_cache.trim_entries() {
                        error!(target: LOG_TARGET, "Error when trimming message dedup cache: {:?}", err);
                    }
                },

                _ = self.shutdown_signal.wait() => {
                    info!(target: LOG_TARGET, "DhtActor is shutting down because it received a shutdown signal.");
                    self.mark_shutdown_time();
                    break Ok(());
                },
            }
        }
    }

    fn mark_shutdown_time(&self) {
        if let Err(err) = self
            .database
            .set_metadata_value(DhtMetadataKey::OfflineTimestamp, Utc::now())
        {
            warn!(target: LOG_TARGET, "Failed to mark offline time: {:?}", err);
        }
    }

    fn request_handler(&mut self, request: DhtRequest) -> BoxFuture<'static, Result<(), DhtActorError>> {
        use DhtRequest::*;
        match request {
            SendJoin => {
                let node_identity = Arc::clone(&self.node_identity);
                let outbound_requester = self.outbound_requester.clone();
                Box::pin(Self::broadcast_join(node_identity, outbound_requester))
            },
            MsgHashCacheInsert {
                message_hash,
                received_from,
                reply_tx,
            } => {
                let msg_hash_cache = self.msg_hash_dedup_cache.clone();
                Box::pin(async move {
                    match msg_hash_cache.add_body_hash(message_hash, received_from) {
                        Ok(hit_count) => {
                            let _ = reply_tx.send(hit_count);
                        },
                        Err(err) => {
                            warn!(
                                target: LOG_TARGET,
                                "Unable to update message dedup cache because {:?}", err
                            );
                            let _ = reply_tx.send(0);
                        },
                    }
                    Ok(())
                })
            },
            GetMsgHashHitCount(hash, reply_tx) => {
                let msg_hash_cache = self.msg_hash_dedup_cache.clone();
                Box::pin(async move {
                    let hit_count = msg_hash_cache.get_hit_count(hash)?;
                    let _ = reply_tx.send(hit_count);
                    Ok(())
                })
            },
            SelectPeers(broadcast_strategy, reply_tx) => {
                let peer_manager = Arc::clone(&self.peer_manager);
                let node_identity = Arc::clone(&self.node_identity);
                let connectivity = self.connectivity.clone();
                let config = self.config.clone();
                Box::pin(async move {
                    match Self::select_peers(&config, node_identity, peer_manager, connectivity, broadcast_strategy)
                        .await
                    {
                        Ok(peers) => reply_tx.send(peers).map_err(|_| DhtActorError::ReplyCanceled),
                        Err(err) => {
                            warn!(target: LOG_TARGET, "Peer selection failed: {:?}", err);
                            reply_tx.send(Vec::new()).map_err(|_| DhtActorError::ReplyCanceled)
                        },
                    }
                })
            },
            GetMetadata(key, reply_tx) => {
                let db = self.database.clone();
                Box::pin(async move {
                    let _ = reply_tx.send(db.get_metadata_value_bytes(key).map_err(Into::into));
                    Ok(())
                })
            },
            SetMetadata(key, value, reply_tx) => {
                let db = self.database.clone();
                Box::pin(async move {
                    match db.set_metadata_value_bytes(key, value) {
                        Ok(_) => {
                            debug!(target: LOG_TARGET, "Dht metadata '{}' set", key);
                            let _ = reply_tx.send(Ok(()));
                        },
                        Err(err) => {
                            warn!(target: LOG_TARGET, "Unable to set metadata because {:?}", err);
                            let _ = reply_tx.send(Err(err.into()));
                        },
                    }
                    Ok(())
                })
            },
            DialDiscoverPeer { public_key, reply } => {
                let connectivity = self.connectivity.clone();
                let discovery = self.discovery.clone();
                let peer_manager = self.peer_manager.clone();
                Box::pin(async move {
                    let mut task = DiscoveryDialTask::new(connectivity, peer_manager, discovery);
                    let result = task.run(public_key).await;
                    let _ = reply.send(result);
                    Ok(())
                })
            },
        }
    }

    async fn broadcast_join(
        node_identity: Arc<NodeIdentity>,
        mut outbound_requester: OutboundMessageRequester,
    ) -> Result<(), DhtActorError> {
        let message = JoinMessage::from(&node_identity);

        debug!(target: LOG_TARGET, "Sending Join message to closest peers");

        outbound_requester
            .send_message_no_header(
                SendMessageParams::new()
                    .closest(node_identity.node_id().clone(), vec![])
                    .with_destination(node_identity.node_id().clone().into())
                    .with_dht_message_type(DhtMessageType::Join)
                    .force_origin()
                    .finish(),
                message,
            )
            .await
            .map_err(DhtActorError::FailedToBroadcastJoinMessage)?;

        Ok(())
    }

    async fn select_peers(
        config: &DhtConfig,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        mut connectivity: ConnectivityRequester,
        broadcast_strategy: BroadcastStrategy,
    ) -> Result<Vec<NodeId>, DhtActorError> {
        use BroadcastStrategy::*;
        match broadcast_strategy {
            DirectNodeId(node_id) => {
                // Send to a particular peer matching the given node ID
                peer_manager
                    .direct_identity_node_id(&node_id)
                    .await
                    .map(|peer| peer.map(|p| vec![p.node_id]).unwrap_or_default())
                    .map_err(Into::into)
            },
            DirectPublicKey(public_key) => {
                // Send to a particular peer matching the given node ID
                peer_manager
                    .direct_identity_public_key(&public_key)
                    .await
                    .map(|peer| peer.map(|p| vec![p.node_id]).unwrap_or_default())
                    .map_err(Into::into)
            },
            Flood(exclude) => {
                let peers = connectivity
                    .select_connections(ConnectivitySelection::all_nodes(exclude))
                    .await?;
                Ok(peers.into_iter().map(|p| p.peer_node_id().clone()).collect())
            },
            ClosestNodes(closest_request) => {
                Self::select_closest_node_connected(closest_request, config, connectivity, peer_manager).await
            },
            DirectOrClosestNodes(closest_request) => {
                // First check if a direct connection exists
                if connectivity
                    .get_connection(closest_request.node_id.clone())
                    .await?
                    .is_some()
                {
                    return Ok(vec![closest_request.node_id.clone()]);
                }
                Self::select_closest_node_connected(closest_request, config, connectivity, peer_manager).await
            },
            Random(n, excluded) => {
                // Send to a random set of peers of size n that are Communication Nodes
                Ok(peer_manager
                    .random_peers(n, &excluded)
                    .await?
                    .into_iter()
                    .map(|p| p.node_id)
                    .collect())
            },
            SelectedPeers(peers) => Ok(peers),
            Broadcast(exclude) => {
                let connections = connectivity
                    .select_connections(ConnectivitySelection::random_nodes(
                        config.broadcast_factor,
                        exclude.clone(),
                    ))
                    .await?;

                let candidates = connections
                    .iter()
                    .map(|c| c.peer_node_id())
                    .cloned()
                    .collect::<Vec<_>>();

                if candidates.is_empty() {
                    warn!(
                        target: LOG_TARGET,
                        "Broadcast requested but there are no node peer connections available"
                    );
                }
                debug!(
                    target: LOG_TARGET,
                    "{} candidate(s) selected for broadcast",
                    candidates.len()
                );

                Ok(candidates)
            },
            Propagate(destination, exclude) => {
                let dest_node_id = destination
                    .node_id()
                    .cloned()
                    .or_else(|| destination.public_key().map(NodeId::from_public_key));

                let connections = match dest_node_id {
                    Some(node_id) => {
                        let dest_connection = connectivity.get_connection(node_id.clone()).await?;
                        // If the peer was added to the exclude list, we don't want to send directly to the peer.
                        // This ensures that we don't just send a message back to the peer that sent it.
                        let dest_connection = dest_connection.filter(|c| !exclude.contains(c.peer_node_id()));
                        match dest_connection {
                            Some(conn) => {
                                // We're connected to the destination, so send the message directly
                                vec![conn]
                            },
                            None => {
                                // Select connections closer to the destination
                                let mut connections = connectivity
                                    .select_connections(ConnectivitySelection::closest_to(
                                        node_id.clone(),
                                        config.num_neighbouring_nodes,
                                        exclude.clone(),
                                    ))
                                    .await?;

                                // Exclude candidates that are further away from the destination than this node
                                // unless this node has not selected a big enough sample i.e. this node is not well
                                // connected
                                if connections.len() >= config.propagation_factor {
                                    let dist_from_dest = node_identity.node_id().distance(&node_id);
                                    let before_len = connections.len();
                                    connections = connections
                                        .into_iter()
                                        .filter(|conn| conn.peer_node_id().distance(&node_id) <= dist_from_dest)
                                        .collect::<Vec<_>>();

                                    debug!(
                                        target: LOG_TARGET,
                                        "Filtered out {} node(s) that are further away than this node.",
                                        before_len - connections.len()
                                    );
                                }

                                connections.truncate(config.propagation_factor);
                                connections
                            },
                        }
                    },
                    None => {
                        debug!(
                            target: LOG_TARGET,
                            "No destination for propagation, sending to {} random peers", config.propagation_factor
                        );
                        connectivity
                            .select_connections(ConnectivitySelection::random_nodes(
                                config.propagation_factor,
                                exclude.clone(),
                            ))
                            .await?
                    },
                };

                if connections.is_empty() {
                    info!(
                        target: LOG_TARGET,
                        "Propagation requested but there are no node peer connections available"
                    );
                }

                let candidates = connections
                    .iter()
                    .map(|c| c.peer_node_id())
                    .cloned()
                    .collect::<Vec<_>>();

                debug!(
                    target: LOG_TARGET,
                    "{} candidate(s) selected for propagation to {}",
                    candidates.len(),
                    destination
                );

                trace!(
                    target: LOG_TARGET,
                    "(ThisNode = {}) Candidates are {}",
                    node_identity.node_id().short_str(),
                    candidates.iter().map(|n| n.short_str()).collect::<Vec<_>>().join(", ")
                );

                Ok(candidates)
            },
        }
    }

    /// Selects at least `n` MESSAGE_PROPAGATION peers (assuming that many are known) that are closest to `node_id` as
    /// well as other peers which do not advertise the MESSAGE_PROPAGATION flag (unless excluded by some other means
    /// e.g. `excluded` list, filter_predicate etc. The filter_predicate is called on each peer excluding them from
    /// the final results if that returns false.
    ///
    /// This ensures that peers are selected which are able to propagate the message further while still allowing
    /// clients to propagate to non-propagation nodes if required (e.g. Discovery messages)
    async fn select_closest_peers_for_propagation(
        peer_manager: &PeerManager,
        node_id: &NodeId,
        n: usize,
        excluded_peers: &[NodeId],
        features: PeerFeatures,
    ) -> Result<Vec<NodeId>, DhtActorError> {
        // Fetch to all n nearest neighbour Communication Nodes
        // which are eligible for connection.
        // Currently that means:
        // - The peer isn't banned,
        // - it has the required features
        // - it didn't recently fail to connect, and
        // - it is not in the exclusion list in closest_request
        let mut connect_ineligable_count = 0;
        let mut banned_count = 0;
        let mut excluded_count = 0;
        let mut filtered_out_node_count = 0;
        let query = PeerQuery::new()
            .select_where(|peer| {
                if peer.is_banned() {
                    banned_count += 1;
                    return false;
                }

                if !peer.features.contains(features) {
                    filtered_out_node_count += 1;
                    return false;
                }

                if peer.is_offline() {
                    connect_ineligable_count += 1;
                    return false;
                }

                let is_excluded = excluded_peers.contains(&peer.node_id);
                if is_excluded {
                    excluded_count += 1;
                    return false;
                }

                true
            })
            .sort_by(PeerQuerySortBy::DistanceFrom(node_id))
            .limit(n);

        let peers = peer_manager.perform_query(query).await?;
        let total_excluded = banned_count + connect_ineligable_count + excluded_count + filtered_out_node_count;
        if total_excluded > 0 {
            debug!(
                target: LOG_TARGET,
                "👨‍👧‍👦 Closest Peer Selection: {num_peers} peer(s) selected, {total} peer(s) not selected, {banned} \
                 banned, {filtered_out} not communication node, {not_connectable} are not connectable, {excluded} \
                 explicitly excluded",
                num_peers = peers.len(),
                total = total_excluded,
                banned = banned_count,
                filtered_out = filtered_out_node_count,
                not_connectable = connect_ineligable_count,
                excluded = excluded_count
            );
        }

        Ok(peers.into_iter().map(|p| p.node_id).collect())
    }

    async fn select_closest_node_connected(
        closest_request: Box<BroadcastClosestRequest>,
        config: &DhtConfig,
        mut connectivity: ConnectivityRequester,
        peer_manager: Arc<PeerManager>,
    ) -> Result<Vec<NodeId>, DhtActorError> {
        let connections = connectivity
            .select_connections(ConnectivitySelection::closest_to(
                closest_request.node_id.clone(),
                config.broadcast_factor,
                closest_request.excluded_peers.clone(),
            ))
            .await?;

        let mut candidates = connections
            .iter()
            .map(|conn| conn.peer_node_id())
            .cloned()
            .collect::<Vec<_>>();

        if !closest_request.connected_only {
            let excluded = closest_request
                .excluded_peers
                .iter()
                .chain(candidates.iter())
                .cloned()
                .collect::<Vec<_>>();
            // If we don't have enough connections, let's select some more disconnected peers (at least 2)
            let n = cmp::max(config.broadcast_factor.saturating_sub(candidates.len()), 2);
            let additional = Self::select_closest_peers_for_propagation(
                &peer_manager,
                &closest_request.node_id,
                n,
                &excluded,
                PeerFeatures::MESSAGE_PROPAGATION,
            )
            .await?;

            candidates.extend(additional);
        }

        Ok(candidates)
    }
}

struct DiscoveryDialTask {
    connectivity: ConnectivityRequester,
    peer_manager: Arc<PeerManager>,
    discovery: DhtDiscoveryRequester,
}

impl DiscoveryDialTask {
    pub fn new(
        connectivity: ConnectivityRequester,
        peer_manager: Arc<PeerManager>,
        discovery: DhtDiscoveryRequester,
    ) -> Self {
        Self {
            connectivity,
            peer_manager,
            discovery,
        }
    }

    pub async fn run(&mut self, public_key: CommsPublicKey) -> Result<PeerConnection, DhtActorError> {
        if self.peer_manager.exists(&public_key).await {
            let node_id = NodeId::from_public_key(&public_key);
            match self.connectivity.dial_peer(node_id).await {
                Ok(conn) => Ok(conn),
                Err(ConnectivityError::ConnectionFailed(err)) => match err {
                    ConnectionManagerError::ConnectFailedMaximumAttemptsReached |
                    ConnectionManagerError::DialConnectFailedAllAddresses => {
                        debug!(
                            target: LOG_TARGET,
                            "Dial failed for peer {}. Attempting discovery.", public_key
                        );
                        self.discover_peer(public_key).await
                    },
                    err => Err(ConnectivityError::from(err).into()),
                },
                Err(err) => Err(err.into()),
            }
        } else {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' not found, initiating discovery", public_key
            );
            self.discover_peer(public_key).await
        }
    }

    async fn discover_peer(&mut self, public_key: CommsPublicKey) -> Result<PeerConnection, DhtActorError> {
        let node_id = NodeId::from_public_key(&public_key);
        let timer = Instant::now();
        let _ = self
            .discovery
            .discover_peer(public_key.clone(), public_key.into())
            .await?;
        debug!(
            target: LOG_TARGET,
            "Discovery succeeded for peer {} in {:.2?}",
            node_id,
            timer.elapsed()
        );
        let conn = self.connectivity.dial_peer(node_id).await?;
        Ok(conn)
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use chrono::{DateTime, Utc};
    use tari_comms::{
        runtime,
        test_utils::mocks::{create_connectivity_mock, create_peer_connection_mock_pair, ConnectivityManagerMockState},
    };
    use tari_shutdown::Shutdown;
    use tari_test_utils::random;

    use super::*;
    use crate::{
        broadcast_strategy::BroadcastClosestRequest,
        envelope::NodeDestination,
        test_utils::{
            build_peer_manager,
            create_dht_discovery_mock,
            make_client_identity,
            make_node_identity,
            DhtDiscoveryMockState,
        },
    };

    async fn db_connection() -> DbConnection {
        let conn = DbConnection::connect_memory(random::string(8)).unwrap();
        conn.migrate().unwrap();
        conn
    }

    #[runtime::test]
    async fn send_join_request() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (out_tx, mut out_rx) = mpsc::channel(1);
        let (connectivity_manager, mock) = create_connectivity_mock();
        mock.spawn();
        let (actor_tx, actor_rx) = mpsc::channel(1);
        let mut requester = DhtRequester::new(actor_tx);
        let outbound_requester = OutboundMessageRequester::new(out_tx);
        let (discovery, _) = create_dht_discovery_mock(Duration::from_secs(10));
        let shutdown = Shutdown::new();
        let actor = DhtActor::new(
            Default::default(),
            db_connection().await,
            node_identity,
            peer_manager,
            connectivity_manager,
            outbound_requester,
            actor_rx,
            discovery,
            shutdown.to_signal(),
        );

        actor.spawn();

        requester.send_join().await.unwrap();
        let (params, _) = unwrap_oms_send_msg!(out_rx.recv().await.unwrap());
        assert_eq!(params.dht_message_type, DhtMessageType::Join);
    }

    mod discovery_dial_peer {
        use super::*;
        use crate::test_utils::make_peer;

        async fn setup(
            shutdown_signal: ShutdownSignal,
        ) -> (
            DhtRequester,
            Arc<NodeIdentity>,
            ConnectivityManagerMockState,
            DhtDiscoveryMockState,
            Arc<PeerManager>,
        ) {
            let node_identity = make_node_identity();
            let peer_manager = build_peer_manager();
            let (out_tx, _) = mpsc::channel(1);
            let (connectivity_manager, mock) = create_connectivity_mock();
            let connectivity_mock = mock.get_shared_state();
            mock.spawn();
            let (actor_tx, actor_rx) = mpsc::channel(1);
            let requester = DhtRequester::new(actor_tx);
            let outbound_requester = OutboundMessageRequester::new(out_tx);
            let (discovery, mock) = create_dht_discovery_mock(Duration::from_secs(10));
            let discovery_mock = mock.get_shared_state();
            mock.spawn();
            DhtActor::new(
                Default::default(),
                db_connection().await,
                node_identity.clone(),
                peer_manager.clone(),
                connectivity_manager,
                outbound_requester,
                actor_rx,
                discovery,
                shutdown_signal,
            )
            .spawn();

            (
                requester,
                node_identity,
                connectivity_mock,
                discovery_mock,
                peer_manager,
            )
        }

        #[runtime::test]
        async fn it_discovers_a_peer() {
            let shutdown = Shutdown::new();
            let (mut dht, node_identity, connectivity_mock, discovery_mock, _) = setup(shutdown.to_signal()).await;
            let peer = make_peer();
            discovery_mock.set_discover_peer_response(peer.clone());
            let (conn1, _, _, _) = create_peer_connection_mock_pair(node_identity.to_peer(), peer.clone()).await;
            connectivity_mock.add_active_connection(conn1).await;

            let conn = dht.dial_or_discover_peer(peer.public_key).await.unwrap();
            assert_eq!(*conn.peer_node_id(), peer.node_id);
            assert_eq!(discovery_mock.call_count(), 1);
        }

        #[runtime::test]
        async fn it_gets_active_peer_connection() {
            let shutdown = Shutdown::new();
            let (mut dht, node_identity, connectivity_mock, discovery_mock, peer_manager) =
                setup(shutdown.to_signal()).await;
            let peer = make_peer();
            peer_manager.add_peer(peer.clone()).await.unwrap();
            let (conn1, _, _, _) = create_peer_connection_mock_pair(node_identity.to_peer(), peer.clone()).await;
            connectivity_mock.add_active_connection(conn1).await;

            let conn = dht.dial_or_discover_peer(peer.public_key).await.unwrap();
            assert_eq!(*conn.peer_node_id(), peer.node_id);
            assert_eq!(discovery_mock.call_count(), 0);
            assert_eq!(connectivity_mock.call_count().await, 1);
        }

        #[runtime::test]
        async fn it_errors_if_discovery_fails_for_unknown_peer() {
            let shutdown = Shutdown::new();
            let (mut dht, _, connectivity_mock, discovery_mock, _) = setup(shutdown.to_signal()).await;
            let peer = make_peer();
            let _ = dht.dial_or_discover_peer(peer.public_key.clone()).await.unwrap_err();
            assert_eq!(discovery_mock.call_count(), 1);
            assert_eq!(connectivity_mock.call_count().await, 0);
        }
    }

    #[runtime::test]
    async fn insert_message_signature() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity_manager, mock) = create_connectivity_mock();
        mock.spawn();
        let (out_tx, _) = mpsc::channel(1);
        let (actor_tx, actor_rx) = mpsc::channel(1);
        let mut requester = DhtRequester::new(actor_tx);
        let (discovery, _) = create_dht_discovery_mock(Duration::from_secs(10));
        let outbound_requester = OutboundMessageRequester::new(out_tx);
        let shutdown = Shutdown::new();
        let actor = DhtActor::new(
            Default::default(),
            db_connection().await,
            node_identity,
            peer_manager,
            connectivity_manager,
            outbound_requester,
            actor_rx,
            discovery,
            shutdown.to_signal(),
        );

        actor.spawn();

        let signature = vec![1u8, 2, 3];
        let num_hits = requester
            .add_message_to_dedup_cache(signature.clone(), CommsPublicKey::default())
            .await
            .unwrap();
        assert_eq!(num_hits, 1);
        let num_hits = requester
            .add_message_to_dedup_cache(signature, CommsPublicKey::default())
            .await
            .unwrap();
        assert_eq!(num_hits, 2);
        let num_hits = requester
            .add_message_to_dedup_cache(Vec::new(), CommsPublicKey::default())
            .await
            .unwrap();
        assert_eq!(num_hits, 1);
    }

    #[runtime::test]
    async fn dedup_cache_cleanup() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity_manager, mock) = create_connectivity_mock();
        mock.spawn();
        let (out_tx, _) = mpsc::channel(1);
        let (actor_tx, actor_rx) = mpsc::channel(1);
        let mut requester = DhtRequester::new(actor_tx);
        let outbound_requester = OutboundMessageRequester::new(out_tx);
        let (discovery, _) = create_dht_discovery_mock(Duration::from_secs(10));
        let shutdown = Shutdown::new();
        // Note: This must be equal or larger than the minimum dedup cache capacity for DedupCacheDatabase
        let capacity = 10;
        let actor = DhtActor::new(
            Arc::new(DhtConfig {
                dedup_cache_capacity: capacity,
                ..Default::default()
            }),
            db_connection().await,
            node_identity,
            peer_manager,
            connectivity_manager,
            outbound_requester,
            actor_rx,
            discovery,
            shutdown.to_signal(),
        );

        // Create signatures for double the dedup cache capacity
        let signatures = (0..(capacity * 2)).map(|i| vec![1u8, 2, i as u8]).collect::<Vec<_>>();

        // Pre-populate the dedup cache; everything should be accepted because the cleanup ticker has not run yet
        for key in &signatures {
            let num_hits = actor
                .msg_hash_dedup_cache
                .add_body_hash(key.clone(), CommsPublicKey::default())
                .unwrap();
            assert_eq!(num_hits, 1);
        }
        // Try to re-insert all; all hashes should have incremented their hit count
        for key in &signatures {
            let num_hits = actor
                .msg_hash_dedup_cache
                .add_body_hash(key.clone(), CommsPublicKey::default())
                .unwrap();
            assert_eq!(num_hits, 2);
        }

        let dedup_cache_db = actor.msg_hash_dedup_cache.clone();
        // The cleanup ticker starts when the actor is spawned; the first cleanup event will fire fairly soon after the
        // task is running on a thread. To remove this race condition, we trim the cache in the test.
        let num_trimmed = dedup_cache_db.trim_entries().unwrap();
        assert_eq!(num_trimmed, 10);
        actor.spawn();

        // Verify that the last half of the signatures are still present in the cache
        for key in signatures.iter().take(capacity * 2).skip(capacity) {
            let num_hits = requester
                .add_message_to_dedup_cache(key.clone(), CommsPublicKey::default())
                .await
                .unwrap();
            assert_eq!(num_hits, 3);
        }
        // Verify that the first half of the signatures have been removed and can be re-inserted into cache
        for key in signatures.iter().take(capacity) {
            let num_hits = requester
                .add_message_to_dedup_cache(key.clone(), CommsPublicKey::default())
                .await
                .unwrap();
            assert_eq!(num_hits, 1);
        }

        // Trim the database of excess entries
        dedup_cache_db.trim_entries().unwrap();

        // Verify that the last half of the signatures have been removed and can be re-inserted into cache
        for key in signatures.iter().take(capacity * 2).skip(capacity) {
            let num_hits = requester
                .add_message_to_dedup_cache(key.clone(), CommsPublicKey::default())
                .await
                .unwrap();
            assert_eq!(num_hits, 1);
        }
    }

    #[runtime::test]
    async fn select_peers() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();

        let client_node_identity = make_client_identity();
        peer_manager.add_peer(client_node_identity.to_peer()).await.unwrap();

        let (connectivity_manager, mock) = create_connectivity_mock();
        let connectivity_manager_mock_state = mock.get_shared_state();
        mock.spawn();

        let (discovery, _) = create_dht_discovery_mock(Duration::from_secs(10));
        let (conn_in, _, conn_out, _) =
            create_peer_connection_mock_pair(client_node_identity.to_peer(), node_identity.to_peer()).await;
        connectivity_manager_mock_state.add_active_connection(conn_in).await;

        peer_manager.add_peer(make_node_identity().to_peer()).await.unwrap();

        let (out_tx, _) = mpsc::channel(1);
        let (actor_tx, actor_rx) = mpsc::channel(1);
        let mut requester = DhtRequester::new(actor_tx);
        let outbound_requester = OutboundMessageRequester::new(out_tx);
        let shutdown = Shutdown::new();
        let actor = DhtActor::new(
            Default::default(),
            db_connection().await,
            Arc::clone(&node_identity),
            peer_manager,
            connectivity_manager,
            outbound_requester,
            actor_rx,
            discovery,
            shutdown.to_signal(),
        );

        actor.spawn();

        let peers = requester
            .select_peers(BroadcastStrategy::Broadcast(Vec::new()))
            .await
            .unwrap();

        assert_eq!(peers.len(), 0);

        connectivity_manager_mock_state
            .set_selected_connections(vec![conn_out.clone()])
            .await;

        let peers = requester
            .select_peers(BroadcastStrategy::Broadcast(Vec::new()))
            .await
            .unwrap();
        assert_eq!(peers.len(), 1);

        let peers = requester
            .select_peers(BroadcastStrategy::Propagate(NodeDestination::Unknown, Vec::new()))
            .await
            .unwrap();
        assert_eq!(peers.len(), 1);

        let peers = requester
            .select_peers(BroadcastStrategy::Propagate(
                conn_out.peer_node_id().clone().into(),
                Vec::new(),
            ))
            .await
            .unwrap();
        assert_eq!(peers.len(), 1);

        let send_request = Box::new(BroadcastClosestRequest {
            node_id: node_identity.node_id().clone(),
            excluded_peers: vec![],
            connected_only: false,
        });
        let peers = requester
            .select_peers(BroadcastStrategy::ClosestNodes(send_request))
            .await
            .unwrap();
        assert_eq!(peers.len(), 2);

        let send_request = Box::new(BroadcastClosestRequest {
            node_id: node_identity.node_id().clone(),
            excluded_peers: vec![],
            connected_only: false,
        });
        let peers = requester
            .select_peers(BroadcastStrategy::DirectOrClosestNodes(send_request))
            .await
            .unwrap();
        assert_eq!(peers.len(), 1);

        let send_request = Box::new(BroadcastClosestRequest {
            node_id: client_node_identity.node_id().clone(),
            excluded_peers: vec![],
            connected_only: false,
        });
        let peers = requester
            .select_peers(BroadcastStrategy::DirectOrClosestNodes(send_request))
            .await
            .unwrap();
        assert_eq!(peers.len(), 2);

        let peers = requester
            .select_peers(BroadcastStrategy::DirectNodeId(Box::new(
                client_node_identity.node_id().clone(),
            )))
            .await
            .unwrap();

        assert_eq!(peers.len(), 1);
    }

    #[runtime::test]
    async fn get_and_set_metadata() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (out_tx, _out_rx) = mpsc::channel(1);
        let (actor_tx, actor_rx) = mpsc::channel(1);
        let (connectivity_manager, mock) = create_connectivity_mock();
        mock.spawn();
        let mut requester = DhtRequester::new(actor_tx);
        let (discovery, _) = create_dht_discovery_mock(Duration::from_secs(10));
        let outbound_requester = OutboundMessageRequester::new(out_tx);
        let mut shutdown = Shutdown::new();
        let actor = DhtActor::new(
            Default::default(),
            db_connection().await,
            node_identity,
            peer_manager,
            connectivity_manager,
            outbound_requester,
            actor_rx,
            discovery,
            shutdown.to_signal(),
        );

        actor.spawn();

        assert!(requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::OfflineTimestamp)
            .await
            .unwrap()
            .is_none());
        let ts = Utc::now();
        requester
            .set_metadata(DhtMetadataKey::OfflineTimestamp, ts)
            .await
            .unwrap();

        let got_ts = requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::OfflineTimestamp)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got_ts, ts);

        // Check upsert
        let ts = Utc::now().checked_add_signed(chrono::Duration::seconds(123)).unwrap();
        requester
            .set_metadata(DhtMetadataKey::OfflineTimestamp, ts)
            .await
            .unwrap();

        let got_ts = requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::OfflineTimestamp)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got_ts, ts);

        shutdown.trigger();
    }
}
