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
use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
use tari_common_types::epoch::VnEpoch;
use tari_script::OpcodeVersion;
use tari_utilities::epoch_time::EpochTime;

use crate::{
    consensus::network::NetworkConsensus,
    tari_amount::MicroMinotari,
    tari_proof_of_work::{Difficulty, PowAlgorithm},
    transaction_components::{
        OutputFeaturesVersion,
        OutputType,
        RangeProofType,
        TransactionInputVersion,
        TransactionKernelVersion,
        TransactionOutputVersion,
    },
    weight::TransactionWeight,
};

const ANNUAL_BLOCKS: u64 = 30 /* blocks/hr */ * 24 /* hr /d */ * 366 /* days / yr */;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockVersion {
    V0 = 0,
    V1 = 1,
    V2 = 2,
}

impl BlockVersion {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0 => Some(BlockVersion::V0),
            1 => Some(BlockVersion::V1),
            2 => Some(BlockVersion::V2),
            _ => None,
        }
    }
}

impl TryFrom<u16> for BlockVersion {
    type Error = &'static str;

    fn try_from(value: u16) -> Result<Self, &'static str> {
        match value {
            0 => Ok(BlockVersion::V0),
            1 => Ok(BlockVersion::V1),
            2 => Ok(BlockVersion::V2),
            _ => Err("Unsupported blockchain version"),
        }
    }
}

impl From<BlockVersion> for u16 {
    fn from(value: BlockVersion) -> Self {
        match value {
            BlockVersion::V0 => 0,
            BlockVersion::V1 => 1,
            BlockVersion::V2 => 2,
        }
    }
}

/// This is the inner struct used to control all consensus values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsensusConstants {
    /// The height at which these constants become effective
    effective_from_height: u64,
    /// The minimum maturity a coinbase utxo must have, in number of blocks
    coinbase_min_maturity: u64,
    /// Current version of the blockchain
    blockchain_version: BlockVersion,
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
    /// Maximum coinbases allowed in a block
    max_block_coinbase_count: u64,
    /// This is how many blocks we use to count towards the median timestamp to ensure the block chain timestamp moves
    /// forward
    median_timestamp_count: usize,
    /// This is the initial emission curve amount
    pub(in crate::consensus) emission_initial: MicroMinotari,
    /// This is the emission curve decay factor as a sum of fraction powers of two. e.g. [1,2] would be 1/2 + 1/4. [2]
    /// would be 1/4
    pub(in crate::consensus) emission_decay: Vec<u64>,
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
    permitted_output_types: Vec<OutputType>,
    /// The allowlist of range proof types
    permitted_range_proof_types: Vec<(OutputType, Vec<RangeProofType>)>,
    /// Coinbase outputs are allowed to have metadata, but it has the following length limit
    coinbase_output_features_extra_max_length: u32,
    /// Maximum number of token elements permitted in covenants
    max_covenant_length: u32,
    /// Epoch duration in blocks
    vn_epoch_length: u64,
    /// The min amount of micro Minotari to deposit for a registration transaction to be allowed onto the blockchain
    vn_registration_min_deposit_amount: MicroMinotari,
    /// The period that the registration funds are required to be locked up.
    vn_registration_lock_height: u64,
    /// The period after which the VNs will be reshuffled.
    vn_registration_shuffle_interval: VnEpoch,
    /// Maximum number of validator nodes activated initially
    /// (in the first epoch when we do not have any vns yet).
    vn_registration_max_vns_initial_epoch: u32,
    /// Maximum number of validator nodes activated in an epoch.
    vn_registration_max_vns_per_epoch: u32,
    /// Maximum number of validator nodes that can exit per epoch
    vn_registration_max_exits_per_epoch: u32,
    /// Cuckaroo cycle length
    cuckaroo_cycle_length: u8,
    /// Cuckaroo edge bits
    cuckaroo_edge_bits: u8,
    /// Include c29 accumulated difficulty or not
    include_c29_accumulated_difficulty_into_total: bool,
    /// The cap (`M_MAX`) on the exponential same-algorithm proof of work backoff modifier (TIP-RFC-MT-0004).
    /// A value of `1` disables the backoff entirely (pre-fork behaviour), `32` is the RFC cap.
    pow_backoff_cap: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PowAlgorithmConstants {
    pub min_difficulty: Difficulty,
    pub max_difficulty: Difficulty,
    pub target_time: u64,
}

const PRE_MINE_VALUE: u64 = 0; // 6_030_157_777_181_012;
const INITIAL_EMISSION: MicroMinotari = MicroMinotari(13_952_877_857);
const ESMERALDA_INITIAL_EMISSION: MicroMinotari = INITIAL_EMISSION;
pub const MAINNET_PRE_MINE_VALUE: MicroMinotari = MicroMinotari((21_000_000_000 - 14_700_000_000) * 1_000_000);

/// The cap (`M_MAX`) on the exponential same-algorithm proof of work backoff modifier, as specified by
/// TIP-RFC-MT-0004. This must stay in sync with `tari_core::proof_of_work::MAX_POW_BACKOFF_MODIFIER`.
pub const POW_BACKOFF_CAP: u64 = 32;
/// A `pow_backoff_cap` of 1 disables the same-algorithm backoff, which is the pre-TIP-RFC-MT-0004 behaviour.
pub const POW_BACKOFF_DISABLED: u64 = 1;
/// The LWMA difficulty block window after TIP-RFC-MT-0004 activates (shortened from 90 for faster response to hash
/// rate swings).
pub const TIP004_DIFFICULTY_BLOCK_WINDOW: u64 = 45;
// TIP-RFC-MT-0004 activation heights.
//
// Every network with live history gets a gated activation entry rather than a change to its height-0 constants:
// shortening `difficulty_block_window` or enabling `pow_backoff_cap` retroactively would make every historical block
// recompute to a different target than the one recorded in its `BlockHeaderAccumulatedData`, so a fresh sync would
// reject at roughly the first block past the window and existing nodes would report that active constants changed.
// This follows how every previous consensus change on these networks was rolled out (see the `include_c29...` entries
// below).
//
// TODO: All five heights are placeholders. `u64::MAX` means the fork never activates, which keeps each network on its
// current rules until a height is chosen. These MUST be set before release.
/// Sentinel activation height meaning "this fork has no scheduled height yet". `ConsensusConstantsBuilder::new`
/// skips entries gated on it, so unscheduled forks do not leak into test fixtures as if they were live rules.
pub const UNSCHEDULED_ACTIVATION_HEIGHT: u64 = u64::MAX;
/// TIP-RFC-MT-0004 activation height for MainNet.
pub const MAINNET_TIP004_ACTIVATION_HEIGHT: u64 = UNSCHEDULED_ACTIVATION_HEIGHT;
/// TIP-RFC-MT-0004 activation height for StageNet.
pub const STAGENET_TIP004_ACTIVATION_HEIGHT: u64 = UNSCHEDULED_ACTIVATION_HEIGHT;
/// TIP-RFC-MT-0004 activation height for NextNet.
pub const NEXTNET_TIP004_ACTIVATION_HEIGHT: u64 = UNSCHEDULED_ACTIVATION_HEIGHT;
/// TIP-RFC-MT-0004 activation height for Esmeralda.
pub const ESMERALDA_TIP004_ACTIVATION_HEIGHT: u64 = UNSCHEDULED_ACTIVATION_HEIGHT;
/// TIP-RFC-MT-0004 activation height for Igor.
pub const IGOR_TIP004_ACTIVATION_HEIGHT: u64 = UNSCHEDULED_ACTIVATION_HEIGHT;

// The target time used by the difficulty adjustment algorithms, their target time is the target block interval * PoW
// algorithm count
impl ConsensusConstants {
    const MAINNET_MAX_WEIGHT_V1: u64 = 90_000;

    /// All consensus constants entries for a network, in the order they become effective.
    pub fn for_network(network: Network) -> Vec<Self> {
        match network {
            Network::LocalNet => ConsensusConstants::localnet(),
            Network::Igor => ConsensusConstants::igor(),
            Network::MainNet => ConsensusConstants::mainnet(),
            Network::Esmeralda => ConsensusConstants::esmeralda(),
            Network::StageNet => ConsensusConstants::stagenet(),
            Network::NextNet => ConsensusConstants::nextnet(),
        }
    }

    /// The single authoritative answer to "which constants are in force at `height`".
    ///
    /// This walks the vector in order and stops at the first entry that is not yet effective, which means a later
    /// entry always wins over an earlier one carrying the same or a higher effective height. That is the behaviour
    /// the node actually runs on, so every other consumer must agree with it. Selecting by "greatest effective
    /// height" instead is only equivalent while the vector is sorted, and a divergence there means two components
    /// disagreeing about the live rules. `activation_test` pins every consumer against this function.
    ///
    /// Returns `None` only for an empty slice. If no entry is effective yet the first is returned, matching the
    /// long standing behaviour of `ConsensusManager::consensus_constants`.
    pub fn active_at_height(constants: &[Self], height: u64) -> Option<&Self> {
        constants.get(Self::active_index_at_height(constants, height)?)
    }

    /// The index of the entry [`ConsensusConstants::active_at_height`] would select. Callers that need to look at the
    /// neighbouring entry - the coinbase maturity tranches do, to work out the previous maturity - must use this
    /// rather than searching the vector by value, so that there is only one definition of "active".
    pub fn active_index_at_height(constants: &[Self], height: u64) -> Option<usize> {
        if constants.is_empty() {
            return None;
        }
        let mut active = 0;
        for (index, c) in constants.iter().enumerate() {
            if c.effective_from_height > height {
                break;
            }
            active = index;
        }
        Some(active)
    }

    /// True if these constants carry the same rules as `other`, ignoring the height they become effective from.
    ///
    /// This answers "are these two rule sets the same", *not* "is consensus unchanged". Comparing the entries
    /// selected at a single height is not sufficient to conclude the latter: moving an activation height can leave
    /// the same entry selected at that height while changing which entry applies over the range the height moved
    /// across. A caller asking about consensus must therefore evaluate this at every effective height of both
    /// vectors, not only at the tip - see `ConsensusConstantsTracker::check_for_changes`.
    ///
    /// Note also that `effective_from_height` is not purely descriptive: header sync compares it against the block
    /// height to decide when to refresh the permitted algorithms, difficulty window and backoff cap, and the coinbase
    /// maturity tranches do arithmetic on it. It is excluded here only because the *selection* already accounts for
    /// it, and only for callers that sweep the breakpoints as described above.
    pub fn has_same_rules_as(&self, other: &Self) -> bool {
        let mut this = self.clone();
        let mut that = other.clone();
        this.effective_from_height = 0;
        that.effective_from_height = 0;
        this == that
    }

    pub fn for_network_at_height(network: Network, height: u64) -> Self {
        let versions = Self::for_network(network);
        Self::active_at_height(&versions, height)
            .expect("There is always at least one consensus version")
            .clone()
    }

    /// The height at which these constants become effective
    pub fn effective_from_height(&self) -> u64 {
        self.effective_from_height
    }

    /// This gets the emission curve values as (initial, decay, inflation_bips, epoch_length)
    pub fn emission_amounts(&self) -> (MicroMinotari, &[u64], u64, u64) {
        (
            self.emission_initial,
            &self.emission_decay,
            self.inflation_bips,
            self.tail_epoch_length,
        )
    }

    /// The min height maturity a coinbase utxo must have.
    pub fn coinbase_min_maturity(&self) -> u64 {
        self.coinbase_min_maturity
    }

    /// Current version of the blockchain.
    pub fn blockchain_version(&self) -> BlockVersion {
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

    /// Maximum block coinbases used for construction of new blocks.
    pub fn max_block_coinbase_count(&self) -> u64 {
        self.max_block_coinbase_count
    }

    pub fn coinbase_output_features_extra_max_length(&self) -> u32 {
        self.coinbase_output_features_extra_max_length
    }

    /// The amount of PoW algorithms used by the Tari chain.
    pub fn pow_algo_count(&self) -> u64 {
        self.proof_of_work.len() as u64
    }

    // Should only be used in tests
    pub fn set_pow_target_block_interval(&mut self, pow_algo: PowAlgorithm, target_time: u64) {
        if let Some(v) = self.proof_of_work.get_mut(&pow_algo) {
            v.target_time = target_time;
        }
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
        &self.permitted_output_types
    }

    /// Returns the permitted range proof types
    pub fn permitted_range_proof_types(&self) -> &[(OutputType, Vec<RangeProofType>)] {
        &self.permitted_range_proof_types
    }

    /// The maximum permitted token length of all covenants. A value of 0 is equivalent to disabling covenants.
    pub fn max_covenant_length(&self) -> u32 {
        self.max_covenant_length
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
        // Every network's consensus constants define a non-zero epoch length.
        VnEpoch(
            height
                .checked_div(self.vn_epoch_length)
                .expect("vn_epoch_length must be non-zero"),
        )
    }

    /// Returns the block height of the start of the given epoch
    pub fn epoch_to_block_height(&self, epoch: VnEpoch) -> u64 {
        epoch.as_u64().saturating_mul(self.vn_epoch_length)
    }

    pub fn vn_registration_max_vns_initial_epoch(&self) -> u32 {
        self.vn_registration_max_vns_initial_epoch
    }

    pub fn vn_registration_max_vns_per_epoch(&self) -> u32 {
        self.vn_registration_max_vns_per_epoch
    }

    pub fn vn_registration_max_exits_per_epoch(&self) -> u32 {
        self.vn_registration_max_exits_per_epoch
    }

    pub fn epoch_length(&self) -> u64 {
        self.vn_epoch_length
    }

    pub fn current_permitted_pow_algos(&self) -> Vec<PowAlgorithm> {
        self.proof_of_work.keys().copied().collect()
    }

    pub fn cuckaroo_cycle_length(&self) -> u8 {
        self.cuckaroo_cycle_length
    }

    pub fn cuckaroo_edge_bits(&self) -> u8 {
        self.cuckaroo_edge_bits
    }

    pub fn include_c29_accumulated_difficulty_into_total(&self) -> bool {
        self.include_c29_accumulated_difficulty_into_total
    }

    /// The cap on the exponential same-algorithm proof of work backoff modifier (TIP-RFC-MT-0004). `1` disables the
    /// backoff.
    pub fn pow_backoff_cap(&self) -> u64 {
        self.pow_backoff_cap
    }

    pub fn localnet() -> Vec<Self> {
        // LocalNet is ephemeral (no persistent chain to invalidate), so TIP-RFC-MT-0004 applies from height 0. Note
        // that LocalNet sets `min_difficulty == max_difficulty == 1`, so the backoff clamps to a no-op there.
        let difficulty_block_window = TIP004_DIFFICULTY_BLOCK_WINDOW;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::min(),
            max_difficulty: Difficulty::min(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::min(),
            max_difficulty: Difficulty::min(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::RandomXT, PowAlgorithmConstants {
            min_difficulty: Difficulty::min(),
            max_difficulty: Difficulty::min(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::Cuckaroo, PowAlgorithmConstants {
            min_difficulty: Difficulty::min(),
            max_difficulty: Difficulty::min(),
            target_time: 360,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants = vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 2,
            blockchain_version: BlockVersion::V0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window,
            max_block_transaction_weight: ConsensusConstants::MAINNET_MAX_WEIGHT_V1,
            max_block_coinbase_count: 1000,
            median_timestamp_count: 11,
            emission_initial: MicroMinotari::from(18_462_816_327u64),
            emission_decay: EMISSION_DECAY.to_vec(),
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
            vn_registration_min_deposit_amount: MicroMinotari(1000),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
            coinbase_output_features_extra_max_length: 256,
            vn_registration_max_vns_initial_epoch: 50,
            vn_registration_max_vns_per_epoch: 10,
            vn_registration_max_exits_per_epoch: 5,
            cuckaroo_cycle_length: 42,
            cuckaroo_edge_bits: 29,
            include_c29_accumulated_difficulty_into_total: true,
            pow_backoff_cap: POW_BACKOFF_CAP,
        }];
        consensus_constants
    }

    pub fn igor() -> Vec<Self> {
        // `igor` is a test network, so calculating these constants are allowed rather than being hardcoded.
        let randomx_split: u64 = 50;
        let sha3x_split: u64 = 100u64.saturating_sub(randomx_split);
        let randomx_target_time: u64 = 20;
        let sha3x_target_time = randomx_target_time
            .saturating_mul(100u64.saturating_sub(sha3x_split))
            .checked_div(sha3x_split)
            .expect("sha3x_split is non-zero");
        let target_time: u64 = randomx_target_time
            .saturating_mul(sha3x_target_time)
            .checked_div(randomx_target_time.saturating_add(sha3x_target_time))
            .expect("target times are non-zero");
        let difficulty_block_window: u64 = 90;
        let future_time_limit = target_time.saturating_mul(difficulty_block_window) / 20;

        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            // (target_time x 200_000/3) ... for easy testing
            min_difficulty: Difficulty::from_u64(sha3x_target_time.saturating_mul(67_000)).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: sha3x_target_time,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            // (target_time x 300/3)     ... for easy testing
            min_difficulty: Difficulty::from_u64(randomx_target_time * 100).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: randomx_target_time,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants = vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 6,
            blockchain_version: BlockVersion::V0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit,
            difficulty_block_window,
            // 65536 =  target_block_size / bytes_per_gram =  (1024*1024) / 16
            // adj. + 95% = 127,795 - this effectively targets ~2Mb blocks closely matching the previous 19500
            // weightings
            max_block_transaction_weight: 127_795,
            max_block_coinbase_count: 1000,
            median_timestamp_count: 11,
            emission_initial: MicroMinotari::from(5_538_846_115u64),
            emission_decay: EMISSION_DECAY.to_vec(),
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
            vn_registration_min_deposit_amount: MicroMinotari(1000),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
            coinbase_output_features_extra_max_length: 256,
            vn_registration_max_vns_initial_epoch: 50,
            vn_registration_max_vns_per_epoch: 10,
            vn_registration_max_exits_per_epoch: 5,
            cuckaroo_cycle_length: 42,
            cuckaroo_edge_bits: 29,
            include_c29_accumulated_difficulty_into_total: true,
            pow_backoff_cap: POW_BACKOFF_DISABLED,
        }];
        Self::with_tip004_activation(consensus_constants, IGOR_TIP004_ACTIVATION_HEIGHT)
    }

    /// *
    /// Esmeralda testnet has the following characteristics:
    /// * 2 min blocks on average (5 min SHA-3, 3 min MM)
    /// * 21 billion tXTM with a 2.76-year half-life
    /// * 800 T tail emission (± 1% inflation after initial 21 billion has been mined)
    /// * Coinbase lock height - 12 hours = 360 blocks
    #[allow(clippy::too_many_lines)]
    pub fn esmeralda() -> Vec<Self> {
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(60_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 60,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(60_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 60,
        });

        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants1 = ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 6,
            blockchain_version: BlockVersion::V0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window: 90,
            max_block_transaction_weight: 127_795,
            max_block_coinbase_count: 1000,
            median_timestamp_count: 11,
            emission_initial: ESMERALDA_INITIAL_EMISSION,
            emission_decay: EMISSION_DECAY.to_vec(),
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
            vn_epoch_length: 80, // 15s per block * 80 ±= 20 mins
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
            coinbase_output_features_extra_max_length: 256,
            vn_registration_max_vns_initial_epoch: 0,
            vn_registration_max_vns_per_epoch: 0,
            vn_registration_max_exits_per_epoch: 0,
            cuckaroo_cycle_length: 42,
            cuckaroo_edge_bits: 29,
            include_c29_accumulated_difficulty_into_total: false,
            pow_backoff_cap: POW_BACKOFF_DISABLED,
        };

        let mut con2 = consensus_constants1.clone();
        con2.effective_from_height = 52000;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(60_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::from_u64(60_000_000_000).expect("valid difficulty"),
            target_time: 60,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(60_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 60,
        });
        algos.insert(PowAlgorithm::RandomXT, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(600).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 60,
        });
        con2.blockchain_version = BlockVersion::V0; // Historical error, should be V1
        con2.proof_of_work = algos;

        let mut con3 = con2.clone();
        con3.effective_from_height = 82_000;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(60_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::from_u64(60_000_000_000).expect("valid difficulty"),
            target_time: 60,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(60_000).expect("valid difficulty"),
            max_difficulty: Difficulty::from_u64(60_000_000).expect("valid difficulty"),
            target_time: 60,
        });
        algos.insert(PowAlgorithm::RandomXT, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(600).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 60,
        });
        algos.insert(PowAlgorithm::Cuckaroo, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 60,
        });
        con3.blockchain_version = BlockVersion::V2;
        con3.valid_blockchain_version_range = 2..=2;
        con3.proof_of_work = algos;
        let mut con4 = con3.clone();
        con4.include_c29_accumulated_difficulty_into_total = true;
        con4.effective_from_height = 181_000;
        let consensus_constants = vec![consensus_constants1, con2, con3, con4];
        Self::with_tip004_activation(consensus_constants, ESMERALDA_TIP004_ACTIVATION_HEIGHT)
    }

    /// *
    /// Stagenet has the following characteristics:
    /// * 2 min blocks on average (5 min SHA-3, 3 min MM)
    /// * 21 billion tXTM with a 3-year half-life
    /// * 800 T tail emission (± 1% inflation after initial 21 billion has been mined)
    /// * Coinbase lock height - 12 hours = 360 blocks
    pub fn stagenet() -> Vec<Self> {
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(450_000_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let consensus_constants = vec![ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 360,
            blockchain_version: BlockVersion::V0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window: 90,
            max_block_transaction_weight: 127_795,
            max_block_coinbase_count: 1000,
            median_timestamp_count: 11,
            emission_initial: INITIAL_EMISSION,
            emission_decay: EMISSION_DECAY.to_vec(),
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
            vn_epoch_length: 10,
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
            coinbase_output_features_extra_max_length: 256,
            vn_registration_max_vns_initial_epoch: 0,
            vn_registration_max_vns_per_epoch: 0,
            vn_registration_max_exits_per_epoch: 0,
            cuckaroo_cycle_length: 42,
            cuckaroo_edge_bits: 29,
            include_c29_accumulated_difficulty_into_total: false,
            pow_backoff_cap: POW_BACKOFF_DISABLED,
        }];
        Self::with_tip004_activation(consensus_constants, STAGENET_TIP004_ACTIVATION_HEIGHT)
    }

    #[allow(clippy::too_many_lines)]
    pub fn nextnet() -> Vec<Self> {
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(150_000_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let con_1 = ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 360,
            blockchain_version: BlockVersion::V0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window: 90,
            max_block_transaction_weight: 127_795,
            max_block_coinbase_count: 1000,
            median_timestamp_count: 11,
            emission_initial: INITIAL_EMISSION,
            emission_decay: EMISSION_DECAY.to_vec(),
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
            vn_epoch_length: 10,
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
            coinbase_output_features_extra_max_length: 256,
            vn_registration_max_vns_initial_epoch: 0,
            vn_registration_max_vns_per_epoch: 0,
            vn_registration_max_exits_per_epoch: 0,
            cuckaroo_cycle_length: 42,
            cuckaroo_edge_bits: 29,
            include_c29_accumulated_difficulty_into_total: false,
            pow_backoff_cap: POW_BACKOFF_DISABLED,
        };
        let mut con_2 = con_1.clone();
        con_2.coinbase_min_maturity = 120;
        con_2.effective_from_height = 30 * 24 * 2; // 2 days
        let mut con_3 = con_2.clone();
        con_3.effective_from_height = 1500;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(150_000_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::RandomXT, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        con_3.blockchain_version = BlockVersion::V1;
        con_3.valid_blockchain_version_range = 1..=1;
        con_3.proof_of_work = algos;

        let mut con_4 = con_3.clone();
        con_4.effective_from_height = 5_500;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(150_000_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::RandomXT, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::Cuckaroo, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        con_4.blockchain_version = BlockVersion::V2;
        con_4.proof_of_work = algos;

        let mut con_5 = con_4.clone();
        con_5.include_c29_accumulated_difficulty_into_total = true;
        // NOTE: this entry was originally declared with `effective_from_height = 5000`, which left the constants
        // vector unsorted. `ConsensusManager::consensus_constants` walks the vector in order and stops at the first
        // entry whose height exceeds the one being looked up, so a height in `5000..5500` stopped at `con_4` and
        // never reached this entry, while any height at or above 5500 walked past `con_4` and landed here. 5500 is
        // therefore the height from which this entry has always actually been in force; stating it explicitly keeps
        // every lookup answering exactly as before while making the vector non-decreasing, which the activation
        // helper below and `consensus_constants` both rely on.
        con_5.effective_from_height = 5_500;

        let consensus_constants = vec![con_1, con_2, con_3, con_4, con_5];
        Self::with_tip004_activation(consensus_constants, NEXTNET_TIP004_ACTIVATION_HEIGHT)
    }

    #[allow(clippy::too_many_lines)]
    pub fn mainnet() -> Vec<Self> {
        let difficulty_block_window = 90;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(4_500_000_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(12_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 240,
        });
        let (input_version_range, output_version_range, kernel_version_range) = version_zero();
        let con_1 = ConsensusConstants {
            effective_from_height: 0,
            coinbase_min_maturity: 720,
            blockchain_version: BlockVersion::V0,
            valid_blockchain_version_range: 0..=0,
            future_time_limit: 540,
            difficulty_block_window,
            max_block_transaction_weight: ConsensusConstants::MAINNET_MAX_WEIGHT_V1,
            max_block_coinbase_count: 1000,
            median_timestamp_count: 11,
            emission_initial: INITIAL_EMISSION,
            emission_decay: EMISSION_DECAY.to_vec(),
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
            vn_registration_min_deposit_amount: MicroMinotari(0),
            vn_registration_lock_height: 0,
            vn_registration_shuffle_interval: VnEpoch(100),
            coinbase_output_features_extra_max_length: 256,
            vn_registration_max_vns_initial_epoch: 0,
            vn_registration_max_vns_per_epoch: 0,
            vn_registration_max_exits_per_epoch: 0,
            cuckaroo_cycle_length: 42,
            cuckaroo_edge_bits: 29,
            include_c29_accumulated_difficulty_into_total: false,
            pow_backoff_cap: POW_BACKOFF_DISABLED,
        };
        let mut con_2 = con_1.clone();
        con_2.coinbase_min_maturity = 540; // 18 hours
        con_2.effective_from_height = 30 * 24 * 7; // 1 week
        let mut con_3 = con_2.clone();
        con_3.coinbase_min_maturity = 360;
        con_3.effective_from_height = 30 * 24 * 7 * 2; // 2 weeks

        let mut con_4 = con_3.clone();
        con_4.effective_from_height = 15_000;
        con_4.coinbase_min_maturity = 180; // 6 hours
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(150_000_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        algos.insert(PowAlgorithm::RandomXT, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 360,
        });
        con_4.blockchain_version = BlockVersion::V1;
        con_4.valid_blockchain_version_range = 1..=1;
        con_4.proof_of_work = algos;

        let mut con_5 = con_4.clone();
        con_5.effective_from_height = 95_000;
        con_5.blockchain_version = BlockVersion::V2;
        con_5.valid_blockchain_version_range = 2..=2;
        let mut algos = HashMap::new();
        algos.insert(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(150_000_000_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 480,
        });
        algos.insert(PowAlgorithm::RandomXM, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 480,
        });
        algos.insert(PowAlgorithm::RandomXT, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1_200_000).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 480,
        });
        algos.insert(PowAlgorithm::Cuckaroo, PowAlgorithmConstants {
            min_difficulty: Difficulty::from_u64(1).expect("valid difficulty"),
            max_difficulty: Difficulty::max(),
            target_time: 480,
        });
        con_5.proof_of_work = algos;

        let mut con_6 = con_5.clone();
        con_6.include_c29_accumulated_difficulty_into_total = true;
        con_6.effective_from_height = 126_000;

        let consensus_constants = vec![con_1, con_2, con_3, con_4, con_5, con_6];
        Self::with_tip004_activation(consensus_constants, MAINNET_TIP004_ACTIVATION_HEIGHT)
    }

    /// Appends the TIP-RFC-MT-0004 activation entry (exponential same-algorithm PoW backoff plus the shortened LWMA
    /// window; both changes are gated on the same fork height) to a network's constants.
    ///
    /// The new entry is a clone of the network's latest entry, so every other consensus value carries over unchanged.
    /// Networks with live history must activate this way rather than at height 0: retroactively changing
    /// `difficulty_block_window` or `pow_backoff_cap` would make historical blocks recompute to a different target
    /// than the one recorded in their accumulated data.
    fn with_tip004_activation(mut constants: Vec<Self>, activation_height: u64) -> Vec<Self> {
        // The base must be the entry `ConsensusManager::consensus_constants` would return for a height just below
        // the activation, which is the *last* element, not the one with the greatest `effective_from_height`. That
        // lookup walks the vector in order without breaking early, so a later entry always wins over an earlier one
        // with a higher height. Picking `max_by_key` instead silently dropped NextNet's
        // `include_c29_accumulated_difficulty_into_total`, bundling an unrelated consensus change into this fork.
        // `assert_activation_entry_matches_runtime_lookup` pins this for every network.
        let latest = constants.last().expect("consensus constants are never empty").clone();
        // A never-activating placeholder height is deliberately still added, so that the entry is exercised by tests
        // and so that setting a real height is a one line change.
        let mut activated = latest;
        activated.effective_from_height = activation_height;
        activated.pow_backoff_cap = POW_BACKOFF_CAP;
        activated.difficulty_block_window = TIP004_DIFFICULTY_BLOCK_WINDOW;
        constants.push(activated);
        constants
    }

    fn current_permitted_output_types() -> Vec<OutputType> {
        vec![OutputType::Coinbase, OutputType::Standard, OutputType::Burn]
    }

    fn current_permitted_range_proof_types() -> Vec<(OutputType, Vec<RangeProofType>)> {
        vec![
            (OutputType::Standard, vec![RangeProofType::BulletProofPlus]),
            (OutputType::Coinbase, vec![
                RangeProofType::BulletProofPlus,
                RangeProofType::RevealedValue,
            ]),
            (OutputType::Burn, vec![RangeProofType::BulletProofPlus]),
            (OutputType::ValidatorNodeRegistration, vec![
                RangeProofType::BulletProofPlus,
            ]),
            (OutputType::CodeTemplateRegistration, vec![
                RangeProofType::BulletProofPlus,
            ]),
            (OutputType::SidechainCheckpoint, vec![RangeProofType::BulletProofPlus]),
            (OutputType::SidechainProof, vec![RangeProofType::BulletProofPlus]),
            (OutputType::ValidatorNodeExit, vec![RangeProofType::BulletProofPlus]),
        ]
    }

    fn all_range_proof_types() -> Vec<(OutputType, Vec<RangeProofType>)> {
        vec![
            (OutputType::Standard, RangeProofType::all()),
            (OutputType::Coinbase, RangeProofType::all()),
            (OutputType::Burn, RangeProofType::all()),
            (OutputType::ValidatorNodeRegistration, RangeProofType::all()),
            (OutputType::CodeTemplateRegistration, RangeProofType::all()),
            (OutputType::SidechainCheckpoint, RangeProofType::all()),
            (OutputType::SidechainProof, RangeProofType::all()),
            (OutputType::ValidatorNodeExit, RangeProofType::all()),
        ]
    }
}

const EMISSION_DECAY: [u64; 6] = [21u64, 22, 23, 25, 26, 37];

/// Class to create custom consensus constants
pub struct ConsensusConstantsBuilder {
    consensus: ConsensusConstants,
}

impl ConsensusConstantsBuilder {
    /// Starts from the constants that are actually live on `network` today.
    ///
    /// Entries gated on an unscheduled activation height (a `u64::MAX` placeholder, such as TIP-RFC-MT-0004 until a
    /// real height is chosen) are skipped, so that fixtures keep exercising the rules the network is really running.
    /// Once a placeholder is replaced by a real height the corresponding entry is picked up automatically.
    ///
    /// The chosen entry is normalised to `effective_from_height = 0`, because the result is normally used as a
    /// single entry constants vector. Lookups that filter on the effective height rather than falling back to the
    /// first entry - `get_maturity_tranches`, and therefore `total_tokens_spendable_at_height` and the chain balance
    /// validator - find nothing at all in a one element vector whose only entry is effective at `u64::MAX`.
    pub fn new(network: Network) -> Self {
        let all = NetworkConsensus::from(network).create_consensus_constants();
        let mut consensus = all
            .iter()
            .rev()
            .find(|c| c.effective_from_height != UNSCHEDULED_ACTIVATION_HEIGHT)
            .or_else(|| all.last())
            .expect("Empty consensus constants")
            .clone();
        consensus.effective_from_height = 0;
        Self { consensus }
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
        decay: Vec<u64>,
        inflation_bips: u64,
        epoch_length: u64,
    ) -> Self {
        self.consensus.emission_initial = intial_amount;
        self.consensus.emission_decay = decay;
        self.consensus.inflation_bips = inflation_bips;
        self.consensus.tail_epoch_length = epoch_length;
        self
    }

    pub fn with_permitted_output_types(mut self, permitted_output_types: Vec<OutputType>) -> Self {
        self.consensus.permitted_output_types = permitted_output_types;
        self
    }

    pub fn with_permitted_range_proof_types(
        mut self,
        permitted_range_proof_types: Vec<(OutputType, Vec<RangeProofType>)>,
    ) -> Self {
        self.consensus.permitted_range_proof_types = permitted_range_proof_types;
        self
    }

    pub fn with_blockchain_version(mut self, version: BlockVersion) -> Self {
        self.consensus.blockchain_version = version;
        self
    }

    pub fn with_valid_blockchain_version_range(mut self, range: RangeInclusive<u16>) -> Self {
        self.consensus.valid_blockchain_version_range = range;
        self
    }

    /// Sets the cap on the exponential same-algorithm PoW backoff modifier (TIP-RFC-MT-0004). Pass
    /// [`POW_BACKOFF_DISABLED`] to switch the backoff off.
    ///
    /// # Panics
    ///
    /// Panics if `cap` is not a power of two in `1..=POW_BACKOFF_CAP`. A cap outside that range would make the
    /// penalty and the LWMA's de-normalisation disagree.
    pub fn with_pow_backoff_cap(mut self, cap: u64) -> Self {
        assert!(
            cap > 0 && cap.is_power_of_two() && cap <= POW_BACKOFF_CAP,
            "pow_backoff_cap must be a power of two in 1..={POW_BACKOFF_CAP}, but {cap} was given"
        );
        self.consensus.pow_backoff_cap = cap;
        self
    }

    /// Sets the LWMA difficulty block window.
    pub fn with_difficulty_block_window(mut self, block_window: u64) -> Self {
        self.consensus.difficulty_block_window = block_window;
        self
    }

    /// Sets the height from which these constants become effective.
    pub fn with_effective_from_height(mut self, height: u64) -> Self {
        self.consensus.effective_from_height = height;
        self
    }

    pub fn build(self) -> ConsensusConstants {
        self.consensus
    }
}

#[cfg(test)]
mod activation_test {
    use tari_common::configuration::Network;

    use super::*;

    const ALL_NETWORKS: [Network; 6] = [
        Network::LocalNet,
        Network::Igor,
        Network::Esmeralda,
        Network::NextNet,
        Network::StageNet,
        Network::MainNet,
    ];

    fn activation_height(network: Network) -> u64 {
        match network {
            Network::LocalNet => 0,
            Network::Igor => IGOR_TIP004_ACTIVATION_HEIGHT,
            Network::Esmeralda => ESMERALDA_TIP004_ACTIVATION_HEIGHT,
            Network::NextNet => NEXTNET_TIP004_ACTIVATION_HEIGHT,
            Network::StageNet => STAGENET_TIP004_ACTIVATION_HEIGHT,
            Network::MainNet => MAINNET_TIP004_ACTIVATION_HEIGHT,
        }
    }

    /// The authoritative lookup, shared with `ConsensusManager::consensus_constants` and the node's consensus
    /// constants tracker. Calling the real function rather than copying it means these tests also guard changes to
    /// it.
    fn runtime_lookup(constants: &[ConsensusConstants], height: u64) -> &ConsensusConstants {
        ConsensusConstants::active_at_height(constants, height).expect("never empty")
    }

    /// The in-order walk only agrees with "the entry with the greatest effective height" while the vector is sorted.
    /// NextNet was not, which is how the TIP-RFC-MT-0004 activation entry came to be cloned from an entry the
    /// runtime never returns.
    #[test]
    fn constants_vectors_are_sorted_by_effective_height() {
        for network in ALL_NETWORKS {
            let constants = ConsensusConstants::for_network(network);
            for pair in constants.windows(2) {
                let (a, b) = (pair.first().expect("windows(2)"), pair.get(1).expect("windows(2)"));
                assert!(
                    a.effective_from_height <= b.effective_from_height,
                    "{network} consensus constants are out of order: {} then {}",
                    a.effective_from_height,
                    b.effective_from_height
                );
            }
        }
    }

    /// `for_network_at_height` picks the greatest effective height while `ConsensusManager` walks in order. Those
    /// two can only agree while the vector is sorted, and a disagreement means two nodes applying different rules
    /// depending on which lookup they happened to use.
    #[test]
    fn the_two_constants_lookups_agree_at_every_boundary() {
        for network in ALL_NETWORKS {
            let constants = ConsensusConstants::for_network(network);
            let mut heights = vec![0u64, 1, u64::MAX - 1, u64::MAX];
            for c in &constants {
                heights.push(c.effective_from_height.saturating_sub(1));
                heights.push(c.effective_from_height);
                heights.push(c.effective_from_height.saturating_add(1));
            }
            for height in heights {
                assert_eq!(
                    ConsensusConstants::for_network_at_height(network, height),
                    *runtime_lookup(&constants, height),
                    "{network} lookups disagree at height {height}"
                );
            }
        }
    }

    /// The activation entry must differ from the rules live just below it in exactly three fields. Anything else
    /// would mean an unrelated consensus change riding along inside the TIP-RFC-MT-0004 fork.
    #[test]
    fn tip004_activation_entry_only_changes_the_backoff_and_the_window() {
        for network in ALL_NETWORKS.into_iter().filter(|n| *n != Network::LocalNet) {
            let constants = ConsensusConstants::for_network(network);
            let activation = activation_height(network);
            let live = runtime_lookup(&constants, activation.saturating_sub(1)).clone();

            let mut expected = live.clone();
            expected.effective_from_height = activation;
            expected.pow_backoff_cap = POW_BACKOFF_CAP;
            expected.difficulty_block_window = TIP004_DIFFICULTY_BLOCK_WINDOW;

            let actual = constants.last().expect("never empty");
            assert_eq!(
                *actual, expected,
                "{network} activation entry drifted from the live rules"
            );
            // Spelled out because this is the field that was silently reverted on NextNet
            assert_eq!(
                actual.include_c29_accumulated_difficulty_into_total,
                live.include_c29_accumulated_difficulty_into_total,
                "{network} activation entry changes include_c29_accumulated_difficulty_into_total"
            );
        }
    }

    #[test]
    fn nextnet_c29_accumulation_survives_the_fork() {
        // Regression test for the specific bug: NextNet's vector was unsorted, so the activation entry was cloned
        // from `con_4` (c29 excluded) rather than `con_5` (c29 included), which would have switched Cuckaroo out of
        // the accumulated difficulty at the fork height.
        let constants = ConsensusConstants::for_network(Network::NextNet);
        assert!(
            ConsensusConstants::for_network_at_height(Network::NextNet, 5_500)
                .include_c29_accumulated_difficulty_into_total()
        );
        assert!(
            constants
                .last()
                .expect("never empty")
                .include_c29_accumulated_difficulty_into_total()
        );
    }

    #[test]
    fn the_builder_hands_out_the_rules_that_are_live_today() {
        for network in ALL_NETWORKS {
            let built = ConsensusConstantsBuilder::new(network).build();
            let constants = ConsensusConstants::for_network(network);
            let activation = activation_height(network);
            let mut live = runtime_lookup(&constants, activation.saturating_sub(1)).clone();
            // The builder's output is used as a single entry vector, so it is normalised to height 0
            live.effective_from_height = 0;
            assert_eq!(built, live, "{network} builder drifted from the live rules");
            // A single entry vector with a non-zero effective height breaks lookups that filter on it, such as the
            // coinbase maturity tranches.
            assert_eq!(built.effective_from_height(), 0, "{network}");
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use std::convert::TryFrom;

    use crate::{
        consensus::{
            ConsensusConstants,
            emission::{Emission, EmissionSchedule},
        },
        tari_amount::{MicroMinotari, uT},
        transaction_components::{OutputType, RangeProofType},
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
            esmeralda[0].emission_decay.clone(),
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
            nextnet[0].emission_decay.clone(),
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
            stagenet[0].emission_decay.clone(),
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
            igor[0].emission_decay.clone(),
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

    #[test]
    fn range_proof_types_coverage() {
        let output_type_variants = OutputType::all();
        let range_proof_type_variants = RangeProofType::all();

        let permitted_range_proof_types = ConsensusConstants::current_permitted_range_proof_types().to_vec();
        for item in &output_type_variants {
            let entries = permitted_range_proof_types
                .iter()
                .filter(|&x| x.0 == *item)
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            assert!(!entries[0].1.is_empty());
        }

        let permitted_range_proof_types = ConsensusConstants::all_range_proof_types().to_vec();
        for output_type in output_type_variants {
            let entries = permitted_range_proof_types
                .iter()
                .filter(|&x| x.0 == output_type)
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            for range_proof_type in &range_proof_type_variants {
                assert!(entries[0].1.contains(range_proof_type));
            }
        }
    }
}
