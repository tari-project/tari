//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! ## Common blockchain utility functions for ArrayLike impls

use crate::chain_storage::ChainStorageError;
use std::cmp;
use tari_mmr::{ArrayLike, ArrayLikeExt, MerkleCheckPoint};

/// Calculate the total leaf node count upto a specified height. If the height is less than the effective pruning
/// horizon the total number of nodes up to the effective pruned height is returned.
pub fn fetch_mmr_nodes_added_count<T>(checkpoints: &T, tip_height: u64, height: u64) -> Result<u32, ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint>,
    ChainStorageError: From<T::Error>,
{
    let cp_count = checkpoints.len()?;
    match cp_count.checked_sub(1) {
        Some(last_index) => {
            let index = last_index.saturating_sub(tip_height.saturating_sub(height) as usize);
            let nodes_added = checkpoints
                .get(index)?
                .map(|cp| cp.accumulated_nodes_added_count())
                .unwrap_or(0);
            Ok(nodes_added)
        },
        None => Ok(0),
    }
}

/// Returns the accumulated node added count.
pub fn fetch_last_mmr_node_added_count<T>(checkpoints: &T) -> Result<u32, ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint>,
    ChainStorageError: From<T::Error>,
{
    let cp_count = checkpoints.len()?;
    match cp_count.checked_sub(1) {
        Some(last_index) => Ok(checkpoints
            .get(last_index)?
            .map(|cp| cp.accumulated_nodes_added_count())
            .unwrap_or(0)),
        None => Ok(0),
    }
}

/// Retrieves the checkpoint corresponding to the provided height, if the checkpoint is part of the horizon state then a
/// BeyondPruningHorizon error will be returned.
pub fn fetch_checkpoint<T>(
    checkpoints: &T,
    pruned_mode: bool,
    tip_height: u64,
    height: u64,
) -> Result<Option<MerkleCheckPoint>, ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint>,
    ChainStorageError: From<T::Error>,
{
    let tip_index = tip_height.saturating_sub(1);
    let height_offset = tip_index
        .checked_sub(height)
        .ok_or_else(|| ChainStorageError::OutOfRange)?;

    let last_cp_index = checkpoints.len()?.saturating_sub(1);
    let index = last_cp_index
        .checked_sub(height_offset as usize)
        .ok_or_else(|| ChainStorageError::BeyondPruningHorizon)?;
    if pruned_mode && index == 0 {
        // In pruned mode the first checkpoint is an accumulation of all checkpoints from the genesis block to horizon
        // block height.
        return Err(ChainStorageError::BeyondPruningHorizon);
    }
    checkpoints.get(index as usize).map_err(Into::into)
}

/// Rewinds checkpoints by `steps_back` elements and returns the last checkpoint.
pub fn rewind_checkpoints<T>(checkpoints: &mut T, steps_back: usize) -> Result<MerkleCheckPoint, ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint> + ArrayLikeExt<Value = MerkleCheckPoint>,
    ChainStorageError: From<T::Error>,
{
    let cp_count = checkpoints.len()?;
    assert!(cp_count > 0, "rewind_checkpoints: `checkpoints` is empty.");
    let rewind_len = cmp::max(cp_count.saturating_sub(steps_back), 1);
    checkpoints.truncate(rewind_len)?;

    let last_cp = checkpoints
        .get(rewind_len - 1)?
        .expect("rewind_checkpoints: `checkpoints` is empty after truncate");

    Ok(last_cp)
}

/// Attempt to merge the set of oldest checkpoints into the horizon state and return the number of checkpoints that have
/// been merged.
pub fn merge_checkpoints<T>(checkpoints: &mut T, max_cp_count: usize) -> Result<(usize, Vec<u32>), ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint> + ArrayLikeExt<Value = MerkleCheckPoint>,
    ChainStorageError: From<T::Error>,
{
    let cp_count = checkpoints.len()?;
    let mut stxo_leaf_indices = Vec::new();
    match (cp_count + 1).checked_sub(max_cp_count) {
        Some(num_cps_merged) => match checkpoints.get(0)? {
            Some(mut merged_cp) => {
                for index in 1..num_cps_merged {
                    if let Some(cp) = checkpoints.get(index)? {
                        stxo_leaf_indices.append(&mut cp.nodes_deleted().to_vec());
                        merged_cp.append(cp);
                    }
                }
                checkpoints.shift(num_cps_merged)?;
                checkpoints.push_front(merged_cp)?;

                Ok((num_cps_merged, stxo_leaf_indices))
            },
            None => Ok((0, stxo_leaf_indices)),
        },
        None => Ok((0, stxo_leaf_indices)),
    }
}
