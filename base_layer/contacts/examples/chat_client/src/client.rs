//   Copyright 2023. The Taiji Project
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
use taiji_common_types::taiji_address::TaijiAddress;
use taiji_comms::{CommsNode, NodeIdentity};
use taiji_contacts::contacts_service::{
    handle::ContactsServiceHandle,
    service::ContactOnlineStatus,
    types::{Message, MessageBuilder},
};
use taiji_shutdown::Shutdown;

use crate::{config::ApplicationConfig, networking};

const LOG_TARGET: &str = "contacts::chat_client";

#[async_trait]
pub trait ChatClient {
    async fn add_contact(&self, address: &TaijiAddress);
    async fn check_online_status(&self, address: &TaijiAddress) -> ContactOnlineStatus;
    async fn send_message(&self, receiver: TaijiAddress, message: String);
    async fn get_messages(&self, sender: &TaijiAddress, limit: u64, page: u64) -> Vec<Message>;
    fn identity(&self) -> &NodeIdentity;
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

    pub async fn initialize(&mut self) {
        debug!(target: LOG_TARGET, "initializing chat");

        let signal = self.shutdown.to_signal();

        let (contacts, comms_node) = networking::start(self.identity.clone(), self.config.clone(), signal)
            .await
            .unwrap();

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

        debug!(target: LOG_TARGET, "Connections established")
    }

    pub fn quit(&mut self) {
        self.shutdown.trigger();
    }
}

#[async_trait]
impl ChatClient for Client {
    fn identity(&self) -> &NodeIdentity {
        &self.identity
    }

    fn shutdown(&mut self) {
        self.shutdown.trigger();
    }

    async fn add_contact(&self, address: &TaijiAddress) {
        if let Some(mut contacts_service) = self.contacts.clone() {
            contacts_service
                .upsert_contact(address.into())
                .await
                .expect("Contact wasn't added");
        }
    }

    async fn check_online_status(&self, address: &TaijiAddress) -> ContactOnlineStatus {
        if let Some(mut contacts_service) = self.contacts.clone() {
            let contact = contacts_service
                .get_contact(address.clone())
                .await
                .expect("Client does not have contact");

            return contacts_service
                .get_contact_online_status(contact)
                .await
                .expect("Failed to get status");
        }

        ContactOnlineStatus::Offline
    }

    async fn send_message(&self, receiver: TaijiAddress, message: String) {
        if let Some(mut contacts_service) = self.contacts.clone() {
            contacts_service
                .send_message(MessageBuilder::new().message(message).address(receiver).build())
                .await
                .expect("Message wasn't sent");
        }
    }

    async fn get_messages(&self, sender: &TaijiAddress, limit: u64, page: u64) -> Vec<Message> {
        let mut messages = vec![];
        if let Some(mut contacts_service) = self.contacts.clone() {
            messages = contacts_service
                .get_messages(sender.clone(), limit, page)
                .await
                .expect("Messages not fetched");
        }

        messages
    }
}

pub async fn wait_for_connectivity(comms: CommsNode) -> anyhow::Result<()> {
    comms
        .connectivity()
        .wait_for_connectivity(Duration::from_secs(30))
        .await?;
    Ok(())
}
