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
    proto::store_forward::stored_messages_response::SafResponseType,
    storage::DbConnection,
    DhtConfig,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::{
    channel::{mpsc, oneshot},
    stream::Fuse,
    SinkExt,
    StreamExt,
};
use log::*;
use std::{convert::TryFrom, time::Duration};
use tari_comms::types::CommsPublicKey;
use tari_shutdown::ShutdownSignal;
use tokio::time;

const LOG_TARGET: &str = "comms::dht::store_forward::actor";
/// The interval to initiate a database cleanup.
/// This involves cleaning up messages which have been stored too long according to their priority
const CLEANUP_INTERVAL: Duration = Duration::from_secs(10 * 60); // 10 mins

#[derive(Debug, Clone)]
pub struct FetchStoredMessageQuery {
    public_key: Box<CommsPublicKey>,
    since: Option<DateTime<Utc>>,
    response_type: SafResponseType,
}

impl FetchStoredMessageQuery {
    pub fn new(public_key: Box<CommsPublicKey>) -> Self {
        Self {
            public_key,
            since: None,
            response_type: SafResponseType::General,
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
}

#[derive(Debug)]
pub enum StoreAndForwardRequest {
    FetchMessages(FetchStoredMessageQuery, oneshot::Sender<SafResult<Vec<StoredMessage>>>),
    InsertMessage(NewStoredMessage),
}

#[derive(Clone)]
pub struct StoreAndForwardRequester {
    sender: mpsc::Sender<StoreAndForwardRequest>,
}

impl StoreAndForwardRequester {
    pub fn new(sender: mpsc::Sender<StoreAndForwardRequest>) -> Self {
        Self { sender }
    }

    pub async fn fetch_messages(
        &mut self,
        request: FetchStoredMessageQuery,
    ) -> Result<Vec<StoredMessage>, StoreAndForwardError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(StoreAndForwardRequest::FetchMessages(request, reply_tx))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        reply_rx.await.map_err(|_| StoreAndForwardError::RequestCancelled)?
    }

    pub async fn insert_message(&mut self, message: NewStoredMessage) -> Result<(), StoreAndForwardError> {
        self.sender
            .send(StoreAndForwardRequest::InsertMessage(message))
            .await
            .map_err(|_| StoreAndForwardError::RequesterChannelClosed)?;
        Ok(())
    }
}

pub struct StoreAndForwardService {
    config: DhtConfig,
    request_rx: Fuse<mpsc::Receiver<StoreAndForwardRequest>>,
    shutdown_signal: Option<ShutdownSignal>,
}

impl StoreAndForwardService {
    pub fn new(
        config: DhtConfig,
        request_rx: mpsc::Receiver<StoreAndForwardRequest>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            config,
            request_rx: request_rx.fuse(),
            shutdown_signal: Some(shutdown_signal),
        }
    }

    pub(crate) async fn connect_database(&self) -> SafResult<StoreAndForwardDatabase> {
        let conn = DbConnection::connect_url(self.config.database_url.clone()).await?;
        let output = conn.migrate().await?;
        info!(target: LOG_TARGET, "Store and forward database migration:\n{}", output);
        Ok(StoreAndForwardDatabase::new(conn))
    }

    pub async fn run(mut self) -> SafResult<()> {
        let db = self.connect_database().await?;
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("StoreAndForwardActor initialized without shutdown_signal");

        let mut cleanup_ticker = time::interval(CLEANUP_INTERVAL).fuse();

        // Do initial cleanup to account for time passed since being offline
        if let Err(err) = self.cleanup(&db).await {
            error!(
                target: LOG_TARGET,
                "Error when performing store and forward cleanup: {:?}", err
            );
        }

        loop {
            futures::select! {
                request = self.request_rx.select_next_some() => {
                    self.handle_request(&db, request).await;
                },

                _ = cleanup_ticker.next() => {
                    if let Err(err) = self.cleanup(&db).await {
                        error!(target: LOG_TARGET, "Error when performing store and forward cleanup: {:?}", err);
                    }
                },

                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "StoreAndForwardActor is shutting down because the shutdown signal was triggered");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_request(&self, db: &StoreAndForwardDatabase, request: StoreAndForwardRequest) {
        use StoreAndForwardRequest::*;
        match request {
            FetchMessages(query, reply_tx) => match self.handle_fetch_message_query(db, query).await {
                Ok(messages) => {
                    let _ = reply_tx.send(Ok(messages));
                },
                Err(err) => {
                    error!(
                        target: LOG_TARGET,
                        "find_messages_by_public_key failed because '{:?}'", err
                    );
                    let _ = reply_tx.send(Err(err));
                },
            },
            InsertMessage(msg) => {
                let public_key = msg.destination_pubkey.clone();
                match db.insert_message(msg).await {
                    Ok(_) => info!(
                        target: LOG_TARGET,
                        "Store and forward message stored for public key '{}'",
                        public_key.unwrap_or_else(|| "<None>".to_string())
                    ),
                    Err(err) => {
                        error!(target: LOG_TARGET, "insert_message failed because '{:?}'", err);
                    },
                }
            },
        }
    }

    async fn handle_fetch_message_query(
        &self,
        db: &StoreAndForwardDatabase,
        query: FetchStoredMessageQuery,
    ) -> SafResult<Vec<StoredMessage>>
    {
        let limit = i64::try_from(self.config.saf_max_returned_messages)
            .ok()
            .or(Some(std::i64::MAX))
            .unwrap();
        let messages = match query.response_type {
            SafResponseType::General => {
                db.find_messages_for_public_key(&query.public_key, query.since, limit)
                    .await?
            },
            SafResponseType::Join => {
                db.find_messages_of_type_for_pubkey(&query.public_key, DhtMessageType::Join, query.since, limit)
                    .await?
            },
            SafResponseType::Discovery => {
                db.find_messages_of_type_for_pubkey(&query.public_key, DhtMessageType::Discovery, query.since, limit)
                    .await?
            },
            SafResponseType::ExplicitlyAddressed => {
                db.find_messages_for_public_key(&query.public_key, query.since, limit)
                    .await?
            },
        };

        Ok(messages)
    }

    async fn cleanup(&self, db: &StoreAndForwardDatabase) -> SafResult<()> {
        let num_removed = db
            .delete_messages_with_priority_older_than(
                StoredMessagePriority::Low,
                since(self.config.saf_low_priority_msg_storage_ttl),
            )
            .await?;
        info!(target: LOG_TARGET, "Cleaned {} old low priority messages", num_removed);

        let num_removed = db
            .delete_messages_with_priority_older_than(
                StoredMessagePriority::High,
                since(self.config.saf_high_priority_msg_storage_ttl),
            )
            .await?;
        info!(target: LOG_TARGET, "Cleaned {} old high priority messages", num_removed);
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
