//   Copyright 2022. The Taiji Project
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

pub type TaijiTransportConfig = c_void;
pub type TaijiCommsConfig = c_void;
pub type TaijiSeedWords = c_void;
pub type TaijiPendingInboundTransaction = c_void;
pub type TaijiCompletedTransaction = c_void;
pub type TaijiTransactionSendStatus = c_void;
pub type TaijiContactsLivenessData = c_void;
pub type TaijiBalance = c_void;
pub type TaijiWallet = c_void;
pub type TaijiWalletAddress = c_void;
pub type ByteVector = c_void;
#[allow(dead_code)]
pub type TaijiFeePerGramStat = c_void;
#[allow(dead_code)]
pub type TaijiTypeTag = c_void;
pub type TaijiVector = c_void;
pub type TaijiCoinPreview = c_void;
pub type TaijiTransactionKernel = c_void;
pub type TaijiPublicKey = c_void;
#[allow(dead_code)]
pub type TaijiPublicKeys = c_void;
#[allow(dead_code)]
pub type TaijiPrivateKey = c_void;
#[allow(dead_code)]
pub type TaijiComAndPubSignature = c_void;
#[allow(dead_code)]
pub type TaijiOutputFeatures = c_void;
#[allow(dead_code)]
pub type TaijiCovenant = c_void;
#[allow(dead_code)]
pub type TaijiEncryptedOpenings = c_void;
#[allow(dead_code)]
pub type TaijiUnblindedOutput = c_void;
#[allow(dead_code)]
pub type TaijiUnblindedOutputs = c_void;
pub type TaijiContact = c_void;
pub type TaijiContacts = c_void;
pub type TaijiCompletedTransactions = c_void;
pub type TaijiPendingOutboundTransactions = c_void;
pub type TaijiPendingOutboundTransaction = c_void;
pub type TaijiPendingInboundTransactions = c_void;
#[allow(dead_code)]
pub type TaijiUtxoSort = c_void;
#[allow(dead_code)]
pub type EmojiSet = c_void;
#[allow(dead_code)]
pub type TaijiFeePerGramStats = c_void;
pub type TaijiBaseNodeState = c_void;

#[cfg_attr(windows, link(name = "minotaiji_wallet_ffi.dll"))]
#[cfg_attr(not(windows), link(name = "minotaiji_wallet_ffi"))]
#[allow(dead_code)]
extern "C" {
    pub fn create_taiji_vector(tag: TaijiTypeTag) -> *mut TaijiVector;
    pub fn taiji_vector_push_string(tv: *mut TaijiVector, s: *const c_char, error_ptr: *mut i32);
    pub fn destroy_taiji_vector(v: *mut TaijiVector);
    pub fn destroy_taiji_coin_preview(p: *mut TaijiCoinPreview);
    pub fn string_destroy(ptr: *mut c_char);
    pub fn transaction_kernel_get_excess_hex(kernel: *mut TaijiTransactionKernel, error_out: *mut c_int) -> *mut c_char;
    pub fn transaction_kernel_get_excess_public_nonce_hex(
        kernel: *mut TaijiTransactionKernel,
        error_out: *mut c_int,
    ) -> *mut c_char;
    pub fn transaction_kernel_get_excess_signature_hex(
        kernel: *mut TaijiTransactionKernel,
        error_out: *mut c_int,
    ) -> *mut c_char;
    pub fn transaction_kernel_destroy(x: *mut TaijiTransactionKernel);
    pub fn byte_vector_create(
        byte_array: *const c_uchar,
        element_count: c_uint,
        error_out: *mut c_int,
    ) -> *mut ByteVector;
    pub fn byte_vector_destroy(bytes: *mut ByteVector);
    pub fn byte_vector_get_at(ptr: *mut ByteVector, position: c_uint, error_out: *mut c_int) -> c_uchar;
    pub fn byte_vector_get_length(vec: *const ByteVector, error_out: *mut c_int) -> c_uint;
    pub fn public_key_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TaijiPublicKey;
    pub fn public_key_destroy(pk: *mut TaijiPublicKey);
    pub fn public_keys_destroy(pks: *mut TaijiPublicKeys);
    pub fn public_key_get_bytes(pk: *mut TaijiPublicKey, error_out: *mut c_int) -> *mut ByteVector;
    pub fn public_key_from_private_key(secret_key: *mut TaijiPrivateKey, error_out: *mut c_int) -> *mut TaijiPublicKey;
    pub fn public_key_from_hex(key: *const c_char, error_out: *mut c_int) -> *mut TaijiPublicKey;
    pub fn taiji_address_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TaijiWalletAddress;
    pub fn taiji_address_destroy(address: *mut TaijiWalletAddress);
    pub fn taiji_address_get_bytes(address: *mut TaijiWalletAddress, error_out: *mut c_int) -> *mut ByteVector;
    pub fn taiji_address_from_private_key(
        secret_key: *mut TaijiPrivateKey,
        network: c_uint,
        error_out: *mut c_int,
    ) -> *mut TaijiWalletAddress;
    pub fn taiji_address_from_hex(address: *const c_char, error_out: *mut c_int) -> *mut TaijiWalletAddress;
    pub fn taiji_address_to_emoji_id(address: *mut TaijiWalletAddress, error_out: *mut c_int) -> *mut c_char;
    pub fn emoji_id_to_taiji_address(emoji: *const c_char, error_out: *mut c_int) -> *mut TaijiWalletAddress;
    pub fn commitment_and_public_signature_create_from_bytes(
        ephemeral_commitment_bytes: *const ByteVector,
        ephemeral_pubkey_bytes: *const ByteVector,
        u_a_bytes: *const ByteVector,
        u_x_bytes: *const ByteVector,
        u_y_bytes: *const ByteVector,
        error_out: *mut c_int,
    ) -> *mut TaijiComAndPubSignature;
    pub fn commitment_and_public_signature_destroy(compub_sig: *mut TaijiComAndPubSignature);
    pub fn create_taiji_unblinded_output(
        amount: c_ulonglong,
        spending_key: *mut TaijiPrivateKey,
        features: *mut TaijiOutputFeatures,
        script: *const c_char,
        input_data: *const c_char,
        metadata_signature: *mut TaijiComAndPubSignature,
        sender_offset_public_key: *mut TaijiPublicKey,
        script_private_key: *mut TaijiPrivateKey,
        covenant: *mut TaijiCovenant,
        encrypted_data: *mut TaijiEncryptedOpenings,
        minimum_value_promise: c_ulonglong,
        script_lock_height: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TaijiUnblindedOutput;
    pub fn taiji_unblinded_output_destroy(output: *mut TaijiUnblindedOutput);
    pub fn unblinded_outputs_get_length(outputs: *mut TaijiUnblindedOutputs, error_out: *mut c_int) -> c_uint;
    pub fn unblinded_outputs_get_at(
        outputs: *mut TaijiUnblindedOutputs,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TaijiUnblindedOutput;
    pub fn unblinded_outputs_destroy(outputs: *mut TaijiUnblindedOutputs);
    pub fn wallet_import_external_utxo_as_non_rewindable(
        wallet: *mut TaijiWallet,
        output: *mut TaijiUnblindedOutput,
        source_address: *mut TaijiWalletAddress,
        message: *const c_char,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn wallet_get_unspent_outputs(wallet: *mut TaijiWallet, error_out: *mut c_int) -> *mut TaijiUnblindedOutputs;
    pub fn private_key_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TaijiPrivateKey;
    pub fn private_key_destroy(pk: *mut TaijiPrivateKey);
    pub fn private_key_get_bytes(pk: *mut TaijiPrivateKey, error_out: *mut c_int) -> *mut ByteVector;
    pub fn private_key_generate() -> *mut TaijiPrivateKey;
    pub fn private_key_from_hex(key: *const c_char, error_out: *mut c_int) -> *mut TaijiPrivateKey;
    pub fn covenant_create_from_bytes(covenant_bytes: *const ByteVector, error_out: *mut c_int) -> *mut TaijiCovenant;
    pub fn covenant_destroy(covenant: *mut TaijiCovenant);
    pub fn encrypted_data_create_from_bytes(
        encrypted_data_bytes: *const ByteVector,
        error_out: *mut c_int,
    ) -> *mut TaijiEncryptedOpenings;
    pub fn encrypted_data_as_bytes(
        encrypted_data: *const TaijiEncryptedOpenings,
        error_out: *mut c_int,
    ) -> *mut ByteVector;
    pub fn encrypted_data_destroy(encrypted_data: *mut TaijiEncryptedOpenings);
    pub fn output_features_create_from_bytes(
        version: c_uchar,
        output_type: c_ushort,
        maturity: c_ulonglong,
        metadata: *const ByteVector,
        error_out: *mut c_int,
    ) -> *mut TaijiOutputFeatures;
    pub fn output_features_destroy(output_features: *mut TaijiOutputFeatures);
    pub fn seed_words_create() -> *mut TaijiSeedWords;
    pub fn seed_words_get_mnemonic_word_list_for_language(
        language: *const c_char,
        error_out: *mut c_int,
    ) -> *mut TaijiSeedWords;
    pub fn seed_words_get_length(seed_words: *const TaijiSeedWords, error_out: *mut c_int) -> c_uint;
    pub fn seed_words_get_at(seed_words: *mut TaijiSeedWords, position: c_uint, error_out: *mut c_int) -> *mut c_char;
    pub fn seed_words_push_word(seed_words: *mut TaijiSeedWords, word: *const c_char, error_out: *mut c_int) -> c_uchar;
    pub fn seed_words_destroy(seed_words: *mut TaijiSeedWords);
    pub fn contact_create(
        alias: *const c_char,
        address: *mut TaijiWalletAddress,
        favourite: bool,
        error_out: *mut c_int,
    ) -> *mut TaijiContact;
    pub fn contact_get_alias(contact: *mut TaijiContact, error_out: *mut c_int) -> *mut c_char;
    pub fn contact_get_taiji_address(contact: *mut TaijiContact, error_out: *mut c_int) -> *mut TaijiWalletAddress;
    pub fn contact_destroy(contact: *mut TaijiContact);
    pub fn contacts_get_length(contacts: *mut TaijiContacts, error_out: *mut c_int) -> c_uint;
    pub fn contacts_get_at(contacts: *mut TaijiContacts, position: c_uint, error_out: *mut c_int) -> *mut TaijiContact;
    pub fn contacts_destroy(contacts: *mut TaijiContacts);
    pub fn liveness_data_get_public_key(
        liveness_data: *mut TaijiContactsLivenessData,
        error_out: *mut c_int,
    ) -> *mut TaijiWalletAddress;
    pub fn liveness_data_get_latency(liveness_data: *mut TaijiContactsLivenessData, error_out: *mut c_int) -> c_int;
    pub fn liveness_data_get_last_seen(
        liveness_data: *mut TaijiContactsLivenessData,
        error_out: *mut c_int,
    ) -> *mut c_char;
    pub fn liveness_data_get_message_type(liveness_data: *mut TaijiContactsLivenessData, error_out: *mut c_int)
        -> c_int;
    pub fn liveness_data_get_online_status(
        liveness_data: *mut TaijiContactsLivenessData,
        error_out: *mut c_int,
    ) -> *const c_char;
    pub fn liveness_data_destroy(liveness_data: *mut TaijiContactsLivenessData);
    pub fn completed_transactions_get_length(
        transactions: *mut TaijiCompletedTransactions,
        error_out: *mut c_int,
    ) -> c_uint;
    pub fn completed_transactions_get_at(
        transactions: *mut TaijiCompletedTransactions,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TaijiCompletedTransaction;
    pub fn completed_transactions_destroy(transactions: *mut TaijiCompletedTransactions);
    pub fn pending_outbound_transactions_get_length(
        transactions: *mut TaijiPendingOutboundTransactions,
        error_out: *mut c_int,
    ) -> c_uint;
    pub fn pending_outbound_transactions_get_at(
        transactions: *mut TaijiPendingOutboundTransactions,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TaijiPendingOutboundTransaction;
    pub fn pending_outbound_transactions_destroy(transactions: *mut TaijiPendingOutboundTransactions);
    pub fn pending_inbound_transactions_get_length(
        transactions: *mut TaijiPendingInboundTransactions,
        error_out: *mut c_int,
    ) -> c_uint;
    pub fn pending_inbound_transactions_get_at(
        transactions: *mut TaijiPendingInboundTransactions,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TaijiPendingInboundTransaction;
    pub fn pending_inbound_transactions_destroy(transactions: *mut TaijiPendingInboundTransactions);
    pub fn completed_transaction_get_transaction_id(
        transaction: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_destination_taiji_address(
        transaction: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> *mut TaijiWalletAddress;
    pub fn completed_transaction_get_transaction_kernel(
        transaction: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> *mut TaijiTransactionKernel;
    pub fn completed_transaction_get_source_taiji_address(
        transaction: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> *mut TaijiWalletAddress;
    pub fn completed_transaction_get_status(transaction: *mut TaijiCompletedTransaction, error_out: *mut c_int)
        -> c_int;
    pub fn completed_transaction_get_amount(
        transaction: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_fee(
        transaction: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_timestamp(
        transaction: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_message(
        transaction: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> *const c_char;
    pub fn completed_transaction_is_outbound(tx: *mut TaijiCompletedTransaction, error_out: *mut c_int) -> bool;
    pub fn completed_transaction_get_confirmations(
        tx: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn completed_transaction_get_cancellation_reason(
        tx: *mut TaijiCompletedTransaction,
        error_out: *mut c_int,
    ) -> c_int;
    pub fn completed_transaction_destroy(transaction: *mut TaijiCompletedTransaction);
    pub fn pending_outbound_transaction_get_transaction_id(
        transaction: *mut TaijiPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_outbound_transaction_get_destination_taiji_address(
        transaction: *mut TaijiPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> *mut TaijiWalletAddress;
    pub fn pending_outbound_transaction_get_amount(
        transaction: *mut TaijiPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_outbound_transaction_get_fee(
        transaction: *mut TaijiPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_outbound_transaction_get_timestamp(
        transaction: *mut TaijiPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_outbound_transaction_get_message(
        transaction: *mut TaijiPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> *const c_char;
    pub fn pending_outbound_transaction_get_status(
        transaction: *mut TaijiPendingOutboundTransaction,
        error_out: *mut c_int,
    ) -> c_int;
    pub fn pending_outbound_transaction_destroy(transaction: *mut TaijiPendingOutboundTransaction);
    pub fn pending_inbound_transaction_get_transaction_id(
        transaction: *mut TaijiPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_inbound_transaction_get_source_taiji_address(
        transaction: *mut TaijiPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> *mut TaijiWalletAddress;
    pub fn pending_inbound_transaction_get_amount(
        transaction: *mut TaijiPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_inbound_transaction_get_timestamp(
        transaction: *mut TaijiPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn pending_inbound_transaction_get_message(
        transaction: *mut TaijiPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> *const c_char;
    pub fn pending_inbound_transaction_get_status(
        transaction: *mut TaijiPendingInboundTransaction,
        error_out: *mut c_int,
    ) -> c_int;
    pub fn pending_inbound_transaction_destroy(transaction: *mut TaijiPendingInboundTransaction);
    pub fn transaction_send_status_decode(status: *const TaijiTransactionSendStatus, error_out: *mut c_int) -> c_uint;
    pub fn transaction_send_status_destroy(status: *mut TaijiTransactionSendStatus);
    pub fn transport_memory_create() -> *mut TaijiTransportConfig;
    pub fn transport_tcp_create(listener_address: *const c_char, error_out: *mut c_int) -> *mut TaijiTransportConfig;
    pub fn transport_tor_create(
        control_server_address: *const c_char,
        tor_cookie: *const ByteVector,
        tor_port: c_ushort,
        tor_proxy_bypass_for_outbound: bool,
        socks_username: *const c_char,
        socks_password: *const c_char,
        error_out: *mut c_int,
    ) -> *mut TaijiTransportConfig;
    pub fn transport_memory_get_address(transport: *const TaijiTransportConfig, error_out: *mut c_int) -> *mut c_char;
    pub fn transport_type_destroy(transport: *mut TaijiTransportConfig);
    pub fn transport_config_destroy(transport: *mut TaijiTransportConfig);
    pub fn comms_config_create(
        public_address: *const c_char,
        transport: *const TaijiTransportConfig,
        database_name: *const c_char,
        datastore_path: *const c_char,
        discovery_timeout_in_secs: c_ulonglong,
        saf_message_duration_in_secs: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TaijiCommsConfig;
    pub fn comms_config_destroy(wc: *mut TaijiCommsConfig);
    pub fn comms_list_connected_public_keys(wallet: *mut TaijiWallet, error_out: *mut c_int) -> *mut TaijiPublicKeys;
    pub fn public_keys_get_length(public_keys: *const TaijiPublicKeys, error_out: *mut c_int) -> c_uint;
    pub fn public_keys_get_at(
        public_keys: *const TaijiPublicKeys,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TaijiPublicKey;
    pub fn wallet_create(
        config: *mut TaijiCommsConfig,
        log_path: *const c_char,
        num_rolling_log_files: c_uint,
        size_per_log_file_bytes: c_uint,
        passphrase: *const c_char,
        seed_words: *const TaijiSeedWords,
        network_str: *const c_char,
        callback_received_transaction: unsafe extern "C" fn(*mut TaijiPendingInboundTransaction),
        callback_received_transaction_reply: unsafe extern "C" fn(*mut TaijiCompletedTransaction),
        callback_received_finalized_transaction: unsafe extern "C" fn(*mut TaijiCompletedTransaction),
        callback_transaction_broadcast: unsafe extern "C" fn(*mut TaijiCompletedTransaction),
        callback_transaction_mined: unsafe extern "C" fn(*mut TaijiCompletedTransaction),
        callback_transaction_mined_unconfirmed: unsafe extern "C" fn(*mut TaijiCompletedTransaction, u64),
        callback_faux_transaction_confirmed: unsafe extern "C" fn(*mut TaijiCompletedTransaction),
        callback_faux_transaction_unconfirmed: unsafe extern "C" fn(*mut TaijiCompletedTransaction, u64),
        callback_transaction_send_result: unsafe extern "C" fn(c_ulonglong, *mut TaijiTransactionSendStatus),
        callback_transaction_cancellation: unsafe extern "C" fn(*mut TaijiCompletedTransaction, u64),
        callback_txo_validation_complete: unsafe extern "C" fn(u64, u64),
        callback_contacts_liveness_data_updated: unsafe extern "C" fn(*mut TaijiContactsLivenessData),
        callback_balance_updated: unsafe extern "C" fn(*mut TaijiBalance),
        callback_transaction_validation_complete: unsafe extern "C" fn(u64, u64),
        callback_saf_messages_received: unsafe extern "C" fn(),
        callback_connectivity_status: unsafe extern "C" fn(u64),
        callback_base_node_state_updated: unsafe extern "C" fn(*mut TaijiBaseNodeState),
        recovery_in_progress: *mut bool,
        error_out: *mut c_int,
    ) -> *mut TaijiWallet;
    pub fn wallet_get_balance(wallet: *mut TaijiWallet, error_out: *mut c_int) -> *mut TaijiBalance;
    pub fn wallet_get_utxos(
        wallet: *mut TaijiWallet,
        page: usize,
        page_size: usize,
        sorting: TaijiUtxoSort,
        states: *mut TaijiVector,
        dust_threshold: u64,
        error_ptr: *mut i32,
    ) -> *mut TaijiVector;
    pub fn wallet_get_all_utxos(wallet: *mut TaijiWallet, error_ptr: *mut i32) -> *mut TaijiVector;
    pub fn wallet_coin_split(
        wallet: *mut TaijiWallet,
        commitments: *mut TaijiVector,
        number_of_splits: usize,
        fee_per_gram: u64,
        error_ptr: *mut i32,
    ) -> u64;
    pub fn wallet_coin_join(
        wallet: *mut TaijiWallet,
        commitments: *mut TaijiVector,
        fee_per_gram: u64,
        error_ptr: *mut i32,
    ) -> u64;
    pub fn wallet_preview_coin_join(
        wallet: *mut TaijiWallet,
        commitments: *mut TaijiVector,
        fee_per_gram: u64,
        error_ptr: *mut i32,
    ) -> *mut TaijiCoinPreview;
    pub fn wallet_preview_coin_split(
        wallet: *mut TaijiWallet,
        commitments: *mut TaijiVector,
        number_of_splits: usize,
        fee_per_gram: u64,
        error_ptr: *mut i32,
    ) -> *mut TaijiCoinPreview;
    pub fn wallet_sign_message(wallet: *mut TaijiWallet, msg: *const c_char, error_out: *mut c_int) -> *mut c_char;
    pub fn wallet_verify_message_signature(
        wallet: *mut TaijiWallet,
        public_key: *mut TaijiPublicKey,
        hex_sig_nonce: *const c_char,
        msg: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_add_base_node_peer(
        wallet: *mut TaijiWallet,
        public_key: *mut TaijiPublicKey,
        address: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_upsert_contact(wallet: *mut TaijiWallet, contact: *mut TaijiContact, error_out: *mut c_int) -> bool;
    pub fn wallet_remove_contact(wallet: *mut TaijiWallet, contact: *mut TaijiContact, error_out: *mut c_int) -> bool;
    pub fn balance_get_available(balance: *mut TaijiBalance, error_out: *mut c_int) -> c_ulonglong;
    pub fn balance_get_time_locked(balance: *mut TaijiBalance, error_out: *mut c_int) -> c_ulonglong;
    pub fn balance_get_pending_incoming(balance: *mut TaijiBalance, error_out: *mut c_int) -> c_ulonglong;
    pub fn balance_get_pending_outgoing(balance: *mut TaijiBalance, error_out: *mut c_int) -> c_ulonglong;
    pub fn balance_destroy(balance: *mut TaijiBalance);
    pub fn wallet_send_transaction(
        wallet: *mut TaijiWallet,
        destination: *mut TaijiWalletAddress,
        amount: c_ulonglong,
        commitments: *mut TaijiVector,
        fee_per_gram: c_ulonglong,
        message: *const c_char,
        one_sided: bool,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn wallet_get_fee_estimate(
        wallet: *mut TaijiWallet,
        amount: c_ulonglong,
        commitments: *mut TaijiVector,
        fee_per_gram: c_ulonglong,
        num_kernels: c_ulonglong,
        num_outputs: c_ulonglong,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn wallet_get_num_confirmations_required(wallet: *mut TaijiWallet, error_out: *mut c_int) -> c_ulonglong;
    pub fn wallet_set_num_confirmations_required(wallet: *mut TaijiWallet, num: c_ulonglong, error_out: *mut c_int);
    pub fn wallet_get_contacts(wallet: *mut TaijiWallet, error_out: *mut c_int) -> *mut TaijiContacts;
    pub fn wallet_get_completed_transactions(
        wallet: *mut TaijiWallet,
        error_out: *mut c_int,
    ) -> *mut TaijiCompletedTransactions;
    pub fn wallet_get_pending_inbound_transactions(
        wallet: *mut TaijiWallet,
        error_out: *mut c_int,
    ) -> *mut TaijiPendingInboundTransactions;
    pub fn wallet_get_pending_outbound_transactions(
        wallet: *mut TaijiWallet,
        error_out: *mut c_int,
    ) -> *mut TaijiPendingOutboundTransactions;
    pub fn wallet_get_cancelled_transactions(
        wallet: *mut TaijiWallet,
        error_out: *mut c_int,
    ) -> *mut TaijiCompletedTransactions;
    pub fn wallet_get_completed_transaction_by_id(
        wallet: *mut TaijiWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TaijiCompletedTransaction;
    pub fn wallet_get_pending_inbound_transaction_by_id(
        wallet: *mut TaijiWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TaijiPendingInboundTransaction;
    pub fn wallet_get_pending_outbound_transaction_by_id(
        wallet: *mut TaijiWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TaijiPendingOutboundTransaction;
    pub fn wallet_get_cancelled_transaction_by_id(
        wallet: *mut TaijiWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> *mut TaijiCompletedTransaction;
    pub fn wallet_get_taiji_address(wallet: *mut TaijiWallet, error_out: *mut c_int) -> *mut TaijiWalletAddress;
    pub fn wallet_cancel_pending_transaction(
        wallet: *mut TaijiWallet,
        transaction_id: c_ulonglong,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_start_txo_validation(wallet: *mut TaijiWallet, error_out: *mut c_int) -> c_ulonglong;
    pub fn wallet_start_transaction_validation(wallet: *mut TaijiWallet, error_out: *mut c_int) -> c_ulonglong;
    pub fn wallet_restart_transaction_broadcast(wallet: *mut TaijiWallet, error_out: *mut c_int) -> bool;
    pub fn wallet_get_seed_words(wallet: *mut TaijiWallet, error_out: *mut c_int) -> *mut TaijiSeedWords;
    pub fn wallet_set_low_power_mode(wallet: *mut TaijiWallet, error_out: *mut c_int);
    pub fn wallet_set_normal_power_mode(wallet: *mut TaijiWallet, error_out: *mut c_int);
    pub fn wallet_set_key_value(
        wallet: *mut TaijiWallet,
        key: *const c_char,
        value: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_get_value(wallet: *mut TaijiWallet, key: *const c_char, error_out: *mut c_int) -> *mut c_char;
    pub fn wallet_clear_value(wallet: *mut TaijiWallet, key: *const c_char, error_out: *mut c_int) -> bool;
    pub fn wallet_is_recovery_in_progress(wallet: *mut TaijiWallet, error_out: *mut c_int) -> bool;
    pub fn wallet_start_recovery(
        wallet: *mut TaijiWallet,
        base_node_public_key: *mut TaijiPublicKey,
        recovery_progress_callback: unsafe extern "C" fn(u8, u64, u64),
        recovered_output_message: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn wallet_set_one_sided_payment_message(
        wallet: *mut TaijiWallet,
        message: *const c_char,
        error_out: *mut c_int,
    ) -> bool;
    pub fn get_emoji_set() -> *mut EmojiSet;
    pub fn emoji_set_get_length(emoji_set: *const EmojiSet, error_out: *mut c_int) -> c_uint;
    pub fn emoji_set_get_at(emoji_set: *const EmojiSet, position: c_uint, error_out: *mut c_int) -> *mut ByteVector;
    pub fn emoji_set_destroy(emoji_set: *mut EmojiSet);
    pub fn wallet_destroy(wallet: *mut TaijiWallet);
    pub fn log_debug_message(msg: *const c_char, error_out: *mut c_int);
    pub fn wallet_get_fee_per_gram_stats(
        wallet: *mut TaijiWallet,
        count: c_uint,
        error_out: *mut c_int,
    ) -> *mut TaijiFeePerGramStats;
    pub fn fee_per_gram_stats_get_length(fee_per_gram_stats: *mut TaijiFeePerGramStats, error_out: *mut c_int)
        -> c_uint;
    pub fn fee_per_gram_stats_get_at(
        fee_per_gram_stats: *mut TaijiFeePerGramStats,
        position: c_uint,
        error_out: *mut c_int,
    ) -> *mut TaijiFeePerGramStat;
    pub fn fee_per_gram_stats_destroy(fee_per_gram_stats: *mut TaijiFeePerGramStats);
    pub fn fee_per_gram_stat_get_order(
        fee_per_gram_stat: *mut TaijiFeePerGramStat,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn fee_per_gram_stat_get_min_fee_per_gram(
        fee_per_gram_stat: *mut TaijiFeePerGramStat,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn fee_per_gram_stat_get_avg_fee_per_gram(
        fee_per_gram_stat: *mut TaijiFeePerGramStat,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn fee_per_gram_stat_get_max_fee_per_gram(
        fee_per_gram_stat: *mut TaijiFeePerGramStat,
        error_out: *mut c_int,
    ) -> c_ulonglong;
    pub fn fee_per_gram_stat_destroy(fee_per_gram_stat: *mut TaijiFeePerGramStat);
}
