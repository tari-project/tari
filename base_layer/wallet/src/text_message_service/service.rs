// Copyright 2019. The Tari Project
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
    error::TextMessageError,
    handle::{TextMessageRequest, TextMessageResponse},
    model::{Contact, ReceivedTextMessage, SentTextMessage, UpdateContact},
};

use crate::text_message_service::handle::TextMessageEvent;
use diesel::{
    r2d2::{ConnectionManager, Pool},
    Connection,
    SqliteConnection,
};
use futures::{future::poll_fn, pin_mut, SinkExt, Stream, StreamExt};
use log::*;
use serde::{Deserialize, Serialize};
use std::{io, path::Path, time::Duration};
use tari_broadcast_channel::Publisher;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{
    envelope::NodeDestination,
    outbound::{BroadcastStrategy, OutboundEncryption, OutboundMessageRequester},
};
use tari_p2p::{
    domain_message::DomainMessage,
    services::liveness::LivenessHandle,
    tari_message::{ExtendedMessage, TariMessageType},
};
use tari_service_framework::reply_channel;
use tokio_executor::threadpool::blocking;

const LOG_TARGET: &'static str = "base_layer::wallet::text_messsage_service";

/// Represents an Acknowledgement of receiving a Text Message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextMessageAck {
    id: Vec<u8>,
}

/// A collection to hold a text message state
#[derive(Debug)]
pub struct TextMessages {
    pub received_messages: Vec<ReceivedTextMessage>,
    pub sent_messages: Vec<SentTextMessage>,
}

/// The TextMessageService manages the local node's text messages. It keeps track of sent messages that require an Ack
/// (pending messages), Ack'ed sent messages and received messages.
pub struct TextMessageService<TTextStream, TAckStream> {
    pub_key: CommsPublicKey,
    screen_name: Option<String>,
    oms: OutboundMessageRequester,
    database_connection_pool: Pool<ConnectionManager<SqliteConnection>>,
    request_stream: Option<reply_channel::Receiver<TextMessageRequest, Result<TextMessageResponse, TextMessageError>>>,
    text_message_stream: Option<TTextStream>,
    text_message_ack_stream: Option<TAckStream>,
    liveness: LivenessHandle,
    event_publisher: Publisher<TextMessageEvent>,
}

impl<TTextStream, TAckStream> TextMessageService<TTextStream, TAckStream>
where
    TTextStream: Stream<Item = DomainMessage<ReceivedTextMessage>>,
    TAckStream: Stream<Item = DomainMessage<TextMessageAck>>,
{
    pub fn new(
        request_stream: reply_channel::Receiver<TextMessageRequest, Result<TextMessageResponse, TextMessageError>>,
        text_message_stream: TTextStream,
        text_message_ack_stream: TAckStream,
        pub_key: CommsPublicKey,
        database_path: String,
        oms: OutboundMessageRequester,
        liveness: LivenessHandle,
        event_publisher: Publisher<TextMessageEvent>,
    ) -> Self
    {
        let pool = Self::establish_db_connection_pool(database_path).expect("Could not establish database connection");

        Self {
            request_stream: Some(request_stream),
            text_message_stream: Some(text_message_stream),
            text_message_ack_stream: Some(text_message_ack_stream),
            pub_key: pub_key.clone(),
            screen_name: None,
            oms,
            database_connection_pool: pool,
            liveness,
            event_publisher,
        }
    }

    pub async fn start(mut self) -> Result<(), TextMessageError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("TextMessageService initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);
        let text_message_stream = self
            .text_message_stream
            .take()
            .expect("TextMessageService initialized without text_message_stream")
            .fuse();
        pin_mut!(text_message_stream);
        let text_message_ack_stream = self
            .text_message_ack_stream
            .take()
            .expect("TextMessageService initialized without text_message_ack_stream")
            .fuse();
        pin_mut!(text_message_ack_stream);
        loop {
            futures::select! {
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                // Incoming messages from the Comms layer
                msg = text_message_stream.select_next_some() => {
                    let _ = self.handle_incoming_text_message(msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming message: {:?}", err);
                        Err(err)
                    });
                },
                 // Incoming messages from the Comms layer
                msg = text_message_ack_stream.select_next_some() => {
                    let _ = self.handle_incoming_ack_message(msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming message: {:?}", err);
                        Err(err)
                    });
                },
                complete => {
                    info!(target: LOG_TARGET, "Text message service shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn establish_db_connection_pool(
        database_path: String,
    ) -> Result<Pool<ConnectionManager<SqliteConnection>>, TextMessageError> {
        let db_exists = Path::new(&database_path).exists();

        let connection = SqliteConnection::establish(&database_path)?;

        connection.execute("PRAGMA foreign_keys = ON")?;

        if !db_exists {
            embed_migrations!("./migrations");
            embedded_migrations::run_with_output(&connection, &mut io::stdout()).map_err(|err| {
                TextMessageError::DatabaseMigrationError(format!("Database migration failed {}", err))
            })?;
        }
        drop(connection);

        let manager = ConnectionManager::<SqliteConnection>::new(database_path);
        let pool = diesel::r2d2::Pool::builder()
            .connection_timeout(Duration::from_millis(2000))
            .idle_timeout(Some(Duration::from_millis(2000)))
            .build(manager)
            .map_err(|_| TextMessageError::R2d2Error)?;
        Ok(pool)
    }

    async fn handle_request(&mut self, request: TextMessageRequest) -> Result<TextMessageResponse, TextMessageError> {
        match request {
            TextMessageRequest::SendTextMessage((destination, message)) => self
                .send_text_message(destination, message)
                .await
                .map(|_| TextMessageResponse::MessageSent),
            TextMessageRequest::GetTextMessages => self
                .get_current_messages()
                .await
                .map(|tm| TextMessageResponse::TextMessages(tm)),
            TextMessageRequest::GetTextMessagesByPubKey(pk) => self
                .get_current_messages_by_pub_key(pk)
                .await
                .map(|tm| TextMessageResponse::TextMessages(tm)),
            TextMessageRequest::GetScreenName => Ok(TextMessageResponse::ScreenName(self.get_screen_name())),
            TextMessageRequest::SetScreenName(s) => {
                self.set_screen_name(s);
                Ok(TextMessageResponse::ScreenNameSet)
            },
            TextMessageRequest::AddContact(c) => self.add_contact(c).await.map(|_| TextMessageResponse::ContactAdded),
            TextMessageRequest::RemoveContact(c) => self
                .remove_contact(c)
                .await
                .map(|_| TextMessageResponse::ContactRemoved),
            TextMessageRequest::GetContacts => self.get_contacts().await.map(|c| TextMessageResponse::Contacts(c)),
            TextMessageRequest::UpdateContact((pk, c)) => self
                .update_contact(pk, c)
                .await
                .map(|_| TextMessageResponse::ContactUpdated),
        }
    }

    /// Send a text message to the specified node using the provided OMS
    async fn send_text_message(
        &mut self,
        dest_pub_key: CommsPublicKey,
        message: String,
    ) -> Result<(), TextMessageError>
    {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;
        let count = SentTextMessage::count_by_dest_pub_key(&dest_pub_key.clone(), &conn)?;

        let text_message = SentTextMessage::new(self.pub_key.clone(), dest_pub_key, message, Some(count as usize));

        self.oms
            .send_message(
                BroadcastStrategy::DirectPublicKey(text_message.dest_pub_key.clone()),
                NodeDestination::Undisclosed,
                OutboundEncryption::EncryptForDestination,
                TariMessageType::new(ExtendedMessage::Text),
                text_message.clone(),
            )
            .await?;

        text_message.commit(&conn)?;

        trace!(target: LOG_TARGET, "Text Message Sent to {}", text_message.dest_pub_key);

        Ok(())
    }

    /// Process an incoming text message
    async fn handle_incoming_text_message(
        &mut self,
        message: DomainMessage<ReceivedTextMessage>,
    ) -> Result<(), TextMessageError>
    {
        trace!(
            target: LOG_TARGET,
            "Text Message received with ID: {:?} from {} with message: {:?}",
            message.inner.id,
            message.inner.source_pub_key,
            message.inner.message,
        );

        let text_message_ack = TextMessageAck {
            id: message.inner.id.clone(),
        };
        self.oms
            .send_message(
                BroadcastStrategy::DirectPublicKey(message.clone().origin_pubkey),
                NodeDestination::Undisclosed,
                OutboundEncryption::EncryptForDestination,
                TariMessageType::new(ExtendedMessage::TextAck),
                text_message_ack,
            )
            .await?;
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;

        let message_inner = message.clone().into_inner();
        poll_fn(move |_| blocking(|| message_inner.commit(&conn))).await??;

        self.event_publisher
            .send(TextMessageEvent::ReceivedTextMessage)
            .await
            .map_err(|_| TextMessageError::EventStreamError)?;

        Ok(())
    }

    /// Process an incoming text message Ack
    async fn handle_incoming_ack_message(
        &mut self,
        message_ack: DomainMessage<TextMessageAck>,
    ) -> Result<(), TextMessageError>
    {
        let message_ack_inner = message_ack.clone().into_inner();

        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;

        debug!(
            target: LOG_TARGET,
            "Text Message Ack received with ID: {:?}", message_ack_inner.id,
        );

        poll_fn(move |_| blocking(|| SentTextMessage::mark_sent_message_ack(&message_ack_inner.id, &conn))).await??;
        self.event_publisher
            .send(TextMessageEvent::ReceivedTextMessageAck)
            .await
            .map_err(|_| TextMessageError::EventStreamError)?;
        Ok(())
    }

    /// Return a copy of the current lists of messages
    async fn get_current_messages(&mut self) -> Result<TextMessages, TextMessageError> {
        let sent_messages;
        let received_messages;

        let conn1 = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;

        sent_messages = poll_fn(move |_| blocking(|| SentTextMessage::index(&conn1))).await??;

        let conn2 = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;

        received_messages = poll_fn(move |_| blocking(|| ReceivedTextMessage::index(&conn2))).await??;
        Ok(TextMessages {
            sent_messages,
            received_messages,
        })
    }

    async fn get_current_messages_by_pub_key(&self, pub_key: CommsPublicKey) -> Result<TextMessages, TextMessageError> {
        let sent_messages;
        let received_messages;

        let conn1 = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;
        let pub_key1 = pub_key.clone();
        sent_messages =
            poll_fn(move |_| blocking(|| SentTextMessage::find_by_dest_pub_key(&pub_key1, &conn1))).await??;

        let conn2 = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;
        let pub_key2 = pub_key.clone();
        received_messages =
            poll_fn(move |_| blocking(|| ReceivedTextMessage::find_by_source_pub_key(&pub_key2, &conn2))).await??;

        Ok(TextMessages {
            sent_messages,
            received_messages,
        })
    }

    pub fn get_screen_name(&self) -> Option<String> {
        self.screen_name.clone()
    }

    pub fn set_screen_name(&mut self, screen_name: String) {
        self.screen_name = Some(screen_name);
    }

    pub async fn add_contact(&mut self, contact: Contact) -> Result<(), TextMessageError> {
        let conn1 = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;
        let contact_clone = contact.clone();
        let found_contact = poll_fn(move |_| blocking(|| Contact::find(&contact_clone.pub_key, &conn1))).await?;
        if let Ok(c) = found_contact {
            if c.pub_key == contact.pub_key {
                return Err(TextMessageError::ContactAlreadyExists);
            }
        }

        let conn2 = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;
        let contact_clone2 = contact.clone();
        poll_fn(move |_| blocking(|| contact_clone2.commit(&conn2))).await??;

        trace!(
            target: LOG_TARGET,
            "Contact Added: Screen name: {:?} - Pub-key: {} - Address: {:?}",
            contact.screen_name,
            contact.pub_key,
            contact.address,
        );

        // Send ping to the contact so that if they are online they will flush all outstanding messages for this node
        // TODO This was removed as it created random lock ups in the tests, Once the new Future based comms Middleware
        // is in put this back
        //        let _ = self
        //            .liveness
        //            .call(LivenessRequest::SendPing(contact.pub_key))
        //            .await
        //            .or_else(|err| {
        //                error!(target: LOG_TARGET, "{:?}", err);
        //                Err(err)
        //            });

        Ok(())
    }

    pub async fn remove_contact(&mut self, contact: Contact) -> Result<(), TextMessageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;
        let contact_clone = contact.clone();
        poll_fn(move |_| blocking(|| contact_clone.delete(&conn))).await??;

        trace!(
            target: LOG_TARGET,
            "Contact Deleted: Screen name: {:?} - Pub-key: {} - Address: {:?}",
            contact.screen_name,
            contact.pub_key,
            contact.address,
        );

        Ok(())
    }

    pub async fn get_contacts(&self) -> Result<Vec<Contact>, TextMessageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;
        poll_fn(move |_| blocking(|| Contact::index(&conn))).await?
    }

    /// Updates the screen_name of a contact if an existing contact with the same pub_key is found
    pub async fn update_contact(
        &mut self,
        pub_key: CommsPublicKey,
        contact_update: UpdateContact,
    ) -> Result<(), TextMessageError>
    {
        let conn1 = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;
        let pub_key1 = pub_key.clone();
        let contact = poll_fn(move |_| blocking(|| Contact::find(&pub_key1, &conn1))).await??;

        let conn2 = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TextMessageError::R2d2Error)?;
        let contact_update = contact_update.clone();
        let contact_clone = contact.clone();
        poll_fn(move |_| blocking(|| contact_clone.update(contact_update.clone(), &conn2))).await??;

        trace!(
            target: LOG_TARGET,
            "Contact Updated: Screen name: {:?} - Pub-key: {} - Address: {:?}",
            contact.screen_name,
            contact.pub_key,
            contact.address,
        );

        Ok(())
    }

    // TODO Some sort of accessor that allows for pagination of messages
}
