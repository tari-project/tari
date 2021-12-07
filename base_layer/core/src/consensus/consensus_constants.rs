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
    consensus::network::NetworkConsensus,
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        tari_amount::{uT, MicroTari, T},
        weight::TransactionWeight,
    },
};
use chrono::{DateTime, Duration, Utc};
use std::{collections::HashMap, ops::Add};
use tari_common::configuration::Network;
use tari_crypto::tari_utilities::epoch_time::EpochTime;

/// This is the inner struct used to control all consensus values.
#[derive(Debug, Clone)]
pub struct ConsensusConstants {
    /// The height at which these constants become effective
    effective_from_height: u64,
    /// The min absolute height maturity a coinbase utxo must have
    coinbase_lock_height: u64,
    /// Current version of the blockchain
    blockchain_version: u16,
    /// The Future Time Limit (FTL) of the blockchain in seconds. This is the max allowable timestamp that is excepted.
    /// We use T*N/20 where T = desired chain target time, and N = block_window
    future_time_limit: u64,
    /// When doing difficulty adjustments and FTL calculations this is the amount of blocks we look at
    /// https://github.com/zawy12/difficulty-algorithms/issues/14
    difficulty_block_window: u64,
    /// Maximum transaction weight used for the construction of new blocks.
    max_block_transaction_weight: u64,
    /// This is how many blocks we use to count towards the median timestamp to ensure the block chain moves forward
    median_timestamp_count: usize,
    /// This is the initial emission curve amount
    pub(in crate::consensus) emission_initial: MicroTari,
    /// This is the emission curve delay for the int
    pub(in crate::consensus) emission_decay: &'static [u64],
    /// This is the emission curve tail amount
    pub(in crate::consensus) emission_tail: MicroTari,
    /// This is the maximum age a monero merge mined seed can be reused
    max_randomx_seed_height: u64,
    /// This keeps track of the block split targets and which algo is accepted
    /// Ideally this should count up to 100. If this does not you will reduce your target time.
    proof_of_work: HashMap<PowAlgorithm, PowAlgorithmConstants>,
    /// This is to keep track of the value inside of the genesis block
    faucet_value: MicroTari,
    /// Transaction Weight params
    transaction_weight: TransactionWeight,
    /// Maximum byte size of TariScript
    max_script_byte_size: usize,
}

/// This is just a convenience  wrapper to put all the info into a hashmap per diff algo
#[derive(Clone, Debug)]
pub struct PowAlgorithmConstants {
    /// NB this is very important to set this as 6 * the target time
    pub max_target_time: u64,
    pub min_difficulty: Difficulty,
    pub max_difficulty: Difficulty,
    /// target time is calculated as desired chain target time / block %.
    /// example 120/0.5 = 240 for a 50% of the blocks, chain target time of 120.
    pub target_time: u64,
}

// The target time used by the difficulty adjustment algorithms, their target time is the target block interval * PoW
// algorithm count
impl ConsensusConstants {
    /// The height at which these constants become effective
    pub fn effective_from_height(&self) -> u64 {
        self.effective_from_height
    }

    /// This gets the emission curve values as (initial, decay, tail)
    pub fn emission_amounts(&self) -> (MicroTari, &[u64], MicroTari) {
        (self.emission_initial, self.emission_decay, self.emission_tail)
    }

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

    /// When doing difficulty adjustments and FTL calculations this is the amount of blocks we look at.
    pub fn get_difficulty_block_window(&self) -> u64 {
        self.difficulty_block_window
    }

    /// Maximum transaction weight used for the construction of new blocks.
    pub fn get_max_block_transaction_weight(&self) -> u64 {
        self.max_block_transaction_weight
    }

    /// Maximum transaction weight used for the construction of new blocks. It leaves place for 1 kernel and 1 output
    pub fn get_max_block_weight_excluding_coinbase(&self) -> u64 {
        self.max_block_transaction_weight - self.coinbase_weight()
    }

    pub fn coinbase_weight(&self) -> u64 {
        self.transaction_weight.calculate(1, 0, 1, 0)
    }

    /// The amount of PoW algorithms used by the Tari chain.
    pub fn get_pow_algo_count(&self) -> u64 {
        self.proof_of_work.len() as u64
    }

    /// The target time used by the difficulty adjustment algorithms, their target time is the target block interval /
    /// algo block percentage
    pub fn get_diff_target_block_interval(&self, pow_algo: PowAlgorithm) -> u64 {
        match self.proof_of_work.get(&pow_algo) {
            Some(v) => v.target_time,
            _ => 0,
        }
    }

    /// The maximum time a block is considered to take. Used by the difficulty adjustment algorithms
    /// Multiplied by the PoW algorithm block percentage.
    pub fn get_difficulty_max_block_interval(&self, pow_algo: PowAlgorithm) -> u64 {
        match self.proof_of_work.get(&pow_algo) {
            Some(v) => v.max_target_time,
            _ => 0,
        }
    }

    /// This is how many blocks we use to count towards the median timestamp to ensure the block chain moves forward.
    pub fn get_median_timestamp_count(&self) -> usize {
        self.median_timestamp_count
    }

    /// The maximum serialized byte size of TariScript
    pub fn get_max_script_byte_size(&self) -> usize {
        self.max_script_byte_size
    }

    /// This is the min initial difficulty that can be requested for the pow
    pub fn min_pow_difficulty(&self, pow_algo: PowAlgorithm) -> Difficulty {
        match self.proof_of_work.get(&pow_algo) {
            Some(v) => v.min_difficulty,
            _ => 0.into(),
        }
    }

    /// This will return the value of the genesis block faucets
    pub fn faucet_value(&self) -> MicroTari {
        self.faucet_value
    }

    pub fn max_pow_difficulty(&self, pow_algo: PowAlgorithm) -> Difficulty {
        match self.proof_of_work.get(&pow_algo) {
            Some(v) => v.max_difficulty,
            _ => 0.into(),
        }
    }

    /// The maximum age a monero merge mined seed can be reused
    pub fn max_randomx_seed_height(&self) -> u64 {
        self.max_randomx_seed_height
    }

    pub fn transaction_weight(&self) -> &TransactionWeight {
        &self.transaction_weight
    }

    pub fn localnet() -> Vec<Self> {
        let difficulty_block_window = 90;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3, PowAlgorithmConstants {
            max_target_time: 1800,
            min_difficulty: 1.into(),
            max_difficulty: 1.into(),
            target_time: 300,
        });
        algos.insert(PowAlgorithm::Monero, PowAlgorithmConstants {
            max_target_time: 1200,
            min_difficulty: 1.into(),
            max_difficulty: 1.into(),
            target_time: 200,
        });
        vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_lock_height: 2,
            blockchain_version: 1,
            future_time_limit: 540,
            difficulty_block_window,
            max_block_transaction_weight: 19500,
            median_timestamp_count: 11,
            emission_initial: 5_538_846_115 * uT,
            emission_decay: &EMISSION_DECAY,
            emission_tail: 100.into(),
            max_randomx_seed_height: u64::MAX,
            proof_of_work: algos,
            faucet_value: (5000 * 4000) * T,
            transaction_weight: TransactionWeight::v2(),
            max_script_byte_size: 2048,
        }]
    }

    pub fn ridcully() -> Vec<Self> {
        let difficulty_block_window = 90;
        let mut algos = HashMap::new();
        // setting sha3/monero to 40/60 split
        algos.insert(PowAlgorithm::Sha3, PowAlgorithmConstants {
            max_target_time: 1800,
            min_difficulty: 60_000_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 300,
        });
        algos.insert(PowAlgorithm::Monero, PowAlgorithmConstants {
            max_target_time: 1200,
            min_difficulty: 60_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 200,
        });
        vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_lock_height: 1,
            blockchain_version: 1,
            future_time_limit: 540,
            difficulty_block_window,
            max_block_transaction_weight: 19500,
            median_timestamp_count: 11,
            emission_initial: 5_538_846_115 * uT,
            emission_decay: &EMISSION_DECAY,
            emission_tail: 100.into(),
            max_randomx_seed_height: u64::MAX,
            proof_of_work: algos,
            faucet_value: (5000 * 4000) * T,
            transaction_weight: TransactionWeight::v1(),
            max_script_byte_size: 2048,
        }]
    }

    pub fn stibbons() -> Vec<Self> {
        let mut algos = HashMap::new();
        // Previously these were incorrectly set to `target_time` of 20 and 30, so
        // most blocks before 1400 hit the minimum difficulty of 60M and 60k
        // algos.insert(PowAlgorithm::Sha3, PowAlgorithmConstants {
        //     max_target_time: 1800,
        //     min_difficulty: 60_000_000.into(),
        //     max_difficulty: u64::MAX.into(),
        //     target_time: 30,
        // });
        // algos.insert(PowAlgorithm::Monero, PowAlgorithmConstants {
        //     max_target_time: 1200,
        //     min_difficulty: 60_000.into(),
        //     max_difficulty: u64::MAX.into(),
        //     target_time: 20,
        // });
        algos.insert(PowAlgorithm::Sha3, PowAlgorithmConstants {
            max_target_time: 1800,
            min_difficulty: 60_000_000.into(),
            max_difficulty: 60_000_000.into(),
            target_time: 300,
        });
        algos.insert(PowAlgorithm::Monero, PowAlgorithmConstants {
            max_target_time: 1200,
            min_difficulty: 60_000.into(),
            max_difficulty: 60_000.into(),
            target_time: 200,
        });
        let mut algos2 = HashMap::new();
        // setting sha3/monero to 40/60 split
        algos2.insert(PowAlgorithm::Sha3, PowAlgorithmConstants {
            max_target_time: 1800,
            min_difficulty: 60_000_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 300,
        });
        algos2.insert(PowAlgorithm::Monero, PowAlgorithmConstants {
            max_target_time: 1200,
            min_difficulty: 60_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 200,
        });
        vec![
            ConsensusConstants {
                effective_from_height: 0,
                coinbase_lock_height: 60,
                blockchain_version: 1,
                future_time_limit: 540,
                difficulty_block_window: 90,
                max_block_transaction_weight: 19500,
                median_timestamp_count: 11,
                emission_initial: 5_538_846_115 * uT,
                emission_decay: &EMISSION_DECAY,
                emission_tail: 100.into(),
                max_randomx_seed_height: u64::MAX,
                proof_of_work: algos,
                faucet_value: (5000 * 4000) * T,
                transaction_weight: TransactionWeight::v1(),
                max_script_byte_size: 2048,
            },
            ConsensusConstants {
                effective_from_height: 1400,
                coinbase_lock_height: 60,
                blockchain_version: 1,
                future_time_limit: 540,
                difficulty_block_window: 90,
                max_block_transaction_weight: 19500,
                median_timestamp_count: 11,
                emission_initial: 5_538_846_115 * uT,
                emission_decay: &EMISSION_DECAY,
                emission_tail: 100.into(),
                max_randomx_seed_height: u64::MAX,
                proof_of_work: algos2,
                faucet_value: (5000 * 4000) * T,
                transaction_weight: TransactionWeight::v1(),
                max_script_byte_size: 2048,
            },
        ]
    }

    pub fn weatherwax() -> Vec<Self> {
        let mut algos = HashMap::new();
        // setting sha3/monero to 40/60 split
        algos.insert(PowAlgorithm::Sha3, PowAlgorithmConstants {
            max_target_time: 1800,
            min_difficulty: 60_000_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 300,
        });
        algos.insert(PowAlgorithm::Monero, PowAlgorithmConstants {
            max_target_time: 1200,
            min_difficulty: 60_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 200,
        });
        vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_lock_height: 6,
            blockchain_version: 1,
            future_time_limit: 540,
            difficulty_block_window: 90,
            max_block_transaction_weight: 19500,
            median_timestamp_count: 11,
            emission_initial: 5_538_846_115 * uT,
            emission_decay: &EMISSION_DECAY,
            emission_tail: 100.into(),
            max_randomx_seed_height: u64::MAX,
            proof_of_work: algos,
            faucet_value: (5000 * 4000) * T,
            transaction_weight: TransactionWeight::v1(),
            max_script_byte_size: 2048,
        }]
    }

    pub fn igor() -> Vec<Self> {
        let mut algos = HashMap::new();
        // setting sha3/monero to 40/60 split
        algos.insert(PowAlgorithm::Sha3, PowAlgorithmConstants {
            max_target_time: 1800,
            min_difficulty: 60_000_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 300,
        });
        algos.insert(PowAlgorithm::Monero, PowAlgorithmConstants {
            max_target_time: 1200,
            min_difficulty: 60_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 200,
        });
        vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_lock_height: 6,
            blockchain_version: 2,
            future_time_limit: 540,
            difficulty_block_window: 90,
            // 65536 =  target_block_size / bytes_per_gram =  (1024*1024) / 16
            // adj. + 95% = 127,795 - this effectively targets ~2Mb blocks closely matching the previous 19500
            // weightings
            max_block_transaction_weight: 127_795,
            median_timestamp_count: 11,
            emission_initial: 5_538_846_115 * uT,
            emission_decay: &EMISSION_DECAY,
            emission_tail: 100.into(),
            max_randomx_seed_height: u64::MAX,
            proof_of_work: algos,
            faucet_value: (5000 * 4000) * T,
            transaction_weight: TransactionWeight::v2(),
            max_script_byte_size: 2048,
        }]
    }

    pub fn mainnet() -> Vec<Self> {
        // Note these values are all placeholders for final values
        let difficulty_block_window = 90;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3, PowAlgorithmConstants {
            max_target_time: 1800,
            min_difficulty: 60_000_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 300,
        });
        algos.insert(PowAlgorithm::Monero, PowAlgorithmConstants {
            max_target_time: 800,
            min_difficulty: 60_000_000.into(),
            max_difficulty: u64::MAX.into(),
            target_time: 200,
        });
        vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_lock_height: 1,
            blockchain_version: 1,
            future_time_limit: 540,
            difficulty_block_window,
            max_block_transaction_weight: 19500,
            median_timestamp_count: 11,
            emission_initial: 10_000_000.into(),
            emission_decay: &EMISSION_DECAY,
            emission_tail: 100.into(),
            max_randomx_seed_height: u64::MAX,
            proof_of_work: algos,
            faucet_value: MicroTari::from(0),
            transaction_weight: TransactionWeight::v2(),
            max_script_byte_size: 2048,
        }]
    }
}

static EMISSION_DECAY: [u64; 5] = [22, 23, 24, 26, 27];

/// Class to create custom consensus constants
pub struct ConsensusConstantsBuilder {
    consensus: ConsensusConstants,
}

impl ConsensusConstantsBuilder {
    pub fn new(network: Network) -> Self {
        Self {
            // TODO: Resolve this unwrap
            consensus: NetworkConsensus::from(network)
                .create_consensus_constants()
                .pop()
                .expect("Empty consensus constants"),
        }
    }

    pub fn clear_proof_of_work(mut self) -> Self {
        self.consensus.proof_of_work = HashMap::new();
        self
    }

    pub fn add_proof_of_work(mut self, proof_of_work: PowAlgorithm, constants: PowAlgorithmConstants) -> Self {
        self.consensus.proof_of_work.insert(proof_of_work, constants);
        self
    }

    pub fn with_coinbase_lockheight(mut self, height: u64) -> Self {
        self.consensus.coinbase_lock_height = height;
        self
    }

    pub fn with_max_script_byte_size(mut self, byte_size: usize) -> Self {
        self.consensus.max_script_byte_size = byte_size;
        self
    }

    pub fn with_max_block_transaction_weight(mut self, weight: u64) -> Self {
        self.consensus.max_block_transaction_weight = weight;
        self
    }

    pub fn with_consensus_constants(mut self, consensus: ConsensusConstants) -> Self {
        self.consensus = consensus;
        self
    }

    pub fn with_max_randomx_seed_height(mut self, height: u64) -> Self {
        self.consensus.max_randomx_seed_height = height;
        self
    }

    pub fn with_faucet_value(mut self, value: MicroTari) -> Self {
        self.consensus.faucet_value = value;
        self
    }

    pub fn with_emission_amounts(
        mut self,
        intial_amount: MicroTari,
        decay: &'static [u64],
        tail_amount: MicroTari,
    ) -> Self {
        self.consensus.emission_initial = intial_amount;
        self.consensus.emission_decay = decay;
        self.consensus.emission_tail = tail_amount;
        self
    }

    pub fn build(self) -> ConsensusConstants {
        self.consensus
    }
}
