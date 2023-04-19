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
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use log::debug;
use tari_common_types::tari_address::TariAddress;
use tari_comms::{peer_manager::Peer, CommsNode, NodeIdentity};
use tari_contacts::contacts_service::{
    handle::ContactsServiceHandle,
    service::ContactOnlineStatus,
    types::{Message, MessageBuilder},
};
use tari_p2p::{Network, P2pConfig};
use tari_shutdown::Shutdown;
use tokio::runtime::Runtime;

use crate::networking;

const LOG_TARGET: &str = "contacts::chat_client";

trait ChatClient {
    fn add_contact(&self, address: &TariAddress);
    fn check_online_status(&self, address: &TariAddress) -> ContactOnlineStatus;
    fn send_message(&self, receiver: TariAddress, message: String);
    fn get_all_messages(&self, sender: &TariAddress) -> Vec<Message>;
}

pub struct Client {
    pub config: P2pConfig,
    pub contacts: Option<ContactsServiceHandle>,
    pub db_path: PathBuf,
    pub identity: Arc<NodeIdentity>,
    pub network: Network,
    pub seed_peers: Vec<Peer>,
    pub shutdown: Shutdown,
}

impl Debug for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("config", &self.config)
            .field("db_path", &self.db_path)
            .field("identity", &self.identity)
            .field("network", &self.network)
            .field("seed_peers", &self.seed_peers)
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
    pub fn new(
        identity: NodeIdentity,
        config: P2pConfig,
        seed_peers: Vec<Peer>,
        db_path: PathBuf,
        network: Network,
    ) -> Self {
        Self {
            config,
            contacts: None,
            db_path,
            identity: Arc::new(identity),
            network,
            seed_peers,
            shutdown: Shutdown::new(),
        }
    }

    pub async fn add_contact(&self, address: &TariAddress) {
        if let Some(mut contacts_service) = self.contacts.clone() {
            contacts_service
                .upsert_contact(address.into())
                .await
                .expect("Contact wasn't added");
        }
    }

    pub async fn check_online_status(&self, address: &TariAddress) -> ContactOnlineStatus {
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

    pub async fn send_message(&self, receiver: TariAddress, message: String) {
        if let Some(mut contacts_service) = self.contacts.clone() {
            contacts_service
                .send_message(MessageBuilder::new().message(message).address(receiver).build())
                .await
                .expect("Message wasn't sent");
        }
    }

    pub async fn get_all_messages(&self, sender: &TariAddress) -> Vec<Message> {
        let mut messages = vec![];
        if let Some(mut contacts_service) = self.contacts.clone() {
            messages = contacts_service
                .get_all_messages(sender.clone())
                .await
                .expect("Messages not fetched");
        }

        messages
    }

    pub async fn initialize(&mut self) {
        debug!(target: LOG_TARGET, "initializing chat");

        let signal = self.shutdown.to_signal();

        let (contacts, comms_node) = networking::start(
            self.identity.clone(),
            self.config.clone(),
            self.seed_peers.clone(),
            self.network,
            self.db_path.clone(),
            signal,
        )
        .await
        .unwrap();

        if !self.seed_peers.is_empty() {
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

pub async fn wait_for_connectivity(comms: CommsNode) -> anyhow::Result<()> {
    comms
        .connectivity()
        .wait_for_connectivity(Duration::from_secs(30))
        .await?;
    Ok(())
}
