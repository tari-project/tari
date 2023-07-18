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

use std::time::Duration;

use cucumber::{then, when};
use tari_common::configuration::Network;
use tari_common_types::tari_address::TariAddress;
use tari_contacts::contacts_service::{
    handle::{DEFAULT_MESSAGE_LIMIT, DEFAULT_MESSAGE_PAGE},
    service::ContactOnlineStatus,
};
use tari_integration_tests::{chat_client::spawn_chat_client, TariWorld};

use crate::steps::{HALF_SECOND, TWO_MINUTES_WITH_HALF_SECOND_SLEEP};

#[when(expr = "I have a chat client {word} connected to seed node {word}")]
async fn chat_client_connected_to_base_node(world: &mut TariWorld, name: String, seed_node_name: String) {
    let base_node = world.get_node(&seed_node_name).unwrap();

    let client = spawn_chat_client(
        &name,
        vec![base_node.identity.to_peer()],
        world.current_base_dir.clone().expect("Expect a base dir on world"),
    )
    .await;

    world.chat_clients.insert(name, Box::new(client));
}

#[when(expr = "I have a chat client {word} with no peers")]
async fn chat_client_with_no_peers(world: &mut TariWorld, name: String) {
    let client = spawn_chat_client(
        &name,
        vec![],
        world.current_base_dir.clone().expect("Expect a base dir on world"),
    )
    .await;

    world.chat_clients.insert(name, Box::new(client));
}

#[when(regex = r"^I use (.+) to send a message '(.+)' to (.*)$")]
async fn send_message_to(world: &mut TariWorld, sender: String, message: String, receiver: String) {
    let sender = world.chat_clients.get(&sender).unwrap();
    let receiver = world.chat_clients.get(&receiver).unwrap();
    let address = TariAddress::from_public_key(receiver.identity().public_key(), Network::LocalNet);

    sender.send_message(address, message).await;
}

#[then(expr = "{word} will have {int} message(s) with {word}")]
async fn receive_n_messages(world: &mut TariWorld, receiver: String, message_count: u64, sender: String) {
    let receiver = world.chat_clients.get(&receiver).unwrap();
    let sender = world.chat_clients.get(&sender).unwrap();
    let address = TariAddress::from_public_key(sender.identity().public_key(), Network::LocalNet);

    let mut messages = vec![];
    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP) {
        messages = (*receiver)
            .get_messages(&address, DEFAULT_MESSAGE_LIMIT, DEFAULT_MESSAGE_PAGE)
            .await;

        if messages.len() as u64 == message_count {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "Receiver {} only received {}/{} messages",
        (*receiver).identity().node_id(),
        messages.len(),
        message_count
    )
}

#[when(expr = "{word} adds {word} as a contact")]
async fn add_as_contact(world: &mut TariWorld, sender: String, receiver: String) {
    let receiver = world.chat_clients.get(&receiver).unwrap();
    let sender = world.chat_clients.get(&sender).unwrap();

    let address = TariAddress::from_public_key(receiver.identity().public_key(), Network::LocalNet);

    sender.add_contact(&address).await;
}

#[when(expr = "{word} waits for contact {word} to be online")]
async fn wait_for_contact_to_be_online(world: &mut TariWorld, client: String, contact: String) {
    let client = world.chat_clients.get(&client).unwrap();
    let contact = world.chat_clients.get(&contact).unwrap();

    let address = TariAddress::from_public_key(contact.identity().public_key(), Network::LocalNet);
    let mut last_status = ContactOnlineStatus::Banned("No result came back".to_string());

    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP / 4) {
        last_status = client.check_online_status(&address).await;
        if ContactOnlineStatus::Online == last_status {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "Contact {} never came online, status is: {}",
        contact.identity().node_id(),
        last_status
    )
}
