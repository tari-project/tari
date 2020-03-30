//  Copyright 2019 The Tari Project
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

use crate::{
    blocks::blockheader::BlockHash,
    chain_storage::{fetch_header, fetch_header_writeguard, BlockchainBackend, ChainMetadata},
    consensus::ConsensusConstants,
    proof_of_work::{
        diff_adj_manager::error::DiffAdjManagerError,
        difficulty::DifficultyAdjustment,
        lwma_diff::LinearWeightedMovingAverage,
        Difficulty,
        PowAlgorithm,
        ProofOfWork,
    },
};
use log::*;
use std::{
    cmp,
    collections::VecDeque,
    sync::{RwLockReadGuard, RwLockWriteGuard},
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hash::Hashable};

pub const LOG_TARGET: &str = "c::pow::diff_adj_manager::diff_adj_storage";

/// The UpdateState enum is used to specify what update operation should be performed to keep the difficulty adjustment
/// system upto date with the blockchain db.
enum UpdateState {
    FullSync,
    SyncToTip,
    Synced,
}

/// DiffAdjManager makes use of DiffAdjStorage to provide thread save access to its LinearWeightedMovingAverages for
/// each PoW algorithm.
pub struct DiffAdjStorage {
    monero_lwma: LinearWeightedMovingAverage,
    blake_lwma: LinearWeightedMovingAverage,
    sync_data: Option<(u64, BlockHash)>,
    timestamps: VecDeque<EpochTime>,
    difficulty_block_window: u64,
    diff_target_block_interval: u64,
    difficulty_max_block_interval: u64,
    median_timestamp_count: usize,
    min_pow_difficulty: Difficulty,
}

impl DiffAdjStorage {
    /// Constructs a new DiffAdjStorage with access to the blockchain db.
    pub fn new(consensus_constants: &ConsensusConstants) -> Self {
        Self {
            monero_lwma: LinearWeightedMovingAverage::new(
                consensus_constants.get_difficulty_block_window() as usize,
                consensus_constants.get_diff_target_block_interval(),
                consensus_constants.min_pow_difficulty(),
                consensus_constants.get_difficulty_max_block_interval(),
            ),
            blake_lwma: LinearWeightedMovingAverage::new(
                consensus_constants.get_difficulty_block_window() as usize,
                consensus_constants.get_diff_target_block_interval(),
                consensus_constants.min_pow_difficulty(),
                consensus_constants.get_difficulty_max_block_interval(),
            ),
            sync_data: None,
            timestamps: VecDeque::new(),
            difficulty_block_window: consensus_constants.get_difficulty_block_window(),
            median_timestamp_count: consensus_constants.get_median_timestamp_count(),
            diff_target_block_interval: consensus_constants.get_diff_target_block_interval(),
            min_pow_difficulty: consensus_constants.min_pow_difficulty(),
            difficulty_max_block_interval: consensus_constants.get_difficulty_max_block_interval(),
        }
    }

    // Check if the difficulty adjustment manager is in sync with specified height. It will also check if a full sync
    // or update sync needs to be performed.
    fn check_sync_state<B: BlockchainBackend>(
        &self,
        db: &RwLockReadGuard<B>,
        block_hash: &BlockHash,
        height: u64,
    ) -> Result<UpdateState, DiffAdjManagerError>
    {
        Ok(match &self.sync_data {
            Some((sync_height, sync_block_hash)) => {
                if *sync_block_hash != *block_hash {
                    if height < *sync_height {
                        UpdateState::FullSync
                    } else {
                        let header = fetch_header(db, *sync_height)?;
                        if *sync_block_hash == header.hash() {
                            UpdateState::SyncToTip
                        } else {
                            UpdateState::FullSync
                        }
                    }
                } else {
                    UpdateState::Synced
                }
            },
            None => UpdateState::FullSync,
        })
    }

    // Check if the difficulty adjustment manager is in sync with specified height. It will also check if a full sync
    // or update sync needs to be performed.
    fn check_sync_state_writeguard<B: BlockchainBackend>(
        &self,
        db: &RwLockWriteGuard<B>,
        block_hash: &BlockHash,
        height: u64,
    ) -> Result<UpdateState, DiffAdjManagerError>
    {
        Ok(match &self.sync_data {
            Some((sync_height, sync_block_hash)) => {
                if *sync_block_hash != *block_hash {
                    if height < *sync_height {
                        UpdateState::FullSync
                    } else {
                        let header = fetch_header_writeguard(db, *sync_height)?;
                        if *sync_block_hash == header.hash() {
                            UpdateState::SyncToTip
                        } else {
                            UpdateState::FullSync
                        }
                    }
                } else {
                    UpdateState::Synced
                }
            },
            None => UpdateState::FullSync,
        })
    }

    // Performs an update on the difficulty adjustment manager based on the detected sync state.
    fn update<B: BlockchainBackend>(
        &mut self,
        db: &RwLockReadGuard<B>,
        height: u64,
    ) -> Result<(), DiffAdjManagerError>
    {
        debug!(
            target: LOG_TARGET,
            "Updating difficulty adjustment manager to height:{}", height
        );
        let block_hash = fetch_header(db, height)?.hash();
        match self.check_sync_state(db, &block_hash, height)? {
            UpdateState::FullSync => self.sync_full_history(db, block_hash, height)?,
            UpdateState::SyncToTip => self.sync_to_chain_tip(db, block_hash, height)?,
            UpdateState::Synced => debug!(
                target: LOG_TARGET,
                "Difficulty adjustment manager is already synced to height:{}", height
            ),
        };
        Ok(())
    }

    // Performs an update on the difficulty adjustment manager based on the detected sync state.
    fn update_writeguard<B: BlockchainBackend>(
        &mut self,
        db: &RwLockWriteGuard<B>,
        height: u64,
    ) -> Result<(), DiffAdjManagerError>
    {
        let block_hash = fetch_header_writeguard(db, height)?.hash();
        match self.check_sync_state_writeguard(db, &block_hash, height)? {
            UpdateState::FullSync => self.sync_full_history_writeguard(db, block_hash, height)?,
            UpdateState::SyncToTip => self.sync_to_chain_tip_writeguard(db, block_hash, height)?,
            UpdateState::Synced => debug!(
                target: LOG_TARGET,
                "Difficulty adjustment manager is already synced to height:{}", height
            ),
        };
        Ok(())
    }

    // Retrieves the height of the longest chain from the blockchain db
    fn get_height_of_longest_chain(
        &self,
        metadata: &RwLockReadGuard<ChainMetadata>,
    ) -> Result<u64, DiffAdjManagerError>
    {
        metadata
            .height_of_longest_chain
            .ok_or_else(|| DiffAdjManagerError::EmptyBlockchain)
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm at the chain tip.
    pub fn get_target_difficulty<B: BlockchainBackend>(
        &mut self,
        metadata: &RwLockReadGuard<ChainMetadata>,
        db: &RwLockReadGuard<B>,
        pow_algo: PowAlgorithm,
    ) -> Result<Difficulty, DiffAdjManagerError>
    {
        let height = self.get_height_of_longest_chain(metadata)?;
        self.get_target_difficulty_at_height(db, pow_algo, height)
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm and provided height.
    pub fn get_target_difficulty_at_height<B: BlockchainBackend>(
        &mut self,
        db: &RwLockReadGuard<B>,
        pow_algo: PowAlgorithm,
        height: u64,
    ) -> Result<Difficulty, DiffAdjManagerError>
    {
        self.update(db, height)?;
        debug!(
            target: LOG_TARGET,
            "Getting target difficulty at height:{} for PoW:{}", height, pow_algo
        );
        Ok(match pow_algo {
            PowAlgorithm::Monero => self.monero_lwma.get_difficulty(),
            PowAlgorithm::Blake => cmp::max(self.min_pow_difficulty, self.blake_lwma.get_difficulty()),
        })
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm and provided height.
    pub fn get_target_difficulty_at_height_writeguard<B: BlockchainBackend>(
        &mut self,
        db: &RwLockWriteGuard<B>,
        pow_algo: PowAlgorithm,
        height: u64,
    ) -> Result<Difficulty, DiffAdjManagerError>
    {
        self.update_writeguard(db, height)?;
        debug!(
            target: LOG_TARGET,
            "Getting target difficulty at height:{} for PoW:{}", height, pow_algo
        );
        Ok(match pow_algo {
            PowAlgorithm::Monero => self.monero_lwma.get_difficulty(),
            PowAlgorithm::Blake => cmp::max(self.min_pow_difficulty, self.blake_lwma.get_difficulty()),
        })
    }

    /// Returns the median timestamp of the past 11 blocks at the chain tip.
    pub fn get_median_timestamp<B: BlockchainBackend>(
        &mut self,
        metadata: &RwLockReadGuard<ChainMetadata>,
        db: &RwLockReadGuard<B>,
    ) -> Result<EpochTime, DiffAdjManagerError>
    {
        let height = self.get_height_of_longest_chain(metadata)?;
        self.get_median_timestamp_at_height(db, height)
    }

    /// Returns the median timestamp of the past 11 blocks at the provided height.
    pub fn get_median_timestamp_at_height<B: BlockchainBackend>(
        &mut self,
        db: &RwLockReadGuard<B>,
        height: u64,
    ) -> Result<EpochTime, DiffAdjManagerError>
    {
        self.update(db, height)?;
        let mut length = self.timestamps.len();
        if length == 0 {
            return Err(DiffAdjManagerError::EmptyBlockchain);
        }
        let mut sorted_timestamps: Vec<EpochTime> = self.timestamps.clone().into();
        sorted_timestamps.sort();
        trace!(target: LOG_TARGET, "sorted median timestamps: {:?}", sorted_timestamps);
        length /= 2; // we want the median, should be index  (MEDIAN_TIMESTAMP_COUNT/2)
        Ok(sorted_timestamps[length])
    }

    /// Returns the median timestamp of the past 11 blocks at the provided height.
    pub fn get_median_timestamp_at_height_writeguard<B: BlockchainBackend>(
        &mut self,
        db: &RwLockWriteGuard<B>,
        height: u64,
    ) -> Result<EpochTime, DiffAdjManagerError>
    {
        self.update_writeguard(db, height)?;
        let mut length = self.timestamps.len();
        if length == 0 {
            return Err(DiffAdjManagerError::EmptyBlockchain);
        }
        let mut sorted_timestamps: Vec<EpochTime> = self.timestamps.clone().into();
        sorted_timestamps.sort();
        trace!(target: LOG_TARGET, "sorted median timestamps: {:?}", sorted_timestamps);
        length /= 2; // we want the median, should be index  (MEDIAN_TIMESTAMP_COUNT/2)
        Ok(sorted_timestamps[length])
    }

    // Resets the DiffAdjStorage.
    fn reset(&mut self) {
        debug!(target: LOG_TARGET, "Resetting difficulty adjustment manager LWMAs");
        self.monero_lwma = LinearWeightedMovingAverage::new(
            self.difficulty_block_window as usize,
            self.diff_target_block_interval,
            self.min_pow_difficulty,
            self.difficulty_max_block_interval,
        );
        self.blake_lwma = LinearWeightedMovingAverage::new(
            self.difficulty_block_window as usize,
            self.diff_target_block_interval,
            self.min_pow_difficulty,
            self.difficulty_max_block_interval,
        );
        self.sync_data = None;
        self.timestamps = VecDeque::new();
    }

    // Adds the new PoW sample to the specific LinearWeightedMovingAverage specified by the PoW algorithm.
    fn add(&mut self, timestamp: EpochTime, pow: ProofOfWork) -> Result<(), DiffAdjManagerError> {
        debug!(
            target: LOG_TARGET,
            "Adding timestamp {} for {}", timestamp, pow.pow_algo
        );
        match pow.pow_algo {
            PowAlgorithm::Monero => {
                let target_difficulty = self.monero_lwma.get_difficulty();
                self.monero_lwma.add(timestamp, target_difficulty)?
            },

            PowAlgorithm::Blake => {
                let target_difficulty = cmp::max(self.min_pow_difficulty, self.blake_lwma.get_difficulty());
                self.blake_lwma.add(timestamp, target_difficulty)?
            },
        }
        Ok(())
    }

    // Resets the DiffAdjStorage and perform a full sync using the blockchain db.
    fn sync_full_history<B: BlockchainBackend>(
        &mut self,
        db: &RwLockReadGuard<B>,
        best_block: BlockHash,
        height_of_longest_chain: u64,
    ) -> Result<(), DiffAdjManagerError>
    {
        self.reset();
        debug!(
            target: LOG_TARGET,
            "Syncing full difficulty adjustment manager history to height:{}", height_of_longest_chain
        );

        // TODO: Store the target difficulty so that we don't have to calculate it for the whole chain
        for height in 0..=height_of_longest_chain {
            let header = fetch_header(db, height)?;
            // keep MEDIAN_TIMESTAMP_COUNT blocks for median timestamp
            // we need to keep the last bunch
            self.timestamps.push_back(header.timestamp);
            if self.timestamps.len() > self.median_timestamp_count {
                let _ = self.timestamps.remove(0);
            }
            self.add(header.timestamp, header.pow)?;
        }
        self.sync_data = Some((height_of_longest_chain, best_block));

        Ok(())
    }

    // Resets the DiffAdjStorage and perform a full sync using the blockchain db.
    fn sync_full_history_writeguard<B: BlockchainBackend>(
        &mut self,
        db: &RwLockWriteGuard<B>,
        best_block: BlockHash,
        height_of_longest_chain: u64,
    ) -> Result<(), DiffAdjManagerError>
    {
        self.reset();
        debug!(
            target: LOG_TARGET,
            "Syncing full difficulty adjustment manager history to height:{}", height_of_longest_chain
        );

        // TODO: Store the target difficulty so that we don't have to calculate it for the whole chain
        for height in 0..=height_of_longest_chain {
            let header = fetch_header_writeguard(db, height)?;
            // keep MEDIAN_TIMESTAMP_COUNT blocks for median timestamp
            // we need to keep the last bunch
            self.timestamps.push_back(header.timestamp);
            if self.timestamps.len() > self.median_timestamp_count {
                let _ = self.timestamps.remove(0);
            }
            self.add(header.timestamp, header.pow)?;
        }
        self.sync_data = Some((height_of_longest_chain, best_block));

        Ok(())
    }

    // The difficulty adjustment manager has fallen behind, perform an update to the chain tip.
    fn sync_to_chain_tip<B: BlockchainBackend>(
        &mut self,
        db: &RwLockReadGuard<B>,
        best_block: BlockHash,
        height_of_longest_chain: u64,
    ) -> Result<(), DiffAdjManagerError>
    {
        if let Some((sync_height, _)) = self.sync_data {
            debug!(
                target: LOG_TARGET,
                "Syncing difficulty adjustment manager from height:{} to height:{}",
                sync_height,
                height_of_longest_chain
            );
            for height in (sync_height + 1)..=height_of_longest_chain {
                let header = fetch_header(db, height)?;
                // add new timestamps
                self.timestamps.push_back(header.timestamp);
                if self.timestamps.len() > self.median_timestamp_count {
                    self.timestamps.remove(0); // remove oldest
                }
                self.add(header.timestamp, header.pow)?;
            }
            self.sync_data = Some((height_of_longest_chain, best_block));
        }
        Ok(())
    }

    // The difficulty adjustment manager has fallen behind, perform an update to the chain tip.
    fn sync_to_chain_tip_writeguard<B: BlockchainBackend>(
        &mut self,
        db: &RwLockWriteGuard<B>,
        best_block: BlockHash,
        height_of_longest_chain: u64,
    ) -> Result<(), DiffAdjManagerError>
    {
        if let Some((sync_height, _)) = self.sync_data {
            debug!(
                target: LOG_TARGET,
                "Syncing difficulty adjustment manager from height:{} to height:{}",
                sync_height,
                height_of_longest_chain
            );
            for height in (sync_height + 1)..=height_of_longest_chain {
                let header = fetch_header_writeguard(db, height)?;
                // add new timestamps
                self.timestamps.push_back(header.timestamp);
                if self.timestamps.len() > self.median_timestamp_count {
                    self.timestamps.remove(0); // remove oldest
                }
                self.add(header.timestamp, header.pow)?;
            }
            self.sync_data = Some((height_of_longest_chain, best_block));
        }
        Ok(())
    }
}
