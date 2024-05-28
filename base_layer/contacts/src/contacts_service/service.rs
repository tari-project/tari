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

use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
    ops::Sub,
    sync::Arc,
    time::Duration,
};

use chrono::{NaiveDateTime, Utc};
use futures::{pin_mut, StreamExt};
use log::*;
use tari_common_types::tari_address::TariAddress;
use tari_comms::{
    connectivity::{ConnectivityEvent, ConnectivityRequester},
    types::CommsPublicKey,
};
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundEncryption, Dht};
use tari_p2p::{
    comms_connector::SubscriptionFactory,
    domain_message::DomainMessage,
    services::{
        liveness::{LivenessEvent, LivenessHandle, MetadataKey, PingPongEvent},
        utils::map_decode,
    },
    tari_message::TariMessageType,
};
use tari_service_framework::reply_channel;
use tari_shutdown::ShutdownSignal;
use tari_utilities::{epoch_time::EpochTime, ByteArray};
use tokio::sync::broadcast;

use crate::contacts_service::{
    error::ContactsServiceError,
    handle::{ContactsLivenessData, ContactsLivenessEvent, ContactsServiceRequest, ContactsServiceResponse},
    proto,
    storage::database::{ContactsBackend, ContactsDatabase},
    types::{Confirmation, Contact, Message, MessageDispatch},
};

const LOG_TARGET: &str = "contacts::contacts_service";
const NUM_ROUNDS_NETWORK_SILENCE: u16 = 3;
pub const SUBSCRIPTION_LABEL: &str = "Chat";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactMessageType {
    Ping,
    Pong,
    NoMessage,
}

impl Display for ContactMessageType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            ContactMessageType::Ping => write!(f, "Ping"),
            ContactMessageType::Pong => write!(f, "Pong"),
            ContactMessageType::NoMessage => write!(f, "NoMessage"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactOnlineStatus {
    Online,
    Offline,
    NeverSeen,
    Banned(String),
}

impl ContactOnlineStatus {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Online => 1,
            Self::Offline => 2,
            Self::NeverSeen => 3,
            Self::Banned(_) => 4,
        }
    }

    pub fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Online),
            2 => Some(Self::Offline),
            3 => Some(Self::NeverSeen),
            4 => Some(Self::Banned("No reason listed".to_string())),
            _ => None,
        }
    }
}

impl Display for ContactOnlineStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            ContactOnlineStatus::Online => write!(f, "Online"),
            ContactOnlineStatus::Offline => write!(f, "Offline"),
            ContactOnlineStatus::NeverSeen => write!(f, "NeverSeen"),
            ContactOnlineStatus::Banned(reason) => write!(f, "Banned: {}", reason),
        }
    }
}

pub struct ContactsService<T>
where T: ContactsBackend + 'static
{
    db: ContactsDatabase<T>,
    request_stream:
        Option<reply_channel::Receiver<ContactsServiceRequest, Result<ContactsServiceResponse, ContactsServiceError>>>,
    shutdown_signal: Option<ShutdownSignal>,
    liveness: LivenessHandle,
    liveness_data: Vec<ContactsLivenessData>,
    connectivity: ConnectivityRequester,
    dht: Dht,
    subscription_factory: Arc<SubscriptionFactory>,
    event_publisher: broadcast::Sender<Arc<ContactsLivenessEvent>>,
    message_publisher: broadcast::Sender<Arc<MessageDispatch>>,
    number_of_rounds_no_pings: u16,
    contacts_auto_ping_interval: Duration,
    contacts_online_ping_window: usize,
}

impl<T> ContactsService<T>
where T: ContactsBackend + 'static
{
    pub fn new(
        db: ContactsDatabase<T>,
        request_stream: reply_channel::Receiver<
            ContactsServiceRequest,
            Result<ContactsServiceResponse, ContactsServiceError>,
        >,
        shutdown_signal: ShutdownSignal,
        liveness: LivenessHandle,
        connectivity: ConnectivityRequester,
        dht: Dht,
        subscription_factory: Arc<SubscriptionFactory>,
        event_publisher: broadcast::Sender<Arc<ContactsLivenessEvent>>,
        message_publisher: broadcast::Sender<Arc<MessageDispatch>>,
        contacts_auto_ping_interval: Duration,
        contacts_online_ping_window: usize,
    ) -> Self {
        Self {
            db,
            request_stream: Some(request_stream),
            shutdown_signal: Some(shutdown_signal),
            liveness,
            liveness_data: Vec::new(),
            connectivity,
            dht,
            subscription_factory,
            event_publisher,
            message_publisher,
            number_of_rounds_no_pings: 0,
            contacts_auto_ping_interval,
            contacts_online_ping_window,
        }
    }

    pub async fn start(mut self) -> Result<(), ContactsServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Contacts Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let liveness_event_stream = self.liveness.get_event_stream();
        pin_mut!(liveness_event_stream);

        let connectivity_events = self.connectivity.get_event_subscription();
        pin_mut!(connectivity_events);

        let chat_messages = self
            .subscription_factory
            .get_subscription(TariMessageType::Chat, SUBSCRIPTION_LABEL)
            .map(map_decode::<proto::MessageDispatch>);

        pin_mut!(chat_messages);

        let shutdown = self
            .shutdown_signal
            .take()
            .expect("Contacts Service initialized without shutdown signal");
        pin_mut!(shutdown);

        // Add all contacts as monitored peers to the liveness service
        let result = self.db.get_contacts();
        if let Ok(ref contacts) = result {
            self.add_contacts_to_liveness_service(contacts).await?;
        }
        self.set_liveness_metadata(b"Watching you!".to_vec()).await?;
        debug!(target: LOG_TARGET, "Contacts Service started");
        loop {
            tokio::select! {
                // Incoming chat messages
                Some(msg) = chat_messages.next() => {
                    if let Err(err) = self.handle_incoming_message(msg).await {
                        warn!(target: LOG_TARGET, "Failed to handle incoming chat message: {}", err);
                    }
                },

                Some(request_context) = request_stream.next() => {
                    let (request, reply_tx) = request_context.split();
                    let response = self.handle_request(request).await.map_err(|e| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", e);
                        e
                    });
                    let _result = reply_tx.send(response).map_err(|e| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        e
                    });
                },

                Ok(event) = liveness_event_stream.recv() => {
                    let _result = self.handle_liveness_event(&event).await.map_err(|e| {
                        error!(target: LOG_TARGET, "Failed to handle contact status liveness event: {:?}", e);
                        e
                    });
                },

                Ok(event) = connectivity_events.recv() => {
                    self.handle_connectivity_event(event);
                },

                _ = shutdown.wait() => {
                    info!(target: LOG_TARGET, "Contacts service shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
        info!(target: LOG_TARGET, "Contacts Service ended");
        Ok(())
    }

    async fn handle_request(
        &mut self,
        request: ContactsServiceRequest,
    ) -> Result<ContactsServiceResponse, ContactsServiceError> {
        match request {
            ContactsServiceRequest::GetContact(address) => {
                let result = self.db.get_contact(address.clone());
                if let Ok(ref contact) = result {
                    self.liveness.check_add_monitored_peer(contact.node_id.clone()).await?;
                };
                Ok(result.map(ContactsServiceResponse::Contact)?)
            },
            ContactsServiceRequest::UpsertContact(c) => {
                self.db.upsert_contact(c.clone())?;
                self.liveness.check_add_monitored_peer(c.node_id.clone()).await?;
                info!(
                    target: LOG_TARGET,
                    "Contact Saved: \nAlias: {}\nAddress: {}\nNodeId: {}", c.alias, c.address, c.node_id
                );
                Ok(ContactsServiceResponse::ContactSaved)
            },
            ContactsServiceRequest::RemoveContact(pk) => {
                let result = self.db.remove_contact(pk.clone())?;
                self.liveness
                    .check_remove_monitored_peer(result.node_id.clone())
                    .await?;
                info!(
                    target: LOG_TARGET,
                    "Contact Removed: \nAlias: {}\nAddress: {} ", result.alias, result.address
                );
                Ok(ContactsServiceResponse::ContactRemoved(result))
            },
            ContactsServiceRequest::GetContacts => {
                let result = self.db.get_contacts();
                if let Ok(ref contacts) = result {
                    self.add_contacts_to_liveness_service(contacts).await?;
                }
                Ok(result.map(ContactsServiceResponse::Contacts)?)
            },
            ContactsServiceRequest::GetContactOnlineStatus(contact) => {
                let result = self.get_online_status(&contact).await;
                Ok(result.map(ContactsServiceResponse::OnlineStatus)?)
            },
            ContactsServiceRequest::GetMessages(pk, limit, page) => {
                let result = self.db.get_messages(pk, limit, page);
                Ok(result.map(ContactsServiceResponse::Messages)?)
            },
            ContactsServiceRequest::SendMessage(address, mut message) => {
                message.sent_at = Utc::now().naive_utc().timestamp() as u64;
                let ob_message = OutboundDomainMessage::from(MessageDispatch::Message(message.clone()));

                message.stored_at = Utc::now().naive_utc().timestamp() as u64;
                match self.db.save_message(message) {
                    Ok(_) => {
                        if let Err(e) = self.deliver_message(address.clone(), ob_message).await {
                            trace!(target: LOG_TARGET, "Failed to broadcast a message {} over the network: {}", address, e);
                        }
                    },
                    Err(e) => {
                        trace!(target: LOG_TARGET, "Failed to save the message locally, did not broadcast the message to the network");
                        return Err(e.into());
                    },
                }

                trace!(target: LOG_TARGET, "Sent message to {} successfully", address);
                Ok(ContactsServiceResponse::MessageSent)
            },
            ContactsServiceRequest::SendReadConfirmation(address, confirmation) => {
                let msg = OutboundDomainMessage::from(MessageDispatch::ReadConfirmation(confirmation.clone()));
                trace!(target: LOG_TARGET, "Sending read confirmation with details: message_id: {:?}, timestamp: {:?}", confirmation.message_id, confirmation.timestamp);

                match self.deliver_message(address.clone(), msg).await {
                    Ok(_) => {
                        trace!(target: LOG_TARGET, "Read confirmation broadcast for message_id: {:?} to {}", confirmation.message_id, address);
                        match self.db.confirm_message(
                            confirmation.message_id.clone(),
                            None,
                            Some(confirmation.timestamp),
                        ) {
                            Ok(_) => {
                                trace!(target: LOG_TARGET, "Read confirmation locally saved for message_id: {:?} to {}", confirmation.message_id, address);
                            },
                            Err(e) => {
                                trace!(target: LOG_TARGET, "Failed to save the read confirmation locally for message_id: {:?} with error {}", confirmation.message_id, e);
                            },
                        }
                    },
                    Err(e) => {
                        trace!(target: LOG_TARGET, "Failed to broadcast the read confirmation of message_id: {:?} to {} with error {}", confirmation.message_id, address, e);
                        return Err(e);
                    },
                }

                Ok(ContactsServiceResponse::ReadConfirmationSent)
            },
            ContactsServiceRequest::GetConversationalists => {
                let result = self.db.get_conversationlists();
                Ok(result.map(ContactsServiceResponse::Conversationalists)?)
            },
            ContactsServiceRequest::GetMessage(message_id) => {
                let result = self.db.get_message(message_id);
                Ok(result.map(ContactsServiceResponse::Message)?)
            },
        }
    }

    async fn add_contacts_to_liveness_service(&mut self, contacts: &[Contact]) -> Result<(), ContactsServiceError> {
        for contact in contacts {
            self.liveness.check_add_monitored_peer(contact.node_id.clone()).await?;
        }
        Ok(())
    }

    /// Tack this node's metadata on to ping/pongs sent by the liveness service
    async fn set_liveness_metadata(&mut self, message: Vec<u8>) -> Result<(), ContactsServiceError> {
        self.liveness
            .set_metadata_entry(MetadataKey::ContactsLiveness, message)
            .await?;
        Ok(())
    }

    async fn handle_liveness_event(&mut self, event: &LivenessEvent) -> Result<(), ContactsServiceError> {
        match event {
            // Received a ping, check if it contains ContactsLiveness
            LivenessEvent::ReceivedPing(event) => {
                self.update_with_ping_pong(event, ContactMessageType::Ping)?;
            },
            // Received a pong, check if our neighbour sent it and it contains ContactsLiveness
            LivenessEvent::ReceivedPong(event) => {
                self.update_with_ping_pong(event, ContactMessageType::Pong)?;
            },
            // New ping round has begun
            LivenessEvent::PingRoundBroadcast(num_peers) => {
                debug!(
                    target: LOG_TARGET,
                    "New contact liveness round sent to {} peer(s)", num_peers
                );
                // If there were no pings for a while, we are probably alone.
                if *num_peers == 0 {
                    self.number_of_rounds_no_pings += 1;
                    if self.number_of_rounds_no_pings >= NUM_ROUNDS_NETWORK_SILENCE {
                        self.send_network_silence().await?;
                        self.number_of_rounds_no_pings = 0;
                    }
                }
                self.resize_contacts_liveness_data_buffer(*num_peers);

                // Update offline status
                if let Ok(contacts) = self.db.get_contacts() {
                    for contact in contacts {
                        let online_status = self.get_online_status(&contact).await?;
                        if online_status == ContactOnlineStatus::Online {
                            continue;
                        }
                        let data = ContactsLivenessData::new(
                            contact.address.clone(),
                            contact.node_id.clone(),
                            contact.latency,
                            contact.last_seen,
                            ContactMessageType::NoMessage,
                            online_status,
                        );
                        // Send only fails if there are no subscribers.
                        let _size = self
                            .event_publisher
                            .send(Arc::new(ContactsLivenessEvent::StatusUpdated(Box::new(data.clone()))));
                        trace!(target: LOG_TARGET, "{}", data);
                    }
                };
            },
        }

        Ok(())
    }

    async fn handle_incoming_message(
        &mut self,
        msg: DomainMessage<Result<proto::MessageDispatch, prost::DecodeError>>,
    ) -> Result<(), ContactsServiceError> {
        trace!(target: LOG_TARGET, "Handling incoming chat message dispatch {:?} from peer {}", msg, msg.source_peer.public_key);

        let msg_inner = match &msg.inner {
            Ok(msg) => msg.clone(),
            Err(e) => {
                debug!(target: LOG_TARGET, "Banning peer {} for illformed message", msg.source_peer.public_key);

                self.connectivity
                    .ban_peer(
                        msg.source_peer.node_id.clone(),
                        "Peer sent illformed message".to_string(),
                    )
                    .await?;
                return Err(ContactsServiceError::MalformedMessageError(e.clone()));
            },
        };
        if let Some(source_public_key) = msg.authenticated_origin {
            let dispatch = MessageDispatch::try_from(msg_inner).map_err(ContactsServiceError::MessageParsingError)?;

            match dispatch {
                MessageDispatch::Message(m) => self.handle_chat_message(m, source_public_key).await,
                MessageDispatch::DeliveryConfirmation(_) | MessageDispatch::ReadConfirmation(_) => {
                    self.handle_confirmation(dispatch.clone()).await
                },
            }
        } else {
            Err(ContactsServiceError::MessageSourceDoesNotMatchOrigin)
        }
    }

    async fn get_online_status(&self, contact: &Contact) -> Result<ContactOnlineStatus, ContactsServiceError> {
        let mut online_status = ContactOnlineStatus::NeverSeen;
        if let Some(peer_data) = self.connectivity.get_peer_info(contact.node_id.clone()).await? {
            if let Some(banned_until) = peer_data.banned_until() {
                let msg = format!(
                    "Until {} ({})",
                    banned_until.format("%m-%d %H:%M"),
                    peer_data.banned_reason
                );
                return Ok(ContactOnlineStatus::Banned(msg));
            }
        };
        if let Some(time) = contact.last_seen {
            if self.is_online(time) {
                online_status = ContactOnlineStatus::Online;
            } else {
                online_status = ContactOnlineStatus::Offline;
            }
        }
        Ok(online_status)
    }

    fn is_online(&self, last_seen: NaiveDateTime) -> bool {
        #[allow(clippy::cast_possible_wrap)]
        let ping_window = chrono::Duration::seconds(
            (self.contacts_online_ping_window as u64 * self.contacts_auto_ping_interval.as_secs()) as i64,
        );
        Utc::now().naive_utc().sub(last_seen) <= ping_window
    }

    fn update_with_ping_pong(
        &mut self,
        event: &PingPongEvent,
        message_type: ContactMessageType,
    ) -> Result<(), ContactsServiceError> {
        self.number_of_rounds_no_pings = 0;
        if event.metadata.has(MetadataKey::ContactsLiveness) {
            let mut latency: Option<u32> = None;
            if let Some(pos) = self
                .liveness_data
                .iter()
                .position(|peer_status| *peer_status.node_id() == event.node_id)
            {
                latency = self.liveness_data[pos].latency();
                self.liveness_data.remove(pos);
            }

            let last_seen = Utc::now();
            // Do not overwrite measured latency with value 'None' if this is a ping from a neighbouring node
            if event.latency.is_some() {
                latency = event
                    .latency
                    .map(|val| u32::try_from(val.as_millis()).unwrap_or(u32::MAX));
            }
            let this_public_key = self
                .db
                .update_contact_last_seen(&event.node_id, last_seen.naive_utc(), latency)?;

            let data = ContactsLivenessData::new(
                this_public_key,
                event.node_id.clone(),
                latency,
                Some(last_seen.naive_utc()),
                message_type,
                ContactOnlineStatus::Online,
            );
            self.liveness_data.push(data.clone());

            trace!(target: LOG_TARGET, "{}", data);
            // Send only fails if there are no subscribers.
            let _size = self
                .event_publisher
                .send(Arc::new(ContactsLivenessEvent::StatusUpdated(Box::new(data))));
        } else {
            trace!(
                target: LOG_TARGET,
                "Ping-pong metadata key from {} not recognized",
                event.node_id
            );
        }
        Ok(())
    }

    async fn send_network_silence(&mut self) -> Result<(), ContactsServiceError> {
        let _size = self
            .event_publisher
            .send(Arc::new(ContactsLivenessEvent::NetworkSilence));
        Ok(())
    }

    // Ensure that we're waiting for the correct amount of peers to respond to
    // and have allocated space for their replies
    fn resize_contacts_liveness_data_buffer(&mut self, n: usize) {
        match self.liveness_data.capacity() {
            cap if n > cap => {
                let additional = n - self.liveness_data.len();
                self.liveness_data.reserve_exact(additional);
            },
            cap if n < cap => {
                self.liveness_data.shrink_to(cap);
            },
            _ => {},
        }
    }

    fn handle_connectivity_event(&mut self, event: ConnectivityEvent) {
        use ConnectivityEvent::{PeerBanned, PeerDisconnected};
        match event {
            PeerDisconnected(node_id, _) | PeerBanned(node_id) => {
                if let Some(pos) = self.liveness_data.iter().position(|p| *p.node_id() == node_id) {
                    debug!(
                        target: LOG_TARGET,
                        "Removing disconnected/banned peer `{}` from contacts status list ", node_id
                    );
                    self.liveness_data.remove(pos);
                }
            },
            _ => {},
        }
    }

    async fn handle_chat_message(
        &mut self,
        message: Message,
        source_public_key: CommsPublicKey,
    ) -> Result<(), ContactsServiceError> {
        if source_public_key != *message.from_address.public_key() {
            return Err(ContactsServiceError::MessageSourceDoesNotMatchOrigin);
        }
        let our_message = Message {
            stored_at: EpochTime::now().as_u64(),
            ..message
        };
        trace!(target: LOG_TARGET, "Handling chat message {:?}", our_message);

        match self.db.save_message(our_message.clone()) {
            Ok(..) => {
                if let Err(e) = self
                    .message_publisher
                    .send(Arc::new(MessageDispatch::Message(our_message.clone())))
                {
                    debug!(target: LOG_TARGET, "Failed to re-broadcast chat message internally: {}", e);
                }

                // Send a delivery notification
                self.create_and_send_delivery_confirmation_for_msg(&our_message).await?;

                Ok(())
            },
            Err(e) => {
                trace!(target: LOG_TARGET, "Failed to save incoming message to the db {}", e);
                Err(e.into())
            },
        }
    }

    async fn create_and_send_delivery_confirmation_for_msg(
        &mut self,
        message: &Message,
    ) -> Result<(), ContactsServiceError> {
        let address = &message.from_address;
        let confirmation = MessageDispatch::DeliveryConfirmation(Confirmation {
            message_id: message.message_id.clone(),
            timestamp: message.stored_at,
        });
        let msg = OutboundDomainMessage::from(confirmation);
        trace!(target: LOG_TARGET, "Sending a delivery notification {:?}", msg);

        self.deliver_message(address.clone(), msg).await?;

        if let Err(e) = self
            .db
            .confirm_message(message.message_id.clone(), Some(message.stored_at), None)
        {
            trace!(target: LOG_TARGET, "Failed to store the delivery confirmation in the db: {}", e);
        }

        Ok(())
    }

    async fn handle_confirmation(&mut self, dispatch: MessageDispatch) -> Result<(), ContactsServiceError> {
        let (message_id, delivery, read) = match dispatch.clone() {
            MessageDispatch::DeliveryConfirmation(c) => (c.message_id, Some(c.timestamp), None),
            MessageDispatch::ReadConfirmation(c) => (c.message_id, None, Some(c.timestamp)),
            _ => {
                return Err(ContactsServiceError::MessageParsingError(
                    "Incorrect confirmation type".to_string(),
                ))
            },
        };

        trace!(target: LOG_TARGET, "Handling confirmation with details: message_id: {:?}, delivery: {:?}, read: {:?}", message_id, delivery, read);
        self.db.confirm_message(message_id, delivery, read)?;
        let _msg = self.message_publisher.send(Arc::new(dispatch));

        Ok(())
    }

    async fn deliver_message(
        &mut self,
        address: TariAddress,
        message: OutboundDomainMessage<proto::MessageDispatch>,
    ) -> Result<(), ContactsServiceError> {
        let contact = match self.db.get_contact(address.clone()) {
            Ok(contact) => contact,
            Err(_) => Contact::from(&address),
        };
        let encryption = OutboundEncryption::EncryptFor(Box::new(address.public_key().clone()));

        match self.get_online_status(&contact).await {
            Ok(ContactOnlineStatus::Online) => {
                info!(target: LOG_TARGET, "Chat message being sent direct");
                let mut comms_outbound = self.dht.outbound_requester();

                comms_outbound
                    .send_direct_encrypted(
                        address.public_key().clone(),
                        message,
                        encryption,
                        "contact service messaging".to_string(),
                    )
                    .await?;
            },
            Err(e) => return Err(e),
            _ => {
                info!(target: LOG_TARGET, "Chat message being sent via closest broadcast");
                let mut comms_outbound = self.dht.outbound_requester();
                comms_outbound
                    .closest_broadcast(address.public_key().clone(), encryption, vec![], message)
                    .await?;
            },
        };

        Ok(())
    }
}
