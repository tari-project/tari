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
use std::{convert::TryFrom, ffi::CString, slice};

use borsh::{BorshDeserialize, BorshSerialize};
use libc::{c_char, c_int, c_uchar, c_uint, c_ulonglong};
use tari_core::{blocks::BlockHeader, proof_of_work::sha3x_difficulty};
use tari_crypto::tari_utilities::hex::Hex;

use crate::error::{InterfaceError, MiningHelperError};

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
pub unsafe extern "C" fn share_difficulty(header: *mut ByteVector, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
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
    share_difficulty: c_ulonglong,
    template_difficulty: c_ulonglong,
    error_out: *mut c_int,
) -> c_int {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
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
        let (nonce, difficulty) = match Network::get_current_or_default() {
            Network::MainNet => (9205754023158580549, Difficulty::from_u64(1015).unwrap()),
            Network::StageNet => (12022341430563186162, Difficulty::from_u64(1011).unwrap()),
            _ => panic!("Invalid network for mainnet target"),
        };
        #[cfg(tari_target_network_nextnet)]
        let (nonce, difficulty) = (8721374869059089110, Difficulty::from_u64(3037).unwrap());
        #[cfg(not(any(tari_target_network_mainnet, tari_target_network_nextnet)))]
        let (nonce, difficulty) = (9860518124890236943, Difficulty::from_u64(2724).unwrap());
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block = create_test_block();
            let header_bytes = borsh::to_vec(&block.header).unwrap();
            #[allow(clippy::cast_possible_truncation)]
            let len = header_bytes.len() as u32;
            let byte_vec = byte_vector_create(header_bytes.as_ptr(), len, error_ptr);
            inject_nonce(byte_vec, nonce, error_ptr);
            assert_eq!(error, 0);
            let result = share_difficulty(byte_vec, error_ptr);
            if result != difficulty.as_u64() {
                // Use this to generate new NONCE and DIFFICULTY
                // Use ONLY if you know encoding has changed
                let (difficulty, nonce) = generate_nonce_with_min_difficulty(min_difficulty()).unwrap();
                let network = Network::get_current_or_default();
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
            let (difficulty, nonce) = generate_nonce_with_min_difficulty(min_difficulty()).unwrap();
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block = create_test_block();
            let header_bytes = borsh::to_vec(&block.header).unwrap();
            let len = u32::try_from(header_bytes.len()).unwrap();
            let byte_vec = byte_vector_create(header_bytes.as_ptr(), len, error_ptr);
            inject_nonce(byte_vec, nonce, error_ptr);
            assert_eq!(error, 0);
            let result = share_difficulty(byte_vec, error_ptr);
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
            let result = share_validate(byte_vec, hash_hex_ptr, share_difficulty, template_difficulty, error_ptr);
            assert_eq!(result, 4);
            assert_eq!(error, 4);
            // let calculate for valid share and invalid target diff
            share_difficulty = difficulty.as_u64();
            let hash_hex = CString::new(hash.clone()).unwrap();
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let result = share_validate(byte_vec, hash_hex_ptr, share_difficulty, template_difficulty, error_ptr);
            assert_eq!(result, 1);
            // let calculate for valid target diff
            template_difficulty = difficulty.as_u64();
            let hash_hex = CString::new(hash).unwrap();
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let result = share_validate(byte_vec, hash_hex_ptr, share_difficulty, template_difficulty, error_ptr);
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
}
