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
    fmt::{Display, Error, Formatter},
    sync::Arc,
};

use chrono::{DateTime, Duration, Utc};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_service_framework::reply_channel::SenderService;
use tokio::sync::broadcast;
use tower::Service;

use crate::contacts_service::{error::ContactsServiceError, service::ContactMessageType, storage::database::Contact};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactsLivenessData {
    public_key: CommsPublicKey,
    node_id: NodeId,
    latency: Option<u32>,
    last_seen: DateTime<Utc>,
    message_type: ContactMessageType,
}

impl ContactsLivenessData {
    pub fn new(
        public_key: CommsPublicKey,
        node_id: NodeId,
        latency: Option<u32>,
        last_seen: DateTime<Utc>,
        message_type: ContactMessageType,
    ) -> Self {
        Self {
            public_key,
            node_id,
            latency,
            last_seen,
            message_type,
        }
    }

    pub fn public_key(&self) -> &CommsPublicKey {
        &self.public_key
    }

    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    pub fn latency(&self) -> Option<u32> {
        self.latency
    }

    pub fn last_ping_pong_received(&self) -> DateTime<Utc> {
        self.last_seen
    }

    pub fn time_since_last_status_update(&self) -> Duration {
        Utc::now() - self.last_seen
    }

    pub fn message_type(&self) -> ContactMessageType {
        self.message_type.clone()
    }
}

impl Display for ContactsLivenessData {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        writeln!(
            f,
            "Node ID {} with latency {:?} last seen {}s ago",
            self.node_id,
            self.latency,
            self.time_since_last_status_update().num_seconds()
        )
    }
}

#[derive(Debug)]
pub enum ContactsLivenessEvent {
    StatusUpdated(Vec<ContactsLivenessData>),
    NetworkSilence,
}

#[derive(Debug)]
pub enum ContactsServiceRequest {
    GetContact(CommsPublicKey),
    UpsertContact(Contact),
    RemoveContact(CommsPublicKey),
    GetContacts,
}

#[derive(Debug)]
pub enum ContactsServiceResponse {
    ContactSaved,
    ContactRemoved(Contact),
    Contact(Contact),
    Contacts(Vec<Contact>),
}

#[derive(Clone)]
pub struct ContactsServiceHandle {
    request_response_service:
        SenderService<ContactsServiceRequest, Result<ContactsServiceResponse, ContactsServiceError>>,
    liveness_events: broadcast::Sender<Arc<ContactsLivenessEvent>>,
}

impl ContactsServiceHandle {
    pub fn new(
        request_response_service: SenderService<
            ContactsServiceRequest,
            Result<ContactsServiceResponse, ContactsServiceError>,
        >,
        liveness_events: broadcast::Sender<Arc<ContactsLivenessEvent>>,
    ) -> Self {
        Self {
            request_response_service,
            liveness_events,
        }
    }

    pub async fn get_contact(&mut self, pub_key: CommsPublicKey) -> Result<Contact, ContactsServiceError> {
        match self
            .request_response_service
            .call(ContactsServiceRequest::GetContact(pub_key))
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

    pub async fn remove_contact(&mut self, pub_key: CommsPublicKey) -> Result<Contact, ContactsServiceError> {
        match self
            .request_response_service
            .call(ContactsServiceRequest::RemoveContact(pub_key))
            .await??
        {
            ContactsServiceResponse::ContactRemoved(c) => Ok(c),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub fn get_contacts_liveness_event_stream(&self) -> broadcast::Receiver<Arc<ContactsLivenessEvent>> {
        self.liveness_events.subscribe()
    }
}
