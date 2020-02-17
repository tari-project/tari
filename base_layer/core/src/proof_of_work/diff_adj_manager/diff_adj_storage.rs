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
    chain_storage::{BlockchainBackend, BlockchainDatabase},
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
use std::collections::VecDeque;
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
pub struct DiffAdjStorage<T>
where T: BlockchainBackend
{
    blockchain_db: BlockchainDatabase<T>,
    monero_lwma: LinearWeightedMovingAverage,
    blake_lwma: LinearWeightedMovingAverage,
    sync_data: Option<(u64, BlockHash)>,
    timestamps: VecDeque<EpochTime>,
}

impl<T> DiffAdjStorage<T>
where T: BlockchainBackend
{
    /// Constructs a new DiffAdjStorage with access to the blockchain db.
    pub fn new(blockchain_db: BlockchainDatabase<T>) -> Self {
        Self {
            blockchain_db,
            monero_lwma: LinearWeightedMovingAverage::default(),
            blake_lwma: LinearWeightedMovingAverage::default(),
            sync_data: None,
            timestamps: VecDeque::new(),
        }
    }

    // Check if the difficulty adjustment manager is in sync with specified height. It will also check if a full sync
    // or update sync needs to be performed.
    fn check_sync_state(&self, block_hash: &BlockHash, height: u64) -> Result<UpdateState, DiffAdjManagerError> {
        Ok(match &self.sync_data {
            Some((sync_height, sync_block_hash)) => {
                if *sync_block_hash != *block_hash {
                    if height < *sync_height {
                        UpdateState::FullSync
                    } else {
                        let header = self.blockchain_db.fetch_header(*sync_height)?;
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
    fn update(&mut self, height: u64) -> Result<(), DiffAdjManagerError> {
        let block_hash = self.blockchain_db.fetch_header(height)?.hash();
        match self.check_sync_state(&block_hash, height)? {
            UpdateState::FullSync => self.sync_full_history(block_hash, height)?,
            UpdateState::SyncToTip => self.sync_to_chain_tip(block_hash, height)?,
            UpdateState::Synced => {},
        };
        Ok(())
    }

    // Retrieves the height of the longest chain from the blockchain db
    fn get_height_of_longest_chain(&mut self) -> Result<u64, DiffAdjManagerError> {
        self.blockchain_db
            .get_metadata()?
            .height_of_longest_chain
            .ok_or_else(|| DiffAdjManagerError::EmptyBlockchain)
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm at the chain tip.
    pub fn get_target_difficulty(&mut self, pow_algo: PowAlgorithm) -> Result<Difficulty, DiffAdjManagerError> {
        let height = self.get_height_of_longest_chain()?;
        self.get_target_difficulty_at_height(pow_algo, height)
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm and provided height.
    pub fn get_target_difficulty_at_height(
        &mut self,
        pow_algo: PowAlgorithm,
        height: u64,
    ) -> Result<Difficulty, DiffAdjManagerError>
    {
        self.update(height)?;
        Ok(match pow_algo {
            PowAlgorithm::Monero => self.monero_lwma.get_difficulty(),
            PowAlgorithm::Blake => self.blake_lwma.get_difficulty(),
        })
    }

    /// Returns the median timestamp of the past 11 blocks at the chain tip.
    pub fn get_median_timestamp(&mut self) -> Result<EpochTime, DiffAdjManagerError> {
        let height = self.get_height_of_longest_chain()?;
        self.get_median_timestamp_at_height(height)
    }

    /// Returns the median timestamp of the past 11 blocks at the provided height.
    pub fn get_median_timestamp_at_height(&mut self, height: u64) -> Result<EpochTime, DiffAdjManagerError> {
        self.update(height)?;
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
        self.monero_lwma = LinearWeightedMovingAverage::default();
        self.blake_lwma = LinearWeightedMovingAverage::default();
        self.sync_data = None;
        self.timestamps = VecDeque::new();
    }

    // Adds the new PoW sample to the specific LinearWeightedMovingAverage specified by the PoW algorithm.
    fn add(&mut self, timestamp: EpochTime, pow: ProofOfWork) -> Result<(), DiffAdjManagerError> {
        match pow.pow_algo {
            PowAlgorithm::Monero => self.monero_lwma.add(timestamp, pow.accumulated_monero_difficulty)?,
            PowAlgorithm::Blake => self.blake_lwma.add(timestamp, pow.accumulated_blake_difficulty)?,
        }
        Ok(())
    }

    // Resets the DiffAdjStorage and perform a full sync using the blockchain db.
    fn sync_full_history(
        &mut self,
        best_block: BlockHash,
        height_of_longest_chain: u64,
    ) -> Result<(), DiffAdjManagerError>
    {
        self.reset();
        let difficulty_block_window = ConsensusConstants::current().get_difficulty_block_window();
        let mut monero_diff_list = Vec::<(EpochTime, Difficulty)>::with_capacity(difficulty_block_window as usize);
        let mut blake_diff_list = Vec::<(EpochTime, Difficulty)>::with_capacity(difficulty_block_window as usize);
        for height in (0..=height_of_longest_chain).rev() {
            let header = self.blockchain_db.fetch_header(height)?;
            // keep MEDIAN_TIMESTAMP_COUNT blocks for median timestamp
            if self.timestamps.len() < ConsensusConstants::current().get_median_timestamp_count() {
                self.timestamps.push_front(header.timestamp);
            }
            match header.pow.pow_algo {
                PowAlgorithm::Monero => {
                    if (monero_diff_list.len() as u64) < difficulty_block_window {
                        monero_diff_list.push((
                            header.timestamp,
                            header.pow.accumulated_monero_difficulty + header.achieved_difficulty(),
                        ));
                    }
                },
                PowAlgorithm::Blake => {
                    if (blake_diff_list.len() as u64) < difficulty_block_window {
                        blake_diff_list.push((
                            header.timestamp,
                            header.pow.accumulated_blake_difficulty + header.achieved_difficulty(),
                        ));
                    }
                },
            }
            if ((monero_diff_list.len() as u64) >= difficulty_block_window) &&
                ((blake_diff_list.len() as u64) >= difficulty_block_window)
            {
                break;
            }
        }
        for (timestamp, accumulated_difficulty) in monero_diff_list.into_iter().rev() {
            self.monero_lwma.add(timestamp, accumulated_difficulty)?
        }
        for (timestamp, accumulated_difficulty) in blake_diff_list.into_iter().rev() {
            self.blake_lwma.add(timestamp, accumulated_difficulty)?
        }
        self.sync_data = Some((height_of_longest_chain, best_block));

        Ok(())
    }

    // The difficulty adjustment manager has fallen behind, perform an update to the chain tip.
    fn sync_to_chain_tip(
        &mut self,
        best_block: BlockHash,
        height_of_longest_chain: u64,
    ) -> Result<(), DiffAdjManagerError>
    {
        if let Some((sync_height, _)) = self.sync_data {
            for height in (sync_height + 1)..=height_of_longest_chain {
                let header = self.blockchain_db.fetch_header(height)?;
                // add new timestamps
                self.timestamps.push_back(header.timestamp);
                if self.timestamps.len() > ConsensusConstants::current().get_median_timestamp_count() {
                    self.timestamps.remove(0); // remove oldest
                }
                match header.pow.pow_algo {
                    PowAlgorithm::Monero => {
                        self.monero_lwma.add(
                            header.timestamp,
                            header.pow.accumulated_monero_difficulty + header.achieved_difficulty(),
                        )?;
                    },
                    PowAlgorithm::Blake => {
                        self.blake_lwma.add(
                            header.timestamp,
                            header.pow.accumulated_blake_difficulty + header.achieved_difficulty(),
                        )?;
                    },
                }
            }
            self.sync_data = Some((height_of_longest_chain, best_block));
        }
        Ok(())
    }
}
