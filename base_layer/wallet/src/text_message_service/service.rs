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

use crate::text_message_service::{
    error::TextMessageError,
    model::{ReceivedTextMessage, SentTextMessage},
    Contact,
    UpdateContact,
};
use crossbeam_channel as channel;
use diesel::{connection::Connection, SqliteConnection};
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    convert::TryInto,
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_comms::{
    domain_connector::MessageInfo,
    message::{Message, MessageError, MessageFlags},
    outbound_message_service::{outbound_message_service::OutboundMessageService, BroadcastStrategy},
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

const LOG_TARGET: &'static str = "base_layer::wallet::text_messsage_service";

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

/// The TextMessageService manages the local node's text messages. It keeps track of sent messages that require an Ack
/// (pending messages), Ack'ed sent messages and received messages.
pub struct TextMessageService {
    pub_key: CommsPublicKey,
    screen_name: Option<String>,
    oms: Option<Arc<OutboundMessageService>>,
    api: ServiceApiWrapper<TextMessageServiceApi, TextMessageApiRequest, TextMessageApiResult>,
    database_path: String,
}

impl TextMessageService {
    pub fn new(pub_key: CommsPublicKey, database_path: String) -> TextMessageService {
        TextMessageService {
            pub_key,
            screen_name: None,
            oms: None,
            api: Self::setup_api(),
            database_path,
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
    fn send_text_message(
        &mut self,
        dest_pub_key: CommsPublicKey,
        message: String,
        conn: &SqliteConnection,
    ) -> Result<(), TextMessageError>
    {
        let oms = self.oms.clone().ok_or(TextMessageError::OMSNotInitialized)?;

        let count = SentTextMessage::count_by_dest_pub_key(&dest_pub_key.clone(), conn)?;

        let text_message = SentTextMessage::new(self.pub_key.clone(), dest_pub_key, message, Some(count as usize));

        oms.send_message(
            BroadcastStrategy::DirectPublicKey(text_message.dest_pub_key.clone()),
            MessageFlags::ENCRYPTED,
            text_message.clone(),
        )?;

        text_message.commit(conn)?;

        trace!(target: LOG_TARGET, "Text Message Sent to {}", text_message.dest_pub_key);

        Ok(())
    }

    /// Process an incoming text message
    fn receive_text_message(
        &mut self,
        connector: &DomainConnector<'static>,
        conn: &SqliteConnection,
    ) -> Result<(), TextMessageError>
    {
        let oms = self.oms.clone().ok_or(TextMessageError::OMSNotInitialized)?;

        let incoming_msg: Option<(MessageInfo, ReceivedTextMessage)> = connector
            .receive_timeout(Duration::from_millis(10))
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

            msg.commit(conn)?;
        }

        Ok(())
    }

    /// Process an incoming text message Ack
    fn receive_text_message_ack(
        &mut self,
        connector: &DomainConnector<'static>,
        conn: &SqliteConnection,
    ) -> Result<(), TextMessageError>
    {
        let incoming_msg: Option<(MessageInfo, TextMessageAck)> = connector
            .receive_timeout(Duration::from_millis(10))
            .map_err(TextMessageError::ConnectorError)?;

        if let Some((_info, msg_ack)) = incoming_msg {
            debug!(
                target: LOG_TARGET,
                "Text Message Ack received with ID: {:?}",
                msg_ack.id.clone(),
            );
            SentTextMessage::mark_sent_message_ack(msg_ack.id.clone(), conn)?;
        }

        Ok(())
    }

    /// Return a copy of the current lists of messages
    fn get_current_messages(&self, conn: &SqliteConnection) -> Result<TextMessages, TextMessageError> {
        Ok(TextMessages {
            sent_messages: SentTextMessage::index(conn)?,
            received_messages: ReceivedTextMessage::index(conn)?,
        })
    }

    fn get_current_messages_by_pub_key(
        &self,
        pub_key: CommsPublicKey,
        conn: &SqliteConnection,
    ) -> Result<TextMessages, TextMessageError>
    {
        Ok(TextMessages {
            sent_messages: SentTextMessage::find_by_dest_pub_key(&pub_key, conn)?,
            received_messages: ReceivedTextMessage::find_by_source_pub_key(&pub_key, conn)?,
        })
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

    pub fn add_contact(&mut self, contact: Contact, conn: &SqliteConnection) -> Result<(), TextMessageError> {
        let found_contact = Contact::find(&contact.pub_key, conn);
        if let Ok(c) = found_contact {
            if c.pub_key == contact.pub_key {
                return Err(TextMessageError::ContactAlreadyExists);
            }
        }

        contact.commit(&conn)?;

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

    pub fn remove_contact(&mut self, contact: Contact, conn: &SqliteConnection) -> Result<(), TextMessageError> {
        contact.clone().delete(conn)?;

        trace!(
            target: LOG_TARGET,
            "Contact Deleted: Screen name: {:?} - Pub-key: {} - Address: {:?}",
            contact.screen_name.clone(),
            contact.pub_key.clone(),
            contact.address.clone()
        );

        Ok(())
    }

    pub fn get_contacts(&self, conn: &SqliteConnection) -> Result<Vec<Contact>, TextMessageError> {
        Contact::index(conn)
    }

    /// Updates the screen_name of a contact if an existing contact with the same pub_key is found
    pub fn update_contact(
        &mut self,
        pub_key: CommsPublicKey,
        contact_update: UpdateContact,
        conn: &SqliteConnection,
    ) -> Result<(), TextMessageError>
    {
        let contact = Contact::find(&pub_key, conn)?;

        contact.clone().update(contact_update, conn)?;

        trace!(
            target: LOG_TARGET,
            "Contact Updated: Screen name: {:?} - Pub-key: {} - Address: {:?}",
            contact.screen_name.clone(),
            contact.pub_key.clone(),
            contact.address.clone()
        );

        Ok(())
    }

    /// This handler is called when the Service executor loops receives an API request
    fn handle_api_message(
        &mut self,
        msg: TextMessageApiRequest,
        connection: &SqliteConnection,
    ) -> Result<(), ServiceError>
    {
        trace!(target: LOG_TARGET, "[{}] Received API message", self.get_name(),);
        let resp = match msg {
            TextMessageApiRequest::SendTextMessage((destination, message)) => self
                .send_text_message(destination, message, connection)
                .map(|_| TextMessageApiResponse::MessageSent),
            TextMessageApiRequest::GetTextMessages => self
                .get_current_messages(connection)
                .map(|tm| TextMessageApiResponse::TextMessages(tm)),
            TextMessageApiRequest::GetTextMessagesByPubKey(pk) => self
                .get_current_messages_by_pub_key(pk, connection)
                .map(|tm| TextMessageApiResponse::TextMessages(tm)),
            TextMessageApiRequest::GetScreenName => Ok(TextMessageApiResponse::ScreenName(self.get_screen_name())),
            TextMessageApiRequest::SetScreenName(s) => {
                self.set_screen_name(s);
                Ok(TextMessageApiResponse::ScreenNameSet)
            },
            TextMessageApiRequest::AddContact(c) => self
                .add_contact(c, connection)
                .map(|_| TextMessageApiResponse::ContactAdded),
            TextMessageApiRequest::RemoveContact(c) => self
                .remove_contact(c, connection)
                .map(|_| TextMessageApiResponse::ContactRemoved),
            TextMessageApiRequest::GetContacts => self
                .get_contacts(connection)
                .map(|c| TextMessageApiResponse::Contacts(c)),
            TextMessageApiRequest::UpdateContact((pk, c)) => self
                .update_contact(pk, c, connection)
                .map(|_| TextMessageApiResponse::ContactUpdated),
        };

        trace!(target: LOG_TARGET, "[{}] Replying to API", self.get_name());
        self.api
            .send_reply(resp)
            .map_err(ServiceError::internal_service_error())
    }

    // TODO Some sort of accessor that allows for pagination of messages
}

/// A collection to hold a text message state
#[derive(Debug)]
pub struct TextMessages {
    pub received_messages: Vec<ReceivedTextMessage>,
    pub sent_messages: Vec<SentTextMessage>,
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

        // Check if the database file exists
        let mut exists = false;
        if std::fs::metadata(self.database_path.clone()).is_ok() {
            exists = true;
        }

        let connection = SqliteConnection::establish(&self.database_path)
            .map_err(|e| ServiceError::ServiceInitializationFailed(format!("{}", e).to_string()))?;

        connection
            .execute("PRAGMA foreign_keys = ON")
            .map_err(|e| ServiceError::ServiceInitializationFailed(format!("{}", e).to_string()))?;

        if !exists {
            embed_migrations!("./migrations");
            embedded_migrations::run_with_output(&connection, &mut std::io::stdout()).map_err(|e| {
                ServiceError::ServiceInitializationFailed(format!("Database migration failed {}", e).to_string())
            })?;
        }

        debug!(target: LOG_TARGET, "Starting Text Message Service executor");
        loop {
            if let Some(msg) = context.get_control_message(Duration::from_millis(5)) {
                match msg {
                    ServiceControlMessage::Shutdown => break,
                }
            }

            match self.receive_text_message(&connector_text, &connection) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Text Message service had error: {:?}", err);
                },
            }

            match self.receive_text_message_ack(&connector_text_ack, &connection) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Text Message service had error: {:?}", err);
                },
            }

            if let Some(msg) = self
                .api
                .recv_timeout(Duration::from_millis(50))
                .map_err(ServiceError::internal_service_error())?
            {
                self.handle_api_message(msg, &connection)?;
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
    use crate::{
        diesel::Connection,
        text_message_service::{error::TextMessageError, Contact, TextMessageService, UpdateContact},
    };
    use diesel::{result::Error as DieselError, SqliteConnection};
    use std::path::PathBuf;
    use tari_comms::types::CommsPublicKey;
    use tari_crypto::keys::PublicKey;

    fn get_path(name: Option<&str>) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/data");
        path.push(name.unwrap_or(""));
        path.to_str().unwrap().to_string()
    }

    fn clean_up(name: &str) {
        if std::fs::metadata(get_path(Some(name))).is_ok() {
            std::fs::remove_file(get_path(Some(name))).unwrap();
        }
    }

    fn init(name: &str) {
        clean_up(name);
        let path = get_path(None);
        let _ = std::fs::create_dir(&path).unwrap_or_default();
    }

    #[test]
    fn test_contacts_crud() {
        let mut rng = rand::OsRng::new().unwrap();

        let (_secret_key, public_key) = CommsPublicKey::random_keypair(&mut rng);

        let db_name = "test_crud.sqlite3";
        let db_path = get_path(Some(db_name));
        init(db_name);

        let conn = SqliteConnection::establish(&db_path).unwrap();
        embed_migrations!("./migrations");
        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        let mut tms = TextMessageService::new(public_key, db_path);

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
            contacts.push(Contact::new(
                screen_names[i].clone(),
                contact_public_key,
                "127.0.0.1:12345".parse().unwrap(),
            ));
        }

        assert_eq!(tms.get_screen_name(), None);
        tms.set_screen_name("Fred".to_string());
        assert_eq!(tms.get_screen_name(), Some("Fred".to_string()));

        for c in contacts.iter() {
            let _ = tms.add_contact(c.clone(), &conn);
        }

        assert_eq!(tms.get_contacts(&conn).unwrap().len(), 5);

        tms.remove_contact(contacts[0].clone(), &conn).unwrap();

        assert_eq!(tms.get_contacts(&conn).unwrap().len(), 4);

        let update_contact = UpdateContact {
            screen_name: Some("Betty".to_string()),
            address: Some(contacts[1].address.clone()),
        };

        tms.update_contact(contacts[1].pub_key.clone(), update_contact, &conn)
            .unwrap();

        let updated_contacts = tms.get_contacts(&conn).unwrap();
        assert_eq!(updated_contacts[0].screen_name, "Betty".to_string());

        match tms.update_contact(
            CommsPublicKey::default(),
            UpdateContact {
                screen_name: Some("Whatever".to_string()),
                address: Some("127.0.0.1:12345".parse().unwrap()),
            },
            &conn,
        ) {
            Err(TextMessageError::DatabaseError(DieselError::NotFound)) => assert!(true),
            _ => assert!(false),
        }

        clean_up(db_name);
    }
}
