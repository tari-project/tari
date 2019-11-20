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
//! This files contains the API calls that will be exposed to external systems that make use of this module. The API
//! will be exposed via FFI and will consist of API calls that the FFI client can make into the Wallet module and a set
//! of Callbacks that the client must implement and provide to the Wallet module to receive asynchronous replies and
//! updates.
//!
//! # Wallet Flow Documentation
//! This documentation will described the flows of the core tasks that the Wallet library supports and will then
//! describe how to use the test functions to produce the behaviour of a second wallet without needing to set one up.
//!
//! ## Generate Test Data
//! The `generate_wallet_test_data(...)` function will generate some test data in the wallet. The data generated will be
//! as follows:
//!
//! - Some Contacts
//! - Add outputs to the wallet that make up its Available Balance that can be spent
//! - Create transaction history
//!    - Pending Inbound Transactions
//!    - Pending Outbound Transactions
//!    - Completed Transactions
//!
//! ## Send Transaction
//! To send a transaction your wallet must have available funds and you must had added the recipient's Public Key as a
//! `Contact`.
//!
//! To send a transaction:
//! 1.  Call the `send_transaction(dest_public_key, amount, fee_per_gram, message)` function which will result in a
//!     `PendingOutboundTransaction` being produced and transmitted to the recipient and the funds becoming
//!     encumbered and appearing in the `PendingOutgoingBalance` and any change will appear in the
//!     `PendingIncomingBalance`.
//! 2.  Wait until the recipient replies to the sent transaction which will result in the `PendingOutboundTransaction`
//!     becoming a `CompletedTransaction` with the `Broadcast` status. This means that the transaction has been
//!     negotiated between the parties and is now broadcast to the Base Layer waiting to be mined. The funds are still
//!     encumbered as pending because the transaction has not been mined yet.
//! 3.  Wait until the transaction is mined. The `CompleteTransaction` status will then move from `Broadcast` to `Mined`
//!     and the pending funds will be spent and received.
//!
//! ## Receive a Transaction
//! 1.  When a transaction is received it will appear as an `InboundTransaction` and the amount to be received will
//!     appear as a `PendingIncomingBalance`. The wallet backend will be listening for these transactions and will
//!     immediately reply to the sending wallet.
//! 2.  This wallet will then monitor the Base Layer to detect when the Sender wallet will broadcast the
//!     `CompletedTransaction` to the mempool i.e. the `CompletedTransaction` status is `Broadcast`. The funds are
//!     still pending at this stage as the transaction has not been mined.
//! 3.  This wallet will then monitor the Base Layer to see when the transaction is mined which means the
//!     `CompletedTransaction` status will become `Mined` and the funds will then move from the `PendingIncomingBalance`
//!     to the `AvailableBalance`.
//!
//! ## Using the test functions
//! The above two flows both require a second wallet for this wallet to interact with. Because we do not yet have a live
//! Test Net and the communications layer is not quite ready the library supplies four functions to help simulate the
//! second wallets role in these flows. The following will describe how to use these functions to produce the flows.
//!
//! ### Send Transaction with test functions
//! 1.  Send Transaction as above to produce a `PendingOutboundTransaction`.
//! 2.  Call the `complete_sent_transaction(...)` function with the tx_id of the sent transaction to simulate a reply.
//!     This will move the `PendingOutboundTransaction` to become a `CompletedTransaction` with the `Broadcast` status
//!     which means it has been broadcast to the Base Layer Mempool but not mined yet.
//! 3.  Call the `mined_transaction(...)` function with the tx_id of the sent transaction which will change
//!     the status of the `CompletedTransaction` from `Broadcast` to `Mined`. The pending funds will also become
//!     finalized as spent and available funds respectively.
//!
//! ### Receive Transaction with test functions
//! Under normal operation another wallet would initiate a Receive Transaction flow by sending you a transaction. We
//! will use the `receive_test_transaction(...)` function to initiate the flow:
//!
//! 1.  Calling `receive_test_transaction(...)` will produce an `InboundTransaction`, the amount of the transaction will
//!     appear under the `PendingIncomingBalance`.
//! 2.  To simulate detecting the `InboundTransaction` being broadcast to the Base Layer Mempool call
//!     `broadcast_transaction(...)` function. This will change the `InboundTransaction` to a
//!     `CompletedTransaction` with the `Broadcast` status. The funds will still reflect in the pending balance.
//! 3.  Call the `mined_transaction(...)` function with the tx_id of the received transaction which will
//!     change the status of the `CompletedTransaction` from    `Broadcast` to `Mined`. The pending funds will also
//!     become finalized as spent and available funds respectively

extern crate libc;
extern crate tari_wallet;

use libc::{c_char, c_int, c_longlong, c_uchar, c_uint, c_ulonglong};
use std::{
    boxed::Box,
    ffi::{CStr, CString},
    slice,
};
use tari_comms::peer_manager::NodeIdentity;
use tari_crypto::keys::SecretKey;
use tari_transactions::tari_amount::MicroTari;
use tari_utilities::ByteArray;
use tari_wallet::wallet::WalletConfig;

use core::ptr;
use std::{sync::Arc, time::Duration};
use tari_comms::{connection::NetAddress, control_service::ControlServiceConfig, peer_manager::PeerFeatures};
use tari_crypto::keys::PublicKey;
use tari_transactions::types::CryptoFactories;
use tari_utilities::hex::Hex;
use tari_wallet::{
    contacts_service::storage::database::Contact,
    storage::memory_db::WalletMemoryDatabase,
    testnet_utils::{
        broadcast_transaction,
        complete_sent_transaction,
        generate_wallet_test_data,
        mine_transaction,
        receive_test_transaction,
    },
};
use tokio::runtime::Runtime;

pub type TariWallet = tari_wallet::wallet::Wallet<WalletMemoryDatabase>;
pub type TariPublicKey = tari_comms::types::CommsPublicKey;
pub type TariPrivateKey = tari_comms::types::CommsSecretKey;
pub type TariCommsConfig = tari_p2p::initialization::CommsConfig;
pub struct TariContacts(Vec<TariContact>);
pub type TariContact = tari_wallet::contacts_service::storage::database::Contact;
pub type TariCompletedTransaction = tari_wallet::transaction_service::storage::database::CompletedTransaction;
pub struct TariCompletedTransactions(Vec<TariCompletedTransaction>);
pub type TariPendingInboundTransaction = tari_wallet::transaction_service::storage::database::InboundTransaction;
pub struct TariPendingInboundTransactions(Vec<TariPendingInboundTransaction>);
pub type TariPendingOutboundTransaction = tari_wallet::transaction_service::storage::database::OutboundTransaction;
pub struct TariPendingOutboundTransactions(Vec<TariPendingOutboundTransaction>);
pub struct ByteVector(Vec<c_uchar>); // declared like this so that it can be exposed to external header

/// -------------------------------- Strings ------------------------------------------------ ///

/// Frees memory for a char array
///
/// ## Arguments
/// `ptr` - The pointer to be freed
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C.

#[no_mangle]
pub unsafe extern "C" fn string_destroy(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = CString::from_raw(ptr);
    }
}
/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- ByteVector ------------------------------------------------ ///

/// Creates a ByteVector
///
/// ## Arguments
/// `byte_array` - The pointer to the byte array
/// `element_count` - The number of elements in byte_array
///
/// ## Returns
/// `*mut ByteVector` - Pointer to the created ByteVector. Note that it will be ptr::null_mut()
/// if the byte_array pointer was null or if the elements in the byte_vector don't match
/// element_count when it is created
#[no_mangle]
pub unsafe extern "C" fn byte_vector_create(byte_array: *const c_uchar, element_count: c_uint) -> *mut ByteVector {
    let mut bytes = ByteVector(Vec::new());
    if byte_array.is_null() {
        return ptr::null_mut();
    } else {
        let array: &[c_uchar] = slice::from_raw_parts(byte_array, element_count as usize);
        bytes.0 = array.to_vec();
        if bytes.0.len() != element_count as usize {
            return ptr::null_mut();
        }
    }
    Box::into_raw(Box::new(bytes))
}

/// Frees memory for a ByteVector
///
/// ## Arguments
/// `bytes` - The pointer to a ByteVector
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn byte_vector_destroy(bytes: *mut ByteVector) {
    if bytes.is_null() {
        Box::from_raw(bytes);
    }
}

/// Gets a c_uchar at position in a ByteVector
///
/// ## Arguments
/// `ptr` - The pointer to a ByteVector
/// `position` - The integer position
///
/// ## Returns
/// `c_uchar` - Returns a character. Note that the character will be a null terminator (0) if ptr
/// is null or if the position is invalid
#[no_mangle]
pub unsafe extern "C" fn byte_vector_get_at(ptr: *mut ByteVector, position: c_uint) -> c_uchar {
    if ptr.is_null() {
        return 0 as c_uchar;
    }
    let len = byte_vector_get_length(ptr) as c_int - 1; // clamp to length
    if len < 0 {
        return 0 as c_uchar;
    }
    if position > len as c_uint {
        return 0 as c_uchar;
    }
    (*ptr).0.clone()[position as usize]
}

/// Gets the number of elements in a ByteVector
///
/// ## Arguments
/// `ptr` - The pointer to a ByteVector
///
/// ## Returns
/// `c_uint` - Returns the integer number of elements in the ByteVector. Note that it will be zero
/// if ptr is null
#[no_mangle]
pub unsafe extern "C" fn byte_vector_get_length(vec: *const ByteVector) -> c_uint {
    if vec.is_null() {
        return 0;
    }
    (&*vec).0.len() as c_uint
}

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Public Key ------------------------------------------------ ///

/// Creates a TariPublicKey from a ByteVector
///
/// ## Arguments
/// `bytes` - The pointer to a ByteVector
///
/// ## Returns
/// `TariPublicKey` - Returns a public key. Note that it will be ptr::null_mut() if bytes is null or
/// if there was an error with the contents of bytes
#[no_mangle]
pub unsafe extern "C" fn public_key_create(bytes: *mut ByteVector) -> *mut TariPublicKey {
    let v;
    if !bytes.is_null() {
        v = (*bytes).0.clone();
    } else {
        return ptr::null_mut();
    }
    let pk = TariPublicKey::from_bytes(&v);
    match pk {
        Ok(pk) => Box::into_raw(Box::new(pk)),
        Err(_) => ptr::null_mut(),
    }
}

/// Frees memory for a TariPublicKey
///
/// ## Arguments
/// `pk` - The pointer to a TariPublicKey
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn public_key_destroy(pk: *mut TariPublicKey) {
    if !pk.is_null() {
        Box::from_raw(pk);
    }
}

/// Gets a ByteVector from a TariPublicKey
///
/// ## Arguments
/// `pk` - The pointer to a TariPublicKey
///
/// ## Returns
/// `*mut ByteVector` - Returns a pointer to a ByteVector. Note that it returns ptr::null_mut() if pk is null
#[no_mangle]
pub unsafe extern "C" fn public_key_get_bytes(pk: *mut TariPublicKey) -> *mut ByteVector {
    let mut bytes = ByteVector(Vec::new());
    if !pk.is_null() {
        bytes.0 = (*pk).to_vec();
    } else {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new(bytes))
}

/// Creates a TariPublicKey from a TariPrivateKey
///
/// ## Arguments
/// `secret_key` - The pointer to a TariPrivateKey
///
/// ## Returns
/// `*mut TariPublicKey` - Returns a pointer to a TariPublicKey
#[no_mangle]
pub unsafe extern "C" fn public_key_from_private_key(secret_key: *mut TariPrivateKey) -> *mut TariPublicKey {
    if secret_key.is_null() {
        return ptr::null_mut();
    }
    let m = TariPublicKey::from_secret_key(&(*secret_key));
    Box::into_raw(Box::new(m))
}

/// Creates a TariPublicKey from a char array
///
/// ## Arguments
/// `key` - The pointer to a char array which is hex encoded
///
/// ## Returns
/// `*mut TariPublicKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut()
/// if key is null or if there was an error creating the TariPublicKey from key
#[no_mangle]
pub unsafe extern "C" fn public_key_from_hex(key: *const c_char) -> *mut TariPublicKey {
    let key_str;
    if !key.is_null() {
        key_str = CStr::from_ptr(key).to_str().unwrap().to_owned();
    } else {
        return ptr::null_mut();
    }

    let public_key = TariPublicKey::from_hex(key_str.as_str());
    match public_key {
        Ok(public_key) => Box::into_raw(Box::new(public_key)),
        Err(_) => ptr::null_mut(),
    }
}
/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Private Key ----------------------------------------------- ///

/// Creates a TariPrivateKey from a ByteVector
///
/// ## Arguments
/// `bytes` - The pointer to a ByteVector
///
/// ## Returns
/// `*mut TariPrivateKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut()
/// if bytes is null or if there was an error creating the TariPrivateKey from bytes
#[no_mangle]
pub unsafe extern "C" fn private_key_create(bytes: *mut ByteVector) -> *mut TariPrivateKey {
    let v;
    if !bytes.is_null() {
        v = (*bytes).0.clone();
    } else {
        return ptr::null_mut();
    }
    let pk = TariPrivateKey::from_bytes(&v);
    match pk {
        Ok(pk) => Box::into_raw(Box::new(pk)),
        Err(_) => ptr::null_mut(),
    }
}

/// Frees memory for a TariPrivateKey
///
/// ## Arguments
/// `pk` - The pointer to a TariPrivateKey
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn private_key_destroy(pk: *mut TariPrivateKey) {
    if !pk.is_null() {
        Box::from_raw(pk);
    }
}

/// Gets a ByteVector from a TariPrivateKey
///
/// ## Arguments
/// `pk` - The pointer to a TariPrivateKey
///
/// ## Returns
/// `*mut ByteVectror` - Returns a pointer to a ByteVector. Note that it returns ptr::null_mut()
/// if pk is null
#[no_mangle]
pub unsafe extern "C" fn private_key_get_bytes(pk: *mut TariPrivateKey) -> *mut ByteVector {
    let mut bytes = ByteVector(Vec::new());
    if !pk.is_null() {
        bytes.0 = (*pk).to_vec();
    } else {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new(bytes))
}

/// Generates a TariPrivateKey
///
/// ## Arguments
/// `()` - Does  not take any arguments
///
/// ## Returns
/// `*mut TariPrivateKey` - Returns a pointer to a TariPrivateKey
#[no_mangle]
pub unsafe extern "C" fn private_key_generate() -> *mut TariPrivateKey {
    let mut rng = rand::OsRng::new().unwrap();
    let secret_key = TariPrivateKey::random(&mut rng);
    Box::into_raw(Box::new(secret_key))
}

/// Creates a TariPrivateKey from a char array
///
/// ## Arguments
/// `key` - The pointer to a char array which is hex encoded
///
/// ## Returns
/// `*mut TariPrivateKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut()
/// if key is null or if there was an error creating the TariPrivateKey from key
#[no_mangle]
pub unsafe extern "C" fn private_key_from_hex(key: *const c_char) -> *mut TariPrivateKey {
    let key_str;
    if !key.is_null() {
        key_str = CStr::from_ptr(key).to_str().unwrap().to_owned();
    } else {
        return ptr::null_mut();
    }

    let secret_key = TariPrivateKey::from_hex(key_str.as_str());

    match secret_key {
        Ok(secret_key) => Box::into_raw(Box::new(secret_key)),
        Err(_) => ptr::null_mut(),
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- Contact -------------------------------------------------///

/// Creates a TariContact
///
/// ## Arguments
/// `alias` - The pointer to a char array
/// `public_key` - The pointer to a TariPublicKey
///
/// ## Returns
/// `*mut TariContact` - Returns a pointer to a TariContact. Note that it returns ptr::null_mut()
/// if alias is null or if pk is null
#[no_mangle]
pub unsafe extern "C" fn contact_create(alias: *const c_char, public_key: *mut TariPublicKey) -> *mut TariContact {
    let alias_string;
    if !alias.is_null() {
        alias_string = CStr::from_ptr(alias).to_str().unwrap().to_owned();
    } else {
        return ptr::null_mut();
    }

    if public_key.is_null() {
        return ptr::null_mut();
    }

    let contact = Contact {
        alias: alias_string.to_string(),
        public_key: (*public_key).clone(),
    };
    Box::into_raw(Box::new(contact))
}

/// Gets the alias of the TariContact
///
/// ## Arguments
/// `contact` - The pointer to a TariContact
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns an empty char array if
/// contact is null
#[no_mangle]
pub unsafe extern "C" fn contact_get_alias(contact: *mut TariContact) -> *mut c_char {
    let mut a = CString::new("").unwrap();
    if !contact.is_null() {
        a = CString::new((*contact).alias.clone()).unwrap();
    }
    CString::into_raw(a)
}

/// Gets the TariPublicKey of the TariContact
///
/// ## Arguments
/// `contact` - The pointer to a TariContact
///
/// ## Returns
/// `*mut TariPublicKey` - Returns a pointer to a TariPublicKey. Note that it returns
/// ptr::null_mut() if contact is null
#[no_mangle]
pub unsafe extern "C" fn contact_get_public_key(contact: *mut TariContact) -> *mut TariPublicKey {
    if contact.is_null() {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new((*contact).public_key.clone()))
}

/// Frees memory for a TariContact
///
/// ## Arguments
/// `contact` - The pointer to a TariContact
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn contact_destroy(contact: *mut TariContact) {
    if !contact.is_null() {
        Box::from_raw(contact);
    }
}

/// ----------------------------------- Contacts -------------------------------------------------///

/// Gets the length of TariContacts
///
/// ## Arguments
/// `contacts` - The pointer to a TariContacts
///
/// ## Returns
/// `c_uint` - Returns number of elements in , zero if contacts is null
#[no_mangle]
pub unsafe extern "C" fn contacts_get_length(contacts: *mut TariContacts) -> c_uint {
    let mut len = 0;
    if !contacts.is_null() {
        len = (*contacts).0.len();
    }
    len as c_uint
}

/// Gets a TariContact from TariContacts at position
///
/// ## Arguments
/// `contacts` - The pointer to a TariContacts
/// `position` - The integer position
///
/// ## Returns
/// `*mut TariContact` - Returns a TariContact, note that it returns ptr::null_mut() if contacts is
/// null or position is invalid
#[no_mangle]
pub unsafe extern "C" fn contacts_get_at(contacts: *mut TariContacts, position: c_uint) -> *mut TariContact {
    if contacts.is_null() {
        return ptr::null_mut();
    }
    let len = contacts_get_length(contacts) as c_int - 1;
    if len < 0 {
        return ptr::null_mut();
    }
    if position > len as c_uint {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new((*contacts).0[position as usize].clone()))
}

/// Frees memory for a TariContacts
///
/// ## Arguments
/// `contacts` - The pointer to a TariContacts
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn contacts_destroy(contacts: *mut TariContacts) {
    if !contacts.is_null() {
        Box::from_raw(contacts);
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- CompletedTransactions ----------------------------------- ///

/// Gets the length of a TariCompletedTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariCompletedTransactions
///
/// ## Returns
/// `c_uint` - Returns the number of elements in a TariCompletedTransactions, note that it will be
/// zero if transactions is null
#[no_mangle]
pub unsafe extern "C" fn completed_transactions_get_length(transactions: *mut TariCompletedTransactions) -> c_uint {
    let mut len = 0;
    if !transactions.is_null() {
        len = (*transactions).0.len();
    }
    len as c_uint
}

/// Gets a TariCompletedTransaction from a TariCompletedTransactions at position
///
/// ## Arguments
/// `transactions` - The pointer to a TariCompletedTransactions
/// `position` - The integer position
///
/// ## Returns
/// `*mut TariCompletedTransaction` - Returns a pointer to a TariCompletedTransaction,
/// note that ptr::null_mut() is returned if transactions is null or position is invalid
#[no_mangle]
pub unsafe extern "C" fn completed_transactions_get_at(
    transactions: *mut TariCompletedTransactions,
    position: c_uint,
) -> *mut TariCompletedTransaction
{
    if transactions.is_null() {
        return ptr::null_mut();
    }
    let len = completed_transactions_get_length(transactions) as c_int - 1;
    if len < 0 {
        return ptr::null_mut();
    }
    if position > len as c_uint {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new((*transactions).0[position as usize].clone()))
}

/// Frees memory for a TariCompletedTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn completed_transactions_destroy(transactions: *mut TariCompletedTransactions) {
    if !transactions.is_null() {
        Box::from_raw(transactions);
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- OutboundTransactions ------------------------------------ ///

/// Gets the length of a TariPendingOutboundTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariPendingOutboundTransactions
///
/// ## Returns
/// `c_uint` - Returns the number of elements in a TariPendingOutboundTransactions, note that it will be
/// zero if transactions is null
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transactions_get_length(
    transactions: *mut TariPendingOutboundTransactions,
) -> c_uint {
    let mut len = 0;
    if !transactions.is_null() {
        len = (*transactions).0.len();
    }
    len as c_uint
}

/// Gets a TariPendingOutboundTransaction of a TariPendingOutboundTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariPendingOutboundTransactions
/// `position` - The integer position
///
/// ## Returns
/// `*mut TariPendingOutboundTransaction` - Returns a pointer to a TariPendingOutboundTransaction,
/// note that ptr::null_mut() is returned if transactions is null or position is invalid
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transactions_get_at(
    transactions: *mut TariPendingOutboundTransactions,
    position: c_uint,
) -> *mut TariPendingOutboundTransaction
{
    if transactions.is_null() {
        return ptr::null_mut();
    }
    let len = pending_outbound_transactions_get_length(transactions) as c_int - 1;
    if len < 0 {
        return ptr::null_mut();
    }
    if position > len as c_uint {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new((*transactions).0[position as usize].clone()))
}

/// Frees memory for a TariPendingOutboundTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariPendingOutboundTransactions
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transactions_destroy(transactions: *mut TariPendingOutboundTransactions) {
    if !transactions.is_null() {
        Box::from_raw(transactions);
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- InboundTransactions ------------------------------------- ///

/// Gets the length of a TariPendingInboundTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariPendingInboundTransactions
///
/// ## Returns
/// `c_uint` - Returns the number of elements in a TariPendingInboundTransactions, note that
/// it will be zero if transactions is null
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transactions_get_length(
    transactions: *mut TariPendingInboundTransactions,
) -> c_uint {
    let mut len = 0;
    if !transactions.is_null() {
        len = (*transactions).0.len();
    }
    len as c_uint
}

/// Gets a TariPendingInboundTransaction of a TariPendingInboundTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariPendingInboundTransactions
/// `position` - The integer position
///
/// ## Returns
/// `*mut TariPendingOutboundTransaction` - Returns a pointer to a TariPendingInboundTransaction,
/// note that ptr::null_mut() is returned if transactions is null or position is invalid
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transactions_get_at(
    transactions: *mut TariPendingInboundTransactions,
    position: c_uint,
) -> *mut TariPendingInboundTransaction
{
    if transactions.is_null() {
        return ptr::null_mut();
    }
    let len = pending_inbound_transactions_get_length(transactions) as c_int - 1;
    if len < 0 {
        return ptr::null_mut();
    }
    if position > len as c_uint {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new((*transactions).0[position as usize].clone()))
}

/// Frees memory for a TariPendingInboundTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariPendingInboundTransactions
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transactions_destroy(transactions: *mut TariPendingInboundTransactions) {
    if !transactions.is_null() {
        Box::from_raw(transactions);
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- CompletedTransaction ------------------------------------- ///

/// Gets the TransactionID of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the TransactionID, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_transaction_id(
    transaction: *mut TariCompletedTransaction,
) -> c_ulonglong {
    if transaction.is_null() {
        return 0;
    }
    (*transaction).tx_id as c_ulonglong
}

/// Gets the destination TariPublicKey of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `*mut TairPublicKey` - Returns the destination TariPublicKey, note that it will be
/// ptr::null_mut() if transaction is null
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_destination_public_key(
    transaction: *mut TariCompletedTransaction,
) -> *mut TariPublicKey {
    if transaction.is_null() {
        return ptr::null_mut();
    }
    let m = (*transaction).destination_public_key.clone();
    Box::into_raw(Box::new(m))
}

/// Gets the source TariPublicKey of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `*mut TairPublicKey` - Returns the source TariPublicKey, note that it will be
/// ptr::null_mut() if transaction is null
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_source_public_key(
    transaction: *mut TariCompletedTransaction,
) -> *mut TariPublicKey {
    if transaction.is_null() {
        return ptr::null_mut();
    }
    let m = (*transaction).source_public_key.clone();
    Box::into_raw(Box::new(m))
}

/// Gets the status of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `c_int` - Returns the status which corresponds to:
/// | Value | Interpretation |
/// |---|---|
/// |  -1 | TxNullError |
/// |   0 | Completed |
/// |   1 | Broadcast |
/// |   2 | Mined |
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_status(transaction: *mut TariCompletedTransaction) -> c_int {
    if transaction.is_null() {
        return -1;
    }
    let status = (*transaction).status.clone();
    status as c_int
}

/// Gets the amount of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the amount, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_amount(transaction: *mut TariCompletedTransaction) -> c_ulonglong {
    if transaction.is_null() {
        return 0;
    }
    c_ulonglong::from((*transaction).amount)
}

/// Gets the fee of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the fee, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_fee(transaction: *mut TariCompletedTransaction) -> c_ulonglong {
    if transaction.is_null() {
        return 0;
    }
    c_ulonglong::from((*transaction).fee)
}

/// Gets the timestamp of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the timestamp, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_timestamp(transaction: *mut TariCompletedTransaction) -> c_longlong {
    if transaction.is_null() {
        return 0;
    }
    (*transaction).timestamp.timestamp() as c_longlong
}

/// Gets the message of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `*const c_char` - Returns the pointer to the char array, note that it will return a pointer
/// to an empty char array if transaction is null
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_message(
    transaction: *mut TariCompletedTransaction,
) -> *const c_char {
    let message = (*transaction).message.clone();
    let mut result = CString::new("").unwrap();
    if transaction.is_null() {
        return result.into_raw();
    }

    result = CString::new(message).unwrap();
    result.into_raw()
}

/// Frees memory for a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_destroy(transaction: *mut TariCompletedTransaction) {
    if !transaction.is_null() {
        Box::from_raw(transaction);
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- OutboundTransaction ------------------------------------- ///

/// Gets the TransactionId of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the TransactionID, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_transaction_id(
    transaction: *mut TariPendingOutboundTransaction,
) -> c_ulonglong {
    if transaction.is_null() {
        return 0;
    }
    (*transaction).tx_id as c_ulonglong
}

/// Gets the destination TariPublicKey of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
///
/// ## Returns
/// `*mut TariPublicKey` - Returns the destination TariPublicKey, note that it will be
/// ptr::null_mut() if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_destination_public_key(
    transaction: *mut TariPendingOutboundTransaction,
) -> *mut TariPublicKey {
    if transaction.is_null() {
        return ptr::null_mut();
    }
    let m = (*transaction).destination_public_key.clone();
    Box::into_raw(Box::new(m))
}

/// Gets the amount of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the amount, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_amount(
    transaction: *mut TariPendingOutboundTransaction,
) -> c_ulonglong {
    if transaction.is_null() {
        return 0;
    }
    c_ulonglong::from((*transaction).amount)
}

/// Gets the timestamp of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the timestamp, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_timestamp(
    transaction: *mut TariPendingOutboundTransaction,
) -> c_longlong {
    if transaction.is_null() {
        return 0;
    }
    (*transaction).timestamp.timestamp() as c_longlong
}

/// Gets the message of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
///
/// ## Returns
/// `*const c_char` - Returns the pointer to the char array, note that it will return a pointer
/// to an empty char array if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_message(
    transaction: *mut TariPendingOutboundTransaction,
) -> *const c_char {
    let message = (*transaction).message.clone();
    let mut result = CString::new("").unwrap();
    if transaction.is_null() {
        return result.into_raw();
    }

    result = CString::new(message).unwrap();
    result.into_raw()
}

/// Frees memory for a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_destroy(transaction: *mut TariPendingOutboundTransaction) {
    if !transaction.is_null() {
        Box::from_raw(transaction);
    }
}

/// -------------------------------------------------------------------------------------------- ///
///
/// ----------------------------------- InboundTransaction ------------------------------------- ///

/// Gets the TransactionId of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the TransactonId, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_transaction_id(
    transaction: *mut TariPendingInboundTransaction,
) -> c_ulonglong {
    if transaction.is_null() {
        return 0;
    }
    (*transaction).tx_id as c_ulonglong
}

/// Gets the source TariPublicKey of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
///
/// ## Returns
/// `*mut TariPublicKey` - Returns a pointer to the source TariPublicKey, note that it will be
/// ptr::null_mut() if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_source_public_key(
    transaction: *mut TariPendingInboundTransaction,
) -> *mut TariPublicKey {
    if transaction.is_null() {
        return ptr::null_mut();
    }
    let m = (*transaction).source_public_key.clone();
    Box::into_raw(Box::new(m))
}

/// Gets the amount of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the amount, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_amount(
    transaction: *mut TariPendingInboundTransaction,
) -> c_ulonglong {
    if transaction.is_null() {
        return 0;
    }
    c_ulonglong::from((*transaction).amount)
}

/// Gets the timestamp of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
///
/// ## Returns
/// `c_ulonglong` - Returns the timestamp, note that it will be zero if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_timestamp(
    transaction: *mut TariPendingInboundTransaction,
) -> c_longlong {
    if transaction.is_null() {
        return 0;
    }
    (*transaction).timestamp.timestamp() as c_longlong
}

/// Gets the message of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
///
/// ## Returns
/// `*const c_char` - Returns the pointer to the char array, note that it will return a pointer
/// to an empty char array if transaction is null
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_message(
    transaction: *mut TariPendingInboundTransaction,
) -> *const c_char {
    let message = (*transaction).message.clone();
    let mut result = CString::new("").unwrap();
    if transaction.is_null() {
        return result.into_raw();
    }

    result = CString::new(message).unwrap();
    result.into_raw()
}

/// Frees memory for a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_destroy(transaction: *mut TariPendingInboundTransaction) {
    if !transaction.is_null() {
        Box::from_raw(transaction);
    }
}
/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- CommsConfig ---------------------------------------------///

/// Creates a TariCommsConfig. The result from this function is required when initializing a TariWallet.
///
/// ## Arguments
/// `control_service_address` - The control service address char array pointer
/// `listener_address` - The listener address char array pointer
/// `database_name` - The database name char array pointer
/// `database_path` - The database path char array pointer which the application has write access to
/// `secret_key` - The TariSecretKey pointer
///
/// ## Returns
/// `*mut TariCommsConfig` - Returns a pointer to a TariCommsConfig, if any of the parameters are
/// null or a problem is encountered when constructing the NetAddress a ptr::null_mut() is returned
#[no_mangle]
pub unsafe extern "C" fn comms_config_create(
    control_service_address: *const c_char,
    listener_address: *const c_char,
    database_name: *const c_char,
    datastore_path: *const c_char,
    secret_key: *mut TariPrivateKey,
) -> *mut TariCommsConfig
{
    let control_service_address_string;
    if !control_service_address.is_null() {
        control_service_address_string = CStr::from_ptr(control_service_address).to_str().unwrap().to_owned();
    } else {
        return ptr::null_mut();
    }

    let listener_address_string;
    if !listener_address.is_null() {
        listener_address_string = CStr::from_ptr(listener_address).to_str().unwrap().to_owned();
    } else {
        return ptr::null_mut();
    }

    let database_name_string;
    if !database_name.is_null() {
        database_name_string = CStr::from_ptr(database_name).to_str().unwrap().to_owned();
    } else {
        return ptr::null_mut();
    }

    let datastore_path_string;
    if !datastore_path.is_null() {
        datastore_path_string = CStr::from_ptr(datastore_path).to_str().unwrap().to_owned();
    } else {
        return ptr::null_mut();
    }

    let listener_address = listener_address_string.parse::<NetAddress>();
    let control_service_address = control_service_address_string.parse::<NetAddress>();

    match listener_address {
        Ok(listener_address) => match control_service_address {
            Ok(control_service_address) => {
                let ni = NodeIdentity::new(
                    (*secret_key).clone(),
                    control_service_address,
                    PeerFeatures::COMMUNICATION_CLIENT,
                );
                match ni {
                    Ok(ni) => {
                        let config = TariCommsConfig {
                            node_identity: Arc::new(ni.clone()),
                            peer_connection_listening_address: listener_address,
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
                    },
                    Err(_) => ptr::null_mut(),
                }
            },
            Err(_) => return ptr::null_mut(),
        },
        Err(_) => return ptr::null_mut(),
    }
}

/// Frees memory for a TariCommsConfig
///
/// ## Arguments
/// `wc` - The TariCommsConfig pointer
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn comms_config_destroy(wc: *mut TariCommsConfig) {
    if !wc.is_null() {
        Box::from_raw(wc);
    }
}

/// ---------------------------------------------------------------------------------------------- ///

/// ------------------------------------- Wallet -------------------------------------------------///

/// Creates a TariWallet
///
/// ## Arguments
/// `config` - The TariCommsConfig pointer
/// `log_path` - An optional file path to the file where the logs will be written. If no log is required pass *null*
/// pointer.
/// ## Returns
/// `*mut TariWallet` - Returns a pointer to a TariWallet, note that it returns ptr::null_mut()
/// if config is null, a wallet error was encountered or if the runtime could not be created
#[no_mangle]
pub unsafe extern "C" fn wallet_create(config: *mut TariCommsConfig, log_path: *const c_char) -> *mut TariWallet {
    if config.is_null() {
        return ptr::null_mut();
    }
    let mut logging_path_string = None;
    if !log_path.is_null() {
        logging_path_string = Some(CStr::from_ptr(log_path).to_str().unwrap().to_owned());
    }

    // TODO Gracefully handle the case where these expects would fail
    let runtime = Runtime::new();
    let factories = CryptoFactories::default();
    let w;

    match runtime {
        Ok(runtime) => {
            w = TariWallet::new(
                WalletConfig {
                    comms_config: (*config).clone(),
                    logging_path: logging_path_string,
                    factories,
                },
                WalletMemoryDatabase::new(),
                runtime,
            );
            match w {
                Ok(w) => Box::into_raw(Box::new(w)),
                Err(_) => ptr::null_mut(),
            }
        },
        Err(_) => ptr::null_mut(),
    }
}

/// This function will generate some test data in the wallet. The data generated will be
/// as follows:
///
/// - Some Contacts
/// - Add outputs to the wallet that make up its Available Balance that can be spent
/// - Create transaction history
///    - Pending Inbound Transactions
///     - Pending Outbound Transactions
///    - Completed Transactions
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_test_generate_data(wallet: *mut TariWallet) -> bool {
    if wallet.is_null() {
        return false;
    }
    match generate_wallet_test_data(&mut *wallet) {
        Ok(_) => true,
        _ => false,
    }
}

/// This function simulates an external `TariWallet` sending a transaction to this `TariWallet`
/// which will become a `TariPendingInboundTransaction`
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_test_receive_transaction(wallet: *mut TariWallet) -> bool {
    if wallet.is_null() {
        return false;
    }

    match receive_test_transaction(&mut *wallet) {
        Ok(_) => true,
        _ => false,
    }
}

/// This function simulates a receiver accepting and replying to a `TariPendingOutboundTransaction`.
/// This results in that transaction being "completed" and it's status set to `Broadcast` which
/// indicated it is in a base_layer mempool.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `tx` - The TariPendingOutboundTransaction
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_test_complete_sent_transaction(
    wallet: *mut TariWallet,
    tx: *mut TariPendingOutboundTransaction,
) -> bool
{
    if wallet.is_null() {
        return false;
    }
    if tx.is_null() {
        return false;
    }
    match complete_sent_transaction(&mut *wallet, (*tx).tx_id.clone()) {
        Ok(_) => true,
        _ => false,
    }
}

/// This function will simulate the process when a completed transaction is detected as mined on
/// the base layer. The function will update the status of the completed transaction AND complete
/// the transaction on the Output Manager Service which will update the status of the outputs
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `tx` - The TariCompletedTransaction pointer
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_test_mined(wallet: *mut TariWallet, tx: *mut TariCompletedTransaction) -> bool {
    if wallet.is_null() {
        return false;
    }
    if tx.is_null() {
        return false;
    }
    match mine_transaction(&mut *wallet, (*tx).tx_id.clone()) {
        Ok(_) => true,
        _ => false,
    }
}

/// This function simulates the detection of a `TariPendingInboundTransaction` as being broadcast
/// to base layer which means the Pending transaction must become a `TariCompletedTransaction` with
/// the `Broadcast` status.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `tx` - The TariPendingInboundTransaction pointer
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_test_transaction_broadcast(
    wallet: *mut TariWallet,
    tx: *mut TariPendingInboundTransaction,
) -> bool
{
    if wallet.is_null() {
        return false;
    }
    match broadcast_transaction(&mut *wallet, (*tx).tx_id.clone()) {
        Ok(_) => true,
        _ => false,
    }
}

/// Adds a base node peer to the TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `public_key` - The TariPublicKey pointer
/// `address` - The pointer to a char array
///
/// ## Returns
/// `bool` - Returns if successful or not
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

    let address_string;
    if !address.is_null() {
        address_string = CStr::from_ptr(address).to_str().unwrap().to_owned();
    } else {
        return false;
    }

    match (*wallet).add_base_node_peer((*public_key).clone(), address_string) {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Adds a TariContact to the TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `contact` - The TariContact pointer
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_add_contact(wallet: *mut TariWallet, contact: *mut TariContact) -> bool {
    if wallet.is_null() {
        return false;
    }
    if contact.is_null() {
        return false;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).contacts_service.save_contact((*contact).clone()))
    {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Removes a TariContact from the TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `tx` - The TariPendingInboundTransaction pointer
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_remove_contact(wallet: *mut TariWallet, contact: *mut TariContact) -> bool {
    if wallet.is_null() {
        return false;
    }
    if contact.is_null() {
        return false;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).contacts_service.remove_contact((*contact).public_key.clone()))
    {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Gets the available balance from a TariWallet. This is the balance the user can spend.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `c_ulonglong` - The available balance, 0 if wallet is null
#[no_mangle]
pub unsafe extern "C" fn wallet_get_available_balance(wallet: *mut TariWallet) -> c_ulonglong {
    if wallet.is_null() {
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).output_manager_service.get_balance())
    {
        Ok(b) => c_ulonglong::from(b.available_balance),
        Err(_) => 0,
    }
}

/// Gets the incoming balance from a `TariWallet`. This is the uncleared balance of Tari that is
/// expected to come into the `TariWallet` but is not yet spendable.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `c_ulonglong` - The incoming balance, 0 if wallet is null
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_incoming_balance(wallet: *mut TariWallet) -> c_ulonglong {
    if wallet.is_null() {
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).output_manager_service.get_balance())
    {
        Ok(b) => c_ulonglong::from(b.pending_incoming_balance),
        Err(_) => 0,
    }
}

/// Gets the outgoing balance from a `TariWallet`. This is the uncleared balance of Tari that has
/// been spent
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `c_ulonglong` - The outgoing balance, 0 if wallet is null
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_outgoing_balance(wallet: *mut TariWallet) -> c_ulonglong {
    if wallet.is_null() {
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).output_manager_service.get_balance())
    {
        Ok(b) => c_ulonglong::from(b.pending_outgoing_balance),
        Err(_) => 0,
    }
}

/// Sends a TariPendingOutboundTransaction
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `dest_public_key` - The TariPublicKey pointer of the peer
/// `amount` - The amount
/// `fee_per_gram` - The transaction fee
/// `message` - The pointer to a char array
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_send_transaction(
    wallet: *mut TariWallet,
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
        Err(_) => false,
    }
}

/// Get the TariContacts from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `*mut TariContacts` - returns the contacts, note that it returns ptr::null_mut() if
/// wallet is null
#[no_mangle]
pub unsafe extern "C" fn wallet_get_contacts(wallet: *mut TariWallet) -> *mut TariContacts {
    let mut contacts = Vec::new();
    if wallet.is_null() {
        return ptr::null_mut();
    }

    let retrieved_contacts = (*wallet).runtime.block_on((*wallet).contacts_service.get_contacts());
    match retrieved_contacts {
        Ok(retrieved_contacts) => {
            contacts.append(&mut retrieved_contacts.clone());
            Box::into_raw(Box::new(TariContacts(contacts)))
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Get the TariCompletedTransactions from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `*mut TariCompletedTransactions` - returns the transactions, note that it returns ptr::null_mut() if
/// wallet is null or an error is encountered
#[no_mangle]
pub unsafe extern "C" fn wallet_get_completed_transactions(wallet: *mut TariWallet) -> *mut TariCompletedTransactions {
    let mut completed = Vec::new();
    if wallet.is_null() {
        return ptr::null_mut();
    }

    let completed_transactions = (*wallet)
        .runtime
        .block_on((*wallet).transaction_service.get_completed_transactions());
    match completed_transactions {
        Ok(completed_transactions) => {
            for (_id, tx) in &completed_transactions {
                completed.push(tx.clone());
            }
            Box::into_raw(Box::new(TariCompletedTransactions(completed)))
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Get the TariPendingInboundTransactions from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `*mut TariPendingInboundTransactions` - returns the transactions, note that it returns ptr::null_mut() if
/// wallet is null or and error is encountered
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_inbound_transactions(
    wallet: *mut TariWallet,
) -> *mut TariPendingInboundTransactions {
    let mut pending = Vec::new();
    if wallet.is_null() {
        return ptr::null_mut();
    }

    let pending_transactions = (*wallet)
        .runtime
        .block_on((*wallet).transaction_service.get_pending_inbound_transactions());
    match pending_transactions {
        Ok(pending_transactions) => {
            for (_id, tx) in &pending_transactions {
                pending.push(tx.clone());
            }
            Box::into_raw(Box::new(TariPendingInboundTransactions(pending)))
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Get the TariPendingOutboundTransactions from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `*mut TariPendingOutboundTransactions` - returns the transactions, note that it returns ptr::null_mut() if
/// wallet is null or and error is encountered
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_outbound_transactions(
    wallet: *mut TariWallet,
) -> *mut TariPendingOutboundTransactions {
    let mut pending = Vec::new();
    if wallet.is_null() {
        return ptr::null_mut();
    }

    let pending_transactions = (*wallet)
        .runtime
        .block_on((*wallet).transaction_service.get_pending_outbound_transactions());
    match pending_transactions {
        Ok(pending_transactions) => {
            for (_id, tx) in &pending_transactions {
                pending.push(tx.clone());
            }
            Box::into_raw(Box::new(TariPendingOutboundTransactions(pending)))
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Get the TariCompletedTransaction from a TariWallet by its' TransactionId
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `transaction_id` - The TransactionId
///
/// ## Returns
/// `*mut TariCompletedTransaction` - returns the transaction, note that it returns ptr::null_mut() if
/// wallet is null, an error is encountered or if the transaction is not found
#[no_mangle]
pub unsafe extern "C" fn wallet_get_completed_transaction_by_id(
    wallet: *mut TariWallet,
    transaction_id: c_ulonglong,
) -> *mut TariCompletedTransaction
{
    if wallet.is_null() {
        return ptr::null_mut();
    }

    let pending_transactions = (*wallet)
        .runtime
        .block_on((*wallet).transaction_service.get_completed_transactions());

    match pending_transactions {
        Ok(pending_transactions) => {
            for (id, tx) in &pending_transactions {
                if id == &transaction_id {
                    let pending = tx.clone();
                    return Box::into_raw(Box::new(pending));
                }
            }
            return ptr::null_mut();
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Get the TariPendingInboundTransaction from a TariWallet by its' TransactionId
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `transaction_id` - The TransactionId
///
/// ## Returns
/// `*mut TariPendingInboundTransaction` - returns the transaction, note that it returns ptr::null_mut() if
/// wallet is null, an error is encountered or if the transaction is not found
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_inbound_transaction_by_id(
    wallet: *mut TariWallet,
    transaction_id: c_ulonglong,
) -> *mut TariPendingInboundTransaction
{
    if wallet.is_null() {
        return ptr::null_mut();
    }

    let pending_transactions = (*wallet)
        .runtime
        .block_on((*wallet).transaction_service.get_pending_inbound_transactions());

    match pending_transactions {
        Ok(pending_transactions) => {
            for (id, tx) in &pending_transactions {
                if id == &transaction_id {
                    let pending = tx.clone();
                    return Box::into_raw(Box::new(pending));
                }
            }
            return ptr::null_mut();
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Get the TariPendingOutboundTransaction from a TariWallet by its' TransactionId
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `transaction_id` - The TransactionId
///
/// ## Returns
/// `*mut TariPendingOutboundTransaction` - returns the transaction, note that it returns ptr::null_mut() if
/// wallet is null, an error is encountered or if the transaction is not found
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_outbound_transaction_by_id(
    wallet: *mut TariWallet,
    transaction_id: c_ulonglong,
) -> *mut TariPendingOutboundTransaction
{
    if wallet.is_null() {
        return ptr::null_mut();
    }

    let pending_transactions = (*wallet)
        .runtime
        .block_on((*wallet).transaction_service.get_pending_outbound_transactions());

    match pending_transactions {
        Ok(pending_transactions) => {
            for (id, tx) in &pending_transactions {
                if id == &transaction_id {
                    let pending = tx.clone();
                    return Box::into_raw(Box::new(pending));
                }
            }
            return ptr::null_mut();
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Get the TariPublicKey from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `*mut TariPublicKey` - returns the public key, note that ptr::null_mut() is returned
/// if wc is null
#[no_mangle]
pub unsafe extern "C" fn wallet_get_public_key(wallet: *mut TariWallet) -> *mut TariPublicKey {
    if wallet.is_null() {
        return ptr::null_mut();
    }
    let pk = (*wallet).comms.node_identity().public_key().clone();
    Box::into_raw(Box::new(pk))
}

/// Frees memory for a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
#[no_mangle]
pub unsafe extern "C" fn wallet_destroy(wallet: *mut TariWallet) {
    if !wallet.is_null() {
        let m = Box::from_raw(wallet);
        let l = m.shutdown();
        match l {
            Ok(_l) => {},
            Err(_) => {},
        }
    }
}

/// ------------------------------------- Callbacks -------------------------------------------- ///

/// Registers a callback function for when a TariPendingInboundTransaction is received
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `call` - The callback function pointer matching the function signature
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_callback_register_received_transaction(
    wallet: *mut TariWallet,
    call: unsafe extern "C" fn(*mut TariPendingInboundTransaction),
) -> bool
{
    let result = (*wallet)
        .runtime
        .block_on((*wallet).register_callback_received_transaction(call));
    match result {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Registers a callback function for when a reply is received for a TariPendingOutboundTransaction
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `call` - The callback function pointer matching the function signature
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_callback_register_received_transaction_reply(
    wallet: *mut TariWallet,
    call: unsafe extern "C" fn(*mut TariCompletedTransaction),
) -> bool
{
    let result = (*wallet)
        .runtime
        .block_on((*wallet).register_callback_received_transaction_reply(call));
    match result {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Registers a callback function for when a TariCompletedTransaction is mined
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `call` - The callback function pointer matching the function signature
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_callback_register_mined(
    wallet: *mut TariWallet,
    call: unsafe extern "C" fn(*mut TariCompletedTransaction),
) -> bool
{
    let result = (*wallet).runtime.block_on((*wallet).register_callback_mined(call));
    match result {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Registers a callback function for when TariPendingInboundTransaction broadcast is detected
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `call` - The callback function pointer matching the function signature
///
/// ## Returns
/// `bool` - Returns if successful or not
#[no_mangle]
pub unsafe extern "C" fn wallet_callback_register_transaction_broadcast(
    wallet: *mut TariWallet,
    call: unsafe extern "C" fn(*mut TariCompletedTransaction),
) -> bool
{
    let result = (*wallet)
        .runtime
        .block_on((*wallet).register_callback_transaction_broadcast(call));
    match result {
        Ok(_) => true,
        Err(_) => false,
    }
}

// TODO (Potentially) Add optional error parameter to methods which can return null

#[cfg(test)]
mod test {
    extern crate libc;
    use crate::*;
    use libc::{c_char, c_uchar, c_uint};
    use std::ffi::CString;
    use tari_wallet::testnet_utils::random_string;
    use tempdir::TempDir;

    unsafe extern "C" fn completed_callback(tx: *mut TariCompletedTransaction) {
        assert_eq!(tx.is_null(), false);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn inbound_callback(tx: *mut TariPendingInboundTransaction) {
        assert_eq!(tx.is_null(), false);
        pending_inbound_transaction_destroy(tx);
    }

    unsafe extern "C" fn mined_callback(tx: *mut TariCompletedTransaction) {
        assert_eq!(tx.is_null(), false);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn broadcast_callback(tx: *mut TariCompletedTransaction) {
        assert_eq!(tx.is_null(), false);
        completed_transaction_destroy(tx);
    }

    #[test]
    fn test_string_destroy() {
        unsafe {
            let m = CString::new("Test").unwrap();
            let m_ptr: *mut c_char = CString::into_raw(m) as *mut c_char;
            assert_ne!(m_ptr.is_null(), true);
            assert!(*m_ptr > 0); // dereference will return first character as integer, T as i8 = 84 > 0 = true
            string_destroy(m_ptr);
            assert_eq!(*m_ptr, 0); // dereference will return zero, avoids malloc error if attempting to evaluate by
                                   // other means.
        }
    }

    #[test]
    fn test_bytevector() {
        unsafe {
            let bytes: [c_uchar; 4] = [2, 114, 34, 255];
            let bytes_ptr = byte_vector_create(bytes.as_ptr(), bytes.len() as c_uint);
            let length = byte_vector_get_length(bytes_ptr);
            // println!("{:?}",c);
            assert_eq!(length, bytes.len() as c_uint);
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
            let private_bytes = private_key_get_bytes(private_key);
            let public_bytes = public_key_get_bytes(public_key);
            let private_key_length = byte_vector_get_length(private_bytes);
            let public_key_length = byte_vector_get_length(public_bytes);
            assert_eq!(private_key_length, 32);
            assert_eq!(public_key_length, 32);
            assert_ne!(private_bytes, public_bytes);
            private_key_destroy(private_key);
            public_key_destroy(public_key);
            byte_vector_destroy(public_bytes);
            byte_vector_destroy(private_bytes);
        }
    }

    #[test]
    fn test_contact() {
        unsafe {
            let test_contact_private_key = private_key_generate();
            let test_contact_public_key = public_key_from_private_key(test_contact_private_key);
            let test_str = "Test Contact";
            let test_contact_str = CString::new(test_str).unwrap();
            let test_contact_alias: *const c_char = CString::into_raw(test_contact_str) as *const c_char;
            let test_contact = contact_create(test_contact_alias, test_contact_public_key);
            let alias = contact_get_alias(test_contact);
            let alias_string = CString::from_raw(alias).to_str().unwrap().to_owned();
            assert_eq!(alias_string, test_str);
            let contact_key = contact_get_public_key(test_contact);
            let contact_key_bytes = public_key_get_bytes(contact_key);
            let contact_bytes_len = byte_vector_get_length(contact_key_bytes);
            assert_eq!(contact_bytes_len, 32);
            contact_destroy(test_contact);
            public_key_destroy(test_contact_public_key);
            private_key_destroy(test_contact_private_key);
            string_destroy(test_contact_alias as *mut c_char);
            byte_vector_destroy(contact_key_bytes);
        }
    }

    #[test]
    fn test_wallet_ffi() {
        unsafe {
            let secret_key_alice = private_key_generate();
            let public_key_alice = public_key_from_private_key(secret_key_alice.clone());
            let db_name_alice = CString::new(random_string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice.clone()) as *const c_char;
            let db_path_alice = CString::new(
                TempDir::new(random_string(8).as_str())
                    .unwrap()
                    .path()
                    .to_str()
                    .unwrap(),
            )
            .unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice.clone()) as *const c_char;
            let address_alice = CString::new("127.0.0.1:21443").unwrap();
            let address_alice_str: *const c_char = CString::into_raw(address_alice.clone()) as *const c_char;

            let address_listener_alice = CString::new("127.0.0.1:0").unwrap();
            let address_listener_alice_str: *const c_char =
                CString::into_raw(address_listener_alice.clone()) as *const c_char;
            let alice_config = comms_config_create(
                address_alice_str,
                address_listener_alice_str,
                db_name_alice_str,
                db_path_alice_str,
                secret_key_alice,
            );
            let alice_wallet = wallet_create(alice_config, ptr::null());
            let secret_key_bob = private_key_generate();
            let public_key_bob = public_key_from_private_key(secret_key_bob.clone());
            let db_name_bob = CString::new(random_string(8).as_str()).unwrap();
            let db_name_bob_str: *const c_char = CString::into_raw(db_name_bob.clone()) as *const c_char;
            let db_path_bob = CString::new(
                TempDir::new(random_string(8).as_str())
                    .unwrap()
                    .path()
                    .to_str()
                    .unwrap(),
            )
            .unwrap();
            let db_path_bob_str: *const c_char = CString::into_raw(db_path_bob.clone()) as *const c_char;
            let address_bob = CString::new("127.0.0.1:21441").unwrap();
            let address_bob_str: *const c_char = CString::into_raw(address_bob.clone()) as *const c_char;
            let address_listener_bob = CString::new("127.0.0.1:0").unwrap();
            let address_listener_bob_str: *const c_char =
                CString::into_raw(address_listener_bob.clone()) as *const c_char;
            let bob_config = comms_config_create(
                address_bob_str,
                address_listener_bob_str,
                db_name_bob_str,
                db_path_bob_str,
                secret_key_bob,
            );
            let bob_wallet = wallet_create(bob_config, ptr::null());

            let mut peer_added = wallet_add_base_node_peer(alice_wallet, public_key_bob.clone(), address_bob_str);
            assert_eq!(peer_added, true);
            peer_added = wallet_add_base_node_peer(bob_wallet, public_key_alice.clone(), address_alice_str);
            assert_eq!(peer_added, true);

            let test_contact_private_key = private_key_generate();
            let test_contact_public_key = public_key_from_private_key(test_contact_private_key);
            let test_contact_str = CString::new("Test Contact").unwrap();
            let test_contact_alias: *const c_char = CString::into_raw(test_contact_str) as *const c_char;
            let test_contact = contact_create(test_contact_alias, test_contact_public_key);
            let contact_added = wallet_add_contact(alice_wallet, test_contact);
            assert_eq!(contact_added, true);
            let contact_removed = wallet_remove_contact(alice_wallet, test_contact);
            assert_eq!(contact_removed, true);
            contact_destroy(test_contact);
            public_key_destroy(test_contact_public_key);
            private_key_destroy(test_contact_private_key);
            string_destroy(test_contact_alias as *mut c_char);

            let mut callback = wallet_callback_register_received_transaction(alice_wallet, inbound_callback);
            assert_eq!(callback, true);
            callback = wallet_callback_register_received_transaction_reply(alice_wallet, completed_callback);
            assert_eq!(callback, true);
            callback = wallet_callback_register_mined(alice_wallet, mined_callback);
            assert_eq!(callback, true);
            callback = wallet_callback_register_transaction_broadcast(alice_wallet, broadcast_callback);
            assert_eq!(callback, true);
            let generated = wallet_test_generate_data(alice_wallet);
            assert_eq!(generated, true);

            assert_eq!(
                (wallet_get_completed_transactions(&mut (*alice_wallet))).is_null(),
                false
            );
            assert_eq!(
                (wallet_get_pending_inbound_transactions(&mut (*alice_wallet))).is_null(),
                false
            );
            assert_eq!(
                (wallet_get_pending_outbound_transactions(&mut (*alice_wallet))).is_null(),
                false
            );
            // TODO: Test transaction collection and transaction methods

            let completed_transactions: std::collections::HashMap<
                u64,
                tari_wallet::transaction_service::storage::database::CompletedTransaction,
            > = (*alice_wallet)
                .runtime
                .block_on((*alice_wallet).transaction_service.get_completed_transactions())
                .unwrap();
            let inbound_transactions: std::collections::HashMap<
                u64,
                tari_wallet::transaction_service::storage::database::InboundTransaction,
            > = (*alice_wallet)
                .runtime
                .block_on((*alice_wallet).transaction_service.get_pending_inbound_transactions())
                .unwrap();
            for (_k, v) in inbound_transactions {
                wallet_test_transaction_broadcast(alice_wallet, Box::into_raw(Box::new(v.clone())));
            }

            for (_k, v) in completed_transactions {
                wallet_test_mined(alice_wallet, Box::into_raw(Box::new(v.clone())));
            }

            let contacts = wallet_get_contacts(alice_wallet);
            assert_eq!(contacts_get_length(contacts), 4);

            // free string memory
            string_destroy(address_listener_alice_str as *mut c_char);
            string_destroy(address_listener_bob_str as *mut c_char);
            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            string_destroy(db_name_bob_str as *mut c_char);
            string_destroy(db_path_bob_str as *mut c_char);
            string_destroy(address_bob_str as *mut c_char);
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
