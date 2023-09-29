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
    sync::Arc,
};

use chrono::{DateTime, Local, NaiveDateTime};
use tari_common_types::tari_address::TariAddress;
use tari_comms::peer_manager::NodeId;
use tari_service_framework::reply_channel::SenderService;
use tokio::sync::broadcast;
use tower::Service;

use crate::contacts_service::{
    error::ContactsServiceError,
    service::{ContactMessageType, ContactOnlineStatus},
    types::{Contact, Message},
};

pub static DEFAULT_MESSAGE_LIMIT: u64 = 35;
pub static MAX_MESSAGE_LIMIT: u64 = 2500;
pub static DEFAULT_MESSAGE_PAGE: u64 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactsLivenessData {
    address: TariAddress,
    node_id: NodeId,
    latency: Option<u32>,
    last_seen: Option<NaiveDateTime>,
    message_type: ContactMessageType,
    online_status: ContactOnlineStatus,
}

impl ContactsLivenessData {
    pub fn new(
        address: TariAddress,
        node_id: NodeId,
        latency: Option<u32>,
        last_seen: Option<NaiveDateTime>,
        message_type: ContactMessageType,
        online_status: ContactOnlineStatus,
    ) -> Self {
        Self {
            address,
            node_id,
            latency,
            last_seen,
            message_type,
            online_status,
        }
    }

    pub fn address(&self) -> &TariAddress {
        &self.address
    }

    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    pub fn latency(&self) -> Option<u32> {
        self.latency
    }

    pub fn last_ping_pong_received(&self) -> Option<NaiveDateTime> {
        self.last_seen
    }

    pub fn message_type(&self) -> ContactMessageType {
        self.message_type.clone()
    }

    pub fn online_status(&self) -> ContactOnlineStatus {
        self.online_status.clone()
    }

    pub fn set_offline(&mut self) {
        self.online_status = ContactOnlineStatus::Offline
    }
}

impl Display for ContactsLivenessData {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        writeln!(
            f,
            "Liveness event '{}' for contact {} ({}) {}",
            self.message_type,
            self.address,
            self.node_id,
            if let Some(time) = self.last_seen {
                let local_time = DateTime::<Local>::from_utc(time, Local::now().offset().to_owned())
                    .format("%FT%T")
                    .to_string();
                format!("last seen {} is '{}'", local_time, self.online_status)
            } else {
                " - contact was never seen".to_string()
            }
        )
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ContactsLivenessEvent {
    StatusUpdated(Box<ContactsLivenessData>),
    NetworkSilence,
}

#[derive(Debug)]
pub enum ContactsServiceRequest {
    GetContact(TariAddress),
    UpsertContact(Contact),
    RemoveContact(TariAddress),
    GetContacts,
    GetContactOnlineStatus(Contact),
    SendMessage(TariAddress, Message),
    GetMessages(TariAddress, i64, i64),
    SendDeliveryConfirmation(TariAddress, Vec<u8>),
}

#[derive(Debug)]
pub enum ContactsServiceResponse {
    ContactSaved,
    ContactRemoved(Contact),
    Contact(Contact),
    Contacts(Vec<Contact>),
    OnlineStatus(ContactOnlineStatus),
    Messages(Vec<Message>),
    MessageSent,
}

#[derive(Clone)]
pub struct ContactsServiceHandle {
    request_response_service:
        SenderService<ContactsServiceRequest, Result<ContactsServiceResponse, ContactsServiceError>>,
    liveness_events: broadcast::Sender<Arc<ContactsLivenessEvent>>,
    message_events: broadcast::Sender<Arc<Message>>,
}

impl ContactsServiceHandle {
    pub fn new(
        request_response_service: SenderService<
            ContactsServiceRequest,
            Result<ContactsServiceResponse, ContactsServiceError>,
        >,
        liveness_events: broadcast::Sender<Arc<ContactsLivenessEvent>>,
        message_events: broadcast::Sender<Arc<Message>>,
    ) -> Self {
        Self {
            request_response_service,
            liveness_events,
            message_events,
        }
    }

    pub async fn get_contact(&mut self, address: TariAddress) -> Result<Contact, ContactsServiceError> {
        match self
            .request_response_service
            .call(ContactsServiceRequest::GetContact(address))
            .await??
        {
            ContactsServiceResponse::Contact(c) => Ok(c),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_contacts(&mut self) -> Result<Vec<Contact>, ContactsServiceError> {
        match self
            .request_response_service
            .call(ContactsServiceRequest::GetContacts)
            .await??
        {
            ContactsServiceResponse::Contacts(c) => Ok(c),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn upsert_contact(&mut self, contact: Contact) -> Result<(), ContactsServiceError> {
        match self
            .request_response_service
            .call(ContactsServiceRequest::UpsertContact(contact))
            .await??
        {
            ContactsServiceResponse::ContactSaved => Ok(()),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn remove_contact(&mut self, address: TariAddress) -> Result<Contact, ContactsServiceError> {
        match self
            .request_response_service
            .call(ContactsServiceRequest::RemoveContact(address))
            .await??
        {
            ContactsServiceResponse::ContactRemoved(c) => Ok(c),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub fn get_contacts_liveness_event_stream(&self) -> broadcast::Receiver<Arc<ContactsLivenessEvent>> {
        self.liveness_events.subscribe()
    }

    pub fn get_messages_event_stream(&self) -> broadcast::Receiver<Arc<Message>> {
        self.message_events.subscribe()
    }

    /// Determines the contact's online status based on their last seen time
    pub async fn get_contact_online_status(
        &mut self,
        contact: Contact,
    ) -> Result<ContactOnlineStatus, ContactsServiceError> {
        match self
            .request_response_service
            .call(ContactsServiceRequest::GetContactOnlineStatus(contact))
            .await??
        {
            ContactsServiceResponse::OnlineStatus(status) => Ok(status),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_messages(
        &mut self,
        pk: TariAddress,
        mut limit: u64,
        mut page: u64,
    ) -> Result<Vec<Message>, ContactsServiceError> {
        if limit == 0 || limit > MAX_MESSAGE_LIMIT {
            limit = DEFAULT_MESSAGE_LIMIT;
        }

        page = match page.checked_mul(limit) {
            Some(_) => page,
            None => DEFAULT_MESSAGE_PAGE,
        };

        // const values won't be a problem here
        #[allow(clippy::cast_possible_wrap)]
        match self
            .request_response_service
            .call(ContactsServiceRequest::GetMessages(
                pk,
                i64::try_from(limit).unwrap_or(DEFAULT_MESSAGE_LIMIT as i64),
                i64::try_from(page).unwrap_or(DEFAULT_MESSAGE_PAGE as i64),
            ))
            .await??
        {
            ContactsServiceResponse::Messages(messages) => Ok(messages),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn send_message(&mut self, message: Message) -> Result<(), ContactsServiceError> {
        match self
            .request_response_service
            .call(ContactsServiceRequest::SendMessage(message.address.clone(), message))
            .await??
        {
            ContactsServiceResponse::MessageSent => Ok(()),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }
}
