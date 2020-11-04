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

use crate::{
    backend::ArrayLike,
    error::MerkleMountainRangeError,
    functions::{prune_mutable_mmr, PrunedMutableMmr},
    merkle_checkpoint::MerkleCheckPoint,
    Hash,
    MutableMmr,
};
use croaring::Bitmap;
use digest::Digest;
use std::ops::Deref;

/// Configuration for the MmrCache.
#[derive(Debug, Clone, Copy)]
pub struct MmrCacheConfig {
    /// The rewind_hist_len specifies the point in history upto where the MMR can be efficiently rewound before the
    /// base mmr needs to be reconstructed.
    pub rewind_hist_len: usize,
}

impl Default for MmrCacheConfig {
    fn default() -> Self {
        Self { rewind_hist_len: 100 }
    }
}

/// The MMR cache is used to calculate Merkle and Merklish roots based on the state of the set of shared checkpoints. It
/// can efficiently create an updated cache state when small checkpoint rewinds were detected or the checkpoint state
/// has been expanded.
#[derive(Debug)]
pub struct MmrCache<D, BaseBackend, CpBackend>
where
    D: Digest,
    BaseBackend: ArrayLike<Value = Hash>,
{
    // The last checkpoint index applied to the base MMR.
    base_cp_index: usize,
    // One more than the last checkpoint index applied to the current MMR.
    curr_cp_index: usize,
    // The base MMR is the anchor point of the mmr cache. A rewind can start at this state if the checkpoint tip is
    // beyond the base checkpoint index. It will have to rebuild the base MMR if the checkpoint tip index is less
    // than the base MMR index.
    base_mmr: MutableMmr<D, BaseBackend>,
    // The current mmr represents the latest mmr with all checkpoints applied.
    pub curr_mmr: PrunedMutableMmr<D>,
    // Access to the checkpoint set.
    checkpoints: CpBackend,
    // Configuration for the MMR cache.
    config: MmrCacheConfig,
}

impl<D, BaseBackend, CpBackend> MmrCache<D, BaseBackend, CpBackend>
where
    D: Digest,
    BaseBackend: ArrayLike<Value = Hash>,
    CpBackend: ArrayLike<Value = MerkleCheckPoint>,
{
    /// Creates a new MMR cache with access to the provided set of shared checkpoints.
    pub fn new(
        base_mmr: BaseBackend,
        checkpoints: CpBackend,
        config: MmrCacheConfig,
    ) -> Result<MmrCache<D, BaseBackend, CpBackend>, MerkleMountainRangeError>
    {
        let base_mmr = MutableMmr::new(base_mmr, Bitmap::create());
        let curr_mmr = prune_mutable_mmr::<D, _>(&base_mmr)?;
        let mut mmr_cache = MmrCache {
            base_cp_index: 0,
            curr_cp_index: 0,
            base_mmr,
            curr_mmr,
            checkpoints,
            config,
        };
        mmr_cache.reset()?;
        Ok(mmr_cache)
    }

    // Calculate the base checkpoint index based on the rewind history length and the number of checkpoints.
    fn calculate_base_cp_index(&mut self) -> Result<usize, MerkleMountainRangeError> {
        let cp_count = self
            .checkpoints
            .len()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        if cp_count > self.config.rewind_hist_len {
            return Ok(cp_count - self.config.rewind_hist_len);
        }
        Ok(0)
    }

    // Reconstruct the base MMR using the shared checkpoints. The base MMR contains the state from the the first
    // checkpoint to the checkpoint tip minus the minimum history length.
    fn create_base_mmr(&mut self) -> Result<(), MerkleMountainRangeError> {
        self.base_mmr.clear()?;
        self.base_cp_index = self.calculate_base_cp_index()?;
        for cp_index in 0..=self.base_cp_index {
            if let Some(cp) = self
                .checkpoints
                .get(cp_index)
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            {
                cp.apply(&mut self.base_mmr)?;
            }
        }
        Ok(())
    }

    // Reconstruct the current MMR from the next checkpoint after the base MMR to the last checkpoints.
    fn create_curr_mmr(&mut self) -> Result<(), MerkleMountainRangeError> {
        self.curr_cp_index = self
            .checkpoints
            .len()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        self.curr_mmr = prune_mutable_mmr::<D, _>(&self.base_mmr)?;
        for cp_index in self.base_cp_index + 1..self.curr_cp_index {
            if let Some(cp) = self
                .checkpoints
                .get(cp_index)
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            {
                cp.apply(&mut self.curr_mmr)?;
            }
        }
        Ok(())
    }

    // An update to the checkpoints have been detected, update the base MMR to the correct position.
    fn update_base_mmr(&mut self) -> Result<(), MerkleMountainRangeError> {
        let prev_cp_index = self.base_cp_index;
        self.base_cp_index = self.calculate_base_cp_index()?;
        if prev_cp_index < self.base_cp_index {
            for cp_index in prev_cp_index + 1..=self.base_cp_index {
                if let Some(cp) = self
                    .checkpoints
                    .get(cp_index)
                    .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
                {
                    cp.apply(&mut self.base_mmr)?;
                }
            }
        } else {
            self.create_base_mmr()?;
        }
        Ok(())
    }

    /// Inform the MmrCache that the first N checkpoints have been merged to allow the base and current indices to be
    /// updated.
    pub fn checkpoints_merged(&mut self, num_merged: usize) -> Result<(), MerkleMountainRangeError> {
        if let Some(num_reverse) = num_merged.checked_sub(1) {
            self.base_cp_index = self.base_cp_index.saturating_sub(num_reverse);
            self.curr_cp_index = self.curr_cp_index.saturating_sub(num_reverse);
        }
        self.update()
    }

    /// This function updates the state of the MMR cache based on the current state of the shared checkpoints.
    pub fn update(&mut self) -> Result<(), MerkleMountainRangeError> {
        let cp_count = self
            .checkpoints
            .len()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        if cp_count <= self.base_cp_index {
            // Checkpoint before or the same as the base MMR index, this will require a full reconstruction of the
            // cache.
            self.create_base_mmr()?;
            self.create_curr_mmr()?;
        } else if cp_count < self.curr_cp_index {
            // A short checkpoint reorg has occurred, and requires the current MMR to be reconstructed.
            self.create_curr_mmr()?;
        } else if cp_count > self.curr_cp_index {
            // The cache has fallen behind and needs to update to the new checkpoint state.
            self.update_base_mmr()?;
            self.create_curr_mmr()?;
        }
        Ok(())
    }

    /// Reset the MmrCache and rebuild the base and current MMR state.
    pub fn reset(&mut self) -> Result<(), MerkleMountainRangeError> {
        self.create_base_mmr()?;
        self.create_curr_mmr()
    }

    /// Returns the hash of the leaf index provided, as well as its deletion status. The node has been marked for
    /// deletion if the boolean value is true.
    pub fn fetch_mmr_node(&self, leaf_index: u32) -> Result<(Option<Hash>, bool), MerkleMountainRangeError> {
        let (base_hash, base_deleted) = self.base_mmr.get_leaf_status(leaf_index)?;
        let (curr_hash, curr_deleted) = self.curr_mmr.get_leaf_status(leaf_index)?;
        if let Some(base_hash) = base_hash {
            return Ok((Some(base_hash), base_deleted | curr_deleted));
        }
        Ok((curr_hash, base_deleted | curr_deleted))
    }

    /// Search for the leaf index of the given hash in the nodes of the current and base MMR.
    pub fn find_leaf_index(&self, hash: &[u8]) -> Result<Option<u32>, MerkleMountainRangeError> {
        let mut index = self.base_mmr.find_leaf_index(hash)?;
        if index.is_none() {
            index = self.curr_mmr.find_leaf_index(hash)?;
        }
        Ok(index)
    }
}

impl<D, BaseBackend, CpBackend> Deref for MmrCache<D, BaseBackend, CpBackend>
where
    D: Digest,
    BaseBackend: ArrayLike<Value = Hash>,
{
    type Target = PrunedMutableMmr<D>;

    fn deref(&self) -> &Self::Target {
        &self.curr_mmr
    }
}
