//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use log::debug;
use rand::rngs::OsRng;
use tari_common_types::tari_address::TariAddress;
use tari_comms::{peer_manager::PeerFeatures, CommsNode, NodeIdentity};
use tari_contacts::contacts_service::{
    handle::ContactsServiceHandle,
    service::ContactOnlineStatus,
    types::{Message, MessageBuilder, MessageMetadata},
};
use tari_shutdown::Shutdown;

use crate::{config::ApplicationConfig, error::Error, networking, networking::Multiaddr};

const LOG_TARGET: &str = "contacts::chat_client";

#[async_trait]
pub trait ChatClient {
    async fn add_contact(&self, address: &TariAddress) -> Result<(), Error>;
    fn add_metadata(&self, message: Message, metadata_type: String, data: String) -> Message;
    async fn check_online_status(&self, address: &TariAddress) -> Result<ContactOnlineStatus, Error>;
    fn create_message(&self, receiver: &TariAddress, message: String) -> Message;
    async fn get_messages(&self, sender: &TariAddress, limit: u64, page: u64) -> Result<Vec<Message>, Error>;
    async fn send_message(&self, message: Message) -> Result<(), Error>;
    async fn send_read_receipt(&self, message: Message) -> Result<(), Error>;
    async fn get_conversationalists(&self) -> Result<Vec<TariAddress>, Error>;
    fn address(&self) -> TariAddress;
    fn shutdown(&mut self);
}

pub struct Client {
    pub config: ApplicationConfig,
    pub contacts: Option<ContactsServiceHandle>,
    pub identity: Arc<NodeIdentity>,
    pub shutdown: Shutdown,
}

impl Debug for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("config", &self.config)
            .field("identity", &self.identity)
            .field("shutdown", &self.shutdown)
            .finish()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.quit();
    }
}

impl Client {
    pub fn new(identity: Arc<NodeIdentity>, config: ApplicationConfig) -> Self {
        Self {
            config,
            contacts: None,
            identity,
            shutdown: Shutdown::new(),
        }
    }

    pub fn sideload(config: ApplicationConfig, contacts: ContactsServiceHandle) -> Self {
        // Create a placeholder ID. It won't be written or used when sideloaded.
        let identity = Arc::new(NodeIdentity::random(
            &mut OsRng,
            Multiaddr::empty(),
            PeerFeatures::COMMUNICATION_NODE,
        ));

        Self {
            config,
            contacts: Some(contacts),
            identity,
            shutdown: Shutdown::new(),
        }
    }

    pub async fn initialize(&mut self) -> Result<(), Error> {
        debug!(target: LOG_TARGET, "initializing chat");

        // Only run the networking if we're operating as a standalone client. If we're sideloading we can skip all this
        if self.contacts.is_none() {
            let signal = self.shutdown.to_signal();

            let (contacts, comms_node) = networking::start(self.identity.clone(), self.config.clone(), signal)
                .await
                .map_err(|e| Error::InitializationError(e.to_string()))?;

            if !self.config.peer_seeds.peer_seeds.is_empty() {
                loop {
                    debug!(target: LOG_TARGET, "Waiting for peer connections...");
                    match wait_for_connectivity(comms_node.clone()).await {
                        Ok(_) => break,
                        Err(e) => debug!(target: LOG_TARGET, "{}. Still waiting...", e),
                    }
                }
            }

            self.contacts = Some(contacts);
        }

        debug!(target: LOG_TARGET, "Connections established");

        Ok(())
    }

    pub fn quit(&mut self) {
        self.shutdown.trigger();
    }
}

#[async_trait]
impl ChatClient for Client {
    fn address(&self) -> TariAddress {
        TariAddress::from_public_key(self.identity.public_key(), self.config.chat_client.network)
    }

    fn shutdown(&mut self) {
        self.shutdown.trigger();
    }

    async fn add_contact(&self, address: &TariAddress) -> Result<(), Error> {
        if let Some(mut contacts_service) = self.contacts.clone() {
            contacts_service.upsert_contact(address.into()).await?;
        }

        Ok(())
    }

    async fn check_online_status(&self, address: &TariAddress) -> Result<ContactOnlineStatus, Error> {
        if let Some(mut contacts_service) = self.contacts.clone() {
            let contact = contacts_service.get_contact(address.clone()).await?;

            return Ok(contacts_service.get_contact_online_status(contact).await?);
        }

        Ok(ContactOnlineStatus::Offline)
    }

    async fn send_message(&self, message: Message) -> Result<(), Error> {
        if let Some(mut contacts_service) = self.contacts.clone() {
            contacts_service.send_message(message).await?;
        }

        Ok(())
    }

    async fn get_messages(&self, sender: &TariAddress, limit: u64, page: u64) -> Result<Vec<Message>, Error> {
        let mut messages = vec![];
        if let Some(mut contacts_service) = self.contacts.clone() {
            messages = contacts_service.get_messages(sender.clone(), limit, page).await?;
        }

        Ok(messages)
    }

    async fn send_read_receipt(&self, message: Message) -> Result<(), Error> {
        if let Some(mut contacts_service) = self.contacts.clone() {
            contacts_service
                .send_read_confirmation(message.address.clone(), message.message_id)
                .await?;
        }

        Ok(())
    }

    fn create_message(&self, receiver: &TariAddress, message: String) -> Message {
        MessageBuilder::new().address(receiver.clone()).message(message).build()
    }

    fn add_metadata(&self, mut message: Message, key: String, data: String) -> Message {
        let metadata = MessageMetadata {
            key: key.into_bytes(),
            data: data.into_bytes(),
        };

        message.push(metadata);
        message
    }

    async fn get_conversationalists(&self) -> Result<Vec<TariAddress>, Error> {
        let mut addresses = vec![];
        if let Some(mut contacts_service) = self.contacts.clone() {
            addresses = contacts_service.get_conversationalists().await?;
        }

        Ok(addresses)
    }
}

pub async fn wait_for_connectivity(comms: CommsNode) -> anyhow::Result<()> {
    comms
        .connectivity()
        .wait_for_connectivity(Duration::from_secs(30))
        .await?;
    Ok(())
}
