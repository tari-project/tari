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

use tari_common_types::types::BlockHash;
use tari_network::identity::PeerId;
use tari_utilities::ByteArray;

use crate::{
    error::{InterfaceError, LibWalletError},
    ByteVector,
};

#[derive(Debug)]
pub struct TariBaseNodeState {
    /// The ID of the base node this wallet is connected to
    pub node_id: Option<PeerId>,

    /// The current chain height, or the block number of the longest valid chain, or zero if there is no chain
    pub best_block_height: u64,

    /// The block hash of the current tip of the longest valid chain
    pub best_block_hash: BlockHash,

    /// Timestamp of the tip block in the longest valid chain
    pub best_block_timestamp: u64,

    /// The configured number of blocks back from the tip that this database tracks. A value of 0 indicates that
    /// pruning mode is disabled and the node will keep full blocks from the time it was set. If pruning horizon
    /// was previously enabled, previously pruned blocks will remain pruned. If set from initial sync, full blocks
    /// are preserved from genesis (i.e. the database is in full archival mode).
    pub pruning_horizon: u64,

    /// The height of the pruning horizon. This indicates from what height a full block can be provided
    /// (exclusive). If `pruned_height` is equal to the `best_block_height` no blocks can be
    /// provided. Archival nodes wil always have an `pruned_height` of zero.
    pub pruned_height: u64,

    pub is_node_synced: bool,
    pub updated_at: u64,
    pub latency: u64,
}

/// Extracts a `NodeId` represented as a vector of bytes wrapped into a `ByteVector`
///
/// ## Arguments
/// `ptr` - The pointer to a `TariBaseNodeState`
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut ByteVector` - Returns a ByteVector or null if the NodeId is None.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn basenode_state_get_node_id(
    ptr: *mut TariBaseNodeState,
    error_out: *mut c_int,
) -> *mut ByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if ptr.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("ptr".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    match (*ptr).node_id {
        None => ptr::null_mut(),
        Some(ref node_id) => Box::into_raw(Box::new(ByteVector(node_id.to_bytes()))),
    }
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

/// Extracts a best block hash [`FixedHash`] represented as a vector of bytes wrapped into a `ByteVector`
///
/// ## Arguments
/// `ptr` - The pointer to a `TariBaseNodeState`
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut ByteVector` - The block hash of the current tip of the longest valid chain. Returns a ByteVector or null if
/// the NodeId is None.  
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn basenode_state_get_best_block(
    ptr: *mut TariBaseNodeState,
    error_out: *mut c_int,
) -> *mut ByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if ptr.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("ptr".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    Box::into_raw(Box::new(ByteVector((*ptr).best_block_hash.to_vec())))
}

/// Extracts a timestamp of the best block
///
/// ## Arguments
/// `ptr` - The pointer to a `TariBaseNodeState`
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Timestamp of the tip block in the longest valid chain
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn basenode_state_get_best_block_timestamp(
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

    (*ptr).best_block_timestamp
}

/// Extracts a pruning horizon
///
/// ## Arguments
/// `ptr` - The pointer to a `TariBaseNodeState`
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The configured number of blocks back from the tip that this database tracks. A value of 0 indicates
/// that pruning mode is disabled and the node will keep full blocks from the time it was set. If pruning horizon
/// was previously enabled, previously pruned blocks will remain pruned. If set from initial sync, full blocks
/// are preserved from genesis (i.e. the database is in full archival mode).
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn basenode_state_get_pruning_horizon(
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

    (*ptr).pruning_horizon
}

/// Extracts a pruned height
///
/// ## Arguments
/// `ptr` - The pointer to a `TariBaseNodeState`
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - The height of the pruning horizon. This indicates from what height a full block can be provided
/// (exclusive). If `pruned_height` is equal to the `best_block_height` no blocks can be
/// provided. Archival nodes wil always have an `pruned_height` of zero.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn basenode_state_get_pruned_height(
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

    (*ptr).pruned_height
}

/// Denotes whether a base node is fully synced or not.
///
/// ## Arguments
/// `ptr` - The pointer to a `TariBaseNodeState`
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut c_ulonglong` - An array of the length of 2 `c_ulonglong`
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn basenode_state_get_is_node_synced(ptr: *mut TariBaseNodeState, error_out: *mut c_int) -> bool {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if ptr.is_null() {
        error = LibWalletError::from(InterfaceError::NullError("ptr".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    }

    (*ptr).is_node_synced
}

/// Extracts the timestamp of when the base node was last updated.
///
/// ## Arguments
/// `ptr` - The pointer to a `TariBaseNodeState`
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_ulonglong` - Timestamp.
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn basenode_state_get_node_updated_at(
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

    (*ptr).updated_at
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
    use tari_common_types::types::{FixedHash, PublicKey};
    use tari_network::ToPeerId;

    use super::*;

    #[test]
    fn test_basenode_state_ffi_accessors() {
        let mut error_code = 0;
        let original_node_id = PublicKey::new_generator("test").unwrap().to_peer_id();
        let original_best_block = BlockHash::zero();

        let boxed_state = Box::into_raw(Box::new(TariBaseNodeState {
            node_id: Some(original_node_id),
            best_block_height: 123,
            best_block_hash: original_best_block,
            best_block_timestamp: 12345,
            pruning_horizon: 456,
            pruned_height: 789,
            is_node_synced: true,
            updated_at: 135,
            latency: 115,
        }));

        unsafe {
            // ----------------------------------------------------------------------------
            // node id

            let wrapped_node_id = basenode_state_get_node_id(boxed_state, &mut error_code);

            assert_eq!(
                original_node_id,
                PeerId::from_bytes((*wrapped_node_id).0.as_bytes()).unwrap()
            );
            assert_eq!(error_code, 0);

            // ----------------------------------------------------------------------------
            // best block

            let mut block_hash = [0u8; FixedHash::byte_size()];
            block_hash.copy_from_slice(
                (*basenode_state_get_best_block(boxed_state, &mut error_code))
                    .0
                    .as_bytes(),
            );

            let best_block = FixedHash::from(block_hash);

            assert_eq!(best_block, original_best_block);
            assert_eq!(error_code, 0);

            // ----------------------------------------------------------------------------
            // other scalars

            assert_eq!(
                basenode_state_get_height_of_the_longest_chain(boxed_state, &mut error_code),
                123
            );
            assert_eq!(error_code, 0);

            assert_eq!(
                basenode_state_get_best_block_timestamp(boxed_state, &mut error_code),
                12345
            );
            assert_eq!(error_code, 0);

            assert_eq!(basenode_state_get_pruning_horizon(boxed_state, &mut error_code), 456);
            assert_eq!(error_code, 0);

            assert_eq!(basenode_state_get_pruned_height(boxed_state, &mut error_code), 789);
            assert_eq!(error_code, 0);

            assert!(basenode_state_get_is_node_synced(boxed_state, &mut error_code));
            assert_eq!(error_code, 0);

            assert_eq!(basenode_state_get_node_updated_at(boxed_state, &mut error_code), 135);
            assert_eq!(error_code, 0);

            assert_eq!(basenode_state_get_latency(boxed_state, &mut error_code), 115);
            assert_eq!(error_code, 0);
        }
    }
}
