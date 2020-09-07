// Copyright 2020. The Tari Project
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

use rand::rngs::OsRng;
use std::sync::atomic::{AtomicU64, Ordering};
use tari_core::transactions::types::PublicKey;
use tari_crypto::keys::PublicKey as PublicKeyTrait;
use tari_wallet::contacts_service::storage::database::Contact;

lazy_static! {
    static ref BN_SYNC_CALLS: AtomicU64 = AtomicU64::new(0);
}

pub fn get_dummy_base_node_status() -> Option<u64> {
    let seconds = BN_SYNC_CALLS.fetch_add(1, Ordering::SeqCst) / 4;

    if seconds / 6 % 2 == 0 {
        None
    } else {
        Some(123456 + seconds / 10)
    }
}

pub fn get_dummy_contacts() -> Vec<Contact> {
    let mut contacts = Vec::new();
    let names = [
        "Alice".to_string(),
        "Bob".to_string(),
        "Carol".to_string(),
        "Dave".to_string(),
        "Elvis".to_string(),
    ];
    for n in names.iter() {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        contacts.push(Contact {
            alias: n.clone(),
            public_key,
        });
    }
    contacts
}
