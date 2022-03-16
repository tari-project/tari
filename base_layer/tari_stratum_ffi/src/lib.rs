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
use std::ffi::CString;

use libc::{c_char, c_int, c_ulonglong};
use tari_core::{
    blocks::Block,
    proof_of_work::{sha3_difficulty, Difficulty},
};
use tari_crypto::tari_utilities::{hex::Hex, message_format::MessageFormat};
use tari_utilities::Hashable;

use crate::error::{InterfaceError, StratumTranscoderError};

pub type TariPublicKey = tari_comms::types::CommsPublicKey;

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
    let native;

    if hex.is_null() {
        error = StratumTranscoderError::from(InterfaceError::Null("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    } else {
        native = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
    }
    let pk = TariPublicKey::from_hex(&native);
    match pk {
        Ok(_pk) => true,
        Err(e) => {
            error = StratumTranscoderError::from(e).code;
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
pub unsafe extern "C" fn inject_nonce(hex: *const c_char, nonce: c_ulonglong, error_out: *mut c_int) -> *const c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let native;

    if hex.is_null() {
        error = StratumTranscoderError::from(InterfaceError::Null("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        ptr::null()
    } else {
        native = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
        let block_hex = hex::decode(native);
        match block_hex {
            Ok(block_hex) => {
                let block: Result<Block, serde_json::Error> =
                    serde_json::from_str(&String::from_utf8_lossy(&block_hex).to_string());
                match block {
                    Ok(mut block) => {
                        block.header.nonce = nonce;
                        let block_json = block.to_json().unwrap();
                        let block_hex = hex::encode(block_json);
                        let result = CString::new(block_hex).unwrap();
                        CString::into_raw(result)
                    },
                    Err(_) => {
                        error = StratumTranscoderError::from(InterfaceError::Conversion("block".to_string())).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        ptr::null()
                    },
                }
            },
            Err(_) => {
                error = StratumTranscoderError::from(InterfaceError::Conversion("hex".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                ptr::null()
            },
        }
    }
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
pub unsafe extern "C" fn share_difficulty(hex: *const c_char, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let block_hex_string;

    if hex.is_null() {
        error = StratumTranscoderError::from(InterfaceError::Null("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    } else {
        block_hex_string = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
    }

    let block_hex = hex::decode(block_hex_string);
    match block_hex {
        Ok(block_hex) => {
            let block: Result<Block, serde_json::Error> =
                serde_json::from_str(&String::from_utf8_lossy(&block_hex).to_string());
            match block {
                Ok(block) => {
                    let difficulty = sha3_difficulty(&block.header);
                    difficulty.as_u64()
                },
                Err(_) => {
                    error = StratumTranscoderError::from(InterfaceError::Conversion("block".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    0
                },
            }
        },
        Err(_) => {
            error = StratumTranscoderError::from(InterfaceError::Conversion("hex".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
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
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn share_validate(
    hex: *const c_char,
    hash: *const c_char,
    stratum_difficulty: c_ulonglong,
    template_difficulty: c_ulonglong,
    error_out: *mut c_int,
) -> c_int {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let block_hex_string;
    let block_hash_string;

    if hex.is_null() {
        error = StratumTranscoderError::from(InterfaceError::Null("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 2;
    } else {
        block_hex_string = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
    }

    if hash.is_null() {
        error = StratumTranscoderError::from(InterfaceError::Null("hash".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 2;
    } else {
        block_hash_string = CString::from_raw(hash as *mut i8).to_str().unwrap().to_owned();
    }

    let block_hex = hex::decode(block_hex_string);
    match block_hex {
        Ok(block_hex) => {
            let block: Result<Block, serde_json::Error> =
                serde_json::from_str(&String::from_utf8_lossy(&block_hex).to_string());
            match block {
                Ok(block) => {
                    if block.header.hash().to_hex() == block_hash_string {
                        // Hash submitted by miner is the same hash produced for the nonce submitted by miner
                        let mut result = 2;
                        let difficulty = sha3_difficulty(&block.header);
                        if difficulty >= Difficulty::from(template_difficulty) {
                            // Valid block
                            result = 0;
                        } else if difficulty >= Difficulty::from(stratum_difficulty) {
                            // Valid share
                            result = 1;
                        } else {
                            // Difficulty not reached
                            error = StratumTranscoderError::from(InterfaceError::LowDifficulty(block_hash_string)).code;
                            ptr::swap(error_out, &mut error as *mut c_int);
                        }
                        result
                    } else {
                        error = StratumTranscoderError::from(InterfaceError::InvalidHash(block_hash_string)).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        2
                    }
                },
                Err(_) => {
                    error = StratumTranscoderError::from(InterfaceError::Conversion("block".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    2
                },
            }
        },
        Err(_) => {
            error = StratumTranscoderError::from(InterfaceError::Conversion("hex".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            2
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::CString, str};

    use libc::{c_char, c_int};
    use tari_core::blocks::Block;
    use tari_crypto::tari_utilities::{hex::Hex, Hashable};

    use crate::{inject_nonce, public_key_hex_validate, share_difficulty, share_validate};

    // Hex representation of block template ("blocktemplate_blob"). A new one can be retrieved by querying the
    // getblocktemplate method of tari_stratum_transcoder when needed. This can be done with Postman by setting up a
    // POST request with the following RAW body: {"id":"0","jsonrpc":"2.0","method":"getblocktemplate"} Note that if
    // this hex string needs to be changed then a new DLL (Windows) and SO (Linux) library will need to be built and
    // committed to the Miningcore codebase since the structure of the template or the underlying data has changed.
    // TODO: Fix tari_stratum_transcoder and get a valid block template hex representation
    const BLOCK_HEX: &str = "";

    // Nonce for the block. Randomly selected, difficulty of at least 10 should be sufficient.
    // TODO: Determine a nonce that will provide an adequate difficulty for these test
    const NONCE: u64 = 995572868245622544;

    // Hash of the block template header AFTER the nonce was injected into it.
    // TODO: Fix tari_stratum_transcoder and get a valid header hex hash
    const HASH_HEX: &str = "";

    // The following function can be used to get the hash for these tests
    // run with --nocapture flag to see output
    #[allow(dead_code)]
    fn get_hash() {
        let block_hex = hex::decode(BLOCK_HEX).unwrap();
        let mut block: Block = serde_json::from_str(&String::from_utf8_lossy(&block_hex).to_string()).unwrap();
        block.header.nonce = NONCE;
        let hash_hex = block.header.hash().to_hex();
        println!("{}", hash_hex);
    }

    // The following function can be used to determine a nonce difficulty for these tests
    // run with --nocapture flag to see output
    #[allow(dead_code)]
    fn nonce_difficulty() {
        unsafe {
            // this can be extended to use a loop to find a nonce matching a set difficulty
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_difficulty(block_hex_ptr2, error_ptr);
            println!("{}", result);
        }
    }

    #[test]
    #[ignore = "to be fixed"]
    fn check_difficulty() {
        // Share Difficulty Achieved (10) = Expected Share Difficulty (10)
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_difficulty(block_hex_ptr2, error_ptr);
            assert_eq!(result, 10);
        }
    }

    #[test]
    #[ignore = "to be fixed"]
    fn check_invalid_share() {
        // Share Difficulty Achieved (10) < Stratum Difficulty (22)
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let hash_hex = CString::new(HASH_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let template_difficulty = 30;
            let stratum_difficulty = 22;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_validate(
                block_hex_ptr2,
                hash_hex_ptr,
                stratum_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 2);
            assert_eq!(error, 4);
        }
    }

    #[test]
    #[ignore = "to be fixed"]
    fn check_valid_share() {
        // Share Difficulty Achieved (10) >= Stratum Difficulty (10)
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let hash_hex = CString::new(HASH_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let template_difficulty = 20;
            let stratum_difficulty = 10;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_validate(
                block_hex_ptr2,
                hash_hex_ptr,
                stratum_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 1);
            assert_eq!(error, 0);
        }
    }

    #[test]
    #[ignore = "to be fixed"]
    fn check_valid_block() {
        // Share Difficulty Achieved (10) >= Network Difficulty (10)
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let hash_hex = CString::new(HASH_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let template_difficulty = 10;
            let stratum_difficulty = 5;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_validate(
                block_hex_ptr2,
                hash_hex_ptr,
                stratum_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 0);
            assert_eq!(error, 0);
        }
    }

    #[test]
    #[ignore = "to be fixed"]
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
    #[ignore = "to be fixed"]
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
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let test_pk = CString::new("5fe83bf62521629ca185098ac24c7b02b184c2e0a2b01455f3a5957d5d").unwrap();
            let test_pk_ptr: *const c_char = CString::into_raw(test_pk) as *const c_char;
            let success = public_key_hex_validate(test_pk_ptr, error_ptr);
            assert!(!success);
            assert_ne!(error, 0);
        }
    }
}
