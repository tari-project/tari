// Copyright 2019 The Tari Project
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
    model::{Contact, UpdateContact},
    service::TextMessages,
};
use crate::text_message_service::error::TextMessageError;
use futures::{stream::Fuse, StreamExt};
use tari_broadcast_channel::Subscriber;
use tari_comms::types::CommsPublicKey;
use tari_service_framework::reply_channel::SenderService;
use tower::Service;

/// API Request enum
#[derive(Debug, PartialEq)]
pub enum TextMessageRequest {
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
pub enum TextMessageResponse {
    MessageSent,
    TextMessages(TextMessages),
    ScreenName(Option<String>),
    ScreenNameSet,
    ContactAdded,
    ContactRemoved,
    Contacts(Vec<Contact>),
    ContactUpdated,
}

/// Events that can be published on the Text Message Service Event Stream
#[derive(Debug, Hash, PartialEq, Eq)]
pub enum TextMessageEvent {
    ReceivedTextMessage,
    ReceivedTextMessageAck,
}

#[derive(Clone)]
pub struct TextMessageHandle {
    handle: SenderService<TextMessageRequest, Result<TextMessageResponse, TextMessageError>>,
    event_stream: Subscriber<TextMessageEvent>,
}

impl TextMessageHandle {
    pub fn new(
        handle: SenderService<TextMessageRequest, Result<TextMessageResponse, TextMessageError>>,
        event_stream: Subscriber<TextMessageEvent>,
    ) -> Self
    {
        Self { handle, event_stream }
    }

    pub fn get_event_stream_fused(&self) -> Fuse<Subscriber<TextMessageEvent>> {
        self.event_stream.clone().fuse()
    }

    pub async fn send_text_message(
        &mut self,
        dest_pubkey: CommsPublicKey,
        message: String,
    ) -> Result<(), TextMessageError>
    {
        match self
            .handle
            .call(TextMessageRequest::SendTextMessage((dest_pubkey, message)))
            .await??
        {
            TextMessageResponse::MessageSent => Ok(()),
            _ => Err(TextMessageError::UnexpectedApiResponse),
        }
    }

    pub async fn get_text_messages(&mut self) -> Result<TextMessages, TextMessageError> {
        match self.handle.call(TextMessageRequest::GetTextMessages).await?? {
            TextMessageResponse::TextMessages(t) => Ok(t),
            _ => Err(TextMessageError::UnexpectedApiResponse),
        }
    }

    pub async fn get_text_messages_by_pub_key(
        &mut self,
        pubkey: CommsPublicKey,
    ) -> Result<TextMessages, TextMessageError>
    {
        match self
            .handle
            .call(TextMessageRequest::GetTextMessagesByPubKey(pubkey))
            .await??
        {
            TextMessageResponse::TextMessages(t) => Ok(t),
            _ => Err(TextMessageError::UnexpectedApiResponse),
        }
    }

    pub async fn set_screen_name(&mut self, name: String) -> Result<(), TextMessageError> {
        match self.handle.call(TextMessageRequest::SetScreenName(name)).await?? {
            TextMessageResponse::ScreenNameSet => Ok(()),
            _ => Err(TextMessageError::UnexpectedApiResponse),
        }
    }

    pub async fn get_screen_name(&mut self) -> Result<Option<String>, TextMessageError> {
        match self.handle.call(TextMessageRequest::GetScreenName).await?? {
            TextMessageResponse::ScreenName(s) => Ok(s),
            _ => Err(TextMessageError::UnexpectedApiResponse),
        }
    }

    pub async fn add_contact(&mut self, contact: Contact) -> Result<(), TextMessageError> {
        match self.handle.call(TextMessageRequest::AddContact(contact)).await?? {
            TextMessageResponse::ContactAdded => Ok(()),
            _ => Err(TextMessageError::UnexpectedApiResponse),
        }
    }

    pub async fn remove_contact(&mut self, contact: Contact) -> Result<(), TextMessageError> {
        match self.handle.call(TextMessageRequest::RemoveContact(contact)).await?? {
            TextMessageResponse::ContactRemoved => Ok(()),
            _ => Err(TextMessageError::UnexpectedApiResponse),
        }
    }

    pub async fn get_contacts(&mut self) -> Result<Vec<Contact>, TextMessageError> {
        match self.handle.call(TextMessageRequest::GetContacts).await?? {
            TextMessageResponse::Contacts(v) => Ok(v),
            _ => Err(TextMessageError::UnexpectedApiResponse),
        }
    }

    pub async fn update_contact(
        &mut self,
        pubkey: CommsPublicKey,
        update_contact: UpdateContact,
    ) -> Result<(), TextMessageError>
    {
        match self
            .handle
            .call(TextMessageRequest::UpdateContact((pubkey, update_contact)))
            .await??
        {
            TextMessageResponse::ContactUpdated => Ok(()),
            _ => Err(TextMessageError::UnexpectedApiResponse),
        }
    }
}
