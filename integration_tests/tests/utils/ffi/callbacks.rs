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

use std::sync::{Arc, Mutex, Once};

use libc::c_void;

use super::{Balance, CompletedTransaction, ContactsLivenessData, PendingInboundTransaction, Wallet};
use crate::utils::ffi::TransactionSendStatus;

#[derive(Debug, Default)]
pub struct Callbacks {
    transaction_received: Mutex<u64>,
    transaction_reply_received: Mutex<u64>,
    transaction_finalized: Mutex<u64>,
    transaction_broadcast: Mutex<u64>,
    transaction_mined: Mutex<u64>,
    transaction_mined_unconfirmed: Mutex<u64>,
    transaction_faux_confirmed: Mutex<u64>,
    transaction_faux_unconfirmed: Mutex<u64>,
    transaction_cancelled: Mutex<u64>,
    txo_validation_complete: Mutex<bool>,
    txo_validation_result: Mutex<u64>,
    tx_validation_complete: Mutex<bool>,
    tx_validation_result: Mutex<u64>,
    transaction_saf_message_received: Mutex<u64>,
    contacts_liveness_data_updated: Mutex<u64>,
    basenode_state_updated: Mutex<u64>,
    pub wallet: Option<Arc<Mutex<Wallet>>>,
}

static mut INSTANCE: Option<Callbacks> = None;
static START: Once = Once::new();

impl Callbacks {
    pub fn get_transaction_received(&self) -> u64 {
        *self.transaction_received.lock().unwrap()
    }

    #[allow(dead_code)]
    pub fn get_transaction_reply_received(&self) -> u64 {
        *self.transaction_reply_received.lock().unwrap()
    }

    pub fn get_transaction_finalized(&self) -> u64 {
        *self.transaction_finalized.lock().unwrap()
    }

    pub fn get_transaction_broadcast(&self) -> u64 {
        *self.transaction_broadcast.lock().unwrap()
    }

    pub fn get_transaction_mined(&self) -> u64 {
        *self.transaction_mined.lock().unwrap()
    }

    #[allow(dead_code)]
    pub fn get_transaction_mined_unconfirmed(&self) -> u64 {
        *self.transaction_mined_unconfirmed.lock().unwrap()
    }

    pub fn get_transaction_faux_confirmed(&self) -> u64 {
        *self.transaction_faux_confirmed.lock().unwrap()
    }

    pub fn get_transaction_faux_unconfirmed(&self) -> u64 {
        *self.transaction_faux_unconfirmed.lock().unwrap()
    }

    #[allow(dead_code)]
    pub fn get_transaction_cancelled(&self) -> u64 {
        *self.transaction_cancelled.lock().unwrap()
    }

    pub fn get_txo_validation_complete(&self) -> bool {
        *self.txo_validation_complete.lock().unwrap()
    }

    #[allow(dead_code)]
    pub fn get_txo_validation_result(&self) -> u64 {
        *self.txo_validation_result.lock().unwrap()
    }

    pub fn get_tx_validation_complete(&self) -> bool {
        *self.tx_validation_complete.lock().unwrap()
    }

    #[allow(dead_code)]
    pub fn get_tx_validation_result(&self) -> u64 {
        *self.tx_validation_result.lock().unwrap()
    }

    #[allow(dead_code)]
    pub fn get_transaction_saf_message_received(&self) -> u64 {
        *self.transaction_saf_message_received.lock().unwrap()
    }

    #[allow(dead_code)]
    pub fn get_contacts_liveness_data_updated(&self) -> u64 {
        *self.contacts_liveness_data_updated.lock().unwrap()
    }

    pub fn on_received_transaction(&mut self, ptr: *mut c_void) {
        let pending_inbound_transaction = PendingInboundTransaction::from_ptr(ptr);
        println!(
            "{} received Transaction with txID {}.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            pending_inbound_transaction.get_transaction_id()
        );
        *self.transaction_received.lock().unwrap() += 1;
    }

    pub fn on_received_transaction_reply(&mut self, ptr: *mut c_void) {
        let completed_transaction = CompletedTransaction::from_ptr(ptr);
        println!(
            "{} received reply for Transaction with txID {}.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            completed_transaction.get_transaction_id()
        );
        *self.transaction_reply_received.lock().unwrap() += 1;
    }

    pub fn on_received_finalized_transaction(&mut self, ptr: *mut c_void) {
        let completed_transaction = CompletedTransaction::from_ptr(ptr);
        println!(
            "{} received finalization for Transaction with txID {}.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            completed_transaction.get_transaction_id()
        );
        *self.transaction_finalized.lock().unwrap() += 1;
    }

    pub fn on_transaction_broadcast(&mut self, ptr: *mut c_void) {
        let completed_transaction = CompletedTransaction::from_ptr(ptr);
        println!(
            "{} Transaction with txID {} was broadcast.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            completed_transaction.get_transaction_id()
        );
        *self.transaction_broadcast.lock().unwrap() += 1;
    }

    pub fn on_transaction_mined(&mut self, ptr: *mut c_void) {
        let completed_transaction = CompletedTransaction::from_ptr(ptr);
        println!(
            "{} Transaction with txID {} was mined.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            completed_transaction.get_transaction_id()
        );
        *self.transaction_mined.lock().unwrap() += 1;
    }

    pub fn on_transaction_mined_unconfirmed(&mut self, ptr: *mut c_void, confirmations: u64) {
        let completed_transaction = CompletedTransaction::from_ptr(ptr);
        println!(
            "{} Transaction with txID {} is mined unconfirmed with {} confirmations.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            completed_transaction.get_transaction_id(),
            confirmations
        );
        *self.transaction_mined_unconfirmed.lock().unwrap() += 1;
    }

    pub fn on_faux_transaction_confirmed(&mut self, ptr: *mut c_void) {
        let completed_transaction = CompletedTransaction::from_ptr(ptr);
        println!(
            "{} Faux transaction with txID {} was confirmed.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            completed_transaction.get_transaction_id(),
        );
        *self.transaction_faux_confirmed.lock().unwrap() += 1;
    }

    pub fn on_faux_transaction_mined_unconfirmed(&mut self, ptr: *mut c_void, confirmations: u64) {
        let completed_transaction = CompletedTransaction::from_ptr(ptr);
        println!(
            "{} Faux transaction with txID {} is mined unconfirmed with {} confirmations.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            completed_transaction.get_transaction_id(),
            confirmations
        );
        *self.transaction_faux_unconfirmed.lock().unwrap() += 1;
    }

    pub fn on_transaction_send_result(&mut self, tx_id: u64, ptr: *mut c_void) {
        let transaction_send_status = TransactionSendStatus::from_ptr(ptr);
        println!(
            "{} callbackTransactionSendResult ({}: ({}))",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            tx_id,
            transaction_send_status.send_status_decode()
        );
    }

    pub fn on_transaction_cancellation(&mut self, ptr: *mut c_void, reason: u64) {
        let completed_transaction = CompletedTransaction::from_ptr(ptr);
        println!(
            "{} transaction with txID {} was cancelled with reason code {}.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            completed_transaction.get_transaction_id(),
            reason,
        );
        *self.transaction_cancelled.lock().unwrap() += 1;
    }

    pub fn on_txo_validation_complete(&mut self, request_key: u64, validation_results: u64) {
        println!(
            "{} callbackTxoValidationComplete({}, {}).",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            request_key,
            validation_results
        );

        *self.txo_validation_complete.lock().unwrap() = true;
        *self.txo_validation_result.lock().unwrap() = validation_results;
    }

    pub fn on_contacts_liveness_data_updated(&mut self, ptr: *mut c_void) {
        let contact_liveness_data = ContactsLivenessData::from_ptr(ptr);
        println!(
            "{} callbackContactsLivenessUpdated: received {} from contact {} with latency {} at {} and is {}.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            contact_liveness_data.get_message_type(),
            contact_liveness_data.get_public_key().address().get_as_hex(),
            contact_liveness_data.get_latency(),
            contact_liveness_data.get_last_seen(),
            contact_liveness_data.get_online_status()
        );
        self.wallet
            .as_mut()
            .unwrap()
            .lock()
            .unwrap()
            .add_liveness_data(contact_liveness_data);
        *self.contacts_liveness_data_updated.lock().unwrap() += 1;
    }

    pub fn on_balance_updated(&mut self, ptr: *mut c_void) {
        let balance = Balance::from_ptr(ptr);
        println!(
            "{} callbackBalanceUpdated: available = {}, time locked = {}, pending incoming = {}, pending outgoing = \
             {}.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            balance.get_available(),
            balance.get_time_locked(),
            balance.get_pending_incoming(),
            balance.get_pending_outgoing()
        );
        self.wallet.as_mut().unwrap().lock().unwrap().set_balance(balance);
    }

    pub fn on_transaction_validation_complete(&mut self, request_key: u64, validation_results: u64) {
        println!(
            "{} callbackTransactionValidationComplete({}, {}).",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            request_key,
            validation_results
        );

        *self.tx_validation_complete.lock().unwrap() = true;
        *self.tx_validation_result.lock().unwrap() = validation_results;
    }

    pub fn on_saf_messages_received(&mut self) {
        println!(
            "{} callbackSafMessageReceived().",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
        );
        *self.transaction_saf_message_received.lock().unwrap() += 1;
    }

    pub fn on_connectivity_status(&mut self, status: u64) {
        println!(
            "{} Connectivity Status Changed to {}.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            status
        );
    }

    pub fn on_basenode_state_update(&mut self, state: *mut c_void) {
        *self.basenode_state_updated.lock().unwrap() += 1;
        println!(
            "{} Base node state changed to {:#?}.",
            chrono::Local::now().format("%Y/%m/%d %H:%M:%S"),
            state
        );
    }

    pub fn reset(&mut self, wallet: Arc<Mutex<Wallet>>) {
        *self.transaction_received.lock().unwrap() = 0;
        *self.transaction_reply_received.lock().unwrap() = 0;
        *self.transaction_finalized.lock().unwrap() = 0;
        *self.transaction_broadcast.lock().unwrap() = 0;
        *self.transaction_mined.lock().unwrap() = 0;
        *self.transaction_mined_unconfirmed.lock().unwrap() = 0;
        *self.transaction_faux_confirmed.lock().unwrap() = 0;
        *self.transaction_faux_unconfirmed.lock().unwrap() = 0;
        *self.transaction_cancelled.lock().unwrap() = 0;
        *self.txo_validation_complete.lock().unwrap() = false;
        *self.txo_validation_result.lock().unwrap() = 0;
        *self.tx_validation_complete.lock().unwrap() = false;
        *self.tx_validation_result.lock().unwrap() = 0;
        *self.transaction_saf_message_received.lock().unwrap() = 0;
        *self.contacts_liveness_data_updated.lock().unwrap() = 0;
        *self.basenode_state_updated.lock().unwrap() = 0;
        self.wallet = Some(wallet);
        println!("wallet {:?}", self.wallet);
    }

    pub fn instance() -> &'static mut Self {
        unsafe {
            START.call_once(|| {
                INSTANCE = Some(Self::default());
            });
            INSTANCE.as_mut().unwrap()
        }
    }
}
