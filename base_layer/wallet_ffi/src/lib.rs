// Copyright 2019. The Tari Project
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

//! # LibWallet API Definition
//! This module contains the Rust backend implementations of the functionality that a wallet for the Tari Base Layer
//! will require. The module contains a number of sub-modules that are implemented as async services. These services are
//! collected into the main Wallet container struct which manages spinning up all the component services and maintains a
//! collection of the handles required to interact with those services.
//! This files contians the API calls that will be exposed to external systems that make use of this module. The API
//! will be exposed via FFI and will consist of API calls that the FFI client can make into the Wallet module and a set
//! of Callbacks that the client must implement and provide to the Wallet module to receive asynchronous replies and
//! updates.
extern crate libc;
extern crate tari_wallet;

use chrono::NaiveDateTime;
use libc::{c_char, c_int, c_uchar, c_ulonglong};
use std::{
    boxed::Box,
    ffi::{CStr, CString},
    slice,
};
use tari_comms::peer_manager::NodeIdentity;
use tari_crypto::keys::SecretKey;
use tari_transactions::tari_amount::MicroTari;
use tari_utilities::ByteArray;
use tari_wallet::wallet::{Wallet, WalletConfig};

use std::{sync::Arc, time::Duration};
use tari_comms::{connection::NetAddress, control_service::ControlServiceConfig, peer_manager::PeerFeatures};
use tari_crypto::keys::PublicKey;
use tari_utilities::hex::Hex;
use tari_wallet::{
    contacts_service::storage::database::Contact,
    storage::memory_db::WalletMemoryDatabase,
    testnet_utils::generate_wallet_test_data,
};
use tokio::runtime::Runtime;

pub type TariWallet = tari_wallet::wallet::Wallet<WalletMemoryDatabase>;
pub type TariWalletConfig = tari_wallet::wallet::WalletConfig;
pub type TariDateTime = chrono::NaiveDateTime;
pub type TariPublicKey = tari_comms::types::CommsPublicKey;
pub type TariPrivateKey = tari_comms::types::CommsSecretKey;
pub type TariCommsConfig = tari_p2p::initialization::CommsConfig;
pub type TariContact = tari_wallet::contacts_service::storage::database::Contact;

pub struct TariContacts(Vec<TariContact>);
pub struct ByteVector(Vec<c_uchar>); // declared like this so that it can be exposed to external header

/// -------------------------------- Strings ------------------------------------------------ ///
// Frees memory for string pointer
#[no_mangle]
pub unsafe extern "C" fn free_string(o: *mut c_char) {
    if !o.is_null() {
        let _ = CString::from_raw(o);
    }
}
/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- ByteVector ------------------------------------------------ ///
#[no_mangle]
pub unsafe extern "C" fn byte_vector_create(byte_array: *const c_uchar, element_count: c_int) -> *mut ByteVector {
    let mut bytes = ByteVector(Vec::new());
    if !byte_array.is_null() {
        let array: &[c_uchar] = slice::from_raw_parts(byte_array, element_count as usize);
        bytes.0 = array.to_vec();
    }
    Box::into_raw(Box::new(bytes))
}

#[no_mangle]
pub unsafe extern "C" fn byte_vector_destroy(bytes: *mut ByteVector) {
    if bytes.is_null() {
        Box::from_raw(bytes);
    }
}

/// returns c_uchar at position in internal vector
#[no_mangle]
pub unsafe extern "C" fn byte_vector_get_at(ptr: *mut ByteVector, i: c_int) -> c_uchar {
    // TODO Bound checking the length of these vectors
    (*ptr).0.clone()[i as usize]
}

/// Returns the number of items, zero-indexed
#[no_mangle]
pub unsafe extern "C" fn byte_vector_get_length(vec: *const ByteVector) -> c_int {
    if vec.is_null() {
        return 0;
    }
    (&*vec).0.len() as c_int
}

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Public Key ------------------------------------------------ ///

#[no_mangle]
pub unsafe extern "C" fn public_key_create(bytes: *mut ByteVector) -> *mut TariPublicKey {
    let mut v = Vec::new();
    if !bytes.is_null() {
        v = (*bytes).0.clone();
    }
    let pk = TariPublicKey::from_bytes(&v).unwrap();
    Box::into_raw(Box::new(pk))
}

#[no_mangle]
pub unsafe extern "C" fn public_key_destroy(pk: *mut TariPublicKey) {
    if !pk.is_null() {
        Box::from_raw(pk);
    }
}

#[no_mangle]
pub unsafe extern "C" fn public_key_get_key(pk: *mut TariPublicKey) -> *mut ByteVector {
    let mut bytes = ByteVector(Vec::new());
    if !pk.is_null() {
        bytes.0 = (*pk).to_vec();
    }
    Box::into_raw(Box::new(bytes))
}

#[no_mangle]
pub unsafe extern "C" fn public_key_from_private_key(secret_key: *mut TariPrivateKey) -> *mut TariPublicKey {
    let m = TariPublicKey::from_secret_key(&(*secret_key));
    //    println!("PK: {:?}", m);
    Box::into_raw(Box::new(m))
}

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Private Key ----------------------------------------------- ///

#[no_mangle]
pub unsafe extern "C" fn private_key_create(bytes: *mut ByteVector) -> *mut TariPrivateKey {
    let mut v = Vec::new();
    if !bytes.is_null() {
        v = (*bytes).0.clone();
    }
    let pk = TariPrivateKey::from_bytes(&v).unwrap();
    Box::into_raw(Box::new(pk))
}

#[no_mangle]
pub unsafe extern "C" fn private_key_destroy(pk: *mut TariPrivateKey) {
    if !pk.is_null() {
        Box::from_raw(pk);
    }
}

#[no_mangle]
pub unsafe extern "C" fn private_key_get_byte_vector(pk: *mut TariPrivateKey) -> *mut ByteVector {
    let mut bytes = ByteVector(Vec::new());
    if !pk.is_null() {
        bytes.0 = (*pk).to_vec();
    }
    Box::into_raw(Box::new(bytes))
}

#[no_mangle]
pub unsafe extern "C" fn private_key_generate() -> *mut TariPrivateKey {
    let mut rng = rand::OsRng::new().unwrap();
    let secret_key = TariPrivateKey::random(&mut rng);
    Box::into_raw(Box::new(secret_key))
}

#[no_mangle]
pub unsafe extern "C" fn private_key_from_hex(key: *const c_char) -> *mut TariPrivateKey {
    let mut key_str = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !key.is_null() {
        key_str = CStr::from_ptr(key).to_str().unwrap().to_owned();
    }

    let secret_key = TariPrivateKey::from_hex(key_str.as_str()).unwrap();
    Box::into_raw(Box::new(secret_key))
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- Contact -------------------------------------------------///

#[no_mangle]
pub unsafe extern "C" fn contact_create(alias: *const c_char, public_key: *mut TariPublicKey) -> *mut TariContact {
    let mut alias_string = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !alias.is_null() {
        alias_string = CStr::from_ptr(alias).to_str().unwrap().to_owned();
    }

    // TODO check if the public key is null and then deal with it.

    let contact = Contact {
        alias: alias_string.to_string(),
        public_key: (*public_key).clone(),
    };
    Box::into_raw(Box::new(contact))
}

#[no_mangle]
pub unsafe extern "C" fn contact_destroy(contact: *mut TariContact) {
    if !contact.is_null() {
        Box::from_raw(contact);
    }
}

#[no_mangle]
pub unsafe extern "C" fn contact_get_alias(contact: *mut TariContact) -> *mut c_char {
    let mut a = CString::new("").unwrap();
    if !contact.is_null() {
        a = CString::new((*contact).alias.clone()).unwrap();
    }
    CString::into_raw(a)
}

#[no_mangle]
pub unsafe extern "C" fn contact_get_public_key(contact: *mut TariContact) -> *mut TariPublicKey {
    // TODO What do we do if its null?
    // if c.is_null() {}
    Box::into_raw(Box::new((*contact).public_key.clone()))
}

#[no_mangle]
pub unsafe extern "C" fn contact_len(contact: *mut TariContacts) -> c_int {
    let mut len = 0;
    if !contact.is_null() {
        len = (*contact).0.len();
    }
    len as c_int
}

#[no_mangle]
pub unsafe extern "C" fn contact_get_at(contacts: *mut TariContacts, position: c_int) -> *mut TariContact {
    // TODO What do we do if its null?
    // if c.is_null() {}
    // TODO Bounds checking, still not sure what to do if there is a problem so leaving as TODO
    Box::into_raw(Box::new((*contacts).0[position as usize].clone()))
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- CommsConfig ---------------------------------------------///

#[no_mangle]
pub unsafe extern "C" fn comms_config_create(
    address: *const c_char,
    database_name: *const c_char,
    datastore_path: *const c_char,
    secret_key: *mut TariPrivateKey,
) -> *mut TariCommsConfig
{
    let mut address_string = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !address.is_null() {
        address_string = CStr::from_ptr(address).to_str().unwrap().to_owned();
    }
    let mut database_name_string = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !database_name.is_null() {
        database_name_string = CStr::from_ptr(database_name).to_str().unwrap().to_owned();
    }
    let mut datastore_path_string = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !datastore_path.is_null() {
        datastore_path_string = CStr::from_ptr(datastore_path).to_str().unwrap().to_owned();
    }

    // TODO Handle this unwrap gracefully
    let net_address = address_string.parse::<NetAddress>().unwrap();

    let ni = NodeIdentity::new(
        (*secret_key).clone(),
        net_address.clone(),
        PeerFeatures::COMMUNICATION_CLIENT,
    )
    .unwrap();

    let config = TariCommsConfig {
        node_identity: Arc::new(ni.clone()),
        peer_connection_listening_address: net_address.host().parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: ni.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        establish_connection_timeout: Duration::from_secs(10),
        datastore_path: datastore_path_string,
        peer_database_name: database_name_string,
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
    };

    Box::into_raw(Box::new(config))
}

#[no_mangle]
pub unsafe extern "C" fn comms_config_destroy(wc: *mut TariCommsConfig) {
    if !wc.is_null() {
        Box::from_raw(wc);
    }
}

/// ---------------------------------------------------------------------------------------------- ///

/// ------------------------------------- Wallet -------------------------------------------------///

#[no_mangle]
pub unsafe extern "C" fn wallet_create(config: *mut TariCommsConfig) -> *mut TariWallet {
    // TODO Check that the config is not null, how do you deal with the case that it is null?

    // TODO Gracefully handle the case where these expects would fail
    let runtime = Runtime::new().expect("Could not create a Tokio runtime.");
    let w = TariWallet::new(
        WalletConfig {
            comms_config: (*config).clone(),
        },
        WalletMemoryDatabase::new(),
        runtime,
    )
    .expect("Could not create wallet"); // expect needs to change due to it panicking.
    Box::into_raw(Box::new(w))
}

#[no_mangle]
pub unsafe extern "C" fn wallet_destroy(wallet: *mut TariWallet) {
    if wallet.is_null() {
        return;
    }

    let m = Box::from_raw(wallet);
    m.shutdown().unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn wallet_generate_test_data(wallet: *mut TariWallet) -> bool {
    if wallet.is_null() {
        return false;
    }

    match generate_wallet_test_data(&mut *wallet) {
        Ok(_) => true,
        _ => false,
    }
}

// ------------------------------------------------------------------------------------------------
// API Functions
// ------------------------------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn wallet_add_base_node_peer(
    wallet: *mut TariWallet,
    public_key: *mut TariPublicKey,
    address: *const c_char,
) -> bool
{
    if wallet.is_null() {
        return false;
    }

    if public_key.is_null() {
        return false;
    }

    let mut address_string = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !address.is_null() {
        address_string = CStr::from_ptr(address).to_str().unwrap().to_owned();
    }

    match (*wallet).add_base_node_peer((*public_key).clone(), address_string) {
        Ok(_) => true,
        Err(_) => false,
    }
}

#[no_mangle]
pub unsafe extern "C" fn wallet_get_balance(wallet: *mut Wallet<WalletMemoryDatabase>) -> c_ulonglong {
    if wallet.is_null() {
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).output_manager_service.get_balance())
    {
        Ok(b) => u64::from(b.available_balance),
        Err(_) => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn wallet_get_num_completed_tx(wallet: *mut Wallet<WalletMemoryDatabase>) -> c_ulonglong {
    if wallet.is_null() {
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).transaction_service.get_completed_transactions())
    {
        Ok(c) => c.len() as u64,
        Err(_) => 0,
    }
}

// Create and send the first stage of a transaction to the specified wallet for the specified amount and with the
// specified fee.
#[no_mangle]
pub unsafe extern "C" fn wallet_send_transaction(
    wallet: *mut Wallet<WalletMemoryDatabase>,
    dest_public_key: *mut TariPublicKey,
    amount: c_ulonglong,
    fee_per_gram: c_ulonglong,
    message: *const c_char,
) -> bool
{
    if wallet.is_null() {
        return false;
    }

    if dest_public_key.is_null() {
        return false;
    }

    let mut message_string = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !message.is_null() {
        message_string = CStr::from_ptr(message).to_str().unwrap().to_owned();
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).transaction_service.send_transaction(
            (*dest_public_key).clone(),
            MicroTari::from(amount),
            MicroTari::from(fee_per_gram),
            message_string,
        )) {
        Ok(_) => true,
        _ => false,
    }
}

#[no_mangle]
pub unsafe extern "C" fn wallet_get_contacts(wallet: *mut Wallet<WalletMemoryDatabase>) -> *mut TariContacts {
    let mut contacts = Vec::new();
    if !wallet.is_null() {
        // TODO gracefully check this unwrap
        let retrieved_contacts = (*wallet)
            .runtime
            .block_on((*wallet).contacts_service.get_contacts())
            .unwrap();
        contacts.append(&mut retrieved_contacts.clone());
    }
    Box::into_raw(Box::new(TariContacts(contacts)))
}

// TODO Get and destructuring a list of contacts
// TODO Add Contact to Contacts Service
// TODO Delete Contact to Contacts Service
// TODO Get and destructure list of completed transactions from Transaction Service
// TODO Get and destructure list of pending_inbound_transactions from Transaction Service
// TODO Get and destructure list pending_outbound_transactions from Transaction Service

// Callback Definition - Example

// Will probably have to implement as a struct of callbacks in wallet, with wallet only calling the
// functions if they are callable from the relevant wallet function, where the register callback functions
// will bind the relevant c equivalent function pointer to the associated function
// The Rust
//
// use std::os::raw::{c_int, c_uchar};
//
// #[no_mangle]
// pub struct MyState {
// pub call_back: extern "C" fn(*const c_uchar) -> c_int
// }
//
// #[no_mangle]
// pub extern fn get_state(call: extern "C" fn(*const c_uchar) -> c_int) -> *const () {
// let state = MyState { call_back: call };
// Box::into_raw(Box::new(state)) as *const _
// }
//
// #[no_mangle]
// pub extern fn run(state: *mut MyState) -> c_int {
// unsafe {
// ((*state).call_back)(format!("Callback run").as_ptr())
// }
// }
//
// #[no_mangle]
// pub extern fn delete_state(state: *mut MyState) {
// unsafe {
// Box::from_raw(state);
// }
// }
//
// The C
// #include <iostream>
//
// extern "C" {
// void* get_state(int (*callback)(char*));
// int run(void* state);
// void delete_state(void* state);
// }
//

// ------------------------------------------------------------------------------------------------
// Callback Functions
// ------------------------------------------------------------------------------------------------
// These functions must be implemented by the FFI client and registered with LibWallet so that
// LibWallet can directly respond to the client when events occur

// TODO Callbacks to be written and registered to receive the following events
// Received a transaction
// Received a transaction reply
// Transaction hit the mempool (send and receive)
// Transaction is mined
// Transaction is confirmed

#[cfg(test)]
mod test {
    extern crate libc;
    use crate::*;
    use libc::{c_char, c_int, c_uchar};
    use std::ffi::CString;

    #[test]
    fn test_free_string() {
        unsafe {
            let m = CString::new("Test").unwrap();
            let m_ptr: *mut c_char = CString::into_raw(m) as *mut c_char;
            assert_ne!(m_ptr.is_null(), true);
            assert!(*m_ptr > 0); // dereference will return first character as integer, T as i8 = 84 > 0 = true
            free_string(m_ptr);
            assert_eq!(*m_ptr, 0); // dereference will return zero, avoids malloc error if attempting to evaluate by
                                   // other means.
        }
    }

    #[test]
    fn test_bytevector() {
        unsafe {
            let bytes: [c_uchar; 4] = [2, 114, 34, 255];
            let bytes_ptr = byte_vector_create(bytes.as_ptr(), bytes.len() as c_int);
            let length = byte_vector_get_length(bytes_ptr);
            // println!("{:?}",c);
            assert_eq!(length, bytes.len() as i32);
            let byte = byte_vector_get_at(bytes_ptr, 2);
            assert_eq!(byte, bytes[2]);
            byte_vector_destroy(bytes_ptr);
        }
    }

    #[test]
    fn test_keys() {
        unsafe {
            let private_key = private_key_generate();
            let public_key = public_key_from_private_key(private_key);
            let private_key_length = byte_vector_get_length(private_key_get_byte_vector(private_key));
            let public_key_length = byte_vector_get_length(public_key_get_key(public_key));
            assert_eq!(private_key_length, 32);
            assert_eq!(public_key_length, 32);
            assert_ne!(private_key_get_byte_vector(private_key), public_key_get_key(public_key));
        }
    }

    #[test]
    fn test_wallet_ffi() {
        unsafe {
            let secret_key_alice = private_key_generate();
            let public_key_alice = public_key_from_private_key(secret_key_alice.clone());
            let db_name_alice = CString::new("ffi_test1_alice").unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice.clone()) as *const c_char;
            let db_path_alice = CString::new("./data_alice").unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice.clone()) as *const c_char;
            let address_alice = CString::new("127.0.0.1:21443").unwrap();
            let address_alice_str: *const c_char = CString::into_raw(address_alice.clone()) as *const c_char;
            let alice_config = comms_config_create(
                address_alice_str,
                db_name_alice_str,
                db_path_alice_str,
                secret_key_alice,
            );
            let alice_wallet = wallet_create(alice_config);

            let secret_key_bob = private_key_generate();
            let public_key_bob = public_key_from_private_key(secret_key_bob.clone());
            let db_name_bob = CString::new("ffi_test1_bob").unwrap();
            let db_name_bob_str: *const c_char = CString::into_raw(db_name_bob.clone()) as *const c_char;
            let db_path_bob = CString::new("./data_bob").unwrap();
            let db_path_bob_str: *const c_char = CString::into_raw(db_path_bob.clone()) as *const c_char;
            let address_bob = CString::new("127.0.0.1:21441").unwrap();
            let address_bob_str: *const c_char = CString::into_raw(address_bob.clone()) as *const c_char;
            let bob_config = comms_config_create(address_bob_str, db_name_bob_str, db_path_bob_str, secret_key_bob);
            let bob_wallet = wallet_create(bob_config);

            wallet_add_base_node_peer(alice_wallet, public_key_bob.clone(), address_bob_str);
            wallet_add_base_node_peer(bob_wallet, public_key_alice.clone(), address_alice_str);

            wallet_generate_test_data(alice_wallet);

            let contacts = wallet_get_contacts(alice_wallet);
            assert_eq!(contact_len(contacts), 4);

            // free string memory
            free_string(db_name_alice_str as *mut c_char);
            free_string(db_path_alice_str as *mut c_char);
            free_string(address_alice_str as *mut c_char);
            free_string(db_name_bob_str as *mut c_char);
            free_string(db_path_bob_str as *mut c_char);
            free_string(address_bob_str as *mut c_char);
            // free wallet memory
            wallet_destroy(alice_wallet);
            wallet_destroy(bob_wallet);
            // free keys
            private_key_destroy(secret_key_alice);
            private_key_destroy(secret_key_bob);
            public_key_destroy(public_key_alice);
            public_key_destroy(public_key_bob);
            // free config memory
            comms_config_destroy(bob_config);
            comms_config_destroy(alice_config);
        }
    }
}
