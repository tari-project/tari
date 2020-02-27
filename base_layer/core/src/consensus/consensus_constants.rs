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
    consensus::network::Network,
    transactions::tari_amount::{uT, MicroTari, T},
};
use chrono::{DateTime, Duration, Utc};
use std::ops::Add;
use tari_crypto::tari_utilities::epoch_time::EpochTime;

/// This is the inner struct used to control all consensus values.
#[derive(Clone)]
pub struct ConsensusConstants {
    /// The min height maturity a coinbase utxo must have
    coinbase_lock_height: u64,
    /// Current version of the blockchain
    blockchain_version: u16,
    /// The Future Time Limit (FTL) of the blockchain in seconds. This is the max allowable timestamp that is excepted.
    /// We use TxN/20 where T = target time = 60 seconds, and N = block_window = 150
    future_time_limit: u64,
    /// This is the our target time in seconds between blocks
    target_block_interval: u64,
    /// When doing difficulty adjustments and FTL calculations this is the amount of blocks we look at
    difficulty_block_window: u64,
    /// Maximum transaction weight used for the construction of new blocks.
    max_block_transaction_weight: u64,
    /// The amount of PoW algorithms used by the Tari chain.
    pow_algo_count: u64,
    /// This is how many blocks we use to count towards the median timestamp to ensure the block chain moves forward
    median_timestamp_count: usize,
    /// This is the initial emission curve amount
    pub(in crate::consensus) emission_initial: MicroTari,
    /// This is the emission curve delay
    pub(in crate::consensus) emission_decay: f64,
    /// This is the emission curve tail amount
    pub(in crate::consensus) emission_tail: MicroTari,
}
// The target time used by the difficulty adjustment algorithms, their target time is the target block interval * PoW
// algorithm count
impl ConsensusConstants {
    /// The min height maturity a coinbase utxo must have.
    pub fn coinbase_lock_height(&self) -> u64 {
        self.coinbase_lock_height
    }

    /// Current version of the blockchain.
    pub fn blockchain_version(&self) -> u16 {
        self.blockchain_version
    }

    /// This returns the FTL(Future Time Limit) for blocks
    /// Any block with a timestamp greater than this is rejected.
    pub fn ftl(&self) -> EpochTime {
        (Utc::now()
            .add(Duration::seconds(self.future_time_limit as i64))
            .timestamp() as u64)
            .into()
    }

    /// This returns the FTL(Future Time Limit) for blocks
    /// Any block with a timestamp greater than this is rejected.
    /// This function returns the FTL as a UTC datetime
    pub fn ftl_as_time(&self) -> DateTime<Utc> {
        Utc::now().add(Duration::seconds(self.future_time_limit as i64))
    }

    /// This is the our target time in seconds between blocks.
    pub fn get_target_block_interval(&self) -> u64 {
        self.target_block_interval
    }

    /// When doing difficulty adjustments and FTL calculations this is the amount of blocks we look at.
    pub fn get_difficulty_block_window(&self) -> u64 {
        self.difficulty_block_window
    }

    /// Maximum transaction weight used for the construction of new blocks.
    pub fn get_max_block_transaction_weight(&self) -> u64 {
        self.max_block_transaction_weight
    }

    /// The amount of PoW algorithms used by the Tari chain.
    pub fn get_pow_algo_count(&self) -> u64 {
        self.pow_algo_count
    }

    /// The target time used by the difficulty adjustment algorithms, their target time is the target block interval *
    /// PoW algorithm count.
    pub fn get_diff_target_block_interval(&self) -> u64 {
        self.pow_algo_count * self.target_block_interval
    }

    /// This is how many blocks we use to count towards the median timestamp to ensure the block chain moves forward.
    pub fn get_median_timestamp_count(&self) -> usize {
        self.median_timestamp_count
    }

    pub fn rincewind() -> Self {
        let target_block_interval = 60;
        let difficulty_block_window = 150;
        ConsensusConstants {
            coinbase_lock_height: 1,
            blockchain_version: 1,
            future_time_limit: target_block_interval * difficulty_block_window / 20,
            target_block_interval,
            difficulty_block_window,
            max_block_transaction_weight: 10000, // TODO: a better weight estimate should be selected
            pow_algo_count: 2,
            median_timestamp_count: 11,
            emission_initial: 5_538_846_115 * uT,
            emission_decay: 0.999_999_560_409_038_5,
            emission_tail: 1 * T,
        }
    }

    pub fn localnet() -> Self {
        let target_block_interval = 120;
        let difficulty_block_window = 90;
        ConsensusConstants {
            coinbase_lock_height: 1,
            blockchain_version: 1,
            future_time_limit: target_block_interval * difficulty_block_window / 20,
            target_block_interval,
            difficulty_block_window,
            max_block_transaction_weight: 10000, // TODO: a better weight estimate should be selected
            pow_algo_count: 2,
            median_timestamp_count: 11,
            emission_initial: 10_000_000.into(),
            emission_decay: 0.999,
            emission_tail: 100.into(),
        }
    }

    pub fn mainnet() -> Self {
        // Note these values are all placeholders for final values
        let target_block_interval = 120;
        let difficulty_block_window = 90;
        ConsensusConstants {
            coinbase_lock_height: 1,
            blockchain_version: 1,
            future_time_limit: target_block_interval * difficulty_block_window / 20,
            target_block_interval,
            difficulty_block_window,
            max_block_transaction_weight: 10000,
            pow_algo_count: 2,
            median_timestamp_count: 11,
            emission_initial: 10_000_000.into(),
            emission_decay: 0.999,
            emission_tail: 100.into(),
        }
    }
}

/// Class to create custom consensus constants
pub struct ConsensusConstantsBuilder {
    consensus: ConsensusConstants,
}

impl ConsensusConstantsBuilder {
    pub fn new(network: Network) -> ConsensusConstantsBuilder {
        ConsensusConstantsBuilder {
            consensus: network.create_consensus_constants(),
        }
    }

    pub fn with_coinbase_lockheight(mut self, height: u64) -> ConsensusConstantsBuilder {
        self.consensus.coinbase_lock_height = height;
        self
    }

    pub fn build(self) -> ConsensusConstants {
        self.consensus
    }
}
