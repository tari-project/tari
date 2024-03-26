//   Copyright 2022. The Tari Project
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
    ffi::CString,
    ptr::null_mut,
    sync::{Arc, Mutex},
};

use callbacks::Callbacks;
use indexmap::IndexMap;
use libc::{c_ulonglong, c_void};

use super::{
    ffi_import::{
        self,
        wallet_create,
        TariBalance,
        TariCompletedTransaction,
        TariContactsLivenessData,
        TariPendingInboundTransaction,
        TariTransactionSendStatus,
        TariWallet,
    },
    Balance,
    CommsConfig,
    CompletedTransactions,
    Contact,
    Contacts,
    ContactsLivenessData,
    FeePerGramStats,
    PendingInboundTransactions,
    PendingOutboundTransactions,
    PublicKey,
    PublicKeys,
    WalletAddress,
};
use crate::ffi::{callbacks, ffi_import::TariBaseNodeState};

extern "C" fn callback_received_transaction(ptr: *mut TariPendingInboundTransaction) {
    let callbacks = Callbacks::instance();
    callbacks.on_received_transaction(ptr);
    // println!("callback_received_transaction");
}
extern "C" fn callback_received_transaction_reply(ptr: *mut TariCompletedTransaction) {
    let callbacks = Callbacks::instance();
    callbacks.on_received_transaction_reply(ptr);
    // println!("callback_received_transaction_reply");
}
extern "C" fn callback_received_finalized_transaction(ptr: *mut TariCompletedTransaction) {
    let callbacks = Callbacks::instance();
    callbacks.on_received_finalized_transaction(ptr);
    // println!("callback_received_finalized_transaction");
}
extern "C" fn callback_transaction_broadcast(ptr: *mut TariCompletedTransaction) {
    let callbacks = Callbacks::instance();
    callbacks.on_transaction_broadcast(ptr);
    // println!("callback_transaction_broadcast");
}
extern "C" fn callback_transaction_mined(ptr: *mut TariCompletedTransaction) {
    let callbacks = Callbacks::instance();
    callbacks.on_transaction_mined(ptr);
    // println!("callback_transaction_mined");
}
extern "C" fn callback_transaction_mined_unconfirmed(ptr: *mut TariCompletedTransaction, confirmations: u64) {
    let callbacks = Callbacks::instance();
    callbacks.on_transaction_mined_unconfirmed(ptr, confirmations);
    // println!("callback_transaction_mined_unconfirmed");
}
extern "C" fn callback_faux_transaction_confirmed(ptr: *mut TariCompletedTransaction) {
    let callbacks = Callbacks::instance();
    callbacks.on_faux_transaction_confirmed(ptr);
    // println!("callback_faux_transaction_confirmed");
}
extern "C" fn callback_faux_transaction_unconfirmed(ptr: *mut TariCompletedTransaction, confirmations: u64) {
    let callbacks = Callbacks::instance();
    callbacks.on_faux_transaction_mined_unconfirmed(ptr, confirmations);
    // println!("callback_faux_transaction_unconfirmed");
}
extern "C" fn callback_transaction_send_result(tx_id: c_ulonglong, ptr: *mut TariTransactionSendStatus) {
    let callbacks = Callbacks::instance();
    callbacks.on_transaction_send_result(tx_id, ptr);
    // println!("callback_transaction_send_result");
}
extern "C" fn callback_transaction_cancellation(ptr: *mut TariCompletedTransaction, reason: u64) {
    let callbacks = Callbacks::instance();
    callbacks.on_transaction_cancellation(ptr, reason);
    // println!("callback_transaction_cancellation");
}
extern "C" fn callback_txo_validation_complete(request_key: u64, validation_results: u64) {
    let callbacks = Callbacks::instance();
    callbacks.on_txo_validation_complete(request_key, validation_results);
    // println!("callback_txo_validation_complete");
}
extern "C" fn callback_contacts_liveness_data_updated(ptr: *mut TariContactsLivenessData) {
    let callbacks = Callbacks::instance();
    callbacks.on_contacts_liveness_data_updated(ptr);
    // println!("callback_contacts_liveness_data_updated");
}
extern "C" fn callback_balance_updated(ptr: *mut TariBalance) {
    let callbacks = Callbacks::instance();
    callbacks.on_balance_updated(ptr);
    // println!("callback_balance_updated");
}
extern "C" fn callback_transaction_validation_complete(request_key: u64, validation_results: u64) {
    let callbacks = Callbacks::instance();
    callbacks.on_transaction_validation_complete(request_key, validation_results);
    // println!("callback_transaction_validation_complete");
}
extern "C" fn callback_saf_messages_received() {
    let callbacks = Callbacks::instance();
    callbacks.on_saf_messages_received();
    // println!("callback_saf_messages_received");
}
extern "C" fn callback_connectivity_status(status: u64) {
    let callbacks = Callbacks::instance();
    callbacks.on_connectivity_status(status);
    // println!("callback_connectivity_status");
}
extern "C" fn callback_base_node_state(state: *mut TariBaseNodeState) {
    let callbacks = Callbacks::instance();
    callbacks.on_basenode_state_update(state);
}

#[derive(Default, Debug)]
struct CachedBalance {
    available: u64,
    time_locked: u64,
    pending_incoming: u64,
    pending_outgoing: u64,
}

#[derive(Debug)]
pub struct Wallet {
    ptr: *mut TariWallet,
    liveness_data: Arc<Mutex<IndexMap<String, ContactsLivenessData>>>,
    balance: CachedBalance,
}

impl Drop for Wallet {
    fn drop(&mut self) {
        self.destroy();
    }
}

impl Wallet {
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn create(comms_config: CommsConfig, log_path: String, seed_words_ptr: *const c_void) -> Arc<Mutex<Self>> {
        let mut recovery_in_progress: bool = false;
        let mut error = 0;
        let ptr;
        unsafe {
            ptr = wallet_create(
                comms_config.get_ptr(),
                CString::new(log_path).unwrap().into_raw(),
                11,
                50,
                104857600, // 100 MB
                CString::new("kensentme").unwrap().into_raw(),
                seed_words_ptr,
                CString::new("localnet").unwrap().into_raw(),
                CString::new("").unwrap().into_raw(),
                false,
                callback_received_transaction,
                callback_received_transaction_reply,
                callback_received_finalized_transaction,
                callback_transaction_broadcast,
                callback_transaction_mined,
                callback_transaction_mined_unconfirmed,
                callback_faux_transaction_confirmed,
                callback_faux_transaction_unconfirmed,
                callback_transaction_send_result,
                callback_transaction_cancellation,
                callback_txo_validation_complete,
                callback_contacts_liveness_data_updated,
                callback_balance_updated,
                callback_transaction_validation_complete,
                callback_saf_messages_received,
                callback_connectivity_status,
                callback_base_node_state,
                &mut recovery_in_progress,
                &mut error,
            );
            if error > 0 {
                println!("wallet_create error {}", error);
            }
        }
        #[allow(clippy::arc_with_non_send_sync)]
        let wallet = Arc::new(Mutex::new(Self {
            ptr,
            liveness_data: Default::default(),
            balance: Default::default(),
        }));
        let callbacks = Callbacks::instance();
        callbacks.reset(wallet.clone());
        wallet
    }

    pub fn add_liveness_data(&mut self, contact_liveness_data: ContactsLivenessData) {
        self.liveness_data.lock().unwrap().insert(
            contact_liveness_data.get_public_key().address().get_as_hex(),
            contact_liveness_data,
        );
    }

    pub fn set_balance(&mut self, balance: Balance) {
        self.balance.available = balance.get_available();
        self.balance.pending_incoming = balance.get_pending_incoming();
        self.balance.pending_outgoing = balance.get_pending_outgoing();
        self.balance.time_locked = balance.get_time_locked();
    }

    pub fn destroy(&mut self) {
        unsafe { ffi_import::wallet_destroy(self.ptr) };
        self.ptr = null_mut();
    }

    pub fn add_base_node_peer(&self, base_node: PublicKey, address: String) -> bool {
        let mut error = 0;
        let success;
        unsafe {
            success = ffi_import::wallet_set_base_node_peer(
                self.ptr,
                base_node.get_ptr(),
                CString::new(address).unwrap().into_raw(),
                &mut error,
            );
            if error > 0 {
                println!("wallet_set_base_node_peer error {}", error);
            }
        }
        success
    }

    pub fn get_address(&self) -> WalletAddress {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::wallet_get_tari_address(self.ptr, &mut error);
            if error > 0 {
                println!("wallet_get_tari_address error {}", error);
            }
        }
        WalletAddress::from_ptr(ptr)
    }

    pub fn connected_public_keys(&self) -> PublicKeys {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::comms_list_connected_public_keys(self.ptr, &mut error);
        }
        PublicKeys::from_ptr(ptr)
    }

    pub fn upsert_contact(&self, contact: Contact) -> bool {
        let success;
        let mut error = 0;
        unsafe {
            success = ffi_import::wallet_upsert_contact(self.ptr, contact.get_ptr(), &mut error);
            if error > 0 {
                println!("wallet_upsert_contact error {}", error);
            }
        }
        success
    }

    pub fn get_contacts(&self) -> Contacts {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::wallet_get_contacts(self.ptr, &mut error);
            if error > 0 {
                println!("wallet_get_contacts error {}", error);
            }
        }
        Contacts::from_ptr(ptr)
    }

    pub fn remove_contact(&self, contact: Contact) -> bool {
        let success;
        let mut error = 0;
        unsafe {
            success = ffi_import::wallet_remove_contact(self.ptr, contact.get_ptr(), &mut error);
            if error > 0 {
                println!("wallet_remove_contact error {}", error);
            }
        }
        success
    }

    pub fn get_balance(&self) -> Balance {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::wallet_get_balance(self.ptr, &mut error);
            if error > 0 {
                println!("wallet_get_balance error {}", error);
            }
        }
        Balance::from_ptr(ptr)
    }

    pub fn send_transaction(
        &self,
        dest: String,
        amount: u64,
        fee_per_gram: u64,
        message: String,
        one_sided: bool,
    ) -> u64 {
        let tx_id;
        let mut error = 0;
        unsafe {
            tx_id = ffi_import::wallet_send_transaction(
                self.ptr,
                WalletAddress::from_hex(dest).get_ptr(),
                amount,
                null_mut(),
                fee_per_gram,
                CString::new(message).unwrap().into_raw(),
                one_sided,
                &mut error,
            );
            if error > 0 {
                println!("wallet_send_transaction error {}", error);
            }
        }
        tx_id
    }

    pub fn get_pending_outbound_transactions(&self) -> PendingOutboundTransactions {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::wallet_get_pending_outbound_transactions(self.ptr, &mut error);
            if error > 0 {
                println!("wallet_get_pending_outbound_transactions error {}", error);
            }
        }
        PendingOutboundTransactions::from_ptr(ptr)
    }

    pub fn get_pending_inbound_transactions(&self) -> PendingInboundTransactions {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::wallet_get_pending_inbound_transactions(self.ptr, &mut error);
            if error > 0 {
                println!("wallet_get_pending_inbound_transactions error {}", error);
            }
        }
        PendingInboundTransactions::from_ptr(ptr)
    }

    pub fn get_completed_transactions(&self) -> CompletedTransactions {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::wallet_get_completed_transactions(self.ptr, &mut error);
            if error > 0 {
                println!("wallet_get_completed_transactions error {}", error);
            }
        }
        CompletedTransactions::from_ptr(ptr)
    }

    pub fn cancel_pending_transaction(&self, transaction_id: u64) -> bool {
        let cancelled;
        let mut error = 0;
        unsafe {
            cancelled = ffi_import::wallet_cancel_pending_transaction(self.ptr, transaction_id, &mut error);
            if error > 0 {
                println!("wallet_cancel_pending_transaction error {}", error);
            }
        }
        cancelled
    }

    pub fn start_txo_validation(&self) -> u64 {
        let request_key;
        let mut error = 0;
        unsafe {
            request_key = ffi_import::wallet_start_txo_validation(self.ptr, &mut error);
            if error > 0 {
                println!("wallet_start_txo_validation error {}", error);
            }
        }
        request_key
    }

    pub fn start_transaction_validation(&self) -> u64 {
        let request_key;
        let mut error = 0;
        unsafe {
            request_key = ffi_import::wallet_start_transaction_validation(self.ptr, &mut error);
            if error > 0 {
                println!("wallet_start_transaction_validation error {}", error);
            }
        }
        request_key
    }

    pub fn get_liveness_data(&self) -> Arc<Mutex<IndexMap<String, ContactsLivenessData>>> {
        self.liveness_data.clone()
    }

    #[allow(dead_code)]
    pub fn get_fee_per_gram_stats(&self, count: u32) -> FeePerGramStats {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::wallet_get_fee_per_gram_stats(self.ptr, count, &mut error);
            if error > 0 {
                println!("wallet_get_fee_per_gram_stats error {}", error);
            }
        }
        FeePerGramStats::from_ptr(ptr)
    }

    pub fn contacts_handle(&self) -> *mut c_void {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::contacts_handle(self.ptr, &mut error);
            if error > 0 {
                println!("contacts_handle error {}", error);
            }
        }
        ptr
    }
}
