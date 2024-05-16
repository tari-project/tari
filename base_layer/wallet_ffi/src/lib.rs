// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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
//! ## Send Transaction
//! To send a transaction your wallet must have available funds and you must had added the recipient's Public Key as a
//! `Contact`.
//!
//! To send a transaction:
//! 1. Call the `send_transaction(dest_public_key, amount, fee_per_gram, message)` function which will result in a
//!    `PendingOutboundTransaction` being produced and transmitted to the recipient and the funds becoming encumbered
//!    and appearing in the `PendingOutgoingBalance` and any change will appear in the `PendingIncomingBalance`.
//! 2. Wait until the recipient replies to the sent transaction which will result in the `PendingOutboundTransaction`
//!    becoming a `CompletedTransaction` with the `Completed` status. This means that the transaction has been
//!    negotiated between the parties and is now ready to be broadcast to the Base Layer. The funds are still encumbered
//!    as pending because the transaction has not been mined yet.
//! 3. The finalized `CompletedTransaction` will be sent back to the receiver so that they have a copy.
//! 4. The wallet will broadcast the `CompletedTransaction` to a Base Node to be added to the mempool. Its status will
//!    move from `Completed` to `Broadcast`.
//! 5. Wait until the transaction is mined. The `CompleteTransaction` status will then move from `Broadcast` to `Mined`
//!    and the pending funds will be spent and received.
//!
//! ## Receive a Transaction
//! 1. When a transaction is received it will appear as an `InboundTransaction` and the amount to be received will
//!    appear as a `PendingIncomingBalance`. The wallet backend will be listening for these transactions and will
//!    immediately reply to the sending wallet.
//! 2. The sender will send back the finalized `CompletedTransaction`
//! 3. This wallet will also broadcast the `CompletedTransaction` to a Base Node to be added to the mempool, its status
//!    will move from `Completed` to `Broadcast`. This is done so that the Receiver can be sure the finalized
//!    transaction is broadcast.
//! 6. This wallet will then monitor the Base Layer to see when the transaction is mined which means the
//!    `CompletedTransaction` status will become `Mined` and the funds will then move from the `PendingIncomingBalance`
//!    to the `AvailableBalance`.

#![recursion_limit = "1024"]

use core::ptr;
use std::{
    boxed::Box,
    convert::{TryFrom, TryInto},
    ffi::{CStr, CString},
    fmt::{Display, Formatter},
    mem::ManuallyDrop,
    num::NonZeroU16,
    path::PathBuf,
    slice,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use chrono::{DateTime, Local};
use error::LibWalletError;
use ffi_basenode_state::TariBaseNodeState;
use itertools::Itertools;
use libc::{c_char, c_int, c_uchar, c_uint, c_ulonglong, c_ushort, c_void};
use log::{LevelFilter, *};
use log4rs::{
    append::{
        file::FileAppender,
        rolling_file::{
            policy::compound::{roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger, CompoundPolicy},
            RollingFileAppender,
        },
        Append,
    },
    config::{Appender, Config, Logger, Root},
    encode::pattern::PatternEncoder,
};
use minotari_wallet::{
    base_node_service::config::BaseNodeServiceConfig,
    connectivity_service::{WalletConnectivityHandle, WalletConnectivityInterface},
    error::{WalletError, WalletStorageError},
    output_manager_service::{
        error::OutputManagerError,
        storage::{
            database::{OutputBackendQuery, OutputManagerDatabase, SortDirection},
            models::DbWalletOutput,
            OutputStatus,
        },
        UtxoSelectionCriteria,
    },
    storage::{
        database::WalletDatabase,
        sqlite_db::wallet::WalletSqliteDatabase,
        sqlite_utilities::{get_last_network, get_last_version, initialize_sqlite_database_backends},
    },
    transaction_service::{
        config::TransactionServiceConfig,
        error::TransactionServiceError,
        storage::{
            database::TransactionDatabase,
            models::{CompletedTransaction, InboundTransaction, OutboundTransaction},
        },
    },
    utxo_scanner_service::{service::UtxoScannerService, RECOVERY_KEY},
    wallet::{derive_comms_secret_key, read_or_create_master_seed, WalletMessageSigningDomain},
    Wallet,
    WalletConfig,
    WalletSqlite,
};
use num_traits::FromPrimitive;
use rand::rngs::OsRng;
use tari_common::{
    configuration::{MultiaddrList, StringList},
    network_check::set_network_if_choice_valid,
};
use tari_common_types::{
    emoji::emoji_set,
    tari_address::{TariAddress, TariAddressError},
    transaction::{TransactionDirection, TransactionStatus, TxId},
    types::{ComAndPubSignature, Commitment, PublicKey, SignatureWithDomain},
    wallet_types::WalletType,
};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerQuery},
    transports::MemoryTransport,
    types::CommsPublicKey,
};
use tari_comms_dht::{
    store_forward::SafConfig,
    DbConnectionUrl,
    DhtConfig,
    DhtConnectivityConfig,
    NetworkDiscoveryConfig,
};
use tari_contacts::contacts_service::{handle::ContactsServiceHandle, types::Contact};
use tari_core::{
    borsh::FromBytes,
    consensus::ConsensusManager,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{OutputFeatures, OutputFeaturesVersion, OutputType, RangeProofType, UnblindedOutput},
        CryptoFactories,
    },
};
use tari_crypto::{
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    tari_utilities::{ByteArray, Hidden},
};
use tari_key_manager::{cipher_seed::CipherSeed, mnemonic::MnemonicLanguage, SeedWords};
use tari_p2p::{
    auto_update::AutoUpdateConfig,
    transport::MemoryTransportConfig,
    Network,
    PeerSeedsConfig,
    SocksAuthentication,
    TcpTransportConfig,
    TorControlAuthentication,
    TorTransportConfig,
    TransportConfig,
    TransportType,
    DEFAULT_DNS_NAME_SERVER,
};
use tari_script::TariScript;
use tari_shutdown::Shutdown;
use tari_utilities::{
    hex,
    hex::{Hex, HexError},
    SafePassword,
};
use tokio::runtime::Runtime;
use zeroize::Zeroize;

use crate::{
    callback_handler::CallbackHandler,
    enums::SeedWordPushResult,
    error::{InterfaceError, TransactionError},
    tasks::recovery_event_monitoring,
};

mod callback_handler;
#[cfg(test)]
mod callback_handler_tests;
mod enums;
mod error;
mod ffi_basenode_state;
#[cfg(test)]
mod output_manager_service_mock;
mod tasks;

mod consts {
    // Import the auto-generated const values from the Manifest and Git
    include!(concat!(env!("OUT_DIR"), "/consts.rs"));
}

const LOG_TARGET: &str = "wallet_ffi";

pub type TariTransportConfig = tari_p2p::TransportConfig;
pub type TariPublicKey = tari_common_types::types::PublicKey;
pub type TariWalletAddress = tari_common_types::tari_address::TariAddress;
pub type TariNodeId = tari_comms::peer_manager::NodeId;
pub type TariPrivateKey = tari_common_types::types::PrivateKey;
pub type TariOutputFeatures = tari_core::transactions::transaction_components::OutputFeatures;
pub type TariCommsConfig = tari_p2p::P2pConfig;
pub type TariTransactionKernel = tari_core::transactions::transaction_components::TransactionKernel;
pub type TariCovenant = tari_core::covenants::Covenant;
pub type TariEncryptedOpenings = tari_core::transactions::transaction_components::EncryptedData;
pub type TariComAndPubSignature = tari_common_types::types::ComAndPubSignature;
pub type TariUnblindedOutput = tari_core::transactions::transaction_components::UnblindedOutput;

pub struct TariUnblindedOutputs(Vec<UnblindedOutput>);

pub struct TariContacts(Vec<TariContact>);

pub type TariContact = tari_contacts::contacts_service::types::Contact;
pub type TariCompletedTransaction = minotari_wallet::transaction_service::storage::models::CompletedTransaction;
pub type TariTransactionSendStatus = minotari_wallet::transaction_service::handle::TransactionSendStatus;
pub type TariFeePerGramStats = minotari_wallet::transaction_service::handle::FeePerGramStatsResponse;
pub type TariFeePerGramStat = tari_core::mempool::FeePerGramStat;
pub type TariContactsLivenessData = tari_contacts::contacts_service::handle::ContactsLivenessData;
pub type TariBalance = minotari_wallet::output_manager_service::service::Balance;
pub type TariMnemonicLanguage = tari_key_manager::mnemonic::MnemonicLanguage;

pub struct TariCompletedTransactions(Vec<TariCompletedTransaction>);

pub type TariPendingInboundTransaction = minotari_wallet::transaction_service::storage::models::InboundTransaction;
pub type TariPendingOutboundTransaction = minotari_wallet::transaction_service::storage::models::OutboundTransaction;

pub struct TariPendingInboundTransactions(Vec<TariPendingInboundTransaction>);

pub struct TariPendingOutboundTransactions(Vec<TariPendingOutboundTransaction>);

#[derive(Debug, PartialEq, Clone)]
pub struct ByteVector(Vec<c_uchar>); // declared like this so that it can be exposed to external header

#[derive(Debug, PartialEq)]
pub struct EmojiSet(Vec<ByteVector>);

#[derive(Debug, PartialEq)]
pub struct TariSeedWords(SeedWords);

#[derive(Debug, PartialEq)]
pub struct TariPublicKeys(Vec<TariPublicKey>);

pub struct TariWallet {
    wallet: WalletSqlite,
    runtime: Runtime,
    shutdown: Shutdown,
}

#[derive(Debug)]
#[repr(C)]
pub struct TariCoinPreview {
    pub expected_outputs: *mut TariVector,
    pub fee: u64,
}

#[derive(Debug)]
#[repr(C)]
pub enum TariUtxoSort {
    ValueAsc = 0,
    ValueDesc = 1,
    MinedHeightAsc = 2,
    MinedHeightDesc = 3,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub enum TariTypeTag {
    Text = 0,
    Utxo = 1,
    Commitment = 2,
    U64 = 3,
    I64 = 4,
}

impl Display for TariTypeTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TariTypeTag::Text => write!(f, "Text"),
            TariTypeTag::Utxo => write!(f, "Utxo"),
            TariTypeTag::Commitment => write!(f, "Commitment"),
            TariTypeTag::U64 => write!(f, "U64"),
            TariTypeTag::I64 => write!(f, "I64"),
        }
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct TariUtxo {
    pub commitment: *const c_char,
    pub value: u64,
    pub mined_height: u64,
    pub mined_timestamp: u64,
    pub lock_height: u64,
    pub status: u8,
    pub coinbase_extra: *const c_char,
}

impl From<DbWalletOutput> for TariUtxo {
    fn from(x: DbWalletOutput) -> Self {
        Self {
            commitment: CString::new(x.commitment.to_hex())
                .expect("failed to obtain hex from a commitment")
                .into_raw(),
            value: x.wallet_output.value.as_u64(),
            mined_height: x.mined_height.unwrap_or(0),
            lock_height: x.wallet_output.features.maturity,
            mined_timestamp: x
                .mined_timestamp
                .map(|ts| ts.timestamp_millis() as u64)
                .unwrap_or_default(),
            status: match x.status {
                OutputStatus::Unspent => 0,
                OutputStatus::Spent => 1,
                OutputStatus::EncumberedToBeReceived => 2,
                OutputStatus::EncumberedToBeSpent => 3,
                OutputStatus::Invalid => 4,
                OutputStatus::CancelledInbound => 5,
                OutputStatus::UnspentMinedUnconfirmed => 6,
                OutputStatus::ShortTermEncumberedToBeReceived => 7,
                OutputStatus::ShortTermEncumberedToBeSpent => 8,
                OutputStatus::SpentMinedUnconfirmed => 9,
                OutputStatus::NotStored => 10,
            },
            coinbase_extra: CString::new(x.wallet_output.features.coinbase_extra.to_hex())
                .expect("failed to obtain hex from a commitment")
                .into_raw(),
        }
    }
}

/// -------------------------------- Vector ------------------------------------------------ ///

#[derive(Debug, Clone)]
#[repr(C)]
pub struct TariVector {
    pub tag: TariTypeTag,
    pub len: usize,
    pub cap: usize,
    pub ptr: *mut c_void,
}

impl From<Vec<i64>> for TariVector {
    fn from(v: Vec<i64>) -> Self {
        let mut v = ManuallyDrop::new(v);

        Self {
            tag: TariTypeTag::I64,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<u64>> for TariVector {
    fn from(v: Vec<u64>) -> Self {
        let mut v = ManuallyDrop::new(v);

        Self {
            tag: TariTypeTag::U64,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<String>> for TariVector {
    fn from(v: Vec<String>) -> Self {
        let mut v = ManuallyDrop::new(
            v.into_iter()
                .map(|x| CString::new(x.as_str()).unwrap().into_raw())
                .collect::<Vec<*mut c_char>>(),
        );

        Self {
            tag: TariTypeTag::Text,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<Commitment>> for TariVector {
    fn from(v: Vec<Commitment>) -> Self {
        let mut v = ManuallyDrop::new(
            v.into_iter()
                .map(|x| CString::new(x.to_hex().as_str()).unwrap().into_raw())
                .collect::<Vec<*mut c_char>>(),
        );

        Self {
            tag: TariTypeTag::Commitment,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<DbWalletOutput>> for TariVector {
    fn from(v: Vec<DbWalletOutput>) -> TariVector {
        let mut v = ManuallyDrop::new(v.into_iter().map(TariUtxo::from).collect_vec());

        Self {
            tag: TariTypeTag::Utxo,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<OutputStatus>> for TariVector {
    fn from(v: Vec<OutputStatus>) -> TariVector {
        let mut v = ManuallyDrop::new(v.into_iter().map(|x| x as i32 as u64).collect_vec());

        Self {
            tag: TariTypeTag::U64,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

#[allow(dead_code)]
impl TariVector {
    fn to_string_vec(&self) -> Result<Vec<String>, InterfaceError> {
        if self.tag != TariTypeTag::Text {
            return Err(InterfaceError::InvalidArgument(format!(
                "expecting String, got {}",
                self.tag
            )));
        }

        if self.ptr.is_null() {
            return Err(InterfaceError::NullError(String::from(
                "tari vector of strings has null pointer",
            )));
        }

        Ok(unsafe {
            Vec::from_raw_parts(self.ptr as *mut *mut c_char, self.len, self.cap)
                .into_iter()
                .map(|x| {
                    CStr::from_ptr(x)
                        .to_str()
                        .expect("failed to convert from a vector of strings")
                        .to_string()
                })
                .collect()
        })
    }

    fn to_commitment_vec(&self) -> Result<Vec<Commitment>, InterfaceError> {
        self.to_string_vec()?
            .into_iter()
            .map(|x| {
                Commitment::from_hex(x.as_str())
                    .map_err(|e| InterfaceError::PointerError(format!("failed to convert hex to commitment: {:?}", e)))
            })
            .try_collect::<Commitment, Vec<Commitment>, InterfaceError>()
    }

    #[allow(dead_code)]
    fn to_utxo_vec(&self) -> Result<Vec<TariUtxo>, InterfaceError> {
        if self.tag != TariTypeTag::Utxo {
            return Err(InterfaceError::InvalidArgument(format!(
                "expecting Utxo, got {}",
                self.tag
            )));
        }

        if self.ptr.is_null() {
            return Err(InterfaceError::NullError(String::from(
                "tari vector of utxos has null pointer",
            )));
        }

        Ok(unsafe { Vec::from_raw_parts(self.ptr as *mut TariUtxo, self.len, self.cap) })
    }
}

/// Initialize a new `TariVector`
///
/// ## Arguments
/// `tag` - A predefined type-tag of the vector's payload.
///
/// ## Returns
/// `*mut TariVector` - Returns a pointer to a `TariVector`.
///
/// # Safety
/// `destroy_tari_vector()` must be called to free the allocated memory.
#[no_mangle]
pub unsafe extern "C" fn create_tari_vector(tag: TariTypeTag) -> *mut TariVector {
    let mut v = ManuallyDrop::new(Vec::with_capacity(2));
    Box::into_raw(Box::new(TariVector {
        tag,
        len: v.len(),
        cap: v.capacity(),
        ptr: v.as_mut_ptr(),
    }))
}

/// Appending a given value to the back of the vector.
///
/// ## Arguments
/// `s` - An item to push.
///
/// ## Returns
///
///
/// # Safety
/// `destroy_tari_vector()` must be called to free the allocated memory.
#[no_mangle]
pub unsafe extern "C" fn tari_vector_push_string(tv: *mut TariVector, s: *const c_char, error_ptr: *mut i32) {
    if tv.is_null() {
        error!(target: LOG_TARGET, "tari vector pointer is null");
        ptr::replace(
            error_ptr,
            LibWalletError::from(InterfaceError::NullError("vector".to_string())).code,
        );
        return;
    }

    // unpacking into native vector
    let mut v = match (*tv).to_string_vec() {
        Ok(v) => v,
        Err(e) => {
            error!(target: LOG_TARGET, "{:#?}", e);
            ptr::replace(error_ptr, LibWalletError::from(e).code);
            return;
        },
    };

    let s = match CStr::from_ptr(s).to_str() {
        Ok(cs) => cs.to_string(),
        Err(e) => {
            error!(target: LOG_TARGET, "failed to convert `s` into native string {:#?}", e);
            ptr::replace(
                error_ptr,
                LibWalletError::from(InterfaceError::PointerError("invalid string".to_string())).code,
            );
            return;
        },
    };

    // appending new value
    // NOTE: relying on native vector's re-allocation
    v.push(s);

    let mut v = ManuallyDrop::new(
        v.into_iter()
            .map(|x| CString::new(x.as_str()).unwrap().into_raw())
            .collect::<Vec<*mut c_char>>(),
    );

    (*tv).len = v.len();
    (*tv).cap = v.capacity();
    (*tv).ptr = v.as_mut_ptr() as *mut c_void;
    ptr::replace(error_ptr, 0);
}

/// Frees memory allocated for `TariVector`.
///
/// ## Arguments
/// `v` - The pointer to `TariVector`
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_tari_vector(v: *mut TariVector) {
    if !v.is_null() {
        let x = Box::from_raw(v);
        let _ = x.ptr;
    }
}

/// Frees memory allocated for `TariCoinPreview`.
///
/// ## Arguments
/// `v` - The pointer to `TariCoinPreview`
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_tari_coin_preview(p: *mut TariCoinPreview) {
    if !p.is_null() {
        let x = Box::from_raw(p);
        destroy_tari_vector(x.expected_outputs);
    }
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
        let _string = CString::from_raw(ptr);
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- Transaction Kernel ------------------------------------- ///

/// Gets the excess for a TariTransactionKernel
///
/// ## Arguments
/// `x` - The pointer to a  TariTransactionKernel
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns empty if there
/// was an error
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn transaction_kernel_get_excess_hex(
    kernel: *mut TariTransactionKernel,
    error_out: *mut c_int,
) -> *mut c_char {
    let mut error = 0;
    let mut result = CString::new("").expect("Blank CString will not fail.");
    ptr::swap(error_out, &mut error as *mut c_int);
    if kernel.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("kernel".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return CString::into_raw(result);
    }
    let excess = (*kernel).excess.clone().to_hex();
    match CString::new(excess) {
        Ok(v) => result = v,
        _ => {
            error = LibWalletError::from(InterfaceError::PointerError("kernel".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    result.into_raw()
}

/// Gets the public nonce for a TariTransactionKernel
///
/// ## Arguments
/// `x` - The pointer to a  TariTransactionKernel
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns empty if there
/// was an error
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn transaction_kernel_get_excess_public_nonce_hex(
    kernel: *mut TariTransactionKernel,
    error_out: *mut c_int,
) -> *mut c_char {
    let mut error = 0;
    let mut result = CString::new("").expect("Blank CString will not fail.");
    ptr::swap(error_out, &mut error as *mut c_int);
    if kernel.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("kernel".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return CString::into_raw(result);
    }
    let nonce = (*kernel).excess_sig.get_public_nonce().to_hex();

    match CString::new(nonce) {
        Ok(v) => result = v,
        _ => {
            error = LibWalletError::from(InterfaceError::PointerError("kernel".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

    result.into_raw()
}

/// Gets the signature for a TariTransactionKernel
///
/// ## Arguments
/// `x` - The pointer to a TariTransactionKernel
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns empty if there
/// was an error
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn transaction_kernel_get_excess_signature_hex(
    kernel: *mut TariTransactionKernel,
    error_out: *mut c_int,
) -> *mut c_char {
    let mut error = 0;
    let mut result = CString::new("").expect("Blank CString will not fail.");
    ptr::swap(error_out, &mut error as *mut c_int);
    if kernel.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("kernel".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return CString::into_raw(result);
    }
    let signature = (*kernel).excess_sig.get_signature().to_hex();
    result = CString::new(signature).expect("Hex string will not fail");
    result.into_raw()
}

/// Frees memory for a TariTransactionKernel
///
/// ## Arguments
/// `x` - The pointer to a  TariTransactionKernel
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn transaction_kernel_destroy(x: *mut TariTransactionKernel) {
    if !x.is_null() {
        drop(Box::from_raw(x))
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
) -> *mut ByteVector {
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
        drop(Box::from_raw(bytes))
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
// converting between here is fine as its used to clamp the array to length
#[allow(clippy::cast_possible_wrap)]
#[no_mangle]
pub unsafe extern "C" fn byte_vector_get_at(ptr: *mut ByteVector, position: c_uint, error_out: *mut c_int) -> c_uchar {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if ptr.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("ptr".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0u8;
    }
    let len = byte_vector_get_length(ptr, error_out) as c_int - 1; // clamp to length
    if len < 0 || position > len as c_uint {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0u8;
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
// casting here is okay a byte vector wont go larger than u32
#[allow(clippy::cast_possible_truncation)]
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
    if bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let v = (*bytes).0.clone();
    let pk = TariPublicKey::from_canonical_bytes(&v);
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
        drop(Box::from_raw(pk))
    }
}

/// Frees memory for TariPublicKeys
///
/// ## Arguments
/// `pks` - The pointer to TariPublicKeys
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn public_keys_destroy(pks: *mut TariPublicKeys) {
    if !pks.is_null() {
        drop(Box::from_raw(pks))
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
) -> *mut TariPublicKey {
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
        match CStr::from_ptr(key).to_str() {
            Ok(v) => {
                key_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("key".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
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

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Tari Address ---------------------------------------------- ///

/// Creates a TariWalletAddress from a ByteVector
///
/// ## Arguments
/// `bytes` - The pointer to a ByteVector
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `TariWalletAddress` - Returns a public key. Note that it will be ptr::null_mut() if bytes is null or
/// if there was an error with the contents of bytes
///
/// # Safety
/// The ```public_key_destroy``` function must be called when finished with a TariWalletAddress to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn tari_address_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let v = (*bytes).0.clone();
    let address = TariWalletAddress::from_bytes(&v);
    match address {
        Ok(address) => Box::into_raw(Box::new(address)),
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Frees memory for a TariWalletAddress
///
/// ## Arguments
/// `pk` - The pointer to a TariWalletAddress
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn tari_address_destroy(address: *mut TariWalletAddress) {
    if !address.is_null() {
        drop(Box::from_raw(address))
    }
}

/// Gets a ByteVector from a TariWalletAddress
///
/// ## Arguments
/// `address` - The pointer to a TariWalletAddress
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut ByteVector` - Returns a pointer to a ByteVector. Note that it returns ptr::null_mut() if address is null
///
/// # Safety
/// The ```byte_vector_destroy``` function must be called when finished with the ByteVector to prevent a memory leak.
#[no_mangle]
pub unsafe extern "C" fn tari_address_get_bytes(
    address: *mut TariWalletAddress,
    error_out: *mut c_int,
) -> *mut ByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut bytes = ByteVector(Vec::new());
    if address.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        bytes.0 = (*address).to_bytes().to_vec();
    }
    Box::into_raw(Box::new(bytes))
}

/// Creates a TariWalletAddress from a TariPrivateKey
///
/// ## Arguments
/// `secret_key` - The pointer to a TariPrivateKey
/// `network` - an u8 indicating the network
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariWalletAddress` - Returns a pointer to a TariWalletAddress
///
/// # Safety
/// The ```private_key_destroy``` method must be called when finished with a private key to prevent a memory leak
// casting here is network is a u8
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn tari_address_from_private_key(
    secret_key: *mut TariPrivateKey,
    network: c_uint,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if secret_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("secret_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let key = PublicKey::from_secret_key(&(*secret_key));
    let network = match (network as u8).try_into() {
        Ok(network) => network,
        Err(_) => {
            error = LibWalletError::from(InterfaceError::InvalidArgument("network".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };
    let address = TariWalletAddress::new(key, network);
    Box::into_raw(Box::new(address))
}

/// Creates a TariWalletAddress from a char array
///
/// ## Arguments
/// `address` - The pointer to a char array which is hex encoded
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariWalletAddress` - Returns a pointer to a TariWalletAddress. Note that it returns ptr::null_mut()
/// if key is null or if there was an error creating the TariWalletAddress from key
///
/// # Safety
/// The ```public_key_destroy``` method must be called when finished with a TariWalletAddress to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn tari_address_from_hex(
    address: *const c_char,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let key_str;
    if address.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(address).to_str() {
            Ok(v) => {
                key_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("key".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }

    let address = TariWalletAddress::from_hex(key_str.as_str());
    match address {
        Ok(address) => Box::into_raw(Box::new(address)),
        Err(e) => {
            error!(target: LOG_TARGET, "Error creating a Tari Address from Hex: {:?}", e);
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Creates a char array from a TariWalletAddress in emoji format
///
/// ## Arguments
/// `address` - The pointer to a TariWalletAddress
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns empty
/// if emoji is null or if there was an error creating the emoji string from TariWalletAddress
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn tari_address_to_emoji_id(
    address: *mut TariWalletAddress,
    error_out: *mut c_int,
) -> *mut c_char {
    let mut error = 0;
    let mut result = CString::new("").expect("Blank CString will not fail.");
    ptr::swap(error_out, &mut error as *mut c_int);
    if address.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return CString::into_raw(result);
    }
    let emoji_string = address.as_ref().expect("Address should not be empty").to_emoji_string();
    result = CString::new(emoji_string).expect("Emoji will not fail.");
    CString::into_raw(result)
}

/// Creates a char array from a TariWalletAddress's network
///
/// ## Arguments
/// `address` - The pointer to a TariWalletAddress
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns empty
/// if there was an error from TariWalletAddress
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn tari_address_network(address: *mut TariWalletAddress, error_out: *mut c_int) -> *mut c_char {
    let mut error = 0;
    let mut result = CString::new("").expect("Blank CString will not fail.");
    ptr::swap(error_out, &mut error as *mut c_int);
    if address.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return CString::into_raw(result);
    }
    let network_string = address
        .as_ref()
        .expect("Address should not be empty")
        .network()
        .to_string();
    result = CString::new(network_string).expect("string will not fail.");
    CString::into_raw(result)
}

/// Creates a TariWalletAddress from a char array in emoji format
///
/// ## Arguments
/// `const *c_char` - The pointer to a TariWalletAddress
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a TariWalletAddress. Note that it returns null on error.
///
/// # Safety
/// The ```public_key_destroy``` method must be called when finished with a TariWalletAddress to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn emoji_id_to_tari_address(
    emoji: *const c_char,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if emoji.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("emoji".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    match CStr::from_ptr(emoji)
        .to_str()
        .map_err(|_| TariAddressError::InvalidEmoji)
        .and_then(TariAddress::from_emoji_string)
    {
        Ok(address) => Box::into_raw(Box::new(address)),
        Err(_) => {
            error = LibWalletError::from(InterfaceError::InvalidEmojiId).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// -------------------------------------------------------------------------------------------- ///
///
/// ------------------------------- ComAndPubSignature Signature ---------------------------------------///

/// Creates a TariComAndPubSignature from `u_a`. `u_x`, `u_y`, `ephemeral_pubkey` and `ephemeral_commitment_bytes`
/// ByteVectors
///
/// ## Arguments
/// `ephemeral_commitment_bytes` - The public ephemeral commitment component as a ByteVector
/// `ephemeral_pubkey_bytes` - The public ephemeral pubkey component as a ByteVector
/// `u_a_bytes` - The u_a signature component as a ByteVector
/// `u_x_bytes` - The u_x signature component as a ByteVector
/// `u_y_bytes` - The u_y signature component as a ByteVector
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `TariComAndPubSignature` - Returns a ComAndPubS signature. Note that it will be ptr::null_mut() if any argument is
/// null or if there was an error with the contents of bytes
///
/// # Safety
/// The ```commitment_signature_destroy``` function must be called when finished with a TariComAndPubSignature to
/// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn commitment_and_public_signature_create_from_bytes(
    ephemeral_commitment_bytes: *const ByteVector,
    ephemeral_pubkey_bytes: *const ByteVector,
    u_a_bytes: *const ByteVector,
    u_x_bytes: *const ByteVector,
    u_y_bytes: *const ByteVector,
    error_out: *mut c_int,
) -> *mut TariComAndPubSignature {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if ephemeral_pubkey_bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("ephemeral_pubkey_bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    if ephemeral_commitment_bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("ephemeral_commitment_bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    if u_a_bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("u_a_bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    if u_x_bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("u_x_bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    if u_y_bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("u_y_bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let ephemeral_commitment = match Commitment::from_canonical_bytes(&(*ephemeral_commitment_bytes).0.clone()) {
        Ok(ephemeral_commitment) => ephemeral_commitment,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Error creating a ephemeral commitment from bytes: {:?}", e
            );
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };
    let ephemeral_pubkey = match PublicKey::from_canonical_bytes(&(*ephemeral_pubkey_bytes).0.clone()) {
        Ok(ephemeral_pubkey) => ephemeral_pubkey,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Error creating a ephemeral pubkey from bytes: {:?}", e
            );
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let u_a = match TariPrivateKey::from_canonical_bytes(&(*u_a_bytes).0.clone()) {
        Ok(u) => u,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Error creating a Private Key (u_a) from bytes: {:?}", e
            );
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };
    let u_x = match TariPrivateKey::from_canonical_bytes(&(*u_x_bytes).0.clone()) {
        Ok(u) => u,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Error creating a Private Key (u_x) from bytes: {:?}", e
            );
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };
    let u_y = match TariPrivateKey::from_canonical_bytes(&(*u_y_bytes).0.clone()) {
        Ok(u) => u,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Error creating a Private Key (u_y) from bytes: {:?}", e
            );
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let sig = ComAndPubSignature::new(ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y);
    Box::into_raw(Box::new(sig))
}

/// Frees memory for a TariComAndPubSignature
///
/// ## Arguments
/// `compub_sig` - The pointer to a TariComAndPubSignature
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn commitment_and_public_signature_destroy(compub_sig: *mut TariComAndPubSignature) {
    if !compub_sig.is_null() {
        drop(Box::from_raw(compub_sig))
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Unblinded utxo -------------------------------------------- ///

/// Creates an unblinded output
///
/// ## Arguments
/// `amount` - The value of the UTXO in MicroMinotari
/// `spending_key` - The private spending key
/// `source_address` - The tari address of the source of the transaction
/// `features` - Options for an output's structure or use
/// `metadata_signature` - UTXO signature with the script offset private key, k_O
/// `sender_offset_public_key` - Tari script offset pubkey, K_O
/// `script_private_key` - Tari script private key, k_S, is used to create the script signature
/// `covenant` - The covenant that will be executed when spending this output
/// `message` - The message that the transaction will have
/// `encrypted_data` - Encrypted data.
/// `minimum_value_promise` - The minimum value of the commitment that is proven by the range proof
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// TariUnblindedOutput -  Returns the TransactionID of the generated transaction, note that it will be zero if the
/// transaction is null
///
/// # Safety
///  The ```tari_unblinded_output_destroy``` function must be called when finished with a TariUnblindedOutput to
/// prevent a memory leak
#[no_mangle]
#[allow(clippy::too_many_lines)]
pub unsafe extern "C" fn create_tari_unblinded_output(
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
) -> *mut TariUnblindedOutput {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if spending_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("spending_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    if metadata_signature.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("metadata_signature".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    if sender_offset_public_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("sender_offset_public_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    if script_private_key.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("script_private_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    if features.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("features".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let script_str;
    if script.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("script".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(script).to_str() {
            Ok(v) => {
                script_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("key".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }
    let script = match TariScript::from_hex(&script_str) {
        Ok(v) => v,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };
    let input_str;
    if input_data.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("input_data".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(input_data).to_str() {
            Ok(v) => {
                input_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("key".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }
    let input_data = match tari_script::ExecutionStack::from_hex(&input_str) {
        Ok(v) => v,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let covenant = if covenant.is_null() {
        TariCovenant::default()
    } else {
        (*covenant).clone()
    };

    let encrypted_data = if encrypted_data.is_null() {
        TariEncryptedOpenings::default()
    } else {
        *encrypted_data
    };

    let unblinded_output = UnblindedOutput::new_current_version(
        amount.into(),
        (*spending_key).clone(),
        (*features).clone(),
        script,
        input_data,
        (*script_private_key).clone(),
        (*sender_offset_public_key).clone(),
        (*metadata_signature).clone(),
        script_lock_height,
        covenant,
        encrypted_data,
        minimum_value_promise.into(),
    );

    Box::into_raw(Box::new(unblinded_output))
}

/// Frees memory for a TariUnblindedOutput
///
/// ## Arguments
/// `output` - The pointer to a TariUnblindedOutput
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn tari_unblinded_output_destroy(output: *mut TariUnblindedOutput) {
    if !output.is_null() {
        drop(Box::from_raw(output))
    }
}

/// returns the TariUnblindedOutput as a json string
///
/// ## Arguments
/// `output` - The pointer to a TariUnblindedOutput
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns an empty char array if
/// TariUnblindedOutput is null or the position is invalid
///
/// # Safety
///  The ```tari_unblinded_output_destroy``` function must be called when finished with a TariUnblindedOutput to
/// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn tari_unblinded_output_to_json(
    output: *mut TariUnblindedOutput,
    error_out: *mut c_int,
) -> *mut c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut hex_bytes = CString::new("").expect("Blank CString will not fail.");
    if output.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("output".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        match serde_json::to_string(&*output) {
            Ok(json_string) => match CString::new(json_string) {
                Ok(v) => hex_bytes = v,
                _ => {
                    error = LibWalletError::from(InterfaceError::PointerError("output".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                },
            },
            Err(_) => {
                error = LibWalletError::from(HexError::HexConversionError {}).code;
                ptr::swap(error_out, &mut error as *mut c_int);
            },
        }
    }
    CString::into_raw(hex_bytes)
}

/// Creates a TariUnblindedOutput from a char array
///
/// ## Arguments
/// `output_json` - The pointer to a char array which is json of the TariUnblindedOutput
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariUnblindedOutput` - Returns a pointer to a TariUnblindedOutput. Note that it returns ptr::null_mut()
/// if key is null or if there was an error creating the TariUnblindedOutput from key
///
/// # Safety
/// The ```tari_unblinded_output_destroy``` function must be called when finished with a TariUnblindedOutput to
// /// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn create_tari_unblinded_output_from_json(
    output_json: *const c_char,
    error_out: *mut c_int,
) -> *mut TariUnblindedOutput {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let output_json_str;
    if output_json.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("output_json".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(output_json).to_str() {
            Ok(v) => {
                output_json_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("output_json".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        };
    }
    let output: Result<TariUnblindedOutput, _> = serde_json::from_str(&output_json_str);

    match output {
        Ok(output) => Box::into_raw(Box::new(output)),
        Err(e) => {
            error!(target: LOG_TARGET, "Error creating a output from json: {:?}", e);

            error = LibWalletError::from(HexError::HexConversionError {}).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- TariUnblindedOutputs ------------------------------------///

/// Gets the length of TariUnblindedOutputs
///
/// ## Arguments
/// `outputs` - The pointer to a TariUnblindedOutputs
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns number of elements in , zero if outputs is null
///
/// # Safety
/// None
// casting here is okay the length of the array wont go over u32
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn unblinded_outputs_get_length(
    outputs: *mut TariUnblindedOutputs,
    error_out: *mut c_int,
) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut len = 0;
    if outputs.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("outputs".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        len = (*outputs).0.len();
    }
    len as c_uint
}

/// Gets a TariUnblindedOutput from TariUnblindedOutputs at position
///
/// ## Arguments
/// `outputs` - The pointer to a TariUnblindedOutputs
/// `position` - The integer position
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariUnblindedOutput` - Returns a TariUnblindedOutput, note that it returns ptr::null_mut() if
/// TariUnblindedOutputs is null or position is invalid
///
/// # Safety
/// The ```contact_destroy``` method must be called when finished with a TariContact to prevent a memory leak
// converting between here is fine as its used to clamp the array to length
#[allow(clippy::cast_possible_wrap)]
#[no_mangle]
pub unsafe extern "C" fn unblinded_outputs_get_at(
    outputs: *mut TariUnblindedOutputs,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariUnblindedOutput {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if outputs.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("outputs".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let len = unblinded_outputs_get_length(outputs, error_out) as c_int - 1;
    if len < 0 || position > len as c_uint {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    Box::into_raw(Box::new((*outputs).0[position as usize].clone()))
}

/// Frees memory for a TariUnblindedOutputs
///
/// ## Arguments
/// `outputs` - The pointer to a TariUnblindedOutputs
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn unblinded_outputs_destroy(outputs: *mut TariUnblindedOutputs) {
    if !outputs.is_null() {
        drop(Box::from_raw(outputs))
    }
}

/// Import an external UTXO into the wallet as a non-rewindable (i.e. non-recoverable) output. This will add a spendable
/// UTXO (as EncumberedToBeReceived) and create a faux completed transaction to record the event.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `amount` - The value of the UTXO in MicroMinotari
/// `spending_key` - The private spending key
/// `source_address` - The tari address of the source of the transaction
/// `features` - Options for an output's structure or use
/// `metadata_signature` - UTXO signature with the script offset private key, k_O
/// `sender_offset_public_key` - Tari script offset pubkey, K_O
/// `script_private_key` - Tari script private key, k_S, is used to create the script signature
/// `covenant` - The covenant that will be executed when spending this output
/// `message` - The message that the transaction will have
/// `encrypted_data` - Encrypted data.
/// `minimum_value_promise` - The minimum value of the commitment that is proven by the range proof
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` -  Returns the TransactionID of the generated transaction, note that it will be zero if the
/// transaction is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_import_external_utxo_as_non_rewindable(
    wallet: *mut TariWallet,
    output: *mut TariUnblindedOutput,
    source_address: *mut TariWalletAddress,
    message: *const c_char,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    let source_address = if source_address.is_null() {
        TariWalletAddress::default()
    } else {
        (*source_address).clone()
    };
    let message_string;
    if message.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        message_string = CString::new("Imported UTXO")
            .expect("CString will not fail")
            .to_str()
            .expect("CString.toStr() will not fail")
            .to_owned();
    } else {
        match CStr::from_ptr(message).to_str() {
            Ok(v) => {
                message_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("message".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                message_string = CString::new("Imported UTXO")
                    .expect("CString will not fail")
                    .to_str()
                    .expect("CString.to_str() will not fail")
                    .to_owned();
            },
        }
    };
    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.import_unblinded_output_as_non_rewindable(
            (*output).clone(),
            source_address,
            message_string,
        )) {
        Ok(tx_id) => tx_id.as_u64(),
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Get the TariUnblindedOutputs from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariUnblindedOutputs` - returns the unspent unblinded outputs, note that it returns ptr::null_mut() if
/// wallet is null
///
/// # Safety
/// The ```unblinded_outputs_destroy``` method must be called when finished with a TariUnblindedOutput to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_unspent_outputs(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> *mut TariUnblindedOutputs {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut outputs = Vec::new();
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let received_outputs = (*wallet)
        .runtime
        .block_on((*wallet).wallet.output_manager_service.get_unspent_outputs());
    match received_outputs {
        Ok(rec_outputs) => {
            for output in rec_outputs {
                let unblinded = (*wallet).runtime.block_on(UnblindedOutput::from_wallet_output(
                    output.wallet_output,
                    &(*wallet).wallet.key_manager_service,
                ));
                match unblinded {
                    Ok(uo) => {
                        outputs.push(uo);
                    },
                    Err(e) => {
                        error = LibWalletError::from(WalletError::TransactionError(e)).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        return ptr::null_mut();
                    },
                }
            }
            Box::into_raw(Box::new(TariUnblindedOutputs(outputs)))
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::OutputManagerError(e)).code;
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
/// `*mut TariPrivateKey` - Returns a pointer to a TariPrivateKey. Note that it returns ptr::null_mut()
/// if bytes is null or if there was an error creating the TariPrivateKey from bytes
///
/// # Safety
/// The ```private_key_destroy``` method must be called when finished with a TariPrivateKey to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn private_key_create(bytes: *mut ByteVector, error_out: *mut c_int) -> *mut TariPrivateKey {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let v = (*bytes).0.clone();
    let pk = TariPrivateKey::from_canonical_bytes(&v);
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
        drop(Box::from_raw(pk))
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
/// `*mut TariPrivateKey` - Returns a pointer to a TariPrivateKey. Note that it returns ptr::null_mut()
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
        match CStr::from_ptr(key).to_str() {
            Ok(v) => {
                key_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("key".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        };
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
/// --------------------------------------- Covenant --------------------------------------------///

/// Creates a TariCovenant from a ByteVector containing the covenant bytes
///
/// ## Arguments
/// `covenant_bytes` - The covenant bytes as a ByteVector
///
/// ## Returns
/// `TariCovenant` - Returns a commitment signature. Note that it will be ptr::null_mut() if any argument is
/// null or if there was an error with the contents of bytes
///
/// # Safety
/// The ```covenant_destroy``` function must be called when finished with a TariCovenant to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn covenant_create_from_bytes(
    covenant_bytes: *const ByteVector,
    error_out: *mut c_int,
) -> *mut TariCovenant {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if covenant_bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("covenant_bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let mut decoded_covenant_bytes = (*covenant_bytes).0.as_bytes();

    match TariCovenant::borsh_from_bytes(&mut decoded_covenant_bytes) {
        Ok(covenant) => Box::into_raw(Box::new(covenant)),
        Err(e) => {
            error!(target: LOG_TARGET, "Error creating a Covenant: {:?}", e);
            error = LibWalletError::from(InterfaceError::InvalidArgument("covenant_bytes".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Frees memory for a TariCovenant
///
/// ## Arguments
/// `covenant` - The pointer to a TariCovenant
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn covenant_destroy(covenant: *mut TariCovenant) {
    if !covenant.is_null() {
        drop(Box::from_raw(covenant))
    }
}

/// -------------------------------------------------------------------------------------------- ///
/// --------------------------------------- EncryptedOpenings --------------------------------------------///

/// Creates a TariEncryptedOpenings from a ByteVector containing the encrypted_data bytes
///
/// ## Arguments
/// `encrypted_data_bytes` - The encrypted_data bytes as a ByteVector
///
/// ## Returns
/// `TariEncryptedOpenings` - Returns  encrypted data. Note that it will be ptr::null_mut() if any argument is
/// null or if there was an error with the contents of bytes
///
/// # Safety
/// The ```encrypted_data_destroy``` function must be called when finished with a TariEncryptedOpenings to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn encrypted_data_create_from_bytes(
    encrypted_data_bytes: *const ByteVector,
    error_out: *mut c_int,
) -> *mut TariEncryptedOpenings {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if encrypted_data_bytes.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("encrypted_data_bytes".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let decoded_encrypted_data_bytes = (*encrypted_data_bytes).0.clone();

    match TariEncryptedOpenings::from_bytes(&decoded_encrypted_data_bytes) {
        Ok(encrypted_data) => Box::into_raw(Box::new(encrypted_data)),
        Err(e) => {
            error!(target: LOG_TARGET, "Error creating an encrypted_data: {:?}", e);
            error = LibWalletError::from(InterfaceError::InvalidArgument("encrypted_data_bytes".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Creates a ByteVector containing the encrypted_data bytes from a TariEncryptedOpenings
///
/// ## Arguments
/// `encrypted_data` - The encrypted_data as a TariEncryptedOpenings
///
/// ## Returns
/// `ByteVector` - Returns a ByteVector containing the encrypted_data bytes. Note that it will be ptr::null_mut() if
/// any argument is null or if there was an error with the contents of bytes
///
/// # Safety
/// The `encrypted_data_destroy` function must be called when finished with a TariEncryptedOpenings to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn encrypted_data_as_bytes(
    encrypted_data: *const TariEncryptedOpenings,
    error_out: *mut c_int,
) -> *mut ByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if encrypted_data.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("encrypted_data".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let encrypted_data_bytes = TariEncryptedOpenings::to_byte_vec(&(*encrypted_data)).to_vec();
    let encrypted_byte_vector = ByteVector(encrypted_data_bytes);
    Box::into_raw(Box::new(encrypted_byte_vector))
}

/// Frees memory for a TariEncryptedOpenings
///
/// ## Arguments
/// `encrypted_data` - The pointer to a TariEncryptedOpenings
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn encrypted_data_destroy(encrypted_data: *mut TariEncryptedOpenings) {
    if !encrypted_data.is_null() {
        // zeroize the data content of encrypted_data, as to prevent memory leaks
        (*encrypted_data).zeroize();
    }
}

/// -------------------------------------------------------------------------------------------- ///
/// ---------------------------------- Output Features ------------------------------------------///

/// Creates a TariOutputFeatures from byte values
///
/// ## Arguments
/// `version` - The encoded value of the version as a byte
/// `output_type` - The encoded value of the output type as a byte
/// `maturity` - The encoded value maturity as bytes
/// `metadata` - The metadata componenet as a ByteVector. It cannot be null
/// `encrypted_data` - The encrypted_data component as a ByteVector. It can be null  to model a None value.
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `TariOutputFeatures` - Returns an output features object. Note that it will be ptr::null_mut() if any mandatory
/// arguments are null or if there was an error with the contents of bytes
///
/// # Safety
/// The ```output_features_destroy``` function must be called when finished with a TariOutputFeatures to
/// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn output_features_create_from_bytes(
    version: c_uchar,
    output_type: c_ushort,
    maturity: c_ulonglong,
    metadata: *const ByteVector,
    range_proof_type: c_ushort,
    error_out: *mut c_int,
) -> *mut TariOutputFeatures {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if metadata.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("metadata".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let decoded_version = match OutputFeaturesVersion::try_from(version) {
        Ok(v) => v,
        Err(message) => {
            error!(
                target: LOG_TARGET,
                "Error creating a OutputFeaturesVersion: {:?}", message
            );
            error = LibWalletError::from(InterfaceError::InvalidArgument("version".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let decoded_output_type = match output_type.try_into().ok().and_then(OutputType::from_byte) {
        Some(val) => val,
        None => {
            error!(target: LOG_TARGET, "output_type overflowed",);
            error = LibWalletError::from(InterfaceError::InvalidArgument("flag".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let decoded_range_proof_type = match range_proof_type.try_into().ok().and_then(RangeProofType::from_byte) {
        Some(val) => val,
        None => {
            error!(target: LOG_TARGET, "range_proof_type overflowed",);
            error = LibWalletError::from(InterfaceError::InvalidArgument("flag".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let decoded_metadata = (*metadata).0.clone();

    let output_features = TariOutputFeatures::new(
        decoded_version,
        decoded_output_type,
        maturity,
        decoded_metadata,
        None,
        decoded_range_proof_type,
    );
    Box::into_raw(Box::new(output_features))
}

/// Frees memory for a TariOutputFeatures
///
/// ## Arguments
/// `output_features` - The pointer to a TariOutputFeatures
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn output_features_destroy(output_features: *mut TariOutputFeatures) {
    if !output_features.is_null() {
        drop(Box::from_raw(output_features))
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- Seed Words ----------------------------------------------///

/// Create an empty instance of TariSeedWords
///
/// ## Arguments
/// None
///
/// ## Returns
/// `TariSeedWords` - Returns an empty TariSeedWords instance
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn seed_words_create() -> *mut TariSeedWords {
    let seed_words = SeedWords::new(vec![]);
    Box::into_raw(Box::new(TariSeedWords(seed_words)))
}

/// Create a TariSeedWords instance containing the entire mnemonic wordlist for the requested language
///
/// ## Arguments
/// `language` - The required language as a string
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `TariSeedWords` - Returns the TariSeedWords instance containing the entire mnemonic wordlist for the
/// requested language.
///
/// # Safety
/// The `seed_words_destroy` method must be called when finished with a TariSeedWords instance from rust to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn seed_words_get_mnemonic_word_list_for_language(
    language: *const c_char,
    error_out: *mut c_int,
) -> *mut TariSeedWords {
    use tari_key_manager::mnemonic_wordlists;

    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let mut mnemonic_word_list_vec = SeedWords::new(vec![]);
    if language.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("mnemonic wordlist".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        let not_supported;
        let language_string = match CStr::from_ptr(language).to_str() {
            Ok(str) => str,
            Err(e) => {
                not_supported = e.to_string();
                not_supported.as_str()
            },
        };
        let mnemonic_word_list = match TariMnemonicLanguage::from_str(language_string) {
            Ok(language) => match language {
                TariMnemonicLanguage::ChineseSimplified => mnemonic_wordlists::MNEMONIC_CHINESE_SIMPLIFIED_WORDS,
                TariMnemonicLanguage::English => mnemonic_wordlists::MNEMONIC_ENGLISH_WORDS,
                TariMnemonicLanguage::French => mnemonic_wordlists::MNEMONIC_FRENCH_WORDS,
                TariMnemonicLanguage::Italian => mnemonic_wordlists::MNEMONIC_ITALIAN_WORDS,
                TariMnemonicLanguage::Japanese => mnemonic_wordlists::MNEMONIC_JAPANESE_WORDS,
                TariMnemonicLanguage::Korean => mnemonic_wordlists::MNEMONIC_KOREAN_WORDS,
                TariMnemonicLanguage::Spanish => mnemonic_wordlists::MNEMONIC_SPANISH_WORDS,
            },
            Err(_) => {
                error!(
                    target: LOG_TARGET,
                    "Mnemonic wordlist - '{}' language not supported", language_string
                );
                error = LibWalletError::from(InterfaceError::InvalidArgument(format!(
                    "mnemonic wordlist - '{}' language not supported",
                    language_string
                )))
                .code;
                ptr::swap(error_out, &mut error as *mut c_int);
                [""; 2048]
            },
        };
        info!(
            target: LOG_TARGET,
            "Retrieved mnemonic wordlist for'{}'", language_string
        );
        mnemonic_word_list_vec =
            SeedWords::new(mnemonic_word_list.iter().map(|s| Hidden::hide(s.to_string())).collect());
    }

    Box::into_raw(Box::new(TariSeedWords(mnemonic_word_list_vec)))
}

/// Gets the length of TariSeedWords
///
/// ## Arguments
/// `seed_words` - The pointer to a TariSeedWords
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns number of elements in seed_words, zero if seed_words is null
///
/// # Safety
/// None
// casting here is okay as we wont get more than u32 seed words
#[allow(clippy::cast_possible_truncation)]
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
// casting here is okay as there aint more as u32 seed words
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn seed_words_get_at(
    seed_words: *mut TariSeedWords,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut word = CString::new("").expect("Blank CString will not fail.");
    if seed_words.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("seed words".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        let len = (*seed_words).0.len(); // clamp to length
        if (*seed_words).0.is_empty() || position > (len - 1) as u32 {
            error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        } else if let Ok(v) = CString::new(
            (*seed_words)
                .0
                .get_word(position as usize)
                .expect("Seed Words position is in bounds")
                .as_str(),
        ) {
            word = v;
        } else {
            error = LibWalletError::from(InterfaceError::PointerError("seed_words".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        }
    }
    CString::into_raw(word)
}

/// Add a word to the provided TariSeedWords instance
///
/// ## Arguments
/// `seed_words` - The pointer to a TariSeedWords
/// `word` - Word to add
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// 'c_uchar' - Returns a u8 version of the `SeedWordPushResult` enum indicating whether the word was not a valid seed
/// word, if the push was successful and whether the push was successful and completed the full Seed Phrase.
///  `seed_words` is only modified in the event of a `SuccessfulPush`.
///     '0' -> InvalidSeedWord
///     '1' -> SuccessfulPush
///     '2' -> SeedPhraseComplete
///     '3' -> InvalidSeedPhrase
///     '4' -> NoLanguageMatch,
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn seed_words_push_word(
    seed_words: *mut TariSeedWords,
    word: *const c_char,
    error_out: *mut c_int,
) -> c_uchar {
    use tari_key_manager::mnemonic::Mnemonic;

    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if seed_words.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("seed words".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let word_string;
    if word.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("word".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return SeedWordPushResult::InvalidSeedWord as u8;
    } else {
        match CStr::from_ptr(word).to_str() {
            Ok(v) => {
                word_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("word".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return SeedWordPushResult::InvalidObject as u8;
            },
        }
    }

    // Check word is from a word list
    match MnemonicLanguage::from(&word_string) {
        Ok(language) => {
            if (*seed_words).0.len() >= MnemonicLanguage::word_count(&language) {
                let error_msg = "Invalid seed words object, i.e. the entire mnemonic word list, is being used";
                log::error!(target: LOG_TARGET, "{}", error_msg);
                error = LibWalletError::from(InterfaceError::InvalidArgument(error_msg.to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return SeedWordPushResult::InvalidObject as u8;
            }
        },
        Err(e) => {
            log::error!(
                target: LOG_TARGET,
                "{} is not a valid mnemonic seed word ({:?})",
                word_string,
                e
            );
            return SeedWordPushResult::InvalidSeedWord as u8;
        },
    }

    // Seed words is currently empty, this is the first word
    if (*seed_words).0.is_empty() {
        (*seed_words).0.push(word_string);
        return SeedWordPushResult::SuccessfulPush as u8;
    }

    // Try push to a temporary copy first to prevent existing object becoming invalid
    if let Ok(language) = MnemonicLanguage::detect_language(&(*seed_words).0) {
        // Check words in temp are still consistent for a language, note that detected language can change
        // depending on word added
        if MnemonicLanguage::detect_language(&(*seed_words).0).is_ok() {
            if (*seed_words).0.len() >= 24 {
                if let Err(e) = CipherSeed::from_mnemonic(&(*seed_words).0, None) {
                    log::error!(
                        target: LOG_TARGET,
                        "Problem building valid private seed from seed phrase: {:?}",
                        e
                    );
                    error = LibWalletError::from(WalletError::KeyManagerError(e)).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    return SeedWordPushResult::InvalidSeedPhrase as u8;
                };
            }

            (*seed_words).0.push(word_string);

            // Note: test for a validity was already done so we can just check length here
            if (*seed_words).0.len() < 24 {
                SeedWordPushResult::SuccessfulPush as u8
            } else {
                SeedWordPushResult::SeedPhraseComplete as u8
            }
        } else {
            log::error!(
                target: LOG_TARGET,
                "Words in seed phrase do not match any language after trying to add word: `{:?}`, previously words \
                 were detected to be in: `{:?}`",
                word_string,
                language
            );
            SeedWordPushResult::NoLanguageMatch as u8
        }
    } else {
        // Seed words are invalid, shouldn't normally be reachable
        log::error!(
            target: LOG_TARGET,
            "Words in seed phrase do not match any language prior to adding word: `{:?}`",
            word_string
        );
        let error_msg = "Invalid seed words object, no language can be detected.";
        log::error!(target: LOG_TARGET, "{}", error_msg);
        error = LibWalletError::from(InterfaceError::InvalidArgument(error_msg.to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        SeedWordPushResult::InvalidObject as u8
    }
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
        drop(Box::from_raw(seed_words))
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- Contact -------------------------------------------------///

/// Creates a TariContact
///
/// ## Arguments
/// `alias` - The pointer to a char array
/// `address` - The pointer to a TariWalletAddress
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
    address: *mut TariWalletAddress,
    favourite: bool,
    error_out: *mut c_int,
) -> *mut TariContact {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let alias_string;
    if alias.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("alias".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(alias).to_str() {
            Ok(v) => {
                alias_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("alias".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }

    if address.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let contact = Contact::new(alias_string, (*address).clone(), None, None, favourite);
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
    let mut a = CString::new("").expect("Blank CString will not fail.");
    if contact.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("contact".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        match CString::new((*contact).alias.clone()) {
            Ok(v) => a = v,
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("contact".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
            },
        }
    }
    CString::into_raw(a)
}

/// Gets the favourite status of the TariContact
///
/// ## Arguments
/// `contact` - The pointer to a TariContact
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Returns a bool indicating the favourite status of a contact. NOTE this will return false if the pointer is
/// null as well.
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn contact_get_favourite(contact: *mut TariContact, error_out: *mut c_int) -> bool {
    let mut error = 0;
    let mut favourite = false;
    ptr::swap(error_out, &mut error as *mut c_int);
    if contact.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("contact".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        favourite = (*contact).favourite;
    }

    favourite
}

/// Gets the TariWalletAddress of the TariContact
///
/// ## Arguments
/// `contact` - The pointer to a TariContact
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariWalletAddress` - Returns a pointer to a TariWalletAddress. Note that it returns
/// ptr::null_mut() if contact is null
///
/// # Safety
/// The ```tari_address_destroy``` method must be called when finished with a TariWalletAddress to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn contact_get_tari_address(
    contact: *mut TariContact,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if contact.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("contact".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    Box::into_raw(Box::new((*contact).address.clone()))
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
        drop(Box::from_raw(contact))
    }
}

/// -------------------------------------------------------------------------------------------- ///

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
// casting here is okay as we dont have more thant u32 contacts
#[allow(clippy::cast_possible_truncation)]
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
// converting between here is fine as its used to clamp the array to length
#[allow(clippy::cast_possible_wrap)]
#[no_mangle]
pub unsafe extern "C" fn contacts_get_at(
    contacts: *mut TariContacts,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariContact {
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
        drop(Box::from_raw(contacts))
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- Contacts Liveness Data ----------------------------------///

/// Gets the public_key from a TariContactsLivenessData
///
/// ## Arguments
/// `liveness_data` - The pointer to a TariContactsLivenessData
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariWalletAddress` - Returns a pointer to a TariWalletAddress. Note that it returns ptr::null_mut() if
/// liveness_data is null.
///
/// # Safety
/// The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn liveness_data_get_public_key(
    liveness_data: *mut TariContactsLivenessData,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if liveness_data.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("liveness_data".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    Box::into_raw(Box::new((*liveness_data).address().clone()))
}

/// Gets the latency in milli-seconds (ms) from a TariContactsLivenessData
///
/// ## Arguments
/// `liveness_data` - The pointer to a TariContactsLivenessData
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_int` - Returns a pointer to a c_int if the optional latency data (in milli-seconds (ms)) exists, with a
/// value of '0' if it is None. Note that it also returns '0' if liveness_data is null.
///
/// # Safety
/// The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
/// memory leak

#[no_mangle]
pub unsafe extern "C" fn liveness_data_get_latency(
    liveness_data: *mut TariContactsLivenessData,
    error_out: *mut c_int,
) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if liveness_data.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("liveness_data".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    if let Some(latency) = (*liveness_data).latency() {
        latency as c_uint
    } else {
        0
    }
}

/// Gets the last_seen time (in local time) from a TariContactsLivenessData
///
/// ## Arguments
/// `liveness_data` - The pointer to a TariContactsLivenessData
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array if the optional last_seen data exists, with a value of '?' if it
/// is None. Note that it returns ptr::null_mut() if liveness_data is null.
///
/// # Safety
/// The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn liveness_data_get_last_seen(
    liveness_data: *mut TariContactsLivenessData,
    error_out: *mut c_int,
) -> *mut c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if liveness_data.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("liveness_data".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    if let Some(last_seen) = (*liveness_data).last_ping_pong_received() {
        let last_seen_local_time =
            DateTime::<Local>::from_naive_utc_and_offset(last_seen, Local::now().offset().to_owned())
                .format("%FT%T")
                .to_string();
        let mut return_value = CString::new("").expect("Blank CString will not fail.");
        match CString::new(last_seen_local_time) {
            Ok(val) => {
                return_value = val;
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("liveness_data".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
            },
        }
        CString::into_raw(return_value)
    } else {
        CString::into_raw(CString::new("?").expect("Single character CString will not fail."))
    }
}

/// Gets the message_type (ContactMessageType enum) from a TariContactsLivenessData
///
/// ## Arguments
/// `liveness_data` - The pointer to a TariContactsLivenessData
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_int` - Returns the status which corresponds to:
/// | Value | Interpretation |
/// |---|---|
/// |  -1 | NullError        |
/// |   0 | Ping             |
/// |   1 | Pong             |
/// |   2 | NoMessage        |
///
/// # Safety
/// The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn liveness_data_get_message_type(
    liveness_data: *mut TariContactsLivenessData,
    error_out: *mut c_int,
) -> c_int {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if liveness_data.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("liveness_data".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return -1;
    }
    let status = (*liveness_data).message_type();
    status as c_int
}

/// Gets the online_status (ContactOnlineStatus enum) from a TariContactsLivenessData
///
/// ## Arguments
/// `liveness_data` - The pointer to a TariContactsLivenessData
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_int` - Returns the status which corresponds to:
/// | Value | Interpretation |
/// |---|---|
/// |  -1 | NullError        |
/// |   0 | Online           |
/// |   1 | Offline          |
/// |   2 | NeverSeen        |
/// |   3 | Banned           |
///
/// # Safety
/// The ```liveness_data_destroy``` method must be called when finished with a TariContactsLivenessData to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn liveness_data_get_online_status(
    liveness_data: *mut TariContactsLivenessData,
    error_out: *mut c_int,
) -> *const c_char {
    let mut error = 0;
    let mut result = CString::new("").expect("Blank CString will not fail.");
    ptr::swap(error_out, &mut error as *mut c_int);
    if liveness_data.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("liveness_data".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result.into_raw();
    }
    let status = (*liveness_data).online_status();
    match CString::new(status.to_string()) {
        Ok(v) => result = v,
        _ => {
            error = LibWalletError::from(InterfaceError::PointerError("message".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }
    result.into_raw()
}

/// Frees memory for a TariContactsLivenessData
///
/// ## Arguments
/// `liveness_data` - The pointer to a TariContactsLivenessData
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn liveness_data_destroy(liveness_data: *mut TariContactsLivenessData) {
    if !liveness_data.is_null() {
        drop(Box::from_raw(liveness_data))
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
// casting here is okay as we wont have more than u32 transctions
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn completed_transactions_get_length(
    transactions: *mut TariCompletedTransactions,
    error_out: *mut c_int,
) -> c_uint {
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
// converting between here is fine as its used to clamp the array to length
#[allow(clippy::cast_possible_wrap)]
#[no_mangle]
pub unsafe extern "C" fn completed_transactions_get_at(
    transactions: *mut TariCompletedTransactions,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariCompletedTransaction {
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
        drop(Box::from_raw(transactions))
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
// casting here is we wont have more than u32 transctions
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transactions_get_length(
    transactions: *mut TariPendingOutboundTransactions,
    error_out: *mut c_int,
) -> c_uint {
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
// converting between here is fine as its used to clamp the array to length
#[allow(clippy::cast_possible_wrap)]
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transactions_get_at(
    transactions: *mut TariPendingOutboundTransactions,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariPendingOutboundTransaction {
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
        drop(Box::from_raw(transactions))
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
// casting here is okay as we wont have mroe than u32 tranasctions
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transactions_get_length(
    transactions: *mut TariPendingInboundTransactions,
    error_out: *mut c_int,
) -> c_uint {
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
// converting between here is fine as its used to clamp the array to length
#[allow(clippy::cast_possible_wrap)]
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transactions_get_at(
    transactions: *mut TariPendingInboundTransactions,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariPendingInboundTransaction {
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
        drop(Box::from_raw(transactions))
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
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).tx_id.as_u64() as c_ulonglong
}

/// Gets the destination TariWalletAddress of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariWalletAddress` - Returns the destination TariWalletAddress, note that it will be
/// ptr::null_mut() if transaction is null
///
/// # Safety
/// The ```tari_address_destroy``` method must be called when finished with a TariWalletAddress to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_destination_tari_address(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let address = (*transaction).destination_address.clone();
    Box::into_raw(Box::new(address))
}

/// Gets the TariTransactionKernel of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariTransactionKernel` - Returns the transaction kernel, note that it will be
/// ptr::null_mut() if transaction is null, if the transaction status is Pending, or if the number of kernels is not
/// exactly one.
///
/// # Safety
/// The ```transaction_kernel_destroy``` method must be called when finished with a TariTransactionKernel to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_transaction_kernel(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *mut TariTransactionKernel {
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

    let kernels = (*transaction).transaction.body().kernels();

    // currently we presume that each CompletedTransaction only has 1 kernel
    // if that changes this will need to be accounted for
    if kernels.len() != 1 {
        let msg = format!("Expected 1 kernel, got {}", kernels.len());
        error = LibWalletError::from(TransactionError::KernelError(msg)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let x = kernels[0].clone();
    Box::into_raw(Box::new(x))
}

/// Gets the source TariWalletAddress of a TariCompletedTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariWalletAddress` - Returns the source TariWalletAddress, note that it will be
/// ptr::null_mut() if transaction is null
///
/// # Safety
/// The ```tari_address_destroy``` method must be called when finished with a TariWalletAddress to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_source_tari_address(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let m = (*transaction).source_address.clone();
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
/// |  -1 | TxNullError         |
/// |   0 | Completed           |
/// |   1 | Broadcast           |
/// |   2 | MinedUnconfirmed    |
/// |   3 | Imported            |
/// |   4 | Pending             |
/// |   5 | Coinbase            |
/// |   6 | MinedConfirmed      |
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_status(
    transaction: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> c_int {
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
) -> c_ulonglong {
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
) -> c_ulonglong {
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
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).timestamp.timestamp() as c_ulonglong
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
) -> *const c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let message = (*transaction).message.clone();
    let mut result = CString::new("").expect("Blank CString will not fail.");
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result.into_raw();
    }

    match CString::new(message) {
        Ok(v) => result = v,
        _ => {
            error = LibWalletError::from(InterfaceError::PointerError("message".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

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
) -> bool {
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

/// Gets the number of confirmations of a TariCompletedTransaction
///
/// ## Arguments
/// `tx` - The TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the number of confirmations of a Completed Transaction
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_confirmations(
    tx: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if tx.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("tx".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    (*tx).confirmations.unwrap_or(0)
}

/// Gets the reason a TariCompletedTransaction is cancelled, if it is indeed cancelled
///
/// ## Arguments
/// `tx` - The TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_int` - Returns the reason for cancellation which corresponds to:
/// | Value | Interpretation |
/// |---|---|
/// |  -1 | Not Cancelled       |
/// |   0 | Unknown             |
/// |   1 | UserCancelled       |
/// |   2 | Timeout             |
/// |   3 | DoubleSpend         |
/// |   4 | Orphan              |
/// |   5 | TimeLocked          |
/// |   6 | InvalidTransaction  |
/// |   7 | AbandonedCoinbase   |
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn completed_transaction_get_cancellation_reason(
    tx: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> c_int {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if tx.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("tx".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*tx).cancelled {
        None => -1i32,
        Some(reason) => reason as i32,
    }
}

/// returns the TariCompletedTransaction as a json string
///
/// ## Arguments
/// `tx` - The pointer to a TariCompletedTransaction
///
/// ## Returns
/// `*mut c_char` - Returns a pointer to a char array. Note that it returns an empty char array if
/// TariCompletedTransaction is null or the position is invalid
///
/// # Safety
///  The ```completed_transaction_destroy``` function must be called when finished with a TariCompletedTransaction to
/// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn tari_completed_transaction_to_json(
    tx: *mut TariCompletedTransaction,
    error_out: *mut c_int,
) -> *mut c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut hex_bytes = CString::new("").expect("Blank CString will not fail.");
    if tx.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        match serde_json::to_string(&*tx) {
            Ok(json_string) => match CString::new(json_string) {
                Ok(v) => hex_bytes = v,
                _ => {
                    error = LibWalletError::from(InterfaceError::PointerError("transaction".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                },
            },
            Err(_) => {
                error = LibWalletError::from(HexError::HexConversionError {}).code;
                ptr::swap(error_out, &mut error as *mut c_int);
            },
        }
    }
    CString::into_raw(hex_bytes)
}

/// Creates a TariUnblindedOutput from a char array
///
/// ## Arguments
/// `tx_json` - The pointer to a char array which is json of the TariCompletedTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariCompletedTransaction` - Returns a pointer to a TariCompletedTransaction. Note that it returns
/// ptr::null_mut() if key is null or if there was an error creating the TariCompletedTransaction from key
///
/// # Safety
/// The ```completed_transaction_destroy``` function must be called when finished with a TariCompletedTransaction to
// /// prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn create_tari_completed_transaction_from_json(
    tx_json: *const c_char,
    error_out: *mut c_int,
) -> *mut TariCompletedTransaction {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let tx_json_str;
    if tx_json.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("tx_json".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(tx_json).to_str() {
            Ok(v) => {
                tx_json_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("tx_json".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        };
    }
    let tx: Result<TariCompletedTransaction, _> = serde_json::from_str(&tx_json_str);

    match tx {
        Ok(tx) => Box::into_raw(Box::new(tx)),
        Err(e) => {
            error!(target: LOG_TARGET, "Error creating a transaction from json: {:?}", e);

            error = LibWalletError::from(HexError::HexConversionError {}).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
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
        drop(Box::from_raw(transaction))
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
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).tx_id.as_u64() as c_ulonglong
}

/// Gets the destination TariWalletAddress of a TariPendingOutboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingOutboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariWalletAddress` - Returns the destination TariWalletAddress, note that it will be
/// ptr::null_mut() if transaction is null
///
/// # Safety
/// The ```tari_address_destroy``` method must be called when finished with a TariWalletAddress to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn pending_outbound_transaction_get_destination_tari_address(
    transaction: *mut TariPendingOutboundTransaction,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let m = (*transaction).destination_address.clone();
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
) -> c_ulonglong {
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
) -> c_ulonglong {
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
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).timestamp.timestamp() as c_ulonglong
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
) -> *const c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let message = (*transaction).message.clone();
    let mut result = CString::new("").expect("Blank CString will not fail.");
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result.into_raw();
    }

    match CString::new(message) {
        Ok(v) => result = v,
        _ => {
            error = LibWalletError::from(InterfaceError::PointerError("message".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

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
) -> c_int {
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
        drop(Box::from_raw(transaction))
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
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).tx_id.as_u64() as c_ulonglong
}

/// Gets the source TariWalletAddress of a TariPendingInboundTransaction
///
/// ## Arguments
/// `transaction` - The pointer to a TariPendingInboundTransaction
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariWalletAddress` - Returns a pointer to the source TariWalletAddress, note that it will be
/// ptr::null_mut() if transaction is null
///
/// # Safety
///  The ```tari_address_destroy``` method must be called when finished with a TariWalletAddress to prevent a memory
/// leak
#[no_mangle]
pub unsafe extern "C" fn pending_inbound_transaction_get_source_tari_address(
    transaction: *mut TariPendingInboundTransaction,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let m = (*transaction).source_address.clone();
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
) -> c_ulonglong {
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
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*transaction).timestamp.timestamp() as c_ulonglong
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
) -> *const c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let message = (*transaction).message.clone();
    let mut result = CString::new("").expect("Blank CString will not fail.");
    if transaction.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result.into_raw();
    }

    match CString::new(message) {
        Ok(v) => result = v,
        _ => {
            error = LibWalletError::from(InterfaceError::PointerError("message".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        },
    }

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
) -> c_int {
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
        drop(Box::from_raw(transaction))
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- Transport Send Status -----------------------------------///

/// Decode the transaction send status of a TariTransactionSendStatus
///
/// ## Arguments
/// `status` - The pointer to a TariTransactionSendStatus
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns
///     !direct_send & !saf_send &  queued   = 0
///      direct_send &  saf_send & !queued   = 1
///      direct_send & !saf_send & !queued   = 2
///     !direct_send &  saf_send & !queued   = 3
///     any other combination (is not valid) = 4
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn transaction_send_status_decode(
    status: *const TariTransactionSendStatus,
    error_out: *mut c_int,
) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut send_status = 4;
    if status.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transaction send status".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else if ((*status).direct_send_result || (*status).store_and_forward_send_result) && (*status).queued_for_retry {
        error = LibWalletError::from(InterfaceError::NullError(
            "transaction send status - not valid".to_string(),
        ))
        .code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else if (*status).queued_for_retry {
        send_status = 0;
    } else if (*status).direct_send_result && (*status).store_and_forward_send_result {
        send_status = 1;
    } else if (*status).direct_send_result && !(*status).store_and_forward_send_result {
        send_status = 2;
    } else if !(*status).direct_send_result && (*status).store_and_forward_send_result {
        send_status = 3;
    } else {
        error = LibWalletError::from(InterfaceError::NullError(
            "transaction send status - not valid".to_string(),
        ))
        .code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }
    send_status
}

/// Frees memory for a TariTransactionSendStatus
///
/// ## Arguments
/// `status` - The pointer to a TariPendingInboundTransaction
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn transaction_send_status_destroy(status: *mut TariTransactionSendStatus) {
    if !status.is_null() {
        drop(Box::from_raw(status))
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
/// `*mut TariTransportConfig` - Returns a pointer to a memory TariTransportConfig
///
/// # Safety
/// The ```transport_type_destroy``` method must be called when finished with a TariTransportConfig to prevent a memory
/// leak
#[no_mangle]
pub unsafe extern "C" fn transport_memory_create() -> *mut TariTransportConfig {
    let port = MemoryTransport::acquire_next_memsocket_port();
    let listener_address: Multiaddr = format!("/memory/{}", port)
        .parse()
        .expect("Should be able to create memory address");
    let transport = TransportConfig {
        transport_type: TransportType::Memory,
        memory: MemoryTransportConfig { listener_address },
        ..Default::default()
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
/// `*mut TariTransportConfig` - Returns a pointer to a tcp TariTransportConfig, null on error.
///
/// # Safety
/// The ```transport_type_destroy``` method must be called when finished with a TariTransportConfig to prevent a memory
/// leak
#[no_mangle]
pub unsafe extern "C" fn transport_tcp_create(
    listener_address: *const c_char,
    error_out: *mut c_int,
) -> *mut TariTransportConfig {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let listener_address_str;
    if listener_address.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("listener_address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(listener_address).to_str() {
            Ok(v) => {
                listener_address_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("listener_address".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }

    match listener_address_str.parse() {
        Ok(v) => {
            let transport = TariTransportConfig {
                transport_type: TransportType::Tcp,
                tcp: TcpTransportConfig {
                    listener_address: v,
                    ..Default::default()
                },
                ..Default::default()
            };
            Box::into_raw(Box::new(transport))
        },
        Err(_) => {
            error = LibWalletError::from(InterfaceError::InvalidArgument("listener_address".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Creates a tor transport type
///
/// ## Arguments
/// `control_server_address` - The pointer to a char array
/// `tor_cookie` - The pointer to a ByteVector containing the contents of the tor cookie file, can be null
/// `tor_port` - The tor port
/// `tor_proxy_bypass_for_outbound` - Whether tor will use a direct tcp connection for a given bypass address instead of
/// the tor proxy if tcp is available, if not it has no effect
/// `socks_password` - The pointer to a char array containing the socks password, can be null
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariTransportConfig` - Returns a pointer to a tor TariTransportConfig, null on error.
///
/// # Safety
/// The ```transport_config_destroy``` method must be called when finished with a TariTransportConfig to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn transport_tor_create(
    control_server_address: *const c_char,
    tor_cookie: *const ByteVector,
    tor_port: c_ushort,
    tor_proxy_bypass_for_outbound: bool,
    socks_username: *const c_char,
    socks_password: *const c_char,
    error_out: *mut c_int,
) -> *mut TariTransportConfig {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let control_address_str;
    if control_server_address.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("control_server_address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(control_server_address).to_str() {
            Ok(v) => {
                control_address_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("control_server_address".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }

    let username_str;
    let password_str;
    let socks_authentication = if !socks_username.is_null() && !socks_password.is_null() {
        match CStr::from_ptr(socks_username).to_str() {
            Ok(v) => {
                username_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("socks_username".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
        match CStr::from_ptr(socks_password).to_str() {
            Ok(v) => {
                password_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("socks_password".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        };
        SocksAuthentication::UsernamePassword {
            username: username_str,
            password: password_str,
        }
    } else {
        SocksAuthentication::None
    };

    let tor_authentication = if tor_cookie.is_null() {
        TorControlAuthentication::None
    } else {
        let cookie_hex = hex::to_hex((*tor_cookie).0.as_slice());
        TorControlAuthentication::hex(cookie_hex)
    };

    let onion_port = match NonZeroU16::new(tor_port) {
        Some(p) => p,
        None => {
            error = LibWalletError::from(InterfaceError::InvalidArgument(
                "onion_port must be greater than 0".to_string(),
            ))
            .code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    match control_address_str.parse() {
        Ok(v) => {
            let transport = TariTransportConfig {
                transport_type: TransportType::Tor,
                tor: TorTransportConfig {
                    control_address: v,
                    control_auth: tor_authentication,
                    // The wallet will populate this from the db
                    identity: None,
                    onion_port,
                    socks_auth: socks_authentication,
                    proxy_bypass_for_outbound_tcp: tor_proxy_bypass_for_outbound,
                    ..Default::default()
                },
                ..Default::default()
            };

            Box::into_raw(Box::new(transport))
        },
        Err(_) => {
            error = LibWalletError::from(InterfaceError::InvalidArgument("control_address".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Gets the address for a memory transport type
///
/// ## Arguments
/// `transport` - Pointer to a TariTransportConfig
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
    transport: *const TariTransportConfig,
    error_out: *mut c_int,
) -> *mut c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut address = CString::new("").expect("Blank CString will not fail.");
    if transport.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transport".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int)
    } else {
        match (*transport).transport_type {
            TransportType::Memory => match CString::new((*transport).memory.listener_address.to_string()) {
                Ok(v) => address = v,
                _ => {
                    error = LibWalletError::from(InterfaceError::PointerError("transport".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                },
            },
            _ => {
                error = LibWalletError::from(InterfaceError::NullError("transport".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
            },
        }
    }

    address.into_raw()
}

/// Frees memory for a TariTransportConfig
///
/// ## Arguments
/// `transport` - The pointer to a TariTransportConfig
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
#[no_mangle]
#[deprecated(note = "use transport_config_destroy instead")]
pub unsafe extern "C" fn transport_type_destroy(transport: *mut TariTransportConfig) {
    transport_config_destroy(transport);
}

/// Frees memory for a TariTransportConfig
///
/// ## Arguments
/// `transport` - The pointer to a TariTransportConfig
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn transport_config_destroy(transport: *mut TariTransportConfig) {
    if !transport.is_null() {
        drop(Box::from_raw(transport))
    }
}

/// ---------------------------------------------------------------------------------------------///

/// ----------------------------------- CommsConfig ---------------------------------------------///

/// Creates a TariCommsConfig. The result from this function is required when initializing a TariWallet.
///
/// ## Arguments
/// `public_address` - The public address char array pointer. This is the address that the wallet advertises publicly to
/// peers
/// `transport` - TariTransportConfig that specifies the type of comms transport to be used.
/// connections are moved to after initial connection. Default if null is 0.0.0.0:7898 which will accept connections
/// from all IP address on port 7898
/// `database_name` - The database name char array pointer. This is the unique name of this
/// wallet's database
/// `database_path` - The database path char array pointer which. This is the folder path where the
/// database files will be created and the application has write access to
/// `discovery_timeout_in_secs`: specify how long the Discovery Timeout for the wallet is.
/// `network`: name of network to connect to. Valid values are: esmeralda, dibbler, igor, localnet, mainnet, stagenet
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
#[allow(clippy::too_many_lines)]
pub unsafe extern "C" fn comms_config_create(
    public_address: *const c_char,
    transport: *const TariTransportConfig,
    database_name: *const c_char,
    datastore_path: *const c_char,
    discovery_timeout_in_secs: c_ulonglong,
    saf_message_duration_in_secs: c_ulonglong,
    error_out: *mut c_int,
) -> *mut TariCommsConfig {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let public_address_str;
    if public_address.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("public_address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(public_address).to_str() {
            Ok(v) => {
                public_address_str = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("public_address".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }

    let database_name_string;
    if database_name.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("database_name".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(database_name).to_str() {
            Ok(v) => {
                database_name_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("database_name".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }

    let datastore_path_string;
    if datastore_path.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("datastore_path".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(datastore_path).to_str() {
            Ok(v) => {
                datastore_path_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("datastore_path".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }
    let datastore_path = PathBuf::from(datastore_path_string);

    if transport.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("transport".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let dht_database_path = datastore_path.join("dht.db");

    let public_address = public_address_str.parse::<Multiaddr>();

    match public_address {
        Ok(public_address) => {
            let addresses = if (*transport).transport_type == TransportType::Tor {
                MultiaddrList::default()
            } else {
                MultiaddrList::from(vec![public_address])
            };

            let config = TariCommsConfig {
                override_from: None,
                public_addresses: addresses,
                transport: (*transport).clone(),
                auxiliary_tcp_listener_address: None,
                datastore_path,
                peer_database_name: database_name_string,
                max_concurrent_inbound_tasks: 25,
                max_concurrent_outbound_tasks: 50,
                dht: DhtConfig {
                    num_neighbouring_nodes: 5,
                    num_random_nodes: 1,
                    minimize_connections: true,
                    discovery_request_timeout: Duration::from_secs(discovery_timeout_in_secs),
                    database_url: DbConnectionUrl::File(dht_database_path),
                    auto_join: true,
                    saf: SafConfig {
                        msg_validity: Duration::from_secs(saf_message_duration_in_secs),
                        // Ensure that SAF messages are requested automatically
                        auto_request: true,
                        ..Default::default()
                    },
                    network_discovery: NetworkDiscoveryConfig {
                        min_desired_peers: 16,
                        initial_peer_sync_delay: Some(Duration::from_secs(25)),
                        ..Default::default()
                    },
                    connectivity: DhtConnectivityConfig {
                        update_interval: Duration::from_secs(5 * 60),
                        minimum_desired_tcpv4_node_ratio: 0.0,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                allow_test_addresses: true,
                listener_liveness_allowlist_cidrs: StringList::new(),
                listener_liveness_max_sessions: 0,
                rpc_max_simultaneous_sessions: 0,
                rpc_max_sessions_per_peer: 0,
                listener_self_liveness_check_interval: None,
            };

            Box::into_raw(Box::new(config))
        },
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
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
        drop(Box::from_raw(wc))
    }
}

/// This function lists the public keys of all connected peers
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `TariPublicKeys` -  Returns a list of connected public keys. Note the result will be null if there was an error
///
/// # Safety
/// The caller is responsible for null checking and deallocating the returned object using public_keys_destroy.
#[no_mangle]
pub unsafe extern "C" fn comms_list_connected_public_keys(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> *mut TariPublicKeys {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let mut connectivity = (*wallet).wallet.comms.connectivity();
    let peer_manager = (*wallet).wallet.comms.peer_manager();

    #[allow(clippy::blocks_in_conditions)]
    match (*wallet).runtime.block_on(async move {
        let connections = connectivity.get_active_connections().await?;
        let mut public_keys = Vec::with_capacity(connections.len());
        for conn in connections {
            if let Some(peer) = peer_manager.find_by_node_id(conn.peer_node_id()).await? {
                public_keys.push(peer.public_key);
            }
        }
        Result::<_, WalletError>::Ok(public_keys)
    }) {
        Ok(public_keys) => Box::into_raw(Box::new(TariPublicKeys(public_keys))),
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Gets the length of the public keys vector
///
/// ## Arguments
/// `public_keys` - Pointer to TariPublicKeys
///
/// ## Returns
/// `c_uint` - Length of the TariPublicKeys vector, 0 if is null
///
/// # Safety
/// None
// casting here is okay as we wont have more than u32 public keys
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn public_keys_get_length(public_keys: *const TariPublicKeys, error_out: *mut c_int) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if public_keys.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("public_keys".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    (*public_keys).0.len() as c_uint
}

/// Gets a ByteVector at position in a EmojiSet
///
/// ## Arguments
/// `public_keys` - The pointer to a TariPublicKeys
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
pub unsafe extern "C" fn public_keys_get_at(
    public_keys: *const TariPublicKeys,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariPublicKey {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if public_keys.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("public_keys".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let last_index = public_keys_get_length(public_keys, error_out) - 1;
    if position > last_index {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let result = (*public_keys).0[position as usize].clone();
    Box::into_raw(Box::new(result))
}

/// ---------------------------------------------------------------------------------------------- ///

/// ------------------------------------- Wallet -------------------------------------------------///

/// Inits logging, this function is deliberately not exposed externally in the header
///
/// ## Arguments
/// `log_path` - Path to where the log will be stored
/// `num_rolling_log_files` - Number of rolling files to be used.
/// `size_per_log_file_bytes` - Max byte size of log file
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[allow(clippy::too_many_lines)]
unsafe fn init_logging(
    log_path: *const c_char,
    log_verbosity: c_int,
    num_rolling_log_files: c_uint,
    size_per_log_file_bytes: c_uint,
    error_out: *mut c_int,
) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let v = CStr::from_ptr(log_path).to_str();
    if v.is_err() {
        error = LibWalletError::from(InterfaceError::PointerError("log_path".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    let log_level = match log_verbosity {
        0 => LevelFilter::Off,
        1 => LevelFilter::Error,
        2 => LevelFilter::Warn,
        3 => LevelFilter::Info,
        4 => LevelFilter::Debug,
        5 | 11 => LevelFilter::Trace, // Cranked up to 11
        _ => LevelFilter::Warn,
    };

    let path = v.unwrap().to_owned();
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
            .expect("Should be able to create a Roller");
        let size_trigger = SizeTrigger::new(u64::from(size_per_log_file_bytes));
        let policy = CompoundPolicy::new(Box::new(size_trigger), Box::new(roller));

        Box::new(
            RollingFileAppender::builder()
                .encoder(Box::new(encoder))
                .append(true)
                .build(path.as_str(), Box::new(policy))
                .expect("Should be able to create an appender"),
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
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("comms", log_level),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("comms::noise", log_level),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("tokio_util", log_level),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("tracing", log_level),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("wallet::transaction_service::callback_handler", log_level),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("p2p", log_level),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("yamux", log_level),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("dht", log_level),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("mio", log_level),
        )
        .build(Root::builder().appender("logfile").build(log_level))
        .expect("Should be able to create a Config");

    match log4rs::init_config(lconfig) {
        Ok(_) => debug!(target: LOG_TARGET, "Logging started"),
        Err(_) => warn!(target: LOG_TARGET, "Logging has already been initialized"),
    }
}

/// Creates a TariWallet
///
/// ## Arguments
/// `config` - The TariCommsConfig pointer
/// `log_path` - An optional file path to the file where the logs will be written. If no log is required pass *null*
/// pointer.
/// `log_verbosity` - how verbose should logging be as a c_int 0-5, or 11
///        0 => Off
///        1 => Error
///        2 => Warn
///        3 => Info
///        4 => Debug
///        5 | 11 => Trace // Cranked up to 11
/// `num_rolling_log_files` - Specifies how many rolling log files to produce, if no rolling files are wanted then set
/// this to 0
/// `size_per_log_file_bytes` - Specifies the size, in bytes, at which the logs files will roll over, if no
/// rolling files are wanted then set this to 0
/// `passphrase` - An optional string that represents the passphrase used to
/// encrypt/decrypt the databases for this wallet. If it is left Null no encryption is used. If the databases have been
/// encrypted then the correct passphrase is required or this function will fail.
/// `seed_words` - An optional instance of TariSeedWords, used to create a wallet for recovery purposes.
/// If this is null, then a new master key is created for the wallet.
/// `callback_received_transaction` - The callback function pointer matching the function signature. This will be
/// called when an inbound transaction is received.
/// `callback_received_transaction_reply` - The callback function
/// pointer matching the function signature. This will be called when a reply is received for a pending outbound
/// transaction
/// `callback_received_finalized_transaction` - The callback function pointer matching the function
/// signature. This will be called when a Finalized version on an Inbound transaction is received
/// `callback_transaction_broadcast` - The callback function pointer matching the function signature. This will be
/// called when a Finalized transaction is detected a Broadcast to a base node mempool.
/// `callback_transaction_mined` - The callback function pointer matching the function signature. This will be called
/// when a Broadcast transaction is detected as mined AND confirmed.
/// `callback_transaction_mined_unconfirmed` - The callback function pointer matching the function signature. This will
/// be called when a Broadcast transaction is detected as mined but not yet confirmed.
/// `callback_faux_transaction_confirmed` - The callback function pointer matching the function signature. This will be
/// called when a one-sided transaction is detected as mined AND confirmed.
/// `callback_faux_transaction_unconfirmed` - The callback function pointer matching the function signature. This
/// will be called when a one-sided transaction is detected as mined but not yet confirmed.
/// `callback_transaction_send_result` - The callback function pointer matching the function signature. This is called
/// when a transaction send is completed. The first parameter is the transaction id and the second contains the
/// transaction send status, weather it was send direct and/or send via saf on the one hand or queued for further retry
/// sending on the other hand.
///     !direct_send & !saf_send &  queued   = 0
///      direct_send &  saf_send & !queued   = 1
///      direct_send & !saf_send & !queued   = 2
///     !direct_send &  saf_send & !queued   = 3
///     any other combination (is not valid) = 4
/// `callback_transaction_cancellation` - The callback function pointer matching
/// the function signature. This is called when a transaction is cancelled. The first parameter is a pointer to the
/// cancelled transaction, the second is a reason as to why said transaction failed that is mapped to the
/// `TxCancellationReason` enum: pub enum TxCancellationReason {
///     Unknown,                // 0
///     UserCancelled,          // 1
///     Timeout,                // 2
///     DoubleSpend,            // 3
///     Orphan,                 // 4
///     TimeLocked,             // 5
///     InvalidTransaction,     // 6
/// }
/// `callback_txo_validation_complete` - The callback function pointer matching the function signature. This is called
/// when a TXO validation process is completed. The request_key is used to identify which request this
/// callback references and the second parameter the second contains, weather it was successful, already busy, failed
/// due to an internal failure or failed due to a communication failure.
///     TxoValidationSuccess,               // 0
///     TxoValidationAlreadyBusy            // 1
///     TxoValidationInternalFailure        // 2
///     TxoValidationCommunicationFailure   // 3
/// `callback_contacts_liveness_data_updated` - The callback function pointer matching the function signature. This is
/// called when a contact's liveness status changed. The data represents the contact's updated status information.
/// `callback_balance_updated` - The callback function pointer matching the function signature. This is called whenever
/// the balance changes.
/// `callback_transaction_validation_complete` - The callback function pointer matching the function signature. This is
/// called when a Transaction validation process is completed. The request_key is used to identify which request this
/// callback references and the second parameter is a u64 that returns if the validation was successful or not.
///         ValidationSuccess,               // 0
///         ValidationAlreadyBusy            // 1
///         ValidationInternalFailure        // 2
///         ValidationCommunicationFailure   // 3
/// `callback_saf_message_received` - The callback function pointer that will be called when the Dht has determined that
/// is has connected to enough of its neighbours to be confident that it has received any SAF messages that were waiting
/// for it.
/// `callback_connectivity_status` -  This callback is called when the status of connection to the set base node
/// changes. it will return an enum encoded as an integer as follows:
/// pub enum OnlineStatus {
///     Connecting,     // 0
///     Online,         // 1
///     Offline,        // 2
/// }
/// `recovery_in_progress` - Pointer to an bool which will be modified to indicate if there is an outstanding recovery
/// that should be completed or not to an error code should one occur, may not be null. Functions as an out parameter.
/// `error_out` - Pointer to an int which will be modified
/// to an error code should one occur, may not be null. Functions as an out parameter.
/// ## Returns
/// `*mut TariWallet` - Returns a pointer to a TariWallet, note that it returns ptr::null_mut()
/// if config is null, a wallet error was encountered or if the runtime could not be created
///
/// # Safety
/// The ```wallet_destroy``` method must be called when finished with a TariWallet to prevent a memory leak
#[no_mangle]
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::too_many_lines)]
pub unsafe extern "C" fn wallet_create(
    config: *mut TariCommsConfig,
    log_path: *const c_char,
    log_verbosity: c_int,
    num_rolling_log_files: c_uint,
    size_per_log_file_bytes: c_uint,
    passphrase: *const c_char,
    seed_words: *const TariSeedWords,
    network_str: *const c_char,
    peer_seed_str: *const c_char,
    dns_sec: bool,

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
    callback_base_node_state: unsafe extern "C" fn(*mut TariBaseNodeState),
    recovery_in_progress: *mut bool,
    error_out: *mut c_int,
) -> *mut TariWallet {
    use tari_key_manager::mnemonic::Mnemonic;

    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if config.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("config".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    if !log_path.is_null() {
        init_logging(
            log_path,
            log_verbosity,
            num_rolling_log_files,
            size_per_log_file_bytes,
            error_out,
        );

        if error > 0 {
            return ptr::null_mut();
        }
    }
    info!(
        target: LOG_TARGET,
        "Starting Tari Wallet FFI version: {}",
        consts::APP_VERSION
    );

    let passphrase = if passphrase.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("passphrase".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        let pf = CStr::from_ptr(passphrase)
            .to_str()
            .expect("A non-null passphrase should be able to be converted to string")
            .to_owned();
        SafePassword::from(pf)
    };

    let peer_seed = if peer_seed_str.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("peer seed dns".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        let peer_seed = CStr::from_ptr(peer_seed_str)
            .to_str()
            .expect("A non-null peer seed should be able to be converted to string");
        info!(target: LOG_TARGET, "peer seed dns {}", peer_seed);
        peer_seed
    };

    let recovery_seed = if seed_words.is_null() {
        None
    } else {
        match CipherSeed::from_mnemonic(&(*seed_words).0, None) {
            Ok(seed) => Some(seed),
            Err(e) => {
                error!(target: LOG_TARGET, "Mnemonic Error for given seed words: {:?}", e);
                error = LibWalletError::from(WalletError::KeyManagerError(e)).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    };

    let network = if network_str.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("network".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        let network = CStr::from_ptr(network_str)
            .to_str()
            .expect("A non-null network should be able to be converted to string");
        info!(target: LOG_TARGET, "network set to {}", network);

        match Network::from_str(network) {
            Ok(n) => n,
            Err(_) => {
                error = LibWalletError::from(InterfaceError::InvalidArgument("network".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    };
    // Set the static network variable according to the user chosen network (for use with
    // `get_current_or_user_setting_or_default()`) -
    if let Err(e) = set_network_if_choice_valid(network) {
        error = LibWalletError::from(InterfaceError::InvalidArgument(e.to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    };

    let runtime = match Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            error = LibWalletError::from(InterfaceError::TokioError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };
    let factories = CryptoFactories::default();

    let sql_database_path = (*config)
        .datastore_path
        .join((*config).peer_database_name.clone())
        .with_extension("sqlite3");

    debug!(target: LOG_TARGET, "Running Wallet database migrations");

    let (wallet_backend, transaction_backend, output_manager_backend, contacts_backend, key_manager_backend) =
        match initialize_sqlite_database_backends(sql_database_path, passphrase, 16) {
            Ok((w, t, o, c, x)) => (w, t, o, c, x),
            Err(e) => {
                error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        };

    let wallet_database = WalletDatabase::new(wallet_backend);
    let output_manager_database = OutputManagerDatabase::new(output_manager_backend.clone());

    debug!(target: LOG_TARGET, "Databases Initialized");

    // If the transport type is Tor then check if there is a stored TorID, if there is update the Transport Type
    let mut comms_config = (*config).clone();
    if let TransportType::Tor = comms_config.transport.transport_type {
        comms_config.transport.tor.identity = wallet_database.get_tor_id().ok().flatten();
    }

    let result = runtime.block_on(async {
        let master_seed = read_or_create_master_seed(recovery_seed, &wallet_database)
            .map_err(|err| WalletStorageError::RecoverySeedError(err.to_string()))?;
        let comms_secret_key = derive_comms_secret_key(&master_seed)
            .map_err(|err| WalletStorageError::RecoverySeedError(err.to_string()))?;

        let node_features = wallet_database.get_node_features()?.unwrap_or_default();
        let node_addresses = if comms_config.public_addresses.is_empty() {
            match wallet_database.get_node_address()? {
                Some(addr) => MultiaddrList::from(vec![addr]),
                None => MultiaddrList::default(),
            }
        } else {
            comms_config.public_addresses.clone()
        };
        debug!(target: LOG_TARGET, "We have the following addresses");
        for address in &node_addresses {
            debug!(target: LOG_TARGET, "Address: {}", address);
        }
        let identity_sig = wallet_database.get_comms_identity_signature()?;

        // This checks if anything has changed by validating the previous signature and if invalid, setting identity_sig
        // to None
        let identity_sig = identity_sig.filter(|sig| {
            let comms_public_key = CommsPublicKey::from_secret_key(&comms_secret_key);
            sig.is_valid(&comms_public_key, node_features, &node_addresses)
        });

        // SAFETY: we are manually checking the validity of this signature before adding Some(..)
        let node_identity = Arc::new(NodeIdentity::with_signature_unchecked(
            comms_secret_key,
            node_addresses.to_vec(),
            node_features,
            identity_sig,
        ));
        if !node_identity.is_signed() {
            node_identity.sign();
            // unreachable panic: signed above
            let sig = node_identity
                .identity_signature_read()
                .as_ref()
                .expect("unreachable panic")
                .clone();
            wallet_database.set_comms_identity_signature(sig)?;
        }
        Ok((master_seed, node_identity))
    });

    let (master_seed, node_identity) = match result {
        Ok(tuple) => tuple,
        Err(e) => {
            error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let shutdown = Shutdown::new();
    let wallet_config = WalletConfig {
        override_from: None,
        p2p: comms_config,
        transaction_service_config: TransactionServiceConfig {
            direct_send_timeout: (*config).dht.discovery_request_timeout,
            ..Default::default()
        },
        base_node_service_config: BaseNodeServiceConfig { ..Default::default() },
        network,
        ..Default::default()
    };

    let mut recovery_lookup = match wallet_database.get_client_key_value(RECOVERY_KEY.to_owned()) {
        Err(_) => false,
        Ok(None) => false,
        Ok(Some(_)) => true,
    };
    ptr::swap(recovery_in_progress, &mut recovery_lookup as *mut bool);

    let peer_seeds = PeerSeedsConfig {
        dns_seeds_name_server: DEFAULT_DNS_NAME_SERVER.parse().unwrap(),
        dns_seeds_use_dnssec: dns_sec,
        dns_seeds: StringList::from(vec![peer_seed.to_string()]),
        ..Default::default()
    };

    let auto_update = AutoUpdateConfig::default();
    let consensus_manager = match ConsensusManager::builder(network).build() {
        Ok(cm) => cm,
        Err(_) => {
            error = 10;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let user_agent = format!("tari/wallet_ffi/{}", env!("CARGO_PKG_VERSION"));
    let w = runtime.block_on(Wallet::start(
        wallet_config,
        peer_seeds,
        auto_update,
        node_identity,
        consensus_manager,
        factories,
        wallet_database,
        output_manager_database,
        transaction_backend.clone(),
        output_manager_backend,
        contacts_backend,
        key_manager_backend,
        shutdown.to_signal(),
        master_seed,
        WalletType::default(),
        user_agent,
    ));

    match w {
        Ok(w) => {
            let wallet_address = TariAddress::new(w.comms.node_identity().public_key().clone(), w.network.as_network());

            // Start Callback Handler
            let callback_handler = CallbackHandler::new(
                TransactionDatabase::new(transaction_backend),
                w.base_node_service.get_event_stream(),
                w.transaction_service.get_event_stream(),
                w.output_manager_service.get_event_stream(),
                w.output_manager_service.clone(),
                w.dht_service.subscribe_dht_events(),
                w.comms.shutdown_signal(),
                wallet_address,
                w.wallet_connectivity.get_connectivity_status_watch(),
                w.contacts_service.get_contacts_liveness_event_stream(),
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
            );

            runtime.spawn(callback_handler.start());

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
}

/// Retrieves the version of an app that last accessed the wallet database
///
/// ## Arguments
/// `config` - The TariCommsConfig pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
/// ## Returns
/// `*mut c_char` - Returns the pointer to the hexadecimal representation of the signature and
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string coming from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_last_version(config: *mut TariCommsConfig, error_out: *mut c_int) -> *mut c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if config.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("config".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let sql_database_path = (*config)
        .datastore_path
        .join((*config).peer_database_name.clone())
        .with_extension("sqlite3");
    match get_last_version(sql_database_path) {
        Ok(None) => ptr::null_mut(),
        Ok(Some(version)) => {
            let version = CString::new(version).expect("failed to initialize CString");
            version.into_raw()
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Retrieves the network of an app that last accessed the wallet database
///
/// ## Arguments
/// `config` - The TariCommsConfig pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
/// ## Returns
/// `*mut c_char` - Returns the pointer to the hexadecimal representation of the signature and
///
/// # Safety
/// The ```string_destroy``` method must be called when finished with a string coming from rust to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_last_network(config: *mut TariCommsConfig, error_out: *mut c_int) -> *mut c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if config.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("config".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let sql_database_path = (*config)
        .datastore_path
        .join((*config).peer_database_name.clone())
        .with_extension("sqlite3");
    match get_last_network(sql_database_path) {
        Ok(None) => ptr::null_mut(),
        Ok(Some(network)) => {
            let network = CString::new(network).expect("failed to initialize CString");
            network.into_raw()
        },
        Err(e) => {
            error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Retrieves the balance from a wallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
/// ## Returns
/// `*mut Balance` - Returns the pointer to the TariBalance or null if error occurs
///
/// # Safety
/// The ```balance_destroy``` method must be called when finished with a TariBalance to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_balance(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariBalance {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let balance = (*wallet)
        .runtime
        .block_on((*wallet).wallet.output_manager_service.get_balance());
    match balance {
        Ok(balance) => Box::into_raw(Box::new(balance)),
        Err(_) => {
            error = LibWalletError::from(InterfaceError::BalanceError).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// This function returns a list of unspent UTXO values and commitments.
///
/// ## Arguments
/// * `wallet` - The TariWallet pointer,
/// * `page` - Page offset,
/// * `page_size` - A number of items per page,
/// * `sorting` - An enum representing desired sorting,
/// * `dust_threshold` - A value filtering threshold. Outputs whose values are <= `dust_threshold` are not listed in the
/// result.
/// * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null.
/// Functions as an out parameter.
///
/// ## Returns
/// `*mut TariVector` - Returns a struct with an array pointer, length and capacity (needed for proper destruction
/// after use).
///
/// # Safety
/// `destroy_tari_vector()` must be called after use.
/// Items that fail to produce `.as_transaction_output()` are omitted from the list and a `warn!()` message is logged to
/// LOG_TARGET.
// casting here is okay as we wont have more than u32 utxos
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn wallet_get_utxos(
    wallet: *mut TariWallet,
    page: usize,
    page_size: usize,
    sorting: TariUtxoSort,
    states: *mut TariVector,
    dust_threshold: u64,
    error_ptr: *mut i32,
) -> *mut TariVector {
    if wallet.is_null() {
        error!(target: LOG_TARGET, "wallet pointer is null");
        ptr::replace(
            error_ptr,
            LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code,
        );
        return ptr::null_mut();
    }

    let page = i64::from_usize(page).unwrap_or(i64::MAX);
    let page_size = i64::from_usize(page_size).unwrap_or(i64::MAX);
    let dust_threshold = i64::from_u64(dust_threshold).unwrap_or(0);

    let status = {
        if states.is_null() {
            vec![]
        } else {
            Vec::from_raw_parts((*states).ptr as *mut u64, (*states).len, (*states).cap)
                .into_iter()
                .map(|x| OutputStatus::try_from(x as i32).unwrap())
                .collect_vec()
        }
    };

    use SortDirection::{Asc, Desc};
    let q = OutputBackendQuery {
        tip_height: i64::MAX,
        status,
        commitments: vec![],
        pagination: Some((page, page_size)),
        value_min: Some((dust_threshold, false)),
        value_max: None,
        sorting: vec![match sorting {
            TariUtxoSort::MinedHeightAsc => ("mined_height", Asc),
            TariUtxoSort::MinedHeightDesc => ("mined_height", Desc),
            TariUtxoSort::ValueAsc => ("value", Asc),
            TariUtxoSort::ValueDesc => ("value", Desc),
        }],
    };

    match (*wallet).wallet.output_db.fetch_outputs_by_query(q) {
        Ok(outputs) => {
            ptr::replace(error_ptr, 0);
            Box::into_raw(Box::new(TariVector::from(outputs)))
        },

        Err(e) => {
            error!(target: LOG_TARGET, "failed to obtain outputs: {:#?}", e);
            ptr::replace(
                error_ptr,
                LibWalletError::from(WalletError::OutputManagerError(
                    OutputManagerError::OutputManagerStorageError(e),
                ))
                .code,
            );
            ptr::null_mut()
        },
    }
}

/// This function returns a list of all UTXO values, commitment's hex values and states.
///
/// ## Arguments
/// * `wallet` - The TariWallet pointer,
/// * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null.
/// Functions as an out parameter.
///
/// ## Returns
/// `*mut TariVector` - Returns a struct with an array pointer, length and capacity (needed for proper destruction
/// after use).
///
/// ## States
/// 0 - Unspent
/// 1 - Spent
/// 2 - EncumberedToBeReceived
/// 3 - EncumberedToBeSpent
/// 4 - Invalid
/// 5 - CancelledInbound
/// 6 - UnspentMinedUnconfirmed
/// 7 - ShortTermEncumberedToBeReceived
/// 8 - ShortTermEncumberedToBeSpent
/// 9 - SpentMinedUnconfirmed
/// 10 - AbandonedCoinbase
/// 11 - NotStored
///
/// # Safety
/// `destroy_tari_vector()` must be called after use.
/// Items that fail to produce `.as_transaction_output()` are omitted from the list and a `warn!()` message is logged to
/// LOG_TARGET.
#[no_mangle]
pub unsafe extern "C" fn wallet_get_all_utxos(wallet: *mut TariWallet, error_ptr: *mut i32) -> *mut TariVector {
    if wallet.is_null() {
        error!(target: LOG_TARGET, "wallet pointer is null");
        ptr::replace(
            error_ptr,
            LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code,
        );
        return ptr::null_mut();
    }

    let q = OutputBackendQuery {
        tip_height: i64::MAX,
        status: vec![],
        commitments: vec![],
        pagination: None,
        value_min: None,
        value_max: None,
        sorting: vec![],
    };

    match (*wallet).wallet.output_db.fetch_outputs_by_query(q) {
        Ok(outputs) => {
            ptr::replace(error_ptr, 0);
            Box::into_raw(Box::new(TariVector::from(outputs)))
        },

        Err(e) => {
            error!(target: LOG_TARGET, "failed to obtain outputs: {:#?}", e);
            ptr::replace(
                error_ptr,
                LibWalletError::from(WalletError::OutputManagerError(
                    OutputManagerError::OutputManagerStorageError(e),
                ))
                .code,
            );
            ptr::null_mut()
        },
    }
}

/// This function will tell the wallet to do a coin split.
///
/// ## Arguments
/// * `wallet` - The TariWallet pointer
/// * `commitments` - A `TariVector` of "strings", tagged as `TariTypeTag::String`, containing commitment's hex values
///   (see `Commitment::to_hex()`)
/// * `number_of_splits` - The number of times to split the amount
/// * `fee_per_gram` - The transaction fee
/// * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null.
/// Functions as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns the transaction id.
///
/// # Safety
/// `TariVector` must be freed after use with `destroy_tari_vector()`
#[no_mangle]
pub unsafe extern "C" fn wallet_coin_split(
    wallet: *mut TariWallet,
    commitments: *mut TariVector,
    number_of_splits: usize,
    fee_per_gram: u64,
    error_ptr: *mut i32,
) -> u64 {
    if wallet.is_null() {
        error!(target: LOG_TARGET, "wallet pointer is null");
        ptr::replace(
            error_ptr,
            LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code as c_int,
        );
        return 0;
    }

    let commitments = match commitments.as_ref() {
        None => {
            error!(target: LOG_TARGET, "failed to obtain commitments as reference");
            ptr::replace(
                error_ptr,
                LibWalletError::from(InterfaceError::NullError("commitments vector".to_string())).code as c_int,
            );
            return 0;
        },
        Some(cs) => match cs.to_commitment_vec() {
            Ok(cs) => cs,
            Err(e) => {
                error!(target: LOG_TARGET, "failed to convert from tari vector: {:?}", e);
                ptr::replace(error_ptr, LibWalletError::from(e).code as c_int);
                return 0;
            },
        },
    };

    match (*wallet).runtime.block_on((*wallet).wallet.coin_split_even(
        commitments,
        number_of_splits,
        MicroMinotari(fee_per_gram),
        String::new(),
    )) {
        Ok(tx_id) => {
            ptr::replace(error_ptr, 0);
            tx_id.as_u64()
        },
        Err(e) => {
            error!(target: LOG_TARGET, "failed to join outputs: {:#?}", e);
            ptr::replace(error_ptr, LibWalletError::from(e).code);
            0
        },
    }
}

/// This function will tell the wallet to do a coin join, resulting in a new coin worth a sum of the joined coins minus
/// the fee.
///
/// ## Arguments
/// * `wallet` - The TariWallet pointer
/// * `commitments` - A `TariVector` of "strings", tagged as `TariTypeTag::String`, containing commitment's hex values
///   (see `Commitment::to_hex()`)
/// * `fee_per_gram` - The transaction fee
/// * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null.
/// Functions as an out parameter.
///
/// ## Returns
/// `TariVector` - Returns the transaction id.
///
/// # Safety
/// `TariVector` must be freed after use with `destroy_tari_vector()`
#[no_mangle]
pub unsafe extern "C" fn wallet_coin_join(
    wallet: *mut TariWallet,
    commitments: *mut TariVector,
    fee_per_gram: u64,
    error_ptr: *mut i32,
) -> u64 {
    if wallet.is_null() {
        error!(target: LOG_TARGET, "wallet pointer is null");
        ptr::replace(
            error_ptr,
            LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code as c_int,
        );
        return 0;
    }

    let commitments = match commitments.as_ref() {
        None => {
            error!(target: LOG_TARGET, "failed to obtain commitments as reference");
            ptr::replace(
                error_ptr,
                LibWalletError::from(InterfaceError::NullError("commitments vector".to_string())).code as c_int,
            );
            return 0;
        },
        Some(cs) => match cs.to_commitment_vec() {
            Ok(cs) => cs,
            Err(e) => {
                error!(target: LOG_TARGET, "failed to convert from tari vector: {:?}", e);
                ptr::replace(error_ptr, LibWalletError::from(e).code as c_int);
                return 0;
            },
        },
    };

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.coin_join(commitments, fee_per_gram.into(), None))
    {
        Ok(tx_id) => {
            ptr::replace(error_ptr, 0);
            tx_id.as_u64()
        },

        Err(e) => {
            error!(target: LOG_TARGET, "failed to join outputs: {:#?}", e);
            ptr::replace(error_ptr, LibWalletError::from(e).code);
            0
        },
    }
}

/// This function will tell what the outcome of a coin join would be.
///
/// ## Arguments
/// * `wallet` - The TariWallet pointer
/// * `commitments` - A `TariVector` of "strings", tagged as `TariTypeTag::String`, containing commitment's hex values
///   (see `Commitment::to_hex()`)
/// * `fee_per_gram` - The transaction fee
/// * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null.
///   Functions as an out parameter.
///
/// ## Returns
/// `*mut TariCoinPreview` - A struct with expected output values and the fee.
///
/// # Safety
/// `TariVector` must be freed after use with `destroy_tari_vector()`
#[no_mangle]
pub unsafe extern "C" fn wallet_preview_coin_join(
    wallet: *mut TariWallet,
    commitments: *mut TariVector,
    fee_per_gram: u64,
    error_ptr: *mut i32,
) -> *mut TariCoinPreview {
    if wallet.is_null() {
        error!(target: LOG_TARGET, "wallet pointer is null");
        ptr::replace(
            error_ptr,
            LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code as c_int,
        );
        return ptr::null_mut();
    }

    let commitments = match commitments.as_ref() {
        None => {
            error!(target: LOG_TARGET, "failed to obtain commitments as reference");
            ptr::replace(
                error_ptr,
                LibWalletError::from(InterfaceError::NullError("commitments vector".to_string())).code as c_int,
            );
            return ptr::null_mut();
        },
        Some(cs) => match cs.to_commitment_vec() {
            Ok(cs) => cs,
            Err(e) => {
                error!(target: LOG_TARGET, "failed to convert from tari vector: {:?}", e);
                ptr::replace(error_ptr, LibWalletError::from(e).code as c_int);
                return ptr::null_mut();
            },
        },
    };

    match (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .preview_coin_join_with_commitments(commitments, MicroMinotari(fee_per_gram)),
    ) {
        Ok((expected_outputs, fee)) => {
            ptr::replace(error_ptr, 0);
            let mut expected_outputs = ManuallyDrop::new(expected_outputs);

            Box::into_raw(Box::new(TariCoinPreview {
                expected_outputs: Box::into_raw(Box::new(TariVector {
                    tag: TariTypeTag::U64,
                    len: expected_outputs.len(),
                    cap: expected_outputs.capacity(),
                    ptr: expected_outputs.as_mut_ptr() as *mut c_void,
                })),
                fee: fee.as_u64(),
            }))
        },
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "failed to preview coin join with commitments: {:#?}", e
            );
            ptr::replace(error_ptr, LibWalletError::from(e).code);
            ptr::null_mut()
        },
    }
}

/// This function will tell what the outcome of a coin split would be.
///
/// ## Arguments
/// * `wallet` - The TariWallet pointer
/// * `commitments` - A `TariVector` of "strings", tagged as `TariTypeTag::String`, containing commitment's hex values
///   (see `Commitment::to_hex()`)
/// * `number_of_splits` - The number of times to split the amount
/// * `fee_per_gram` - The transaction fee
/// * `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null.
///   Functions as an out parameter.
///
/// ## Returns
/// `*mut TariCoinPreview` - A struct with expected output values and the fee.
///
/// # Safety
/// `TariVector` must be freed after use with `destroy_tari_vector()`
#[no_mangle]
pub unsafe extern "C" fn wallet_preview_coin_split(
    wallet: *mut TariWallet,
    commitments: *mut TariVector,
    number_of_splits: usize,
    fee_per_gram: u64,
    error_ptr: *mut i32,
) -> *mut TariCoinPreview {
    if wallet.is_null() {
        error!(target: LOG_TARGET, "wallet pointer is null");
        ptr::replace(
            error_ptr,
            LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code as c_int,
        );
        return ptr::null_mut();
    }

    let commitments = match commitments.as_ref() {
        None => {
            error!(target: LOG_TARGET, "failed to obtain commitments as reference");
            ptr::replace(
                error_ptr,
                LibWalletError::from(InterfaceError::NullError("commitments vector".to_string())).code as c_int,
            );
            return ptr::null_mut();
        },
        Some(cs) => match cs.to_commitment_vec() {
            Ok(cs) => cs,
            Err(e) => {
                error!(target: LOG_TARGET, "failed to convert from tari vector: {:?}", e);
                ptr::replace(error_ptr, LibWalletError::from(e).code as c_int);
                return ptr::null_mut();
            },
        },
    };

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.preview_coin_split_with_commitments_no_amount(
            commitments,
            number_of_splits,
            MicroMinotari(fee_per_gram),
        )) {
        Ok((expected_outputs, fee)) => {
            ptr::replace(error_ptr, 0);
            let mut expected_outputs = ManuallyDrop::new(expected_outputs);

            Box::into_raw(Box::new(TariCoinPreview {
                expected_outputs: Box::into_raw(Box::new(TariVector {
                    tag: TariTypeTag::U64,
                    len: expected_outputs.len(),
                    cap: expected_outputs.capacity(),
                    ptr: expected_outputs.as_mut_ptr() as *mut c_void,
                })),
                fee: fee.as_u64(),
            }))
        },
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "failed to preview split with commitments outputs (no amount): {:#?}", e
            );
            ptr::replace(error_ptr, LibWalletError::from(e).code);
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
) -> *mut c_char {
    let mut error = 0;
    let mut result = CString::new("").expect("Blank CString will not fail.");

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

    let secret = (*wallet).wallet.comms.node_identity().secret_key().clone();
    let message = CStr::from_ptr(msg)
        .to_str()
        .expect("CString should not fail here.")
        .to_owned();

    let signature = (*wallet).wallet.sign_message(&secret, &message);

    match signature {
        Ok(s) => {
            let hex_sig = s.get_signature().to_hex();
            let hex_nonce = s.get_public_nonce().to_hex();
            let hex_return = format!("{}|{}", hex_sig, hex_nonce);
            result = CString::new(hex_return).expect("CString should not fail here.");
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
) -> bool {
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

    let message = match CStr::from_ptr(msg).to_str() {
        Ok(v) => v.to_owned(),
        _ => {
            error = LibWalletError::from(InterfaceError::PointerError("msg".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return false;
        },
    };
    let hex = match CStr::from_ptr(hex_sig_nonce).to_str() {
        Ok(v) => v.to_owned(),
        _ => {
            error = LibWalletError::from(InterfaceError::PointerError("hex_sig_nonce".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return false;
        },
    };
    let hex_keys: Vec<&str> = hex.split('|').collect();
    if hex_keys.len() != 2 {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return result;
    }

    if let Some(key1) = hex_keys.first() {
        if let Some(key2) = hex_keys.get(1) {
            let secret = TariPrivateKey::from_hex(key1);
            match secret {
                Ok(p) => {
                    let public_nonce = TariPublicKey::from_hex(key2);
                    match public_nonce {
                        Ok(pn) => {
                            let sig = SignatureWithDomain::<WalletMessageSigningDomain>::new(pn, p);
                            result = (*wallet).wallet.verify_message_signature(&*public_key, &sig, &message)
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
        } else {
            error = LibWalletError::from(InterfaceError::InvalidArgument("hex_sig_nonce".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
        }
    } else {
        error = LibWalletError::from(InterfaceError::InvalidArgument("hex_sig_nonce".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    result
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
pub unsafe extern "C" fn wallet_set_base_node_peer(
    wallet: *mut TariWallet,
    public_key: *mut TariPublicKey,
    address: *const c_char,
    error_out: *mut c_int,
) -> bool {
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

    let parsed_addr = if address.is_null() {
        None
    } else {
        match CStr::from_ptr(address).to_str() {
            Ok(v) => match Multiaddr::from_str(v) {
                Ok(v) => Some(v),
                Err(_) => {
                    error =
                        LibWalletError::from(InterfaceError::InvalidArgument("address is invalid".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    return false;
                },
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("address".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return false;
            },
        }
    };

    if let Err(e) = (*wallet)
        .runtime
        .block_on((*wallet).wallet.set_base_node_peer((*public_key).clone(), parsed_addr))
    {
        error = LibWalletError::from(e).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    true
}
/// Gets all seed peers known by the wallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `TariPublicKeys` - Returns a list of all known public keys
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_get_seed_peers(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut TariPublicKeys {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let peer_manager = (*wallet).wallet.comms.peer_manager();
    let query = PeerQuery::new().select_where(|p| p.is_seed());
    #[allow(clippy::blocks_in_conditions)]
    match (*wallet).runtime.block_on(async move {
        let peers = peer_manager.perform_query(query).await?;
        let mut public_keys = Vec::with_capacity(peers.len());
        for peer in peers {
            public_keys.push(peer.public_key);
        }
        Result::<_, WalletError>::Ok(public_keys)
    }) {
        Ok(public_keys) => Box::into_raw(Box::new(TariPublicKeys(public_keys))),
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
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
) -> bool {
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
) -> bool {
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
            .remove_contact((*contact).address.clone()),
    ) {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(WalletError::ContactsServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// Gets the available balance from a TariBalance. This is the balance the user can spend.
///
/// ## Arguments
/// `balance` - The TariBalance pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The available balance, 0 if wallet is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn balance_get_available(balance: *mut TariBalance, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if balance.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("balance".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    c_ulonglong::from((*balance).available_balance)
}

/// Gets the time locked balance from a TariBalance. This is the balance the user can spend.
///
/// ## Arguments
/// `balance` - The TariBalance pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The time locked balance, 0 if wallet is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn balance_get_time_locked(balance: *mut TariBalance, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if balance.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("balance".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    let b = if let Some(bal) = (*balance).time_locked_balance {
        bal
    } else {
        MicroMinotari::from(0)
    };
    c_ulonglong::from(b)
}

/// Gets the pending incoming balance from a TariBalance. This is the balance the user can spend.
///
/// ## Arguments
/// `balance` - The TariBalance pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The pending incoming, 0 if wallet is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn balance_get_pending_incoming(balance: *mut TariBalance, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if balance.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("balance".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    c_ulonglong::from((*balance).pending_incoming_balance)
}

/// Gets the pending outgoing balance from a TariBalance. This is the balance the user can spend.
///
/// ## Arguments
/// `balance` - The TariBalance pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The pending outgoing balance, 0 if wallet is null
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn balance_get_pending_outgoing(balance: *mut TariBalance, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if balance.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("balance".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    c_ulonglong::from((*balance).pending_outgoing_balance)
}

/// Frees memory for a TariBalance
///
/// ## Arguments
/// `balance` - The pointer to a TariBalance
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn balance_destroy(balance: *mut TariBalance) {
    if !balance.is_null() {
        drop(Box::from_raw(balance))
    }
}

/// Sends a TariPendingOutboundTransaction
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `destination` - The TariWalletAddress pointer of the peer
/// `amount` - The amount
/// `commitments` - A `TariVector` of "strings", tagged as `TariTypeTag::String`, containing commitment's hex values
///   (see `Commitment::to_hex()`)
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
    destination: *mut TariWalletAddress,
    amount: c_ulonglong,
    commitments: *mut TariVector,
    fee_per_gram: c_ulonglong,
    message: *const c_char,
    one_sided: bool,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    if destination.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("dest_public_key".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    let selection_criteria = match commitments.as_ref() {
        None => UtxoSelectionCriteria::default(),
        Some(cs) => match cs.to_commitment_vec() {
            Ok(cs) => UtxoSelectionCriteria::specific(cs),
            Err(e) => {
                error!(target: LOG_TARGET, "failed to convert from tari vector: {:?}", e);
                ptr::replace(error_out, LibWalletError::from(e).code as c_int);
                return 0;
            },
        },
    };

    let message_string;
    if message.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        message_string = CString::new("")
            .expect("Blank CString will not fail")
            .to_str()
            .expect("CString.to_str() will not fail")
            .to_owned();
    } else {
        match CStr::from_ptr(message).to_str() {
            Ok(v) => {
                message_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::NullError("message".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                message_string = CString::new("")
                    .expect("Blank CString will not fail")
                    .to_str()
                    .expect("CString.to_str() will not fail")
                    .to_owned();
            },
        }
    };

    if one_sided {
        match (*wallet).runtime.block_on(
            (*wallet)
                .wallet
                .transaction_service
                .send_one_sided_to_stealth_address_transaction(
                    (*destination).clone(),
                    MicroMinotari::from(amount),
                    selection_criteria,
                    OutputFeatures::default(),
                    MicroMinotari::from(fee_per_gram),
                    message_string,
                ),
        ) {
            Ok(tx_id) => tx_id.as_u64(),
            Err(e) => {
                error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                0
            },
        }
    } else {
        match (*wallet)
            .runtime
            .block_on((*wallet).wallet.transaction_service.send_transaction(
                (*destination).clone(),
                MicroMinotari::from(amount),
                selection_criteria,
                OutputFeatures::default(),
                MicroMinotari::from(fee_per_gram),
                message_string,
            )) {
            Ok(tx_id) => tx_id.as_u64(),
            Err(e) => {
                error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                0
            },
        }
    }
}

/// Gets a fee estimate for an amount
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `amount` - The amount
/// `commitments` - A `TariVector` of "strings", tagged as `TariTypeTag::String`, containing commitment's hex values
///   (see `Commitment::to_hex()`)
/// `fee_per_gram` - The fee per gram
/// `num_kernels` - The number of transaction kernels
/// `num_outputs` - The number of outputs
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `unsigned long long` - Returns 0 if unsuccessful or the fee estimate in MicroMinotari if successful
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_get_fee_estimate(
    wallet: *mut TariWallet,
    amount: c_ulonglong,
    commitments: *mut TariVector,
    fee_per_gram: c_ulonglong,
    num_kernels: c_uint,
    num_outputs: c_uint,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    let selection_criteria = match commitments.as_ref() {
        None => UtxoSelectionCriteria::default(),
        Some(cs) => match cs.to_commitment_vec() {
            Ok(cs) => UtxoSelectionCriteria::specific(cs),
            Err(e) => {
                error!(target: LOG_TARGET, "failed to convert from tari vector: {:?}", e);
                ptr::replace(error_out, LibWalletError::from(e).code as c_int);
                return 0;
            },
        },
    };

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.output_manager_service.fee_estimate(
            MicroMinotari::from(amount),
            selection_criteria,
            MicroMinotari::from(fee_per_gram),
            num_kernels as usize,
            num_outputs as usize,
        )) {
        Ok(fee) => fee.into(),
        Err(e) => {
            error = LibWalletError::from(WalletError::OutputManagerError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Gets the number of mining confirmations required
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `unsigned long long` - Returns the number of confirmations required
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_get_num_confirmations_required(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.get_num_confirmations_required())
    {
        Ok(num) => num,
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Sets the number of mining confirmations required
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `num` - The number of confirmations to require
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_set_num_confirmations_required(
    wallet: *mut TariWallet,
    num: c_ulonglong,
    error_out: *mut c_int,
) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int)
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.set_num_confirmations_required(num))
    {
        Ok(()) => (),
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int)
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
) -> *mut TariCompletedTransactions {
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
                .filter(|ct| ct.status != TransactionStatus::Imported)
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
) -> *mut TariPendingInboundTransactions {
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
                    .filter(|ct| {
                        ct.status == TransactionStatus::Completed ||
                            ct.status == TransactionStatus::Broadcast ||
                            ct.status == TransactionStatus::Imported
                    })
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
) -> *mut TariPendingOutboundTransactions {
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
) -> *mut TariCompletedTransactions {
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
    let wallet_address = TariAddress::new(
        (*wallet).wallet.comms.node_identity().public_key().clone(),
        (*wallet).wallet.network.as_network(),
    );
    for tx in inbound_transactions.values() {
        let mut inbound_tx = CompletedTransaction::from(tx.clone());
        inbound_tx.destination_address = wallet_address.clone();
        completed.push(inbound_tx);
    }
    for tx in outbound_transactions.values() {
        let mut outbound_tx = CompletedTransaction::from(tx.clone());
        outbound_tx.source_address = wallet_address.clone();
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
) -> *mut TariCompletedTransaction {
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
            if let Some(tx) = completed_transactions.get(&TxId::from(transaction_id)) {
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
) -> *mut TariPendingInboundTransaction {
    let mut error = 0;
    let transaction_id = TxId::from(transaction_id);
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
) -> *mut TariPendingOutboundTransaction {
    let mut error = 0;
    let transaction_id = TxId::from(transaction_id);
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
) -> *mut TariCompletedTransaction {
    let mut error = 0;
    let transaction_id = TxId::from(transaction_id);
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
        let network = (*wallet).wallet.network.as_network();
        let address = TariWalletAddress::new((*wallet).wallet.comms.node_identity().public_key().clone(), network);
        if let Some(tx) = outbound_transactions.remove(&transaction_id) {
            let mut outbound_tx = CompletedTransaction::from(tx);
            outbound_tx.source_address = address;
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
                inbound_tx.destination_address = address;
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

/// Get the TariWalletAddress from a TariWallet
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariWalletAddress` - returns the address, note that ptr::null_mut() is returned
/// if wc is null
///
/// # Safety
/// The ```tari_address_destroy``` method must be called when finished with a TariWalletAddress to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn wallet_get_tari_address(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> *mut TariWalletAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let network = (*wallet).wallet.network.as_network();
    let pk = (*wallet).wallet.comms.node_identity().public_key().clone();
    let address = TariWalletAddress::new(pk, network);
    Box::into_raw(Box::new(address))
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
) -> bool {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    match (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .transaction_service
            .cancel_transaction(TxId::from(transaction_id)),
    ) {
        Ok(_) => true,
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// This function will tell the wallet to query the set base node to confirm the status of transaction outputs
/// (TXOs).
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
pub unsafe extern "C" fn wallet_start_txo_validation(wallet: *mut TariWallet, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    if let Err(e) = (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .store_and_forward_requester
            .request_saf_messages_from_neighbours(),
    ) {
        error = LibWalletError::from(e).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.output_manager_service.validate_txos())
    {
        Ok(request_key) => request_key,
        Err(e) => {
            error = LibWalletError::from(WalletError::OutputManagerError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// This function will tell the wallet to query the set base node to confirm the status of mined transactions.
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
pub unsafe extern "C" fn wallet_start_transaction_validation(
    wallet: *mut TariWallet,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    if let Err(e) = (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .store_and_forward_requester
            .request_saf_messages_from_neighbours(),
    ) {
        error = LibWalletError::from(e).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.validate_transactions())
    {
        Ok(request_key) => request_key.as_u64(),
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// This function will tell the wallet retart any broadcast protocols for completed transactions. Ideally this should be
/// called after a successfuly Transaction Validation is complete
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` -  Returns a boolean value indicating if the launch was success or not.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_restart_transaction_broadcast(wallet: *mut TariWallet, error_out: *mut c_int) -> bool {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    if let Err(e) = (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .store_and_forward_requester
            .request_saf_messages_from_neighbours(),
    ) {
        error = LibWalletError::from(e).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    match (*wallet)
        .runtime
        .block_on((*wallet).wallet.transaction_service.restart_broadcast_protocols())
    {
        Ok(()) => true,
        Err(e) => {
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
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

    match (*wallet).wallet.get_seed_words(&MnemonicLanguage::English) {
        Ok(seed_words) => Box::into_raw(Box::new(TariSeedWords(seed_words))),
        Err(e) => {
            error = LibWalletError::from(e).code;
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
) -> bool {
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
        match CStr::from_ptr(key).to_str() {
            Ok(v) => {
                key_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("key".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return false;
            },
        }
    }

    let value_string;
    if value.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("value".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    } else {
        match CStr::from_ptr(value).to_str() {
            Ok(v) => {
                value_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("value".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return false;
            },
        }
    }

    match (*wallet).wallet.db.set_client_key_value(key_string, value_string) {
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
) -> *mut c_char {
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
        match CStr::from_ptr(key).to_str() {
            Ok(v) => {
                key_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("key".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }

    match (*wallet).wallet.db.get_client_key_value(key_string) {
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
) -> bool {
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
        match CStr::from_ptr(key).to_str() {
            Ok(v) => {
                key_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("key".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return false;
            },
        }
    }

    match (*wallet).wallet.db.clear_client_value(key_string) {
        Ok(result) => result,
        Err(e) => {
            error = LibWalletError::from(WalletError::WalletStorageError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// Check if a Wallet has the data of an In Progress Recovery in its database.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Return a boolean value indicating whether there is an in progress recovery or not. An error will also
/// result in a false result.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_is_recovery_in_progress(wallet: *mut TariWallet, error_out: *mut c_int) -> bool {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    match (*wallet).wallet.is_recovery_in_progress() {
        Ok(result) => result,
        Err(e) => {
            error = LibWalletError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// Starts the Wallet recovery process.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `base_node_public_key` - The TariPublicKey pointer of the Base Node the recovery process will use
/// `recovery_progress_callback` - The callback function pointer that will be used to asynchronously communicate
/// progress to the client. The first argument of the callback is an event enum encoded as a u8 as follows:
/// ```
/// enum RecoveryEvent {
///     ConnectingToBaseNode,       // 0
///     ConnectedToBaseNode,        // 1
///     ConnectionToBaseNodeFailed, // 2
///     Progress,                   // 3
///     Completed,                  // 4
///     ScanningRoundFailed,        // 5
///     RecoveryFailed,             // 6
/// }
/// ```
/// The second and third arguments are u64 values that will contain different information depending on the event
/// that triggered the callback. The meaning of the second and third argument for each event are as follows:
///     - ConnectingToBaseNode, 0, 0
///     - ConnectedToBaseNode, 0, 1
///     - ConnectionToBaseNodeFailed, number of retries, retry limit
///     - Progress, current block, total number of blocks
///     - Completed, total number of UTXO's recovered, MicroMinotari recovered,
///     - ScanningRoundFailed, number of retries, retry limit
///     - RecoveryFailed, 0, 0
///
/// If connection to a base node is successful the flow of callbacks should be:
///     - The process will start with a callback with `ConnectingToBaseNode` showing a connection is being attempted
///       this could be repeated multiple times until a connection is made.
///     - The next a callback with `ConnectedToBaseNode` indicate a successful base node connection and process has
///       started
///     - In Progress callbacks will be of the form (n, m) where n < m
///     - If the process completed successfully then the final `Completed` callback will return how many UTXO's were
///       scanned and how much MicroMinotari was recovered
///     - If there is an error in the connection process then the `ConnectionToBaseNodeFailed` will be returned
///     - If there is a minor error in scanning then `ScanningRoundFailed` will be returned and another connection/sync
///       attempt will be made
///     - If a unrecoverable error occurs the `RecoveryFailed` event will be returned and the client will need to start
///       a new process.
///
/// `recovered_output_message` - A string that will be used as the message for any recovered outputs. If Null the
/// default     message will be used
///
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `bool` - Return a boolean value indicating whether the process started successfully or not, the process will
/// continue to run asynchronously and communicate it progress via the callback. An error will also produce a false
/// result.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn wallet_start_recovery(
    wallet: *mut TariWallet,
    base_node_public_key: *mut TariPublicKey,
    recovery_progress_callback: unsafe extern "C" fn(u8, u64, u64),
    recovered_output_message: *const c_char,
    error_out: *mut c_int,
) -> bool {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    let shutdown_signal = (*wallet).shutdown.to_signal();
    let peer_public_keys: Vec<TariPublicKey> = vec![(*base_node_public_key).clone()];
    let mut recovery_task_builder = UtxoScannerService::<WalletSqliteDatabase, WalletConnectivityHandle>::builder();

    if !recovered_output_message.is_null() {
        let message_str = match CStr::from_ptr(recovered_output_message).to_str() {
            Ok(v) => v.to_owned(),
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("recovered_output_message".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return false;
            },
        };
        recovery_task_builder.with_recovery_message(message_str);
    }

    let mut recovery_task = recovery_task_builder
        .with_peers(peer_public_keys)
        .with_retry_limit(10)
        .build_with_wallet(&(*wallet).wallet, shutdown_signal);

    let event_stream = recovery_task.get_event_receiver();
    let recovery_join_handle = (*wallet).runtime.spawn(recovery_task.run());

    // Spawn a task to monitor the recovery process events and call the callback appropriately
    (*wallet).runtime.spawn(recovery_event_monitoring(
        event_stream,
        recovery_join_handle,
        recovery_progress_callback,
    ));

    true
}

/// Set the text message that is applied to a detected One-Side payment transaction when it is scanned from the
/// blockchain
///
/// ## Arguments
/// `wallet` - The TariWallet pointer.
/// `message` - The pointer to a Utf8 string representing the Message
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
pub unsafe extern "C" fn wallet_set_one_sided_payment_message(
    wallet: *mut TariWallet,
    message: *const c_char,
    error_out: *mut c_int,
) -> bool {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    let message_string;
    if message.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    } else {
        match CStr::from_ptr(message).to_str() {
            Ok(v) => {
                message_string = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("message".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return false;
            },
        }
    }

    (*wallet)
        .wallet
        .utxo_scanner_service
        .set_one_sided_payment_message(message_string);

    true
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
    for emoji in &current_emoji_set {
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
// casting here is okay as emoji set wont get larger than u32
#[allow(clippy::cast_possible_truncation)]
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
) -> *mut ByteVector {
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
        drop(Box::from_raw(emoji_set))
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
    debug!(target: LOG_TARGET, "Wallet destroy called");
    if !wallet.is_null() {
        debug!(target: LOG_TARGET, "Wallet pointer not yet destroyed, shutting down now");
        let mut w = Box::from_raw(wallet);
        let wallet_comms = w.wallet.comms.clone();
        w.shutdown.trigger();
        w.runtime.block_on(w.wallet.wait_until_shutdown());
        // The wallet should be shutdown by now; these are just additional confirmations
        loop {
            if w.shutdown.is_triggered() &&
                wallet_comms.shutdown_signal().is_triggered() &&
                w.runtime
                    .block_on(wallet_comms.connectivity().get_connectivity_status())
                    .is_err()
            {
                break;
            };
            w.runtime
                .block_on(async { tokio::time::sleep(Duration::from_millis(250)).await });
        }
    }
}

/// This function will log the provided string at debug level. To be used to have a client log messages to the LibWallet
/// logs.
///
/// ## Arguments
/// `msg` - A string that will be logged at the debug level. If msg is null nothing will be done.
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn log_debug_message(msg: *const c_char, error_out: *mut c_int) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let message;
    if !msg.is_null() {
        match CStr::from_ptr(msg).to_str() {
            Ok(v) => {
                message = v.to_owned();
            },
            _ => {
                error = LibWalletError::from(InterfaceError::PointerError("msg".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return;
            },
        }
        debug!(target: LOG_TARGET, "{}", message);
    }
}

/// ------------------------------------- FeePerGramStats ------------------------------------ ///

/// Get the TariFeePerGramStats from a TariWallet.
///
/// ## Arguments
/// `wallet` - The TariWallet pointer
/// `count` - The maximum number of blocks to be checked
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter
///
/// ## Returns
/// `*mut TariCompletedTransactions` - returns the transactions, note that it returns ptr::null_mut() if
/// wallet is null or an error is encountered.
///
/// # Safety
/// The ```fee_per_gram_stats_destroy``` method must be called when finished with a TariFeePerGramStats to prevent
/// a memory leak.
#[no_mangle]
pub unsafe extern "C" fn wallet_get_fee_per_gram_stats(
    wallet: *mut TariWallet,
    count: c_uint,
    error_out: *mut c_int,
) -> *mut TariFeePerGramStats {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    match (*wallet).runtime.block_on(
        (*wallet)
            .wallet
            .transaction_service
            .get_fee_per_gram_stats_per_block(count as usize),
    ) {
        Ok(estimates) => Box::into_raw(Box::new(estimates)),
        Err(e) => {
            error!(target: LOG_TARGET, "Error getting the fee estimates: {:?}", e);
            error = LibWalletError::from(WalletError::TransactionServiceError(e)).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Get length of stats from the TariFeePerGramStats.
///
/// ## Arguments
/// `fee_per_gram_stats` - The pointer to a TariFeePerGramStats
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter
///
/// ## Returns
/// `c_uint` - length of stats in TariFeePerGramStats
///
/// # Safety
/// None
// casting here is okay as fee per gram stats cannot get larger than u32
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn fee_per_gram_stats_get_length(
    fee_per_gram_stats: *mut TariFeePerGramStats,
    error_out: *mut c_int,
) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut len = 0;
    if fee_per_gram_stats.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("fee_per_gram_stats".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        len = (*fee_per_gram_stats).stats.len();
    }
    len as c_uint
}

/// Get TariFeePerGramStat at position from the TariFeePerGramStats.
///
/// ## Arguments
/// `fee_per_gram_stats` - The pointer to a TariFeePerGramStats.
/// `position` - The integer position.
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TariCompletedTransactions` - returns the TariFeePerGramStat, note that it returns ptr::null_mut() if
/// fee_per_gram_stats is null or an error is encountered.
///
/// # Safety
/// The ```fee_per_gram_stat_destroy``` method must be called when finished with a TariCompletedTransactions to 4prevent
/// a memory leak.
#[no_mangle]
pub unsafe extern "C" fn fee_per_gram_stats_get_at(
    fee_per_gram_stats: *mut TariFeePerGramStats,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariFeePerGramStat {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if fee_per_gram_stats.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("fee_per_gram_stats".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    let len = fee_per_gram_stats_get_length(fee_per_gram_stats, error_out);
    if *error_out != 0 {
        return ptr::null_mut();
    }
    if len == 0 || position > len - 1 {
        error = LibWalletError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    Box::into_raw(Box::new((*fee_per_gram_stats).stats[position as usize].clone()))
}

/// Frees memory for a TariFeePerGramStats
///
/// ## Arguments
/// `fee_per_gram_stats` - The TariFeePerGramStats pointer
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn fee_per_gram_stats_destroy(fee_per_gram_stats: *mut TariFeePerGramStats) {
    if !fee_per_gram_stats.is_null() {
        drop(Box::from_raw(fee_per_gram_stats))
    }
}

/// ------------------------------------------------------------------------------------------ ///

/// ------------------------------------- FeePerGramStat ------------------------------------- ///

/// Get the order of TariFeePerGramStat
///
/// ## Arguments
/// `fee_per_gram_stats` - The TariFeePerGramStat pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns order
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn fee_per_gram_stat_get_order(
    fee_per_gram_stat: *mut TariFeePerGramStat,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut order = 0;
    if fee_per_gram_stat.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("fee_per_gram_stat".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        order = (*fee_per_gram_stat).order;
    }
    order
}

/// Get the minimum fee per gram of TariFeePerGramStat
///
/// ## Arguments
/// `fee_per_gram_stats` - The TariFeePerGramStat pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns minimum fee per gram
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn fee_per_gram_stat_get_min_fee_per_gram(
    fee_per_gram_stat: *mut TariFeePerGramStat,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut fee_per_gram = 0;
    if fee_per_gram_stat.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("fee_per_gram_stat".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        fee_per_gram = (*fee_per_gram_stat).min_fee_per_gram.as_u64();
    }
    fee_per_gram
}

/// Get the average fee per gram of TariFeePerGramStat
///
/// ## Arguments
/// `fee_per_gram_stats` - The TariFeePerGramStat pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns average fee per gram
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn fee_per_gram_stat_get_avg_fee_per_gram(
    fee_per_gram_stat: *mut TariFeePerGramStat,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut fee_per_gram = 0;
    if fee_per_gram_stat.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("fee_per_gram_stat".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        fee_per_gram = (*fee_per_gram_stat).avg_fee_per_gram.as_u64();
    }
    fee_per_gram
}

/// Get the maximum fee per gram of TariFeePerGramStat
///
/// ## Arguments
/// `fee_per_gram_stats` - The TariFeePerGramStat pointer
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Returns maximum fee per gram
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn fee_per_gram_stat_get_max_fee_per_gram(
    fee_per_gram_stat: *mut TariFeePerGramStat,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut fee_per_gram = 0;
    if fee_per_gram_stat.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("fee_per_gram_stat".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    } else {
        fee_per_gram = (*fee_per_gram_stat).max_fee_per_gram.as_u64();
    }
    fee_per_gram
}

/// Frees memory for a TariFeePerGramStat
///
/// ## Arguments
/// `fee_per_gram_stats` - The TariFeePerGramStat pointer
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn fee_per_gram_stat_destroy(fee_per_gram_stat: *mut TariFeePerGramStat) {
    if !fee_per_gram_stat.is_null() {
        drop(Box::from_raw(fee_per_gram_stat))
    }
}

/// Returns a ptr to the ContactsServiceHandle for use with chat
///
/// ## Arguments
/// `wallet` - The wallet instance
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ContactsServiceHandle` an opaque pointer used in chat sideloading initialization
///
/// # Safety
/// You should release the returned pointer after it's been used to initialize chat using `contacts_handle_destroy`
#[no_mangle]
pub unsafe extern "C" fn contacts_handle(wallet: *mut TariWallet, error_out: *mut c_int) -> *mut ContactsServiceHandle {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if wallet.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("wallet".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    Box::into_raw(Box::new((*wallet).wallet.contacts_service.clone()))
}

/// Frees memory for a ContactsServiceHandle
///
/// ## Arguments
/// `contacts_handle` - The pointer to a ContactsServiceHandle
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn contacts_handle_destroy(contacts_handle: *mut ContactsServiceHandle) {
    if !contacts_handle.is_null() {
        drop(Box::from_raw(contacts_handle))
    }
}
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

    #[cfg(tari_target_network_mainnet)]
    const NETWORK_STRING: &str = "stagenet";
    #[cfg(tari_target_network_nextnet)]
    const NETWORK_STRING: &str = "nextnet";
    #[cfg(not(any(tari_target_network_mainnet, tari_target_network_nextnet)))]
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

            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                alice_network_str,
                dns_string,
                false,
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
                dns_string,
                false,
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                dns_string,
                false,
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let wallet = wallet_create(
                config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                dns_string,
                false,
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let recovered_wallet = wallet_create(
                config,
                log_path,
                0,
                0,
                0,
                passphrase,
                seed_words,
                network_str,
                dns_string,
                false,
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                dns_string,
                false,
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
            let alice_wallet_runtime = &(*alice_wallet).runtime;
            let key_manager = &(*alice_wallet).wallet.key_manager_service;

            assert_eq!(error, 0);
            let mut test_outputs = Vec::with_capacity(10);
            for i in 0..10u8 {
                let uout = alice_wallet_runtime.block_on(create_test_input(
                    (1000u64 * u64::from(i)).into(),
                    0,
                    key_manager,
                    vec![i, i + 1, i + 2, i + 3, i + 4],
                ));
                test_outputs.push(uout.clone());
                alice_wallet_runtime
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
            let utxos: &[TariUtxo] = slice::from_raw_parts((*outputs).ptr as *mut TariUtxo, (*outputs).len);
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
            for utxo in utxos {
                let output = test_outputs
                    .iter()
                    .find(|val| {
                        alice_wallet_runtime
                            .block_on(val.commitment(key_manager))
                            .unwrap()
                            .to_hex() ==
                            CStr::from_ptr(utxo.commitment).to_str().unwrap()
                    })
                    .unwrap();
                assert_eq!(output.value.as_u64(), utxo.value);
                assert_eq!(output.features.maturity, utxo.lock_height);
                assert_eq!(
                    output.features.coinbase_extra.to_hex(),
                    CStr::from_ptr(utxo.coinbase_extra).to_str().unwrap()
                );
            }
            println!();
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
            let utxos: &[TariUtxo] = slice::from_raw_parts((*outputs).ptr as *mut TariUtxo, (*outputs).len);
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                dns_string,
                false,
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
                let uo = (*alice_wallet).runtime.block_on(create_test_input(
                    (1000 * i).into(),
                    0,
                    &(*alice_wallet).wallet.key_manager_service,
                    vec![],
                ));
                (*alice_wallet)
                    .runtime
                    .block_on(
                        (*alice_wallet)
                            .wallet
                            .output_manager_service
                            .add_output(uo.clone(), None),
                    )
                    .unwrap();
                (*alice_wallet)
                    .wallet
                    .output_db
                    .mark_outputs_as_unspent(vec![(
                        (*alice_wallet)
                            .runtime
                            .block_on(uo.hash(&(*alice_wallet).wallet.key_manager_service))
                            .unwrap(),
                        true,
                    )])
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

            let commitments = Box::into_raw(Box::new(TariVector::from(payload)));
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                dns_string,
                false,
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
                let uo = (*alice_wallet).runtime.block_on(create_test_input(
                    (15000 * i).into(),
                    0,
                    &(*alice_wallet).wallet.key_manager_service,
                    vec![],
                ));
                (*alice_wallet)
                    .runtime
                    .block_on(
                        (*alice_wallet)
                            .wallet
                            .output_manager_service
                            .add_output(uo.clone(), None),
                    )
                    .unwrap();
                (*alice_wallet)
                    .wallet
                    .output_db
                    .mark_outputs_as_unspent(vec![(
                        (*alice_wallet)
                            .runtime
                            .block_on(uo.hash(&(*alice_wallet).wallet.key_manager_service))
                            .unwrap(),
                        true,
                    )])
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

            let commitments = Box::into_raw(Box::new(TariVector::from(payload)));
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

            let commitments = Box::into_raw(Box::new(TariVector::from(payload)));
            let result = wallet_coin_join(alice_wallet, commitments, 5, error_ptr);
            assert_eq!(error, 0);
            assert!(result > 0);

            let unspent_outputs = (*alice_wallet)
                .wallet
                .output_db
                .fetch_outputs_by_query(OutputBackendQuery {
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
                .fetch_outputs_by_query(OutputBackendQuery {
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                dns_string,
                false,
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
                let uo = (*alice_wallet).runtime.block_on(create_test_input(
                    (15000 * i).into(),
                    0,
                    &(*alice_wallet).wallet.key_manager_service,
                    vec![],
                ));
                (*alice_wallet)
                    .runtime
                    .block_on(
                        (*alice_wallet)
                            .wallet
                            .output_manager_service
                            .add_output(uo.clone(), None),
                    )
                    .unwrap();
                (*alice_wallet)
                    .wallet
                    .output_db
                    .mark_outputs_as_unspent(vec![(
                        (*alice_wallet)
                            .runtime
                            .block_on(uo.hash(&(*alice_wallet).wallet.key_manager_service))
                            .unwrap(),
                        true,
                    )])
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

            let commitments = Box::into_raw(Box::new(TariVector::from(payload)));

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

            let commitments = Box::into_raw(Box::new(TariVector::from(payload)));

            let result = wallet_coin_split(alice_wallet, commitments, 3, 5, error_ptr);
            assert_eq!(error, 0);
            assert!(result > 0);

            let unspent_outputs = (*alice_wallet)
                .wallet
                .output_db
                .fetch_outputs_by_query(OutputBackendQuery {
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
                .fetch_outputs_by_query(OutputBackendQuery {
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let alice_wallet = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                dns_string,
                false,
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

            let key_manager = &(*alice_wallet).wallet.key_manager_service;
            for i in 1..=5 {
                (*alice_wallet)
                    .runtime
                    .block_on(
                        (*alice_wallet).wallet.output_manager_service.add_output(
                            (*alice_wallet).runtime.block_on(create_test_input(
                                (15000 * i).into(),
                                0,
                                key_manager,
                                vec![],
                            )),
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
            assert_eq!((*tv).cap, 12);

            tari_vector_push_string(
                tv,
                CString::new("test string 2").unwrap().into_raw() as *const c_char,
                &mut error as *mut c_int,
            );
            assert_eq!(error, 0);
            assert_eq!((*tv).tag, TariTypeTag::Text);
            assert_eq!((*tv).len, 2);
            assert_eq!((*tv).cap, 12);

            tari_vector_push_string(
                tv,
                CString::new("test string 3").unwrap().into_raw() as *const c_char,
                &mut error as *mut c_int,
            );
            assert_eq!(error, 0);
            assert_eq!((*tv).tag, TariTypeTag::Text);
            assert_eq!((*tv).len, 3);
            assert_eq!((*tv).cap, 12);

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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let wallet_ptr = wallet_create(
                config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                network_str,
                dns_string,
                false,
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
            let key_manager = &(*wallet_ptr).wallet.key_manager_service;

            let node_identity =
                NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
            let base_node_peer_public_key_ptr = Box::into_raw(Box::new(node_identity.public_key().clone()));
            let base_node_peer_address_ptr =
                CString::into_raw(CString::new(node_identity.first_public_address().unwrap().to_string()).unwrap())
                    as *const c_char;
            wallet_set_base_node_peer(
                wallet_ptr,
                base_node_peer_public_key_ptr,
                base_node_peer_address_ptr,
                error_ptr,
            );

            // Test the consistent features case
            let utxo_1 = runtime
                .block_on(create_wallet_output_with_data(
                    script!(Nop),
                    OutputFeatures::default(),
                    &runtime.block_on(TestParams::new(key_manager)),
                    MicroMinotari(1234u64),
                    key_manager,
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let alice_wallet_ptr = wallet_create(
                alice_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                alice_network_str,
                dns_string,
                false,
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
            let dns_string: *const c_char = CString::into_raw(CString::new("").unwrap()) as *const c_char;
            let bob_wallet_ptr = wallet_create(
                bob_config,
                ptr::null(),
                0,
                0,
                0,
                passphrase,
                ptr::null(),
                bob_network_str,
                dns_string,
                false,
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
            wallet_set_base_node_peer(
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
            wallet_set_base_node_peer(
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
                            sent_at: u64::from(i),
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
                            sent_at: u64::from(i),
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
