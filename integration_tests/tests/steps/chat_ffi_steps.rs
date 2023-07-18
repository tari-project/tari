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
use tari_integration_tests::{
    chat_ffi::{spawn_ffi_chat_client, ChatCallback},
    TariWorld,
};

use crate::steps::{HALF_SECOND, TWO_MINUTES_WITH_HALF_SECOND_SLEEP};

#[when(expr = "I have a chat FFI client {word} connected to seed node {word}")]
async fn chat_ffi_client_connected_to_base_node(world: &mut TariWorld, name: String, seed_node_name: String) {
    let base_node = world.get_node(&seed_node_name).unwrap();

    let client = spawn_ffi_chat_client(
        &name,
        vec![base_node.identity.to_peer()],
        world.current_base_dir.clone().expect("Base dir on world"),
    )
    .await;
    world.chat_clients.insert(name, Box::new(client));
}

#[then(expr = "there will be a contact status update callback of at least {int}")]
async fn contact_status_update_callback(_world: &mut TariWorld, callback_count: usize) {
    let mut count = 0;
    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP) {
        count = *ChatCallback::instance().contact_status_change.lock().unwrap();

        if count >= callback_count as u64 {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "contact status update never received. Callbacks expected: {}, Callbacks received: {:?}",
        callback_count, count
    );
}

#[then(expr = "there will be a MessageReceived callback of at least {int}")]
async fn message_reveived_callback(_world: &mut TariWorld, callback_count: usize) {
    let mut count = 0;
    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP) {
        count = *ChatCallback::instance().message_received.lock().unwrap();

        if count >= callback_count as u64 {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "contact status update never received. Callbacks expected: {}, Callbacks received: {:?}",
        callback_count, count
    );
}
