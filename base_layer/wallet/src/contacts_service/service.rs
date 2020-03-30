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

use crate::contacts_service::{
    error::ContactsServiceError,
    handle::{ContactsServiceRequest, ContactsServiceResponse},
    storage::database::{ContactsBackend, ContactsDatabase},
};
use futures::{pin_mut, StreamExt};
use log::*;
use tari_service_framework::reply_channel;

const LOG_TARGET: &str = "wallet:contacts_service";

pub struct ContactsService<T>
where T: ContactsBackend + 'static
{
    db: ContactsDatabase<T>,
    request_stream:
        Option<reply_channel::Receiver<ContactsServiceRequest, Result<ContactsServiceResponse, ContactsServiceError>>>,
}

impl<T> ContactsService<T>
where T: ContactsBackend + 'static
{
    pub fn new(
        request_stream: reply_channel::Receiver<
            ContactsServiceRequest,
            Result<ContactsServiceResponse, ContactsServiceError>,
        >,

        db: ContactsDatabase<T>,
    ) -> Self
    {
        Self {
            db,
            request_stream: Some(request_stream),
        }
    }

    pub async fn start(mut self) -> Result<(), ContactsServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Contacts Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        info!(target: LOG_TARGET, "Contacts Service started");
        loop {
            futures::select! {
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", resp);
                        Err(resp)
                    })).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                complete => {
                    info!(target: LOG_TARGET, "Contacts service shutting down");
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
    ) -> Result<ContactsServiceResponse, ContactsServiceError>
    {
        match request {
            ContactsServiceRequest::GetContact(pk) => {
                Ok(self.db.get_contact(pk).await.map(ContactsServiceResponse::Contact)?)
            },
            ContactsServiceRequest::UpsertContact(c) => {
                self.db.upsert_contact(c.clone()).await?;
                info!(
                    target: LOG_TARGET,
                    "Contact Saved: \nAlias: {}\nPubKey: {} ", c.alias, c.public_key
                );
                Ok(ContactsServiceResponse::ContactSaved)
            },
            ContactsServiceRequest::RemoveContact(pk) => {
                let result = self.db.remove_contact(pk).await?;
                info!(
                    target: LOG_TARGET,
                    "Contact Removed: \nAlias: {}\nPubKey: {} ", result.alias, result.public_key
                );
                Ok(ContactsServiceResponse::ContactRemoved(result))
            },
            ContactsServiceRequest::GetContacts => {
                Ok(self.db.get_contacts().await.map(ContactsServiceResponse::Contacts)?)
            },
        }
    }
}
