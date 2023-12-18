//  Copyright 2022. The Tari Project
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

/// ------------------------------------------------------------------------------------------ ///
#[cfg(test)]
mod test {
    use std::{
        ffi::CString,
        path::Path,
        str::{from_utf8, FromStr},
        sync::Mutex,
    };

    use libc::{c_char, c_uchar, c_uint};
    use minotari_wallet::{
        storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
        transaction_service::handle::TransactionSendStatus,
    };
    use once_cell::sync::Lazy;
    use tari_common_types::{emoji, transaction::TransactionStatus, types::PrivateKey};
    use tari_comms::peer_manager::PeerFeatures;
    use tari_contacts::contacts_service::types::{Direction, Message, MessageMetadata};
    use tari_core::{
        covenant,
        transactions::{
            key_manager::{create_memory_db_key_manager, SecretTransactionKeyManagerInterface},
            test_helpers::{create_test_input, create_wallet_output_with_data, TestParams},
        },
    };
    use tari_key_manager::{mnemonic::MnemonicLanguage, mnemonic_wordlists};
    use tari_p2p::initialization::MESSAGING_PROTOCOL_ID;
    use tari_script::script;
    use tari_test_utils::random;
    use tempfile::tempdir;

    use crate::*;

    fn type_of<T>(_: T) -> String {
        std::any::type_name::<T>().to_string()
    }

    #[allow(dead_code)]
    #[derive(Debug)]
    #[allow(clippy::struct_excessive_bools)]
    struct CallbackState {
        pub received_tx_callback_called: bool,
        pub received_tx_reply_callback_called: bool,
        pub received_finalized_tx_callback_called: bool,
        pub broadcast_tx_callback_called: bool,
        pub mined_tx_callback_called: bool,
        pub mined_tx_unconfirmed_callback_called: bool,
        pub scanned_tx_callback_called: bool,
        pub scanned_tx_unconfirmed_callback_called: bool,
        pub transaction_send_result_callback: bool,
        pub tx_cancellation_callback_called: bool,
        pub callback_txo_validation_complete: bool,
        pub callback_contacts_liveness_data_updated: bool,
        pub callback_balance_updated: bool,
        pub callback_transaction_validation_complete: bool,
        pub callback_basenode_state_updated: bool,
    }

    impl CallbackState {
        fn new() -> Self {
            Self {
                received_tx_callback_called: false,
                received_tx_reply_callback_called: false,
                received_finalized_tx_callback_called: false,
                broadcast_tx_callback_called: false,
                mined_tx_callback_called: false,
                mined_tx_unconfirmed_callback_called: false,
                scanned_tx_callback_called: false,
                scanned_tx_unconfirmed_callback_called: false,
                transaction_send_result_callback: false,
                tx_cancellation_callback_called: false,
                callback_txo_validation_complete: false,
                callback_contacts_liveness_data_updated: false,
                callback_balance_updated: false,
                callback_transaction_validation_complete: false,
                callback_basenode_state_updated: false,
            }
        }
    }

    static CALLBACK_STATE_FFI: Lazy<Mutex<CallbackState>> = Lazy::new(|| Mutex::new(CallbackState::new()));

    unsafe extern "C" fn received_tx_callback(tx: *mut TariPendingInboundTransaction) {
        assert!(!tx.is_null());
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariPendingInboundTransaction>()
        );
        let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
        lock.received_tx_callback_called = true;
        drop(lock);
        pending_inbound_transaction_destroy(tx);
    }

    unsafe extern "C" fn received_tx_reply_callback(tx: *mut TariCompletedTransaction) {
        assert!(!tx.is_null());
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::Completed);
        let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
        lock.received_tx_reply_callback_called = true;
        drop(lock);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn received_tx_finalized_callback(tx: *mut TariCompletedTransaction) {
        assert!(!tx.is_null());
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::Completed);
        let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
        lock.received_finalized_tx_callback_called = true;
        drop(lock);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn broadcast_callback(tx: *mut TariCompletedTransaction) {
        assert!(!tx.is_null());
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
        lock.broadcast_tx_callback_called = true;
        drop(lock);
        assert_eq!((*tx).status, TransactionStatus::Broadcast);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn mined_callback(tx: *mut TariCompletedTransaction) {
        assert!(!tx.is_null());
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::MinedUnconfirmed);
        let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
        lock.mined_tx_callback_called = true;
        drop(lock);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn mined_unconfirmed_callback(tx: *mut TariCompletedTransaction, _confirmations: u64) {
        assert!(!tx.is_null());
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::MinedUnconfirmed);
        let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
        lock.mined_tx_unconfirmed_callback_called = true;
        let mut error = 0;
        let error_ptr = &mut error as *mut c_int;
        let kernel = completed_transaction_get_transaction_kernel(tx, error_ptr);
        let excess_hex_ptr = transaction_kernel_get_excess_hex(kernel, error_ptr);
        let excess_hex = CString::from_raw(excess_hex_ptr).to_str().unwrap().to_owned();
        assert!(!excess_hex.is_empty());
        let nonce_hex_ptr = transaction_kernel_get_excess_public_nonce_hex(kernel, error_ptr);
        let nonce_hex = CString::from_raw(nonce_hex_ptr).to_str().unwrap().to_owned();
        assert!(!nonce_hex.is_empty());
        let sig_hex_ptr = transaction_kernel_get_excess_signature_hex(kernel, error_ptr);
        let sig_hex = CString::from_raw(sig_hex_ptr).to_str().unwrap().to_owned();
        assert!(!sig_hex.is_empty());
        string_destroy(excess_hex_ptr as *mut c_char);
        string_destroy(sig_hex_ptr as *mut c_char);
        string_destroy(nonce_hex_ptr);
        transaction_kernel_destroy(kernel);
        drop(lock);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn scanned_callback(tx: *mut TariCompletedTransaction) {
        assert!(!tx.is_null());
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::OneSidedConfirmed);
        let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
        lock.scanned_tx_callback_called = true;
        drop(lock);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn scanned_unconfirmed_callback(tx: *mut TariCompletedTransaction, _confirmations: u64) {
        assert!(!tx.is_null());
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::OneSidedUnconfirmed);
        let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
        lock.scanned_tx_unconfirmed_callback_called = true;
        let mut error = 0;
        let error_ptr = &mut error as *mut c_int;
        let kernel = completed_transaction_get_transaction_kernel(tx, error_ptr);
        let excess_hex_ptr = transaction_kernel_get_excess_hex(kernel, error_ptr);
        let excess_hex = CString::from_raw(excess_hex_ptr).to_str().unwrap().to_owned();
        assert!(!excess_hex.is_empty());
        let nonce_hex_ptr = transaction_kernel_get_excess_public_nonce_hex(kernel, error_ptr);
        let nonce_hex = CString::from_raw(nonce_hex_ptr).to_str().unwrap().to_owned();
        assert!(!nonce_hex.is_empty());
        let sig_hex_ptr = transaction_kernel_get_excess_signature_hex(kernel, error_ptr);
        let sig_hex = CString::from_raw(sig_hex_ptr).to_str().unwrap().to_owned();
        assert!(!sig_hex.is_empty());
        string_destroy(excess_hex_ptr as *mut c_char);
        string_destroy(sig_hex_ptr as *mut c_char);
        string_destroy(nonce_hex_ptr);
        transaction_kernel_destroy(kernel);
        drop(lock);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn transaction_send_result_callback(_tx_id: c_ulonglong, status: *mut TransactionSendStatus) {
        assert!(!status.is_null());
        assert_eq!(
            type_of((*status).clone()),
            std::any::type_name::<TransactionSendStatus>()
        );
        transaction_send_status_destroy(status);
    }

    unsafe extern "C" fn tx_cancellation_callback(tx: *mut TariCompletedTransaction, _reason: u64) {
        assert!(!tx.is_null());
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn txo_validation_complete_callback(_tx_id: c_ulonglong, _result: u64) {
        // assert!(true); //optimized out by compiler
    }

    unsafe extern "C" fn contacts_liveness_data_updated_callback(_balance: *mut TariContactsLivenessData) {
        // assert!(true); //optimized out by compiler
    }

    unsafe extern "C" fn balance_updated_callback(_balance: *mut TariBalance) {
        // assert!(true); //optimized out by compiler
    }

    unsafe extern "C" fn transaction_validation_complete_callback(_tx_id: c_ulonglong, _result: u64) {
        // assert!(true); //optimized out by compiler
    }

    unsafe extern "C" fn saf_messages_received_callback() {
        // assert!(true); //optimized out by compiler
    }

    unsafe extern "C" fn connectivity_status_callback(_status: u64) {
        // assert!(true); //optimized out by compiler
    }

    unsafe extern "C" fn base_node_state_callback(_state: *mut TariBaseNodeState) {
        // assert!(true); //optimized out by compiler
    }

    const NETWORK_STRING: &str = "localnet";

    #[test]
    // casting is okay in tests
    #[allow(clippy::cast_possible_truncation)]
    fn test_bytevector() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let bytes: [c_uchar; 4] = [2, 114, 34, 255];
            let bytes_ptr = byte_vector_create(bytes.as_ptr(), bytes.len() as c_uint, error_ptr);
            assert_eq!(error, 0);
            let length = byte_vector_get_length(bytes_ptr, error_ptr);
            assert_eq!(error, 0);
            assert_eq!(length, bytes.len() as c_uint);
            let byte = byte_vector_get_at(bytes_ptr, 2, error_ptr);
            assert_eq!(error, 0);
            assert_eq!(byte, bytes[2]);
            byte_vector_destroy(bytes_ptr);
        }
    }

    #[test]
    fn test_bytevector_dont_panic() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let bytes_ptr = byte_vector_create(ptr::null_mut(), 20u32, error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("bytes_ptr".to_string())).code
            );
            assert_eq!(byte_vector_get_length(bytes_ptr, error_ptr), 0);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("bytes_ptr".to_string())).code
            );
            byte_vector_destroy(bytes_ptr);
        }
    }

    #[test]
    fn test_emoji_set() {
        unsafe {
            let emoji_set = get_emoji_set();
            let compare_emoji_set = emoji::emoji_set();
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let len = emoji_set_get_length(emoji_set, error_ptr);
            assert_eq!(error, 0);
            for i in 0..len {
                let emoji_byte_vector = emoji_set_get_at(emoji_set, i as c_uint, error_ptr);
                assert_eq!(error, 0);
                let emoji_byte_vector_length = byte_vector_get_length(emoji_byte_vector, error_ptr);
                assert_eq!(error, 0);
                let mut emoji_bytes = Vec::new();
                for c in 0..emoji_byte_vector_length {
                    let byte = byte_vector_get_at(emoji_byte_vector, c as c_uint, error_ptr);
                    assert_eq!(error, 0);
                    emoji_bytes.push(byte);
                }
                let emoji = char::from_str(from_utf8(emoji_bytes.as_slice()).unwrap()).unwrap();
                let compare = compare_emoji_set[i as usize] == emoji;
                byte_vector_destroy(emoji_byte_vector);
                assert!(compare);
            }
            emoji_set_destroy(emoji_set);
        }
    }

    #[test]
    fn test_transport_type_memory() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let transport = transport_memory_create();
            let _address = transport_memory_get_address(transport, error_ptr);
            assert_eq!(error, 0);
            transport_config_destroy(transport);
        }
    }

    #[test]
    fn test_transaction_send_status() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let status = Box::into_raw(Box::new(TariTransactionSendStatus {
                direct_send_result: false,
                store_and_forward_send_result: false,
                queued_for_retry: true,
            }));
            let transaction_status = transaction_send_status_decode(status, error_ptr);
            transaction_send_status_destroy(status);
            assert_eq!(error, 0);
            assert_eq!(transaction_status, 0);

            let status = Box::into_raw(Box::new(TariTransactionSendStatus {
                direct_send_result: true,
                store_and_forward_send_result: true,
                queued_for_retry: false,
            }));
            let transaction_status = transaction_send_status_decode(status, error_ptr);
            transaction_send_status_destroy(status);
            assert_eq!(error, 0);
            assert_eq!(transaction_status, 1);

            let status = Box::into_raw(Box::new(TariTransactionSendStatus {
                direct_send_result: true,
                store_and_forward_send_result: false,
                queued_for_retry: false,
            }));
            let transaction_status = transaction_send_status_decode(status, error_ptr);
            transaction_send_status_destroy(status);
            assert_eq!(error, 0);
            assert_eq!(transaction_status, 2);

            let status = Box::into_raw(Box::new(TariTransactionSendStatus {
                direct_send_result: false,
                store_and_forward_send_result: true,
                queued_for_retry: false,
            }));
            let transaction_status = transaction_send_status_decode(status, error_ptr);
            transaction_send_status_destroy(status);
            assert_eq!(error, 0);
            assert_eq!(transaction_status, 3);

            let status = Box::into_raw(Box::new(TariTransactionSendStatus {
                direct_send_result: false,
                store_and_forward_send_result: false,
                queued_for_retry: false,
            }));
            let transaction_status = transaction_send_status_decode(status, error_ptr);
            transaction_send_status_destroy(status);
            assert_eq!(error, 1);
            assert_eq!(transaction_status, 4);

            let status = Box::into_raw(Box::new(TariTransactionSendStatus {
                direct_send_result: true,
                store_and_forward_send_result: true,
                queued_for_retry: true,
            }));
            let transaction_status = transaction_send_status_decode(status, error_ptr);
            transaction_send_status_destroy(status);
            assert_eq!(error, 1);
            assert_eq!(transaction_status, 4);

            let status = Box::into_raw(Box::new(TariTransactionSendStatus {
                direct_send_result: true,
                store_and_forward_send_result: false,
                queued_for_retry: true,
            }));
            let transaction_status = transaction_send_status_decode(status, error_ptr);
            transaction_send_status_destroy(status);
            assert_eq!(error, 1);
            assert_eq!(transaction_status, 4);

            let status = Box::into_raw(Box::new(TariTransactionSendStatus {
                direct_send_result: false,
                store_and_forward_send_result: true,
                queued_for_retry: true,
            }));
            let transaction_status = transaction_send_status_decode(status, error_ptr);
            transaction_send_status_destroy(status);
            assert_eq!(error, 1);
            assert_eq!(transaction_status, 4);
        }
    }

    #[test]
    fn test_transport_type_tcp() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let address_listener = CString::new("/ip4/127.0.0.1/tcp/0").unwrap();
            let address_listener_str: *const c_char = CString::into_raw(address_listener) as *const c_char;
            let transport = transport_tcp_create(address_listener_str, error_ptr);
            assert_eq!(error, 0);
            transport_config_destroy(transport);
        }
    }

    #[test]
    fn test_transport_type_tor() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let address_control = CString::new("/ip4/127.0.0.1/tcp/8080").unwrap();
            let mut bypass = false;
            let address_control_str: *const c_char = CString::into_raw(address_control) as *const c_char;
            let mut transport = transport_tor_create(
                address_control_str,
                ptr::null(),
                8080,
                bypass,
                ptr::null(),
                ptr::null(),
                error_ptr,
            );
            assert_eq!(error, 0);
            transport_config_destroy(transport);

            bypass = true;
            transport = transport_tor_create(
                address_control_str,
                ptr::null(),
                8080,
                bypass,
                ptr::null(),
                ptr::null(),
                error_ptr,
            );
            assert_eq!(error, 0);
            transport_config_destroy(transport);
        }
    }

    #[test]
    fn test_keys() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let private_key = private_key_generate();
            let public_key = public_key_from_private_key(private_key, error_ptr);
            assert_eq!(error, 0);
            let address = tari_address_from_private_key(private_key, 0x26, error_ptr);
            assert_eq!(error, 0);
            let private_bytes = private_key_get_bytes(private_key, error_ptr);
            assert_eq!(error, 0);
            let public_bytes = public_key_get_bytes(public_key, error_ptr);
            assert_eq!(error, 0);
            let address_bytes = tari_address_get_bytes(address, error_ptr);
            assert_eq!(error, 0);
            let private_key_length = byte_vector_get_length(private_bytes, error_ptr);
            assert_eq!(error, 0);
            let public_key_length = byte_vector_get_length(public_bytes, error_ptr);
            assert_eq!(error, 0);
            let tari_address_length = byte_vector_get_length(address_bytes, error_ptr);
            assert_eq!(error, 0);
            assert_eq!(private_key_length, 32);
            assert_eq!(public_key_length, 32);
            assert_eq!(tari_address_length, 33);
            assert_ne!((*private_bytes), (*public_bytes));
            let emoji = tari_address_to_emoji_id(address, error_ptr) as *mut c_char;
            let emoji_str = CStr::from_ptr(emoji).to_str().unwrap();
            assert!(TariAddress::from_emoji_string(emoji_str).is_ok());
            let address_emoji = emoji_id_to_tari_address(emoji, error_ptr);
            assert_eq!((*address), (*address_emoji));
            private_key_destroy(private_key);
            public_key_destroy(public_key);
            tari_address_destroy(address_emoji);
            tari_address_destroy(address);
            byte_vector_destroy(public_bytes);
            byte_vector_destroy(private_bytes);
            byte_vector_destroy(address_bytes);
        }
    }

    #[test]
    fn test_covenant_create_empty() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let covenant_bytes = Box::into_raw(Box::new(ByteVector(vec![0u8])));
            let covenant = covenant_create_from_bytes(covenant_bytes, error_ptr);

            assert_eq!(error, 0);
            let empty_covenant = covenant!();
            assert_eq!(*covenant, empty_covenant);

            covenant_destroy(covenant);
            byte_vector_destroy(covenant_bytes);
        }
    }

    #[test]
    fn test_covenant_create_filled() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let expected_covenant = covenant!(identity());
            let covenant_bytes = Box::into_raw(Box::new(ByteVector(borsh::to_vec(&expected_covenant).unwrap())));
            let covenant = covenant_create_from_bytes(covenant_bytes, error_ptr);

            assert_eq!(error, 0);
            assert_eq!(*covenant, expected_covenant);

            covenant_destroy(covenant);
            byte_vector_destroy(covenant_bytes);
        }
    }

    #[test]
    fn test_encrypted_data_empty() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let encrypted_data_bytes = Box::into_raw(Box::new(ByteVector(Vec::new())));
            let encrypted_data_1 = encrypted_data_create_from_bytes(encrypted_data_bytes, error_ptr);

            assert_ne!(error, 0);

            encrypted_data_destroy(encrypted_data_1);
            byte_vector_destroy(encrypted_data_bytes);
        }
    }

    #[test]
    fn test_encrypted_data_filled() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let spending_key = PrivateKey::random(&mut OsRng);
            let commitment = Commitment::from_public_key(&PublicKey::from_secret_key(&spending_key));
            let encryption_key = PrivateKey::random(&mut OsRng);
            let amount = MicroMinotari::from(123456);
            let encrypted_data =
                TariEncryptedOpenings::encrypt_data(&encryption_key, &commitment, amount, &spending_key).unwrap();
            let encrypted_data_bytes = encrypted_data.to_byte_vec();

            let encrypted_data_1 = Box::into_raw(Box::new(encrypted_data));
            let encrypted_data_1_as_bytes = encrypted_data_as_bytes(encrypted_data_1, error_ptr);
            assert_eq!(error, 0);

            let encrypted_data_2 = encrypted_data_create_from_bytes(encrypted_data_1_as_bytes, error_ptr);
            assert_eq!(error, 0);
            assert_eq!(*encrypted_data_1, *encrypted_data_2);

            assert_eq!((*encrypted_data_1_as_bytes).0, encrypted_data_bytes.to_vec());

            encrypted_data_destroy(encrypted_data_2);
            encrypted_data_destroy(encrypted_data_1);
            byte_vector_destroy(encrypted_data_1_as_bytes);
        }
    }

    #[test]
    // casting is okay in tests
    #[allow(clippy::cast_possible_truncation)]
    fn test_output_features_create_empty() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let version: c_uchar = 0;
            let output_type: c_ushort = 0;
            let range_proof_type: c_ushort = 0;
            let maturity: c_ulonglong = 20;
            let metadata = Box::into_raw(Box::new(ByteVector(Vec::new())));

            let output_features = output_features_create_from_bytes(
                version,
                output_type,
                maturity,
                metadata,
                range_proof_type,
                error_ptr,
            );
            assert_eq!(error, 0);
            assert_eq!((*output_features).version, OutputFeaturesVersion::V0);
            assert_eq!(
                (*output_features).output_type,
                OutputType::from_byte(output_type as u8).unwrap()
            );
            assert_eq!((*output_features).maturity, maturity);
            assert!((*output_features).coinbase_extra.is_empty());

            output_features_destroy(output_features);
            byte_vector_destroy(metadata);
        }
    }

    #[test]
    fn test_output_features_create_filled() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let version: c_uchar = OutputFeaturesVersion::V1.as_u8();
            let output_type = OutputType::Coinbase.as_byte();
            let range_proof_type = RangeProofType::RevealedValue.as_byte();
            let maturity: c_ulonglong = 20;

            let expected_metadata = vec![1; 1024];
            let metadata = Box::into_raw(Box::new(ByteVector(expected_metadata.clone())));

            let output_features = output_features_create_from_bytes(
                version,
                c_ushort::from(output_type),
                maturity,
                metadata,
                c_ushort::from(range_proof_type),
                error_ptr,
            );
            assert_eq!(error, 0);
            assert_eq!((*output_features).version, OutputFeaturesVersion::V1);
            assert_eq!(
                (*output_features).output_type,
                OutputType::from_byte(output_type).unwrap()
            );
            assert_eq!(
                (*output_features).range_proof_type,
                RangeProofType::from_byte(range_proof_type).unwrap()
            );
            assert_eq!((*output_features).maturity, maturity);
            assert_eq!((*output_features).coinbase_extra, expected_metadata);

            output_features_destroy(output_features);
            byte_vector_destroy(metadata);
        }
    }

    #[test]
    fn test_keys_dont_panic() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let private_key = private_key_create(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("bytes_ptr".to_string())).code
            );
            let public_key = public_key_from_private_key(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("secret_key_ptr".to_string())).code
            );
            let private_bytes = private_key_get_bytes(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("pk_ptr".to_string())).code
            );
            let public_bytes = public_key_get_bytes(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("pk_ptr".to_string())).code
            );
            let private_key_length = byte_vector_get_length(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("vec_ptr".to_string())).code
            );
            let public_key_length = byte_vector_get_length(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("vec_ptr".to_string())).code
            );
            assert_eq!(private_key_length, 0);
            assert_eq!(public_key_length, 0);
            private_key_destroy(private_key);
            public_key_destroy(public_key);
            byte_vector_destroy(public_bytes);
            byte_vector_destroy(private_bytes);
        }
    }

    #[test]
    fn test_contact() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let test_contact_private_key = private_key_generate();
            let test_address = tari_address_from_private_key(test_contact_private_key, 0x10, error_ptr);
            let test_str = "Test Contact";
            let test_contact_str = CString::new(test_str).unwrap();
            let test_contact_alias: *const c_char = CString::into_raw(test_contact_str) as *const c_char;
            let test_contact = contact_create(test_contact_alias, test_address, true, error_ptr);
            let favourite = contact_get_favourite(test_contact, error_ptr);
            assert!(favourite);
            let alias = contact_get_alias(test_contact, error_ptr);
            let alias_string = CString::from_raw(alias).to_str().unwrap().to_owned();
            assert_eq!(alias_string, test_str);
            let contact_address = contact_get_tari_address(test_contact, error_ptr);
            let contact_key_bytes = tari_address_get_bytes(contact_address, error_ptr);
            let contact_bytes_len = byte_vector_get_length(contact_key_bytes, error_ptr);
            assert_eq!(contact_bytes_len, 33);
            contact_destroy(test_contact);
            tari_address_destroy(test_address);
            private_key_destroy(test_contact_private_key);
            string_destroy(test_contact_alias as *mut c_char);
            byte_vector_destroy(contact_key_bytes);
        }
    }

    #[test]
    fn test_contact_dont_panic() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let test_contact_private_key = private_key_generate();
            let test_contact_address = tari_address_from_private_key(test_contact_private_key, 0x00, error_ptr);
            let test_str = "Test Contact";
            let test_contact_str = CString::new(test_str).unwrap();
            let test_contact_alias: *const c_char = CString::into_raw(test_contact_str) as *const c_char;
            let mut _test_contact = contact_create(ptr::null_mut(), test_contact_address, false, error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("alias_ptr".to_string())).code
            );
            _test_contact = contact_create(test_contact_alias, ptr::null_mut(), false, error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("public_key_ptr".to_string())).code
            );
            let _alias = contact_get_alias(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("contact_ptr".to_string())).code
            );
            let _contact_address = contact_get_tari_address(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("contact_ptr".to_string())).code
            );
            let _contact_address = contact_get_favourite(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("contact_ptr".to_string())).code
            );
            let contact_key_bytes = public_key_get_bytes(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("contact_ptr".to_string())).code
            );
            let contact_bytes_len = byte_vector_get_length(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("contact_ptr".to_string())).code
            );
            assert_eq!(contact_bytes_len, 0);
            contact_destroy(_test_contact);
            tari_address_destroy(test_contact_address);
            private_key_destroy(test_contact_private_key);
            string_destroy(test_contact_alias as *mut c_char);
            byte_vector_destroy(contact_key_bytes);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_master_private_key_persistence() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = true;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            let secret_key_alice = private_key_generate();
            let public_key_alice = public_key_from_private_key(secret_key_alice, error_ptr);
            let db_name = random::string(8);
            let db_name_alice = CString::new(db_name.as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice) as *const c_char;
            let transport_config_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_config_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;

            let sql_database_path = Path::new(alice_temp_dir.path().to_str().unwrap())
                .join(db_name)
                .with_extension("sqlite3");

            let alice_network = CString::new(NETWORK_STRING).unwrap();
            let alice_network_str: *const c_char = CString::into_raw(alice_network) as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_config_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                10800,
                error_ptr,
            );

            let passphrase: *const c_char =
                CString::into_raw(CString::new("Hello from Alasca").unwrap()) as *const c_char;

            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                alice_network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert!(!(*recovery_in_progress_ptr), "no recovery in progress");
            assert_eq!(*error_ptr, 0, "No error expected");
            wallet_destroy(alice_wallet);

            let connection =
                run_migration_and_create_sqlite_connection(&sql_database_path, 16).expect("Could not open Sqlite db");
            let wallet_backend = WalletDatabase::new(
                WalletSqliteDatabase::new(connection, "Hello from Alasca".to_string().into()).unwrap(),
            );

            let stored_seed1 = wallet_backend.get_master_seed().unwrap().unwrap();

            drop(wallet_backend);

            // Check that the same key is returned when the wallet is started a second time
            let alice_wallet2 = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                alice_network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert_eq!(error, 0);
            assert!(!(*recovery_in_progress_ptr), "no recovery in progress");

            assert_eq!(*error_ptr, 0, "No error expected");
            wallet_destroy(alice_wallet2);

            let connection =
                run_migration_and_create_sqlite_connection(&sql_database_path, 16).expect("Could not open Sqlite db");

            let passphrase = SafePassword::from("Hello from Alasca");
            let wallet_backend = WalletDatabase::new(WalletSqliteDatabase::new(connection, passphrase).unwrap());

            let stored_seed2 = wallet_backend.get_master_seed().unwrap().unwrap();

            assert_eq!(stored_seed1, stored_seed2);

            drop(wallet_backend);

            // Test the file path based version
            let backup_path_alice =
                CString::new(alice_temp_dir.path().join("backup.sqlite3").to_str().unwrap()).unwrap();
            let backup_path_alice_str: *const c_char = CString::into_raw(backup_path_alice) as *const c_char;
            let original_path_cstring = CString::new(sql_database_path.to_str().unwrap()).unwrap();
            let original_path_str: *const c_char = CString::into_raw(original_path_cstring) as *const c_char;

            let sql_database_path = alice_temp_dir.path().join("backup").with_extension("sqlite3");
            let connection =
                run_migration_and_create_sqlite_connection(sql_database_path, 16).expect("Could not open Sqlite db");
            let wallet_backend =
                WalletDatabase::new(WalletSqliteDatabase::new(connection, "holiday".to_string().into()).unwrap());

            let stored_seed = wallet_backend.get_master_seed().unwrap();

            assert!(stored_seed.is_none(), "key should be cleared");
            drop(wallet_backend);

            string_destroy(alice_network_str as *mut c_char);
            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            string_destroy(backup_path_alice_str as *mut c_char);
            string_destroy(original_path_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            public_key_destroy(public_key_alice);
            transport_config_destroy(transport_config_alice);
            comms_config_destroy(alice_config);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_wallet_client_key_value_store() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = true;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            let secret_key_alice = private_key_generate();
            let db_name_alice = CString::new(random::string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice) as *const c_char;
            let transport_config_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_config_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_config_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                10800,
                error_ptr,
            );

            let passphrase: *const c_char =
                CString::into_raw(CString::new("dolphis dancing in the coastal waters").unwrap()) as *const c_char;

            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert_eq!(error, 0);

            let client_key_values = vec![
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string()),
                ("key3".to_string(), "value3".to_string()),
            ];

            for kv in &client_key_values {
                let k = CString::new(kv.0.as_str()).unwrap();
                let k_str: *const c_char = CString::into_raw(k) as *const c_char;
                let v = CString::new(kv.1.as_str()).unwrap();
                let v_str: *const c_char = CString::into_raw(v.clone()) as *const c_char;
                assert!(wallet_set_key_value(alice_wallet, k_str, v_str, error_ptr));
                string_destroy(k_str as *mut c_char);
                string_destroy(v_str as *mut c_char);
            }

            let passphrase =
                "A pretty long passphrase that should test the hashing to a 32-bit key quite well".to_string();
            let passphrase_str = CString::new(passphrase).unwrap();
            let passphrase_const_str: *const c_char = CString::into_raw(passphrase_str) as *const c_char;

            assert_eq!(error, 0);

            for kv in &client_key_values {
                let k = CString::new(kv.0.as_str()).unwrap();
                let k_str: *const c_char = CString::into_raw(k) as *const c_char;

                let found_value = wallet_get_value(alice_wallet, k_str, error_ptr);
                let found_string = CString::from_raw(found_value).to_str().unwrap().to_owned();
                assert_eq!(found_string, kv.1.clone());
                string_destroy(k_str as *mut c_char);
            }
            let wrong_key = CString::new("Wrong").unwrap();
            let wrong_key_str: *const c_char = CString::into_raw(wrong_key) as *const c_char;
            assert!(!wallet_clear_value(alice_wallet, wrong_key_str, error_ptr));
            string_destroy(wrong_key_str as *mut c_char);

            let k = CString::new(client_key_values[0].0.as_str()).unwrap();
            let k_str: *const c_char = CString::into_raw(k) as *const c_char;
            assert!(wallet_clear_value(alice_wallet, k_str, error_ptr));

            let found_value = wallet_get_value(alice_wallet, k_str, error_ptr);
            assert_eq!(found_value, ptr::null_mut());
            assert_eq!(*error_ptr, 424i32);

            string_destroy(network_str as *mut c_char);
            string_destroy(k_str as *mut c_char);
            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            string_destroy(passphrase_const_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            transport_config_destroy(transport_config_alice);

            comms_config_destroy(alice_config);
            wallet_destroy(alice_wallet);
        }
    }

    #[test]
    pub fn test_mnemonic_word_lists() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            for language in MnemonicLanguage::iterator() {
                let language_str: *const c_char =
                    CString::into_raw(CString::new(language.to_string()).unwrap()) as *const c_char;
                let mnemonic_wordlist_ffi = seed_words_get_mnemonic_word_list_for_language(language_str, error_ptr);
                assert_eq!(error, 0);
                let mnemonic_wordlist = match *(language) {
                    TariMnemonicLanguage::ChineseSimplified => mnemonic_wordlists::MNEMONIC_CHINESE_SIMPLIFIED_WORDS,
                    TariMnemonicLanguage::English => mnemonic_wordlists::MNEMONIC_ENGLISH_WORDS,
                    TariMnemonicLanguage::French => mnemonic_wordlists::MNEMONIC_FRENCH_WORDS,
                    TariMnemonicLanguage::Italian => mnemonic_wordlists::MNEMONIC_ITALIAN_WORDS,
                    TariMnemonicLanguage::Japanese => mnemonic_wordlists::MNEMONIC_JAPANESE_WORDS,
                    TariMnemonicLanguage::Korean => mnemonic_wordlists::MNEMONIC_KOREAN_WORDS,
                    TariMnemonicLanguage::Spanish => mnemonic_wordlists::MNEMONIC_SPANISH_WORDS,
                };
                // Compare from Rust's perspective
                assert_eq!(
                    (*mnemonic_wordlist_ffi).0,
                    SeedWords::new(
                        mnemonic_wordlist
                            .to_vec()
                            .iter()
                            .map(|s| Hidden::hide(s.to_string()))
                            .collect::<Vec<Hidden<String>>>()
                    )
                );
                // Compare from C's perspective
                let count = seed_words_get_length(mnemonic_wordlist_ffi, error_ptr);
                assert_eq!(error, 0);
                for i in 0..count {
                    // Compare each word in the list
                    let mnemonic_word_ffi = CString::from_raw(seed_words_get_at(mnemonic_wordlist_ffi, i, error_ptr));
                    assert_eq!(error, 0);
                    assert_eq!(
                        mnemonic_word_ffi.to_str().unwrap().to_string(),
                        mnemonic_wordlist[i as usize].to_string()
                    );
                }
                // Try to wrongfully add a new seed word onto the mnemonic wordlist seed words object
                let w = CString::new(mnemonic_wordlist[188]).unwrap();
                let w_str: *const c_char = CString::into_raw(w) as *const c_char;
                seed_words_push_word(mnemonic_wordlist_ffi, w_str, error_ptr);
                assert_eq!(
                    seed_words_push_word(mnemonic_wordlist_ffi, w_str, error_ptr),
                    SeedWordPushResult::InvalidObject as u8
                );
                assert_ne!(error, 0);
                // Clear memory
                seed_words_destroy(mnemonic_wordlist_ffi);
            }
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    pub fn test_seed_words() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = true;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            // To create a new seed word sequence, uncomment below
            // let seed = CipherSeed::new();
            // use tari_key_manager::mnemonic::{Mnemonic, MnemonicLanguage};
            // let mnemonic_seq = seed
            //     .to_mnemonic(MnemonicLanguage::English, None)
            //     .expect("Couldn't convert CipherSeed to Mnemonic");
            // println!("{:?}", mnemonic_seq);

            let mnemonic = vec![
                "scan", "couch", "work", "water", "find", "electric", "weasel", "code", "column", "sick", "secret",
                "birth", "word", "infant", "fatigue", "upper", "vacuum", "senior", "build", "post", "lend", "electric",
                "pact", "retire",
            ];

            let seed_words = seed_words_create();

            let w = CString::new("hodl").unwrap();
            let w_str: *const c_char = CString::into_raw(w) as *const c_char;

            assert_eq!(
                seed_words_push_word(seed_words, w_str, error_ptr),
                SeedWordPushResult::InvalidSeedWord as u8
            );

            for (count, w) in mnemonic.iter().enumerate() {
                let w = CString::new(*w).unwrap();
                let w_str: *const c_char = CString::into_raw(w) as *const c_char;

                if count + 1 < 24 {
                    assert_eq!(
                        seed_words_push_word(seed_words, w_str, error_ptr),
                        SeedWordPushResult::SuccessfulPush as u8
                    );
                } else {
                    assert_eq!(
                        seed_words_push_word(seed_words, w_str, error_ptr),
                        SeedWordPushResult::SeedPhraseComplete as u8
                    );
                }
            }

            // create a new wallet
            let db_name = CString::new(random::string(8).as_str()).unwrap();
            let db_name_str: *const c_char = CString::into_raw(db_name) as *const c_char;
            let temp_dir = tempdir().unwrap();
            let db_path = CString::new(temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_str: *const c_char = CString::into_raw(db_path) as *const c_char;
            let transport_type = transport_memory_create();
            let address = transport_memory_get_address(transport_type, error_ptr);
            let address_str = CStr::from_ptr(address).to_str().unwrap().to_owned();
            let address_str = CString::new(address_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let config = comms_config_create(
                address_str,
                transport_type,
                db_name_str,
                db_path_str,
                20,
                10800,
                error_ptr,
            );

            let passphrase: *const c_char =
                CString::into_raw(CString::new("a cat outside in Istanbul").unwrap()) as *const c_char;

            let wallet = wallet_create(
                config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );

            assert_eq!(error, 0);
            let seed_words = wallet_get_seed_words(wallet, error_ptr);
            assert_eq!(error, 0);
            let public_address = wallet_get_tari_address(wallet, error_ptr);
            assert_eq!(error, 0);

            // use seed words to create recovery wallet
            let db_name = CString::new(random::string(8).as_str()).unwrap();
            let db_name_str: *const c_char = CString::into_raw(db_name) as *const c_char;
            let temp_dir = tempdir().unwrap();
            let db_path = CString::new(temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_str: *const c_char = CString::into_raw(db_path) as *const c_char;
            let transport_type = transport_memory_create();
            let address = transport_memory_get_address(transport_type, error_ptr);
            let address_str = CStr::from_ptr(address).to_str().unwrap().to_owned();
            let address_str = CString::new(address_str).unwrap().into_raw() as *const c_char;

            let config = comms_config_create(
                address_str,
                transport_type,
                db_name_str,
                db_path_str,
                20,
                10800,
                error_ptr,
            );

            let passphrase: *const c_char =
                CString::into_raw(CString::new("a wave in teahupoo").unwrap()) as *const c_char;

            let log_path: *const c_char =
                CString::into_raw(CString::new(temp_dir.path().join("asdf").to_str().unwrap()).unwrap())
                    as *const c_char;
            let recovered_wallet = wallet_create(
                config,
                log_path,
                0,
                0,
                0,
                passphrase,
                seed_words,
                network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert_eq!(error, 0);

            let recovered_seed_words = wallet_get_seed_words(recovered_wallet, error_ptr);
            assert_eq!(error, 0);
            let recovered_address = wallet_get_tari_address(recovered_wallet, error_ptr);
            assert_eq!(error, 0);

            assert_eq!(*seed_words, *recovered_seed_words);
            assert_eq!(*public_address, *recovered_address);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_wallet_get_utxos() {
        unsafe {
            let key_manager = create_memory_db_key_manager();
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = true;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            let secret_key_alice = private_key_generate();
            let db_name_alice = CString::new(random::string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice) as *const c_char;
            let transport_config_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_config_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_config_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                10800,
                error_ptr,
            );

            let passphrase: *const c_char =
                CString::into_raw(CString::new("Satoshi Nakamoto").unwrap()) as *const c_char;

            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );

            assert_eq!(error, 0);
            for i in 0..10 {
                let uout = (*alice_wallet)
                    .runtime
                    .block_on(create_test_input((1000 * i).into(), 0, &key_manager));
                (*alice_wallet)
                    .runtime
                    .block_on((*alice_wallet).wallet.output_manager_service.add_output(uout, None))
                    .unwrap();
            }

            // ascending order
            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                20,
                TariUtxoSort::ValueAsc,
                ptr::null_mut(),
                3000,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);
            assert_eq!((*outputs).len, 6);
            assert_eq!(utxos.len(), 6);
            assert!(
                utxos
                    .iter()
                    .skip(1)
                    .fold((true, utxos[0].value), |acc, x| { (acc.0 && x.value > acc.1, x.value) })
                    .0
            );
            destroy_tari_vector(outputs);

            // descending order
            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                20,
                TariUtxoSort::ValueDesc,
                ptr::null_mut(),
                3000,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);
            assert_eq!((*outputs).len, 6);
            assert_eq!(utxos.len(), 6);
            assert!(
                utxos
                    .iter()
                    .skip(1)
                    .fold((true, utxos[0].value), |acc, x| (acc.0 && x.value < acc.1, x.value))
                    .0
            );
            destroy_tari_vector(outputs);

            // result must be empty due to high dust threshold
            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                20,
                TariUtxoSort::ValueAsc,
                ptr::null_mut(),
                15000,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);
            assert_eq!((*outputs).len, 0);
            assert_eq!(utxos.len(), 0);
            destroy_tari_vector(outputs);

            string_destroy(network_str as *mut c_char);
            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            transport_config_destroy(transport_config_alice);
            comms_config_destroy(alice_config);
            wallet_destroy(alice_wallet);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_wallet_get_all_utxos() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = true;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            let secret_key_alice = private_key_generate();
            let db_name_alice = CString::new(random::string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice) as *const c_char;
            let transport_config_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_config_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_config_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                10800,
                error_ptr,
            );

            let passphrase: *const c_char =
                CString::into_raw(CString::new("J-bay open corona").unwrap()) as *const c_char;

            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert_eq!(error, 0);

            let key_manager = create_memory_db_key_manager();
            for i in 0..10 {
                let uout = (*alice_wallet)
                    .runtime
                    .block_on(create_test_input((1000 * i).into(), 0, &key_manager));
                (*alice_wallet)
                    .runtime
                    .block_on((*alice_wallet).wallet.output_manager_service.add_output(uout, None))
                    .unwrap();
            }

            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                100,
                TariUtxoSort::ValueAsc,
                ptr::null_mut(),
                0,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);

            let payload = utxos[0..3]
                .iter()
                .map(|x| CStr::from_ptr(x.commitment).to_str().unwrap().to_owned())
                .collect::<Vec<String>>();

            let commitments = Box::into_raw(Box::new(TariVector::from(payload))) as *mut TariVector;
            let result = wallet_coin_join(alice_wallet, commitments, 5, error_ptr);
            assert_eq!(error, 0);
            assert!(result > 0);

            let outputs = wallet_get_all_utxos(alice_wallet, error_ptr);
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);
            assert_eq!((*outputs).len, 11);
            assert_eq!(utxos.len(), 11);
            destroy_tari_vector(outputs);

            string_destroy(network_str as *mut c_char);
            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            transport_config_destroy(transport_config_alice);
            comms_config_destroy(alice_config);
            wallet_destroy(alice_wallet);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::needless_collect)]
    fn test_wallet_coin_join() {
        unsafe {
            let key_manager = create_memory_db_key_manager();
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = true;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            let secret_key_alice = private_key_generate();
            let db_name_alice = CString::new(random::string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice) as *const c_char;
            let transport_config_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_config_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_config_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                10800,
                error_ptr,
            );

            let passphrase: *const c_char =
                CString::into_raw(CString::new("The master and margarita").unwrap()) as *const c_char;

            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );

            assert_eq!(error, 0);
            for i in 1..=5 {
                (*alice_wallet)
                    .runtime
                    .block_on(
                        (*alice_wallet).wallet.output_manager_service.add_output(
                            (*alice_wallet)
                                .runtime
                                .block_on(create_test_input((15000 * i).into(), 0, &key_manager)),
                            None,
                        ),
                    )
                    .unwrap();
            }

            // ----------------------------------------------------------------------------
            // preview

            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                100,
                TariUtxoSort::ValueAsc,
                ptr::null_mut(),
                0,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);

            let pre_join_total_amount = utxos[0..3].iter().fold(0u64, |acc, x| acc + x.value);

            let payload = utxos[0..3]
                .iter()
                .map(|x| CStr::from_ptr(x.commitment).to_str().unwrap().to_owned())
                .collect::<Vec<String>>();

            let commitments = Box::into_raw(Box::new(TariVector::from(payload))) as *mut TariVector;
            let preview = wallet_preview_coin_join(alice_wallet, commitments, 5, error_ptr);
            assert_eq!(error, 0);

            // ----------------------------------------------------------------------------
            // join

            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                100,
                TariUtxoSort::ValueAsc,
                ptr::null_mut(),
                0,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);

            let payload = utxos[0..3]
                .iter()
                .map(|x| CStr::from_ptr(x.commitment).to_str().unwrap().to_owned())
                .collect::<Vec<String>>();

            let commitments = Box::into_raw(Box::new(TariVector::from(payload))) as *mut TariVector;
            let result = wallet_coin_join(alice_wallet, commitments, 5, error_ptr);
            assert_eq!(error, 0);
            assert!(result > 0);

            let unspent_outputs = (*alice_wallet)
                .wallet
                .output_db
                .fetch_outputs_by(OutputBackendQuery {
                    status: vec![OutputStatus::Unspent],
                    ..Default::default()
                })
                .unwrap()
                .into_iter()
                .map(|x| x.wallet_output.value)
                .collect::<Vec<MicroMinotari>>();

            let new_pending_outputs = (*alice_wallet)
                .wallet
                .output_db
                .fetch_outputs_by(OutputBackendQuery {
                    status: vec![OutputStatus::EncumberedToBeReceived],
                    ..Default::default()
                })
                .unwrap()
                .into_iter()
                .map(|x| x.wallet_output.value)
                .collect::<Vec<MicroMinotari>>();

            let post_join_total_amount = new_pending_outputs.iter().fold(0u64, |acc, x| acc + x.as_u64());
            let expected_output_values: Vec<u64> = Vec::from_raw_parts(
                (*(*preview).expected_outputs).ptr as *mut u64,
                (*(*preview).expected_outputs).len,
                (*(*preview).expected_outputs).cap,
            );

            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                20,
                TariUtxoSort::ValueAsc,
                Box::into_raw(Box::new(TariVector::from(vec![OutputStatus::Unspent]))),
                0,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);
            assert_eq!(utxos.len(), 2);
            assert_eq!(unspent_outputs.len(), 2);

            // lengths
            assert_eq!(new_pending_outputs.len(), 1);
            assert_eq!(new_pending_outputs.len(), expected_output_values.len());

            // comparing result with expected
            assert_eq!(new_pending_outputs[0].as_u64(), expected_output_values[0]);

            // checking fee
            assert_eq!(pre_join_total_amount - post_join_total_amount, (*preview).fee);

            destroy_tari_vector(outputs);
            destroy_tari_vector(commitments);
            destroy_tari_coin_preview(preview);

            string_destroy(network_str as *mut c_char);
            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            transport_config_destroy(transport_config_alice);
            comms_config_destroy(alice_config);
            wallet_destroy(alice_wallet);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::needless_collect)]
    fn test_wallet_coin_split() {
        unsafe {
            let key_manager = create_memory_db_key_manager();
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = true;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            let secret_key_alice = private_key_generate();
            let db_name_alice = CString::new(random::string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice) as *const c_char;
            let transport_config_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_config_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_config_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                10800,
                error_ptr,
            );

            let passphrase: *const c_char = CString::into_raw(CString::new("niao").unwrap()) as *const c_char;

            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert_eq!(error, 0);
            for i in 1..=5 {
                (*alice_wallet)
                    .runtime
                    .block_on(
                        (*alice_wallet).wallet.output_manager_service.add_output(
                            (*alice_wallet)
                                .runtime
                                .block_on(create_test_input((15000 * i).into(), 0, &key_manager)),
                            None,
                        ),
                    )
                    .unwrap();
            }

            // ----------------------------------------------------------------------------
            // preview

            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                100,
                TariUtxoSort::ValueAsc,
                ptr::null_mut(),
                0,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);

            let pre_split_total_amount = utxos[0..3].iter().fold(0u64, |acc, x| acc + x.value);

            let payload = utxos[0..3]
                .iter()
                .map(|x| CStr::from_ptr(x.commitment).to_str().unwrap().to_owned())
                .collect::<Vec<String>>();

            let commitments = Box::into_raw(Box::new(TariVector::from(payload))) as *mut TariVector;

            let preview = wallet_preview_coin_split(alice_wallet, commitments, 3, 5, error_ptr);
            assert_eq!(error, 0);
            destroy_tari_vector(commitments);

            // ----------------------------------------------------------------------------
            // split

            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                100,
                TariUtxoSort::ValueAsc,
                ptr::null_mut(),
                0,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);

            let payload = utxos[0..3]
                .iter()
                .map(|x| CStr::from_ptr(x.commitment).to_str().unwrap().to_owned())
                .collect::<Vec<String>>();

            let commitments = Box::into_raw(Box::new(TariVector::from(payload))) as *mut TariVector;

            let result = wallet_coin_split(alice_wallet, commitments, 3, 5, error_ptr);
            assert_eq!(error, 0);
            assert!(result > 0);

            let unspent_outputs = (*alice_wallet)
                .wallet
                .output_db
                .fetch_outputs_by(OutputBackendQuery {
                    status: vec![OutputStatus::Unspent],
                    ..Default::default()
                })
                .unwrap()
                .into_iter()
                .map(|x| x.wallet_output.value)
                .collect::<Vec<_>>();

            let new_pending_outputs = (*alice_wallet)
                .wallet
                .output_db
                .fetch_outputs_by(OutputBackendQuery {
                    status: vec![OutputStatus::EncumberedToBeReceived],
                    ..Default::default()
                })
                .unwrap()
                .into_iter()
                .map(|x| x.wallet_output.value)
                .collect::<Vec<_>>();

            let post_split_total_amount = new_pending_outputs.iter().fold(0u64, |acc, x| acc + x.as_u64());
            let expected_output_values: Vec<u64> = Vec::from_raw_parts(
                (*(*preview).expected_outputs).ptr as *mut u64,
                (*(*preview).expected_outputs).len,
                (*(*preview).expected_outputs).cap,
            );

            let outputs = wallet_get_utxos(
                alice_wallet,
                0,
                20,
                TariUtxoSort::ValueAsc,
                Box::into_raw(Box::new(TariVector::from(vec![OutputStatus::Unspent]))),
                0,
                error_ptr,
            );
            let utxos: &[TariUtxo] = slice::from_raw_parts_mut((*outputs).ptr as *mut TariUtxo, (*outputs).len);
            assert_eq!(error, 0);
            assert_eq!(utxos.len(), 2);
            assert_eq!(unspent_outputs.len(), 2);

            // lengths
            assert_eq!(new_pending_outputs.len(), 3);
            assert_eq!(new_pending_outputs.len(), expected_output_values.len());

            // comparing resulting output values relative to itself
            assert_eq!(new_pending_outputs[0], new_pending_outputs[1]);
            assert_eq!(new_pending_outputs[2], new_pending_outputs[1] + MicroMinotari(1));

            // comparing resulting output values to the expected
            assert_eq!(new_pending_outputs[0].as_u64(), expected_output_values[0]);
            assert_eq!(new_pending_outputs[1].as_u64(), expected_output_values[1]);
            assert_eq!(new_pending_outputs[2].as_u64(), expected_output_values[2]);

            // checking fee
            assert_eq!(pre_split_total_amount - post_split_total_amount, (*preview).fee);

            destroy_tari_vector(outputs);
            destroy_tari_vector(commitments);
            destroy_tari_coin_preview(preview);

            string_destroy(network_str as *mut c_char);
            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            transport_config_destroy(transport_config_alice);
            comms_config_destroy(alice_config);
            wallet_destroy(alice_wallet);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::needless_collect)]
    fn test_wallet_get_network_and_version() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = true;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            let secret_key_alice = private_key_generate();
            let db_name_alice = CString::new(random::string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice) as *const c_char;
            let transport_config_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_config_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_config_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                10800,
                error_ptr,
            );

            let passphrase: *const c_char = CString::into_raw(CString::new("niao").unwrap()) as *const c_char;

            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert_eq!(error, 0);

            let key_manager = create_memory_db_key_manager();
            for i in 1..=5 {
                (*alice_wallet)
                    .runtime
                    .block_on(
                        (*alice_wallet).wallet.output_manager_service.add_output(
                            (*alice_wallet)
                                .runtime
                                .block_on(create_test_input((15000 * i).into(), 0, &key_manager)),
                            None,
                        ),
                    )
                    .unwrap();
            }

            // obtaining network and version
            let _ = wallet_get_last_version(alice_config, &mut error as *mut c_int);
            let _ = wallet_get_last_network(alice_config, &mut error as *mut c_int);

            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            transport_config_destroy(transport_config_alice);
            comms_config_destroy(alice_config);
            wallet_destroy(alice_wallet);
        }
    }

    #[test]
    fn test_tari_vector() {
        let mut error = 0;

        unsafe {
            let tv = create_tari_vector(TariTypeTag::Text);
            assert_eq!((*tv).tag, TariTypeTag::Text);
            assert_eq!((*tv).len, 0);
            assert_eq!((*tv).cap, 2);

            tari_vector_push_string(
                tv,
                CString::new("test string 1").unwrap().into_raw() as *const c_char,
                &mut error as *mut c_int,
            );
            assert_eq!(error, 0);
            assert_eq!((*tv).tag, TariTypeTag::Text);
            assert_eq!((*tv).len, 1);
            assert_eq!((*tv).cap, 1);

            tari_vector_push_string(
                tv,
                CString::new("test string 2").unwrap().into_raw() as *const c_char,
                &mut error as *mut c_int,
            );
            assert_eq!(error, 0);
            assert_eq!((*tv).tag, TariTypeTag::Text);
            assert_eq!((*tv).len, 2);
            assert_eq!((*tv).cap, 2);

            tari_vector_push_string(
                tv,
                CString::new("test string 3").unwrap().into_raw() as *const c_char,
                &mut error as *mut c_int,
            );
            assert_eq!(error, 0);
            assert_eq!((*tv).tag, TariTypeTag::Text);
            assert_eq!((*tv).len, 3);
            assert_eq!((*tv).cap, 3);

            destroy_tari_vector(tv);
        }
    }

    #[test]
    fn test_com_pub_sig_create() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let (a_value, ephemeral_pubkey) = PublicKey::random_keypair(&mut OsRng);
            let (x_value, ephemeral_com) = PublicKey::random_keypair(&mut OsRng);
            let (y_value, _) = PublicKey::random_keypair(&mut OsRng);
            let ephemeral_com = Commitment::from_public_key(&ephemeral_com);

            let a_bytes = Box::into_raw(Box::new(ByteVector(a_value.to_vec())));
            let x_bytes = Box::into_raw(Box::new(ByteVector(x_value.to_vec())));
            let y_bytes = Box::into_raw(Box::new(ByteVector(y_value.to_vec())));
            let ephemeral_pubkey_bytes = Box::into_raw(Box::new(ByteVector(ephemeral_pubkey.to_vec())));
            let ephemeral_com_bytes = Box::into_raw(Box::new(ByteVector(ephemeral_com.to_vec())));

            let sig = commitment_and_public_signature_create_from_bytes(
                ephemeral_com_bytes,
                ephemeral_pubkey_bytes,
                a_bytes,
                x_bytes,
                y_bytes,
                error_ptr,
            );

            assert_eq!(error, 0);
            assert_eq!(*(*sig).ephemeral_commitment(), ephemeral_com);
            assert_eq!(*(*sig).ephemeral_pubkey(), ephemeral_pubkey);
            assert_eq!(*(*sig).u_a(), a_value);
            assert_eq!(*(*sig).u_x(), x_value);
            assert_eq!(*(*sig).u_y(), y_value);

            commitment_and_public_signature_destroy(sig);
            byte_vector_destroy(ephemeral_com_bytes);
            byte_vector_destroy(ephemeral_pubkey_bytes);
            byte_vector_destroy(a_bytes);
            byte_vector_destroy(x_bytes);
            byte_vector_destroy(y_bytes);
        }
    }

    #[test]
    pub fn test_create_external_utxo() {
        let runtime = Runtime::new().unwrap();
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            // Test the consistent features case
            let key_manager = create_memory_db_key_manager();
            let utxo_1 = runtime
                .block_on(create_wallet_output_with_data(
                    script!(Nop),
                    OutputFeatures::default(),
                    &runtime.block_on(TestParams::new(&key_manager)),
                    MicroMinotari(1234u64),
                    &key_manager,
                ))
                .unwrap();
            let amount = utxo_1.value.as_u64();
            let spending_key = runtime
                .block_on(key_manager.get_private_key(&utxo_1.spending_key_id))
                .unwrap();
            let script_private_key = runtime
                .block_on(key_manager.get_private_key(&utxo_1.script_key_id))
                .unwrap();
            let spending_key_ptr = Box::into_raw(Box::new(spending_key));
            let features_ptr = Box::into_raw(Box::new(utxo_1.features.clone()));
            let metadata_signature_ptr = Box::into_raw(Box::new(utxo_1.metadata_signature.clone()));
            let sender_offset_public_key_ptr = Box::into_raw(Box::new(utxo_1.sender_offset_public_key.clone()));
            let script_private_key_ptr = Box::into_raw(Box::new(script_private_key));
            let covenant_ptr = Box::into_raw(Box::new(utxo_1.covenant.clone()));
            let encrypted_data_ptr = Box::into_raw(Box::new(utxo_1.encrypted_data));
            let minimum_value_promise = utxo_1.minimum_value_promise.as_u64();
            let script_ptr = CString::into_raw(CString::new(script!(Nop).to_hex()).unwrap()) as *const c_char;
            let input_data_ptr = CString::into_raw(CString::new(utxo_1.input_data.to_hex()).unwrap()) as *const c_char;

            let tari_utxo = create_tari_unblinded_output(
                amount,
                spending_key_ptr,
                features_ptr,
                script_ptr,
                input_data_ptr,
                metadata_signature_ptr,
                sender_offset_public_key_ptr,
                script_private_key_ptr,
                covenant_ptr,
                encrypted_data_ptr,
                minimum_value_promise,
                0,
                error_ptr,
            );

            assert_eq!(error, 0);
            assert_eq!((*tari_utxo).sender_offset_public_key, utxo_1.sender_offset_public_key);
            tari_unblinded_output_destroy(tari_utxo);

            // Cleanup
            string_destroy(script_ptr as *mut c_char);
            string_destroy(input_data_ptr as *mut c_char);
            let _covenant = Box::from_raw(covenant_ptr);
            let _script_private_key = Box::from_raw(script_private_key_ptr);
            let _sender_offset_public_key = Box::from_raw(sender_offset_public_key_ptr);
            let _metadata_signature = Box::from_raw(metadata_signature_ptr);
            let _features = Box::from_raw(features_ptr);
            let _spending_key = Box::from_raw(spending_key_ptr);
        }
    }

    fn get_next_memory_address() -> Multiaddr {
        let port = MemoryTransport::acquire_next_memsocket_port();
        format!("/memory/{}", port).parse().unwrap()
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    pub fn test_import_external_utxo() {
        let runtime = Runtime::new().unwrap();
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = true;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            // create a new wallet
            let db_name = CString::new(random::string(8).as_str()).unwrap();
            let db_name_str: *const c_char = CString::into_raw(db_name) as *const c_char;
            let temp_dir = tempdir().unwrap();
            let db_path = CString::new(temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_str: *const c_char = CString::into_raw(db_path) as *const c_char;
            let transport_type = transport_memory_create();
            let address = transport_memory_get_address(transport_type, error_ptr);
            let address_str = CStr::from_ptr(address).to_str().unwrap().to_owned();
            let address_str = CString::new(address_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let config = comms_config_create(
                address_str,
                transport_type,
                db_name_str,
                db_path_str,
                20,
                10800,
                error_ptr,
            );
            let passphrase: *const c_char = CString::into_raw(CString::new("niao").unwrap()) as *const c_char;
            let wallet_ptr = wallet_create(
                config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert_eq!(error, 0);
            let node_identity =
                NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
            let base_node_peer_public_key_ptr = Box::into_raw(Box::new(node_identity.public_key().clone()));
            let base_node_peer_address_ptr =
                CString::into_raw(CString::new(node_identity.first_public_address().unwrap().to_string()).unwrap())
                    as *const c_char;
            wallet_add_base_node_peer(
                wallet_ptr,
                base_node_peer_public_key_ptr,
                base_node_peer_address_ptr,
                error_ptr,
            );

            // Test the consistent features case
            let key_manager = create_memory_db_key_manager();
            let utxo_1 = runtime
                .block_on(create_wallet_output_with_data(
                    script!(Nop),
                    OutputFeatures::default(),
                    &runtime.block_on(TestParams::new(&key_manager)),
                    MicroMinotari(1234u64),
                    &key_manager,
                ))
                .unwrap();
            let amount = utxo_1.value.as_u64();

            let spending_key = runtime
                .block_on(key_manager.get_private_key(&utxo_1.spending_key_id))
                .unwrap();
            let script_private_key = runtime
                .block_on(key_manager.get_private_key(&utxo_1.script_key_id))
                .unwrap();
            let spending_key_ptr = Box::into_raw(Box::new(spending_key));
            let features_ptr = Box::into_raw(Box::new(utxo_1.features.clone()));
            let source_address_ptr = Box::into_raw(Box::default());
            let metadata_signature_ptr = Box::into_raw(Box::new(utxo_1.metadata_signature.clone()));
            let sender_offset_public_key_ptr = Box::into_raw(Box::new(utxo_1.sender_offset_public_key.clone()));
            let script_private_key_ptr = Box::into_raw(Box::new(script_private_key));
            let covenant_ptr = Box::into_raw(Box::new(utxo_1.covenant.clone()));
            let encrypted_data_ptr = Box::into_raw(Box::new(utxo_1.encrypted_data));
            let minimum_value_promise = utxo_1.minimum_value_promise.as_u64();
            let message_ptr = CString::into_raw(CString::new("For my friend").unwrap()) as *const c_char;
            let script_ptr = CString::into_raw(CString::new(script!(Nop).to_hex()).unwrap()) as *const c_char;
            let input_data_ptr = CString::into_raw(CString::new(utxo_1.input_data.to_hex()).unwrap()) as *const c_char;

            let tari_utxo = create_tari_unblinded_output(
                amount,
                spending_key_ptr,
                features_ptr,
                script_ptr,
                input_data_ptr,
                metadata_signature_ptr,
                sender_offset_public_key_ptr,
                script_private_key_ptr,
                covenant_ptr,
                encrypted_data_ptr,
                minimum_value_promise,
                0,
                error_ptr,
            );
            let tx_id = wallet_import_external_utxo_as_non_rewindable(
                wallet_ptr,
                tari_utxo,
                source_address_ptr,
                message_ptr,
                error_ptr,
            );

            assert_eq!(error, 0);
            assert!(tx_id > 0);

            let outputs = wallet_get_unspent_outputs(wallet_ptr, error_ptr);
            assert_eq!((*outputs).0.len(), 0);
            assert_eq!(unblinded_outputs_get_length(outputs, error_ptr), 0);

            // Cleanup
            tari_unblinded_output_destroy(tari_utxo);
            unblinded_outputs_destroy(outputs);
            string_destroy(message_ptr as *mut c_char);
            string_destroy(script_ptr as *mut c_char);
            string_destroy(input_data_ptr as *mut c_char);
            let _covenant = Box::from_raw(covenant_ptr);
            let _script_private_key = Box::from_raw(script_private_key_ptr);
            let _sender_offset_public_key = Box::from_raw(sender_offset_public_key_ptr);
            let _metadata_signature = Box::from_raw(metadata_signature_ptr);
            let _features = Box::from_raw(features_ptr);
            let _source_address = Box::from_raw(source_address_ptr);
            let _spending_key = Box::from_raw(spending_key_ptr);

            let _base_node_peer_public_key = Box::from_raw(base_node_peer_public_key_ptr);
            string_destroy(base_node_peer_address_ptr as *mut c_char);

            string_destroy(network_str as *mut c_char);
            string_destroy(db_name_str as *mut c_char);
            string_destroy(db_path_str as *mut c_char);
            string_destroy(address_str as *mut c_char);
            transport_config_destroy(transport_type);

            comms_config_destroy(config);
            wallet_destroy(wallet_ptr);
        }
    }

    #[test]
    pub fn test_utxo_json() {
        let runtime = Runtime::new().unwrap();
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let key_manager = create_memory_db_key_manager();
            let utxo_1 = runtime
                .block_on(create_wallet_output_with_data(
                    script!(Nop),
                    OutputFeatures::default(),
                    &runtime.block_on(TestParams::new(&key_manager)),
                    MicroMinotari(1234u64),
                    &key_manager,
                ))
                .unwrap();
            let amount = utxo_1.value.as_u64();
            let spending_key = runtime
                .block_on(key_manager.get_private_key(&utxo_1.spending_key_id))
                .unwrap();
            let script_private_key = runtime
                .block_on(key_manager.get_private_key(&utxo_1.script_key_id))
                .unwrap();
            let spending_key_ptr = Box::into_raw(Box::new(spending_key));
            let features_ptr = Box::into_raw(Box::new(utxo_1.features.clone()));
            let source_address_ptr = Box::into_raw(Box::<TariWalletAddress>::default());
            let metadata_signature_ptr = Box::into_raw(Box::new(utxo_1.metadata_signature.clone()));
            let sender_offset_public_key_ptr = Box::into_raw(Box::new(utxo_1.sender_offset_public_key.clone()));
            let script_private_key_ptr = Box::into_raw(Box::new(script_private_key));
            let covenant_ptr = Box::into_raw(Box::new(utxo_1.covenant.clone()));
            let encrypted_data_ptr = Box::into_raw(Box::new(utxo_1.encrypted_data));
            let minimum_value_promise = utxo_1.minimum_value_promise.as_u64();
            let message_ptr = CString::into_raw(CString::new("For my friend").unwrap()) as *const c_char;
            let script_ptr = CString::into_raw(CString::new(script!(Nop).to_hex()).unwrap()) as *const c_char;
            let input_data_ptr = CString::into_raw(CString::new(utxo_1.input_data.to_hex()).unwrap()) as *const c_char;

            let tari_utxo = create_tari_unblinded_output(
                amount,
                spending_key_ptr,
                features_ptr,
                script_ptr,
                input_data_ptr,
                metadata_signature_ptr,
                sender_offset_public_key_ptr,
                script_private_key_ptr,
                covenant_ptr,
                encrypted_data_ptr,
                minimum_value_promise,
                0,
                error_ptr,
            );
            let json_string = tari_unblinded_output_to_json(tari_utxo, error_ptr);
            assert_eq!(error, 0);
            let tari_utxo2 = create_tari_unblinded_output_from_json(json_string, error_ptr);
            assert_eq!(error, 0);
            assert_eq!(*tari_utxo, *tari_utxo2);
            // Cleanup
            tari_unblinded_output_destroy(tari_utxo);
            tari_unblinded_output_destroy(tari_utxo2);
            string_destroy(message_ptr as *mut c_char);
            string_destroy(script_ptr as *mut c_char);
            string_destroy(input_data_ptr as *mut c_char);
            let _covenant = Box::from_raw(covenant_ptr);
            let _script_private_key = Box::from_raw(script_private_key_ptr);
            let _sender_offset_public_key = Box::from_raw(sender_offset_public_key_ptr);
            let _metadata_signature = Box::from_raw(metadata_signature_ptr);
            let _features = Box::from_raw(features_ptr);
            let _source_address = Box::from_raw(source_address_ptr);
            let _spending_key = Box::from_raw(spending_key_ptr);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    pub fn test_wallet_shutdown() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let mut recovery_in_progress = false;
            let recovery_in_progress_ptr = &mut recovery_in_progress as *mut bool;

            // Create a new wallet for Alice
            let db_name = CString::new(random::string(8).as_str()).unwrap();
            let alice_db_name_str: *const c_char = CString::into_raw(db_name) as *const c_char;
            let temp_dir = tempdir().unwrap();
            let db_path = CString::new(temp_dir.path().to_str().unwrap()).unwrap();
            let alice_db_path_str: *const c_char = CString::into_raw(db_path) as *const c_char;
            let alice_transport_type = transport_memory_create();
            let address = transport_memory_get_address(alice_transport_type, error_ptr);
            let address_str = CStr::from_ptr(address).to_str().unwrap().to_owned();
            let alice_address_str = CString::new(address_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let alice_network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let alice_config = comms_config_create(
                alice_address_str,
                alice_transport_type,
                alice_db_name_str,
                alice_db_path_str,
                20,
                10800,
                error_ptr,
            );
            let passphrase: *const c_char = CString::into_raw(CString::new("niao").unwrap()) as *const c_char;
            let alice_wallet_ptr = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                alice_network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert_eq!(error, 0);
            string_destroy(alice_network_str as *mut c_char);
            string_destroy(alice_db_name_str as *mut c_char);
            string_destroy(alice_db_path_str as *mut c_char);
            string_destroy(alice_address_str as *mut c_char);
            transport_config_destroy(alice_transport_type);
            comms_config_destroy(alice_config);

            // Create a new wallet for bob
            let db_name = CString::new(random::string(8).as_str()).unwrap();
            let bob_db_name_str: *const c_char = CString::into_raw(db_name) as *const c_char;
            let temp_dir = tempdir().unwrap();
            let db_path = CString::new(temp_dir.path().to_str().unwrap()).unwrap();
            let bob_db_path_str: *const c_char = CString::into_raw(db_path) as *const c_char;
            let bob_transport_type = transport_memory_create();
            let address = transport_memory_get_address(bob_transport_type, error_ptr);
            let address_str = CStr::from_ptr(address).to_str().unwrap().to_owned();
            let bob_address_str = CString::new(address_str).unwrap().into_raw() as *const c_char;
            let network = CString::new(NETWORK_STRING).unwrap();
            let bob_network_str: *const c_char = CString::into_raw(network) as *const c_char;

            let bob_config = comms_config_create(
                bob_address_str,
                bob_transport_type,
                bob_db_name_str,
                bob_db_path_str,
                20,
                10800,
                error_ptr,
            );
            let passphrase: *const c_char = CString::into_raw(CString::new("niao").unwrap()) as *const c_char;
            let bob_wallet_ptr = wallet_create(
                bob_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                bob_network_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                mined_unconfirmed_callback,
                scanned_callback,
                scanned_unconfirmed_callback,
                transaction_send_result_callback,
                tx_cancellation_callback,
                txo_validation_complete_callback,
                contacts_liveness_data_updated_callback,
                balance_updated_callback,
                transaction_validation_complete_callback,
                saf_messages_received_callback,
                connectivity_status_callback,
                base_node_state_callback,
                recovery_in_progress_ptr,
                error_ptr,
            );
            assert_eq!(error, 0);
            string_destroy(bob_network_str as *mut c_char);
            string_destroy(bob_db_name_str as *mut c_char);
            string_destroy(bob_db_path_str as *mut c_char);
            string_destroy(bob_address_str as *mut c_char);
            transport_config_destroy(bob_transport_type);
            comms_config_destroy(bob_config);

            // Add some peers
            // - Wallet peer for Alice (add Bob as a base node peer; not how it will be done in production but good
            //   enough for the test as we just need to make sure the wallet can connect to a peer)
            let bob_wallet_comms = (*bob_wallet_ptr).wallet.comms.clone();
            let bob_node_identity = bob_wallet_comms.node_identity();
            let bob_peer_public_key_ptr = Box::into_raw(Box::new(bob_node_identity.public_key().clone()));
            let bob_peer_address_ptr =
                CString::into_raw(CString::new(bob_node_identity.first_public_address().unwrap().to_string()).unwrap())
                    as *const c_char;
            wallet_add_base_node_peer(
                alice_wallet_ptr,
                bob_peer_public_key_ptr,
                bob_peer_address_ptr,
                error_ptr,
            );
            string_destroy(bob_peer_address_ptr as *mut c_char);
            let _destroyed = Box::from_raw(bob_peer_public_key_ptr);
            // - Wallet peer for Bob (add Alice as a base node peer; same as above)
            let alice_wallet_comms = (*alice_wallet_ptr).wallet.comms.clone();
            let alice_node_identity = alice_wallet_comms.node_identity();
            let alice_peer_public_key_ptr = Box::into_raw(Box::new(alice_node_identity.public_key().clone()));
            let alice_peer_address_ptr = CString::into_raw(
                CString::new(alice_node_identity.first_public_address().unwrap().to_string()).unwrap(),
            ) as *const c_char;
            wallet_add_base_node_peer(
                bob_wallet_ptr,
                alice_peer_public_key_ptr,
                alice_peer_address_ptr,
                error_ptr,
            );
            string_destroy(alice_peer_address_ptr as *mut c_char);
            let _destroyed = Box::from_raw(alice_peer_public_key_ptr);

            // Add some contacts
            // - Contact for Alice
            let bob_wallet_address = TariWalletAddress::new(bob_node_identity.public_key().clone(), Network::LocalNet);
            let alice_contact_alias_ptr: *const c_char =
                CString::into_raw(CString::new("bob").unwrap()) as *const c_char;
            let alice_contact_address_ptr = Box::into_raw(Box::new(bob_wallet_address.clone()));
            let alice_contact_ptr = contact_create(alice_contact_alias_ptr, alice_contact_address_ptr, true, error_ptr);
            tari_address_destroy(alice_contact_address_ptr);
            assert!(wallet_upsert_contact(alice_wallet_ptr, alice_contact_ptr, error_ptr));
            contact_destroy(alice_contact_ptr);
            // - Contact for Bob
            let alice_wallet_address =
                TariWalletAddress::new(alice_node_identity.public_key().clone(), Network::LocalNet);
            let bob_contact_alias_ptr: *const c_char =
                CString::into_raw(CString::new("alice").unwrap()) as *const c_char;
            let bob_contact_address_ptr = Box::into_raw(Box::new(alice_wallet_address.clone()));
            let bob_contact_ptr = contact_create(bob_contact_alias_ptr, bob_contact_address_ptr, true, error_ptr);
            tari_address_destroy(bob_contact_address_ptr);
            assert!(wallet_upsert_contact(bob_wallet_ptr, bob_contact_ptr, error_ptr));
            contact_destroy(bob_contact_ptr);

            // Use comms service - do `dial_peer` for both wallets (we do not 'assert!' here to not make the test flaky)
            // Note: This loop is just to make sure we actually connect as the first attempts do not always succeed
            let alice_wallet_runtime = &(*alice_wallet_ptr).runtime;
            let bob_wallet_runtime = &(*bob_wallet_ptr).runtime;
            let mut alice_dialed_bob = false;
            let mut bob_dialed_alice = false;
            let mut dial_count = 0;
            loop {
                dial_count += 1;
                if !alice_dialed_bob {
                    alice_dialed_bob = alice_wallet_runtime
                        .block_on(
                            alice_wallet_comms
                                .connectivity()
                                .dial_peer(bob_node_identity.node_id().clone()),
                        )
                        .is_ok();
                }
                if !bob_dialed_alice {
                    bob_dialed_alice = bob_wallet_runtime
                        .block_on(
                            bob_wallet_comms
                                .connectivity()
                                .dial_peer(alice_node_identity.node_id().clone()),
                        )
                        .is_ok();
                }
                if alice_dialed_bob && bob_dialed_alice || dial_count > 10 {
                    break;
                }
                // Wait a bit before the next attempt
                alice_wallet_runtime.block_on(async { tokio::time::sleep(Duration::from_millis(500)).await });
            }

            // Use contacts service - send some messages for both wallets
            let mut alice_wallet_contacts_service = (*alice_wallet_ptr).wallet.contacts_service.clone();
            let mut bob_wallet_contacts_service = (*bob_wallet_ptr).wallet.contacts_service.clone();
            let mut alice_msg_count = 0;
            let mut bob_msg_count = 0;
            // Note: This loop is just to make sure we actually send a couple of messages as the first attempts do not
            // always succeed (we do not 'assert!' here to not make the test flaky)
            for i in 0..60 {
                if alice_msg_count < 5 {
                    let alice_message_result =
                        alice_wallet_runtime.block_on(alice_wallet_contacts_service.send_message(Message {
                            body: vec![i],
                            metadata: vec![MessageMetadata::default()],
                            address: bob_wallet_address.clone(),
                            direction: Direction::Outbound,
                            stored_at: u64::from(i),
                            delivery_confirmation_at: None,
                            read_confirmation_at: None,
                            message_id: vec![i],
                        }));
                    if alice_message_result.is_ok() {
                        alice_msg_count += 1;
                    }
                }
                if bob_msg_count < 5 {
                    let bob_message_result =
                        bob_wallet_runtime.block_on(bob_wallet_contacts_service.send_message(Message {
                            body: vec![i],
                            metadata: vec![MessageMetadata::default()],
                            address: alice_wallet_address.clone(),
                            direction: Direction::Outbound,
                            stored_at: u64::from(i),
                            delivery_confirmation_at: None,
                            read_confirmation_at: None,
                            message_id: vec![i],
                        }));
                    if bob_message_result.is_ok() {
                        bob_msg_count += 1;
                    }
                }
                if alice_msg_count >= 5 && bob_msg_count >= 5 {
                    break;
                }
                // Wait a bit before the next attempt
                alice_wallet_runtime.block_on(async { tokio::time::sleep(Duration::from_millis(500)).await });
            }

            // Trigger Alice wallet shutdown (same as `pub unsafe extern "C" fn wallet_destroy(wallet: *mut TariWallet)`
            wallet_destroy(alice_wallet_ptr);

            // Bob's peer connection to Alice will still be active for a short while until Bob figures out Alice is
            // gone, and a 'dial_peer' command to Alice from Bob may return the previous connection state, but it
            // should not be possible to do anything with the connection.
            let bob_comms_dial_peer = bob_wallet_runtime.block_on(
                bob_wallet_comms
                    .connectivity()
                    .dial_peer(alice_node_identity.node_id().clone()),
            );
            if let Ok(mut connection_to_alice) = bob_comms_dial_peer {
                if bob_wallet_runtime
                    .block_on(connection_to_alice.open_substream(&MESSAGING_PROTOCOL_ID.clone()))
                    .is_ok()
                {
                    panic!("Connection to Alice should not be active!");
                }
            }

            // - Bob can still retrieve messages Alice sent
            let bob_contacts_get_messages =
                bob_wallet_runtime.block_on(bob_wallet_contacts_service.get_messages(alice_wallet_address, 1, 1));
            assert!(bob_contacts_get_messages.is_ok());

            // Cleanup
            wallet_destroy(bob_wallet_ptr);
        }
    }
}
