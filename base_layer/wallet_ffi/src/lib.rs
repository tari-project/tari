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

#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]

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
//!     becoming a `CompletedTransaction` with the `Completed` status. This means that the transaction has been
//!     negotiated between the parties and is now ready to be broadcast to the Base Layer. The funds are still
//!     encumbered as pending because the transaction has not been mined yet.
//! 3.  The finalized `CompletedTransaction' will be sent back to the the receiver so that they have a copy.
//! 4.  The wallet will broadcast the `CompletedTransaction` to a Base Node to be added to the mempool. its status will
//!     from `Completed` to `Broadcast.
//! 5.  Wait until the transaction is mined. The `CompleteTransaction` status will then move from `Broadcast` to `Mined`
//!     and the pending funds will be spent and received.
//!
//! ## Receive a Transaction
//! 1.  When a transaction is received it will appear as an `InboundTransaction` and the amount to be received will
//!     appear as a `PendingIncomingBalance`. The wallet backend will be listening for these transactions and will
//!     immediately reply to the sending wallet.
//! 2.  The sender will send back the finalized `CompletedTransaction`
//! 3.  This wallet will also broadcast the `CompletedTransaction` to a Base Node to be added to the mempool, its status
//!     will move from `Completed` to `Broadcast`. This is done so that the Receiver can be sure the finalized
//!     transaction is broadcast.
//! 6.  This wallet will then monitor the Base Layer to see when the transaction is mined which means the
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
//!     This will move the `PendingOutboundTransaction` to become a `CompletedTransaction` with the `Completed` status.
//! 3.  Call the 'broadcast_transaction(...)` function with the tx_id of the sent transaction and its status will move
//!     from 'Completed' to 'Broadcast' which means it has been broadcast to the Base Layer Mempool but not mined yet.
//!     from 'Completed' to 'Broadcast' which means it has been broadcast to the Base Layer Mempool but not mined yet.
//! 4.  Call the `mined_transaction(...)` function with the tx_id of the sent transaction which will change
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

#![recursion_limit = "512"]

#[cfg(test)]
#[macro_use]
extern crate lazy_static;
mod callback_handler;
mod error;

use crate::{
    callback_handler::CallbackHandler,
    error::{InterfaceError, TransactionError},
};
use core::ptr;
use error::LibWalletError;
use libc::{c_char, c_int, c_longlong, c_uchar, c_uint, c_ulonglong, c_ushort};
use log::{LevelFilter, *};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
};
use rand::rngs::OsRng;
use std::{
    boxed::Box,
    ffi::{CStr, CString},
    path::PathBuf,
    slice,
    sync::Arc,
    time::Duration,
};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerFeatures},
    socks,
    tor,
    types::CommsSecretKey,
};
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_core::transactions::{tari_amount::MicroTari, types::CryptoFactories};
use tari_crypto::{
    keys::{PublicKey, SecretKey},
    tari_utilities::ByteArray,
};
use tari_p2p::transport::{TorConfig, TransportType};
use tari_shutdown::Shutdown;
use tari_utilities::{hex, hex::Hex, message_format::MessageFormat};
use tari_wallet::{
    contacts_service::storage::database::Contact,
    error::WalletError,
    output_manager_service::protocols::txo_validation_protocol::TxoValidationRetry,
    storage::{
        database::WalletDatabase,
        sqlite_db::WalletSqliteDatabase,
        sqlite_utilities::{
            initialize_sqlite_database_backends,
            partial_wallet_backup,
            run_migration_and_create_sqlite_connection,
        },
    },
    testnet_utils::{
        broadcast_transaction,
        complete_sent_transaction,
        finalize_received_transaction,
        generate_wallet_test_data,
        get_next_memory_address,
        mine_transaction,
        receive_test_transaction,
    },
    transaction_service::{
        config::TransactionServiceConfig,
        error::TransactionServiceError,
        storage::{
            database::TransactionDatabase,
            models::{
                CompletedTransaction,
                InboundTransaction,
                OutboundTransaction,
                TransactionDirection,
                TransactionStatus,
            },
        },
    },
    util::emoji::{emoji_set, EmojiId},
    wallet::WalletConfig,
    Wallet,
};

use futures::StreamExt;
use log4rs::append::{
    rolling_file::{
        policy::compound::{roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger, CompoundPolicy},
        RollingFileAppender,
    },
    Append,
};
use tari_core::consensus::Network;
use tari_p2p::transport::TransportType::Tor;
use tari_wallet::{
    error::WalletStorageError,
    output_manager_service::protocols::txo_validation_protocol::TxoValidationType,
    WalletSqlite,
};
use tokio::runtime::Runtime;

const LOG_TARGET: &str = "wallet_ffi";

pub type TariTransportType = tari_p2p::transport::TransportType;
pub type TariPublicKey = tari_comms::types::CommsPublicKey;
pub type TariPrivateKey = tari_comms::types::CommsSecretKey;
pub type TariCommsConfig = tari_p2p::initialization::CommsConfig;
pub type TariExcess = tari_core::transactions::types::Commitment;
pub type TariExcessPublicNonce = tari_crypto::ristretto::RistrettoPublicKey;
pub type TariExcessSignature = tari_crypto::ristretto::RistrettoSecretKey;

pub struct TariContacts(Vec<TariContact>);

pub type TariContact = tari_wallet::contacts_service::storage::database::Contact;
pub type TariCompletedTransaction = tari_wallet::transaction_service::storage::models::CompletedTransaction;

pub struct TariCompletedTransactions(Vec<TariCompletedTransaction>);

pub type TariPendingInboundTransaction = tari_wallet::transaction_service::storage::models::InboundTransaction;
pub type TariPendingOutboundTransaction = tari_wallet::transaction_service::storage::models::OutboundTransaction;

pub struct TariPendingInboundTransactions(Vec<TariPendingInboundTransaction>);

pub struct TariPendingOutboundTransactions(Vec<TariPendingOutboundTransaction>);

#[derive(Debug, PartialEq, Clone)]
pub struct ByteVector(Vec<c_uchar>); // declared like this so that it can be exposed to external header

#[derive(Debug, PartialEq)]
pub struct EmojiSet(Vec<ByteVector>);

pub struct TariSeedWords(Vec<String>);

pub struct TariWallet {
    wallet: WalletSqlite,
    runtime: Runtime,
    shutdown: Shutdown,
}

/// -------------------------------- Strings ------------------------------------------------ ///

/// Frees memory for a char array
///
/// ## Arguments
/// `ptr` - The pointer to be freed
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C.
///
/// # Safety
/// None
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut ByteVector` - Pointer to the created ByteVector. Note that it will be ptr::null_mut()
/// if the byte_array pointer was null or if the elements in the byte_vector don't match
/// element_count when it is created
///
/// # Safety
/// The ```byte_vector_destroy``` function must be called when finished with a ByteVector to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn byte_vector_create(
    byte_array: *const c_uchar,
    element_count: c_uint,
    error_out: *mut c_int,
) -> *mut ByteVector
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut bytes = ByteVector(Vec::new());
    if byte_array.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("byte_array".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        let array: &[c_uchar] = slice::from_raw_parts(byte_array, element_count as usize);
        bytes.0 = array.to_vec();
        if bytes.0.len() != element_count as usize {
            error = LibWalletError::from(InterfaceError::AllocationError).code;
            ptr::swap(error_out, &mut error as *mut c_int);
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
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn byte_vector_destroy(bytes: *mut ByteVector) {
    if !bytes.is_null() {
        Box::from_raw(bytes);
    }
}

/// Gets a c_uchar at position in a ByteVector
///
/// ## Arguments
/// `ptr` - The pointer to a ByteVector
/// `position` - The integer position
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uchar` - Returns a character. Note that the character will be a null terminator (0) if ptr
/// is null or if the position is invalid
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn byte_vector_get_at(ptr: *mut ByteVector, position: c_uint, error_out: *mut c_int) -> c_uchar {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if ptr.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("ptr".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0 as c_uchar;
    }
    let len = byte_vector_get_length(ptr, error_out) as c_int - 1; // clamp to length
    if len < 0 || position > len as c_uint {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0 as c_uchar;
    }
    (*ptr).0[position as usize]
}

/// Gets the number of elements in a ByteVector
///
/// ## Arguments
/// `ptr` - The pointer to a ByteVector
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns the integer number of elements in the ByteVector. Note that it will be zero
/// if ptr is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn byte_vector_get_length(vec: *const ByteVector, error_out: *mut c_int) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if vec.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("vec".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*vec).0.len() as c_uint
}

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Public Key ------------------------------------------------ ///

/// Creates a TariPublicKey from a ByteVector
///
/// ## Arguments
/// `bytes` - The pointer to a ByteVector
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `TariPublicKey` - Returns a public key. Note that it will be ptr::null_mut() if bytes is null or
/// if there was an error with the contents of bytes
///
/// # Safety
/// The ```public_key_destroy``` function must be called when finished with a TariPublicKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn public_key_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TariPublicKey {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let v;
    if bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        v = (*bytes).0.clone();
    }
    let pk = TariPublicKey::from_bytes(&v);
    match pk {
        Ok(pk) => Box::into_raw(Box::new(pk)),
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Frees memory for a TariPublicKey
///
/// ## Arguments
/// `pk` - The pointer to a TariPublicKey
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn public_key_destroy(pk: *mut TariPublicKey) {
    if !pk.is_null() {
        Box::from_raw(pk);
    }
}

/// Frees memory for a TariExcess
///
/// ## Arguments
/// `x` - The pointer to a TariExcess
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn excess_destroy(x: *mut TariExcess) {
    if !x.is_null() {
        Box::from_raw(x);
    }
}

/// Frees memory for a TariExcessPublicNonce
///
/// ## Arguments
/// `r` - The pointer to a TariExcessPublicNonce
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn nonce_destroy(r: *mut TariExcessPublicNonce) {
    if !r.is_null() {
        Box::from_raw(r);
    }
}

/// Frees memory for a TariExcessSignature
///
/// ## Arguments
/// `s` - The pointer to a TariExcessSignature
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn signature_destroy(s: *mut TariExcessSignature) {
    if !s.is_null() {
        Box::from_raw(s);
    }
}

/// Gets a ByteVector from a TariPublicKey
///
/// ## Arguments
/// `pk` - The pointer to a TariPublicKey
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut ByteVector` - Returns a pointer to a ByteVector. Note that it returns ptr::null_mut() if pk is null
///
/// # Safety
/// The ```byte_vector_destroy``` function must be called when finished with the ByteVector to prevent a memory leak.
#[no_mangle]
pub unsafe extern "C" fn public_key_get_bytes(pk: *mut TariPublicKey, error_out: *mut c_int) -> *mut ByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut bytes = ByteVector(Vec::new());
    if pk.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("pk".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        bytes.0 = (*pk).to_vec();
    }
    Box::into_raw(Box::new(bytes))
}

/// Creates a TariPublicKey from a TariPrivateKey
///
/// ## Arguments
/// `secret_key` - The pointer to a TariPrivateKey
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPublicKey` - Returns a pointer to a TariPublicKey
///
/// # Safety
/// The ```private_key_destroy``` method must be called when finished with a private key to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn public_key_from_private_key(
    secret_key: *mut TariPrivateKey,
    error_out: *mut c_int,
) -> *mut TariPublicKey
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if secret_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("secret_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let m = TariPublicKey::from_secret_key(&(*secret_key));
    Box::into_raw(Box::new(m))
}

/// Creates a TariPublicKey from a char array
///
/// ## Arguments
/// `key` - The pointer to a char array which is hex encoded
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPublicKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut()
/// if key is null or if there was an error creating the TariPublicKey from key
///
/// # Safety
/// The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn public_key_from_hex(key: *const c_char, error_out: *mut c_int) -> *mut TariPublicKey {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let key_str;
    if key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        key_str = CStr::from_ptr(key).to_str().unwrap().to_owned();
    }

    let public_key = TariPublicKey::from_hex(key_str.as_str());
    match public_key {
        Ok(public_key) => Box::into_raw(Box::new(public_key)),
        Err(e) => {
            error!(target: LOG_TARGET, "Error creating a Public Key from Hex: {:?}", e);
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Creates a char array from a TariPublicKey in emoji format
///
/// ## Arguments
/// `pk` - The pointer to a TariPublicKey
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns empty
/// if emoji is null or if there was an error creating the emoji string from TariPublicKey
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn public_key_to_emoji_id(pk: *mut TariPublicKey, error_out: *mut c_int) -> *mut c_char {
    let mut error = 0;
    let mut result = CString::new("").unwrap();
    ptr::swap(error_out, &mut error as *mut c_int);
    if pk.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return CString::into_raw(result);
    }

    let emoji = EmojiId::from_pubkey(&(*pk));
    result = CString::new(emoji.as_str()).unwrap();
    CString::into_raw(result)
}

/// Creates a TariPublicKey from a char array in emoji format
///
/// ## Arguments
/// `const *c_char` - The pointer to a TariPublicKey
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a TariPublicKey. Note that it returns null on error.
///
/// # Safety
/// The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn emoji_id_to_public_key(emoji: *const c_char, error_out: *mut c_int) -> *mut TariPublicKey {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if emoji.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("emoji".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    match CStr::from_ptr(emoji)
        .to_str()
        .map_err(|_| ())
        .and_then(EmojiId::str_to_pubkey)
    {
        Ok(pk) => Box::into_raw(Box::new(pk)),
        Err(_) => {
            error = LibWalletError::from(InterfaceError::InvalidEmojiId).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Private Key ----------------------------------------------- ///

/// Creates a TariPrivateKey from a ByteVector
///
/// ## Arguments
/// `bytes` - The pointer to a ByteVector
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPrivateKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut()
/// if bytes is null or if there was an error creating the TariPrivateKey from bytes
///
/// # Safety
/// The ```private_key_destroy``` method must be called when finished with a TariPrivateKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn private_key_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TariPrivateKey {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let v;
    if bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        v = (*bytes).0.clone();
    }
    let pk = TariPrivateKey::from_bytes(&v);
    match pk {
        Ok(pk) => Box::into_raw(Box::new(pk)),
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Frees memory for a TariPrivateKey
///
/// ## Arguments
/// `pk` - The pointer to a TariPrivateKey
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut ByteVectror` - Returns a pointer to a ByteVector. Note that it returns ptr::null_mut()
/// if pk is null
///
/// # Safety
/// The ```byte_vector_destroy``` must be called when finished with a ByteVector to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn private_key_get_bytes(pk: *mut TariPrivateKey, error_out: *mut c_int) -> *mut ByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut bytes = ByteVector(Vec::new());
    if pk.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("pk".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        bytes.0 = (*pk).to_vec();
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
///
/// # Safety
/// The ```private_key_destroy``` method must be called when finished with a TariPrivateKey to prevent a memory leak.
#[no_mangle]
pub unsafe extern "C" fn private_key_generate() -> *mut TariPrivateKey {
    let secret_key = TariPrivateKey::random(&mut OsRng);
    Box::into_raw(Box::new(secret_key))
}

/// Creates a TariPrivateKey from a char array
///
/// ## Arguments
/// `key` - The pointer to a char array which is hex encoded
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPrivateKey` - Returns a pointer to a TariPublicKey. Note that it returns ptr::null_mut()
/// if key is null or if there was an error creating the TariPrivateKey from key
///
/// # Safety
/// The ```private_key_destroy``` method must be called when finished with a TariPrivateKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn private_key_from_hex(key: *const c_char, error_out: *mut c_int) -> *mut TariPrivateKey {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let key_str;
    if key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        key_str = CStr::from_ptr(key).to_str().unwrap().to_owned();
    }

    let secret_key = TariPrivateKey::from_hex(key_str.as_str());

    match secret_key {
        Ok(secret_key) => Box::into_raw(Box::new(secret_key)),
        Err(e) => {
            error!(target: LOG_TARGET, "Error creating a Public Key from Hex: {:?}", e);

            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// -------------------------------------------------------------------------------------------- ///
/// ----------------------------------- Seed Words ----------------------------------------------///

/// Gets the length of TariSeedWords
///
/// ## Arguments
/// `seed_words` - The pointer to a TariSeedWords
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns number of elements in , zero if contacts is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn seed_words_get_length(seed_words: *const TariSeedWords, error_out: *mut c_int) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut len = 0;
    if seed_words.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("seed words".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        len = (*seed_words).0.len();
    }
    len as c_uint
}

/// Gets a seed word from TariSeedWords at position
///
/// ## Arguments
/// `seed_words` - The pointer to a TariSeedWords
/// `position` - The integer position
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns an empty char array if
/// TariSeedWords collection is null or the position is invalid
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn seed_words_get_at(
    seed_words: *mut TariSeedWords,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut c_char
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut word = CString::new("").unwrap();
    if seed_words.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("seed words".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        let len = (*seed_words).0.len();
        if position > len as u32 {
            error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        } else {
            word = CString::new((*seed_words).0[position as usize].clone()).unwrap()
        }
    }
    CString::into_raw(word)
}

/// Frees memory for a TariSeedWords
///
/// ## Arguments
/// `seed_words` - The pointer to a TariSeedWords
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn seed_words_destroy(seed_words: *mut TariSeedWords) {
    if !seed_words.is_null() {
        Box::from_raw(seed_words);
    }
}

/// ----------------------------------- Contact -------------------------------------------------///

/// Creates a TariContact
///
/// ## Arguments
/// `alias` - The pointer to a char array
/// `public_key` - The pointer to a TariPublicKey
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariContact` - Returns a pointer to a TariContact. Note that it returns ptr::null_mut()
/// if alias is null or if pk is null
///
/// # Safety
/// The ```contact_destroy``` method must be called when finished with a TariContact
#[no_mangle]
pub unsafe extern "C" fn contact_create(
    alias: *const c_char,
    public_key: *mut TariPublicKey,
    error_out: *mut c_int,
) -> *mut TariContact
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let alias_string;
    if alias.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("alias".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        alias_string = CStr::from_ptr(alias).to_str().unwrap().to_owned();
    }

    if public_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("public_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let contact = Contact {
        alias: alias_string,
        public_key: (*public_key).clone(),
    };
    Box::into_raw(Box::new(contact))
}

/// Gets the alias of the TariContact
///
/// ## Arguments
/// `contact` - The pointer to a TariContact
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns an empty char array if
/// contact is null
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn contact_get_alias(contact: *mut TariContact, error_out: *mut c_int) -> *mut c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut a = CString::new("").unwrap();
    if contact.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("contact".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        a = CString::new((*contact).alias.clone()).unwrap();
    }
    CString::into_raw(a)
}

/// Gets the TariPublicKey of the TariContact
///
/// ## Arguments
/// `contact` - The pointer to a TariContact
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPublicKey` - Returns a pointer to a TariPublicKey. Note that it returns
/// ptr::null_mut() if contact is null
///
/// # Safety
/// The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn contact_get_public_key(
    contact: *mut TariContact,
    error_out: *mut c_int,
) -> *mut TariPublicKey
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if contact.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("contact".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
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
///
/// # Safety
/// None
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns number of elements in , zero if contacts is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn contacts_get_length(contacts: *mut TariContacts, error_out: *mut c_int) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut len = 0;
    if contacts.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("contacts".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        len = (*contacts).0.len();
    }
    len as c_uint
}

/// Gets a TariContact from TariContacts at position
///
/// ## Arguments
/// `contacts` - The pointer to a TariContacts
/// `position` - The integer position
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariContact` - Returns a TariContact, note that it returns ptr::null_mut() if contacts is
/// null or position is invalid
///
/// # Safety
/// The ```contact_destroy``` method must be called when finished with a TariContact to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn contacts_get_at(
    contacts: *mut TariContacts,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariContact
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if contacts.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("contacts".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let len = contacts_get_length(contacts, error_out) as c_int - 1;
    if len < 0 || position > len as c_uint {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
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
///
/// # Safety
/// None
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns the number of elements in a TariCompletedTransactions, note that it will be
/// zero if transactions is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transactions_get_length(
    transactions: *mut TariCompletedTransactions,
    error_out: *mut c_int,
) -> c_uint
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut len = 0;
    if transactions.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        len = (*transactions).0.len();
    }
    len as c_uint
}

/// Gets a TariCompletedTransaction from a TariCompletedTransactions at position
///
/// ## Arguments
/// `transactions` - The pointer to a TariCompletedTransactions
/// `position` - The integer position
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariCompletedTransaction` - Returns a pointer to a TariCompletedTransaction,
/// note that ptr::null_mut() is returned if transactions is null or position is invalid
///
/// # Safety
/// The ```completed_transaction_destroy``` method must be called when finished with a TariCompletedTransaction to
/// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transactions_get_at(
    transactions: *mut TariCompletedTransactions,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariCompletedTransaction
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transactions.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transactions".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let len = completed_transactions_get_length(transactions, error_out) as c_int - 1;
    if len < 0 || position > len as c_uint {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
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
///
/// # Safety
/// None
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns the number of elements in a TariPendingOutboundTransactions, note that it will be
/// zero if transactions is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transactions_get_length(
    transactions: *mut TariPendingOutboundTransactions,
    error_out: *mut c_int,
) -> c_uint
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut len = 0;
    if transactions.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        len = (*transactions).0.len();
    }

    len as c_uint
}

/// Gets a TariPendingOutboundTransaction of a TariPendingOutboundTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariPendingOutboundTransactions
/// `position` - The integer position
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPendingOutboundTransaction` - Returns a pointer to a TariPendingOutboundTransaction,
/// note that ptr::null_mut() is returned if transactions is null or position is invalid
///
/// # Safety
/// The ```pending_outbound_transaction_destroy``` method must be called when finished with a
/// TariPendingOutboundTransaction to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transactions_get_at(
    transactions: *mut TariPendingOutboundTransactions,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariPendingOutboundTransaction
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transactions.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let len = pending_outbound_transactions_get_length(transactions, error_out) as c_int - 1;
    if len < 0 || position > len as c_uint {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
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
///
/// # Safety
/// None
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns the number of elements in a TariPendingInboundTransactions, note that
/// it will be zero if transactions is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transactions_get_length(
    transactions: *mut TariPendingInboundTransactions,
    error_out: *mut c_int,
) -> c_uint
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut len = 0;
    if transactions.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        len = (*transactions).0.len();
    }
    len as c_uint
}

/// Gets a TariPendingInboundTransaction of a TariPendingInboundTransactions
///
/// ## Arguments
/// `transactions` - The pointer to a TariPendingInboundTransactions
/// `position` - The integer position
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPendingOutboundTransaction` - Returns a pointer to a TariPendingInboundTransaction,
/// note that ptr::null_mut() is returned if transactions is null or position is invalid
///
/// # Safety
/// The ```pending_inbound_transaction_destroy``` method must be called when finished with a
/// TariPendingOutboundTransaction to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transactions_get_at(
    transactions: *mut TariPendingInboundTransactions,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariPendingInboundTransaction
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transactions.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let len = pending_inbound_transactions_get_length(transactions, error_out) as c_int - 1;
    if len < 0 || position > len as c_uint {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
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
///
/// # Safety
/// None
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the TransactionID, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_transaction_id(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).tx_id as c_ulonglong
}

/// Gets the destination TariPublicKey of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPublicKey` - Returns the destination TariPublicKey, note that it will be
/// ptr::null_mut() if transaction is null
///
/// # Safety
/// The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_destination_public_key(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *mut TariPublicKey
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let m = (*transaction).destination_public_key.clone();
    Box::into_raw(Box::new(m))
}

/// Gets the TariExcess of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariExcess` - Returns the transaction excess, note that it will be
/// ptr::null_mut() if transaction is null, if the transaction status is Pending, or if the number of kernels is not
/// exactly one.
///
/// # Safety
/// The ```excess_destroy``` method must be called when finished with a TariExcess to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_excess(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *mut TariExcess
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    // check the tx is not in pending state
    if matches!(
        (*transaction).status,
        TransactionStatus::Pending | TransactionStatus::Imported
    ) {
        let msg = format!("Incorrect transaction status: {}", (*transaction).status);
        error = LibWalletError::from(TransactionError::StatusError(msg)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let kernels = (*transaction).transaction.get_body().kernels();

    // currently we presume that each CompletedTransaction only has 1 kernel
    // if that changes this will need to be accounted for
    if kernels.len() != 1 {
        let msg = format!("Expected 1 kernel, got {}", kernels.len());
        error = LibWalletError::from(TransactionError::KernelError(msg)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let x = kernels[0].excess.clone();
    Box::into_raw(Box::new(x))
}

/// Gets the TariExcessPublicNonce of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariExcessPublicNonce` - Returns the transaction excess public nonce, note that it will be
/// ptr::null_mut() if transaction is null, if the transaction status is Pending, or if the number of kernels is not
/// exactly one.
///
/// # Safety
/// The ```nonce_destroy``` method must be called when finished with a TariExcessPublicNonce to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_public_nonce(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *mut TariExcessPublicNonce
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    // check the tx is not in pending state
    if matches!(
        (*transaction).status,
        TransactionStatus::Pending | TransactionStatus::Imported
    ) {
        let msg = format!("Incorrect transaction status: {}", (*transaction).status);
        error = LibWalletError::from(TransactionError::StatusError(msg)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let kernels = (*transaction).transaction.get_body().kernels();

    // currently we presume that each CompletedTransaction only has 1 kernel
    // if that changes this will need to be accounted for
    if kernels.len() != 1 {
        let msg = format!("Expected 1 kernel, got {}", kernels.len());
        error = LibWalletError::from(TransactionError::KernelError(msg)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let r = kernels[0].excess_sig.get_public_nonce().clone();
    Box::into_raw(Box::new(r))
}

/// Gets the TariExcessSignature of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariExcessSignature` - Returns the transaction excess signature, note that it will be
/// ptr::null_mut() if transaction is null, if the transaction status is Pending, or if the number of kernels is not
/// exactly one.
///
/// # Safety
/// The ```signature_destroy``` method must be called when finished with a TariExcessSignature to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_signature(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *mut TariExcessSignature
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    // check the tx is not in pending state
    if matches!(
        (*transaction).status,
        TransactionStatus::Pending | TransactionStatus::Imported
    ) {
        let msg = format!("Incorrect transaction status: {}", (*transaction).status);
        error = LibWalletError::from(TransactionError::StatusError(msg)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let kernels = (*transaction).transaction.get_body().kernels();

    // currently we presume that each CompletedTransaction only has 1 kernel
    // if that changes this will need to be accounted for
    if kernels.len() != 1 {
        let msg = format!("Expected 1 kernel, got {}", kernels.len());
        error = LibWalletError::from(TransactionError::KernelError(msg)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let s = kernels[0].excess_sig.get_signature().clone();
    Box::into_raw(Box::new(s))
}

/// Gets the source TariPublicKey of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPublicKey` - Returns the source TariPublicKey, note that it will be
/// ptr::null_mut() if transaction is null
///
/// # Safety
/// The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_source_public_key(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *mut TariPublicKey
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let m = (*transaction).source_public_key.clone();
    Box::into_raw(Box::new(m))
}

/// Gets the status of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_int` - Returns the status which corresponds to:
/// | Value | Interpretation |
/// |---|---|
/// |  -1 | TxNullError |
/// |   0 | Completed   |
/// |   1 | Broadcast   |
/// |   2 | Mined       |
/// |   3 | Imported    |
/// |   4 | Pending     |
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_status(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> c_int
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return -1;
    }
    let status = (*transaction).status.clone();
    status as c_int
}

/// Gets the amount of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the amount, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_amount(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    c_ulonglong::from((*transaction).amount)
}

/// Gets the fee of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the fee, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_fee(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    c_ulonglong::from((*transaction).fee)
}

/// Gets the timestamp of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the timestamp, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_timestamp(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> c_longlong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).timestamp.timestamp() as c_longlong
}

/// Gets the message of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*const c_char` - Returns the pointer to the char array, note that it will return a pointer
/// to an empty char array if transaction is null
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with string coming from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_message(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *const c_char
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let message = (*transaction).message.clone();
    let mut result = CString::new("").unwrap();
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result.into_raw();
    }

    result = CString::new(message).unwrap();
    result.into_raw()
}

/// This function checks to determine if a TariCompletedTransaction was originally a TariPendingOutboundTransaction
///
/// ## Arguments
/// `tx` - The TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if the transaction was originally sent from the wallet
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_is_outbound(
    tx: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if tx.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("tx".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    if (*tx).direction == TransactionDirection::Outbound {
        return true;
    }

    false
}

/// Frees memory for a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the TransactionID, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_transaction_id(
    transaction: *mut TariPendingOutboundTransaction,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).tx_id as c_ulonglong
}

/// Gets the destination TariPublicKey of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPublicKey` - Returns the destination TariPublicKey, note that it will be
/// ptr::null_mut() if transaction is null
///
/// # Safety
/// The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_destination_public_key(
    transaction: *mut TariPendingOutboundTransaction,
    error_out: *mut c_int,
) -> *mut TariPublicKey
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let m = (*transaction).destination_public_key.clone();
    Box::into_raw(Box::new(m))
}

/// Gets the amount of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the amount, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_amount(
    transaction: *mut TariPendingOutboundTransaction,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    c_ulonglong::from((*transaction).amount)
}

/// Gets the fee of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the fee, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_fee(
    transaction: *mut TariPendingOutboundTransaction,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    c_ulonglong::from((*transaction).fee)
}

/// Gets the timestamp of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the timestamp, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_timestamp(
    transaction: *mut TariPendingOutboundTransaction,
    error_out: *mut c_int,
) -> c_longlong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).timestamp.timestamp() as c_longlong
}

/// Gets the message of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*const c_char` - Returns the pointer to the char array, note that it will return a pointer
/// to an empty char array if transaction is null
///
/// # Safety
///  The ```string_destroy``` method must be called when finished with a string coming from rust to prevent a memory
/// leak
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_message(
    transaction: *mut TariPendingOutboundTransaction,
    error_out: *mut c_int,
) -> *const c_char
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let message = (*transaction).message.clone();
    let mut result = CString::new("").unwrap();
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result.into_raw();
    }

    result = CString::new(message).unwrap();
    result.into_raw()
}

/// Gets the status of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_int` - Returns the status which corresponds to:
/// | Value | Interpretation |
/// |---|---|
/// |  -1 | TxNullError |
/// |   0 | Completed   |
/// |   1 | Broadcast   |
/// |   2 | Mined       |
/// |   3 | Imported    |
/// |   4 | Pending     |
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_status(
    transaction: *mut TariPendingOutboundTransaction,
    error_out: *mut c_int,
) -> c_int
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return -1;
    }
    let status = (*transaction).status.clone();
    status as c_int
}

/// Frees memory for a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the TransactonId, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_transaction_id(
    transaction: *mut TariPendingInboundTransaction,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).tx_id as c_ulonglong
}

/// Gets the source TariPublicKey of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPublicKey` - Returns a pointer to the source TariPublicKey, note that it will be
/// ptr::null_mut() if transaction is null
///
/// # Safety
///  The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_source_public_key(
    transaction: *mut TariPendingInboundTransaction,
    error_out: *mut c_int,
) -> *mut TariPublicKey
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let m = (*transaction).source_public_key.clone();
    Box::into_raw(Box::new(m))
}

/// Gets the amount of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the amount, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_amount(
    transaction: *mut TariPendingInboundTransaction,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    c_ulonglong::from((*transaction).amount)
}

/// Gets the timestamp of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the timestamp, note that it will be zero if transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_timestamp(
    transaction: *mut TariPendingInboundTransaction,
    error_out: *mut c_int,
) -> c_longlong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).timestamp.timestamp() as c_longlong
}

/// Gets the message of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*const c_char` - Returns the pointer to the char array, note that it will return a pointer
/// to an empty char array if transaction is null
///
/// # Safety
///  The ```string_destroy``` method must be called when finished with a string coming from rust to prevent a memory
/// leak
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_message(
    transaction: *mut TariPendingInboundTransaction,
    error_out: *mut c_int,
) -> *const c_char
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let message = (*transaction).message.clone();
    let mut result = CString::new("").unwrap();
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result.into_raw();
    }

    result = CString::new(message).unwrap();
    result.into_raw()
}

/// Gets the status of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_int` - Returns the status which corresponds to:
/// | Value | Interpretation |
/// |---|---|
/// |  -1 | TxNullError |
/// |   0 | Completed   |
/// |   1 | Broadcast   |
/// |   2 | Mined       |
/// |   3 | Imported    |
/// |   4 | Pending     |
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_status(
    transaction: *mut TariPendingInboundTransaction,
    error_out: *mut c_int,
) -> c_int
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return -1;
    }
    let status = (*transaction).status.clone();
    status as c_int
}

/// Frees memory for a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_destroy(transaction: *mut TariPendingInboundTransaction) {
    if !transaction.is_null() {
        Box::from_raw(transaction);
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- Transport Types -----------------------------------------///

/// Creates a memory transport type
///
/// ## Arguments
/// `()` - Does not take any arguments
///
/// ## Returns
/// `*mut TariTransportType` - Returns a pointer to a memory TariTransportType
///
/// # Safety
/// The ```transport_type_destroy``` method must be called when finished with a TariTransportType to prevent a memory
/// leak
#[no_mangle]
pub unsafe extern "C" fn transport_memory_create() -> *mut TariTransportType {
    let transport = TariTransportType::Memory {
        listener_address: get_next_memory_address(),
    };
    Box::into_raw(Box::new(transport))
}

/// Creates a tcp transport type
///
/// ## Arguments
/// `listener_address` - The pointer to a char array
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariTransportType` - Returns a pointer to a tcp TariTransportType, null on error.
///
/// # Safety
/// The ```transport_type_destroy``` method must be called when finished with a TariTransportType to prevent a memory
/// leak
#[no_mangle]
pub unsafe extern "C" fn transport_tcp_create(
    listener_address: *const c_char,
    error_out: *mut c_int,
) -> *mut TariTransportType
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let listener_address_str;
    if !listener_address.is_null() {
        listener_address_str = CStr::from_ptr(listener_address).to_str().unwrap().to_owned();
    } else {
        error = LibWalletError::from(InterfaceError::NullError("listener_address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let transport = TariTransportType::Tcp {
        listener_address: listener_address_str.parse::<Multiaddr>().unwrap(),
        tor_socks_config: None,
    };
    Box::into_raw(Box::new(transport))
}

/// Creates a tor transport type
///
/// ## Arguments
/// `control_server_address` - The pointer to a char array
/// `tor_cookie` - The pointer to a ByteVector containing the contents of the tor cookie file, can be null
/// `tor_identity` - The pointer to a ByteVector containing the tor identity, can be null.
/// `tor_port` - The tor port
/// `socks_username` - The pointer to a char array containing the socks username, can be null
/// `socks_password` - The pointer to a char array containing the socks password, can be null
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariTransportType` - Returns a pointer to a tor TariTransportType, null on error.
///
/// # Safety
/// The ```transport_type_destroy``` method must be called when finished with a TariTransportType to prevent a memory
/// leak
#[no_mangle]
pub unsafe extern "C" fn transport_tor_create(
    control_server_address: *const c_char,
    tor_cookie: *const ByteVector,
    tor_port: c_ushort,
    socks_username: *const c_char,
    socks_password: *const c_char,
    error_out: *mut c_int,
) -> *mut TariTransportType
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let control_address_str;
    if !control_server_address.is_null() {
        control_address_str = CStr::from_ptr(control_server_address).to_str().unwrap().to_owned();
    } else {
        error = LibWalletError::from(InterfaceError::NullError("control_server_address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let username_str;
    let password_str;
    let authentication = if !socks_username.is_null() && !socks_password.is_null() {
        username_str = CStr::from_ptr(socks_username).to_str().unwrap().to_owned();
        password_str = CStr::from_ptr(socks_password).to_str().unwrap().to_owned();
        socks::Authentication::Password(username_str, password_str)
    } else {
        socks::Authentication::None
    };

    let tor_authentication = if !tor_cookie.is_null() {
        let cookie_hex = hex::to_hex((*tor_cookie).0.as_slice());
        tor::Authentication::Cookie(cookie_hex)
    } else {
        tor::Authentication::None
    };

    let identity = None;

    let tor_config = TorConfig {
        control_server_addr: control_address_str.parse::<Multiaddr>().unwrap(),
        control_server_auth: tor_authentication,
        identity,
        // Proxy the onion address to an OS-assigned local port
        port_mapping: tor::PortMapping::new(tor_port, "127.0.0.1:0".parse().unwrap()),
        socks_address_override: None,
        socks_auth: authentication,
    };
    let transport = TariTransportType::Tor(tor_config);

    Box::into_raw(Box::new(transport))
}

/// Gets the address for a memory transport type
///
/// ## Arguments
/// `transport` - Pointer to a TariTransportType
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns the address as a pointer to a char array, array will be empty on error
///
/// # Safety
/// Can only be used with a memory transport type, will crash otherwise
#[no_mangle]
pub unsafe extern "C" fn transport_memory_get_address(
    transport: *const TariTransportType,
    error_out: *mut c_int,
) -> *mut c_char
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut address = CString::new("").unwrap();
    if !transport.is_null() {
        match &*transport {
            TransportType::Memory { listener_address } => {
                address = CString::new(listener_address.to_string()).unwrap();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::NullError("transport".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
            },
        };
    } else {
        error = LibWalletError::from(InterfaceError::NullError("transport".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    address.into_raw()
}

/// Gets the private key for tor
///
/// ## Arguments
/// `wallet` - Pointer to a TariWallet
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut ByteVector` - Returns the serialized tor identity as a pointer to a ByteVector, contents for ByteVector will
/// be empty on error.
///
/// # Safety
/// Can only be used with a tor transport type, will crash otherwise
#[no_mangle]
pub unsafe extern "C" fn wallet_get_tor_identity(wallet: *const TariWallet, error_out: *mut c_int) -> *mut ByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let identity_bytes;
    if !wallet.is_null() {
        let service = (*wallet).wallet.comms.hidden_service();
        match service {
            Some(s) => {
                let tor_identity = s.tor_identity();
                identity_bytes = tor_identity.to_binary().unwrap();
            },
            None => {
                identity_bytes = Vec::new();
            },
        };
    } else {
        identity_bytes = Vec::new();
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let bytes = ByteVector(identity_bytes);
    Box::into_raw(Box::new(bytes))
}

/// Frees memory for a TariTransportType
///
/// ## Arguments
/// `transport` - The pointer to a TariTransportType
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn transport_type_destroy(transport: *mut TariTransportType) {
    if !transport.is_null() {
        Box::from_raw(transport);
    }
}

/// ---------------------------------------------------------------------------------------------///

/// ----------------------------------- CommsConfig ---------------------------------------------///

/// Creates a TariCommsConfig. The result from this function is required when initializing a TariWallet.
///
/// ## Arguments
/// `public_address` - The public address char array pointer. This is the address that the wallet advertises publicly to
/// peers
/// `transport_type` - TariTransportType that specifies the type of comms transport to be used.
/// connections are moved to after initial connection. Default if null is 0.0.0.0:7898 which will accept connections
/// from all IP address on port 7898
/// `database_name` - The database name char array pointer. This is the unique name of this
/// wallet's database
/// `database_path` - The database path char array pointer which. This is the folder path where the
/// database files will be created and the application has write access to
/// `discovery_timeout_in_secs`: specify how long the Discovery Timeout for the wallet is.
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariCommsConfig` - Returns a pointer to a TariCommsConfig, if any of the parameters are
/// null or a problem is encountered when constructing the NetAddress a ptr::null_mut() is returned
///
/// # Safety
/// The ```comms_config_destroy``` method must be called when finished with a TariCommsConfig to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn comms_config_create(
    public_address: *const c_char,
    transport_type: *const TariTransportType,
    database_name: *const c_char,
    datastore_path: *const c_char,
    discovery_timeout_in_secs: c_ulonglong,
    error_out: *mut c_int,
) -> *mut TariCommsConfig
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let public_address_str;
    if !public_address.is_null() {
        public_address_str = CStr::from_ptr(public_address).to_str().unwrap().to_owned();
    } else {
        error = LibWalletError::from(InterfaceError::NullError("public_address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let database_name_string;
    if !database_name.is_null() {
        database_name_string = CStr::from_ptr(database_name).to_str().unwrap().to_owned();
    } else {
        error = LibWalletError::from(InterfaceError::NullError("database_name".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let datastore_path_string;
    if !datastore_path.is_null() {
        datastore_path_string = CStr::from_ptr(datastore_path).to_str().unwrap().to_owned();
    } else {
        error = LibWalletError::from(InterfaceError::NullError("datastore_path".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let datastore_path = PathBuf::from(datastore_path_string);

    let dht_database_path = datastore_path.join("dht.db");

    // Check to see if we have a comms private key stored in the Sqlite database. If not generate a new one.
    let sql_database_path = datastore_path
        .join(database_name_string.clone())
        .with_extension("sqlite3");
    let connection = run_migration_and_create_sqlite_connection(&sql_database_path)
        .map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Error creating Sqlite Connection in Wallet: {:?}", e
            );
            e
        })
        .expect("Could not open Sqlite db");

    // Try create a Wallet Sqlite backend without a Cipher, if it fails then the DB is encrypted and we will have to
    // extract the Comms Secret Key in wallet_create(...) with the supplied passphrase
    let comms_secret_key = match WalletSqliteDatabase::new(connection.clone(), None) {
        Ok(wallet_sqlite_db) => {
            let wallet_backend = WalletDatabase::new(wallet_sqlite_db);

            match Runtime::new() {
                Ok(mut rt) => {
                    let secret_key = match rt.block_on(wallet_backend.get_comms_secret_key()) {
                        Ok(sk) => sk,
                        Err(e) => {
                            error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
                            ptr::swap(error_out, &mut error as *mut c_int);
                            return ptr::null_mut();
                        },
                    };
                    match secret_key {
                        None => CommsSecretKey::random(&mut OsRng),
                        Some(sk) => sk,
                    }
                },
                Err(e) => {
                    error = LibWalletError::from(InterfaceError::TokioError(e.to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    return ptr::null_mut();
                },
            }
        },
        Err(_) => CommsSecretKey::default(),
    };

    let transport_type = (*transport_type).clone();
    let transport_type = match transport_type {
        Tor(mut tor_config) => {
            match WalletSqliteDatabase::new(connection, None) {
                Ok(database) => {
                    let db = WalletDatabase::new(database);

                    match Runtime::new() {
                        Ok(mut rt) => {
                            tor_config.identity = match tor_config.identity {
                                Some(v) => {
                                    // This is temp code and should be removed after testnet
                                    let _ = rt.block_on(db.set_tor_identity((*v).clone()));
                                    Some(v)
                                },
                                _ => match rt.block_on(db.get_tor_id()) {
                                    Ok(Some(v)) => Some(Box::new(v)),
                                    _ => None,
                                },
                            };
                            Tor(tor_config)
                        },
                        Err(e) => {
                            error = LibWalletError::from(InterfaceError::TokioError(e.to_string())).code;
                            ptr::swap(error_out, &mut error as *mut c_int);
                            return ptr::null_mut();
                        },
                    }
                },
                _ => Tor(tor_config),
            }
        },
        _ => transport_type,
    };

    let public_address = public_address_str.parse::<Multiaddr>();

    match public_address {
        Ok(public_address) => {
            let ni = NodeIdentity::new(comms_secret_key, public_address, PeerFeatures::COMMUNICATION_CLIENT);
            match ni {
                Ok(ni) => {
                    let config = TariCommsConfig {
                        node_identity: Arc::new(ni),
                        transport_type,
                        datastore_path,
                        peer_database_name: database_name_string,
                        max_concurrent_inbound_tasks: 100,
                        outbound_buffer_size: 100,
                        dht: DhtConfig {
                            discovery_request_timeout: Duration::from_secs(discovery_timeout_in_secs),
                            database_url: DbConnectionUrl::File(dht_database_path),
                            auto_join: true,
                            ..Default::default()
                        },
                        // TODO: This should be set to false for non-test wallets. See the `allow_test_addresses` field
                        //       docstring for more info.
                        allow_test_addresses: true,
                        listener_liveness_allowlist_cidrs: Vec::new(),
                        listener_liveness_max_sessions: 0,
                        user_agent: format!("tari/wallet/{}", env!("CARGO_PKG_VERSION")),
                        dns_seeds_name_server: "1.1.1.1:53".parse().unwrap(),
                        peer_seeds: Default::default(),
                        dns_seeds: Default::default(),
                        dns_seeds_use_dnssec: true,
                    };

                    Box::into_raw(Box::new(config))
                },
                Err(e) => {
                    error = LibWalletError::from(e).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    ptr::null_mut()
                },
            }
        },
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Set the Comms Secret Key for an existing TariCommsConfig. Usually this key is maintained by the backend but if it is
/// required to set a specific new one this function can be used.
///
/// ## Arguments
/// `comms_config` - TariCommsConfig to be updated
/// `secret_key` - The TariSecretKey pointer. This is the secret key corresponding to the Public key that represents
/// this node on the Tari comms network
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// None
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn comms_config_set_secret_key(
    comms_config: *mut TariCommsConfig,
    secret_key: *const TariPrivateKey,
    error_out: *mut c_int,
)
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if comms_config.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("comms_config".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    if secret_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("secret_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    match NodeIdentity::new(
        (*secret_key).clone(),
        (*comms_config).node_identity.public_address(),
        PeerFeatures::COMMUNICATION_CLIENT,
    ) {
        Ok(ni) => {
            (*comms_config).node_identity = Arc::new(ni);
        },
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }
}

/// Frees memory for a TariCommsConfig
///
/// ## Arguments
/// `wc` - The TariCommsConfig pointer
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
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
/// `num_rolling_log_files` - Specifies how many rolling log files to produce, if no rolling files are wanted then set
/// this to 0
/// `size_per_log_file_bytes` - Specifies the size, in bytes, at which the logs files will roll over, if no
/// rolling files are wanted then set this to 0
/// `passphrase` - An optional string that represents the passphrase used to
/// encrypt/decrypt the databases for this wallet. If it is left Null no encryption is used. If the databases have been
/// encrypted then the correct passphrase is required or this function will fail.
/// `callback_received_transaction` - The callback function pointer matching the
/// function signature. This will be called when an inbound transaction is received.
/// `callback_received_transaction_reply` - The callback function pointer matching the function signature. This will be
/// called when a reply is received for a pending outbound transaction
/// `callback_received_finalized_transaction` - The callback function pointer matching the function signature. This will
/// be called when a Finalized version on an Inbound transaction is received
/// `callback_transaction_broadcast` - The callback function pointer matching the function signature. This will be
/// called when a Finalized transaction is detected a Broadcast to a base node mempool.
/// `callback_transaction_mined` - The callback function pointer matching the function signature. This will be called
/// when a Broadcast transaction is detected as mined.
/// `callback_discovery_process_complete` - The callback function pointer matching the function signature. This will be
/// called when a `send_transacion(..)` call is made to a peer whose address is not known and a discovery process must
/// be conducted. The outcome of the discovery process is relayed via this callback
/// `callback_base_node_sync_complete` - The callback function pointer matching the function signature. This is called
/// when a Base Node Sync process is completed or times out. The request_key is used to identify which request this
/// callback references and a result of true means it was successful and false that the process timed out and new one
/// will be started
/// `callback_saf_message_received` - The callback function pointer that will be called when the Dht has determined that
/// is has connected to enough of its neighbours to be confident that it has received any SAF messages that were waiting
/// for it.
/// `error_out` - Pointer to an int which will be modified
/// to an error code should one occur, may not be null. Functions as an out parameter.
/// ## Returns
/// `*mut TariWallet` - Returns a pointer to a TariWallet, note that it returns ptr::null_mut()
/// if config is null, a wallet error was encountered or if the runtime could not be created
///
/// # Safety
/// The ```wallet_destroy``` method must be called when finished with a TariWallet to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_create(
    config: *mut TariCommsConfig,
    log_path: *const c_char,
    num_rolling_log_files: c_uint,
    size_per_log_file_bytes: c_uint,
    passphrase: *const c_char,
    callback_received_transaction: unsafe extern "C" fn(*mut TariPendingInboundTransaction),
    callback_received_transaction_reply: unsafe extern "C" fn(*mut TariCompletedTransaction),
    callback_received_finalized_transaction: unsafe extern "C" fn(*mut TariCompletedTransaction),
    callback_transaction_broadcast: unsafe extern "C" fn(*mut TariCompletedTransaction),
    callback_transaction_mined: unsafe extern "C" fn(*mut TariCompletedTransaction),
    callback_direct_send_result: unsafe extern "C" fn(c_ulonglong, bool),
    callback_store_and_forward_send_result: unsafe extern "C" fn(c_ulonglong, bool),
    callback_transaction_cancellation: unsafe extern "C" fn(*mut TariCompletedTransaction),
    callback_base_node_sync_complete: unsafe extern "C" fn(u64, bool),
    callback_saf_messages_received: unsafe extern "C" fn(),
    error_out: *mut c_int,
) -> *mut TariWallet
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if config.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("config".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    if !log_path.is_null() {
        let path = CStr::from_ptr(log_path).to_str().unwrap().to_owned();
        let encoder = PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S.%f)} [{t}] {l:5} {m}{n}");
        let log_appender: Box<dyn Append> = if num_rolling_log_files != 0 && size_per_log_file_bytes != 0 {
            let mut pattern;
            let split_str: Vec<&str> = path.split('.').collect();
            if split_str.len() <= 1 {
                pattern = format!("{}{}", path.clone(), "{}");
            } else {
                pattern = split_str[0].to_string();
                for part in split_str.iter().take(split_str.len() - 1).skip(1) {
                    pattern = format!("{}.{}", pattern, part);
                }

                pattern = format!("{}{}", pattern, ".{}.");
                pattern = format!("{}{}", pattern, split_str[split_str.len() - 1]);
            }
            let roller = FixedWindowRoller::builder()
                .build(pattern.as_str(), num_rolling_log_files)
                .unwrap();
            let size_trigger = SizeTrigger::new(size_per_log_file_bytes as u64);
            let policy = CompoundPolicy::new(Box::new(size_trigger), Box::new(roller));

            Box::new(
                RollingFileAppender::builder()
                    .encoder(Box::new(encoder))
                    .append(true)
                    .build(path.as_str(), Box::new(policy))
                    .unwrap(),
            )
        } else {
            Box::new(
                FileAppender::builder()
                    .encoder(Box::new(encoder))
                    .append(true)
                    .build(path.as_str())
                    .expect("Should be able to create Appender"),
            )
        };

        let lconfig = Config::builder()
            .appender(Appender::builder().build("logfile", log_appender))
            .build(Root::builder().appender("logfile").build(LevelFilter::Debug))
            .unwrap();

        match log4rs::init_config(lconfig) {
            Ok(_) => debug!(target: LOG_TARGET, "Logging started"),
            Err(_) => warn!(target: LOG_TARGET, "Logging has already been initialized"),
        }
    }

    let passphrase_option = if !passphrase.is_null() {
        let pf = CStr::from_ptr(passphrase)
            .to_str()
            .expect("A non-null passphrase should be able to be converted to string")
            .to_owned();
        Some(pf)
    } else {
        None
    };

    let runtime = Runtime::new();
    let factories = CryptoFactories::default();
    let w;

    match runtime {
        Ok(mut runtime) => {
            let sql_database_path = (*config)
                .datastore_path
                .join((*config).peer_database_name.clone())
                .with_extension("sqlite3");

            debug!(target: LOG_TARGET, "Running Wallet database migrations");
            let (wallet_backend, transaction_backend, output_manager_backend, contacts_backend) =
                match initialize_sqlite_database_backends(sql_database_path, passphrase_option) {
                    Ok((w, t, o, c)) => (w, t, o, c),
                    Err(e) => {
                        error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        return ptr::null_mut();
                    },
                };
            debug!(target: LOG_TARGET, "Databases Initialized");

            // Check to see if the comms private key needs to be read from the encrypted DB
            if (*config).node_identity.secret_key() == &CommsSecretKey::default() {
                let wallet_db = WalletDatabase::new(wallet_backend.clone());
                let secret_key = match runtime.block_on(wallet_db.get_comms_secret_key()) {
                    Ok(sk_option) => match sk_option {
                        None => {
                            error = LibWalletError::from(InterfaceError::MissingCommsPrivateKey).code;
                            ptr::swap(error_out, &mut error as *mut c_int);
                            return ptr::null_mut();
                        },
                        Some(sk) => sk,
                    },
                    Err(e) => {
                        error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        return ptr::null_mut();
                    },
                };
                let ni = match NodeIdentity::new(
                    secret_key,
                    (*config).node_identity.public_address(),
                    PeerFeatures::COMMUNICATION_CLIENT,
                ) {
                    Ok(n) => n,
                    Err(e) => {
                        error = LibWalletError::from(e).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        return ptr::null_mut();
                    },
                };
                (*config).node_identity = Arc::new(ni);
            }

            // TODO remove after next TestNet
            transaction_backend.migrate((*config).node_identity.public_key().clone());

            let shutdown = Shutdown::new();

            w = runtime.block_on(Wallet::new(
                WalletConfig::new(
                    (*config).clone(),
                    factories,
                    Some(TransactionServiceConfig {
                        direct_send_timeout: (*config).dht.discovery_request_timeout,
                        ..Default::default()
                    }),
                    None,
                    Network::Stibbons,
                    None,
                    None,
                    None,
                ),
                wallet_backend,
                transaction_backend.clone(),
                output_manager_backend,
                contacts_backend,
                shutdown.to_signal(),
            ));

            match w {
                Ok(mut w) => {
                    // lets ensure the wallet tor_id is saved
                    if let Some(hs) = w.comms.hidden_service() {
                        if let Err(e) = runtime.block_on(w.db.set_tor_identity(hs.tor_identity().clone())) {
                            warn!(target: LOG_TARGET, "Could not save tor identity to db: {}", e);
                        }
                    }
                    // Start Callback Handler
                    let callback_handler = CallbackHandler::new(
                        TransactionDatabase::new(transaction_backend),
                        w.transaction_service.get_event_stream_fused(),
                        w.output_manager_service.get_event_stream_fused(),
                        w.dht_service.subscribe_dht_events().fuse(),
                        w.comms.shutdown_signal(),
                        w.comms.node_identity().public_key().clone(),
                        callback_received_transaction,
                        callback_received_transaction_reply,
                        callback_received_finalized_transaction,
                        callback_transaction_broadcast,
                        callback_transaction_mined,
                        callback_direct_send_result,
                        callback_store_and_forward_send_result,
                        callback_transaction_cancellation,
                        callback_base_node_sync_complete,
                        callback_saf_messages_received,
                    );

                    runtime.spawn(callback_handler.start());

                    if let Err(e) = runtime.block_on(w.transaction_service.restart_transaction_protocols()) {
                        warn!(
                            target: LOG_TARGET,
                            "Could not restart transaction negotiation protocols: {}", e
                        );
                    }

                    let tari_wallet = TariWallet {
                        wallet: w,
                        runtime,
                        shutdown,
                    };

                    Box::into_raw(Box::new(tari_wallet))
                },
                Err(e) => {
                    error = LibWalletError::from(e).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    ptr::null_mut()
                },
            }
        },
        Err(e) => {
            error = LibWalletError::from(InterfaceError::TokioError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Signs a message using the public key of the TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `msg` - The message pointer.
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
/// ## Returns
/// `*mut c_char` - Returns the pointer to the hexadecimal representation of the signature and
/// public nonce, seperated by a pipe character. Empty if an error occured.
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string coming from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_sign_message(
    wallet: *mut TariWallet,
    msg: *const c_char,
    error_out: *mut c_int,
) -> *mut c_char
{
    let mut error = 0;
    let mut result = CString::new("").unwrap();

    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result.into_raw();
    }

    if msg.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result.into_raw();
    }

    let nonce = TariPrivateKey::random(&mut OsRng);
    let secret = (*wallet).wallet.comms.node_identity().secret_key().clone();
    let message = CStr::from_ptr(msg).to_str().unwrap().to_owned();
    let signature = (*wallet).wallet.sign_message(secret, nonce, &message);

    match signature {
        Ok(s) => {
            let hex_sig = s.get_signature().to_hex();
            let hex_nonce = s.get_public_nonce().to_hex();
            let hex_return = format!("{}|{}", hex_sig, hex_nonce);
            result = CString::new(hex_return).unwrap();
        },
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    result.into_raw()
}

/// Verifies the signature of the message signed by a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `public_key` - The pointer to the TariPublicKey of the wallet which originally signed the message
/// `hex_sig_nonce` - The pointer to the sting containing the hexadecimal representation of the
/// signature and public nonce seperated by a pipe character.
/// `msg` - The pointer to the msg the signature will be checked against.
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
/// ## Returns
/// `bool` - Returns if the signature is valid or not, will be false if an error occurs.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_verify_message_signature(
    wallet: *mut TariWallet,
    public_key: *mut TariPublicKey,
    hex_sig_nonce: *const c_char,
    msg: *const c_char,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    let mut result = false;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result;
    }
    if public_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("public key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result;
    }
    if hex_sig_nonce.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("signature".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result;
    }
    if msg.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result;
    }

    let message = CStr::from_ptr(msg).to_str().unwrap().to_owned();
    let hex = CStr::from_ptr(hex_sig_nonce).to_str().unwrap().to_owned();
    let hex_keys: Vec<&str> = hex.split('|').collect();
    if hex_keys.len() != 2 {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result;
    }
    let secret = TariPrivateKey::from_hex(hex_keys.get(0).unwrap());
    match secret {
        Ok(p) => {
            let public_nonce = TariPublicKey::from_hex(hex_keys.get(1).unwrap());
            match public_nonce {
                Ok(pn) => {
                    result = (*wallet)
                        .wallet
                        .verify_message_signature((*public_key).clone(), pn, p, message)
                },
                Err(e) => {
                    error = LibWalletError::from(e).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                },
            }
        },
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    result
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if successful or not
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_test_generate_data(
    wallet: *mut TariWallet,
    datastore_path: *const c_char,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    let datastore_path_string;
    if !datastore_path.is_null() {
        datastore_path_string = CStr::from_ptr(datastore_path).to_str().unwrap().to_owned();
    } else {
        error = LibWalletError::from(InterfaceError::NullError("datastore_path".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    match (*wallet).runtime.block_on(generate_wallet_test_data(
        &mut (*wallet).wallet,
        datastore_path_string.as_str(),
        (*wallet).wallet.transaction_backend.clone(),
    )) {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// This function simulates an external `TariWallet` sending a transaction to this `TariWallet`
/// which will become a `TariPendingInboundTransaction`
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if successful or not
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_test_receive_transaction(wallet: *mut TariWallet, error_out: *mut c_int) -> bool {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    match (*wallet)
        .runtime
        .block_on(receive_test_transaction(&mut (*wallet).wallet))
    {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// This function simulates a receiver accepting and replying to a `TariPendingOutboundTransaction`.
/// This results in that transaction being "completed" and it's status set to `Broadcast` which
/// indicated it is in a base_layer mempool.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `tx` - The TariPendingOutboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if successful or not
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_test_complete_sent_transaction(
    wallet: *mut TariWallet,
    tx: *mut TariPendingOutboundTransaction,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    if tx.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("tx".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    match (*wallet)
        .runtime
        .block_on(complete_sent_transaction(&mut (*wallet).wallet, (*tx).tx_id))
    {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// This function will simulate the process when a completed transaction is broadcast to
/// the base layer mempool. The function will update the status of the completed transaction
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `tx` - The pending inbound transaction to operate on
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if successful or not
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_test_finalize_received_transaction(
    wallet: *mut TariWallet,
    tx: *mut TariPendingInboundTransaction,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    match (*wallet)
        .runtime
        .block_on(finalize_received_transaction(&mut (*wallet).wallet, (*tx).tx_id))
    {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// This function will simulate the process when a completed transaction is broadcast to
/// the base layer mempool. The function will update the status of the completed transaction
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `tx_id` - The transaction id to operate on
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if successful or not
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_test_broadcast_transaction(
    wallet: *mut TariWallet,
    tx_id: c_ulonglong,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    match (*wallet)
        .runtime
        .block_on(broadcast_transaction(&mut (*wallet).wallet, tx_id))
    {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// This function will simulate the process when a completed transaction is detected as mined on
/// the base layer. The function will update the status of the completed transaction AND complete
/// the transaction on the Output Manager Service which will update the status of the outputs
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `tx_id` - The transaction id to operate on
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if successful or not
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_test_mine_transaction(
    wallet: *mut TariWallet,
    tx_id: c_ulonglong,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    match (*wallet)
        .runtime
        .block_on(mine_transaction(&mut (*wallet).wallet, tx_id))
    {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// Adds a base node peer to the TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `public_key` - The TariPublicKey pointer
/// `address` - The pointer to a char array
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if successful or not
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_add_base_node_peer(
    wallet: *mut TariWallet,
    public_key: *mut TariPublicKey,
    address: *const c_char,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    if public_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("public_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    let address_string;
    if !address.is_null() {
        address_string = CStr::from_ptr(address).to_str().unwrap().to_owned();
    } else {
        error = LibWalletError::from(InterfaceError::NullError("address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    if let Err(e) = (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .set_base_node_peer((*public_key).clone(), address_string),
    ) {
        error = LibWalletError::from(e).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    // Restart Transaction Service protocols that need the base node peer
    if let Err(e) = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.restart_broadcast_protocols())
    {
        error = LibWalletError::from(WalletError::from(e)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    // Start once off Output manager validation protocols that need the base node peer to be set
    if let Err(e) = (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .output_manager_service
            .validate_txos(TxoValidationType::Invalid, TxoValidationRetry::Limited(5)),
    ) {
        error = LibWalletError::from(WalletError::from(e)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    if let Err(e) = (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .output_manager_service
            .validate_txos(TxoValidationType::Spent, TxoValidationRetry::Limited(5)),
    ) {
        error = LibWalletError::from(WalletError::from(e)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    true
}

/// Upserts a TariContact to the TariWallet. If the contact does not exist it will be Inserted. If it does exist the
/// Alias will be updated.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `contact` - The TariContact pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if successful or not
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_upsert_contact(
    wallet: *mut TariWallet,
    contact: *mut TariContact,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    if contact.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("contact".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.contacts_service.upsert_contact((*contact).clone()))
    {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(WalletError::ContactsServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// Removes a TariContact from the TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `tx` - The TariPendingInboundTransaction pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns if successful or not
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_remove_contact(
    wallet: *mut TariWallet,
    contact: *mut TariContact,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    if contact.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("contact".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    match (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .contacts_service
            .remove_contact((*contact).public_key.clone()),
    ) {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(WalletError::ContactsServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// Gets the available balance from a TariWallet. This is the balance the user can spend.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The available balance, 0 if wallet is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_get_available_balance(wallet: *mut TariWallet, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.output_manager_service.get_balance())
    {
        Ok(b) => c_ulonglong::from(b.available_balance),
        Err(e) => {
            error = LibWalletError::from(WalletError::OutputManagerError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Gets the incoming balance from a `TariWallet`. This is the uncleared balance of Tari that is
/// expected to come into the `TariWallet` but is not yet spendable.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The incoming balance, 0 if wallet is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_incoming_balance(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.output_manager_service.get_balance())
    {
        Ok(b) => c_ulonglong::from(b.pending_incoming_balance),
        Err(e) => {
            error = LibWalletError::from(WalletError::OutputManagerError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Gets the outgoing balance from a `TariWallet`. This is the uncleared balance of Tari that has
/// been spent
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The outgoing balance, 0 if wallet is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_outgoing_balance(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.output_manager_service.get_balance())
    {
        Ok(b) => c_ulonglong::from(b.pending_outgoing_balance),
        Err(e) => {
            error = LibWalletError::from(WalletError::OutputManagerError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
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
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `unsigned long long` - Returns 0 if unsuccessful or the TxId of the sent transaction if successful
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_send_transaction(
    wallet: *mut TariWallet,
    dest_public_key: *mut TariPublicKey,
    amount: c_ulonglong,
    fee_per_gram: c_ulonglong,
    message: *const c_char,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    if dest_public_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("dest_public_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    let message_string = if !message.is_null() {
        CStr::from_ptr(message).to_str().unwrap().to_owned()
    } else {
        error = LibWalletError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        CString::new("").unwrap().to_str().unwrap().to_owned()
    };

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.send_transaction(
            (*dest_public_key).clone(),
            MicroTari::from(amount),
            MicroTari::from(fee_per_gram),
            message_string,
        )) {
        Ok(tx_id) => tx_id,
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Gets a fee estimate for an amount
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `amount` - The amount
/// `fee_per_gram` - The fee per gram
/// `num_kernels` - The number of transaction kernels
/// `num_outputs` - The number of outputs
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `unsigned long long` - Returns 0 if unsuccessful or the fee estimate in MicroTari if successful
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_get_fee_estimate(
    wallet: *mut TariWallet,
    amount: c_ulonglong,
    fee_per_gram: c_ulonglong,
    num_kernels: c_ulonglong,
    num_outputs: c_ulonglong,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.output_manager_service.fee_estimate(
            MicroTari::from(amount),
            MicroTari::from(fee_per_gram),
            num_kernels,
            num_outputs,
        )) {
        Ok(fee) => fee.into(),
        Err(e) => {
            error = LibWalletError::from(WalletError::OutputManagerError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Get the TariContacts from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariContacts` - returns the contacts, note that it returns ptr::null_mut() if
/// wallet is null
///
/// # Safety
/// The ```contacts_destroy``` method must be called when finished with a TariContacts to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_contacts(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariContacts {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut contacts = Vec::new();
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let retrieved_contacts = (*wallet)
        .runtime
        .block_on((*wallet).wallet.contacts_service.get_contacts());
    match retrieved_contacts {
        Ok(mut retrieved_contacts) => {
            contacts.append(&mut retrieved_contacts);
            Box::into_raw(Box::new(TariContacts(contacts)))
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::ContactsServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Get the TariCompletedTransactions from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariCompletedTransactions` - returns the transactions, note that it returns ptr::null_mut() if
/// wallet is null or an error is encountered
///
/// # Safety
/// The ```completed_transactions_destroy``` method must be called when finished with a TariCompletedTransactions to
/// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_completed_transactions(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> *mut TariCompletedTransactions
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut completed = Vec::new();
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let completed_transactions = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.get_completed_transactions());
    match completed_transactions {
        Ok(completed_transactions) => {
            // The frontend specification calls for completed transactions that have not yet been mined to be
            // classified as Pending Transactions. In order to support this logic without impacting the practical
            // definitions and storage of a MimbleWimble CompletedTransaction we will remove CompletedTransactions with
            // the Completed and Broadcast states from the list returned by this FFI function
            for tx in completed_transactions
                .values()
                .filter(|ct| ct.status != TransactionStatus::Completed)
                .filter(|ct| ct.status != TransactionStatus::Broadcast)
            {
                completed.push(tx.clone());
            }
            Box::into_raw(Box::new(TariCompletedTransactions(completed)))
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Get the TariPendingInboundTransactions from a TariWallet
///
/// Currently a CompletedTransaction with the Status of Completed and Broadcast is considered Pending by the frontend
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPendingInboundTransactions` - returns the transactions, note that it returns ptr::null_mut() if
/// wallet is null or and error is encountered
///
/// # Safety
/// The ```pending_inbound_transactions_destroy``` method must be called when finished with a
/// TariPendingInboundTransactions to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_inbound_transactions(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> *mut TariPendingInboundTransactions
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut pending = Vec::new();
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let pending_transactions = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.get_pending_inbound_transactions());

    match pending_transactions {
        Ok(pending_transactions) => {
            for tx in pending_transactions.values() {
                pending.push(tx.clone());
            }

            if let Ok(completed_txs) = (*wallet)
                .runtime
                .block_on((*wallet).wallet.transaction_service.get_completed_transactions())
            {
                // The frontend specification calls for completed transactions that have not yet been mined to be
                // classified as Pending Transactions. In order to support this logic without impacting the practical
                // definitions and storage of a MimbleWimble CompletedTransaction we will add those transaction to the
                // list here in the FFI interface
                for ct in completed_txs
                    .values()
                    .filter(|ct| ct.status == TransactionStatus::Completed || ct.status == TransactionStatus::Broadcast)
                    .filter(|ct| ct.direction == TransactionDirection::Inbound)
                {
                    pending.push(InboundTransaction::from(ct.clone()));
                }
            }

            Box::into_raw(Box::new(TariPendingInboundTransactions(pending)))
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Get the TariPendingOutboundTransactions from a TariWallet
///
/// Currently a CompletedTransaction with the Status of Completed and Broadcast is considered Pending by the frontend
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPendingOutboundTransactions` - returns the transactions, note that it returns ptr::null_mut() if
/// wallet is null or and error is encountered
///
/// # Safety
/// The ```pending_outbound_transactions_destroy``` method must be called when finished with a
/// TariPendingOutboundTransactions to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_outbound_transactions(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> *mut TariPendingOutboundTransactions
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut pending = Vec::new();
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let pending_transactions = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.get_pending_outbound_transactions());
    match pending_transactions {
        Ok(pending_transactions) => {
            for tx in pending_transactions.values() {
                pending.push(tx.clone());
            }
            if let Ok(completed_txs) = (*wallet)
                .runtime
                .block_on((*wallet).wallet.transaction_service.get_completed_transactions())
            {
                // The frontend specification calls for completed transactions that have not yet been mined to be
                // classified as Pending Transactions. In order to support this logic without impacting the practical
                // definitions and storage of a MimbleWimble CompletedTransaction we will add those transaction to the
                // list here in the FFI interface
                for ct in completed_txs
                    .values()
                    .filter(|ct| ct.status == TransactionStatus::Completed || ct.status == TransactionStatus::Broadcast)
                    .filter(|ct| ct.direction == TransactionDirection::Outbound)
                {
                    pending.push(OutboundTransaction::from(ct.clone()));
                }
            }
            Box::into_raw(Box::new(TariPendingOutboundTransactions(pending)))
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Get the all Cancelled Transactions from a TariWallet. This function will also get cancelled pending inbound and
/// outbound transaction and include them in this list by converting them to CompletedTransactions
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariCompletedTransactions` - returns the transactions, note that it returns ptr::null_mut() if
/// wallet is null or an error is encountered
///
/// # Safety
/// The ```completed_transactions_destroy``` method must be called when finished with a TariCompletedTransactions to
/// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_cancelled_transactions(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> *mut TariCompletedTransactions
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let completed_transactions = match (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .transaction_service
            .get_cancelled_completed_transactions(),
    ) {
        Ok(txs) => txs,
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };
    let inbound_transactions = match (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .transaction_service
            .get_cancelled_pending_inbound_transactions(),
    ) {
        Ok(txs) => txs,
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };
    let outbound_transactions = match (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .transaction_service
            .get_cancelled_pending_outbound_transactions(),
    ) {
        Ok(txs) => txs,
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let mut completed = Vec::new();
    for tx in completed_transactions.values() {
        completed.push(tx.clone());
    }
    for tx in inbound_transactions.values() {
        let mut inbound_tx = CompletedTransaction::from(tx.clone());
        inbound_tx.destination_public_key = (*wallet).wallet.comms.node_identity().public_key().clone();
        completed.push(inbound_tx);
    }
    for tx in outbound_transactions.values() {
        let mut outbound_tx = CompletedTransaction::from(tx.clone());
        outbound_tx.source_public_key = (*wallet).wallet.comms.node_identity().public_key().clone();
        completed.push(outbound_tx);
    }

    Box::into_raw(Box::new(TariCompletedTransactions(completed)))
}

/// Get the TariCompletedTransaction from a TariWallet by its' TransactionId
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `transaction_id` - The TransactionId
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariCompletedTransaction` - returns the transaction, note that it returns ptr::null_mut() if
/// wallet is null, an error is encountered or if the transaction is not found
///
/// # Safety
/// The ```completed_transaction_destroy``` method must be called when finished with a TariCompletedTransaction to
/// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_completed_transaction_by_id(
    wallet: *mut TariWallet,
    transaction_id: c_ulonglong,
    error_out: *mut c_int,
) -> *mut TariCompletedTransaction
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let completed_transactions = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.get_completed_transactions());

    match completed_transactions {
        Ok(completed_transactions) => {
            if let Some(tx) = completed_transactions.get(&transaction_id) {
                if tx.status != TransactionStatus::Completed && tx.status != TransactionStatus::Broadcast {
                    let completed = tx.clone();
                    return Box::into_raw(Box::new(completed));
                }
            }
            error = 108;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    ptr::null_mut()
}

/// Get the TariPendingInboundTransaction from a TariWallet by its' TransactionId
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `transaction_id` - The TransactionId
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPendingInboundTransaction` - returns the transaction, note that it returns ptr::null_mut() if
/// wallet is null, an error is encountered or if the transaction is not found
///
/// # Safety
/// The ```pending_inbound_transaction_destroy``` method must be called when finished with a
/// TariPendingInboundTransaction to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_inbound_transaction_by_id(
    wallet: *mut TariWallet,
    transaction_id: c_ulonglong,
    error_out: *mut c_int,
) -> *mut TariPendingInboundTransaction
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let pending_transactions = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.get_pending_inbound_transactions());

    let completed_transactions = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.get_completed_transactions());

    match completed_transactions {
        Ok(completed_transactions) => {
            if let Some(tx) = completed_transactions.get(&transaction_id) {
                if (tx.status == TransactionStatus::Broadcast || tx.status == TransactionStatus::Completed) &&
                    tx.direction == TransactionDirection::Inbound
                {
                    let completed = tx.clone();
                    let pending_tx = TariPendingInboundTransaction::from(completed);
                    return Box::into_raw(Box::new(pending_tx));
                }
            }
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    match pending_transactions {
        Ok(pending_transactions) => {
            if let Some(tx) = pending_transactions.get(&transaction_id) {
                let pending = tx.clone();
                return Box::into_raw(Box::new(pending));
            }
            error = 108;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    ptr::null_mut()
}

/// Get the TariPendingOutboundTransaction from a TariWallet by its' TransactionId
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `transaction_id` - The TransactionId
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPendingOutboundTransaction` - returns the transaction, note that it returns ptr::null_mut() if
/// wallet is null, an error is encountered or if the transaction is not found
///
/// # Safety
/// The ```pending_outbound_transaction_destroy``` method must be called when finished with a
/// TariPendingOutboundtransaction to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_pending_outbound_transaction_by_id(
    wallet: *mut TariWallet,
    transaction_id: c_ulonglong,
    error_out: *mut c_int,
) -> *mut TariPendingOutboundTransaction
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let pending_transactions = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.get_pending_outbound_transactions());

    let completed_transactions = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.get_completed_transactions());

    match completed_transactions {
        Ok(completed_transactions) => {
            if let Some(tx) = completed_transactions.get(&transaction_id) {
                if (tx.status == TransactionStatus::Broadcast || tx.status == TransactionStatus::Completed) &&
                    tx.direction == TransactionDirection::Outbound
                {
                    let completed = tx.clone();
                    let pending_tx = TariPendingOutboundTransaction::from(completed);
                    return Box::into_raw(Box::new(pending_tx));
                }
            }
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    match pending_transactions {
        Ok(pending_transactions) => {
            if let Some(tx) = pending_transactions.get(&transaction_id) {
                let pending = tx.clone();
                return Box::into_raw(Box::new(pending));
            }
            error = 108;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    ptr::null_mut()
}

/// Get a Cancelled transaction from a TariWallet by its TransactionId. Pending Inbound or Outbound transaction will be
/// converted to a CompletedTransaction
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `transaction_id` - The TransactionId
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariCompletedTransaction` - returns the transaction, note that it returns ptr::null_mut() if
/// wallet is null, an error is encountered or if the transaction is not found
///
/// # Safety
/// The ```completed_transaction_destroy``` method must be called when finished with a TariCompletedTransaction to
/// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_cancelled_transaction_by_id(
    wallet: *mut TariWallet,
    transaction_id: c_ulonglong,
    error_out: *mut c_int,
) -> *mut TariCompletedTransaction
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let mut transaction = None;

    let mut completed_transactions = match (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .transaction_service
            .get_cancelled_completed_transactions(),
    ) {
        Ok(txs) => txs,
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    if let Some(tx) = completed_transactions.remove(&transaction_id) {
        transaction = Some(tx);
    } else {
        let mut outbound_transactions = match (*wallet).runtime.block_on(
            (*wallet)
                .wallet
                .transaction_service
                .get_cancelled_pending_outbound_transactions(),
        ) {
            Ok(txs) => txs,
            Err(e) => {
                error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        };

        if let Some(tx) = outbound_transactions.remove(&transaction_id) {
            let mut outbound_tx = CompletedTransaction::from(tx);
            outbound_tx.source_public_key = (*wallet).wallet.comms.node_identity().public_key().clone();
            transaction = Some(outbound_tx);
        } else {
            let mut inbound_transactions = match (*wallet).runtime.block_on(
                (*wallet)
                    .wallet
                    .transaction_service
                    .get_cancelled_pending_inbound_transactions(),
            ) {
                Ok(txs) => txs,
                Err(e) => {
                    error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    return ptr::null_mut();
                },
            };
            if let Some(tx) = inbound_transactions.remove(&transaction_id) {
                let mut inbound_tx = CompletedTransaction::from(tx);
                inbound_tx.destination_public_key = (*wallet).wallet.comms.node_identity().public_key().clone();
                transaction = Some(inbound_tx);
            }
        }
    }

    match transaction {
        Some(tx) => {
            return Box::into_raw(Box::new(tx));
        },
        None => {
            error = LibWalletError::from(WalletError::TransactionServiceError(
                TransactionServiceError::TransactionDoesNotExistError,
            ))
            .code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    ptr::null_mut()
}

/// Get the TariPublicKey from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariPublicKey` - returns the public key, note that ptr::null_mut() is returned
/// if wc is null
///
/// # Safety
/// The ```public_key_destroy``` method must be called when finished with a TariPublicKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_public_key(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariPublicKey {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let pk = (*wallet).wallet.comms.node_identity().public_key().clone();
    Box::into_raw(Box::new(pk))
}

/// Import a UTXO into the wallet. This will add a spendable UTXO and create a faux completed transaction to record the
/// event.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `amount` - The value of the UTXO in MicroTari
/// `spending_key` - The private spending key
/// `source_public_key` - The public key of the source of the transaction
/// `message` - The message that the transaction will have
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` -  Returns the TransactionID of the generated transaction, note that it will be zero if transaction is
/// null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_import_utxo(
    wallet: *mut TariWallet,
    amount: c_ulonglong,
    spending_key: *mut TariPrivateKey,
    source_public_key: *mut TariPublicKey,
    message: *const c_char,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    if spending_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("spending_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    if source_public_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("source_public_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    let message_string = if !message.is_null() {
        CStr::from_ptr(message).to_str().unwrap().to_owned()
    } else {
        error = LibWalletError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        CString::new("Imported UTXO").unwrap().to_str().unwrap().to_owned()
    };

    match (*wallet).runtime.block_on((*wallet).wallet.import_utxo(
        MicroTari::from(amount),
        &(*spending_key).clone(),
        &(*source_public_key).clone(),
        message_string,
    )) {
        Ok(tx_id) => tx_id,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Cancel a Pending Transaction
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `transaction_id` - The TransactionId
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - returns whether the transaction could be cancelled
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_cancel_pending_transaction(
    wallet: *mut TariWallet,
    transaction_id: c_ulonglong,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.cancel_transaction(transaction_id))
    {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// This function will tell the wallet to query the set base node to confirm the status of wallet data. For example this
/// will check that Unspent Outputs stored in the wallet are still available as UTXO's on the blockchain. This will also
/// trigger a request for outstanding SAF messages to you neighbours
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` -  Returns a unique Request Key that is used to identify which callbacks refer to this specific sync
/// request. Note the result will be 0 if there was an error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_sync_with_base_node(wallet: *mut TariWallet, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.validate_utxos(TxoValidationRetry::Limited(1)))
    {
        Ok(request_key) => request_key,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// This function will tell the wallet to do a coin split.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `amount` - The amount to split
/// `count` - The number of times to split the amount
/// `fee` - The transaction fee
/// `msg` - Message for split
/// `lock_height` - The number of bocks to lock the transaction for
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the transaction id.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_coin_split(
    wallet: *mut TariWallet,
    amount: c_ulonglong,
    count: c_ulonglong,
    fee: c_ulonglong,
    msg: *const c_char,
    lock_height: c_ulonglong,
    error_out: *mut c_int,
) -> c_ulonglong
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let message = if !msg.is_null() {
        CStr::from_ptr(msg).to_str().unwrap().to_owned()
    } else {
        "Coin Split".to_string()
    };

    match (*wallet).runtime.block_on((*wallet).wallet.coin_split(
        MicroTari(amount),
        count as usize,
        MicroTari(fee),
        message,
        Some(lock_height),
    )) {
        Ok(request_key) => request_key,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Gets the seed words representing the seed private key of the provided `TariWallet`.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariSeedWords` - A collection of the seed words
///
/// # Safety
/// The ```tari_seed_words_destroy``` method must be called when finished with a
/// TariSeedWords to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_seed_words(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariSeedWords {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.output_manager_service.get_seed_words())
    {
        Ok(sw) => Box::into_raw(Box::new(TariSeedWords(sw))),
        Err(e) => {
            error = LibWalletError::from(WalletError::OutputManagerError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Set the power mode of the wallet to Low Power mode which will reduce the amount of network operations the wallet
/// performs to conserve power
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_set_low_power_mode(wallet: *mut TariWallet, error_out: *mut c_int) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    if let Err(e) = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.set_low_power_mode())
    {
        error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }
}

/// Set the power mode of the wallet to Normal Power mode which will then use the standard level of network traffic
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_set_normal_power_mode(wallet: *mut TariWallet, error_out: *mut c_int) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    if let Err(e) = (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.set_normal_power_mode())
    {
        error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }
}

/// Apply encryption to the databases used in this wallet using the provided passphrase. If the databases are already
/// encrypted this function will fail.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `passphrase` - A string that represents the passphrase will be used to encrypt the databases for this
/// wallet. Once encrypted the passphrase will be required to start a wallet using these databases
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_apply_encryption(
    wallet: *mut TariWallet,
    passphrase: *const c_char,
    error_out: *mut c_int,
)
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    if passphrase.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("passphrase".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    let pf = CStr::from_ptr(passphrase)
        .to_str()
        .expect("A non-null passphrase should be able to be converted to string")
        .to_owned();

    if let Err(e) = (*wallet).runtime.block_on((*wallet).wallet.apply_encryption(pf)) {
        error = LibWalletError::from(e).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }
}

/// Remove encryption to the databases used in this wallet. If this wallet is currently encrypted this encryption will
/// be removed. If it is not encrypted then this function will still succeed to make the operation idempotent
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_remove_encryption(wallet: *mut TariWallet, error_out: *mut c_int) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    if let Err(e) = (*wallet).runtime.block_on((*wallet).wallet.remove_encryption()) {
        error = LibWalletError::from(e).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }
}

/// Set a Key Value in the Wallet storage used for Client Key Value store
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `key` - The pointer to a Utf8 string representing the Key
/// `value` - The pointer to a Utf8 string representing the Value ot be stored
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Return a boolean value indicating the operation's success or failure. The error_ptr will hold the error
/// code if there was a failure
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_set_key_value(
    wallet: *mut TariWallet,
    key: *const c_char,
    value: *const c_char,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    let key_string;
    if key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    } else {
        key_string = CStr::from_ptr(key).to_str().unwrap().to_owned();
    }

    let value_string;
    if value.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("value".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    } else {
        value_string = CStr::from_ptr(value).to_str().unwrap().to_owned();
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.db.set_client_key_value(key_string, value_string))
    {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// get a stored Value that was previously stored in the Wallet storage used for Client Key Value store
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `key` - The pointer to a Utf8 string representing the Key
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array of the Value string. Note that it returns an null pointer if an
/// error occured.
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_value(
    wallet: *mut TariWallet,
    key: *const c_char,
    error_out: *mut c_int,
) -> *mut c_char
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let key_string;
    if key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        key_string = CStr::from_ptr(key).to_str().unwrap().to_owned();
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.db.get_client_key_value(key_string))
    {
        Ok(result) => match result {
            None => {
                error = LibWalletError::from(WalletError::WalletStorageError(WalletStorageError::ValuesNotFound)).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                ptr::null_mut()
            },
            Some(value) => {
                let v = CString::new(value).expect("Should be able to make a CString");
                CString::into_raw(v)
            },
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Clears a Value for the provided Key Value in the Wallet storage used for Client Key Value store
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `key` - The pointer to a Utf8 string representing the Key
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Return a boolean value indicating the operation's success or failure. The error_ptr will hold the error
/// code if there was a failure
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_clear_value(
    wallet: *mut TariWallet,
    key: *const c_char,
    error_out: *mut c_int,
) -> bool
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    let key_string;
    if key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    } else {
        key_string = CStr::from_ptr(key).to_str().unwrap().to_owned();
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.db.clear_client_value(key_string))
    {
        Ok(result) => result,
        Err(e) => {
            error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// This function will produce a partial backup of the specified wallet database file. This backup will be written to
/// the provided file (full path must include the filename and extension) and will include the full wallet db but will
/// clear the sensitive Comms Private Key
///
/// ## Arguments
/// `original_file_path` - The full path of the original database file to be backed up, including the file name and
/// extension `backup_file_path` - The full path, including the file name and extension, of where the backup db will be
/// written `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null.
/// Functions as an out parameter.
///
/// ## Returns
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn file_partial_backup(
    original_file_path: *const c_char,
    backup_file_path: *const c_char,
    error_out: *mut c_int,
)
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let original_path_string;
    if !original_file_path.is_null() {
        original_path_string = CStr::from_ptr(original_file_path).to_str().unwrap().to_owned();
    } else {
        error = LibWalletError::from(InterfaceError::NullError("original_file_path".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }
    let original_path = PathBuf::from(original_path_string);

    let backup_path_string;
    if !backup_file_path.is_null() {
        backup_path_string = CStr::from_ptr(backup_file_path).to_str().unwrap().to_owned();
    } else {
        error = LibWalletError::from(InterfaceError::NullError("backup_file_path".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }
    let backup_path = PathBuf::from(backup_path_string);

    let runtime = Runtime::new();
    match runtime {
        Ok(mut runtime) => match runtime.block_on(partial_wallet_backup(original_path, backup_path)) {
            Ok(_) => (),
            Err(e) => {
                error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
                ptr::swap(error_out, &mut error as *mut c_int);
            },
        },
        Err(e) => {
            error = LibWalletError::from(InterfaceError::TokioError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }
}

/// Gets the current emoji set
///
/// ## Arguments
/// `()` - Does not take any arguments
///
/// ## Returns
/// `*mut EmojiSet` - Pointer to the created EmojiSet.
///
/// # Safety
/// The ```emoji_set_destroy``` function must be called when finished with a ByteVector to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn get_emoji_set() -> *mut EmojiSet {
    let current_emoji_set = emoji_set();
    let mut emoji_set: Vec<ByteVector> = Vec::with_capacity(current_emoji_set.len());
    for emoji in current_emoji_set.iter() {
        let mut b = [0; 4]; // emojis are 4 bytes, unicode character
        let emoji_char = ByteVector(emoji.encode_utf8(&mut b).as_bytes().to_vec());
        emoji_set.push(emoji_char);
    }
    let result = EmojiSet(emoji_set);
    Box::into_raw(Box::new(result))
}

/// Gets the length of the current emoji set
///
/// ## Arguments
/// `*mut EmojiSet` - Pointer to emoji set
///
/// ## Returns
/// `c_int` - Pointer to the created EmojiSet.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn emoji_set_get_length(emoji_set: *const EmojiSet, error_out: *mut c_int) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if emoji_set.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("emoji_set".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*emoji_set).0.len() as c_uint
}

/// Gets a ByteVector at position in a EmojiSet
///
/// ## Arguments
/// `emoji_set` - The pointer to a EmojiSet
/// `position` - The integer position
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `ByteVector` - Returns a ByteVector. Note that the ByteVector will be null if ptr
/// is null or if the position is invalid
///
/// # Safety
/// The ```byte_vector_destroy``` function must be called when finished with the ByteVector to prevent a memory leak.
#[no_mangle]
pub unsafe extern "C" fn emoji_set_get_at(
    emoji_set: *const EmojiSet,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut ByteVector
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if emoji_set.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("emoji_set".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let last_index = emoji_set_get_length(emoji_set, error_out) - 1;
    if position > last_index {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let result = (*emoji_set).0[position as usize].clone();
    Box::into_raw(Box::new(result))
}

/// Frees memory for a EmojiSet
///
/// ## Arguments
/// `emoji_set` - The EmojiSet pointer
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn emoji_set_destroy(emoji_set: *mut EmojiSet) {
    if !emoji_set.is_null() {
        Box::from_raw(emoji_set);
    }
}

/// Frees memory for a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_destroy(wallet: *mut TariWallet) {
    if !wallet.is_null() {
        let mut w = Box::from_raw(wallet);
        match w.shutdown.trigger() {
            Err(_) => error!(target: LOG_TARGET, "No listeners for the shutdown signal!"),
            Ok(()) => w.runtime.block_on(w.wallet.wait_until_shutdown()),
        }
    }
}

/// This function will log the provided string at debug level. To be used to have a client log messages to the LibWallet
/// logs.
///
/// ## Arguments
/// `msg` - A string that will be logged at the debug level. If msg is null nothing will be done.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn log_debug_message(msg: *const c_char) {
    if !msg.is_null() {
        let message = CStr::from_ptr(msg).to_str().unwrap().to_owned();
        debug!(target: LOG_TARGET, "{}", message);
    }
}

#[cfg(test)]
mod test {

    use crate::*;
    use libc::{c_char, c_uchar, c_uint};
    use std::{
        ffi::CString,
        path::Path,
        str::{from_utf8, FromStr},
        sync::Mutex,
        thread,
    };
    use tari_comms::types::CommsPublicKey;
    use tari_core::transactions::{fee::Fee, tari_amount::uT, types::PrivateKey};
    use tari_key_manager::mnemonic::Mnemonic;
    use tari_wallet::{
        testnet_utils::random_string,
        transaction_service::storage::models::TransactionStatus,
        util::emoji,
    };
    use tempfile::tempdir;

    fn type_of<T>(_: T) -> String {
        std::any::type_name::<T>().to_string()
    }

    #[derive(Debug)]
    struct CallbackState {
        pub received_tx_callback_called: bool,
        pub received_tx_reply_callback_called: bool,
        pub received_finalized_tx_callback_called: bool,
        pub broadcast_tx_callback_called: bool,
        pub mined_tx_callback_called: bool,
        pub direct_send_callback_called: bool,
        pub store_and_forward_send_callback_called: bool,
        pub tx_cancellation_callback_called: bool,
        pub base_node_sync_callback_called: bool,
    }

    impl CallbackState {
        fn new() -> Self {
            Self {
                received_tx_callback_called: false,
                received_tx_reply_callback_called: false,
                received_finalized_tx_callback_called: false,
                broadcast_tx_callback_called: false,
                mined_tx_callback_called: false,
                direct_send_callback_called: false,
                store_and_forward_send_callback_called: false,
                base_node_sync_callback_called: false,
                tx_cancellation_callback_called: false,
            }
        }

        fn reset(&mut self) {
            self.received_tx_callback_called = false;
            self.received_tx_reply_callback_called = false;
            self.received_finalized_tx_callback_called = false;
            self.broadcast_tx_callback_called = false;
            self.mined_tx_callback_called = false;
            self.direct_send_callback_called = false;
            self.store_and_forward_send_callback_called = false;
            self.tx_cancellation_callback_called = false;
            self.base_node_sync_callback_called = false;
        }
    }

    lazy_static! {
        static ref CALLBACK_STATE_FFI: Mutex<CallbackState> = {
            let c = Mutex::new(CallbackState::new());
            c
        };
    }

    unsafe extern "C" fn received_tx_callback(tx: *mut TariPendingInboundTransaction) {
        assert_eq!(tx.is_null(), false);
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
        assert_eq!(tx.is_null(), false);
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
        assert_eq!(tx.is_null(), false);
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
        assert_eq!(tx.is_null(), false);
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
        assert_eq!(tx.is_null(), false);
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::Mined);
        let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
        lock.mined_tx_callback_called = true;
        drop(lock);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn direct_send_callback(_tx_id: c_ulonglong, _result: bool) {
        assert!(true);
    }

    unsafe extern "C" fn store_and_forward_send_callback(_tx_id: c_ulonglong, _result: bool) {
        assert!(true);
    }

    unsafe extern "C" fn tx_cancellation_callback(tx: *mut TariCompletedTransaction) {
        assert_eq!(tx.is_null(), false);
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn base_node_sync_process_complete_callback(_tx_id: c_ulonglong, _result: bool) {
        assert!(true);
    }

    unsafe extern "C" fn saf_messages_received_callback() {
        assert!(true);
    }

    unsafe extern "C" fn received_tx_callback_bob(tx: *mut TariPendingInboundTransaction) {
        assert_eq!(tx.is_null(), false);
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariPendingInboundTransaction>()
        );
        pending_inbound_transaction_destroy(tx);
    }

    unsafe extern "C" fn received_tx_reply_callback_bob(tx: *mut TariCompletedTransaction) {
        assert_eq!(tx.is_null(), false);
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::Completed);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn received_tx_finalized_callback_bob(tx: *mut TariCompletedTransaction) {
        assert_eq!(tx.is_null(), false);
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::Completed);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn broadcast_callback_bob(tx: *mut TariCompletedTransaction) {
        assert_eq!(tx.is_null(), false);
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::Broadcast);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn mined_callback_bob(tx: *mut TariCompletedTransaction) {
        assert_eq!(tx.is_null(), false);
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        assert_eq!((*tx).status, TransactionStatus::Mined);
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn direct_send_callback_bob(_tx_id: c_ulonglong, _result: bool) {
        assert!(true);
    }

    unsafe extern "C" fn store_and_forward_send_callback_bob(_tx_id: c_ulonglong, _result: bool) {
        assert!(true);
    }

    unsafe extern "C" fn tx_cancellation_callback_bob(tx: *mut TariCompletedTransaction) {
        assert_eq!(tx.is_null(), false);
        assert_eq!(
            type_of((*tx).clone()),
            std::any::type_name::<TariCompletedTransaction>()
        );
        completed_transaction_destroy(tx);
    }

    unsafe extern "C" fn base_node_sync_process_complete_callback_bob(_tx_id: c_ulonglong, _result: bool) {
        assert!(true);
    }

    unsafe extern "C" fn saf_messages_received_callback_bob() {
        assert!(true);
    }

    #[test]
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
            let bytes_ptr = byte_vector_create(ptr::null_mut(), 20 as c_uint, error_ptr);
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
                assert_eq!(compare, true);
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
        }
    }

    #[test]
    fn test_transport_type_tcp() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let address_listener = CString::new("/ip4/127.0.0.1/tcp/0").unwrap();
            let address_listener_str: *const c_char = CString::into_raw(address_listener.clone()) as *const c_char;
            let _transport = transport_tcp_create(address_listener_str, error_ptr);
            assert_eq!(error, 0);
        }
    }

    #[test]
    fn test_transport_type_tor() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let address_control = CString::new("/ip4/127.0.0.1/tcp/8080").unwrap();
            let address_control_str: *const c_char = CString::into_raw(address_control.clone()) as *const c_char;
            let _transport = transport_tor_create(
                address_control_str,
                ptr::null_mut(),
                8080,
                ptr::null_mut(),
                ptr::null_mut(),
                error_ptr,
            );
            assert_eq!(error, 0);
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
            let private_bytes = private_key_get_bytes(private_key, error_ptr);
            assert_eq!(error, 0);
            let public_bytes = public_key_get_bytes(public_key, error_ptr);
            assert_eq!(error, 0);
            let private_key_length = byte_vector_get_length(private_bytes, error_ptr);
            assert_eq!(error, 0);
            let public_key_length = byte_vector_get_length(public_bytes, error_ptr);
            assert_eq!(error, 0);
            assert_eq!(private_key_length, 32);
            assert_eq!(public_key_length, 32);
            assert_ne!((*private_bytes), (*public_bytes));
            let emoji = public_key_to_emoji_id(public_key, error_ptr) as *mut c_char;
            let emoji_str = CStr::from_ptr(emoji).to_str().unwrap();
            assert!(EmojiId::is_valid(emoji_str));
            let pk_emoji = emoji_id_to_public_key(emoji, error_ptr);
            assert_eq!((*public_key), (*pk_emoji));
            private_key_destroy(private_key);
            public_key_destroy(public_key);
            public_key_destroy(pk_emoji);
            byte_vector_destroy(public_bytes);
            byte_vector_destroy(private_bytes);
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
            let test_contact_public_key = public_key_from_private_key(test_contact_private_key, error_ptr);
            let test_str = "Test Contact";
            let test_contact_str = CString::new(test_str).unwrap();
            let test_contact_alias: *const c_char = CString::into_raw(test_contact_str) as *const c_char;
            let test_contact = contact_create(test_contact_alias, test_contact_public_key, error_ptr);
            let alias = contact_get_alias(test_contact, error_ptr);
            let alias_string = CString::from_raw(alias).to_str().unwrap().to_owned();
            assert_eq!(alias_string, test_str);
            let contact_key = contact_get_public_key(test_contact, error_ptr);
            let contact_key_bytes = public_key_get_bytes(contact_key, error_ptr);
            let contact_bytes_len = byte_vector_get_length(contact_key_bytes, error_ptr);
            assert_eq!(contact_bytes_len, 32);
            contact_destroy(test_contact);
            public_key_destroy(test_contact_public_key);
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
            let test_contact_public_key = public_key_from_private_key(test_contact_private_key, error_ptr);
            let test_str = "Test Contact";
            let test_contact_str = CString::new(test_str).unwrap();
            let test_contact_alias: *const c_char = CString::into_raw(test_contact_str) as *const c_char;
            let mut _test_contact = contact_create(ptr::null_mut(), test_contact_public_key, error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("alias_ptr".to_string())).code
            );
            _test_contact = contact_create(test_contact_alias, ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("public_key_ptr".to_string())).code
            );
            let _alias = contact_get_alias(ptr::null_mut(), error_ptr);
            assert_eq!(
                error,
                LibWalletError::from(InterfaceError::NullError("contact_ptr".to_string())).code
            );
            let _contact_key = contact_get_public_key(ptr::null_mut(), error_ptr);
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
            public_key_destroy(test_contact_public_key);
            private_key_destroy(test_contact_private_key);
            string_destroy(test_contact_alias as *mut c_char);
            byte_vector_destroy(contact_key_bytes);
        }
    }

    #[test]
    fn test_wallet_ffi() {
        unsafe {
            {
                let mut lock = CALLBACK_STATE_FFI.lock().unwrap();
                lock.reset();
            }
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;
            let secret_key_alice = private_key_generate();
            let public_key_alice = public_key_from_private_key(secret_key_alice.clone(), error_ptr);
            let db_name_alice = CString::new(random_string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice.clone()) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice.clone()) as *const c_char;
            let transport_type_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_type_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;

            let alice_log_path =
                CString::new(format!("{}{}", alice_temp_dir.path().to_str().unwrap(), "/test.log")).unwrap();
            let alice_log_path_str: *const c_char = CString::into_raw(alice_log_path.clone()) as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_type_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                error_ptr,
            );
            comms_config_set_secret_key(alice_config, secret_key_alice, error_ptr);
            let alice_wallet = wallet_create(
                alice_config,
                alice_log_path_str,
                2,
                10000,
                ptr::null(),
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                direct_send_callback,
                store_and_forward_send_callback,
                tx_cancellation_callback,
                base_node_sync_process_complete_callback,
                saf_messages_received_callback,
                error_ptr,
            );
            let secret_key_bob = private_key_generate();
            let public_key_bob = public_key_from_private_key(secret_key_bob.clone(), error_ptr);
            let db_name_bob = CString::new(random_string(8).as_str()).unwrap();
            let db_name_bob_str: *const c_char = CString::into_raw(db_name_bob.clone()) as *const c_char;
            let bob_temp_dir = tempdir().unwrap();
            let db_path_bob = CString::new(bob_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_bob_str: *const c_char = CString::into_raw(db_path_bob.clone()) as *const c_char;
            let transport_type_bob = transport_memory_create();
            let address_bob = transport_memory_get_address(transport_type_bob, error_ptr);
            let address_bob_str = CStr::from_ptr(address_bob).to_str().unwrap().to_owned();
            let address_bob_str: *const c_char = CString::new(address_bob_str).unwrap().into_raw() as *const c_char;
            let bob_config = comms_config_create(
                address_bob_str,
                transport_type_bob,
                db_name_bob_str,
                db_path_bob_str,
                20,
                error_ptr,
            );
            comms_config_set_secret_key(bob_config, secret_key_bob, error_ptr);

            let bob_log_path =
                CString::new(format!("{}{}", bob_temp_dir.path().to_str().unwrap(), "/test.log")).unwrap();
            let bob_log_path_str: *const c_char = CString::into_raw(bob_log_path.clone()) as *const c_char;

            let bob_wallet = wallet_create(
                bob_config,
                bob_log_path_str,
                0,
                0,
                ptr::null(),
                received_tx_callback_bob,
                received_tx_reply_callback_bob,
                received_tx_finalized_callback_bob,
                broadcast_callback_bob,
                mined_callback_bob,
                direct_send_callback_bob,
                store_and_forward_send_callback_bob,
                tx_cancellation_callback_bob,
                base_node_sync_process_complete_callback_bob,
                saf_messages_received_callback_bob,
                error_ptr,
            );

            let sig_msg = CString::new("Test Contact").unwrap();
            let sig_msg_str: *const c_char = CString::into_raw(sig_msg) as *const c_char;
            let sig_msg_compare = CString::new("Test Contact").unwrap();
            let sig_msg_compare_str: *const c_char = CString::into_raw(sig_msg_compare) as *const c_char;
            let sig_nonce_str: *mut c_char = wallet_sign_message(alice_wallet, sig_msg_str, error_ptr) as *mut c_char;
            let alice_wallet_key = wallet_get_public_key(alice_wallet, error_ptr);
            let verify_msg = wallet_verify_message_signature(
                alice_wallet,
                alice_wallet_key,
                sig_nonce_str,
                sig_msg_compare_str,
                error_ptr,
            );
            assert_eq!(verify_msg, true);

            let test_contact_private_key = private_key_generate();
            let test_contact_public_key = public_key_from_private_key(test_contact_private_key, error_ptr);
            let test_contact_str = CString::new("Test Contact").unwrap();
            let test_contact_alias: *const c_char = CString::into_raw(test_contact_str) as *const c_char;
            let test_contact = contact_create(test_contact_alias, test_contact_public_key, error_ptr);
            let contact_added = wallet_upsert_contact(alice_wallet, test_contact, error_ptr);
            assert_eq!(contact_added, true);
            let contact_removed = wallet_remove_contact(alice_wallet, test_contact, error_ptr);
            assert_eq!(contact_removed, true);
            contact_destroy(test_contact);
            public_key_destroy(test_contact_public_key);
            private_key_destroy(test_contact_private_key);
            string_destroy(test_contact_alias as *mut c_char);

            let generated = wallet_test_generate_data(alice_wallet, db_path_alice_str, error_ptr);
            assert_eq!(generated, true);

            // minimum fee
            let fee = wallet_get_fee_estimate(alice_wallet, 100, 1, 1, 1, error_ptr);
            assert_eq!(fee, 100);
            assert_eq!(error, 0);

            for outputs in 1..5 {
                let fee = wallet_get_fee_estimate(alice_wallet, 100, 25, 1, outputs, error_ptr);
                assert_eq!(
                    MicroTari::from(fee),
                    Fee::calculate(MicroTari::from(25), 1, 1, outputs as usize)
                );
                assert_eq!(error, 0);
            }

            // not enough funds
            let fee = wallet_get_fee_estimate(alice_wallet, 1_000_000_000, 2_500, 1, 1, error_ptr);
            assert_eq!(fee, 0);
            assert_eq!(error, 101);

            assert_eq!(
                (wallet_get_completed_transactions(&mut (*alice_wallet), error_ptr)).is_null(),
                false
            );
            assert_eq!(
                (wallet_get_pending_inbound_transactions(&mut (*alice_wallet), error_ptr)).is_null(),
                false
            );
            assert_eq!(
                (wallet_get_pending_outbound_transactions(&mut (*alice_wallet), error_ptr)).is_null(),
                false
            );

            let inbound_transactions: std::collections::HashMap<
                u64,
                tari_wallet::transaction_service::storage::models::InboundTransaction,
            > = (*alice_wallet)
                .runtime
                .block_on(
                    (*alice_wallet)
                        .wallet
                        .transaction_service
                        .get_pending_inbound_transactions(),
                )
                .unwrap();

            assert_eq!(inbound_transactions.len(), 0);

            // `wallet_test_generate_data(...)` creates 5 completed inbound tx which should appear in this list
            let ffi_inbound_txs = wallet_get_pending_inbound_transactions(&mut (*alice_wallet), error_ptr);
            assert_eq!(pending_inbound_transactions_get_length(ffi_inbound_txs, error_ptr), 5);

            wallet_test_receive_transaction(alice_wallet, error_ptr);

            let inbound_transactions: std::collections::HashMap<
                u64,
                tari_wallet::transaction_service::storage::models::InboundTransaction,
            > = (*alice_wallet)
                .runtime
                .block_on(
                    (*alice_wallet)
                        .wallet
                        .transaction_service
                        .get_pending_inbound_transactions(),
                )
                .unwrap();

            assert_eq!(inbound_transactions.len(), 1);

            let ffi_inbound_txs = wallet_get_pending_inbound_transactions(&mut (*alice_wallet), error_ptr);
            assert_eq!(pending_inbound_transactions_get_length(ffi_inbound_txs, error_ptr), 6);

            let mut found_pending = false;
            for i in 0..pending_inbound_transactions_get_length(ffi_inbound_txs, error_ptr) {
                let pending_tx = pending_inbound_transactions_get_at(ffi_inbound_txs, i, error_ptr);
                let status = pending_inbound_transaction_get_status(pending_tx, error_ptr);
                if status == 4 {
                    found_pending = true;
                }
            }
            assert!(found_pending, "At least 1 transaction should be in the Pending state");

            // `wallet_test_generate_data(...)` creates 9 completed outbound transactions that are not mined
            let ffi_outbound_txs = wallet_get_pending_outbound_transactions(&mut (*alice_wallet), error_ptr);
            assert_eq!(pending_outbound_transactions_get_length(ffi_outbound_txs, error_ptr), 9);

            let mut found_broadcast = false;
            for i in 0..pending_outbound_transactions_get_length(ffi_outbound_txs, error_ptr) {
                let pending_tx = pending_outbound_transactions_get_at(ffi_outbound_txs, i, error_ptr);
                let status = pending_outbound_transaction_get_status(pending_tx, error_ptr);
                if status == 1 {
                    found_broadcast = true;
                }
            }
            assert!(
                found_broadcast,
                "At least 1 transaction should be in the Broadcast state"
            );

            let completed_transactions: std::collections::HashMap<
                u64,
                tari_wallet::transaction_service::storage::models::CompletedTransaction,
            > = (*alice_wallet)
                .runtime
                .block_on((*alice_wallet).wallet.transaction_service.get_completed_transactions())
                .unwrap();

            let num_completed_tx_pre = completed_transactions.len();

            for (_k, v) in inbound_transactions {
                let tx_ptr = Box::into_raw(Box::new(v.clone()));
                wallet_test_finalize_received_transaction(alice_wallet, tx_ptr, error_ptr);
                break;
            }

            let completed_transactions: std::collections::HashMap<
                u64,
                tari_wallet::transaction_service::storage::models::CompletedTransaction,
            > = (*alice_wallet)
                .runtime
                .block_on((*alice_wallet).wallet.transaction_service.get_completed_transactions())
                .unwrap();

            assert_eq!(num_completed_tx_pre + 1, completed_transactions.len());

            // At this stage there is only 1 Mined transaction created by the `wallet_test_generate_data(...)` function
            let ffi_completed_txs = wallet_get_completed_transactions(&mut (*alice_wallet), error_ptr);
            assert_eq!(completed_transactions_get_length(ffi_completed_txs, error_ptr), 1);

            for x in 0..completed_transactions_get_length(ffi_completed_txs, error_ptr) {
                let id_completed = completed_transactions_get_at(&mut (*ffi_completed_txs), x, error_ptr);
                let id_completed_get = wallet_get_completed_transaction_by_id(
                    &mut (*alice_wallet),
                    (&mut (*id_completed)).tx_id,
                    error_ptr,
                );
                if (&mut (*id_completed)).status == TransactionStatus::Mined {
                    assert_eq!((*id_completed), (*id_completed_get));
                    assert_eq!((*id_completed_get).status, TransactionStatus::Mined);
                } else {
                    assert_eq!(id_completed_get, ptr::null_mut());
                    let pk_compare = wallet_get_public_key(&mut (*alice_wallet), error_ptr);
                    if (&mut (*pk_compare)).as_bytes() == (&mut (*id_completed)).destination_public_key.as_bytes() {
                        let id_inbound_get = wallet_get_pending_inbound_transaction_by_id(
                            &mut (*alice_wallet),
                            (&mut (*id_completed_get)).tx_id,
                            error_ptr,
                        );
                        assert_ne!(id_inbound_get, ptr::null_mut());
                        assert_ne!((&mut (*id_inbound_get)).status, TransactionStatus::Mined);
                        pending_inbound_transaction_destroy(&mut (*id_inbound_get));
                    } else {
                        let id_outbound_get = wallet_get_pending_outbound_transaction_by_id(
                            &mut (*alice_wallet),
                            (&mut (*id_completed_get)).tx_id,
                            error_ptr,
                        );
                        assert_ne!(id_outbound_get, ptr::null_mut());
                        assert_ne!((&mut (*id_outbound_get)).status, TransactionStatus::Mined);
                        pending_outbound_transaction_destroy(&mut (*id_outbound_get));
                    }
                    public_key_destroy(&mut (*pk_compare));
                }
                completed_transaction_destroy(&mut (*id_completed));
                completed_transaction_destroy(&mut (*id_completed_get));
            }

            // TODO: Test transaction collection and transaction methods
            let completed_transactions = (*alice_wallet)
                .runtime
                .block_on((*alice_wallet).wallet.transaction_service.get_completed_transactions())
                .unwrap();

            for (_k, v) in completed_transactions {
                if v.status == TransactionStatus::Completed {
                    let tx_ptr = Box::into_raw(Box::new(v.clone()));
                    wallet_test_broadcast_transaction(alice_wallet, (*tx_ptr).tx_id, error_ptr);
                    wallet_test_mine_transaction(alice_wallet, (*tx_ptr).tx_id, error_ptr);
                    // test ffi calls for excess, public nonce, and signature
                    let kernels = v.transaction.get_body().kernels();
                    if !kernels.is_empty() {
                        for k in kernels {
                            let x = completed_transaction_get_excess(tx_ptr, error_ptr);
                            assert_eq!(k.excess, *x);
                            excess_destroy(x);
                            let nonce = k.excess_sig.get_public_nonce().clone();
                            let r = completed_transaction_get_public_nonce(tx_ptr, error_ptr);
                            assert_eq!(nonce, *r);
                            nonce_destroy(r);
                            let sig = k.excess_sig.get_signature().clone();
                            let s = completed_transaction_get_signature(tx_ptr, error_ptr);
                            assert_eq!(sig, *s);
                            signature_destroy(s);
                        }
                    } else {
                        let x = completed_transaction_get_excess(tx_ptr, error_ptr);
                        assert!(x.is_null());
                        excess_destroy(x);
                        let r = completed_transaction_get_public_nonce(tx_ptr, error_ptr);
                        assert!(r.is_null());
                        nonce_destroy(r);
                        let s = completed_transaction_get_signature(tx_ptr, error_ptr);
                        assert!(s.is_null());
                        signature_destroy(s);
                    }
                }
            }

            // Now all completed transactions are mined as should be returned
            let ffi_completed_txs = wallet_get_completed_transactions(&mut (*alice_wallet), error_ptr);
            assert_eq!(completed_transactions_get_length(ffi_completed_txs, error_ptr), 15);

            let contacts = wallet_get_contacts(alice_wallet, error_ptr);
            assert_eq!(contacts_get_length(contacts, error_ptr), 4);

            let utxo_spending_key = private_key_generate();
            let utxo_value = 20000;

            let pre_balance = (*alice_wallet)
                .runtime
                .block_on((*alice_wallet).wallet.output_manager_service.get_balance())
                .unwrap();

            let secret_key_base_node = private_key_generate();
            let public_key_base_node = public_key_from_private_key(secret_key_base_node.clone(), error_ptr);
            let utxo_message_str = CString::new("UTXO Import").unwrap();
            let utxo_message: *const c_char = CString::into_raw(utxo_message_str) as *const c_char;

            let utxo_tx_id = wallet_import_utxo(
                alice_wallet,
                utxo_value,
                utxo_spending_key,
                public_key_base_node,
                utxo_message,
                error_ptr,
            );

            let post_balance = (*alice_wallet)
                .runtime
                .block_on((*alice_wallet).wallet.output_manager_service.get_balance())
                .unwrap();

            assert_eq!(
                pre_balance.available_balance + utxo_value * uT,
                post_balance.available_balance
            );

            let import_transaction = (*alice_wallet)
                .runtime
                .block_on((*alice_wallet).wallet.transaction_service.get_completed_transactions())
                .unwrap()
                .remove(&utxo_tx_id)
                .expect("Tx should be in collection");

            assert_eq!(import_transaction.amount, utxo_value * uT);
            assert_eq!(wallet_sync_with_base_node(alice_wallet, error_ptr), 0);
            let mut peer_added =
                wallet_add_base_node_peer(alice_wallet, public_key_bob.clone(), address_bob_str, error_ptr);
            assert_eq!(peer_added, true);
            peer_added = wallet_add_base_node_peer(bob_wallet, public_key_alice.clone(), address_alice_str, error_ptr);
            assert_eq!(peer_added, true);
            assert!(wallet_sync_with_base_node(alice_wallet, error_ptr) > 0);

            // Test pending tx cancellation
            let ffi_cancelled_txs = wallet_get_cancelled_transactions(&mut (*alice_wallet), error_ptr);
            assert_eq!(
                completed_transactions_get_length(ffi_cancelled_txs, error_ptr),
                0,
                "Should have no cancelled txs"
            );

            wallet_test_receive_transaction(&mut (*alice_wallet), error_ptr);

            let inbound_txs = (*alice_wallet)
                .runtime
                .block_on(
                    (*alice_wallet)
                        .wallet
                        .transaction_service
                        .get_pending_inbound_transactions(),
                )
                .unwrap();

            let mut inbound_tx_id = 0;
            for (k, v) in inbound_txs {
                // test ffi calls for excess, public nonce, and signature when given a pending tx
                let tx_ptr = Box::into_raw(Box::new(CompletedTransaction::from(v.clone())));
                let x = completed_transaction_get_excess(tx_ptr, error_ptr);
                assert!(x.is_null());
                excess_destroy(x);
                let r = completed_transaction_get_public_nonce(tx_ptr, error_ptr);
                assert!(r.is_null());
                nonce_destroy(r);
                let s = completed_transaction_get_signature(tx_ptr, error_ptr);
                assert!(s.is_null());
                signature_destroy(s);

                inbound_tx_id = k;

                let inbound_tx = wallet_get_cancelled_transaction_by_id(&mut (*alice_wallet), inbound_tx_id, error_ptr);

                assert_eq!(inbound_tx, ptr::null_mut());

                (*alice_wallet)
                    .runtime
                    .block_on(async { (*alice_wallet).wallet.transaction_service.cancel_transaction(k).await })
                    .unwrap();

                let inbound_tx = wallet_get_cancelled_transaction_by_id(&mut (*alice_wallet), inbound_tx_id, error_ptr);

                assert_ne!(inbound_tx, ptr::null_mut());
                assert_eq!(completed_transaction_get_transaction_id(inbound_tx, error_ptr), k);

                break;
            }

            let mut found_cancelled_tx = false;
            let mut ffi_cancelled_txs = ptr::null_mut();
            for _ in 0..12 {
                ffi_cancelled_txs = wallet_get_cancelled_transactions(&mut (*alice_wallet), error_ptr);
                if completed_transactions_get_length(ffi_cancelled_txs, error_ptr) >= 1 {
                    found_cancelled_tx = true;
                    break;
                }
                thread::sleep(Duration::from_secs(5));
            }
            assert!(found_cancelled_tx, "Should have found a cancelled tx");

            let cancelled_tx = completed_transactions_get_at(ffi_cancelled_txs, 0, error_ptr);
            let tx_id = completed_transaction_get_transaction_id(cancelled_tx, error_ptr);
            let dest_pubkey = completed_transaction_get_destination_public_key(cancelled_tx, error_ptr);
            let pub_key_ptr = Box::into_raw(Box::new(
                (*alice_wallet).wallet.comms.node_identity().public_key().clone(),
            ));
            assert_eq!(tx_id, inbound_tx_id);
            assert_eq!(*dest_pubkey, *pub_key_ptr);
            public_key_destroy(pub_key_ptr);

            completed_transaction_destroy(cancelled_tx);

            let split_msg = CString::new("Test Coin Split").unwrap();
            let split_msg_str: *const c_char = CString::into_raw(split_msg) as *const c_char;
            let split_tx_id = wallet_coin_split(alice_wallet, 1000, 3, 100, split_msg_str, 0, error_ptr);
            assert_eq!(error, 0);
            let split_tx = (*alice_wallet).runtime.block_on(
                (*alice_wallet)
                    .wallet
                    .transaction_service
                    .get_completed_transaction(split_tx_id),
            );
            assert_eq!(split_tx.is_ok(), true);
            string_destroy(split_msg_str as *mut c_char);

            wallet_set_low_power_mode(alice_wallet, error_ptr);
            assert_eq!((*error_ptr), 0);
            wallet_set_normal_power_mode(alice_wallet, error_ptr);
            assert_eq!((*error_ptr), 0);

            // Test seed words
            let seed_words = wallet_get_seed_words(alice_wallet, error_ptr);
            let seed_word_len = seed_words_get_length(seed_words, error_ptr);

            let mut seed_words_vec = Vec::new();
            for i in 0..seed_word_len {
                let word = seed_words_get_at(seed_words, i as c_uint, error_ptr);
                let word_string = CString::from_raw(word).to_str().unwrap().to_owned();
                seed_words_vec.push(word_string);
            }
            let _seed_word_private_key = PrivateKey::from_mnemonic(&seed_words_vec)
                .expect("Seed words should be able to convert to private key");

            let lock = CALLBACK_STATE_FFI.lock().unwrap();
            assert!(lock.received_tx_callback_called);
            assert!(lock.received_tx_reply_callback_called);
            assert!(lock.received_finalized_tx_callback_called);
            assert!(lock.broadcast_tx_callback_called);
            assert!(lock.mined_tx_callback_called);
            drop(lock);
            // Not testing for the discovery_process_completed callback as its tricky to evoke and it is unit tested
            // elsewhere

            // free string memory
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
            transport_type_destroy(transport_type_alice);
            transport_type_destroy(transport_type_bob);
            seed_words_destroy(seed_words);
        }
    }

    #[test]
    fn test_comms_private_key_persistence() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let secret_key_alice = private_key_generate();
            let public_key_alice = public_key_from_private_key(secret_key_alice.clone(), error_ptr);
            let db_name = random_string(8);
            let db_name_alice = CString::new(db_name.as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice.clone()) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice.clone()) as *const c_char;
            let transport_type_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_type_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;

            let sql_database_path = Path::new(alice_temp_dir.path().to_str().unwrap())
                .join(db_name)
                .with_extension("sqlite3");

            let alice_config = comms_config_create(
                address_alice_str,
                transport_type_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                error_ptr,
            );

            let alice_config2 = comms_config_create(
                address_alice_str,
                transport_type_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                error_ptr,
            );

            let mut runtime = Runtime::new().unwrap();

            let connection =
                run_migration_and_create_sqlite_connection(&sql_database_path).expect("Could not open Sqlite db");
            let wallet_backend = WalletDatabase::new(WalletSqliteDatabase::new(connection, None).unwrap());

            let stored_key = runtime.block_on(wallet_backend.get_comms_secret_key()).unwrap();
            drop(wallet_backend);
            assert!(stored_key.is_none(), "No key should be stored yet");
            let generated_public_key1 = (*alice_config).node_identity.public_key().clone();

            comms_config_set_secret_key(alice_config, secret_key_alice, error_ptr);
            assert_eq!(*error_ptr, 0, "No error expected");

            assert_eq!(&(*public_key_alice), (*alice_config).node_identity.public_key());

            assert_ne!(&generated_public_key1, (*alice_config2).node_identity.public_key());

            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                ptr::null(),
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                direct_send_callback,
                store_and_forward_send_callback,
                tx_cancellation_callback,
                base_node_sync_process_complete_callback,
                saf_messages_received_callback,
                error_ptr,
            );

            assert_eq!(*error_ptr, 0, "No error expected");
            wallet_destroy(alice_wallet);

            let connection =
                run_migration_and_create_sqlite_connection(&sql_database_path).expect("Could not open Sqlite db");
            let wallet_backend = WalletDatabase::new(WalletSqliteDatabase::new(connection, None).unwrap());

            let stored_key = runtime
                .block_on(wallet_backend.get_comms_secret_key())
                .unwrap()
                .unwrap();
            let public_stored_key = CommsPublicKey::from_secret_key(&stored_key);
            assert_eq!(public_stored_key, (*public_key_alice));
            drop(wallet_backend);

            // Test the file path based version
            let backup_path_alice =
                CString::new(alice_temp_dir.path().join("backup.sqlite3").to_str().unwrap()).unwrap();
            let backup_path_alice_str: *const c_char = CString::into_raw(backup_path_alice.clone()) as *const c_char;
            let original_path_cstring = CString::new(sql_database_path.to_str().unwrap()).unwrap();
            let original_path_str: *const c_char = CString::into_raw(original_path_cstring.clone()) as *const c_char;
            file_partial_backup(original_path_str, backup_path_alice_str, error_ptr);

            let sql_database_path = alice_temp_dir.path().join("backup").with_extension("sqlite3");
            let connection =
                run_migration_and_create_sqlite_connection(&sql_database_path).expect("Could not open Sqlite db");
            let wallet_backend = WalletDatabase::new(WalletSqliteDatabase::new(connection, None).unwrap());

            let stored_key = runtime.block_on(wallet_backend.get_comms_secret_key()).unwrap();

            assert!(stored_key.is_none(), "key should be cleared");
            drop(wallet_backend);

            let alice_config3 = comms_config_create(
                address_alice_str,
                transport_type_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                error_ptr,
            );
            assert_eq!((*alice_config3).node_identity.public_key(), &(*public_key_alice));

            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            string_destroy(backup_path_alice_str as *mut c_char);
            string_destroy(original_path_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            public_key_destroy(public_key_alice);
            transport_type_destroy(transport_type_alice);
            comms_config_destroy(alice_config);
            comms_config_destroy(alice_config2);
            comms_config_destroy(alice_config3);
        }
    }

    #[test]
    fn test_wallet_encryption() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let secret_key_alice = private_key_generate();
            let public_key_alice = public_key_from_private_key(secret_key_alice.clone(), error_ptr);
            let db_name_alice = CString::new(random_string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice.clone()) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice.clone()) as *const c_char;
            let transport_type_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_type_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_type_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                error_ptr,
            );
            comms_config_set_secret_key(alice_config, secret_key_alice, error_ptr);
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                ptr::null(),
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                direct_send_callback,
                store_and_forward_send_callback,
                tx_cancellation_callback,
                base_node_sync_process_complete_callback,
                saf_messages_received_callback,
                error_ptr,
            );

            let generated = wallet_test_generate_data(alice_wallet, db_path_alice_str, error_ptr);
            assert!(generated);

            let passphrase =
                "A pretty long passphrase that should test the hashing to a 32-bit key quite well".to_string();
            let passphrase_str = CString::new(passphrase).unwrap();
            let passphrase_const_str: *const c_char = CString::into_raw(passphrase_str) as *const c_char;

            wallet_apply_encryption(alice_wallet, passphrase_const_str, error_ptr);
            assert_eq!(error, 0);

            comms_config_destroy(alice_config);
            wallet_destroy(alice_wallet);

            let alice_config = comms_config_create(
                address_alice_str,
                transport_type_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                error_ptr,
            );
            comms_config_set_secret_key(alice_config, secret_key_alice, error_ptr);

            // no passphrase
            let _alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                ptr::null(),
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                direct_send_callback,
                store_and_forward_send_callback,
                tx_cancellation_callback,
                base_node_sync_process_complete_callback,
                saf_messages_received_callback,
                error_ptr,
            );

            assert_eq!(error, 426);

            let wrong_passphrase = "wrong pf".to_string();
            let wrong_passphrase_str = CString::new(wrong_passphrase).unwrap();
            let wrong_passphrase_const_str: *const c_char = CString::into_raw(wrong_passphrase_str) as *const c_char;

            let _alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                wrong_passphrase_const_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                direct_send_callback,
                store_and_forward_send_callback,
                tx_cancellation_callback,
                base_node_sync_process_complete_callback,
                saf_messages_received_callback,
                error_ptr,
            );
            assert_eq!(error, 423);

            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                passphrase_const_str,
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                direct_send_callback,
                store_and_forward_send_callback,
                tx_cancellation_callback,
                base_node_sync_process_complete_callback,
                saf_messages_received_callback,
                error_ptr,
            );

            assert_eq!(error, 0);
            // Try a read of an encrypted value to check the wallet is using the ciphers
            let seed_words = wallet_get_seed_words(alice_wallet, error_ptr);
            assert_eq!(error, 0);

            wallet_remove_encryption(alice_wallet, error_ptr);
            assert_eq!(error, 0);

            comms_config_destroy(alice_config);
            wallet_destroy(alice_wallet);

            let alice_config = comms_config_create(
                address_alice_str,
                transport_type_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                error_ptr,
            );
            comms_config_set_secret_key(alice_config, secret_key_alice, error_ptr);
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                ptr::null(),
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                direct_send_callback,
                store_and_forward_send_callback,
                tx_cancellation_callback,
                base_node_sync_process_complete_callback,
                saf_messages_received_callback,
                error_ptr,
            );

            assert_eq!(error, 0);

            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            string_destroy(passphrase_const_str as *mut c_char);
            string_destroy(wrong_passphrase_const_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            public_key_destroy(public_key_alice);
            transport_type_destroy(transport_type_alice);

            comms_config_destroy(alice_config);
            seed_words_destroy(seed_words);
            wallet_destroy(alice_wallet);
        }
    }

    #[test]
    fn test_wallet_client_key_value_store() {
        unsafe {
            let mut error = 0;
            let error_ptr = &mut error as *mut c_int;

            let secret_key_alice = private_key_generate();
            let db_name_alice = CString::new(random_string(8).as_str()).unwrap();
            let db_name_alice_str: *const c_char = CString::into_raw(db_name_alice.clone()) as *const c_char;
            let alice_temp_dir = tempdir().unwrap();
            let db_path_alice = CString::new(alice_temp_dir.path().to_str().unwrap()).unwrap();
            let db_path_alice_str: *const c_char = CString::into_raw(db_path_alice.clone()) as *const c_char;
            let transport_type_alice = transport_memory_create();
            let address_alice = transport_memory_get_address(transport_type_alice, error_ptr);
            let address_alice_str = CStr::from_ptr(address_alice).to_str().unwrap().to_owned();
            let address_alice_str: *const c_char = CString::new(address_alice_str).unwrap().into_raw() as *const c_char;

            let alice_config = comms_config_create(
                address_alice_str,
                transport_type_alice,
                db_name_alice_str,
                db_path_alice_str,
                20,
                error_ptr,
            );
            comms_config_set_secret_key(alice_config, secret_key_alice, error_ptr);
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                ptr::null(),
                received_tx_callback,
                received_tx_reply_callback,
                received_tx_finalized_callback,
                broadcast_callback,
                mined_callback,
                direct_send_callback,
                store_and_forward_send_callback,
                tx_cancellation_callback,
                base_node_sync_process_complete_callback,
                saf_messages_received_callback,
                error_ptr,
            );

            let client_key_values = vec![
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string()),
                ("key3".to_string(), "value3".to_string()),
            ];

            for kv in client_key_values.iter() {
                let k = CString::new(kv.0.as_str()).unwrap();
                let k_str: *const c_char = CString::into_raw(k.clone()) as *const c_char;
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

            wallet_apply_encryption(alice_wallet, passphrase_const_str, error_ptr);
            assert_eq!(error, 0);

            for kv in client_key_values.iter() {
                let k = CString::new(kv.0.as_str()).unwrap();
                let k_str: *const c_char = CString::into_raw(k.clone()) as *const c_char;

                let found_value = wallet_get_value(alice_wallet, k_str, error_ptr);
                let found_string = CString::from_raw(found_value).to_str().unwrap().to_owned();
                assert_eq!(found_string, kv.1.clone());
                string_destroy(k_str as *mut c_char);
            }
            let wrong_key = CString::new("Wrong").unwrap();
            let wrong_key_str: *const c_char = CString::into_raw(wrong_key.clone()) as *const c_char;
            assert!(!wallet_clear_value(alice_wallet, wrong_key_str, error_ptr));
            string_destroy(wrong_key_str as *mut c_char);

            let k = CString::new(client_key_values[0].0.as_str()).unwrap();
            let k_str: *const c_char = CString::into_raw(k.clone()) as *const c_char;
            assert!(wallet_clear_value(alice_wallet, k_str, error_ptr));

            let found_value = wallet_get_value(alice_wallet, k_str, error_ptr);
            assert_eq!(found_value, ptr::null_mut());
            assert_eq!(*error_ptr, 424i32);

            string_destroy(k_str as *mut c_char);
            string_destroy(db_name_alice_str as *mut c_char);
            string_destroy(db_path_alice_str as *mut c_char);
            string_destroy(address_alice_str as *mut c_char);
            string_destroy(passphrase_const_str as *mut c_char);
            private_key_destroy(secret_key_alice);
            transport_type_destroy(transport_type_alice);

            comms_config_destroy(alice_config);
            wallet_destroy(alice_wallet);
        }
    }
}
