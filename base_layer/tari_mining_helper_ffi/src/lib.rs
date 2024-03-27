// Copyright 2021. The Tari Project
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
//
#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]

mod error;
use core::ptr;
use std::{convert::TryFrom, ffi::CString, slice, str::FromStr};

use borsh::{BorshDeserialize, BorshSerialize};
use libc::{c_char, c_int, c_uchar, c_uint, c_ulonglong};
use tari_common::{configuration::Network, network_check::set_network_if_choice_valid};
use tari_common_types::tari_address::TariAddress;
use tari_core::{
    blocks::{BlockHeader, NewBlockTemplate},
    consensus::ConsensusManager,
    proof_of_work::sha3x_difficulty,
    transactions::{
        generate_coinbase,
        key_manager::create_memory_db_key_manager,
        transaction_components::RangeProofType,
    },
};
use tari_crypto::tari_utilities::hex::Hex;
use tokio::runtime::Runtime;

use crate::error::{InterfaceError, MiningHelperError};
mod consts {
    // Import the auto-generated const values from the Manifest and Git
    include!(concat!(env!("OUT_DIR"), "/consts.rs"));
}

pub type TariPublicKey = tari_comms::types::CommsPublicKey;
#[derive(Debug, PartialEq, Clone)]
pub struct ByteVector(Vec<c_uchar>);

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
        error = MiningHelperError::from(InterfaceError::NullError("byte_array".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        let array: &[c_uchar] = slice::from_raw_parts(byte_array, element_count as usize);
        bytes.0 = array.to_vec();
        if bytes.0.len() != element_count as usize {
            error = MiningHelperError::from(InterfaceError::AllocationError).code;
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
        drop(Box::from_raw(bytes));
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
        error = MiningHelperError::from(InterfaceError::NullError("ptr".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0u8;
    }
    let len = byte_vector_get_length(ptr, error_out);
    if len == 0 || position > len - 1 {
        error = MiningHelperError::from(InterfaceError::PositionInvalidError).code;
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
#[no_mangle]
pub unsafe extern "C" fn byte_vector_get_length(vec: *const ByteVector, error_out: *mut c_int) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if vec.is_null() {
        error = MiningHelperError::from(InterfaceError::NullError("vec".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }
    match c_uint::try_from((*vec).0.len()) {
        Ok(v) => v,
        Err(_) => {
            error = MiningHelperError::from(InterfaceError::Conversion("byte_vector".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Validates a hex string is convertible into a TariPublicKey
///
/// ## Arguments
/// `hex` - The hex formatted cstring to be validated
///
/// ## Returns
/// `bool` - Returns true/false
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn public_key_hex_validate(hex: *const c_char, error_out: *mut c_int) -> bool {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if hex.is_null() {
        error = MiningHelperError::from(InterfaceError::NullError("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }
    let native = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
    let pk = TariPublicKey::from_hex(&native);
    match pk {
        Ok(_pk) => true,
        Err(e) => {
            error = MiningHelperError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// Injects a nonce into a blocktemplate
///
/// ## Arguments
/// `hex` - The hex formatted cstring
/// `nonce` - The nonce to be injected
///
/// ## Returns
/// `c_char` - The updated hex formatted cstring or null on error
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn inject_nonce(header: *mut ByteVector, nonce: c_ulonglong, error_out: *mut c_int) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if header.is_null() {
        error = MiningHelperError::from(InterfaceError::NullError("header".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }
    let mut bytes = (*header).0.as_slice();
    let mut block_header: BlockHeader = match BorshDeserialize::deserialize(&mut bytes) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::Conversion(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };
    block_header.nonce = nonce;
    let mut buffer = Vec::new();
    BorshSerialize::serialize(&block_header, &mut buffer).unwrap();
    (*header).0 = buffer;
}

/// Injects a coinbase into a blocktemplate
///
/// ## Arguments
/// `block_template_bytes` - The block template as bytes, serialized with borsh.io
/// `value` - The value of the coinbase
/// `stealth_payment` - Boolean value, is this a stealh payment or normal one-sided
/// `revealed_value_proof` - Boolean value, should this use the reveal value proof, or BP+
/// `wallet_payment_address` - The address to pay the coinbase to
/// `coinbase_extra` - The value of the coinbase extra field
/// `network` - The value of the network
///
/// ## Returns
/// `block_template_bytes` - The updated block template
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[allow(clippy::too_many_lines)]
#[no_mangle]
pub unsafe extern "C" fn inject_coinbase(
    block_template_bytes: *mut ByteVector,
    coibase_value: c_ulonglong,
    stealth_payment: bool,
    revealed_value_proof: bool,
    wallet_payment_address: *const c_char,
    coinbase_extra: *const c_char,
    network: c_uint,
    error_out: *mut c_int,
) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if block_template_bytes.is_null() {
        error = MiningHelperError::from(InterfaceError::NullError("block template".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }
    if wallet_payment_address.is_null() {
        error = MiningHelperError::from(InterfaceError::NullError("wallet_payment_address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }
    let native_string_address = CString::from_raw(wallet_payment_address as *mut i8)
        .to_str()
        .unwrap()
        .to_owned();
    let wallet_address = match TariAddress::from_str(&native_string_address) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::InvalidAddress(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };
    if coinbase_extra.is_null() {
        error = MiningHelperError::from(InterfaceError::NullError("coinbase_extra".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }
    let network_u8 = match u8::try_from(network) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::InvalidNetwork(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };
    let network = match Network::try_from(network_u8) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::InvalidNetwork(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };
    // Set the static network variable according to the user chosen network (for use with
    // `get_current_or_user_setting_or_default()`) -
    if let Err(e) = set_network_if_choice_valid(network) {
        error = MiningHelperError::from(InterfaceError::InvalidNetwork(e.to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    };
    let coinbase_extra_string = CString::from_raw(coinbase_extra as *mut i8)
        .to_str()
        .unwrap()
        .to_owned();
    let mut bytes = (*block_template_bytes).0.as_slice();
    let mut block_template: NewBlockTemplate = match BorshDeserialize::deserialize(&mut bytes) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::Conversion(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };
    let key_manager = create_memory_db_key_manager();

    let consensus_manager = match ConsensusManager::builder(network).build() {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::NullError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };
    let runtime = match Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::TokioError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };
    let range_proof_type = if revealed_value_proof {
        RangeProofType::RevealedValue
    } else {
        RangeProofType::BulletProofPlus
    };
    let height = block_template.header.height;
    let (coinbase_output, coinbase_kernel) = match runtime.block_on(async {
        // we dont count the fee or the reward here, we assume the caller has calculated the amount to be the exact
        // value for the coinbase(s) they want.
        generate_coinbase(
            0.into(),
            coibase_value.into(),
            height,
            coinbase_extra_string.as_bytes(),
            &key_manager,
            &wallet_address,
            stealth_payment,
            consensus_manager.consensus_constants(height),
            range_proof_type,
        )
        .await
    }) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::CoinbaseBuildError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };
    block_template.body.add_output(coinbase_output);
    block_template.body.add_kernel(coinbase_kernel);
    block_template.body.sort();
    let mut buffer = Vec::new();
    BorshSerialize::serialize(&block_template, &mut buffer).unwrap();
    (*block_template_bytes).0 = buffer;
}

/// Returns the difficulty of a share
///
/// ## Arguments
/// `hex` - The hex formatted cstring to be validated
///
/// ## Returns
/// `c_ulonglong` - Difficulty, 0 on error
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn share_difficulty(
    header: *mut ByteVector,
    network: c_uint,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let network_u8 = match u8::try_from(network) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::InvalidNetwork(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return 1;
        },
    };
    let network = match Network::try_from(network_u8) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::InvalidNetwork(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return 1;
        },
    };
    // Set the static network variable according to the user chosen network (for use with
    // `get_current_or_user_setting_or_default()`) -
    if let Err(e) = set_network_if_choice_valid(network) {
        error = MiningHelperError::from(InterfaceError::InvalidNetwork(e.to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 1;
    };
    if header.is_null() {
        error = MiningHelperError::from(InterfaceError::NullError("header".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 1;
    }
    let mut bytes = (*header).0.as_slice();
    let block_header = match BorshDeserialize::deserialize(&mut bytes) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::Conversion(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return 2;
        },
    };
    let difficulty = match sha3x_difficulty(&block_header) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::Conversion(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return 3;
        },
    };
    difficulty.as_u64()
}

/// Validates a share submission
///
/// ## Arguments
/// `hex` - The hex representation of the share to be validated
/// `hash` - The hash of the share to be validated
/// `nonce` - The nonce for the share to be validated
/// `stratum_difficulty` - The stratum difficulty to be checked against (meeting this means that the share is valid for
/// payout) `template_difficulty` - The difficulty to be checked against (meeting this means the share is also a block
/// to be submitted to the chain)
///
/// ## Returns
/// `c_uint` - Returns one of the following:
///             0: Valid Block
///             1: Valid Share
///             2: Invalid Share
///             3: Invalid Difficulty
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn share_validate(
    header: *mut ByteVector,
    hash: *const c_char,
    network: c_uint,
    share_difficulty: c_ulonglong,
    template_difficulty: c_ulonglong,
    error_out: *mut c_int,
) -> c_int {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let network_u8 = match u8::try_from(network) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::InvalidNetwork(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return 1;
        },
    };
    let network = match Network::try_from(network_u8) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::InvalidNetwork(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return 1;
        },
    };
    // Set the static network variable according to the user chosen network (for use with
    // `get_current_or_user_setting_or_default()`) -
    if let Err(e) = set_network_if_choice_valid(network) {
        error = MiningHelperError::from(InterfaceError::InvalidNetwork(e.to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 1;
    };
    if header.is_null() {
        error = MiningHelperError::from(InterfaceError::NullError("header".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 2;
    }
    let mut bytes = (*header).0.as_slice();
    let block_header = match BlockHeader::deserialize(&mut bytes) {
        Ok(v) => v,
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::Conversion(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return 2;
        },
    };

    if hash.is_null() {
        error = MiningHelperError::from(InterfaceError::NullError("hash".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 2;
    }
    let block_hash_string = CString::from_raw(hash as *mut i8).to_str().unwrap().to_owned();
    if block_header.hash().to_hex() != block_hash_string {
        error = MiningHelperError::from(InterfaceError::InvalidHash(block_hash_string)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 2;
    }
    let difficulty = match sha3x_difficulty(&block_header) {
        Ok(v) => v.as_u64(),
        Err(e) => {
            error = MiningHelperError::from(InterfaceError::Conversion(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return 3;
        },
    };
    if difficulty >= template_difficulty {
        0
    } else if difficulty >= share_difficulty {
        1
    } else {
        error = MiningHelperError::from(InterfaceError::LowDifficulty(block_hash_string)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        4
    }
}

#[cfg(test)]
mod tests {
    use libc::c_int;
    use tari_common::configuration::Network;
    use tari_core::{
        blocks::{genesis_block::get_genesis_block, Block},
        proof_of_work::Difficulty,
        transactions::tari_amount::MicroMinotari,
    };

    use super::*;
    use crate::{inject_nonce, public_key_hex_validate, share_difficulty, share_validate};

    fn min_difficulty() -> Difficulty {
        Difficulty::from_u64(1000).expect("Failed to create difficulty")
    }

    fn create_test_block() -> Block {
        get_genesis_block(Network::LocalNet).block().clone()
    }

    fn generate_nonce_with_min_difficulty(difficulty: Difficulty) -> Result<(Difficulty, u64), String> {
        use rand::Rng;
        let mut block = create_test_block();
        block.header.nonce = rand::thread_rng().gen();
        for _ in 0..20000 {
            if sha3x_difficulty(&block.header).unwrap() >= difficulty {
                return Ok((sha3x_difficulty(&block.header).unwrap(), block.header.nonce));
            }
            block.header.nonce += 1;
        }
        Err(format!(
            "Failed to generate nonce for difficulty {} within 20000 iterations",
            difficulty
        ))
    }

    #[test]
    fn detect_change_in_consensus_encoding() {
        #[cfg(tari_target_network_mainnet)]
        let (nonce, difficulty, network) = match Network::get_current_or_user_setting_or_default() {
            Network::MainNet => (
                3145418102407526886,
                Difficulty::from_u64(1505).unwrap(),
                Network::MainNet,
            ),
            Network::StageNet => (
                5024328429923549037,
                Difficulty::from_u64(2065).unwrap(),
                Network::StageNet,
            ),
            _ => panic!("Invalid network for mainnet target"),
        };
        #[cfg(tari_target_network_nextnet)]
        let (nonce, difficulty, network) = (
            10034243937442353464,
            Difficulty::from_u64(1190).unwrap(),
            Network::NextNet,
        );
        #[cfg(not(any(tari_target_network_mainnet, tari_target_network_nextnet)))]
        let (nonce, difficulty, network) = (
            9571285381070445492,
            Difficulty::from_u64(2412).unwrap(),
            Network::Esmeralda,
        );
        unsafe {
            set_network_if_choice_valid(network).unwrap();
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block = create_test_block();
            let header_bytes = borsh::to_vec(&block.header).unwrap();
            #[allow(clippy::cast_possible_truncation)]
            let len = header_bytes.len() as u32;
            let byte_vec = byte_vector_create(header_bytes.as_ptr(), len, error_ptr);
            inject_nonce(byte_vec, nonce, error_ptr);
            assert_eq!(error, 0);
            let result = share_difficulty(byte_vec, u32::from(network.as_byte()), error_ptr);
            if result != difficulty.as_u64() {
                // Use this to generate new NONCE and DIFFICULTY
                // Use ONLY if you know encoding has changed
                let (difficulty, nonce) = generate_nonce_with_min_difficulty(min_difficulty()).unwrap();
                eprintln!("network = {network:?}");
                eprintln!("nonce = {:?}", nonce);
                eprintln!("difficulty = {:?}", difficulty);
                panic!(
                    "detect_change_in_consensus_encoding has failed. This might be a change in consensus encoding \
                     which requires an update to the pool miner code."
                )
            }
            byte_vector_destroy(byte_vec);
        }
    }

    #[test]
    fn check_difficulty() {
        unsafe {
            let network = Network::get_current_or_user_setting_or_default();
            let (difficulty, nonce) = generate_nonce_with_min_difficulty(min_difficulty()).unwrap();
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block = create_test_block();
            let header_bytes = borsh::to_vec(&block.header).unwrap();
            let len = u32::try_from(header_bytes.len()).unwrap();
            let byte_vec = byte_vector_create(header_bytes.as_ptr(), len, error_ptr);
            inject_nonce(byte_vec, nonce, error_ptr);
            assert_eq!(error, 0);
            let result = share_difficulty(byte_vec, u32::from(network.as_byte()), error_ptr);
            assert_eq!(result, difficulty.as_u64());
            byte_vector_destroy(byte_vec);
        }
    }

    #[test]
    fn check_inject_nonce() {
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block = create_test_block();
            let header_bytes = borsh::to_vec(&block.header).unwrap();
            #[allow(clippy::cast_possible_truncation)]
            let len = header_bytes.len() as u32;
            let byte_vec = byte_vector_create(header_bytes.as_ptr(), len, error_ptr);
            inject_nonce(byte_vec, 1234, error_ptr);
            assert_eq!(error, 0);
            let header: BlockHeader = BorshDeserialize::deserialize(&mut (*byte_vec).0.as_slice()).unwrap();
            assert_eq!(header.nonce, 1234);
            byte_vector_destroy(byte_vec);
        }
    }

    #[test]
    fn check_share() {
        unsafe {
            let network = Network::get_current_or_user_setting_or_default();
            let (difficulty, nonce) = generate_nonce_with_min_difficulty(min_difficulty()).unwrap();
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block = create_test_block();
            let hash_hex_broken = CString::new(block.header.hash().to_hex()).unwrap();
            let hash_hex_broken_ptr: *const c_char = CString::into_raw(hash_hex_broken) as *const c_char;
            let mut template_difficulty = 30000;
            let mut share_difficulty = 24000;
            let header_bytes = borsh::to_vec(&block.header).unwrap();
            #[allow(clippy::cast_possible_truncation)]
            let len = header_bytes.len() as u32;
            let byte_vec = byte_vector_create(header_bytes.as_ptr(), len, error_ptr);
            inject_nonce(byte_vec, nonce, error_ptr);
            assert_eq!(error, 0);
            // let calculate for invalid hash
            let result = share_validate(
                byte_vec,
                hash_hex_broken_ptr,
                u32::from(network.as_byte()),
                share_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 2);
            let header: BlockHeader = BorshDeserialize::deserialize(&mut (*byte_vec).0.as_slice()).unwrap();
            let hash = header.hash().to_hex();
            let hash_hex = CString::new(hash.clone()).unwrap();
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            // We need to make sure we did not accidentally mine a good difficulty this must fail both template and
            // share difficulty
            share_difficulty = difficulty.as_u64() + 1000;
            template_difficulty = difficulty.as_u64() + 2000;
            // let calculate for invalid share and target diff
            let result = share_validate(
                byte_vec,
                hash_hex_ptr,
                u32::from(network.as_byte()),
                share_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 4);
            assert_eq!(error, 4);
            // let calculate for valid share and invalid target diff
            share_difficulty = difficulty.as_u64();
            let hash_hex = CString::new(hash.clone()).unwrap();
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let result = share_validate(
                byte_vec,
                hash_hex_ptr,
                u32::from(network.as_byte()),
                share_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 1);
            // let calculate for valid target diff
            template_difficulty = difficulty.as_u64();
            let hash_hex = CString::new(hash).unwrap();
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let result = share_validate(
                byte_vec,
                hash_hex_ptr,
                u32::from(network.as_byte()),
                share_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 0);
            byte_vector_destroy(byte_vec);
        }
    }

    #[test]
    fn check_valid_address() {
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let test_pk = CString::new("5ce83bf62521629ca185098ac24c7b02b184c2e0a2b01455f3a5957d5df94126").unwrap();
            let test_pk_ptr: *const c_char = CString::into_raw(test_pk) as *const c_char;
            let success = public_key_hex_validate(test_pk_ptr, error_ptr);
            assert_eq!(error, 0);
            assert!(success);
        }
    }

    #[test]
    fn check_invalid_address() {
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let test_pk = CString::new("5fe83bf62521629ca185098ac24c7b02b184c2e0a2b01455f3a5957d5df94126").unwrap();
            let test_pk_ptr: *const c_char = CString::into_raw(test_pk) as *const c_char;
            let success = public_key_hex_validate(test_pk_ptr, error_ptr);
            assert!(!success);
            assert_ne!(error, 0);
        }
    }

    #[test]
    fn check_inject_coinbase() {
        unsafe {
            let network = Network::get_current_or_user_setting_or_default();
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let header = BlockHeader::new(0);
            let block =
                NewBlockTemplate::from_block(header.into_builder().build(), Difficulty::min(), 0.into()).unwrap();

            let block_bytes = borsh::to_vec(&block).unwrap();
            #[allow(clippy::cast_possible_truncation)]
            let len = block_bytes.len() as u32;
            let byte_vec = byte_vector_create(block_bytes.as_ptr(), len, error_ptr);

            let address = TariAddress::default();
            let add_string = CString::new(address.to_string()).unwrap();
            let add_ptr: *const c_char = CString::into_raw(add_string) as *const c_char;

            let extra_string = CString::new("a").unwrap();
            let extra_ptr: *const c_char = CString::into_raw(extra_string) as *const c_char;

            inject_coinbase(
                byte_vec,
                100,
                false,
                true,
                add_ptr,
                extra_ptr,
                u32::from(network.as_byte()),
                error_ptr,
            );

            assert_eq!(error, 0);

            let block_temp: NewBlockTemplate = BorshDeserialize::deserialize(&mut (*byte_vec).0.as_slice()).unwrap();

            assert_eq!(block_temp.body.kernels().len(), 1);
            assert_eq!(block_temp.body.outputs().len(), 1);
            assert!(block_temp.body.outputs()[0].features.is_coinbase());
            assert_eq!(block_temp.body.outputs()[0].features.coinbase_extra, vec![97]);
            assert_eq!(block_temp.body.outputs()[0].minimum_value_promise, MicroMinotari(100));
        }
    }
}
