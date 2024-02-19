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

use libc::{c_char, c_int, c_uchar, c_uint, c_ulonglong, c_ushort, c_void};

pub type TariTransportConfig = c_void;
pub type TariCommsConfig = c_void;
pub type TariSeedWords = c_void;
pub type TariPendingInboundTransaction = c_void;
pub type TariCompletedTransaction = c_void;
pub type TariTransactionSendStatus = c_void;
pub type TariContactsLivenessData = c_void;
pub type TariBalance = c_void;
pub type TariWallet = c_void;
pub type TariWalletAddress = c_void;
pub type ByteVector = c_void;
#[allow(dead_code)]
pub type TariFeePerGramStat = c_void;
#[allow(dead_code)]
pub type TariTypeTag = c_void;
pub type TariVector = c_void;
pub type TariCoinPreview = c_void;
pub type TariTransactionKernel = c_void;
pub type TariPublicKey = c_void;
#[allow(dead_code)]
pub type TariPublicKeys = c_void;
#[allow(dead_code)]
pub type TariPrivateKey = c_void;
#[allow(dead_code)]
pub type TariComAndPubSignature = c_void;
#[allow(dead_code)]
pub type TariOutputFeatures = c_void;
#[allow(dead_code)]
pub type TariCovenant = c_void;
#[allow(dead_code)]
pub type TariEncryptedOpenings = c_void;
#[allow(dead_code)]
pub type TariUnblindedOutput = c_void;
#[allow(dead_code)]
pub type TariUnblindedOutputs = c_void;
pub type TariContact = c_void;
pub type TariContacts = c_void;
pub type TariCompletedTransactions = c_void;
pub type TariPendingOutboundTransactions = c_void;
pub type TariPendingOutboundTransaction = c_void;
pub type TariPendingInboundTransactions = c_void;
#[allow(dead_code)]
pub type TariUtxoSort = c_void;
#[allow(dead_code)]
pub type EmojiSet = c_void;
#[allow(dead_code)]
pub type TariFeePerGramStats = c_void;
pub type TariBaseNodeState = c_void;

#[cfg_attr(windows, link(name = "minotari_wallet_ffi.dll"))]
#[cfg_attr(not(windows), link(name = "minotari_wallet_ffi"))]
#[allow(dead_code)]
extern "C" {
    pub fn create_tari_vector(tag: TariTypeTag) -> *mut TariVector;
    pub fn tari_vector_push_string(tv: *mut TariVector, s: *const c_char, error_ptr: *mut i32);
    pub fn destroy_tari_vector(v: *mut TariVector);
    pub fn destroy_tari_coin_preview(p: *mut TariCoinPreview);
    pub fn string_destroy(ptr: *mut c_char);
    pub fn transaction_kernel_get_excess_hex(kernel: *mut TariTransactionKernel, error_out: *mut c_int) -> *mut c_char;
    pub fn transaction_kernel_get_excess_public_nonce_hex(
        kernel: *mut TariTransactionKernel,
        error_out: *mut c_int,
    ) -> *mut c_char;
    pub fn transaction_kernel_get_excess_signature_hex(
        kernel: *mut TariTransactionKernel,
        error_out: *mut c_int,
    ) -> *mut c_char;
    pub fn transaction_kernel_destroy(x: *mut TariTransactionKernel);
    pub fn byte_vector_create(
        byte_array: *const c_uchar,
        element_count: c_uint,
        error_out: *mut c_int,
    ) -> *mut ByteVector;
    pub fn byte_vector_destroy(bytes: *mut ByteVector);
    pub fn byte_vector_get_at(ptr: *mut ByteVector, position: c_uint, error_out: *mut c_int) -> c_uchar;
    pub fn byte_vector_get_length(vec: *const ByteVector, error_out: *mut c_int) -> c_uint;
    pub fn public_key_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TariPublicKey;
    pub fn public_key_destroy(pk: *mut TariPublicKey);
    pub fn public_keys_destroy(pks: *mut TariPublicKeys);
    pub fn public_key_get_bytes(pk: *mut TariPublicKey, error_out: *mut c_int) -> *mut ByteVector;
    pub fn public_key_from_private_key(secret_key: *mut TariPrivateKey, error_out: *mut c_int) -> *mut TariPublicKey;
    pub fn public_key_from_hex(key: *const c_char, error_out: *mut c_int) -> *mut TariPublicKey;
    pub fn tari_address_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TariWalletAddress;
    pub fn tari_address_destroy(address: *mut TariWalletAddress);
    pub fn tari_address_get_bytes(address: *mut TariWalletAddress, error_out: *mut c_int) -> *mut ByteVector;
    pub fn tari_address_from_private_key(
        secret_key: *mut TariPrivateKey,
        network: c_uint,
        error_out: *mut c_int,
    ) -> *mut TariWalletAddress;
    pub fn tari_address_from_hex(address: *const c_char, error_out: *mut c_int) -> *mut TariWalletAddress;
    pub fn tari_address_to_emoji_id(address: *mut TariWalletAddress, error_out: *mut c_int) -> *mut c_char;
    pub fn emoji_id_to_tari_address(emoji: *const c_char, error_out: *mut c_int) -> *mut TariWalletAddress;
    pub fn commitment_and_public_signature_create_from_bytes(
        ephemeral_commitment_bytes: *const ByteVector,
        ephemeral_pubkey_bytes: *const ByteVector,
        u_a_bytes: *const ByteVector,
        u_x_bytes: *const ByteVector,
        u_y_bytes: *const ByteVector,
        error_out: *mut c_int,
    ) -> *mut TariComAndPubSignature;
    pub fn commitment_and_public_signature_destroy(compub_sig: *mut TariComAndPubSignature);
    pub fn create_tari_unblinded_output(
        amount: c_ulonglong,
        spending_key: *mut TariPrivateKey,
        features: *mut TariOutputFeatures,
        script: *const c_char,
        input_data: *const c_char,
        metadata_signature: *mut TariComAndPubSignature,
        sender_offset_public_key: *mut TariPublicKey,
        script_private_key: *mut TariPrivateKey,
        covenant: *mut TariCovenant,
        encrypted_data: *mut TariEncryptedOpenings,
        minimum_value_promise: c_ulonglong,
        script_lock_height: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TariUnblindedOutput;
    pub fn tari_unblinded_output_destroy(output: *mut TariUnblindedOutput);
    pub fn unblinded_outputs_get_length(outputs: *mut TariUnblindedOutputs, error_out: *mut c_int) -> c_uint;
    pub fn unblinded_outputs_get_at(
        outputs: *mut TariUnblindedOutputs,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TariUnblindedOutput;
    pub fn unblinded_outputs_destroy(outputs: *mut TariUnblindedOutputs);
    pub fn wallet_import_external_utxo_as_non_rewindable(
        wallet: *mut TariWallet,
        output: *mut TariUnblindedOutput,
        source_address: *mut TariWalletAddress,
        message: *const c_char,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn wallet_get_unspent_outputs(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariUnblindedOutputs;
    pub fn private_key_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TariPrivateKey;
    pub fn private_key_destroy(pk: *mut TariPrivateKey);
    pub fn private_key_get_bytes(pk: *mut TariPrivateKey, error_out: *mut c_int) -> *mut ByteVector;
    pub fn private_key_generate() -> *mut TariPrivateKey;
    pub fn private_key_from_hex(key: *const c_char, error_out: *mut c_int) -> *mut TariPrivateKey;
    pub fn covenant_create_from_bytes(covenant_bytes: *const ByteVector, error_out: *mut c_int) -> *mut TariCovenant;
    pub fn covenant_destroy(covenant: *mut TariCovenant);
    pub fn encrypted_data_create_from_bytes(
        encrypted_data_bytes: *const ByteVector,
        error_out: *mut c_int,
    ) -> *mut TariEncryptedOpenings;
    pub fn encrypted_data_as_bytes(
        encrypted_data: *const TariEncryptedOpenings,
        error_out: *mut c_int,
    ) -> *mut ByteVector;
    pub fn encrypted_data_destroy(encrypted_data: *mut TariEncryptedOpenings);
    pub fn output_features_create_from_bytes(
        version: c_uchar,
        output_type: c_ushort,
        maturity: c_ulonglong,
        metadata: *const ByteVector,
        error_out: *mut c_int,
    ) -> *mut TariOutputFeatures;
    pub fn output_features_destroy(output_features: *mut TariOutputFeatures);
    pub fn seed_words_create() -> *mut TariSeedWords;
    pub fn seed_words_get_mnemonic_word_list_for_language(
        language: *const c_char,
        error_out: *mut c_int,
    ) -> *mut TariSeedWords;
    pub fn seed_words_get_length(seed_words: *const TariSeedWords, error_out: *mut c_int) -> c_uint;
    pub fn seed_words_get_at(seed_words: *mut TariSeedWords, position: c_uint, error_out: *mut c_int) -> *mut c_char;
    pub fn seed_words_push_word(seed_words: *mut TariSeedWords, word: *const c_char, error_out: *mut c_int) -> c_uchar;
    pub fn seed_words_destroy(seed_words: *mut TariSeedWords);
    pub fn contact_create(
        alias: *const c_char,
        address: *mut TariWalletAddress,
        favourite: bool,
        error_out: *mut c_int,
    ) -> *mut TariContact;
    pub fn contact_get_alias(contact: *mut TariContact, error_out: *mut c_int) -> *mut c_char;
    pub fn contact_get_tari_address(contact: *mut TariContact, error_out: *mut c_int) -> *mut TariWalletAddress;
    pub fn contact_destroy(contact: *mut TariContact);
    pub fn contacts_get_length(contacts: *mut TariContacts, error_out: *mut c_int) -> c_uint;
    pub fn contacts_get_at(contacts: *mut TariContacts, position: c_uint, error_out: *mut c_int) -> *mut TariContact;
    pub fn contacts_destroy(contacts: *mut TariContacts);
    pub fn liveness_data_get_public_key(
        liveness_data: *mut TariContactsLivenessData,
        error_out: *mut c_int,
    ) -> *mut TariWalletAddress;
    pub fn liveness_data_get_latency(liveness_data: *mut TariContactsLivenessData, error_out: *mut c_int) -> c_int;
    pub fn liveness_data_get_last_seen(
        liveness_data: *mut TariContactsLivenessData,
        error_out: *mut c_int,
    ) -> *mut c_char;
    pub fn liveness_data_get_message_type(liveness_data: *mut TariContactsLivenessData, error_out: *mut c_int)
        -> c_int;
    pub fn liveness_data_get_online_status(
        liveness_data: *mut TariContactsLivenessData,
        error_out: *mut c_int,
    ) -> *const c_char;
    pub fn liveness_data_destroy(liveness_data: *mut TariContactsLivenessData);
    pub fn completed_transactions_get_length(
        transactions: *mut TariCompletedTransactions,
        error_out: *mut c_int,
    ) -> c_uint;
    pub fn completed_transactions_get_at(
        transactions: *mut TariCompletedTransactions,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TariCompletedTransaction;
    pub fn completed_transactions_destroy(transactions: *mut TariCompletedTransactions);
    pub fn pending_outbound_transactions_get_length(
        transactions: *mut TariPendingOutboundTransactions,
        error_out: *mut c_int,
    ) -> c_uint;
    pub fn pending_outbound_transactions_get_at(
        transactions: *mut TariPendingOutboundTransactions,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TariPendingOutboundTransaction;
    pub fn pending_outbound_transactions_destroy(transactions: *mut TariPendingOutboundTransactions);
    pub fn pending_inbound_transactions_get_length(
        transactions: *mut TariPendingInboundTransactions,
        error_out: *mut c_int,
    ) -> c_uint;
    pub fn pending_inbound_transactions_get_at(
        transactions: *mut TariPendingInboundTransactions,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TariPendingInboundTransaction;
    pub fn pending_inbound_transactions_destroy(transactions: *mut TariPendingInboundTransactions);
    pub fn completed_transaction_get_transaction_id(
        transaction: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_destination_tari_address(
        transaction: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> *mut TariWalletAddress;
    pub fn completed_transaction_get_transaction_kernel(
        transaction: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> *mut TariTransactionKernel;
    pub fn completed_transaction_get_source_tari_address(
        transaction: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> *mut TariWalletAddress;
    pub fn completed_transaction_get_status(transaction: *mut TariCompletedTransaction, error_out: *mut c_int)
        -> c_int;
    pub fn completed_transaction_get_amount(
        transaction: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_fee(
        transaction: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_timestamp(
        transaction: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_message(
        transaction: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> *const c_char;
    pub fn completed_transaction_is_outbound(tx: *mut TariCompletedTransaction, error_out: *mut c_int) -> bool;
    pub fn completed_transaction_get_confirmations(
        tx: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_cancellation_reason(
        tx: *mut TariCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_int;
    pub fn completed_transaction_destroy(transaction: *mut TariCompletedTransaction);
    pub fn pending_outbound_transaction_get_transaction_id(
        transaction: *mut TariPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_outbound_transaction_get_destination_tari_address(
        transaction: *mut TariPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> *mut TariWalletAddress;
    pub fn pending_outbound_transaction_get_amount(
        transaction: *mut TariPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_outbound_transaction_get_fee(
        transaction: *mut TariPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_outbound_transaction_get_timestamp(
        transaction: *mut TariPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_outbound_transaction_get_message(
        transaction: *mut TariPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> *const c_char;
    pub fn pending_outbound_transaction_get_status(
        transaction: *mut TariPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_int;
    pub fn pending_outbound_transaction_destroy(transaction: *mut TariPendingOutboundTransaction);
    pub fn pending_inbound_transaction_get_transaction_id(
        transaction: *mut TariPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_inbound_transaction_get_source_tari_address(
        transaction: *mut TariPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> *mut TariWalletAddress;
    pub fn pending_inbound_transaction_get_amount(
        transaction: *mut TariPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_inbound_transaction_get_timestamp(
        transaction: *mut TariPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_inbound_transaction_get_message(
        transaction: *mut TariPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> *const c_char;
    pub fn pending_inbound_transaction_get_status(
        transaction: *mut TariPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> c_int;
    pub fn pending_inbound_transaction_destroy(transaction: *mut TariPendingInboundTransaction);
    pub fn transaction_send_status_decode(status: *const TariTransactionSendStatus, error_out: *mut c_int) -> c_uint;
    pub fn transaction_send_status_destroy(status: *mut TariTransactionSendStatus);
    pub fn transport_memory_create() -> *mut TariTransportConfig;
    pub fn transport_tcp_create(listener_address: *const c_char, error_out: *mut c_int) -> *mut TariTransportConfig;
    pub fn transport_tor_create(
        control_server_address: *const c_char,
        tor_cookie: *const ByteVector,
        tor_port: c_ushort,
        tor_proxy_bypass_for_outbound: bool,
        socks_username: *const c_char,
        socks_password: *const c_char,
        error_out: *mut c_int,
    ) -> *mut TariTransportConfig;
    pub fn transport_memory_get_address(transport: *const TariTransportConfig, error_out: *mut c_int) -> *mut c_char;
    pub fn transport_type_destroy(transport: *mut TariTransportConfig);
    pub fn transport_config_destroy(transport: *mut TariTransportConfig);
    pub fn comms_config_create(
        public_address: *const c_char,
        transport: *const TariTransportConfig,
        database_name: *const c_char,
        datastore_path: *const c_char,
        discovery_timeout_in_secs: c_ulonglong,
        saf_message_duration_in_secs: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TariCommsConfig;
    pub fn comms_config_destroy(wc: *mut TariCommsConfig);
    pub fn comms_list_connected_public_keys(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariPublicKeys;
    pub fn public_keys_get_length(public_keys: *const TariPublicKeys, error_out: *mut c_int) -> c_uint;
    pub fn public_keys_get_at(
        public_keys: *const TariPublicKeys,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TariPublicKey;
    pub fn wallet_create(
        config: *mut TariCommsConfig,
        log_path: *const c_char,
        log_level: c_int,
        num_rolling_log_files: c_uint,
        size_per_log_file_bytes: c_uint,
        passphrase: *const c_char,
        seed_words: *const TariSeedWords,
        network_str: *const c_char,
        callback_received_transaction: unsafe extern "C" fn(*mut TariPendingInboundTransaction),
        callback_received_transaction_reply: unsafe extern "C" fn(*mut TariCompletedTransaction),
        callback_received_finalized_transaction: unsafe extern "C" fn(*mut TariCompletedTransaction),
        callback_transaction_broadcast: unsafe extern "C" fn(*mut TariCompletedTransaction),
        callback_transaction_mined: unsafe extern "C" fn(*mut TariCompletedTransaction),
        callback_transaction_mined_unconfirmed: unsafe extern "C" fn(*mut TariCompletedTransaction, u64),
        callback_faux_transaction_confirmed: unsafe extern "C" fn(*mut TariCompletedTransaction),
        callback_faux_transaction_unconfirmed: unsafe extern "C" fn(*mut TariCompletedTransaction, u64),
        callback_transaction_send_result: unsafe extern "C" fn(c_ulonglong, *mut TariTransactionSendStatus),
        callback_transaction_cancellation: unsafe extern "C" fn(*mut TariCompletedTransaction, u64),
        callback_txo_validation_complete: unsafe extern "C" fn(u64, u64),
        callback_contacts_liveness_data_updated: unsafe extern "C" fn(*mut TariContactsLivenessData),
        callback_balance_updated: unsafe extern "C" fn(*mut TariBalance),
        callback_transaction_validation_complete: unsafe extern "C" fn(u64, u64),
        callback_saf_messages_received: unsafe extern "C" fn(),
        callback_connectivity_status: unsafe extern "C" fn(u64),
        callback_base_node_state_updated: unsafe extern "C" fn(*mut TariBaseNodeState),
        recovery_in_progress: *mut bool,
        error_out: *mut c_int,
    ) -> *mut TariWallet;
    pub fn wallet_get_balance(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariBalance;
    pub fn wallet_get_utxos(
        wallet: *mut TariWallet,
        page: usize,
        page_size: usize,
        sorting: TariUtxoSort,
        states: *mut TariVector,
        dust_threshold: u64,
        error_ptr: *mut i32,
    ) -> *mut TariVector;
    pub fn wallet_get_all_utxos(wallet: *mut TariWallet, error_ptr: *mut i32) -> *mut TariVector;
    pub fn wallet_coin_split(
        wallet: *mut TariWallet,
        commitments: *mut TariVector,
        number_of_splits: usize,
        fee_per_gram: u64,
        error_ptr: *mut i32,
    ) -> u64;
    pub fn wallet_coin_join(
        wallet: *mut TariWallet,
        commitments: *mut TariVector,
        fee_per_gram: u64,
        error_ptr: *mut i32,
    ) -> u64;
    pub fn wallet_preview_coin_join(
        wallet: *mut TariWallet,
        commitments: *mut TariVector,
        fee_per_gram: u64,
        error_ptr: *mut i32,
    ) -> *mut TariCoinPreview;
    pub fn wallet_preview_coin_split(
        wallet: *mut TariWallet,
        commitments: *mut TariVector,
        number_of_splits: usize,
        fee_per_gram: u64,
        error_ptr: *mut i32,
    ) -> *mut TariCoinPreview;
    pub fn wallet_sign_message(wallet: *mut TariWallet, msg: *const c_char, error_out: *mut c_int) -> *mut c_char;
    pub fn wallet_verify_message_signature(
        wallet: *mut TariWallet,
        public_key: *mut TariPublicKey,
        hex_sig_nonce: *const c_char,
        msg: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_set_base_node_peer(
        wallet: *mut TariWallet,
        public_key: *mut TariPublicKey,
        address: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_upsert_contact(wallet: *mut TariWallet, contact: *mut TariContact, error_out: *mut c_int) -> bool;
    pub fn wallet_remove_contact(wallet: *mut TariWallet, contact: *mut TariContact, error_out: *mut c_int) -> bool;
    pub fn balance_get_available(balance: *mut TariBalance, error_out: *mut c_int) -> c_ulonglong;
    pub fn balance_get_time_locked(balance: *mut TariBalance, error_out: *mut c_int) -> c_ulonglong;
    pub fn balance_get_pending_incoming(balance: *mut TariBalance, error_out: *mut c_int) -> c_ulonglong;
    pub fn balance_get_pending_outgoing(balance: *mut TariBalance, error_out: *mut c_int) -> c_ulonglong;
    pub fn balance_destroy(balance: *mut TariBalance);
    pub fn wallet_send_transaction(
        wallet: *mut TariWallet,
        destination: *mut TariWalletAddress,
        amount: c_ulonglong,
        commitments: *mut TariVector,
        fee_per_gram: c_ulonglong,
        message: *const c_char,
        one_sided: bool,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn wallet_get_fee_estimate(
        wallet: *mut TariWallet,
        amount: c_ulonglong,
        commitments: *mut TariVector,
        fee_per_gram: c_ulonglong,
        num_kernels: c_ulonglong,
        num_outputs: c_ulonglong,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn wallet_get_num_confirmations_required(wallet: *mut TariWallet, error_out: *mut c_int) -> c_ulonglong;
    pub fn wallet_set_num_confirmations_required(wallet: *mut TariWallet, num: c_ulonglong, error_out: *mut c_int);
    pub fn wallet_get_contacts(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariContacts;
    pub fn wallet_get_completed_transactions(
        wallet: *mut TariWallet,
        error_out: *mut c_int,
    ) -> *mut TariCompletedTransactions;
    pub fn wallet_get_pending_inbound_transactions(
        wallet: *mut TariWallet,
        error_out: *mut c_int,
    ) -> *mut TariPendingInboundTransactions;
    pub fn wallet_get_pending_outbound_transactions(
        wallet: *mut TariWallet,
        error_out: *mut c_int,
    ) -> *mut TariPendingOutboundTransactions;
    pub fn wallet_get_cancelled_transactions(
        wallet: *mut TariWallet,
        error_out: *mut c_int,
    ) -> *mut TariCompletedTransactions;
    pub fn wallet_get_completed_transaction_by_id(
        wallet: *mut TariWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TariCompletedTransaction;
    pub fn wallet_get_pending_inbound_transaction_by_id(
        wallet: *mut TariWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TariPendingInboundTransaction;
    pub fn wallet_get_pending_outbound_transaction_by_id(
        wallet: *mut TariWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TariPendingOutboundTransaction;
    pub fn wallet_get_cancelled_transaction_by_id(
        wallet: *mut TariWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TariCompletedTransaction;
    pub fn wallet_get_tari_address(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariWalletAddress;
    pub fn wallet_cancel_pending_transaction(
        wallet: *mut TariWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_start_txo_validation(wallet: *mut TariWallet, error_out: *mut c_int) -> c_ulonglong;
    pub fn wallet_start_transaction_validation(wallet: *mut TariWallet, error_out: *mut c_int) -> c_ulonglong;
    pub fn wallet_restart_transaction_broadcast(wallet: *mut TariWallet, error_out: *mut c_int) -> bool;
    pub fn wallet_get_seed_words(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariSeedWords;
    pub fn wallet_set_low_power_mode(wallet: *mut TariWallet, error_out: *mut c_int);
    pub fn wallet_set_normal_power_mode(wallet: *mut TariWallet, error_out: *mut c_int);
    pub fn wallet_set_key_value(
        wallet: *mut TariWallet,
        key: *const c_char,
        value: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_get_value(wallet: *mut TariWallet, key: *const c_char, error_out: *mut c_int) -> *mut c_char;
    pub fn wallet_clear_value(wallet: *mut TariWallet, key: *const c_char, error_out: *mut c_int) -> bool;
    pub fn wallet_is_recovery_in_progress(wallet: *mut TariWallet, error_out: *mut c_int) -> bool;
    pub fn wallet_start_recovery(
        wallet: *mut TariWallet,
        base_node_public_key: *mut TariPublicKey,
        recovery_progress_callback: unsafe extern "C" fn(u8, u64, u64),
        recovered_output_message: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_set_one_sided_payment_message(
        wallet: *mut TariWallet,
        message: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn get_emoji_set() -> *mut EmojiSet;
    pub fn emoji_set_get_length(emoji_set: *const EmojiSet, error_out: *mut c_int) -> c_uint;
    pub fn emoji_set_get_at(emoji_set: *const EmojiSet, position: c_uint, error_out: *mut c_int) -> *mut ByteVector;
    pub fn emoji_set_destroy(emoji_set: *mut EmojiSet);
    pub fn wallet_destroy(wallet: *mut TariWallet);
    pub fn log_debug_message(msg: *const c_char, error_out: *mut c_int);
    pub fn wallet_get_fee_per_gram_stats(
        wallet: *mut TariWallet,
        count: c_uint,
        error_out: *mut c_int,
    ) -> *mut TariFeePerGramStats;
    pub fn fee_per_gram_stats_get_length(fee_per_gram_stats: *mut TariFeePerGramStats, error_out: *mut c_int)
        -> c_uint;
    pub fn fee_per_gram_stats_get_at(
        fee_per_gram_stats: *mut TariFeePerGramStats,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TariFeePerGramStat;
    pub fn fee_per_gram_stats_destroy(fee_per_gram_stats: *mut TariFeePerGramStats);
    pub fn fee_per_gram_stat_get_order(
        fee_per_gram_stat: *mut TariFeePerGramStat,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn fee_per_gram_stat_get_min_fee_per_gram(
        fee_per_gram_stat: *mut TariFeePerGramStat,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn fee_per_gram_stat_get_avg_fee_per_gram(
        fee_per_gram_stat: *mut TariFeePerGramStat,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn fee_per_gram_stat_get_max_fee_per_gram(
        fee_per_gram_stat: *mut TariFeePerGramStat,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn fee_per_gram_stat_destroy(fee_per_gram_stat: *mut TariFeePerGramStat);
    pub fn contacts_handle(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut c_void;
}
