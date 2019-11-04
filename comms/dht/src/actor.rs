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

use crate::{
    broadcast_strategy::{BroadcastClosestRequest, BroadcastStrategy},
    envelope::NodeDestination,
    outbound::{OutboundEncryption, OutboundMessageRequester},
    proto::{
        dht::{DiscoverMessage, JoinMessage},
        envelope::DhtMessageType,
        store_forward::StoredMessagesRequest,
    },
    DhtConfig,
};
use chrono::{DateTime, Utc};
use derive_error::Error;
use futures::{
    channel::{mpsc, mpsc::SendError, oneshot},
    future,
    future::BoxFuture,
    stream::{Fuse, FuturesUnordered},
    FutureExt,
    SinkExt,
    StreamExt,
};
use log::*;
use std::sync::Arc;
use tari_comms::{
    peer_manager::{NodeId, NodeIdentity, Peer, PeerManager, PeerManagerError, PeerQuery, PeerQuerySortBy},
    types::CommsPublicKey,
};
use tari_shutdown::ShutdownSignal;
use tari_utilities::ByteArray;
use tokio_executor::blocking;
use ttl_cache::TtlCache;

const LOG_TARGET: &'static str = "comms::dht::actor";

#[derive(Debug, Error)]
pub enum DhtActorError {
    /// MPSC channel is disconnected
    ChannelDisconnected,
    /// MPSC sender was unable to send because the channel buffer is full
    SendBufferFull,
    /// Reply sender canceled the request
    ReplyCanceled,
    PeerManagerError(PeerManagerError),
    #[error(msg_embedded, no_from, non_std)]
    SendFailed(String),
}

impl From<SendError> for DhtActorError {
    fn from(err: SendError) -> Self {
        if err.is_disconnected() {
            DhtActorError::ChannelDisconnected
        } else if err.is_full() {
            DhtActorError::SendBufferFull
        } else {
            unreachable!();
        }
    }
}

#[derive(Debug)]
pub enum DhtRequest {
    /// Send a Join request to the network
    SendJoin,
    /// Send a discover request for a network region or node
    SendDiscover {
        dest_public_key: CommsPublicKey,
        dest_node_id: Option<NodeId>,
        destination: NodeDestination,
    },
    /// Send a request for stored messages, optionally specifying a date time that the foreign node should
    /// use to filter the returned messages.
    SendRequestStoredMessages(Option<DateTime<Utc>>),
    /// Inserts a message signature to the signature cache. This operation replies with a boolean
    /// which is true if the signature already exists in the cache, otherwise false
    SignatureCacheInsert(Vec<u8>, oneshot::Sender<bool>),
    /// Fetch selected peers according to the broadcast strategy
    SelectPeers(BroadcastStrategy, oneshot::Sender<Vec<Peer>>),
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

    pub async fn select_peers(&mut self, broadcast_strategy: BroadcastStrategy) -> Result<Vec<Peer>, DhtActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(DhtRequest::SelectPeers(broadcast_strategy, reply_tx))
            .await?;
        reply_rx.await.map_err(|_| DhtActorError::ReplyCanceled)
    }

    pub async fn send_discover(
        &mut self,
        dest_public_key: CommsPublicKey,
        dest_node_id: Option<NodeId>,
        destination: NodeDestination,
    ) -> Result<(), DhtActorError>
    {
        self.sender
            .send(DhtRequest::SendDiscover {
                dest_public_key,
                dest_node_id,
                destination,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn insert_message_signature(&mut self, signature: Vec<u8>) -> Result<bool, DhtActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(DhtRequest::SignatureCacheInsert(signature, reply_tx))
            .await?;

        reply_rx.await.map_err(|_| DhtActorError::ReplyCanceled)
    }

    pub async fn send_request_stored_messages(&mut self) -> Result<(), DhtActorError> {
        self.sender
            .send(DhtRequest::SendRequestStoredMessages(None))
            .await
            .map_err(Into::into)
    }
}

pub struct DhtActor {
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    outbound_requester: OutboundMessageRequester,
    config: DhtConfig,
    shutdown_signal: Option<ShutdownSignal>,
    request_rx: Fuse<mpsc::Receiver<DhtRequest>>,
    signature_cache: TtlCache<Vec<u8>, ()>,
}

impl DhtActor {
    pub fn new(
        config: DhtConfig,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        outbound_requester: OutboundMessageRequester,
        request_rx: mpsc::Receiver<DhtRequest>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            signature_cache: TtlCache::new(config.signature_cache_capacity),
            config,
            outbound_requester,
            peer_manager,
            node_identity,
            shutdown_signal: Some(shutdown_signal),
            request_rx: request_rx.fuse(),
        }
    }

    pub async fn start(mut self) {
        let mut pending_jobs = FuturesUnordered::new();

        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("DhtActor initialized without shutdown_signal")
            .fuse();

        loop {
            futures::select! {
                request = self.request_rx.select_next_some() => {
                    debug!(target: LOG_TARGET, "DhtActor received message: {:?}", request);
                    pending_jobs.push(self.request_handler(request));
                },

                result = pending_jobs.select_next_some() => {
                    match result {
                        Ok(_) => {
                            trace!(target: LOG_TARGET, "Successfully handled DHT request message");
                        },
                        Err(err) => {
                            error!(target: LOG_TARGET, "Error when handling DHT request message. {}", err);
                        },
                    }
                },

                _guard = shutdown_signal => {
                    info!(target: LOG_TARGET, "DhtActor is shutting down because it received a shutdown signal.");
                    break;
                },
                complete => {
                    info!(target: LOG_TARGET, "DhtActor is shutting down because the request stream ended.");
                    break;
                }
            }
        }
    }

    fn request_handler(&mut self, request: DhtRequest) -> BoxFuture<'static, Result<(), DhtActorError>> {
        use DhtRequest::*;
        match request {
            SendJoin => {
                let node_identity = Arc::clone(&self.node_identity);
                let outbound_requester = self.outbound_requester.clone();
                Box::pin(Self::send_join(
                    node_identity,
                    outbound_requester,
                    self.config.num_neighbouring_nodes,
                ))
            },
            SendDiscover {
                destination,
                dest_node_id,
                dest_public_key,
            } => {
                let node_identity = Arc::clone(&self.node_identity);
                let outbound_requester = self.outbound_requester.clone();
                Box::pin(Self::send_discover(
                    node_identity,
                    outbound_requester,
                    self.config.num_neighbouring_nodes,
                    dest_public_key,
                    dest_node_id,
                    destination,
                ))
            },

            SignatureCacheInsert(signature, reply_tx) => {
                // No locks needed here. Downside is this isn't really async, however this should be
                // fine as it is very quick
                let already_exists = self
                    .signature_cache
                    .insert(signature, (), self.config.signature_cache_ttl)
                    .is_some();
                let _ = reply_tx.send(already_exists);
                Box::pin(future::ready(Ok(())))
            },
            SelectPeers(broadcast_strategy, reply_tx) => {
                let peer_manager = Arc::clone(&self.peer_manager);
                let node_identity = Arc::clone(&self.node_identity);
                let num_neighbouring_nodes = self.config.num_neighbouring_nodes;
                Box::pin(blocking::run(move || {
                    match Self::select_peers(node_identity, peer_manager, num_neighbouring_nodes, broadcast_strategy) {
                        Ok(peers) => {
                            let _ = reply_tx.send(peers);
                            Ok(())
                        },
                        Err(err) => {
                            let _ = reply_tx.send(Vec::new());
                            Err(err)
                        },
                    }
                }))
            },
            SendRequestStoredMessages(maybe_since) => {
                let node_identity = Arc::clone(&self.node_identity);
                let outbound_requester = self.outbound_requester.clone();
                Box::pin(Self::request_stored_messages(
                    node_identity,
                    outbound_requester,
                    self.config.num_neighbouring_nodes,
                    maybe_since,
                ))
            },
        }
    }

    async fn send_join(
        node_identity: Arc<NodeIdentity>,
        mut outbound_requester: OutboundMessageRequester,
        num_neighbouring_nodes: usize,
    ) -> Result<(), DhtActorError>
    {
        let message = JoinMessage {
            node_id: node_identity.node_id().to_vec(),
            addresses: vec![node_identity.control_service_address().to_string()],
            peer_features: node_identity.features().bits(),
        };

        debug!(
            target: LOG_TARGET,
            "Sending Join message to (at most) {} closest peers", num_neighbouring_nodes
        );

        outbound_requester
            .send_dht_message(
                BroadcastStrategy::Closest(Box::new(BroadcastClosestRequest {
                    n: num_neighbouring_nodes,
                    node_id: node_identity.node_id().clone(),
                    excluded_peers: Vec::new(),
                })),
                NodeDestination::Unknown,
                OutboundEncryption::None,
                DhtMessageType::Join,
                message,
            )
            .await
            .map_err(|err| DhtActorError::SendFailed(format!("Failed to send join message: {}", err)))?;

        Ok(())
    }

    async fn send_discover(
        node_identity: Arc<NodeIdentity>,
        mut outbound_requester: OutboundMessageRequester,
        num_neighbouring_nodes: usize,
        dest_public_key: CommsPublicKey,
        dest_node_id: Option<NodeId>,
        destination: NodeDestination,
    ) -> Result<(), DhtActorError>
    {
        let discover_msg = DiscoverMessage {
            node_id: node_identity.node_id().to_vec(),
            addresses: vec![node_identity.control_service_address().to_string()],
            peer_features: node_identity.features().bits(),
        };
        debug!(
            target: LOG_TARGET,
            "Sending Discover message to (at most) {} closest peers", num_neighbouring_nodes
        );

        // If the destination node is is known, send to the closest peers we know. Otherwise...
        let network_location_node_id = dest_node_id.unwrap_or(match &destination {
            // ... if the destination is undisclosed or a public key, send discover to our closest peers
            NodeDestination::Unknown | NodeDestination::PublicKey(_) => node_identity.node_id().clone(),
            // otherwise, send it to the closest peers to the given NodeId destination we know
            NodeDestination::NodeId(node_id) => node_id.clone(),
        });

        let broadcast_strategy = BroadcastStrategy::Closest(Box::new(BroadcastClosestRequest {
            n: num_neighbouring_nodes,
            node_id: network_location_node_id,
            excluded_peers: Vec::new(),
        }));

        outbound_requester
            .send_dht_message(
                broadcast_strategy,
                destination,
                OutboundEncryption::EncryptFor(dest_public_key),
                DhtMessageType::Discover,
                discover_msg,
            )
            .await
            .map_err(|err| DhtActorError::SendFailed(format!("Failed to send discovery message: {}", err)))?;

        Ok(())
    }

    async fn request_stored_messages(
        node_identity: Arc<NodeIdentity>,
        mut outbound_requester: OutboundMessageRequester,
        num_neighbouring_nodes: usize,
        maybe_since: Option<DateTime<Utc>>,
    ) -> Result<(), DhtActorError>
    {
        let broadcast_strategy = BroadcastStrategy::Closest(Box::new(BroadcastClosestRequest {
            n: num_neighbouring_nodes,
            node_id: node_identity.node_id().clone(),
            excluded_peers: Vec::new(),
        }));

        outbound_requester
            .send_dht_message(
                broadcast_strategy,
                NodeDestination::Unknown,
                OutboundEncryption::EncryptForDestination,
                DhtMessageType::SafRequestMessages,
                maybe_since
                    .map(StoredMessagesRequest::since)
                    .unwrap_or(StoredMessagesRequest::new()),
            )
            .await
            .map_err(|err| DhtActorError::SendFailed(format!("Failed to send request for stored messages: {}", err)))?;

        Ok(())
    }

    fn select_peers(
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        num_neighbouring_nodes: usize,
        broadcast_strategy: BroadcastStrategy,
    ) -> Result<Vec<Peer>, DhtActorError>
    {
        use BroadcastStrategy::*;
        match broadcast_strategy {
            DirectNodeId(node_id) => {
                // Send to a particular peer matching the given node ID
                peer_manager
                    .direct_identity_node_id(&node_id)
                    .map(|peer| peer.map(|p| vec![p]).unwrap_or_default())
                    .map_err(Into::into)
            },
            DirectPublicKey(public_key) => {
                // Send to a particular peer matching the given node ID
                peer_manager
                    .direct_identity_public_key(&public_key)
                    .map(|peer| peer.map(|p| vec![p]).unwrap_or_default())
                    .map_err(Into::into)
            },
            Flood => {
                // Send to all known Communication Node peers
                peer_manager.flood_peers().map_err(Into::into)
            },
            Closest(closest_request) => Self::select_closest_peers(
                peer_manager,
                &closest_request.node_id,
                closest_request.n,
                &closest_request.excluded_peers,
            ),
            Random(n) => {
                // Send to a random set of peers of size n that are Communication Nodes
                peer_manager.random_peers(n).map_err(Into::into)
            },
            // TODO: This is a common and expensive search - values here should be cached
            Neighbours(exclude) => {
                // Send to a random set of peers of size n that are Communication Nodes
                Self::select_closest_peers(peer_manager, node_identity.node_id(), num_neighbouring_nodes, &*exclude)
            },
        }
    }

    fn select_closest_peers(
        peer_manager: Arc<PeerManager>,
        node_id: &NodeId,
        n: usize,
        excluded_peers: &[CommsPublicKey],
    ) -> Result<Vec<Peer>, DhtActorError>
    {
        // TODO: This query is expensive. We can probably cache a list of neighbouring peers which are online
        // Fetch to all n nearest neighbour Communication Nodes
        // which are eligible for connection.
        // Currently that means:
        // - The peer isn't banned,
        // - it didn't recently fail to connect, and
        // - it is not in the exclusion list in closest_request
        let reconnect_cooldown_period = chrono::Duration::minutes(30);
        let mut connect_ineligable_count = 0;
        let mut banned_count = 0;
        let mut excluded_count = 0;
        let query = PeerQuery::new()
            .select_where(|peer| {
                // This is a quite ugly but is done this way to get the logging
                let is_banned = peer.is_banned();

                if is_banned {
                    banned_count += 1;
                    return false;
                }

                let is_connect_eligible = {
                    peer.connection_stats.last_connect_failed_at
                        // Did we fail to connect in the last 30 minutes?
                        .map(|failed_at| Utc::now().naive_utc() - failed_at >= reconnect_cooldown_period)
                        .unwrap_or(true)
                };

                if !is_connect_eligible {
                    connect_ineligable_count += 1;
                    return false;
                }

                let is_excluded = excluded_peers.contains(&peer.public_key);
                if is_excluded {
                    excluded_count += 1;
                    return false;
                }

                true
            })
            .sort_by(PeerQuerySortBy::DistanceFrom(&node_id))
            .limit(n);

        let peers = peer_manager.perform_query(query)?;

        let total = banned_count + connect_ineligable_count + excluded_count;
        if total > 0 {
            debug!(
                target: LOG_TARGET,
                "\n====================================\n {total} peer(s) were excluded from closest query\n {banned} \
                 banned\n {not_connectable} are not connectable\n {excluded} \
                 excluded\n====================================\n",
                total = total,
                banned = banned_count,
                not_connectable = connect_ineligable_count,
                excluded = excluded_count
            );
        }

        Ok(peers)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::{make_node_identity, make_peer_manager};
    use tari_shutdown::Shutdown;
    use tari_test_utils::runtime;
    //    #[test]
    //    fn auto_messages() {
    //        runtime::test_async(|rt| {
    //            let node_identity = make_node_identity();
    //            let peer_manager = make_peer_manager();
    //            let (out_tx, mut out_rx) = mpsc::channel(1);
    //            let (_actor_tx, actor_rx) = mpsc::channel(1);
    //            let outbound_requester = OutboundMessageRequester::new(out_tx);
    //            let shutdown = Shutdown::new();
    //            let actor = DhtActor::new(
    //                DhtConfig::default(),
    //                node_identity,
    //                peer_manager,
    //                outbound_requester,
    //                actor_rx,
    //                shutdown.to_signal(),
    //            );
    //
    //            rt.spawn(actor.start());
    //
    //            rt.block_on(async move {
    //                let request = unwrap_oms_send_msg!(out_rx.next().await.unwrap());
    //                assert_eq!(request.dht_message_type, DhtMessageType::Join);
    //                let request = unwrap_oms_send_msg!(out_rx.next().await.unwrap());
    //                assert_eq!(request.dht_message_type, DhtMessageType::SafRequestMessages);
    //            });
    //        });
    //    }

    #[test]
    fn send_join_request() {
        runtime::test_async(|rt| {
            let node_identity = make_node_identity();
            let peer_manager = make_peer_manager();
            let (out_tx, mut out_rx) = mpsc::channel(1);
            let (actor_tx, actor_rx) = mpsc::channel(1);
            let mut requester = DhtRequester::new(actor_tx);
            let outbound_requester = OutboundMessageRequester::new(out_tx);
            let shutdown = Shutdown::new();
            let actor = DhtActor::new(
                Default::default(),
                node_identity,
                peer_manager,
                outbound_requester,
                actor_rx,
                shutdown.to_signal(),
            );

            rt.spawn(actor.start());

            rt.block_on(async move {
                requester.send_join().await.unwrap();
                let request = unwrap_oms_send_msg!(out_rx.next().await.unwrap());
                assert_eq!(request.dht_message_type, DhtMessageType::Join);
            });
        });
    }

    #[test]
    fn send_discover_request() {
        runtime::test_async(|rt| {
            let node_identity = make_node_identity();
            let peer_manager = make_peer_manager();
            let (out_tx, mut out_rx) = mpsc::channel(1);
            let (actor_tx, actor_rx) = mpsc::channel(1);
            let mut requester = DhtRequester::new(actor_tx);
            let outbound_requester = OutboundMessageRequester::new(out_tx);
            let shutdown = Shutdown::new();
            let actor = DhtActor::new(
                Default::default(),
                node_identity,
                peer_manager,
                outbound_requester,
                actor_rx,
                shutdown.to_signal(),
            );

            rt.spawn(actor.start());

            rt.block_on(async move {
                requester
                    .send_discover(CommsPublicKey::default(), None, NodeDestination::Unknown)
                    .await
                    .unwrap();
                let request = unwrap_oms_send_msg!(out_rx.next().await.unwrap());
                assert_eq!(request.dht_message_type, DhtMessageType::Discover);
            });
        });
    }

    #[test]
    fn insert_message_signature() {
        runtime::test_async(|rt| {
            let node_identity = make_node_identity();
            let peer_manager = make_peer_manager();
            let (out_tx, _) = mpsc::channel(1);
            let (actor_tx, actor_rx) = mpsc::channel(1);
            let mut requester = DhtRequester::new(actor_tx);
            let outbound_requester = OutboundMessageRequester::new(out_tx);
            let shutdown = Shutdown::new();
            let actor = DhtActor::new(
                Default::default(),
                node_identity,
                peer_manager,
                outbound_requester,
                actor_rx,
                shutdown.to_signal(),
            );

            rt.spawn(actor.start());

            rt.block_on(async move {
                let signature = vec![1u8, 2, 3];
                let is_dup = requester.insert_message_signature(signature.clone()).await.unwrap();
                assert_eq!(is_dup, false);
                let is_dup = requester.insert_message_signature(signature).await.unwrap();
                assert_eq!(is_dup, true);
                let is_dup = requester.insert_message_signature(Vec::new()).await.unwrap();
                assert_eq!(is_dup, false);
            });
        });
    }
}
