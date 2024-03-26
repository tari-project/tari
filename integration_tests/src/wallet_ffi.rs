//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    ffi::CString,
    path::PathBuf,
    ptr::null,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use libc::c_void;

use super::ffi::{
    Balance,
    Callbacks,
    CompletedTransactions,
    Contact,
    Contacts,
    ContactsLivenessData,
    FeePerGramStats,
    PendingInboundTransactions,
    PendingOutboundTransactions,
    PublicKeys,
    WalletAddress,
};
use crate::{
    ffi::{self},
    get_port,
    TariWorld,
};

#[derive(Debug)]
pub struct WalletFFI {
    pub name: String,
    pub port: u64,
    pub base_dir: PathBuf,
    pub wallet: Arc<Mutex<ffi::Wallet>>,
}

impl WalletFFI {
    fn spawn(name: String, seed_words_ptr: *const c_void, base_dir: PathBuf) -> Self {
        let port = get_port(18000..18499).unwrap();
        let transport_config =
            ffi::TransportConfig::create_tcp(CString::new(format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap().into_raw());
        let base_dir_path = base_dir.join("ffi_wallets").join(format!("{}_port_{}", name, port));
        let base_dir: String = base_dir_path.as_os_str().to_str().unwrap().into();
        let comms_config = ffi::CommsConfig::create(port, transport_config, base_dir);
        let log_path = base_dir_path
            .join("logs")
            .join("ffi_wallet.log")
            .as_os_str()
            .to_str()
            .unwrap()
            .into();
        let wallet = ffi::Wallet::create(comms_config, log_path, seed_words_ptr);
        Self {
            name,
            port,
            base_dir: base_dir_path,
            wallet,
        }
    }

    pub fn identify(&self) -> String {
        let tari_address = self.get_address();
        let key = tari_address.address();
        key.get_as_hex()
    }

    pub fn get_emoji_id(&self) -> String {
        let tari_address = self.get_address();
        let emoji_id = tari_address.emoji_id();
        emoji_id.as_string()
    }

    pub fn add_base_node(&self, public_key: String, address: String) {
        let node_public_key = ffi::PublicKey::from_hex(public_key);
        self.wallet.lock().unwrap().add_base_node_peer(node_public_key, address);
    }

    pub fn destroy(&mut self) {
        self.wallet.lock().unwrap().destroy();
    }

    pub fn get_address(&self) -> WalletAddress {
        self.wallet.lock().unwrap().get_address()
    }

    pub fn connected_public_keys(&self) -> PublicKeys {
        self.wallet.lock().unwrap().connected_public_keys()
    }

    pub fn get_balance(&self) -> Balance {
        self.wallet.lock().unwrap().get_balance()
    }

    pub fn upsert_contact(&self, contact: Contact) -> bool {
        self.wallet.lock().unwrap().upsert_contact(contact)
    }

    pub fn get_contacts(&self) -> Contacts {
        self.wallet.lock().unwrap().get_contacts()
    }

    pub fn remove_contact(&self, contact_to_remove: Contact) -> bool {
        self.wallet.lock().unwrap().remove_contact(contact_to_remove)
    }

    pub fn get_pending_inbound_transactions(&self) -> PendingInboundTransactions {
        self.wallet.lock().unwrap().get_pending_inbound_transactions()
    }

    pub fn get_pending_outbound_transactions(&self) -> PendingOutboundTransactions {
        self.wallet.lock().unwrap().get_pending_outbound_transactions()
    }

    pub fn get_completed_transactions(&self) -> CompletedTransactions {
        self.wallet.lock().unwrap().get_completed_transactions()
    }

    pub fn cancel_pending_transaction(&self, transaction_id: u64) -> bool {
        self.wallet.lock().unwrap().cancel_pending_transaction(transaction_id)
    }

    pub fn get_counters(&self) -> &mut Callbacks {
        Callbacks::instance()
    }

    pub fn start_txo_validation(&self) -> u64 {
        self.wallet.lock().unwrap().start_txo_validation()
    }

    pub fn start_transaction_validation(&self) -> u64 {
        self.wallet.lock().unwrap().start_transaction_validation()
    }

    pub fn get_liveness_data(&self) -> Arc<Mutex<IndexMap<String, ContactsLivenessData>>> {
        self.wallet.lock().unwrap().get_liveness_data()
    }

    pub fn send_transaction(
        &self,
        dest: String,
        amount: u64,
        fee_per_gram: u64,
        message: String,
        one_sided: bool,
    ) -> u64 {
        self.wallet
            .lock()
            .unwrap()
            .send_transaction(dest, amount, fee_per_gram, message, one_sided)
    }

    pub fn restart(&mut self) {
        self.wallet.lock().unwrap().destroy();
        let port = get_port(18000..18499).unwrap();
        let transport_config =
            ffi::TransportConfig::create_tcp(CString::new(format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap().into_raw());
        let now: DateTime<Utc> = SystemTime::now().into();
        let base_dir = format!("./log/ffi_wallets/{}", now.format("%Y%m%d-%H%M%S"));
        let comms_config = ffi::CommsConfig::create(port, transport_config, base_dir.clone());
        let log_path = format!("{}/log/ffi_wallet.log", base_dir);
        self.wallet = ffi::Wallet::create(comms_config, log_path, null());
    }

    pub fn get_fee_per_gram_stats(&self, count: u32) -> FeePerGramStats {
        self.wallet.lock().unwrap().get_fee_per_gram_stats(count)
    }

    pub fn contacts_handle(&self) -> *mut c_void {
        self.wallet.lock().unwrap().contacts_handle()
    }
}

pub fn spawn_wallet_ffi(world: &mut TariWorld, wallet_name: String, seed_words_ptr: *const c_void) {
    let wallet_ffi = WalletFFI::spawn(
        wallet_name.clone(),
        seed_words_ptr,
        world.current_base_dir.clone().expect("Base dir on world"),
    );
    world.ffi_wallets.insert(wallet_name, wallet_ffi);
}

pub fn get_mnemonic_word_list_for_language(language: String) -> ffi::SeedWords {
    let language = match language.as_str() {
        "CHINESE_SIMPLIFIED" => "ChineseSimplified",
        "ENGLISH" => "English",
        "FRENCH" => "French",
        "ITALIAN" => "Italian",
        "JAPANESE" => "Japanese",
        "KOREAN" => "Korean",
        "SPANISH" => "Spanish",
        _ => panic!("Unknown language {}", language),
    };
    ffi::SeedWords::get_mnemonic_word_list_for_language(language.to_string())
}

pub fn create_contact(alias: String, address: String) -> ffi::Contact {
    ffi::Contact::create(alias, address)
}

pub fn create_seed_words(words: Vec<&str>) -> ffi::SeedWords {
    let seed_words = ffi::SeedWords::create();
    for word in words {
        seed_words.push_word(word.to_string());
    }
    seed_words
}
