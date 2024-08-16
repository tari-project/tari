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

use std::{
    collections::HashMap,
    ops::{Add, RangeInclusive},
};

use chrono::{DateTime, Duration, Utc};
use tari_common::configuration::Network;
use tari_common_types::epoch::VnEpoch;
use tari_script::{script, OpcodeVersion};
use tari_utilities::epoch_time::EpochTime;

use crate::{
    borsh::SerializedSize,
    consensus::network::NetworkConsensus,
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        tari_amount::{uT, MicroMinotari},
        transaction_components::{
            CoinBaseExtra,
            OutputFeatures,
            OutputFeaturesVersion,
            OutputType,
            RangeProofType,
            TransactionInputVersion,
            TransactionKernelVersion,
            TransactionOutputVersion,
        },
        weight::TransactionWeight,
    },
};

const ANNUAL_BLOCKS: u64 = 30 /* blocks/hr */ * 24 /* hr /d */ * 366 /* days / yr */;

/// This is the inner struct used to control all consensus values.
#[derive(Debug, Clone)]
pub struct ConsensusConstants {
    /// The height at which these constants become effective
    effective_from_height: u64,
    /// The minimum maturity a coinbase utxo must have, in number of blocks
    coinbase_min_maturity: u64,
    /// Current version of the blockchain
    blockchain_version: u16,
    /// The blockchain version that are accepted. Values outside of this range will be rejected.
    valid_blockchain_version_range: RangeInclusive<u16>,
    /// The Future Time Limit (FTL) of the blockchain in seconds. This is the max allowable timestamp that is accepted.
    /// We suggest using T*N/20 where T = desired chain target time, and N = block_window
    future_time_limit: u64,
    /// When doing difficulty adjustments and FTL calculations this is the amount of blocks we look at
    /// <https://github.com/zawy12/difficulty-algorithms/issues/14>
    difficulty_block_window: u64,
    /// Maximum transaction weight used for the construction of new blocks.
    max_block_transaction_weight: u64,
    /// This is how many blocks we use to count towards the median timestamp to ensure the block chain timestamp moves
    /// forward
    median_timestamp_count: usize,
    /// This is the initial emission curve amount
    pub(in crate::consensus) emission_initial: MicroMinotari,
    /// This is the emission curve decay factor as a sum of fraction powers of two. e.g. [1,2] would be 1/2 + 1/4. [2]
    /// would be 1/4
    pub(in crate::consensus) emission_decay: &'static [u64],
    /// The tail emission inflation rate in basis points (bips). 100 bips = 1 percentage_point
    pub(in crate::consensus) inflation_bips: u64,
    /// The length, in blocks of each tail emission epoch (where the reward is held constant)
    pub(in crate::consensus) tail_epoch_length: u64,
    /// This is the maximum age a Monero merge mined seed can be reused
    /// Monero forces a change every height mod 2048 blocks
    max_randomx_seed_height: u64,
    /// Monero Coinbases are unlimited in size, but we limited the extra field to only a certain bytes.
    max_extra_field_size: usize,
    /// This keeps track of the block split targets and which algo is accepted
    /// Ideally this should count up to 100. If this does not you will reduce your target time.
    proof_of_work: HashMap<PowAlgorithm, PowAlgorithmConstants>,
    /// This is to keep track of the value inside of the genesis block
    pre_mine_value: MicroMinotari,
    /// Transaction Weight params
    transaction_weight: TransactionWeight,
    /// Maximum byte size of TariScript
    max_script_byte_size: usize,
    /// Maximum byte size of encrypted data
    max_extra_encrypted_data_byte_size: usize,
    /// Range of valid transaction input versions
    input_version_range: RangeInclusive<TransactionInputVersion>,
    /// Range of valid transaction output (and features) versions
    output_version_range: OutputVersionRange,
    /// Range of valid transaction kernel versions
    kernel_version_range: RangeInclusive<TransactionKernelVersion>,
    /// An allowlist of output types
    permitted_output_types: &'static [OutputType],
    /// The allowlist of range proof types
    permitted_range_proof_types: [(OutputType, &'static [RangeProofType]); 5],
    /// Maximum number of token elements permitted in covenants
    max_covenant_length: u32,
    /// Epoch duration in blocks
    vn_epoch_length: u64,
    /// The number of Epochs that a validator node registration is valid
    vn_validity_period_epochs: VnEpoch,
    /// The min amount of micro Minotari to deposit for a registration transaction to be allowed onto the blockchain
    vn_registration_min_deposit_amount: MicroMinotari,
    /// The period that the registration funds are required to be locked up.
    vn_registration_lock_height: u64,
    /// The period after which the VNs will be reshuffled.
    vn_registration_shuffle_interval: VnEpoch,
}

#[derive(Debug, Clone)]
pub struct OutputVersionRange {
    pub outputs: RangeInclusive<TransactionOutputVersion>,
    pub features: RangeInclusive<OutputFeaturesVersion>,
    pub opcode: RangeInclusive<OpcodeVersion>,
}

/// All V0 for Inputs, Outputs + Features, Kernels
fn version_zero() -> (
    RangeInclusive<TransactionInputVersion>,
    OutputVersionRange,
    RangeInclusive<TransactionKernelVersion>,
) {
    let input_version_range = TransactionInputVersion::V0..=TransactionInputVersion::V0;
    let kernel_version_range = TransactionKernelVersion::V0..=TransactionKernelVersion::V0;
    let output_version_range = OutputVersionRange {
        outputs: TransactionOutputVersion::V0..=TransactionOutputVersion::V0,
        features: OutputFeaturesVersion::V0..=OutputFeaturesVersion::V0,
        opcode: OpcodeVersion::V0..=OpcodeVersion::V0,
    };

    (input_version_range, output_version_range, kernel_version_range)
}

/// This is a convenience struct to put all the info into a hashmap for each algorithm
#[derive(Clone, Debug)]
pub struct PowAlgorithmConstants {
    pub min_difficulty: Difficulty,
    pub max_difficulty: Difficulty,
    pub target_time: u64,
}

const PRE_MINE_VALUE: u64 = 0; // 6_030_157_777_181_012;
const INITIAL_EMISSION: MicroMinotari = MicroMinotari(13_952_877_857);
const ESMERALDA_INITIAL_EMISSION: MicroMinotari = INITIAL_EMISSION;
pub const MAINNET_PRE_MINE_VALUE: MicroMinotari = MicroMinotari((21_000_000_000 - 14_700_000_000) * 1_000_000);

// The target time used by the difficulty adjustment algorithms, their target time is the target block interval * PoW
// algorithm count
impl ConsensusConstants {
    /// The height at which these constants become effective
    pub fn effective_from_height(&self) -> u64 {
        self.effective_from_height
    }

    /// This gets the emission curve values as (initial, decay, inflation_bips, epoch_length)
    pub fn emission_amounts(&self) -> (MicroMinotari, &'static [u64], u64, u64) {
        (
            self.emission_initial,
            self.emission_decay,
            self.inflation_bips,
            self.tail_epoch_length,
        )
    }

    /// The min height maturity a coinbase utxo must have.
    pub fn coinbase_min_maturity(&self) -> u64 {
        self.coinbase_min_maturity
    }

    /// Current version of the blockchain.
    pub fn blockchain_version(&self) -> u16 {
        self.blockchain_version
    }

    /// Returns the valid blockchain version range
    pub fn valid_blockchain_version_range(&self) -> &RangeInclusive<u16> {
        &self.valid_blockchain_version_range
    }

    /// This returns the FTL (Future Time Limit) for blocks.
    /// Any block with a timestamp greater than this is rejected.
    // converting u64 to i64 is okay as the future time limit is the hundreds so way below u32 even
    #[allow(clippy::cast_possible_wrap)]
    pub fn ftl(&self) -> EpochTime {
        // Timestamp never negative
        (Utc::now()
            .add(Duration::seconds(self.future_time_limit as i64))
            .timestamp() as u64)
            .into()
    }

    /// This returns the FTL(Future Time Limit) for blocks
    /// Any block with a timestamp greater than this is rejected.
    /// This function returns the FTL as a UTC datetime
    // converting u64 to i64 is okay as the future time limit is the hundreds so way below u32 even
    #[allow(clippy::cast_possible_wrap)]
    pub fn ftl_as_time(&self) -> DateTime<Utc> {
        Utc::now().add(Duration::seconds(self.future_time_limit as i64))
    }

    /// Monero Coinbases are unlimited in size, but we limited the extra field to only a certain bytes.
    pub fn max_extra_field_size(&self) -> usize {
        self.max_extra_field_size
    }

    /// When doing difficulty adjustments and FTL calculations this is the amount of blocks we look at.
    pub fn difficulty_block_window(&self) -> u64 {
        self.difficulty_block_window
    }

    /// Maximum transaction weight used for the construction of new blocks.
    pub fn max_block_transaction_weight(&self) -> u64 {
        self.max_block_transaction_weight
    }

    /// Maximum transaction weight used for the construction of new blocks. It leaves place for 1 kernel and 1 output
    /// with default features, as well as the maximum possible value of the `coinbase_extra` field
    pub fn max_block_weight_excluding_coinbase(&self) -> std::io::Result<u64> {
        Ok(self.max_block_transaction_weight - self.calculate_1_output_kernel_weight()?)
    }

    fn calculate_1_output_kernel_weight(&self) -> std::io::Result<u64> {
        let output_features = OutputFeatures { ..Default::default() };

        let features_and_scripts_size = self.transaction_weight.round_up_features_and_scripts_size(
            output_features.get_serialized_size()? +
                CoinBaseExtra::default().max_size() +
                script![Nop].get_serialized_size()?,
        );
        Ok(self.transaction_weight.calculate(1, 0, 1, features_and_scripts_size))
    }

    /// The amount of PoW algorithms used by the Tari chain.
    pub fn pow_algo_count(&self) -> u64 {
        self.proof_of_work.len() as u64
    }

    /// The target time used by the difficulty adjustment algorithms, their target time is the target block interval /
    /// algo block percentage
    pub fn pow_target_block_interval(&self, pow_algo: PowAlgorithm) -> u64 {
        match self.proof_of_work.get(&pow_algo) {
            Some(v) => v.target_time,
            _ => 0,
        }
    }

    /// This is how many blocks we use to count towards the median timestamp to ensure the block chain moves forward.
    pub fn median_timestamp_count(&self) -> usize {
        self.median_timestamp_count
    }

    /// The maximum serialized byte size of TariScript
    pub fn max_script_byte_size(&self) -> usize {
        self.max_script_byte_size
    }

    /// The maximum serialized byte size of TariScript
    pub fn max_extra_encrypted_data_byte_size(&self) -> usize {
        self.max_extra_encrypted_data_byte_size
    }

    /// This is the min initial difficulty that can be requested for the pow
    pub fn min_pow_difficulty(&self, pow_algo: PowAlgorithm) -> Difficulty {
        match self.proof_of_work.get(&pow_algo) {
            Some(v) => v.min_difficulty,
            _ => Difficulty::min(),
        }
    }

    /// This will return the value of the genesis block pre-mine
    pub fn pre_mine_value(&self) -> MicroMinotari {
        self.pre_mine_value
    }

    pub fn max_pow_difficulty(&self, pow_algo: PowAlgorithm) -> Difficulty {
        match self.proof_of_work.get(&pow_algo) {
            Some(v) => v.max_difficulty,
            _ => Difficulty::min(),
        }
    }

    /// The maximum age a Monero merge mined seed can be reused
    pub fn max_randomx_seed_height(&self) -> u64 {
        self.max_randomx_seed_height
    }

    /// Gets the transaction weight parameters to calculate the weight of a transaction
    pub fn transaction_weight_params(&self) -> &TransactionWeight {
        &self.transaction_weight
    }

    /// The range of acceptable transaction input versions
    pub fn input_version_range(&self) -> &RangeInclusive<TransactionInputVersion> {
        &self.input_version_range
    }

    /// The range of acceptable transaction output and features versions
    pub fn output_version_range(&self) -> &OutputVersionRange {
        &self.output_version_range
    }

    /// The range of acceptable transaction kernel versions
    pub fn kernel_version_range(&self) -> &RangeInclusive<TransactionKernelVersion> {
        &self.kernel_version_range
    }

    /// Returns the permitted OutputTypes
    pub fn permitted_output_types(&self) -> &[OutputType] {
        self.permitted_output_types
    }

    /// Returns the permitted range proof types
    pub fn permitted_range_proof_types(&self) -> [(OutputType, &[RangeProofType]); 5] {
        self.permitted_range_proof_types
    }

    /// The maximum permitted token length of all covenants. A value of 0 is equivalent to disabling covenants.
    pub fn max_covenant_length(&self) -> u32 {
        self.max_covenant_length
    }

    pub fn validator_node_validity_period_epochs(&self) -> VnEpoch {
        self.vn_validity_period_epochs
    }

    pub fn validator_node_registration_shuffle_interval(&self) -> VnEpoch {
        self.vn_registration_shuffle_interval
    }

    pub fn validator_node_registration_min_deposit_amount(&self) -> MicroMinotari {
        self.vn_registration_min_deposit_amount
    }

    pub fn validator_node_registration_min_lock_height(&self) -> u64 {
        self.vn_registration_lock_height
    }

    /// Returns the current epoch from the given height
    pub fn block_height_to_epoch(&self, height: u64) -> VnEpoch {
        VnEpoch(height / self.vn_epoch_length)
    }

    /// Returns the block height of the start of the given epoch
    pub fn epoch_to_block_height(&self, epoch: VnEpoch) -> u64 {
        epoch.as_u64() * self.vn_epoch_length
    }

    pub fn epoch_length(&self) -> u64 {
        self.vn_epoch_length
    }

    pub fn localnet() -> Vec<Self> {
        let difficulty_block_window = 90;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::min(),
            max_difficulty: Difficulty::min(),
            target_time: 240,
        });
        algos.insert(PowAlgorithm::RandomX, PowAlgorithmConstants {
            min_difficulty: Difficulty::min(),
            max_difficulty: Difficulty::min(),
            target_time: 240,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants = vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 2,
            blockchain_version: 0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window,
            max_block_transaction_weight: 19500,
            median_timestamp_count: 11,
            emission_initial: 18_462_816_327 * uT,
            emission_decay: &ESMERALDA_DECAY_PARAMS,
            inflation_bips: 1000,
            tail_epoch_length: 100,
            max_randomx_seed_height: u64::MAX,
            max_extra_field_size: 200,
            proof_of_work: algos,
            pre_mine_value: 0.into(),
            transaction_weight: TransactionWeight::latest(),
            max_script_byte_size: 512,
            max_extra_encrypted_data_byte_size: 240,
            input_version_range,
            output_version_range,
            kernel_version_range,
            permitted_output_types: OutputType::all(),
            permitted_range_proof_types: Self::all_range_proof_types(),
            max_covenant_length: 100,
            vn_epoch_length: 10,
            vn_validity_period_epochs: VnEpoch(100),
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
        }];
        #[cfg(any(test, debug_assertions))]
        assert_hybrid_pow_constants(&consensus_constants, &[120], &[50], &[50]);
        consensus_constants
    }

    pub fn igor() -> Vec<Self> {
        // `igor` is a test network, so calculating these constants are allowed rather than being hardcoded.
        let randomx_split: u64 = 50;
        let sha3x_split: u64 = 100 - randomx_split;
        let randomx_target_time = 20;
        let sha3x_target_time = randomx_target_time * (100 - sha3x_split) / sha3x_split;
        let target_time: u64 = (randomx_target_time * sha3x_target_time) / (randomx_target_time + sha3x_target_time);
        let difficulty_block_window = 90;
        let future_time_limit = target_time * difficulty_block_window / 20;

        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            // (target_time x 200_000/3) ... for easy testing
            min_difficulty: Difficulty::from_u64(sha3x_target_time * 67_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: sha3x_target_time,
        });
        algos.insert(PowAlgorithm::RandomX, PowAlgorithmConstants {
            // (target_time x 300/3)     ... for easy testing
            min_difficulty: Difficulty::from_u64(randomx_target_time * 100).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: randomx_target_time,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants = vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 6,
            blockchain_version: 0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit,
            difficulty_block_window,
            // 65536 =  target_block_size / bytes_per_gram =  (1024*1024) / 16
            // adj. + 95% = 127,795 - this effectively targets ~2Mb blocks closely matching the previous 19500
            // weightings
            max_block_transaction_weight: 127_795,
            median_timestamp_count: 11,
            emission_initial: 5_538_846_115 * uT,
            emission_decay: &EMISSION_DECAY,
            inflation_bips: 100,
            tail_epoch_length: ANNUAL_BLOCKS,
            max_randomx_seed_height: u64::MAX,
            max_extra_field_size: 200,
            proof_of_work: algos,
            pre_mine_value: 0.into(), // IGOR_PRE_MINE_VALUE.into(),
            transaction_weight: TransactionWeight::v1(),
            max_script_byte_size: 512,
            max_extra_encrypted_data_byte_size: 256,
            input_version_range,
            output_version_range,
            kernel_version_range,
            // igor is the first network to support the new output types
            permitted_output_types: OutputType::all(),
            permitted_range_proof_types: Self::all_range_proof_types(),
            max_covenant_length: 100,
            vn_epoch_length: 10,
            vn_validity_period_epochs: VnEpoch(3),
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
        }];
        #[cfg(any(test, debug_assertions))]
        assert_hybrid_pow_constants(&consensus_constants, &[target_time], &[randomx_split], &[sha3x_split]);
        consensus_constants
    }

    /// *
    /// Esmeralda testnet has the following characteristics:
    /// * 2 min blocks on average (5 min SHA-3, 3 min MM)
    /// * 21 billion tXTR with a 2.76-year half-life
    /// * 800 T tail emission (± 1% inflation after initial 21 billion has been mined)
    /// * Coinbase lock height - 12 hours = 360 blocks
    pub fn esmeralda() -> Vec<Self> {
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(60_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        algos.insert(PowAlgorithm::RandomX, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(60_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants = vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 6,
            blockchain_version: 0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window: 90,
            max_block_transaction_weight: 127_795,
            median_timestamp_count: 11,
            emission_initial: ESMERALDA_INITIAL_EMISSION,
            emission_decay: &ESMERALDA_DECAY_PARAMS,
            inflation_bips: 100,
            tail_epoch_length: ANNUAL_BLOCKS,
            max_randomx_seed_height: 3000,
            max_extra_field_size: 200,
            proof_of_work: algos,
            pre_mine_value: MAINNET_PRE_MINE_VALUE,
            transaction_weight: TransactionWeight::v1(),
            max_script_byte_size: 512,
            max_extra_encrypted_data_byte_size: 256,
            input_version_range,
            output_version_range,
            kernel_version_range,
            permitted_output_types: Self::current_permitted_output_types(),
            permitted_range_proof_types: Self::current_permitted_range_proof_types(),
            max_covenant_length: 0,
            vn_epoch_length: 60,
            vn_validity_period_epochs: VnEpoch(100),
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
        }];
        #[cfg(any(test, debug_assertions))]
        assert_hybrid_pow_constants(&consensus_constants, &[120], &[50], &[50]);
        consensus_constants
    }

    /// *
    /// Stagenet has the following characteristics:
    /// * 2 min blocks on average (5 min SHA-3, 3 min MM)
    /// * 21 billion tXTR with a 3-year half-life
    /// * 800 T tail emission (± 1% inflation after initial 21 billion has been mined)
    /// * Coinbase lock height - 12 hours = 360 blocks
    pub fn stagenet() -> Vec<Self> {
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        algos.insert(PowAlgorithm::RandomX, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants = vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 360,
            blockchain_version: 0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window: 90,
            max_block_transaction_weight: 127_795,
            median_timestamp_count: 11,
            emission_initial: INITIAL_EMISSION,
            emission_decay: &EMISSION_DECAY,
            inflation_bips: 100,
            tail_epoch_length: ANNUAL_BLOCKS,
            max_randomx_seed_height: 3000,
            max_extra_field_size: 200,
            proof_of_work: algos,
            pre_mine_value: PRE_MINE_VALUE.into(),
            transaction_weight: TransactionWeight::v1(),
            max_script_byte_size: 512,
            max_extra_encrypted_data_byte_size: 256,
            input_version_range,
            output_version_range,
            kernel_version_range,
            permitted_output_types: Self::current_permitted_output_types(),
            permitted_range_proof_types: Self::current_permitted_range_proof_types(),
            max_covenant_length: 0,
            vn_epoch_length: 60,
            vn_validity_period_epochs: VnEpoch(100),
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
        }];
        #[cfg(any(test, debug_assertions))]
        assert_hybrid_pow_constants(&consensus_constants, &[120], &[50], &[50]);
        consensus_constants
    }

    pub fn nextnet() -> Vec<Self> {
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        algos.insert(PowAlgorithm::RandomX, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants = vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 360,
            blockchain_version: 0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window: 90,
            max_block_transaction_weight: 127_795,
            median_timestamp_count: 11,
            emission_initial: INITIAL_EMISSION,
            emission_decay: &EMISSION_DECAY,
            inflation_bips: 100,
            tail_epoch_length: ANNUAL_BLOCKS,
            max_randomx_seed_height: 3000,
            max_extra_field_size: 200,
            proof_of_work: algos,
            pre_mine_value: PRE_MINE_VALUE.into(),
            transaction_weight: TransactionWeight::v1(),
            max_script_byte_size: 512,
            max_extra_encrypted_data_byte_size: 256,
            input_version_range,
            output_version_range,
            kernel_version_range,
            permitted_output_types: Self::current_permitted_output_types(),
            permitted_range_proof_types: Self::current_permitted_range_proof_types(),
            max_covenant_length: 0,
            vn_epoch_length: 60,
            vn_validity_period_epochs: VnEpoch(100),
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
        }];
        #[cfg(any(test, debug_assertions))]
        assert_hybrid_pow_constants(&consensus_constants, &[120], &[50], &[50]);
        consensus_constants
    }

    // These values are mainly place holder till the final decision has been made about their values.
    pub fn mainnet() -> Vec<Self> {
        let difficulty_block_window = 90;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        algos.insert(PowAlgorithm::RandomX, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants = vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 1,
            blockchain_version: 1,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window,
            max_block_transaction_weight: 127_795,
            median_timestamp_count: 11,
            emission_initial: INITIAL_EMISSION,
            emission_decay: &EMISSION_DECAY,
            inflation_bips: 100,
            tail_epoch_length: ANNUAL_BLOCKS,
            max_randomx_seed_height: 3000,
            max_extra_field_size: 200,
            proof_of_work: algos,
            pre_mine_value: MAINNET_PRE_MINE_VALUE,
            transaction_weight: TransactionWeight::v1(),
            max_script_byte_size: 512,
            max_extra_encrypted_data_byte_size: 256,
            input_version_range,
            output_version_range,
            kernel_version_range,
            permitted_output_types: Self::current_permitted_output_types(),
            permitted_range_proof_types: Self::current_permitted_range_proof_types(),
            max_covenant_length: 0,
            vn_epoch_length: 60,
            vn_validity_period_epochs: VnEpoch(100),
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
        }];
        #[cfg(any(test, debug_assertions))]
        assert_hybrid_pow_constants(&consensus_constants, &[120], &[50], &[50]);
        consensus_constants
    }

    const fn current_permitted_output_types() -> &'static [OutputType] {
        &[OutputType::Coinbase, OutputType::Standard, OutputType::Burn]
    }

    const fn current_permitted_range_proof_types() -> [(OutputType, &'static [RangeProofType]); 5] {
        [
            (OutputType::Standard, &[RangeProofType::BulletProofPlus]),
            (OutputType::Coinbase, &[
                RangeProofType::BulletProofPlus,
                RangeProofType::RevealedValue,
            ]),
            (OutputType::Burn, &[RangeProofType::BulletProofPlus]),
            (OutputType::ValidatorNodeRegistration, &[
                RangeProofType::BulletProofPlus,
            ]),
            (OutputType::CodeTemplateRegistration, &[RangeProofType::BulletProofPlus]),
        ]
    }

    const fn all_range_proof_types() -> [(OutputType, &'static [RangeProofType]); 5] {
        [
            (OutputType::Standard, RangeProofType::all()),
            (OutputType::Coinbase, RangeProofType::all()),
            (OutputType::Burn, RangeProofType::all()),
            (OutputType::ValidatorNodeRegistration, RangeProofType::all()),
            (OutputType::CodeTemplateRegistration, RangeProofType::all()),
        ]
    }
}

// Assert the hybrid POW constants.
// Note: The math and constants in this function should not be changed without ample consideration that should include
//       discussion with the Tari community, modelling and system level tests.
// For SHA3/Monero to have a 40/60 split:
//   > sha3x_target_time = randomx_target_time * (100 - 40) / 40
//   > randomx_target_time = sha3x_target_time * (100 - 60) / 60
//   > target_time = randomx_target_time * sha3x_target_time / (ramdomx_target_time + sha3x_target_time)
#[cfg(any(test, debug_assertions))]
fn assert_hybrid_pow_constants(
    consensus_constants: &[ConsensusConstants],
    target_time: &[u64],
    randomx_split: &[u64], // RamdomX
    sha3x_split: &[u64],
) {
    assert_eq!(consensus_constants.len(), target_time.len());
    assert_eq!(consensus_constants.len(), randomx_split.len());
    assert_eq!(consensus_constants.len(), sha3x_split.len());

    for (i, constants) in consensus_constants.iter().enumerate() {
        let sha3x_constants = constants
            .proof_of_work
            .get(&PowAlgorithm::Sha3x)
            .expect("Sha3 constants not found");
        let randomx_constants = constants
            .proof_of_work
            .get(&PowAlgorithm::RandomX)
            .expect("RandomX constants not found");

        // POW algorithm dependencies
        // - Basics
        assert!(
            sha3x_constants.min_difficulty <= sha3x_constants.max_difficulty,
            "SHA3X min_difficulty > max_difficulty"
        );
        assert!(
            randomx_constants.min_difficulty <= randomx_constants.max_difficulty,
            "RandomX min_difficulty > max_difficulty"
        );
        // - Target time (the ratios here are important to determine the SHA3/Monero split and overall block time)
        assert_eq!(randomx_split[i] + sha3x_split[i], 100, "Split must add up to 100");
        assert_eq!(
            sha3x_constants.target_time * sha3x_split[i],
            randomx_constants.target_time * (100 - sha3x_split[i]),
            "SHA3 target times are not inversely proportional to SHA3 split"
        );
        assert_eq!(
            randomx_constants.target_time * randomx_split[i],
            sha3x_constants.target_time * (100 - randomx_split[i]),
            "Monero target times are not inversely proportional to Monero split"
        );
        assert_eq!(
            target_time[i] * (randomx_constants.target_time + sha3x_constants.target_time),
            randomx_constants.target_time * sha3x_constants.target_time,
            "Overall target time is not inversely proportional to target split times"
        );
        // General LWMA dependencies
        assert_eq!(
            constants.future_time_limit * 20,
            target_time[i] * constants.difficulty_block_window,
            "20x future_time_limit is not target_time * difficulty_block_window"
        );
    }
}

const EMISSION_DECAY: [u64; 6] = [21u64, 22, 23, 25, 26, 37];
const ESMERALDA_DECAY_PARAMS: [u64; 6] = EMISSION_DECAY; // less significant values don't matter

/// Class to create custom consensus constants
pub struct ConsensusConstantsBuilder {
    consensus: ConsensusConstants,
}

impl ConsensusConstantsBuilder {
    pub fn new(network: Network) -> Self {
        Self {
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
        self.consensus.coinbase_min_maturity = height;
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

    pub fn with_pre_mine_value(mut self, value: MicroMinotari) -> Self {
        self.consensus.pre_mine_value = value;
        self
    }

    pub fn with_emission_amounts(
        mut self,
        intial_amount: MicroMinotari,
        decay: &'static [u64],
        inflation_bips: u64,
        epoch_length: u64,
    ) -> Self {
        self.consensus.emission_initial = intial_amount;
        self.consensus.emission_decay = decay;
        self.consensus.inflation_bips = inflation_bips;
        self.consensus.tail_epoch_length = epoch_length;
        self
    }

    pub fn with_permitted_output_types(mut self, permitted_output_types: &'static [OutputType]) -> Self {
        self.consensus.permitted_output_types = permitted_output_types;
        self
    }

    pub fn with_permitted_range_proof_types(
        mut self,
        permitted_range_proof_types: [(OutputType, &'static [RangeProofType]); 5],
    ) -> Self {
        self.consensus.permitted_range_proof_types = permitted_range_proof_types;
        self
    }

    pub fn with_blockchain_version(mut self, version: u16) -> Self {
        self.consensus.blockchain_version = version;
        self
    }

    pub fn build(self) -> ConsensusConstants {
        self.consensus
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use crate::{
        consensus::{
            emission::{Emission, EmissionSchedule},
            ConsensusConstants,
        },
        transactions::{
            tari_amount::{uT, MicroMinotari},
            transaction_components::{OutputType, RangeProofType},
        },
    };

    #[test]
    fn hybrid_pow_constants_are_well_formed() {
        ConsensusConstants::localnet();
        ConsensusConstants::igor();
        ConsensusConstants::esmeralda();
        ConsensusConstants::stagenet();
        ConsensusConstants::nextnet();
        ConsensusConstants::mainnet();
    }

    #[test]
    fn esmeralda_schedule() {
        let esmeralda = ConsensusConstants::esmeralda();
        let schedule = EmissionSchedule::new(
            esmeralda[0].emission_initial,
            esmeralda[0].emission_decay,
            esmeralda[0].inflation_bips,
            esmeralda[0].tail_epoch_length,
            esmeralda[0].pre_mine_value(),
        );
        // No genesis block coinbase
        assert_eq!(schedule.block_reward(0), MicroMinotari(0));
        // Coinbases starts at block 1
        let coinbase_offset = 1;
        let first_reward = schedule.block_reward(coinbase_offset);
        assert_eq!(first_reward, esmeralda[0].emission_initial);
        assert_eq!(
            schedule.supply_at_block(coinbase_offset),
            first_reward + esmeralda[0].pre_mine_value()
        );
        // 'half_life_block' at approximately '(total supply - pre-mine value) / 2'
        #[allow(clippy::cast_possible_truncation)]
        let half_life_block = 365 * 24 * 30 * 3;
        assert_eq!(
            schedule.supply_at_block(half_life_block + coinbase_offset),
            7_935_818_494_624_306 * uT + esmeralda[0].pre_mine_value()
        );
        // 21 billion
        let mut rewards = schedule
            .iter()
            .skip(3255552 + usize::try_from(coinbase_offset).unwrap());
        let (block_num, reward, supply) = rewards.next().unwrap();
        assert_eq!(block_num, 3255553 + coinbase_offset);
        assert_eq!(reward, 806000000 * uT);
        assert_eq!(supply, 21269867877433906 * uT);
        let (_, reward, _) = rewards.next().unwrap();
        assert_eq!(reward, 806000000 * uT);
        // Inflating tail emission
        let mut rewards = schedule.iter().skip(3259845);
        let (block_num, reward, supply) = rewards.next().unwrap();
        assert_eq!(block_num, 3259846);
        assert_eq!(reward, 806000000.into());
        assert_eq!(supply, 21273327229433906 * uT);
    }

    #[test]
    fn nextnet_schedule() {
        let nextnet = ConsensusConstants::nextnet();
        let schedule = EmissionSchedule::new(
            nextnet[0].emission_initial,
            nextnet[0].emission_decay,
            nextnet[0].inflation_bips,
            nextnet[0].tail_epoch_length,
            nextnet[0].pre_mine_value(),
        );
        // No genesis block coinbase
        assert_eq!(schedule.block_reward(0), MicroMinotari(0));
        // Coinbases starts at block 1
        let coinbase_offset = 1;
        let first_reward = schedule.block_reward(coinbase_offset);
        assert_eq!(first_reward, nextnet[0].emission_initial * uT);
        assert_eq!(
            schedule.supply_at_block(coinbase_offset),
            first_reward + nextnet[0].pre_mine_value()
        );
        // 'half_life_block' at approximately '(total supply - pre-mine value) / 2'
        #[allow(clippy::cast_possible_truncation)]
        let half_life_block = (365.0 * 24.0 * 30.0 * 2.76) as u64;
        assert_eq!(
            schedule.supply_at_block(half_life_block + coinbase_offset),
            7_483_280_506_356_578 * uT + nextnet[0].pre_mine_value()
        );
        // Tail emission
        let mut rewards = schedule.iter().skip(3259845);
        let (block_num, reward, supply) = rewards.next().unwrap();
        assert_eq!(block_num, 3259846);
        assert_eq!(reward, 796_998_899.into());
        assert_eq!(supply, 14_973_269_379_635_607 * uT);
    }

    #[test]
    fn stagenet_schedule() {
        let stagenet = ConsensusConstants::stagenet();
        let schedule = EmissionSchedule::new(
            stagenet[0].emission_initial,
            stagenet[0].emission_decay,
            stagenet[0].inflation_bips,
            stagenet[0].tail_epoch_length,
            stagenet[0].pre_mine_value(),
        );
        // No genesis block coinbase
        assert_eq!(schedule.block_reward(0), MicroMinotari(0));
        // Coinbases starts at block 1
        let coinbase_offset = 1;
        let first_reward = schedule.block_reward(coinbase_offset);
        assert_eq!(first_reward, stagenet[0].emission_initial * uT);
        assert_eq!(
            schedule.supply_at_block(coinbase_offset),
            first_reward + stagenet[0].pre_mine_value()
        );
        // 'half_life_block' at approximately '(total supply - pre-mine value) / 2'
        #[allow(clippy::cast_possible_truncation)]
        let half_life_block = (365.0 * 24.0 * 30.0 * 2.76) as u64;
        assert_eq!(
            schedule.supply_at_block(half_life_block + coinbase_offset),
            7_483_280_506_356_578 * uT + stagenet[0].pre_mine_value()
        );
        // Tail emission
        let mut rewards = schedule.iter().skip(3259845);
        let (block_num, reward, supply) = rewards.next().unwrap();
        assert_eq!(block_num, 3259846);
        assert_eq!(reward, 796_998_899.into());
        assert_eq!(supply, 14_973_269_379_635_607 * uT);
    }

    #[test]
    fn igor_schedule() {
        let igor = ConsensusConstants::igor();
        let schedule = EmissionSchedule::new(
            igor[0].emission_initial,
            igor[0].emission_decay,
            igor[0].inflation_bips,
            igor[0].tail_epoch_length,
            igor[0].pre_mine_value(),
        );
        // No genesis block coinbase
        assert_eq!(schedule.block_reward(0), MicroMinotari(0));
        // Coinbases starts at block 1
        let coinbase_offset = 1;
        let first_reward = schedule.block_reward(coinbase_offset);
        assert_eq!(first_reward, igor[0].emission_initial * uT);
        assert_eq!(schedule.supply_at_block(coinbase_offset), first_reward);
        // Tail emission starts after block 11_084_819
        let rewards = schedule.iter().skip(11_084_819 - 25);
        let mut previous_reward = MicroMinotari(0);
        for (block_num, reward, supply) in rewards {
            if reward == previous_reward {
                assert_eq!(block_num, 11_084_796);
                assert_eq!(supply, MicroMinotari(8_010_884_615_082_026));
                assert_eq!(reward, MicroMinotari(303_000_000));
                break;
            }
            previous_reward = reward;
        }
    }

    // This function is to ensure all OutputType variants are assessed in the tests
    fn cycle_output_type_enum(output_type: OutputType) -> OutputType {
        match output_type {
            OutputType::Standard => OutputType::Coinbase,
            OutputType::Coinbase => OutputType::Burn,
            OutputType::Burn => OutputType::ValidatorNodeRegistration,
            OutputType::ValidatorNodeRegistration => OutputType::CodeTemplateRegistration,
            OutputType::CodeTemplateRegistration => OutputType::Standard,
        }
    }

    // This function is to ensure all RangeProofType variants are assessed in the tests
    fn cycle_range_proof_type_enum(range_proof_type: RangeProofType) -> RangeProofType {
        match range_proof_type {
            RangeProofType::BulletProofPlus => RangeProofType::RevealedValue,
            RangeProofType::RevealedValue => RangeProofType::BulletProofPlus,
        }
    }

    #[test]
    fn range_proof_types_coverage() {
        let mut output_type_enums = vec![OutputType::Standard];
        loop {
            let next_variant = cycle_output_type_enum(*output_type_enums.last().unwrap());
            if output_type_enums.contains(&next_variant) {
                break;
            }
            output_type_enums.push(next_variant);
        }

        let mut range_proof_type_enums = vec![RangeProofType::BulletProofPlus];
        loop {
            let next_variant = cycle_range_proof_type_enum(*range_proof_type_enums.last().unwrap());
            if range_proof_type_enums.contains(&next_variant) {
                break;
            }
            range_proof_type_enums.push(next_variant);
        }

        let permitted_range_proof_types = ConsensusConstants::current_permitted_range_proof_types().to_vec();
        for item in &output_type_enums {
            let entries = permitted_range_proof_types
                .iter()
                .filter(|&&x| x.0 == *item)
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            assert!(!entries[0].1.is_empty());
        }

        let permitted_range_proof_types = ConsensusConstants::all_range_proof_types().to_vec();
        for output_type in &output_type_enums {
            let entries = permitted_range_proof_types
                .iter()
                .filter(|&&x| x.0 == *output_type)
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            for range_proof_type in &range_proof_type_enums {
                assert!(entries[0].1.iter().any(|&x| x == *range_proof_type));
            }
        }
    }
}
