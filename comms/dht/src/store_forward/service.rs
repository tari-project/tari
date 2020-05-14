// Copyright 2020, The Tari Project
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
    database::{NewStoredMessage, StoreAndForwardDatabase, StoredMessage},
    message::StoredMessagePriority,
    SafResult,
    StoreAndForwardError,
};
use crate::{
    envelope::DhtMessageType,
    outbound::{OutboundMessageRequester, SendMessageParams},
    proto::store_forward::{stored_messages_response::SafResponseType, StoredMessagesRequest},
    storage::{DbConnection, DhtMetadataKey},
    DhtConfig,
    DhtRequester,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::{
    channel::{mpsc, oneshot},
    stream::Fuse,
    SinkExt,
    StreamExt,
};
use log::*;
use std::{convert::TryFrom, sync::Arc, time::Duration};
use tari_comms::{
    connection_manager::ConnectionManagerRequester,
    peer_manager::{node_id::NodeDistance, NodeId, PeerFeatures},
    types::CommsPublicKey,
    ConnectionManagerEvent,
    NodeIdentity,
    PeerManager,
};
use tari_shutdown::ShutdownSignal;
use tari_utilities::ByteArray;
use tokio::{sync::broadcast, task, time};

const LOG_TARGET: &str = "comms::dht::storeforward::actor";
/// The interval to initiate a database cleanup.
/// This involves cleaning up messages which have been stored too long according to their priority
const CLEANUP_INTERVAL: Duration = Duration::from_secs(10 * 60); // 10 mins

#[derive(Debug, Clone)]
pub struct FetchStoredMessageQuery {
    public_key: Box<CommsPublicKey>,
    node_id: Box<NodeId>,
    since: Option<DateTime<Utc>>,
    dist_threshold: Option<Box<NodeDistance>>,
    response_type: SafResponseType,
}

impl FetchStoredMessageQuery {
    pub fn new(public_key: Box<CommsPublicKey>, node_id: Box<NodeId>) -> Self {
        Self {
            public_key,
            node_id,
            since: None,
            response_type: SafResponseType::Anonymous,
            dist_threshold: None,
        }
    }

    pub fn since(&mut self, since: DateTime<Utc>) -> &mut Self {
        self.since = Some(since);
        self
    }

    pub fn with_response_type(&mut self, response_type: SafResponseType) -> &mut Self {
        self.response_type = response_type;
        self
    }

    pub fn with_dist_threshold(&mut self, dist_threshold: Box<NodeDistance>) -> &mut Self {
        self.dist_threshold = Some(dist_threshold);
        self
    }
}

#[derive(Debug)]
pub enum StoreAndForwardRequest {
    FetchMessages(FetchStoredMessageQuery, oneshot::Sender<SafResult<Vec<StoredMessage>>>),
    InsertMessage(NewStoredMessage),
    RemoveMessages(Vec<i32>),
    SendStoreForwardRequestToPeer(Box<NodeId>),
    SendStoreForwardRequestNeighbours,
}

#[derive(Clone)]
pub struct StoreAndForwardRequester {
    sender: mpsc::Sender<StoreAndForwardRequest>,
}

impl StoreAndForwardRequester {
    pub fn new(sender: mpsc::Sender<StoreAndForwardRequest>) -> Self {
        Self { sender }
    }

    pub async fn fetch_messages(&mut self, request: FetchStoredMessageQuery) -> SafResult<Vec<StoredMessage>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(StoreAndForwardRequest::FetchMessages(request, reply_tx))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        reply_rx.await.map_err(|_| StoreAndForwardError::RequestCancelled)?
    }

    pub async fn insert_message(&mut self, message: NewStoredMessage) -> SafResult<()> {
        self.sender
            .send(StoreAndForwardRequest::InsertMessage(message))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        Ok(())
    }

    pub async fn remove_messages(&mut self, message_ids: Vec<i32>) -> SafResult<()> {
        self.sender
            .send(StoreAndForwardRequest::RemoveMessages(message_ids))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        Ok(())
    }

    pub async fn request_saf_messages_from_peer(&mut self, node_id: NodeId) -> SafResult<()> {
        self.sender
            .send(StoreAndForwardRequest::SendStoreForwardRequestToPeer(Box::new(node_id)))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        Ok(())
    }

    pub async fn request_saf_messages_from_neighbours(&mut self) -> SafResult<()> {
        self.sender
            .send(StoreAndForwardRequest::SendStoreForwardRequestNeighbours)
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        Ok(())
    }
}

pub struct StoreAndForwardService {
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    dht_requester: DhtRequester,
    database: StoreAndForwardDatabase,
    peer_manager: Arc<PeerManager>,
    connection_events: Fuse<broadcast::Receiver<Arc<ConnectionManagerEvent>>>,
    outbound_requester: OutboundMessageRequester,
    request_rx: Fuse<mpsc::Receiver<StoreAndForwardRequest>>,
    shutdown_signal: Option<ShutdownSignal>,
}

impl StoreAndForwardService {
    pub fn new(
        config: DhtConfig,
        conn: DbConnection,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        dht_requester: DhtRequester,
        connection_manager: ConnectionManagerRequester,
        outbound_requester: OutboundMessageRequester,
        request_rx: mpsc::Receiver<StoreAndForwardRequest>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            config,
            database: StoreAndForwardDatabase::new(conn),
            node_identity,
            peer_manager,
            dht_requester,
            request_rx: request_rx.fuse(),
            connection_events: connection_manager.get_event_subscription().fuse(),
            outbound_requester,
            shutdown_signal: Some(shutdown_signal),
        }
    }

    pub async fn spawn(self) -> SafResult<()> {
        info!(target: LOG_TARGET, "Store and forward service started");
        task::spawn(Self::run(self));
        Ok(())
    }

    async fn run(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("StoreAndForwardActor initialized without shutdown_signal");

        let mut cleanup_ticker = time::interval(CLEANUP_INTERVAL).fuse();

        loop {
            futures::select! {
                request = self.request_rx.select_next_some() => {
                    self.handle_request(request).await;
                },

               event = self.connection_events.select_next_some() => {
                    if let Ok(event) = event {
                         if let Err(err) = self.handle_connection_manager_event(&event).await {
                            error!(target: LOG_TARGET, "Error handling connection manager event: {:?}", err);
                        }
                    }
                },

                _ = cleanup_ticker.select_next_some() => {
                    if let Err(err) = self.cleanup().await {
                        error!(target: LOG_TARGET, "Error when performing store and forward cleanup: {:?}", err);
                    }
                },

                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "StoreAndForwardActor is shutting down because the shutdown signal was triggered");
                    break;
                }
            }
        }
    }

    async fn handle_request(&mut self, request: StoreAndForwardRequest) {
        use StoreAndForwardRequest::*;
        trace!(target: LOG_TARGET, "Request: {:?}", request);
        match request {
            FetchMessages(query, reply_tx) => match self.handle_fetch_message_query(query).await {
                Ok(messages) => {
                    let _ = reply_tx.send(Ok(messages));
                },
                Err(err) => {
                    error!(
                        target: LOG_TARGET,
                        "Failed to fetch stored messages because '{:?}'", err
                    );
                    let _ = reply_tx.send(Err(err));
                },
            },
            InsertMessage(msg) => {
                let public_key = msg.destination_pubkey.clone();
                let node_id = msg.destination_node_id.clone();
                match self.database.insert_message(msg).await {
                    Ok(_) => info!(
                        target: LOG_TARGET,
                        "Stored message for {}",
                        public_key
                            .map(|p| format!("public key '{}'", p))
                            .or_else(|| node_id.map(|n| format!("node id '{}'", n)))
                            .unwrap_or_else(|| "<Anonymous>".to_string())
                    ),
                    Err(err) => {
                        error!(target: LOG_TARGET, "InsertMessage failed because '{:?}'", err);
                    },
                }
            },
            RemoveMessages(message_ids) => match self.database.remove_message(message_ids.clone()).await {
                Ok(_) => trace!(target: LOG_TARGET, "Removed messages: {:?}", message_ids),
                Err(err) => error!(target: LOG_TARGET, "RemoveMessage failed because '{:?}'", err),
            },
            SendStoreForwardRequestToPeer(node_id) => {
                if let Err(err) = self.request_stored_messages_from_peer(&node_id).await {
                    error!(target: LOG_TARGET, "Error sending store and forward request: {:?}", err);
                }
            },
            SendStoreForwardRequestNeighbours => {
                if let Err(err) = self.request_stored_messages_neighbours().await {
                    error!(
                        target: LOG_TARGET,
                        "Error sending store and forward request to neighbours: {:?}", err
                    );
                }
            },
        }
    }

    async fn handle_connection_manager_event(&mut self, event: &ConnectionManagerEvent) -> SafResult<()> {
        use ConnectionManagerEvent::*;
        if !self.config.saf_auto_request {
            debug!(
                target: LOG_TARGET,
                "Auto store and forward request disabled. Ignoring connection manager event"
            );
            return Ok(());
        }

        match event {
            PeerConnected(conn) => {
                // Whenever we connect to a peer, request SAF messages
                let features = self.peer_manager.get_peer_features(conn.peer_node_id()).await?;
                if features.contains(PeerFeatures::DHT_STORE_FORWARD) {
                    info!(
                        target: LOG_TARGET,
                        "Connected peer '{}' is a SAF node. Requesting stored messages.",
                        conn.peer_node_id().short_str()
                    );
                    self.request_stored_messages_from_peer(conn.peer_node_id()).await?;
                }
            },
            _ => {},
        }

        Ok(())
    }

    async fn request_stored_messages_from_peer(&mut self, node_id: &NodeId) -> SafResult<()> {
        let request = self.get_saf_request().await?;
        info!(
            target: LOG_TARGET,
            "Sending store and forward request to peer '{}' (Since = {:?})", node_id, request.since
        );

        self.outbound_requester
            .send_message_no_header(
                SendMessageParams::new()
                    .direct_node_id(node_id.clone())
                    .with_dht_message_type(DhtMessageType::SafRequestMessages)
                    .finish(),
                request,
            )
            .await
            .map_err(StoreAndForwardError::RequestMessagesFailed)?;

        Ok(())
    }

    async fn request_stored_messages_neighbours(&mut self) -> SafResult<()> {
        let request = self.get_saf_request().await?;
        info!(
            target: LOG_TARGET,
            "Sending store and forward request to neighbours (Since = {:?})", request.since
        );
        self.outbound_requester
            .send_message_no_header(
                SendMessageParams::new()
                    .broadcast(vec![])
                    .with_dht_message_type(DhtMessageType::SafRequestMessages)
                    .finish(),
                request,
            )
            .await
            .map_err(StoreAndForwardError::RequestMessagesFailed)?;

        Ok(())
    }

    async fn get_saf_request(&mut self) -> SafResult<StoredMessagesRequest> {
        let mut request = self
            .dht_requester
            .get_metadata(DhtMetadataKey::OfflineTimestamp)
            .await?
            .map(StoredMessagesRequest::since)
            .unwrap_or_else(StoredMessagesRequest::new);

        // Calculate the network region threshold for our node id.
        // i.e. "Give me all messages that are this close to my node ID"
        let threshold = self
            .peer_manager
            .calc_region_threshold(
                self.node_identity.node_id(),
                self.config.num_neighbouring_nodes,
                PeerFeatures::DHT_STORE_FORWARD,
            )
            .await?;

        request.dist_threshold = threshold.to_vec();

        Ok(request)
    }

    async fn handle_fetch_message_query(&self, query: FetchStoredMessageQuery) -> SafResult<Vec<StoredMessage>> {
        use SafResponseType::*;
        let limit = i64::try_from(self.config.saf_max_returned_messages)
            .ok()
            .or(Some(std::i64::MAX))
            .unwrap();
        let db = &self.database;
        let messages = match query.response_type {
            ForMe => {
                db.find_messages_for_peer(&query.public_key, &query.node_id, query.since, limit)
                    .await?
            },
            Join => db.find_join_messages(query.since, limit).await?,
            Discovery => {
                db.find_messages_of_type_for_pubkey(&query.public_key, DhtMessageType::Discovery, query.since, limit)
                    .await?
            },
            Anonymous => db.find_anonymous_messages(query.since, limit).await?,
            InRegion => {
                db.find_regional_messages(&query.node_id, query.dist_threshold, query.since, limit)
                    .await?
            },
        };

        Ok(messages)
    }

    async fn cleanup(&self) -> SafResult<()> {
        let num_removed = self
            .database
            .delete_messages_with_priority_older_than(
                StoredMessagePriority::Low,
                since(self.config.saf_low_priority_msg_storage_ttl),
            )
            .await?;
        info!(target: LOG_TARGET, "Cleaned {} old low priority messages", num_removed);

        let num_removed = self
            .database
            .delete_messages_with_priority_older_than(
                StoredMessagePriority::High,
                since(self.config.saf_high_priority_msg_storage_ttl),
            )
            .await?;
        info!(target: LOG_TARGET, "Cleaned {} old high priority messages", num_removed);

        let num_removed = self
            .database
            .truncate_messages(self.config.saf_msg_storage_capacity)
            .await?;
        if num_removed > 0 {
            info!(
                target: LOG_TARGET,
                "Storage limits exceeded, removing {} oldest messages", num_removed
            );
        }

        Ok(())
    }
}

fn since(period: Duration) -> NaiveDateTime {
    use chrono::Duration as OldDuration;
    let period = OldDuration::from_std(period).expect("period was out of range for chrono::Duration");
    Utc::now()
        .naive_utc()
        .checked_sub_signed(period)
        .expect("period overflowed when used with checked_sub_signed")
}
