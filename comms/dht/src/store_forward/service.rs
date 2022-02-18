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

use std::{convert::TryFrom, sync::Arc, time::Duration};

use chrono::{DateTime, NaiveDateTime, Utc};
use log::*;
use tari_comms::{
    connectivity::{ConnectivityEvent, ConnectivityEventRx, ConnectivityRequester},
    peer_manager::{NodeId, PeerFeatures},
    types::CommsPublicKey,
    PeerManager,
};
use tari_shutdown::ShutdownSignal;
use tokio::{
    sync::{mpsc, oneshot},
    task,
    time,
    time::MissedTickBehavior,
};

use super::{
    database::{NewStoredMessage, StoreAndForwardDatabase, StoredMessage},
    message::StoredMessagePriority,
    SafResult,
    StoreAndForwardError,
};
use crate::{
    broadcast_strategy::BroadcastStrategy,
    envelope::DhtMessageType,
    event::{DhtEvent, DhtEventSender},
    outbound::{OutboundMessageRequester, SendMessageParams},
    proto::store_forward::{stored_messages_response::SafResponseType, StoredMessagesRequest},
    storage::{DbConnection, DhtMetadataKey},
    store_forward::{local_state::SafLocalState, SafConfig},
    DhtRequester,
};

const LOG_TARGET: &str = "comms::dht::storeforward::actor";
/// The interval to initiate a database cleanup.
/// This involves cleaning up messages which have been stored too long according to their priority
const CLEANUP_INTERVAL: Duration = Duration::from_secs(10 * 60); // 10 mins

#[derive(Debug, Clone)]
pub struct FetchStoredMessageQuery {
    public_key: Box<CommsPublicKey>,
    node_id: Box<NodeId>,
    since: Option<DateTime<Utc>>,
    response_type: SafResponseType,
}

impl FetchStoredMessageQuery {
    pub fn new(public_key: Box<CommsPublicKey>, node_id: Box<NodeId>) -> Self {
        Self {
            public_key,
            node_id,
            since: None,
            response_type: SafResponseType::Anonymous,
        }
    }

    pub fn with_messages_since(&mut self, since: DateTime<Utc>) -> &mut Self {
        self.since = Some(since);
        self
    }

    pub fn with_response_type(&mut self, response_type: SafResponseType) -> &mut Self {
        self.response_type = response_type;
        self
    }

    pub fn since(&self) -> Option<DateTime<Utc>> {
        self.since
    }
}

#[derive(Debug)]
pub enum StoreAndForwardRequest {
    FetchMessages(FetchStoredMessageQuery, oneshot::Sender<SafResult<Vec<StoredMessage>>>),
    InsertMessage(NewStoredMessage, oneshot::Sender<SafResult<bool>>),
    RemoveMessages(Vec<i32>),
    RemoveMessagesOlderThan(DateTime<Utc>),
    SendStoreForwardRequestToPeer(NodeId),
    SendStoreForwardRequestNeighbours,
    MarkSafResponseReceived(NodeId, oneshot::Sender<Option<Duration>>),
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

    pub async fn insert_message(&mut self, message: NewStoredMessage) -> SafResult<bool> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(StoreAndForwardRequest::InsertMessage(message, reply_tx))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        reply_rx.await.map_err(|_| StoreAndForwardError::RequestCancelled)?
    }

    pub async fn remove_messages(&mut self, message_ids: Vec<i32>) -> SafResult<()> {
        self.sender
            .send(StoreAndForwardRequest::RemoveMessages(message_ids))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        Ok(())
    }

    pub async fn remove_messages_older_than(&mut self, threshold: DateTime<Utc>) -> SafResult<()> {
        self.sender
            .send(StoreAndForwardRequest::RemoveMessagesOlderThan(threshold))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        Ok(())
    }

    pub async fn request_saf_messages_from_peer(&mut self, node_id: NodeId) -> SafResult<()> {
        self.sender
            .send(StoreAndForwardRequest::SendStoreForwardRequestToPeer(node_id))
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

    pub async fn mark_saf_response_received(&mut self, peer: NodeId) -> SafResult<Option<Duration>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(StoreAndForwardRequest::MarkSafResponseReceived(peer, reply_tx))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        reply_rx.await.map_err(|_| StoreAndForwardError::RequestCancelled)
    }
}

pub struct StoreAndForwardService {
    config: SafConfig,
    dht_requester: DhtRequester,
    database: StoreAndForwardDatabase,
    peer_manager: Arc<PeerManager>,
    connection_events: ConnectivityEventRx,
    outbound_requester: OutboundMessageRequester,
    request_rx: mpsc::Receiver<StoreAndForwardRequest>,
    shutdown_signal: ShutdownSignal,
    num_received_saf_responses: Option<usize>,
    num_online_peers: Option<usize>,
    saf_response_signal_rx: mpsc::Receiver<()>,
    event_publisher: DhtEventSender,
    local_state: SafLocalState,
}

impl StoreAndForwardService {
    pub fn new(
        config: SafConfig,
        conn: DbConnection,
        peer_manager: Arc<PeerManager>,
        dht_requester: DhtRequester,
        connectivity: ConnectivityRequester,
        outbound_requester: OutboundMessageRequester,
        request_rx: mpsc::Receiver<StoreAndForwardRequest>,
        saf_response_signal_rx: mpsc::Receiver<()>,
        event_publisher: DhtEventSender,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            config,
            database: StoreAndForwardDatabase::new(conn),
            peer_manager,
            dht_requester,
            request_rx,
            connection_events: connectivity.get_event_subscription(),
            outbound_requester,
            shutdown_signal,
            num_received_saf_responses: Some(0),
            num_online_peers: None,
            saf_response_signal_rx,
            event_publisher,
            local_state: Default::default(),
        }
    }

    pub fn spawn(self) {
        debug!(target: LOG_TARGET, "Store and forward service started");
        task::spawn(self.run());
    }

    async fn run(mut self) {
        let mut cleanup_ticker = time::interval(CLEANUP_INTERVAL);
        cleanup_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                Some(request) = self.request_rx.recv() => {
                    self.handle_request(request).await;
                },

               event = self.connection_events.recv() => {
                    if let Ok(event) = event {
                         if let Err(err) = self.handle_connectivity_event(&event).await {
                            error!(target: LOG_TARGET, "Error handling connection manager event: {:?}", err);
                        }
                    }
                },

                _ = cleanup_ticker.tick() => {
                    if let Err(err) = self.cleanup() {
                        error!(target: LOG_TARGET, "Error when performing store and forward cleanup: {:?}", err);
                    }
                },

                Some(_) = self.saf_response_signal_rx.recv() => {
                    if let Some(n) = self.num_received_saf_responses {
                        self.num_received_saf_responses = Some(n + 1);
                        self.check_saf_response_threshold();
                    }
                },

                _ = self.shutdown_signal.wait() => {
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
            FetchMessages(query, reply_tx) => match self.handle_fetch_message_query(query) {
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
            InsertMessage(msg, reply_tx) => {
                let public_key = msg.destination_pubkey.clone();
                let node_id = msg.destination_node_id.clone();
                match self.database.insert_message_if_unique(msg) {
                    Ok(existed) => {
                        let pub_key = public_key
                            .map(|p| format!("public key '{}'", p))
                            .or_else(|| node_id.map(|n| format!("node id '{}'", n)))
                            .unwrap_or_else(|| "<Anonymous>".to_string());
                        if !existed {
                            info!(target: LOG_TARGET, "Stored message for {}", pub_key);
                        } else {
                            info!(target: LOG_TARGET, "SAF message for {} already stored", pub_key);
                        }
                        let _ = reply_tx.send(Ok(existed));
                    },
                    Err(err) => {
                        error!(target: LOG_TARGET, "InsertMessage failed because '{:?}'", err);
                        let _ = reply_tx.send(Err(err.into()));
                    },
                }
            },
            RemoveMessages(message_ids) => match self.database.remove_message(message_ids.clone()) {
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
            RemoveMessagesOlderThan(threshold) => {
                match self.database.delete_messages_older_than(threshold.naive_utc()) {
                    Ok(_) => trace!(target: LOG_TARGET, "Removed messages older than {}", threshold),
                    Err(err) => error!(target: LOG_TARGET, "RemoveMessage failed because '{:?}'", err),
                }
            },
            MarkSafResponseReceived(peer, reply) => {
                let _ = reply.send(self.local_state.mark_infight_response_received(peer));
            },
        }
    }

    async fn handle_connectivity_event(&mut self, event: &ConnectivityEvent) -> SafResult<()> {
        use ConnectivityEvent::*;

        #[allow(clippy::single_match)]
        match event {
            PeerConnected(conn) => {
                if !self.config.auto_request {
                    debug!(
                        target: LOG_TARGET,
                        "Auto store and forward request disabled. Ignoring PeerConnected event"
                    );
                    return Ok(());
                }

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
            ConnectivityStateOnline(n) => {
                // Capture the number of online peers when this event occurs for the first time
                if self.num_online_peers.is_none() {
                    self.num_online_peers = Some(*n);
                }
                self.check_saf_response_threshold();
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
        self.local_state.register_inflight_request(node_id.clone());
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
        let selected_peers = self
            .dht_requester
            .select_peers(BroadcastStrategy::Broadcast(vec![]))
            .await?;

        self.local_state.register_inflight_requests(&selected_peers);

        self.outbound_requester
            .send_message_no_header(
                SendMessageParams::new()
                    .selected_peers(selected_peers)
                    .with_dht_message_type(DhtMessageType::SafRequestMessages)
                    .finish(),
                request,
            )
            .await
            .map_err(StoreAndForwardError::RequestMessagesFailed)?;

        Ok(())
    }

    async fn get_saf_request(&mut self) -> SafResult<StoredMessagesRequest> {
        let request = self
            .dht_requester
            .get_metadata(DhtMetadataKey::LastSafMessageReceived)
            .await?
            .map(StoredMessagesRequest::since)
            .unwrap_or_else(StoredMessagesRequest::new);

        Ok(request)
    }

    fn check_saf_response_threshold(&mut self) {
        // This check can only be done after the `ConnectivityStateOnline` event has arrived
        if let Some(num_peers) = self.num_online_peers {
            // We only perform the check while we are still tracking responses
            if let Some(n) = self.num_received_saf_responses {
                if n >= num_peers {
                    // A send operation can only fail if there are no subscribers, so it is safe to ignore the error
                    self.publish_event(DhtEvent::StoreAndForwardMessagesReceived);
                    // Once this event is fired we stop tracking responses
                    self.num_received_saf_responses = None;
                    debug!(
                        target: LOG_TARGET,
                        "Store and Forward responses received from {} connected peers", num_peers
                    );
                } else {
                    trace!(
                        target: LOG_TARGET,
                        "Not enough Store and Forward responses received yet ({} out of a required {})",
                        n,
                        num_peers
                    );
                }
            }
        }
    }

    fn handle_fetch_message_query(&self, query: FetchStoredMessageQuery) -> SafResult<Vec<StoredMessage>> {
        use SafResponseType::*;
        let limit = i64::try_from(self.config.max_returned_messages)
            .ok()
            .or(Some(std::i64::MAX))
            .unwrap();
        let db = &self.database;
        let messages = match query.response_type {
            ForMe => db.find_messages_for_peer(&query.public_key, &query.node_id, query.since, limit)?,
            Join => db.find_join_messages(query.since, limit)?,
            Discovery => {
                db.find_messages_of_type_for_pubkey(&query.public_key, DhtMessageType::Discovery, query.since, limit)?
            },
            Anonymous => db.find_anonymous_messages(query.since, limit)?,
        };

        Ok(messages)
    }

    fn cleanup(&mut self) -> SafResult<()> {
        self.local_state
            .garbage_collect(self.config.max_inflight_request_age * 2);

        let num_removed = self.database.delete_messages_with_priority_older_than(
            StoredMessagePriority::Low,
            since(self.config.low_priority_msg_storage_ttl),
        )?;
        debug!(target: LOG_TARGET, "Cleaned {} old low priority messages", num_removed);

        let num_removed = self.database.delete_messages_with_priority_older_than(
            StoredMessagePriority::High,
            since(self.config.high_priority_msg_storage_ttl),
        )?;
        debug!(target: LOG_TARGET, "Cleaned {} old high priority messages", num_removed);

        let num_removed = self.database.truncate_messages(self.config.msg_storage_capacity)?;
        if num_removed > 0 {
            debug!(
                target: LOG_TARGET,
                "Storage limits exceeded, removing {} oldest messages", num_removed
            );
        }

        Ok(())
    }

    fn publish_event(&mut self, event: DhtEvent) {
        let _ = self.event_publisher.send(Arc::new(event)).map_err(|_| {
            trace!(
                target: LOG_TARGET,
                "Could not publish DhtEvent as there are no subscribers"
            )
        });
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
