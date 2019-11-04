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

use crate::contacts_service::{error::ContactsServiceError, storage::database::Contact};
use tari_comms::types::CommsPublicKey;
use tari_service_framework::reply_channel::SenderService;
use tower::Service;

#[derive(Debug)]
pub enum ContactsServiceRequest {
    GetContact(CommsPublicKey),
    SaveContact(Contact),
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
    handle: SenderService<ContactsServiceRequest, Result<ContactsServiceResponse, ContactsServiceError>>,
}
impl ContactsServiceHandle {
    pub fn new(
        handle: SenderService<ContactsServiceRequest, Result<ContactsServiceResponse, ContactsServiceError>>,
    ) -> Self {
        Self { handle }
    }

    pub async fn get_contact(&mut self, pub_key: CommsPublicKey) -> Result<Contact, ContactsServiceError> {
        match self.handle.call(ContactsServiceRequest::GetContact(pub_key)).await?? {
            ContactsServiceResponse::Contact(c) => Ok(c),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_contacts(&mut self) -> Result<Vec<Contact>, ContactsServiceError> {
        match self.handle.call(ContactsServiceRequest::GetContacts).await?? {
            ContactsServiceResponse::Contacts(c) => Ok(c),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn save_contact(&mut self, contact: Contact) -> Result<(), ContactsServiceError> {
        match self.handle.call(ContactsServiceRequest::SaveContact(contact)).await?? {
            ContactsServiceResponse::ContactSaved => Ok(()),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn remove_contact(&mut self, pub_key: CommsPublicKey) -> Result<Contact, ContactsServiceError> {
        match self
            .handle
            .call(ContactsServiceRequest::RemoveContact(pub_key))
            .await??
        {
            ContactsServiceResponse::ContactRemoved(c) => Ok(c),
            _ => Err(ContactsServiceError::UnexpectedApiResponse),
        }
    }
}
