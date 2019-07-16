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

use crate::types::HashDigest;
use chrono::prelude::*;
use core::cmp::Ordering;
use crossbeam_channel as channel;
use derive_error::Error;
use digest::Digest;
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::TryInto,
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_comms::{
    builder::CommsServicesError,
    connection::NetAddress,
    dispatcher::DispatchError,
    domain_connector::{ConnectorError, MessageInfo},
    message::{Message, MessageError, MessageFlags},
    outbound_message_service::{outbound_message_service::OutboundMessageService, BroadcastStrategy, OutboundError},
    types::CommsPublicKey,
    DomainConnector,
};
use tari_p2p::{
    ping_pong::PingPong,
    services::{
        Service,
        ServiceApiWrapper,
        ServiceContext,
        ServiceControlMessage,
        ServiceError,
        DEFAULT_API_TIMEOUT_MS,
    },
    tari_message::{ExtendedMessage, TariMessageType},
};
use tari_utilities::{byte_array::ByteArray, hex::Hex, message_format::MessageFormatError};
const LOG_TARGET: &'static str = "base_layer::wallet::text_messsage_service";

#[derive(Debug, Error)]
pub enum TextMessageError {
    MessageFormatError(MessageFormatError),
    DispatchError(DispatchError),
    MessageError(MessageError),
    OutboundError(OutboundError),
    ServiceError(ServiceError),
    ConnectorError(ConnectorError),
    CommsServicesError(CommsServicesError),
    /// If a received TextMessageAck doesn't matching any pending messages
    MessageNotFound,
    /// Failed to send from API
    ApiSendFailed,
    /// Failed to receive in API from service
    ApiReceiveFailed,
    /// The Outbound Message Service is not initialized
    OMSNotInitialized,
    /// The Comms service stack is not initialized
    CommsNotInitialized,
    /// Received an unexpected API response
    UnexpectedApiResponse,
    /// Contact not found
    ContactNotFound,
    /// Contact already exists
    ContactAlreadyExists,
}

/// Represents a single Text Message
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextMessage {
    pub id: Vec<u8>,
    pub source_pub_key: CommsPublicKey,
    pub dest_pub_key: CommsPublicKey,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

impl TryInto<Message> for TextMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(ExtendedMessage::Text), self).try_into()?)
    }
}

impl PartialOrd<TextMessage> for TextMessage {
    /// Orders OutboundMessage from least to most time remaining from being scheduled
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.timestamp.partial_cmp(&other.timestamp)
    }
}

impl Ord for TextMessage {
    /// Orders OutboundMessage from least to most time remaining from being scheduled
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}

/// Represents an Acknowledgement of receiving a Text Message
#[derive(Debug, Serialize, Deserialize)]
pub struct TextMessageAck {
    id: Vec<u8>,
}

impl TryInto<Message> for TextMessageAck {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(ExtendedMessage::TextAck), self).try_into()?)
    }
}

/// A message contact
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Contact {
    pub screen_name: String,
    pub pub_key: CommsPublicKey,
    pub address: NetAddress,
}

/// The updatable fields of message contact
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateContact {
    pub screen_name: String,
    pub address: NetAddress,
}

/// This function generates a unique ID hash for a Text Message from the message components and an index integer
///
/// `index`: This value should be incremented for every message sent to the same destination. This ensures that if you
/// send a duplicate message to the same destination that the ID hashes will be unique
fn generate_id<D: Digest>(
    source_pub_key: &CommsPublicKey,
    dest_pub_key: &CommsPublicKey,
    message: &String,
    timestamp: &DateTime<Utc>,
    index: usize,
) -> Vec<u8>
{
    D::new()
        .chain(source_pub_key.as_bytes())
        .chain(dest_pub_key.as_bytes())
        .chain(message.as_bytes())
        .chain(timestamp.to_string())
        .chain(index.to_le_bytes())
        .result()
        .to_vec()
}

/// The TextMessageService manages the local node's text messages. It keeps track of sent messages that require an Ack
/// (pending messages), Ack'ed sent messages and received messages.
pub struct TextMessageService {
    pub_key: CommsPublicKey,
    pending_messages: HashMap<Vec<u8>, TextMessage>,
    sent_messages: Vec<TextMessage>,
    received_messages: Vec<TextMessage>,
    screen_name: Option<String>,
    contacts: Vec<Contact>,
    oms: Option<Arc<OutboundMessageService>>,
    api: ServiceApiWrapper<TextMessageServiceApi, TextMessageApiRequest, TextMessageApiResult>,
}

impl TextMessageService {
    pub fn new(pub_key: CommsPublicKey) -> TextMessageService {
        TextMessageService {
            pub_key,
            pending_messages: HashMap::new(),
            sent_messages: Vec::new(),
            received_messages: Vec::new(),
            screen_name: None,
            contacts: Vec::new(),
            oms: None,
            api: Self::setup_api(),
        }
    }

    /// Return this service's API
    pub fn get_api(&self) -> Arc<TextMessageServiceApi> {
        self.api.get_api()
    }

    fn setup_api() -> ServiceApiWrapper<TextMessageServiceApi, TextMessageApiRequest, TextMessageApiResult> {
        let (api_sender, service_receiver) = channel::bounded(0);
        let (service_sender, api_receiver) = channel::bounded(0);

        let api = Arc::new(TextMessageServiceApi::new(api_sender, api_receiver));
        ServiceApiWrapper::new(service_receiver, service_sender, api)
    }

    /// Send a text message to the specified node using the provided OMS
    fn send_text_message(&mut self, dest_pub_key: CommsPublicKey, message: String) -> Result<(), TextMessageError> {
        let oms = self.oms.clone().ok_or(TextMessageError::OMSNotInitialized)?;

        let timestamp = Utc::now();
        let count = self
            .sent_messages
            .iter()
            .filter(|i| i.dest_pub_key == dest_pub_key)
            .count() +
            self.pending_messages
                .values()
                .filter(|i| i.dest_pub_key == dest_pub_key)
                .count();
        let text_message = TextMessage {
            id: generate_id::<HashDigest>(&self.pub_key, &dest_pub_key, &message, &timestamp, count),
            source_pub_key: self.pub_key.clone(),
            dest_pub_key,
            message,
            timestamp,
        };

        oms.send_message(
            BroadcastStrategy::DirectPublicKey(text_message.dest_pub_key.clone()),
            MessageFlags::ENCRYPTED,
            text_message.clone(),
        )?;
        self.pending_messages
            .insert(text_message.id.clone(), text_message.clone());

        trace!(target: LOG_TARGET, "Text Message Sent to {}", text_message.dest_pub_key);

        Ok(())
    }

    /// Process an incoming text message
    fn receive_text_message(&mut self, connector: &DomainConnector<'static>) -> Result<(), TextMessageError> {
        let oms = self.oms.clone().ok_or(TextMessageError::OMSNotInitialized)?;

        let incoming_msg: Option<(MessageInfo, TextMessage)> = connector
            .receive_timeout(Duration::from_millis(1))
            .map_err(TextMessageError::ConnectorError)?;

        if let Some((info, msg)) = incoming_msg {
            trace!(
                target: LOG_TARGET,
                "Text Message received with ID: {:?} from {} with message: {:?}",
                msg.id.clone(),
                msg.source_pub_key,
                msg.message.clone()
            );

            let text_message_ack = TextMessageAck { id: msg.clone().id };
            oms.send_message(
                BroadcastStrategy::DirectPublicKey(info.source_identity.public_key),
                MessageFlags::ENCRYPTED,
                text_message_ack,
            )?;

            self.received_messages.push(msg.clone());
        }

        Ok(())
    }

    /// Process an incoming text message Ack
    fn receive_text_message_ack(&mut self, connector: &DomainConnector<'static>) -> Result<(), TextMessageError> {
        let incoming_msg: Option<(MessageInfo, TextMessageAck)> = connector
            .receive_timeout(Duration::from_millis(1))
            .map_err(TextMessageError::ConnectorError)?;

        if let Some((_info, msg_ack)) = incoming_msg {
            debug!(
                target: LOG_TARGET,
                "Text Message Ack received with ID: {:?}",
                msg_ack.id.clone(),
            );
            match self.pending_messages.remove(&msg_ack.id) {
                Some(m) => self.sent_messages.push(m),
                None => return Err(TextMessageError::MessageNotFound),
            }
        }

        Ok(())
    }

    /// Return a copy of the current lists of messages
    /// TODO Remove this in memory storing of message in favour of Sqlite persistence
    fn get_current_messages(&self) -> TextMessages {
        let mut pending_messages: Vec<TextMessage> = Vec::new();

        for (_k, v) in self.pending_messages.iter() {
            pending_messages.push(v.clone());
        }

        TextMessages {
            pending_messages,
            sent_messages: self.sent_messages.clone(),
            received_messages: self.received_messages.clone(),
        }
    }

    fn get_current_messages_by_pub_key(&self, pub_key: CommsPublicKey) -> TextMessages {
        let mut pending_messages: Vec<TextMessage> = Vec::new();

        for (_k, v) in self.pending_messages.iter() {
            if v.dest_pub_key == pub_key {
                pending_messages.push(v.clone());
            }
        }
        TextMessages {
            pending_messages,
            sent_messages: self
                .sent_messages
                .iter()
                .filter(|t| t.dest_pub_key == pub_key)
                .cloned()
                .collect(),
            received_messages: self
                .received_messages
                .iter()
                .filter(|t| t.source_pub_key == pub_key)
                .cloned()
                .collect(),
        }
    }

    pub fn get_pub_key(&self) -> CommsPublicKey {
        self.pub_key.clone()
    }

    pub fn set_pub_key(&mut self, pub_key: CommsPublicKey) {
        self.pub_key = pub_key;
    }

    pub fn get_screen_name(&self) -> Option<String> {
        self.screen_name.clone()
    }

    pub fn set_screen_name(&mut self, screen_name: String) {
        self.screen_name = Some(screen_name);
    }

    pub fn add_contact(&mut self, contact: Contact) -> Result<(), TextMessageError> {
        if self.contacts.iter().any(|c| c.pub_key == contact.pub_key) {
            return Err(TextMessageError::ContactAlreadyExists);
        }
        self.contacts.push(contact.clone());
        // Send ping to the contact so that if they are online they will flush all outstanding messages for this node
        let oms = self.oms.clone().ok_or(TextMessageError::OMSNotInitialized)?;
        oms.send_message(
            BroadcastStrategy::DirectPublicKey(contact.pub_key.clone()),
            MessageFlags::empty(),
            PingPong::Ping,
        )?;

        trace!(
            target: LOG_TARGET,
            "Contact Added: Screen name: {:?} - Pub-key: {} - Address: {:?}",
            contact.screen_name.clone(),
            contact.pub_key.clone(),
            contact.address.clone()
        );
        Ok(())
    }

    pub fn remove_contact(&mut self, contact: Contact) -> Result<(), TextMessageError> {
        let position = self
            .contacts
            .iter()
            .position(|x| x == &contact)
            .ok_or(TextMessageError::ContactNotFound)?;

        let _ = self.contacts.remove(position);

        trace!(
            target: LOG_TARGET,
            "Contact Added: Screen name: {:?} - Pub-key: {} - Address: {:?}",
            contact.screen_name.clone(),
            contact.pub_key.clone(),
            contact.address.clone()
        );

        Ok(())
    }

    pub fn get_contacts(&self) -> Vec<Contact> {
        self.contacts.clone()
    }

    /// Updates the screen_name of a contact if an existing contact with the same pub_key is found
    pub fn update_contact(&mut self, pub_key: CommsPublicKey, contact: UpdateContact) -> Result<(), TextMessageError> {
        let found_contact = self
            .contacts
            .iter_mut()
            .find(|c| c.pub_key == pub_key)
            .ok_or(TextMessageError::ContactNotFound)?;
        found_contact.screen_name = contact.screen_name.clone();

        trace!(
            target: LOG_TARGET,
            "Contact Added: Screen name: {:?} - Pub-key: {} - Address: {:?}",
            found_contact.screen_name.clone(),
            found_contact.pub_key.clone(),
            found_contact.address.clone()
        );

        Ok(())
    }

    /// This handler is called when the Service executor loops receives an API request
    fn handle_api_message(&mut self, msg: TextMessageApiRequest) -> Result<(), ServiceError> {
        trace!(target: LOG_TARGET, "[{}] Received API message", self.get_name(),);
        let resp = match msg {
            TextMessageApiRequest::SendTextMessage((destination, message)) => self
                .send_text_message(destination, message)
                .map(|_| TextMessageApiResponse::MessageSent),
            TextMessageApiRequest::GetTextMessages => {
                Ok(TextMessageApiResponse::TextMessages(self.get_current_messages()))
            },
            TextMessageApiRequest::GetTextMessagesByPubKey(pk) => Ok(TextMessageApiResponse::TextMessages(
                self.get_current_messages_by_pub_key(pk),
            )),
            TextMessageApiRequest::GetScreenName => Ok(TextMessageApiResponse::ScreenName(self.get_screen_name())),
            TextMessageApiRequest::SetScreenName(s) => {
                self.set_screen_name(s);
                Ok(TextMessageApiResponse::ScreenNameSet)
            },
            TextMessageApiRequest::AddContact(c) => self.add_contact(c).map(|_| TextMessageApiResponse::ContactAdded),
            TextMessageApiRequest::RemoveContact(c) => {
                self.remove_contact(c).map(|_| TextMessageApiResponse::ContactRemoved)
            },
            TextMessageApiRequest::GetContacts => Ok(TextMessageApiResponse::Contacts(self.get_contacts())),
            TextMessageApiRequest::UpdateContact((pk, c)) => self
                .update_contact(pk, c)
                .map(|_| TextMessageApiResponse::ContactUpdated),
        };

        trace!(target: LOG_TARGET, "[{}] Replying to API", self.get_name());
        self.api
            .send_reply(resp)
            .map_err(ServiceError::internal_service_error())
    }

    // TODO return a reference to the text messages when requested via API rather than copying them. (This will be taken
    // care of when moving to Sqlite persistence)
    // TODO Disk persistence of messages
    // TODO Some sort of accessor that allows for pagination of messages
}

/// A collection to hold a text message state
#[derive(Debug)]
pub struct TextMessages {
    pub pending_messages: Vec<TextMessage>,
    pub sent_messages: Vec<TextMessage>,
    pub received_messages: Vec<TextMessage>,
}

/// The Domain Service trait implementation for the TestMessageService
impl Service for TextMessageService {
    fn get_name(&self) -> String {
        "Text Message service".to_string()
    }

    fn get_message_types(&self) -> Vec<TariMessageType> {
        vec![ExtendedMessage::Text.into(), ExtendedMessage::TextAck.into()]
    }

    /// Function called by the Service Executor in its own thread. This function polls for both API request and Comms
    /// layer messages from the Message Broker
    fn execute(&mut self, context: ServiceContext) -> Result<(), ServiceError> {
        let connector_text = context.create_connector(&ExtendedMessage::Text.into()).map_err(|err| {
            ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
        })?;

        let connector_text_ack = context
            .create_connector(&ExtendedMessage::TextAck.into())
            .map_err(|err| {
                ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
            })?;

        self.oms = Some(context.outbound_message_service());
        debug!(target: LOG_TARGET, "Starting Text Message Service executor");
        loop {
            if let Some(msg) = context.get_control_message(Duration::from_millis(5)) {
                match msg {
                    ServiceControlMessage::Shutdown => break,
                }
            }

            match self.receive_text_message(&connector_text) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Text Message service had error: {:?}", err);
                },
            }

            match self.receive_text_message_ack(&connector_text_ack) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Text Message service had error: {:?}", err);
                },
            }

            if let Some(msg) = self
                .api
                .recv_timeout(Duration::from_millis(5))
                .map_err(ServiceError::internal_service_error())?
            {
                self.handle_api_message(msg)?;
            }
        }

        Ok(())
    }
}

/// API Request enum
#[derive(Debug)]
pub enum TextMessageApiRequest {
    SendTextMessage((CommsPublicKey, String)),
    GetTextMessages,
    GetTextMessagesByPubKey(CommsPublicKey),
    SetScreenName(String),
    GetScreenName,
    AddContact(Contact),
    RemoveContact(Contact),
    GetContacts,
    UpdateContact((CommsPublicKey, UpdateContact)),
}

/// API Response enum
#[derive(Debug)]
pub enum TextMessageApiResponse {
    MessageSent,
    TextMessages(TextMessages),
    ScreenName(Option<String>),
    ScreenNameSet,
    ContactAdded,
    ContactRemoved,
    Contacts(Vec<Contact>),
    ContactUpdated,
}

/// Result for all API requests
pub type TextMessageApiResult = Result<TextMessageApiResponse, TextMessageError>;

/// The TextMessage service public API that other services and application will use to interact with this service.
/// The requests and responses are transmitted via channels into the Service Executor thread where this service is
/// running
pub struct TextMessageServiceApi {
    sender: channel::Sender<TextMessageApiRequest>,
    receiver: channel::Receiver<TextMessageApiResult>,
    mutex: Mutex<()>,
    timeout: Duration,
}

impl TextMessageServiceApi {
    fn new(sender: channel::Sender<TextMessageApiRequest>, receiver: channel::Receiver<TextMessageApiResult>) -> Self {
        Self {
            sender,
            receiver,
            mutex: Mutex::new(()),
            timeout: Duration::from_millis(DEFAULT_API_TIMEOUT_MS),
        }
    }

    pub fn send_text_message(&self, destination: CommsPublicKey, message: String) -> Result<(), TextMessageError> {
        self.send_recv(TextMessageApiRequest::SendTextMessage((destination, message)))
            .and_then(|resp| match resp {
                TextMessageApiResponse::MessageSent => Ok(()),
                _ => Err(TextMessageError::UnexpectedApiResponse),
            })
    }

    pub fn get_text_messages(&self) -> Result<TextMessages, TextMessageError> {
        self.send_recv(TextMessageApiRequest::GetTextMessages)
            .and_then(|resp| match resp {
                TextMessageApiResponse::TextMessages(msgs) => Ok(msgs),
                _ => Err(TextMessageError::UnexpectedApiResponse),
            })
    }

    pub fn get_text_messages_by_pub_key(&self, pub_key: CommsPublicKey) -> Result<TextMessages, TextMessageError> {
        self.send_recv(TextMessageApiRequest::GetTextMessagesByPubKey(pub_key))
            .and_then(|resp| match resp {
                TextMessageApiResponse::TextMessages(msgs) => Ok(msgs),
                _ => Err(TextMessageError::UnexpectedApiResponse),
            })
    }

    pub fn get_screen_name(&self) -> Result<Option<String>, TextMessageError> {
        self.send_recv(TextMessageApiRequest::GetScreenName)
            .and_then(|resp| match resp {
                TextMessageApiResponse::ScreenName(s) => Ok(s),
                _ => Err(TextMessageError::UnexpectedApiResponse),
            })
    }

    pub fn set_screen_name(&self, screen_name: String) -> Result<(), TextMessageError> {
        self.send_recv(TextMessageApiRequest::SetScreenName(screen_name))
            .and_then(|resp| match resp {
                TextMessageApiResponse::ScreenNameSet => Ok(()),
                _ => Err(TextMessageError::UnexpectedApiResponse),
            })
    }

    pub fn add_contact(&self, contact: Contact) -> Result<(), TextMessageError> {
        self.send_recv(TextMessageApiRequest::AddContact(contact))
            .and_then(|resp| match resp {
                TextMessageApiResponse::ContactAdded => Ok(()),
                _ => Err(TextMessageError::UnexpectedApiResponse),
            })
    }

    pub fn remove_contact(&self, contact: Contact) -> Result<(), TextMessageError> {
        self.send_recv(TextMessageApiRequest::RemoveContact(contact))
            .and_then(|resp| match resp {
                TextMessageApiResponse::ContactRemoved => Ok(()),
                _ => Err(TextMessageError::UnexpectedApiResponse),
            })
    }

    pub fn get_contacts(&self) -> Result<Vec<Contact>, TextMessageError> {
        self.send_recv(TextMessageApiRequest::GetContacts)
            .and_then(|resp| match resp {
                TextMessageApiResponse::Contacts(v) => Ok(v),
                _ => Err(TextMessageError::UnexpectedApiResponse),
            })
    }

    pub fn update_contact(&self, pub_key: CommsPublicKey, contact: UpdateContact) -> Result<(), TextMessageError> {
        self.send_recv(TextMessageApiRequest::UpdateContact((pub_key, contact)))
            .and_then(|resp| match resp {
                TextMessageApiResponse::ContactUpdated => Ok(()),
                _ => Err(TextMessageError::UnexpectedApiResponse),
            })
    }

    fn send_recv(&self, msg: TextMessageApiRequest) -> TextMessageApiResult {
        self.lock(|| -> TextMessageApiResult {
            self.sender.send(msg).map_err(|_| TextMessageError::ApiSendFailed)?;
            self.receiver
                .recv_timeout(self.timeout.clone())
                .map_err(|_| TextMessageError::ApiReceiveFailed)?
        })
    }

    fn lock<F, T>(&self, func: F) -> T
    where F: FnOnce() -> T {
        let lock = acquire_lock!(self.mutex);
        let res = func();
        drop(lock);
        res
    }
}

#[cfg(test)]
mod test {
    use crate::text_message_service::{Contact, TextMessageError, TextMessageService, UpdateContact};
    use tari_comms::types::CommsPublicKey;
    use tari_crypto::keys::PublicKey;

    #[test]
    fn test_contacts_crud() {
        let mut rng = rand::OsRng::new().unwrap();

        let (_secret_key, public_key) = CommsPublicKey::random_keypair(&mut rng);

        let mut tms = TextMessageService::new(public_key);

        let mut contacts = Vec::new();

        let screen_names = vec![
            "Alice".to_string(),
            "Bob".to_string(),
            "Carol".to_string(),
            "Dave".to_string(),
            "Eric".to_string(),
        ];
        for i in 0..5 {
            let (_contact_secret_key, contact_public_key) = CommsPublicKey::random_keypair(&mut rng);
            contacts.push(Contact {
                screen_name: screen_names[i].clone(),
                pub_key: contact_public_key,
                address: "127.0.0.1:12345".parse().unwrap(),
            });
        }

        assert_eq!(tms.get_screen_name(), None);
        tms.set_screen_name("Fred".to_string());
        assert_eq!(tms.get_screen_name(), Some("Fred".to_string()));

        for c in contacts.iter() {
            let _ = tms.add_contact(c.clone());
        }

        assert_eq!(tms.get_contacts().len(), 5);

        tms.remove_contact(contacts[0].clone()).unwrap();

        assert_eq!(tms.get_contacts().len(), 4);

        let update_contact = UpdateContact {
            screen_name: "Betty".to_string(),
            address: contacts[1].address.clone(),
        };

        tms.update_contact(contacts[1].pub_key.clone(), update_contact).unwrap();

        let updated_contacts = tms.get_contacts();
        assert_eq!(updated_contacts[0].screen_name, "Betty".to_string());

        match tms.update_contact(CommsPublicKey::default(), UpdateContact {
            screen_name: "Whatever".to_string(),
            address: "127.0.0.1:12345".parse().unwrap(),
        }) {
            Err(TextMessageError::ContactNotFound) => assert!(true),
            _ => assert!(false),
        }
    }

}
