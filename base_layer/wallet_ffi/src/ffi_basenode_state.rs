// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2020. The Tari Project
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

use std::{
    ffi::{c_int, c_ulonglong},
    ptr,
};

use crate::error::{InterfaceError, LibWalletError};

#[derive(Debug, Default, Clone, Copy)]
pub struct TariBaseNodeState {
    /// The current chain height, or the block number of the longest valid chain, or zero if there is no chain
    pub best_block_height: u64,
    /// The latency of the last connection to the base node in microseconds
    pub latency: u64,
}

/// Extracts height of th elongest chain from the `TariBaseNodeState`
///
/// ## Arguments
/// `ptr` - The pointer to a TariBaseNodeState
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The current chain height, or the block number of the longest valid chain, or `None` if there is no
/// chain
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn basenode_state_get_height_of_the_longest_chain(
    ptr: *mut TariBaseNodeState,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if ptr.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("ptr".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    (*ptr).best_block_height
}

/// Extracts the connection latency to the base node.
///
/// ## Arguments
/// `ptr` - The pointer to a `TariBaseNodeState`
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Latency value measured in microseconds.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn basenode_state_get_latency(ptr: *mut TariBaseNodeState, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if ptr.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("ptr".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    (*ptr).latency
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basenode_state_ffi_accessors() {
        let mut error_code = 0;

        let boxed_state = Box::into_raw(Box::new(TariBaseNodeState {
            best_block_height: 115,
            latency: 115,
        }));

        unsafe {
            // ----------------------------------------------------------------------------
            // other scalars

            assert_eq!(
                basenode_state_get_height_of_the_longest_chain(boxed_state, &mut error_code),
                115
            );
            assert_eq!(error_code, 0);

            assert_eq!(basenode_state_get_latency(boxed_state, &mut error_code), 115);
            assert_eq!(error_code, 0);
        }
    }
}
