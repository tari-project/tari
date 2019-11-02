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
use tari_comms::peer_manager::Peer;
use tari_crypto::keys::SecretKey;
use tari_p2p::initialization::CommsConfig;
use tari_transactions::{
    tari_amount::MicroTari,
    transaction::{
        OutputFeatures,
        OutputFlags,
        Transaction,
        TransactionInput,
        TransactionKernel,
        TransactionOutput,
        UnblindedOutput,
    },
    types::{PrivateKey, PublicKey},
};
use tari_utilities::ByteArray;
use tari_wallet::{
    output_manager_service::{storage::database::PendingTransactionOutputs, OutputManagerConfig},
    wallet::WalletConfig,
    Wallet,
};
use tokio::runtime::Runtime;

pub type TariWallet = Wallet;
pub type WalletDateTime = NaiveDateTime;

/// -------------------------------- ByteVector ------------------------------------------------ ///
pub struct ByteVector(Vec<c_uchar>); // declared like this so that it can be exposed to external header

#[no_mangle]
pub unsafe extern "C" fn byte_vector_create(byte_array: *const c_uchar, element_count: c_int) -> *mut ByteVector {
    let bytes = ByteVector(Vec::new());
    let mut v = Vec::new();
    if !byte_array.is_null() {
        let array: &[c_uchar] = slice::from_raw_parts(byte_array, element_count as usize);
        v = array.to_vec();
    }
    Box::into_raw(Box::new(bytes))
}

#[no_mangle]
pub unsafe extern "C" fn byte_vector_destroy(bytes: *mut ByteVector) {
    if bytes.is_null() {
        let b = Box::from_raw(bytes);
    }
}

/// returns c_uchar at position in internal vector
#[no_mangle]
pub unsafe extern "C" fn byte_vector_get_at(ptr: *mut ByteVector, i: c_int) -> c_uchar {
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
pub type WalletPublicKey = PublicKey;

#[no_mangle]
pub unsafe extern "C" fn public_key_create(bytes: *mut ByteVector) -> *mut WalletPublicKey {
    let mut v = Vec::new();
    if !bytes.is_null() {
        v = (*bytes).0.clone();
    }
    let pk = WalletPublicKey::from_bytes(&v).unwrap();
    Box::into_raw(Box::new(pk))
}

#[no_mangle]
pub unsafe extern "C" fn public_key_destroy(pk: *mut WalletPublicKey) {
    if !pk.is_null() {
        Box::from_raw(pk);
    }
}

#[no_mangle]
pub unsafe extern "C" fn public_key_get_key(pk: *mut WalletPublicKey) -> *mut ByteVector {
    let mut bytes = ByteVector(Vec::new());
    if !pk.is_null() {
        bytes.0 = (*pk).to_vec();
    }
    Box::into_raw(Box::new(bytes))
}

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Private Key ----------------------------------------------- ///
pub type WalletPrivateKey = PrivateKey;

#[no_mangle]
pub unsafe extern "C" fn private_key_create(bytes: *mut ByteVector) -> *mut WalletPrivateKey {
    let mut v = Vec::new();
    if !bytes.is_null() {
        v = (*bytes).0.clone();
    }
    let pk = WalletPrivateKey::from_bytes(&v).unwrap();
    Box::into_raw(Box::new(pk))
}

#[no_mangle]
pub unsafe extern "C" fn private_key_destroy(pk: *mut WalletPrivateKey) {
    if !pk.is_null() {
        Box::from_raw(pk);
    }
}

#[no_mangle]
pub unsafe extern "C" fn private_key_get_key(pk: *mut WalletPrivateKey) -> *mut ByteVector {
    let mut bytes = ByteVector(Vec::new());
    if !pk.is_null() {
        bytes.0 = (*pk).to_vec();
    }
    Box::into_raw(Box::new(bytes))
}

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------------- OutputManagerConfig --------------------------------- ///
pub type WalletOutputManagerConfig = OutputManagerConfig;

#[no_mangle]
pub unsafe extern "C" fn output_manager_config_create(
    master_key: *mut PrivateKey,
    branch_seed: *mut c_char,
    primary_key_index: c_ulonglong,
) -> *mut WalletOutputManagerConfig
{
    let mut rng = rand::OsRng::new().unwrap();
    let mut k = PrivateKey::random(&mut rng);

    if !master_key.is_null() {
        k = (*master_key).clone();
    }

    let mut str = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !branch_seed.is_null() {
        str = CStr::from_ptr(branch_seed).to_str().unwrap().to_owned();
    }

    let omc = WalletOutputManagerConfig {
        master_key: Some(k),
        seed_words: None,
        branch_seed: str.to_string(),
        primary_key_index: primary_key_index as usize,
    };
    Box::into_raw(Box::new(omc))
}

/// TODO Add Seed Words Version

#[no_mangle]
pub unsafe extern "C" fn output_manager_config_destroy(wc: *mut WalletOutputManagerConfig) {
    if !wc.is_null() {
        Box::from_raw(wc);
    }
}

/// -------------------------------------------------------------------------------------------- ///

/// ----------------------------------- CommsConfig ---------------------------------------------///
pub type WalletCommsConfig = CommsConfig;

#[no_mangle]
pub unsafe extern "C" fn comms_config_create(
    address: *mut c_char,
    datastore: *mut c_char,
    database: *mut c_char,
    secret_key: *mut PrivateKey,
    public_key: *mut PublicKey,
) -> () //*mut WalletCommsConfig
{
    let mut str1 = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !address.is_null() {
        str1 = CStr::from_ptr(address).to_str().unwrap().to_owned();
    }
    let mut str2 = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !datastore.is_null() {
        str2 = CStr::from_ptr(datastore).to_str().unwrap().to_owned();
    }
    let mut str3 = CString::new("").unwrap().to_str().unwrap().to_owned();
    if !database.is_null() {
        str3 = CStr::from_ptr(database).to_str().unwrap().to_owned();
    }
    //    let ni = NodeIdentity::new(
    //        (*secret_key).clone(),
    //        (*public_key).clone(),
    //        str1.parse::<NetAddress>().unwrap(),
    //        (*peer_features).clone(),
    //    )
    //    .unwrap();

    //    let config = CommsConfig {
    //        node_identity: Arc::new(ni.clone()),
    //        host: "127.0.0.1".parse().unwrap(),
    //        socks_proxy_address: None,
    //        control_service: ControlServiceConfig {
    //            listener_address: ni.control_service_address(),
    //            socks_proxy_address: None,
    //            requested_connection_timeout: Duration::from_millis(2000),
    //        },
    //        datastore_path: str2,
    //        peer_database_name: str3,
    //        inbound_buffer_size: 100,
    //        outbound_buffer_size: 100,
    //        dht: Default::default(),
    //    };

    //    Box::into_raw(Box::new(config))
}

#[no_mangle]
pub unsafe extern "C" fn wallet_comms_config_destroy(wc: *mut WalletCommsConfig) {
    if !wc.is_null() {
        Box::from_raw(wc);
    }
}

/// ---------------------------------------------------------------------------------------------///

/// -------------------------------- KeyManagerWords ------------------------------------------- ///
pub struct KeyManagerSeedWords {
    words: Vec<String>,
}

#[no_mangle]
pub unsafe extern "C" fn key_manager_seed_words_create() -> *mut KeyManagerSeedWords {
    let m = KeyManagerSeedWords { words: Vec::new() };

    let boxed = Box::new(m);
    Box::into_raw(boxed)
}

#[no_mangle]
pub unsafe extern "C" fn key_manager_seed_words_get_at(mgr: *mut KeyManagerSeedWords, i: c_int) -> *const c_char {
    if mgr.is_null() {
        return std::ptr::null_mut();
    }
    let words = &mut (*mgr).words;
    let word = words.get(i as usize).unwrap();
    let m = CString::new(word.as_str()).unwrap();
    CString::into_raw(m)
}

#[no_mangle]
pub unsafe extern "C" fn key_manager_seed_words_add(s: *const c_char, mgr: *mut KeyManagerSeedWords) -> bool {
    if mgr.is_null() {
        return false;
    }
    let mut add = CString::new("").unwrap();
    if s.is_null() {
        return false;
    }
    let str = CStr::from_ptr(s).to_str().unwrap().to_owned();
    (*mgr).words.push(str);
    return true;
}

#[no_mangle]
pub unsafe extern "C" fn key_manager_seed_length(vec: *const KeyManagerSeedWords) -> c_int {
    if vec.is_null() {
        return 0;
    }

    (&*vec).words.len() as c_int
}

#[no_mangle]
pub unsafe extern "C" fn key_manager_seed_words_destroy(obj: *mut KeyManagerSeedWords) {
    // as a rule of thumb, freeing a null pointer is just a noop.
    if obj.is_null() {
        return;
    }

    Box::from_raw(obj);
}

/// -------------------------------------------------------------------------------------------- ///

/// -----------------------------------------UnblindedOutput------------------------------------ ///
pub type WalletUnblindedOutput = UnblindedOutput;

pub unsafe extern "C" fn wallet_unblinded_output_create(
    value: c_ulonglong,
    spending_key: *mut ByteVector,
    maturity: c_ulonglong,
    flags: c_uchar,
) -> *mut WalletUnblindedOutput
{
    let amount = MicroTari::from(value);
    let pk = PrivateKey::from_bytes(&((*spending_key).0));
    let mut of = OutputFeatures::default();
    of.maturity = maturity;
    // of.flags = flags;
    let uo = WalletUnblindedOutput {
        value: amount,
        spending_key: pk.unwrap(),
        features: of,
    };
    Box::into_raw(Box::new(uo))
}

pub unsafe extern "C" fn wallet_unblinded_output_destroy(output: *mut WalletUnblindedOutput) {
    if !output.is_null() {
        Box::from_raw(output);
    }
}
/// -------------------------------------------------------------------------------------------- ///

/// --------------------------------- PendingTransactionOutputs--------------------------------- ///
#[no_mangle]
pub unsafe extern "C" fn create_pending_transaction_outputs(
    tx_id: c_ulonglong,       // u64
    timestamp: *const c_char, // NaiveDateTime
) -> *mut PendingTransactionOutputs
{
    Box::into_raw(Box::new(PendingTransactionOutputs {
        tx_id,
        outputs_to_be_spent: Vec::new(),
        outputs_to_be_received: Vec::new(),
        timestamp: NaiveDateTime::parse_from_str("timestamp", "THE FORMAT WE CHOOSE").unwrap(), /* Use the rfc-3339 Format for this. */
    }))
}

#[no_mangle]
pub unsafe extern "C" fn destroy_pending_transaction_outputs(pto: *mut PendingTransactionOutputs) {
    if !pto.is_null() {
        Box::from_raw(pto);
    }
}
/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Compound Inputs, Outputs, Kernels ------------------------- ///
/// Initialize a Transaction struct to be populated

pub struct TransactionInputs(Vec<TransactionInput>);
pub struct TransactionOutputs(Vec<TransactionOutput>);
pub struct TransactionKernels(Vec<TransactionKernel>);

#[no_mangle]
pub unsafe extern "C" fn transaction_inputs_add_transaction_input(
    inputs: *mut TransactionInputs,
    transaction: *mut TransactionInput,
) -> bool
{
    if inputs.is_null() {
        return false;
    }

    if transaction.is_null() {
        return false;
    }

    (*inputs).0.push((*transaction).clone());
    return true;
}

#[no_mangle]
pub unsafe extern "C" fn transaction_inputs_create() -> *mut TransactionInputs {
    let i = TransactionInputs(Vec::new());
    Box::into_raw(Box::new(i))
}

#[no_mangle]
pub unsafe extern "C" fn transaction_inputs_destroy(inputs: *mut TransactionInputs) {
    if inputs.is_null() {
        let i = Box::from_raw(inputs);
    }
}

#[no_mangle]
pub unsafe extern "C" fn transaction_outputs_create() -> *mut TransactionOutputs {
    let i = TransactionOutputs(Vec::new());
    Box::into_raw(Box::new(i))
}

#[no_mangle]
pub unsafe extern "C" fn transaction_outputs_destroy(inputs: *mut TransactionOutputs) {
    if inputs.is_null() {
        let i = Box::from_raw(inputs);
    }
}

#[no_mangle]
pub unsafe extern "C" fn transaction_kernels_create() -> *mut TransactionKernels {
    let i = TransactionKernels(Vec::new());
    Box::into_raw(Box::new(i))
}

#[no_mangle]
pub unsafe extern "C" fn transaction_kernels_destroy(kernels: *mut TransactionKernels) {
    if kernels.is_null() {
        let i = Box::from_raw(kernels);
    }
}

/// Add a transaction input to a transaction struct
#[no_mangle]
pub unsafe extern "C" fn transaction_input_add_transaction_input(
    inputs: *mut TransactionInputs,
    transaction: *mut TransactionInput,
) -> bool
{
    if inputs.is_null() {
        return false;
    }

    if transaction.is_null() {
        return false;
    }

    (*inputs).0.push((*transaction).clone());
    return true;
}

/// Add a transaction output to a transaction struct
#[no_mangle]
pub unsafe extern "C" fn transaction_outputs_add_transaction_output(
    outputs: *mut TransactionOutputs,
    transaction: *mut TransactionOutput,
) -> bool
{
    if outputs.is_null() {
        return false;
    }

    if transaction.is_null() {
        return false;
    }

    (*outputs).0.push((*transaction).clone());
    return true;
}

/// Add a transaction kernel to a transaction struct
#[no_mangle]
pub unsafe extern "C" fn transaction_kernels_add_transaction_kernel(
    kernels: *mut TransactionKernels,
    kernel: *mut TransactionKernel,
) -> bool
{
    if kernels.is_null() {
        return false;
    }

    if kernel.is_null() {
        return false;
    }

    (*kernels).0.push((*kernel).clone());
    return true;
}

/// -------------------------------------------------------------------------------------------- ///

/// -------------------------------- Wallet ---------------------------------------------------- ///
// TODO: Fully implement wallet to finish this off

pub type WalletMasterConfig = WalletConfig;

pub unsafe extern "C" fn create_wallet(
    // Local Node Identity data
    config: *const WalletMasterConfig,
) -> *mut Wallet
{
    // TODO do null check for config, runtime
    let runtime = Runtime::new();
    let mut w = Wallet::new((*config).clone(), runtime.unwrap());
    Box::into_raw(Box::new(w.unwrap()))
}

#[no_mangle]
pub unsafe extern "C" fn start_wallet(wallet: *mut Wallet) -> bool {
    // (*wallet).start() ? true : false; implement start() on wallet
    // i.e return (*wallet).start()
    true
}

pub unsafe extern "C" fn wallet_generate_master_seed(wallet: *mut Wallet) -> *mut KeyManagerSeedWords {
    let mut seed = KeyManagerSeedWords { words: vec![] };
    if wallet.is_null() {
        // seed.words = (*wallet).generate_seed();
    }
    Box::into_raw(Box::new(seed))
}

/// Add an output to the wallet.
#[no_mangle]
pub unsafe extern "C" fn wallet_add_outputs(wallet: *mut Wallet, output: *mut WalletUnblindedOutput) -> bool {
    if wallet.is_null() {
        return false;
    }

    if output.is_null() {
        return false;
    }

    (*wallet).output_manager_service.add_output((*output).clone()); // ? true : false; implement AddOutput(O: UnblindedOutput) -> bool on Wallet
                                                                    // i.e return (*wallet).addOutput((*output));
    return true;
}

/// Append an UnblindedOutput to be spent to the pending transaction outputs object
#[no_mangle]
pub unsafe extern "C" fn wallet_add_output_to_spend(
    wallet: *mut TariWallet,
    output: *mut WalletUnblindedOutput,
) -> bool
{
    if wallet.is_null() {
        return false;
    }

    if output.is_null() {
        return false;
    }

    // (*wallet).pendingtransactionoutputs.addSpendoutput((*output)) ? true : false;
    return true;
}

/// Append an UnblindedOutput to be received to the pending transaction outputs object
#[no_mangle]
pub unsafe extern "C" fn wallet_add_output_to_received(
    wallet: *mut TariWallet,
    output: *mut WalletUnblindedOutput,
) -> bool
{
    if wallet.is_null() {
        return false;
    }

    if output.is_null() {
        return false;
    }

    // (*wallet).pendingtransactionoutputs.addReceivedoutput((*output)) ? true : false;
    return true;
}

/// Add an output to the wallet. `spent` is a boolean that indicates if this output is a spent or unspent output.
#[no_mangle]
pub unsafe extern "C" fn wallet_add_pending_transaction_outputs(
    wallet: *mut Wallet,
    output: *mut PendingTransactionOutputs,
    spent: bool,
) -> bool
{
    if wallet.is_null() {
        return false;
    }

    if output.is_null() {
        return false;
    }

    match spent {
        true => {},  //(*wallet).pendingtransactionoutputs.addSpentOutput((*output)) ? true : false;
        false => {}, //(*wallet).pendingtransactionoutputs.addReceivedOutput((*output)) ? true : false;
    }
    return true;
}

/// TODO Methods to construct, free above 3 types

#[no_mangle]
pub unsafe extern "C" fn wallet_create_transaction(
    inputs: *mut TransactionInputs,
    outputs: *mut TransactionOutputs,
    kernels: *mut TransactionKernels,
    offset: *const PrivateKey,
) -> *mut Transaction
{
    // TODO null check
    let t = Transaction::new(
        (*inputs).0.clone(),
        (*outputs).0.clone(),
        (*kernels).0.clone(),
        (*offset).clone(),
    );
    Box::into_raw(Box::new(t))
}

/// Add an completed transaction to the wallet.
#[no_mangle]
pub unsafe extern "C" fn wallet_add_transaction(
    wallet: *mut Wallet,
    pending_tx: *mut Transaction,
    inbound: bool,
) -> bool
{
    return true;
}

/// Add a ReceivedTransactionProtocol instance to the wallet
#[no_mangle]
pub unsafe extern "C" fn wallet_add_pending_inbound_transaction(
    wallet: *mut Wallet,
    transaction: *mut Transaction,
) -> bool
{
    if wallet.is_null() {
        return false;
    }

    if transaction.is_null() {
        return false;
    }

    //(*wallet).pendingtransactionoutputs.addinboundtransaction((*transaction)) ? true : false;

    return true;
    // append this data to the wallet.
    // Assume it is RecipientState::Finalized
    // TODO figure out best way to get this into the Rust struct, the protocol structs are strictly locked down
}

/// Add a ReceivedTransactionProtocol instance to the wallet
#[no_mangle]
pub unsafe extern "C" fn wallet_add_pending_outbound_transaction(
    wallet: *mut Wallet,
    transaction: *mut Transaction,
) -> bool
{
    if wallet.is_null() {
        return false;
    }

    if transaction.is_null() {
        return false;
    }

    //(*wallet).pendingtransactionoutputs.addoutboundtransaction((*transaction)) ? true : false;

    return true;
    // append this data to the wallet.
    // Assume it is RecipientState::Finalized
    // TODO figure out best way to get this into the Rust struct, the protocol structs are strictly locked down
}

/// Create an initial RawTransactionInfo struct that will be used to build the SenderTransactionProtocol
//#[no_mangle]
// pub unsafe extern "C" fn create_pending_outbound_transaction(
//    num_recipients: c_uint,                // usize,
//    amount_to_self: c_ulonglong,           // MicroTari,
//    change: c_ulonglong,                   // MicroTari,
//    offset: *const c_char,                 // Byte[32] - BlindingFactor,
//    offset_blinding_factor: *const c_char, // Byte[32] - BlindingFactor,
//    public_excess: *const c_char,          // Byte[32] - PublicKey,
//    private_nonce: *const c_char,          // Byte[32] - PrivateKey,
//    public_nonce: *const c_char,           // Byte[32] - PublicKey,
//    public_nonce_sum: *const c_char,       // Byte[32] - PublicKey,
// Metadata members
//    fee: c_ulonglong,             // MicroTari,
//    lock_height: c_ulonglong,     // u64,
//    meta_info: *const c_char,     // Option<Byte[32]> - Option<HashOutput>,
//    linked_kernel: *const c_char, // Option<Byte[32]> - Option<HashOutput>,
// RecipientInfo members
//    tx_id: c_ulonglong,               // u64,
//    output: *const c_char,            // Byte[32] - TransactionOutput,
//    public_spend_key: *const c_char,  // Byte[32] - PublicKey,
// partial_signature: *const c_char, // Byte[32] - Signature,
//) -> () //*mut RawTransactionInfo, //TODO Figure out the best way to expose this struct for this interface
//{

//}

/// Append an id to a pending outbound transaction RawTransactionInfo struct
#[no_mangle]
pub unsafe extern "C" fn wallet_add_pending_outbound_id(
    // raw_info: *mut RawTransactionInfo, //TODO RawTransactionInfo is private
    id: c_ulonglong,
) -> bool
{
    return true;
    // append id
}

/// Append an id to a pending outbound transaction RawTransactionInfo struct
#[no_mangle]
pub unsafe extern "C" fn wallet_add_pending_outbound_amount(
    // raw_info: *mut RawTransactionInfo, //TODO RawTransactionInfo is private
    amount: c_ulonglong,
) -> bool
{
    return true;
    // append amount
}

/// Append an id to a pending outbound transaction RawTransactionInfo struct
#[no_mangle]
pub unsafe extern "C" fn wallet_add_pending_outbound_input(
    // raw_info: *mut RawTransactionInfo, //TODO RawTransactionInfo is private
    commitment: *const c_char, // Byte[32] - Commitment
    feature_flags: c_char,     // OutputFlags,
    maturity: c_ulonglong,     // u64
) -> bool
{
    return true;
    // append input
}

/// Append an id to a pending outbound transaction RawTransactionInfo struct
#[no_mangle]
pub unsafe extern "C" fn wallet_add_pending_outbound_output(
    // raw_info: *mut RawTransactionInfo, //TODO RawTransactionInfo is private
    commitment: ByteVector,     // Byte[32] - Commitment, TODO
    proof: ByteVector,          // Byte[32] - Rangeproof, TODO
    feature_flags: OutputFlags, // OutputFlags,
    maturity: c_ulonglong,      // u64
) -> bool
{
    return true;
    // append output
}

/// Append an id to a pending outbound transaction RawTransactionInfo struct
#[no_mangle]
pub unsafe extern "C" fn wallet_add_pending_outbound_signature(
    // raw_info: *mut RawTransactionInfo, //TODO RawTransactionInfo is private
    signature: *const ByteVector, // Byte[32] - Signature
) -> bool
{
    return true;
    // append signature
}

// Add an completed transaction to the wallet.
// #[no_mangle]
// pub unsafe extern "C" fn wallet_add_pending_outbound_transaction(
// wallet: *mut Wallet,
// raw_info: *mut RawTransactionInfo, //TODO RawTransactionInfo is private
// ) -> bool
// {
// return true;
// Build the SenderTransactionProtocol and append it.
// }

// ------------------------------------------------------------------------------------------------
// API Functions
// ------------------------------------------------------------------------------------------------

//#[no_mangle]
// pub unsafe extern "C" fn generate_key_manager_from_seed_words(
// wallet: *mut Wallet,
//    seed_words: *mut KeyManagerSeedWords,
//    branch_seed: *const c_char, // String
//) -> bool
//{
//}

//#[no_mangle]
// pub unsafe extern "C" fn generate_identity(wallet: *mut Wallet) -> *mut IdentityFfi {}
// TODO C Destructuring methods for the IdentityFfi struct

#[no_mangle]
pub unsafe extern "C" fn wallet_add_base_node_peer(wallet: *mut Wallet, peer: *mut Peer) -> bool {
    if wallet.is_null() {
        return false;
    }

    if peer.is_null() {
        return false;
    }

    // (*wallet).addPeer((*peer));
    return true;
}

//#[no_mangle]
// pub unsafe extern "C" fn get_network_status(wallet: *mut Wallet) -> *mut NetworkStatusFfi {}
// TODO C Destructuring methods for the NetworkStatusFfi struct

#[no_mangle]
pub unsafe extern "C" fn wallet_get_balance(wallet: *mut Wallet) -> c_ulonglong {
    //(*wallet).getBalance();
    return 0;
}

// Create and send the first stage of a transaction to the specified wallet for the specified amount and with the
// specified fee.
#[no_mangle]
pub unsafe extern "C" fn wallet_send_transaction(
    wallet: *mut Wallet,
    dest_pubkey: ByteVector,
    amount: MicroTari,
    fee_per_gram: MicroTari,
) -> bool
{
    //(*wallet).sendTransaction((*peer),(*transaction)) ? true : false
    return true;
}

/// Cancel a pending outbound transaction so that the wallet will not complete and broadcast it if a reply is received
#[no_mangle]
pub unsafe extern "C" fn wallet_cancel_transaction(wallet: *mut Wallet, tx_id: c_ulonglong) -> bool {
    if wallet.is_null() {
        return false;
    }

    //(*wallet).cancelTransaction ((*tx_id)) ? true : false
    return true;
}

// Callback Definition - Example

// Will probably have to implement as a struct of callbacks in wallet, with wallet only calling the
// functions if they are callable from the relevant wallet function, where the register callback functions
// will bind the relevant c equivalent funciton pointer to the associated function
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

// Initialize a new PendingTransactionOutputs record
// int create_pending_transaction_outputs(longlong tx_id, char* timestamp) {}

// Append an output to be spent onto an existing PendingTransactionOutputs record
// int add_output_to_be_spent(
//      ulonglong tx_id,
//      ulonglong value,
//      *char spending_key,
//      uchar feature_flags,
//      ulonglong maturity
// ) {}

// Append an output to be received onto an existing PendingTransactionOutputs record
// int add_output_to_be_received(
//      ulonglong tx_id,
//      ulonglong value,
//      *char spending_key,
//      uchar feature_flags,
//      ulonglong maturity,
// ) {}

// This function should result in the outputs that are tied up in a PendingTransactionOutputs collection to be moved to
// spent and unspent respectively
//      int confirm_pending_tx_outputs(longlong tx_id){}

// This function should result in the `outputs to be spent` that are tied up in a PendingTransactionOutputs collection
// to be moved to unspent and the `outputs to be received` should be dropped
//      int cancel_pending_tx_outputs(longlong tx_id){}

// Create a Pending Inbound Transaction
// int add_pending_inbound_transaction(
//    ulonglong tx_id ,
//    *char output,
//    *char public_spend_key,
//    *char partial_signature,
//) {}

// Initialize a new PendingOutboundTransaction record
// int create_pending_outbound_transaction(
//    uint num_recipients,
//    ulonglong amount_to_self,
//    ulonglong change,
//    *char offset,
//    *char offset_blinding_factor,
//    *char public_excess,
//    *char private_nonce,
//    *char public_nonce,
//    *char public_nonce_sum,
//    // Metadata members
//    ulonglong fee,
//    ulonglong lock_height,
//    *char meta_info,
//    *char linked_kernel,
//    // RecipientInfo members
//    ulonglong tx_id,
//    *char output,
//    *char public_spend_key,
//    *char partial_signature
//) {}

// Append an ID to an existing Pending Outbound Transaction record
// int add_pending_outbound_id(longlong tx_id, longlong id) {}

// Append an amount to an existing Pending Outbound Transaction record
// int add_pending_outbound_amount(longlong tx_id, longlong amount) {}

// Append an input to an existing Pending Outbound Transaction record
// int add_pending_outbound_input(longlong tx_id, *char commitment, char features) {}

// Append an output to an existing Pending Outbound Transaction record
// int add_pending_outbound_output(
//      longlong tx_id,
//      *char commitment,
//      *char proof,
//      uchar feature_flags,
//      ulonglong maturity
// ) {}

// Initialize a new Completed Transaction record
// int create_completed_transaction(longlong tx_id, *char offset){}

// Append an input to an existing Completed Transaction record
// int add_pending_transaction_input(longlong tx_id, *char commitment, char features) {}

// Append an output to an existing Completed Transaction record
// int add_pending_transaction_output(
//      longlong tx_id,
//      *char commitment,
//      *char proof,
//      uchar feature_flags,
//      ulonglong maturity
// ) {}

// Append a transaction kernel to an existing Completed Transaction record
// int add_pending_transaction_kernel(
//    longlong tx_id,
//    char features,
//    longlong fee,
//    longlong lock_height,
//    *char meta_info,
//    *char linked_kernel,
//    *char excess,
//    *char excess_sig,
//) {}

// Mark this Pending Inbound Transaction as Confirmed and clean up the DB accordingly
// int confirm_pending_inbound_transaction(longlong tx_id){}
// Mark this Pending Outbound Transaction as Confirmed and clean up the DB accordingly
// int confirm_pending_outbound_transaction(longlong tx_id){}
