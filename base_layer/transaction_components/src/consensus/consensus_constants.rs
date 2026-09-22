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
///
/// `struct_excessive_bools` is allowed rather than fixed: each bool is an independent consensus rule gated at its own
/// activation height, and they are read one at a time by unrelated validators. Folding them into a sub-struct or a
/// bitflag would change the serialised shape of `consensus_constants.json`, which
/// `the_previous_releases_json_shape_still_deserializes_with_pre_fork_defaults` deliberately pins so an upgrading
/// node can still read the file the previous release wrote.
#[allow(clippy::struct_excessive_bools)]
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
    /// Verify Cuckaroo cycles as a bipartite graph, keeping the U and V endpoint namespaces distinct
    /// (GHSA-3qmx-q9pv-f3m4). False selects the pre-fork merged-namespace verifier.
    ///
    /// `serde(default)` is load-bearing, not tidiness. `ConsensusConstantsTracker` persists this struct as
    /// `consensus_constants.json` in the node's data directory, and every upgrading node has a file that was
    /// written before this field existed. Without a default, `serde_json::from_str` fails on it,
    /// `ConsensusConstantsTracker::load_previous` degrades the parse error to a `warn!` and returns `None`, and
    /// `check_for_changes` skips its entire body when `previous` is `None` - so the consensus change alarm would
    /// silently not fire on exactly the upgrade it exists for. `false` is the right default: it is what a file
    /// written by a pre-fork binary meant.
    #[serde(default)]
    bipartite_cuckaroo_verification: bool,
    /// Bind the aux-chain merkle proof's branch length to the depth implied by the merge mining tag's chain count.
    /// False selects the pre-fork behaviour, where the branch length was unconstrained apart from the generic
    /// 32-element cap.
    ///
    /// `#[serde(default)]` for the same reason as `bipartite_cuckaroo_verification` above: it is new in this
    /// release, so the previous run's JSON does not carry it. `false` is the pre-fork value, so a vector read back
    /// from an older file describes the rules that binary was really running.
    #[serde(default)]
    aux_chain_merkle_proof_depth_binding: bool,
    /// Require a RandomXT header's `pow_data` to be the minimal representative of its zero-extension class: empty,
    /// or ending in a non-zero byte. NOT "empty" - both the empty and the 32 byte zero padded forms are in live use,
    /// and a rule of "must be empty" would orphan the latter. False selects the pre-fork rule, which accepted any
    /// length up to 32 bytes, including the trailing zeros that `create_tari_mining_blob` pads away and that
    /// therefore buy a block a free, equal-work, different-hash variant per zero.
    ///
    /// `#[serde(default)]` for the same reason as the two fields above.
    #[serde(default)]
    require_canonical_randomxt_pow_data: bool,
    /// Accept only the canonical encoding of the merge mining `MerkleTreeParameters`, i.e. reject any varint that
    /// does not survive decode-then-re-encode (GHSA-3qmx-q9pv-f3m4, item 3). False selects the pre-fork decoder,
    /// which accepts non-canonical widths, a discarded nonce bit, unread high bits, and saturates an aux chain
    /// count of 256 onto 255.
    ///
    /// `#[serde(default)]` for the same reason as the three fields above: it is new in this release, so the
    /// previous run's JSON does not carry it, and `false` is what a file written by a pre-fork binary meant.
    #[serde(default)]
    strict_merkle_tree_parameter_decoding: bool,
    /// Derive the Monero coinbase prefix hash from raw coinbase prefix bytes carried in `pow_data`
    /// (GHSA-3qmx-q9pv-f3m4). False selects the pre-fork wire format, in which `pow_data` carried a Keccak sponge
    /// state that the sender chose and the verifier trusted.
    ///
    /// `#[serde(default)]` for the same reason as the fields above: it is new in this release, so the previous
    /// run's JSON does not carry it, and `false` is what a file written by a pre-fork binary meant.
    #[serde(default)]
    derive_monero_coinbase_hasher: bool,
    /// The cap (`M_MAX`) on the exponential same-algorithm proof of work backoff modifier (TIP-RFC-MT-0004).
    /// A value of `1` disables the backoff entirely (pre-fork behaviour), `32` is the RFC cap.
    ///
    /// Defaulted for the same reason as the two flags above, but *not* with a bare `#[serde(default)]`:
    /// `u64::default()` is `0`, which is not a value this field is ever allowed to hold. The pre-fork value is
    /// `POW_BACKOFF_DISABLED`, i.e. `1`. This field is absent from the JSON written by v5.6.0, the last public
    /// release and therefore the binary most nodes will be upgrading from.
    #[serde(default = "pow_backoff_disabled")]
    pow_backoff_cap: u64,
}

/// The pre-TIP-RFC-MT-0004 value of `pow_backoff_cap`, used as its serde default so that a
/// `consensus_constants.json` written before the field existed reads back as the rules that binary was really
/// running rather than as `0`.
fn pow_backoff_disabled() -> u64 {
    POW_BACKOFF_DISABLED
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
pub const ESMERALDA_TIP004_ACTIVATION_HEIGHT: u64 = 860_000;
/// TIP-RFC-MT-0004 activation height for Igor.
pub const IGOR_TIP004_ACTIVATION_HEIGHT: u64 = UNSCHEDULED_ACTIVATION_HEIGHT;

pub const MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT: u64 = 350_000;
pub const STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT: u64 = UNSCHEDULED_ACTIVATION_HEIGHT;
pub const NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT: u64 = UNSCHEDULED_ACTIVATION_HEIGHT;
pub const ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT: u64 = 900_000;
/// so the tip could not be established.
pub const IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT: u64 = UNSCHEDULED_ACTIVATION_HEIGHT;

// Aux-chain merkle proof depth binding activation heights: the height from which the aux-chain merkle proof's
// branch length must match the length implied by the merge mining tag's chain count.
//
// `check_aux_chains` never reconciled the branch length with the chain count: `calculate_root` walked
// `branch.len()` levels, an attacker-chosen number up to 31, while `get_position_from_path` derived its own depth
// from `number_of_chains`. With the single-chain encoding the position check is vacuous and any branch length
// validated, so one Monero proof of work could commit to a large set of distinct Tari headers at the same height,
// breaking the 1:1 binding between a Monero solution and a Tari block.
//
// This is a hard fork, gated per network rather than applied from height 0 so that blocks already accepted below
// the activation height stay valid: no reorg, no retroactive invalidation. The heights deliberately *coincide*
// with the GHSA-3qmx-q9pv-f3m4 Cuckaroo heights - both fixes ship in the same mandatory upgrade, so there is one
// flag day per network rather than two.
//
// They are deliberately defined as aliases of the Cuckaroo heights rather than as separate literals. The two rules
// ship in one mandatory upgrade and are meant to share a flag day, so rescheduling the Cuckaroo fork - the
// documented pre-tagging action - must move this fork with it. Writing the heights out twice would let the two
// drift apart silently, which is the failure mode that actually matters here: a node applying one rule but not
// the other at a given height is a consensus split. If the two ever need to diverge, break the alias explicitly
// and give this fork its own literal at that point.
/// Aux-chain merkle proof depth binding activation height for MainNet.
pub const MAINNET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT: u64 = MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Aux-chain merkle proof depth binding activation height for StageNet.
pub const STAGENET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT: u64 = STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Aux-chain merkle proof depth binding activation height for NextNet.
pub const NEXTNET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT: u64 = NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Aux-chain merkle proof depth binding activation height for Esmeralda.
pub const ESMERALDA_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT: u64 = ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Aux-chain merkle proof depth binding activation height for Igor.
pub const IGOR_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT: u64 = IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT;

// Canonical RandomXT `pow_data` activation heights. GHSA-3qmx-q9pv-f3m4.
//
// `create_tari_mining_blob` zero pads `pow_data` out to 33 bytes (1 algo byte + 32 data bytes), so two `pow_data`
// values hash to the same RandomX input - and therefore have the same achieved difficulty - exactly when one is the
// other extended by zero bytes. They still produce different block hashes, because `BlockHeader::hash` covers `pow`.
// A `pow_data` that ends in `k` zero bytes consequently has `k` hash-distinct, equal-work variants that a third
// party can mint in flight, each one fresh to the bad-block and reconcile-dedup caches because those key on the full
// header hash.
//
// The canonical form is the minimal representative of each zero-extension class: `pow_data` must be empty, or end in
// a non-zero byte. That is injective, so the class collapses to one accepted value.
//
// Choosing the minimal representative rather than "empty" or "exactly 32 bytes" is load-bearing, because both of
// those forms are in live use. Over the 500 MainNet blocks up to height 347,715 (tip 347,716), of the 134 RandomXT
// blocks:
//
//   74 (55%) carry a 32 byte `pow_data` - 14 meaningful bytes zero padded out to 32, e.g.
//            `00000000000035eb17fbb03b0dfb` followed by 18 zero bytes.
//   60 (45%) carry an empty `pow_data`.
//
// A rule of "must be empty" would orphan the first group and "must be exactly 32 bytes" the second. The minimal
// representative accepts the second group unchanged, and needs the first only to stop zero padding: same 14 bytes,
// same RandomX blob input, same achieved difficulty, no re-mining. Nothing reads these bytes - the RandomXT VM key
// comes from `tari_rx_vm_key_height` - so truncating the padding loses nothing.
//
// The all-time figures differ sharply from the figures at the tip and should not be used to pick the rule: a
// read-only scan of a synced node's `headers` database (`cargo run --release --features rxt_pow_data_audit --example
// audit_rxt_pow_data`) found 83,109 of 85,639 MainNet RandomXT blocks (97%) carrying a 32 byte `pow_data`, from the
// first RandomXT block at height 15,000 onwards, and 52 of 492,547 on Esmeralda between heights 164,916 and 653,327.
// That 97% is dominated by an earlier era; near the tip the split is the 55/45 above.
//
// Aliased to the Cuckaroo heights for exactly the reason given above for the aux-chain binding: all three rules ship
// in one mandatory upgrade and share a flag day, and writing the heights out separately would let them drift into a
// state where a node applies one rule but not another at a given height, which is a consensus split. The intended
// MainNet and Esmeralda values - 350,000 and 900,000 - are what the alias resolves to today.
/// Canonical RandomXT `pow_data` activation height for MainNet.
pub const MAINNET_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT: u64 = MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Canonical RandomXT `pow_data` activation height for StageNet.
pub const STAGENET_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT: u64 = STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Canonical RandomXT `pow_data` activation height for NextNet.
pub const NEXTNET_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT: u64 = NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Canonical RandomXT `pow_data` activation height for Esmeralda.
pub const ESMERALDA_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT: u64 = ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Canonical RandomXT `pow_data` activation height for Igor.
pub const IGOR_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT: u64 = IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT;

// Strict `MerkleTreeParameters` decoding activation heights. GHSA-3qmx-q9pv-f3m4, item 3: the height from which
// `MerkleTreeParameters::from_varint` accepts only the canonical encoding of the parameters a varint decodes to.
//
// The pre-fork decoder was very far from injective. Widening the aux chain count arithmetic - the original finding -
// closes only one collision pair; three families remain, because the size field can name any width wide enough to
// hold the count, because `get_aux_nonce` reads 33 bits and folds them into a `u32` so one bit is discarded, and
// because bits 44..=63 are never read at all. Every one of those is free block hash malleability: the same Monero
// proof of work, the same aux chain parameters, a different Tari block hash, fresh to every cache that keys on the
// header hash. The strict decoder therefore re-encodes what it decoded and rejects anything that does not round
// trip, which closes all of them with one rule. `to_varint` is unchanged and is the definition of the canonical
// form.
//
// !!! OUTSTANDING PRE-SHIP VERIFICATION !!!
// A MainNet scan must confirm that no historical merge mining varint is non-canonical *at all* - not merely that
// none carries the ambiguous 256-chain raw encoding. The canonicality check has a far larger rejection surface than
// the count check did, so the risk of orphaning a real block is real rather than theoretical, and this scan is what
// decides whether the scheduled height is safe. The scan must be exhaustive over *recent* blocks in particular,
// not a sample spread across the chain: what it is really proving is that every currently deployed producer of the
// Tari merge mining tag emits Tari's exact `to_varint` output, and a producer that went live last month is
// precisely the one that would orphan blocks on day one. Do not tag a release carrying this fork until that scan
// has run and come back clean.
//
// Aliased to the Cuckaroo heights for exactly the reason given on the aux-chain binding and the canonical RandomXT
// `pow_data` heights above: all four rules ship in one mandatory upgrade and share a flag day, and writing the
// heights out separately would let them drift into a state where a node applies one rule but not another at a given
// height, which is a consensus split. The intended MainNet and Esmeralda values - 350,000 and 900,000 - are what
// the alias resolves to today. If this fork ever genuinely needs a flag day of its own, break the alias explicitly
// and give it its own literal at that point.
/// Strict `MerkleTreeParameters` decoding activation height for MainNet.
pub const MAINNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT: u64 = MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Strict `MerkleTreeParameters` decoding activation height for StageNet.
pub const STAGENET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT: u64 = STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Strict `MerkleTreeParameters` decoding activation height for NextNet.
pub const NEXTNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT: u64 = NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Strict `MerkleTreeParameters` decoding activation height for Esmeralda.
pub const ESMERALDA_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT: u64 = ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Strict `MerkleTreeParameters` decoding activation height for Igor.
pub const IGOR_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT: u64 = IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT;

// Derived Monero coinbase prefix hash activation heights. GHSA-3qmx-q9pv-f3m4.
//
// `MoneroPowData::coinbase_tx_hasher` was a live `tiny_keccak::Keccak` sponge carried on the wire inside
// `header.pow.pow_data` and rebuilt field by field by borsh. `is_coinbase_valid_merkle_root` cloned it, absorbed
// `coinbase_tx_extra` and finalized, and the resulting prefix hash is what ties a Monero block - and therefore its
// RandomX solution - to one particular Tari header. The sender chose that state and the verifier treated it as
// authoritative, which is forgeable by arithmetic rather than by search: with `offset = 0` an attacker solves
// `buffer = target_state XOR extra XOR padding` and reproduces any real Monero block's coinbase prefix hash under an
// extra field carrying their own merge mining tag, inheriting that block's difficulty having done no work.
//
// From this height `pow_data` carries the raw coinbase transaction prefix bytes instead, and the verifier builds its
// own `Keccak::v256()` and absorbs `prefix || extra` itself. No sponge state is on the wire at all.
//
// This is RandomXM (`MoneroPowData`), and it is a different rule from the canonical RandomXT `pow_data` rule above:
// that one constrains the bytes of a *Tari* `pow_data`, this one changes the shape of a *Monero* one. They do not
// overlap and neither subsumes the other.
//
// Gated per network rather than applied from height 0, and more strongly than any of the rules above: this is a
// wire-format change, so applying it to history would fail to *parse* every merge mined block since mainnet genesis.
// Everything below the activation height keeps the legacy shape, and the forgery it permits, forever - which is why
// the deep reorg anchor exists as well.
//
// Aliased to the Cuckaroo heights for exactly the reason given for the three rules above: all of these rules ship in
// one mandatory upgrade and share a flag day, and writing the heights out separately would let them drift into a
// state where a node applies one rule but not another at a given height, which is a consensus split. Note this fork
// needs merge mining proxies and miners to ship a wire format change, not just node operators, so recompute the lead
// time against the real tip before tagging.
/// Derived Monero coinbase prefix hash activation height for MainNet.
pub const MAINNET_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT: u64 = MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Derived Monero coinbase prefix hash activation height for StageNet.
pub const STAGENET_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT: u64 = STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Derived Monero coinbase prefix hash activation height for NextNet.
pub const NEXTNET_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT: u64 = NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Derived Monero coinbase prefix hash activation height for Esmeralda.
pub const ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT: u64 = ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT;
/// Derived Monero coinbase prefix hash activation height for Igor.
pub const IGOR_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT: u64 = IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT;

// GHSA-3qmx-q9pv-f3m4: how big a Monero coinbase transaction prefix is allowed to be.
//
// From the activation height above, `MoneroPowData` carries the raw `version | unlock_time | inputs | outputs`
// consensus encoding of the Monero coinbase transaction (everything the Monero `transaction_prefix` contains except
// the extra field, which travels beside it and has its own `max_extra_field_size` bound). A peer chooses those
// bytes, so like the extra field they need a bound, or a single header could carry a megabyte.
//
// The bound is derived from what a Monero coinbase prefix can actually be, not picked round:
//
//   * fixed part, every VarInt taken at its widest 10-byte encoding rather than the 1-4 bytes real values use:
//     `version` (10) + `unlock_time` (10) + the input vector, which for a coinbase is always exactly one `TxIn::Gen`:
//     length (1) + tag (1) + `height` (10), + the output count VarInt (10). That is 42 bytes. The constant below is
//     held at 52 rather than 42: it is an upper bound and nothing but abuse lives in the 10 bytes of slack, so the
//     value is left where the fix shipped it instead of being tightened under a security fork.
//   * per output: `amount` VarInt (10) + target tag (1) + one-time public key (32) + view tag (1) = 44 bytes.
//   * output count: Monero itself enforces a 1,000 output limit on a coinbase transaction, so this bound sits exactly
//     on Monero's own limit and excludes no legitimate merge miner - including P2Pool, which pays every miner in its
//     PPLNS window straight out of the coinbase. A coinbase over it is one Monero would not have accepted in the first
//     place. It still leaves ~20 kB of headroom inside `PowData`'s 65,535-byte ceiling once the two merkle proofs (32
//     hashes each) and the extra field are accounted for.
//
// A real single-output coinbase prefix is under 60 bytes, i.e. the new format is *smaller* on the wire than the
// 203-byte sponge it replaces in the overwhelmingly common case. This number is only here to stop abuse.
const MONERO_COINBASE_PREFIX_FIXED_MAX_SIZE: usize = 52;
const MONERO_COINBASE_OUTPUT_MAX_SIZE: usize = 44;
const MONERO_COINBASE_MAX_OUTPUTS: usize = 1_000;
/// The largest Monero coinbase transaction prefix a `MoneroPowData` may carry (GHSA-3qmx-q9pv-f3m4). See the
/// derivation above.
pub const MAX_MONERO_COINBASE_PREFIX_SIZE: usize =
    MONERO_COINBASE_PREFIX_FIXED_MAX_SIZE + MONERO_COINBASE_MAX_OUTPUTS * MONERO_COINBASE_OUTPUT_MAX_SIZE;

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

    /// True once the Cuckaroo verifier must treat the cycle as a bipartite graph, keeping the U and V endpoint
    /// namespaces distinct (GHSA-3qmx-q9pv-f3m4). False below the fork height selects the merged-namespace verifier
    /// that accepted the historical chain.
    pub fn bipartite_cuckaroo_verification(&self) -> bool {
        self.bipartite_cuckaroo_verification
    }

    /// The first height in `constants` at which the bipartite Cuckaroo verifier is in force, or
    /// [`UNSCHEDULED_ACTIVATION_HEIGHT`] if no entry ever turns it on.
    ///
    /// This is the height of the GHSA-3qmx-q9pv-f3m4 fork, and therefore the height from which stored
    /// `target_difficulty` values may have been computed under rules that no longer apply: the activation entry is
    /// also where a network may change `difficulty_block_window` or `pow_backoff_cap`, and `target_difficulty` is
    /// the only quantity that accumulates, so one stale entry poisons chain strength for every block above it. The
    /// accumulated-data rebuild migration keys on this height.
    ///
    /// Derived from the constants vector rather than declared as a separate per-network constant, for the same
    /// reason as [`ConsensusConstants::derived_monero_coinbase_activation_height`]: whatever height the entry that
    /// carries the rule is scheduled at *is* the activation height, by construction, so it cannot go stale. The
    /// vector is sorted by effective height (`active_at_height` relies on that), so the first match is the earliest.
    pub fn bipartite_cuckaroo_activation_height(constants: &[ConsensusConstants]) -> u64 {
        constants
            .iter()
            .find(|c| c.bipartite_cuckaroo_verification)
            .map_or(UNSCHEDULED_ACTIVATION_HEIGHT, |c| c.effective_from_height)
    }

    /// True once the aux-chain merkle proof's branch length must equal the length an honest proof for the
    /// independently derived leaf position in a tree of `number_of_chains` leaves would have. False below the fork
    /// height selects the pre-fork behaviour, which accepted any branch length up to the generic cap.
    pub fn aux_chain_merkle_proof_depth_binding(&self) -> bool {
        self.aux_chain_merkle_proof_depth_binding
    }

    /// True once a RandomXT header's `pow_data` must be empty or end in a non-zero byte. Below the fork height any
    /// length up to 32 bytes is accepted, which is what keeps the historical chain valid.
    pub fn require_canonical_randomxt_pow_data(&self) -> bool {
        self.require_canonical_randomxt_pow_data
    }

    /// True once `MerkleTreeParameters::from_varint` must accept only the canonical encoding of the parameters a
    /// varint decodes to, which makes the decoding injective (GHSA-3qmx-q9pv-f3m4, item 3). False below the fork
    /// height selects the pre-fork decoder that validated the historical chain, which accepts several distinct
    /// varints per parameter set.
    pub fn strict_merkle_tree_parameter_decoding(&self) -> bool {
        self.strict_merkle_tree_parameter_decoding
    }

    /// True once Monero merge-mining `pow_data` carries the raw coinbase transaction prefix, from which the
    /// verifier derives the coinbase Keccak state itself (GHSA-3qmx-q9pv-f3m4).
    ///
    /// False below the fork height selects the pre-fork wire format, in which `pow_data` carried a serialized
    /// Keccak sponge: 200 bytes of state plus `offset`, `rate` and `mode`, all attacker chosen. Because the
    /// 200 bytes are the whole sponge, an attacker could solve `buffer = target_state XOR extra XOR padding` and
    /// reproduce any real Monero block's coinbase prefix hash under an extra field carrying their own merge mining
    /// tag - one XOR, no search - and so mint Tari blocks at that block's difficulty for free. This is gated
    /// rather than applied from height 0 for the same reason as the Cuckaroo verifier above, and more strongly:
    /// it is a wire-format change, so applying it to history would fail to parse every merge mined block on chain.
    pub fn derive_monero_coinbase_hasher(&self) -> bool {
        self.derive_monero_coinbase_hasher
    }

    /// The first height in `constants` at which the derived Monero coinbase prefix hash is in force, or
    /// [`UNSCHEDULED_ACTIVATION_HEIGHT`] if no entry ever turns it on.
    ///
    /// This is what the deep reorg anchor keys on. Derived from the constants vector rather than declared as a
    /// separate per-network constant, so it cannot go stale: whatever height the entry that carries the rule is
    /// scheduled at *is* the activation height, by construction. The vector is sorted by effective height
    /// (`active_at_height` relies on that), so the first match is the earliest.
    pub fn derived_monero_coinbase_activation_height(constants: &[ConsensusConstants]) -> u64 {
        constants
            .iter()
            .find(|c| c.derive_monero_coinbase_hasher)
            .map_or(UNSCHEDULED_ACTIVATION_HEIGHT, |c| c.effective_from_height)
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
            // LocalNet is ephemeral (no persistent chain to invalidate), so GHSA-3qmx-q9pv-f3m4 applies from
            // height 0 rather than through a gated activation entry.
            bipartite_cuckaroo_verification: true,
            // LocalNet is ephemeral (no persistent chain to invalidate), so the aux-chain depth binding applies
            // from height 0 rather than through a gated activation entry. Same for the canonical RandomXT rule.
            aux_chain_merkle_proof_depth_binding: true,
            require_canonical_randomxt_pow_data: true,
            strict_merkle_tree_parameter_decoding: true,
            // Same reasoning: LocalNet never uses the legacy Monero coinbase wire format at all.
            derive_monero_coinbase_hasher: true,
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
        let con_1 = ConsensusConstants {
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
            bipartite_cuckaroo_verification: false,
            aux_chain_merkle_proof_depth_binding: false,
            require_canonical_randomxt_pow_data: false,
            strict_merkle_tree_parameter_decoding: false,
            derive_monero_coinbase_hasher: false,
            pow_backoff_cap: POW_BACKOFF_DISABLED,
        };

        // All five GHSA-3qmx-q9pv-f3m4 rules share one flag day, so they share one entry: their activation
        // heights are aliases of the Cuckaroo height, and `active_index_at_height` returns the last entry at a
        // height, so separate per-rule entries only ever resolved to one entry carrying all of them anyway.
        let mut con_2 = con_1.clone();
        con_2.effective_from_height = IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT;
        con_2.bipartite_cuckaroo_verification = true;
        con_2.aux_chain_merkle_proof_depth_binding = true;
        con_2.require_canonical_randomxt_pow_data = true;
        con_2.strict_merkle_tree_parameter_decoding = true;
        con_2.derive_monero_coinbase_hasher = true;

        let consensus_constants = vec![con_1, con_2];
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
            bipartite_cuckaroo_verification: false,
            aux_chain_merkle_proof_depth_binding: false,
            require_canonical_randomxt_pow_data: false,
            strict_merkle_tree_parameter_decoding: false,
            derive_monero_coinbase_hasher: false,
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

        let mut con5 = con4.clone();
        con5.effective_from_height = ESMERALDA_TIP004_ACTIVATION_HEIGHT;
        con5.pow_backoff_cap = POW_BACKOFF_CAP;
        con5.difficulty_block_window = TIP004_DIFFICULTY_BLOCK_WINDOW;

        let mut con6 = con5.clone();
        con6.effective_from_height = 869_000;
        con6.pow_backoff_cap = 2;

        let mut con7 = con6.clone();
        con7.effective_from_height = 884_000;
        con7.pow_backoff_cap = 8;

        let mut con8 = con7.clone();
        con8.effective_from_height = 886_000;
        con8.max_randomx_seed_height = 6000;

        // GHSA-3qmx-q9pv-f3m4. Esmeralda has no `with_tip004_activation` call to sit in front of - its TIP-004
        // entry is `con5` and it schedules further changes above it - so these entries are simply appended last.
        let mut con9 = con8.clone();
        con9.effective_from_height = ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT;
        con9.bipartite_cuckaroo_verification = true;
        con9.aux_chain_merkle_proof_depth_binding = true;
        con9.require_canonical_randomxt_pow_data = true;
        con9.strict_merkle_tree_parameter_decoding = true;
        con9.derive_monero_coinbase_hasher = true;

        vec![consensus_constants1, con2, con3, con4, con5, con6, con7, con8, con9]
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
            bipartite_cuckaroo_verification: false,
            aux_chain_merkle_proof_depth_binding: false,
            require_canonical_randomxt_pow_data: false,
            strict_merkle_tree_parameter_decoding: false,
            derive_monero_coinbase_hasher: false,
            pow_backoff_cap: POW_BACKOFF_DISABLED,
        };

        // All five GHSA-3qmx-q9pv-f3m4 rules share one flag day, so they share one entry: their activation
        // heights are aliases of the Cuckaroo height, and `active_index_at_height` returns the last entry at a
        // height, so separate per-rule entries only ever resolved to one entry carrying all of them anyway.
        let mut con_2 = con_1.clone();
        con_2.effective_from_height = STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT;
        con_2.bipartite_cuckaroo_verification = true;
        con_2.aux_chain_merkle_proof_depth_binding = true;
        con_2.require_canonical_randomxt_pow_data = true;
        con_2.strict_merkle_tree_parameter_decoding = true;
        con_2.derive_monero_coinbase_hasher = true;

        let consensus_constants = vec![con_1, con_2];
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
            bipartite_cuckaroo_verification: false,
            aux_chain_merkle_proof_depth_binding: false,
            require_canonical_randomxt_pow_data: false,
            strict_merkle_tree_parameter_decoding: false,
            derive_monero_coinbase_hasher: false,
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

        // GHSA-3qmx-q9pv-f3m4. Appended *before* `with_tip004_activation` pushes its own entry, so that the vector
        // stays sorted by effective height and the TIP-004 entry, which clones the last element, inherits the fix.
        let mut con_6 = con_5.clone();
        con_6.effective_from_height = NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
        con_6.bipartite_cuckaroo_verification = true;
        con_6.aux_chain_merkle_proof_depth_binding = true;
        con_6.require_canonical_randomxt_pow_data = true;
        con_6.strict_merkle_tree_parameter_decoding = true;
        con_6.derive_monero_coinbase_hasher = true;

        let consensus_constants = vec![con_1, con_2, con_3, con_4, con_5, con_6];
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
            bipartite_cuckaroo_verification: false,
            aux_chain_merkle_proof_depth_binding: false,
            require_canonical_randomxt_pow_data: false,
            strict_merkle_tree_parameter_decoding: false,
            derive_monero_coinbase_hasher: false,
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
        con_6.vn_epoch_length = 10;

        // GHSA-3qmx-q9pv-f3m4. All three entries are appended *before* `with_tip004_activation` pushes its own:
        // TIP-004 sits at `u64::MAX` on MainNet, so adding them afterwards would leave the vector unsorted. Because
        // `with_tip004_activation` clones the last element, the TIP-004 entry inherits all three fixes for free.
        let mut con_7 = con_6.clone();
        con_7.effective_from_height = MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT;
        con_7.bipartite_cuckaroo_verification = true;
        con_7.aux_chain_merkle_proof_depth_binding = true;
        con_7.require_canonical_randomxt_pow_data = true;
        con_7.strict_merkle_tree_parameter_decoding = true;
        con_7.derive_monero_coinbase_hasher = true;
        con_7.difficulty_block_window = TIP004_DIFFICULTY_BLOCK_WINDOW;

        let consensus_constants = vec![con_1, con_2, con_3, con_4, con_5, con_6, con_7];
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

    /// Sets the Cuckaroo graph size in bits. The live networks use 29; fixtures use a smaller graph so that a
    /// proof can be searched for in milliseconds instead of hours.
    pub fn with_cuckaroo_edge_bits(mut self, edge_bits: u8) -> Self {
        self.consensus.cuckaroo_edge_bits = edge_bits;
        self
    }

    /// Sets the Cuckaroo cycle length. The live networks use 42; fixtures use a shorter cycle for the same reason
    /// as `with_cuckaroo_edge_bits`.
    pub fn with_cuckaroo_cycle_length(mut self, cycle_length: u8) -> Self {
        self.consensus.cuckaroo_cycle_length = cycle_length;
        self
    }

    /// Selects the Cuckaroo verifier: `false` is the pre-fork merged-namespace verifier, `true` is the bipartite one
    /// (GHSA-3qmx-q9pv-f3m4). Combine with `with_effective_from_height` to build a fixture whose activation height
    /// is known, which is what the fork gate test in `proof_of_work::cuckaroo_pow` does.
    pub fn with_bipartite_cuckaroo_verification(mut self, bipartite: bool) -> Self {
        self.consensus.bipartite_cuckaroo_verification = bipartite;
        self
    }

    /// Turn the derived Monero coinbase prefix hash on or off for this entry (GHSA-3qmx-q9pv-f3m4). `false` is the
    /// pre-fork wire format, in which `pow_data` carried a Keccak sponge state the sender chose.
    ///
    /// Networks get their entries from `ConsensusConstants::for_network`, so this exists for building a constants
    /// *vector* by hand, which is mostly a matter of putting a scheduled activation in front of code under test -
    /// the deep reorg anchor's fixtures do exactly that.
    pub fn with_derive_monero_coinbase_hasher(mut self, derive: bool) -> Self {
        self.consensus.derive_monero_coinbase_hasher = derive;
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

    /// The GHSA-3qmx-q9pv-f3m4 activation height per network. LocalNet is ephemeral and carries the fix on its
    /// single height-0 entry rather than through a gated entry.
    fn c29_bipartite_activation_height(network: Network) -> u64 {
        match network {
            Network::LocalNet => 0,
            Network::Igor => IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT,
            Network::Esmeralda => ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT,
            Network::NextNet => NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT,
            Network::StageNet => STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT,
            Network::MainNet => MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT,
        }
    }

    /// The canonical (empty) RandomXT `pow_data` activation height per network. LocalNet is ephemeral and carries
    /// the rule on its single height-0 entry rather than through a gated entry.
    fn rxt_canonical_pow_data_activation_height(network: Network) -> u64 {
        match network {
            Network::LocalNet => 0,
            Network::Igor => IGOR_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT,
            Network::Esmeralda => ESMERALDA_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT,
            Network::NextNet => NEXTNET_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT,
            Network::StageNet => STAGENET_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT,
            Network::MainNet => MAINNET_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT,
        }
    }

    /// The strict `MerkleTreeParameters` decoding activation height per network (GHSA-3qmx-q9pv-f3m4, item 3).
    /// LocalNet is ephemeral and carries the rule on its single height-0 entry rather than through a gated entry.
    fn strict_merkle_tree_params_activation_height(network: Network) -> u64 {
        match network {
            Network::LocalNet => 0,
            Network::Igor => IGOR_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT,
            Network::Esmeralda => ESMERALDA_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT,
            Network::NextNet => NEXTNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT,
            Network::StageNet => STAGENET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT,
            Network::MainNet => MAINNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT,
        }
    }

    /// The derived Monero coinbase prefix hash activation height per network (GHSA-3qmx-q9pv-f3m4). LocalNet is
    /// ephemeral and carries the rule on its single height-0 entry rather than through a gated entry.
    fn derived_monero_coinbase_activation_height(network: Network) -> u64 {
        match network {
            Network::LocalNet => 0,
            Network::Igor => IGOR_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
            Network::Esmeralda => ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
            Network::NextNet => NEXTNET_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
            Network::StageNet => STAGENET_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
            Network::MainNet => MAINNET_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
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

    /// The index of the last entry effective at `height`. Two forks scheduled at the same height - which is what
    /// `UNSCHEDULED_ACTIVATION_HEIGHT` does to every network without a chosen height - both sit at `u64::MAX`, and
    /// only the *last* of them is the one the runtime ever selects.
    fn last_index_at(constants: &[ConsensusConstants], height: u64) -> usize {
        constants
            .iter()
            .rposition(|c| c.effective_from_height == height)
            .unwrap_or_else(|| panic!("no entry at height {height}"))
    }

    /// The activation entry must differ from the entry immediately below it in the vector in exactly three fields.
    /// Anything else would mean an unrelated consensus change riding along inside the TIP-RFC-MT-0004 fork.
    ///
    /// The comparison is against the vector predecessor rather than `runtime_lookup(activation - 1)` because that
    /// is precisely what `with_tip004_activation` clones. The two are the same entry whenever the fork has a
    /// height of its own, and differ only when another fork is scheduled at the same `u64::MAX` placeholder, where
    /// the predecessor is the right answer: the TIP-004 entry inherits that fork's rules, and both become live at
    /// the same instant.
    ///
    /// The entry is found by its effective height rather than by taking the last one, because a network may
    /// schedule further changes above the fork (Esmeralda tunes the backoff cap again after activation).
    #[test]
    fn tip004_activation_entry_only_changes_the_backoff_and_the_window() {
        for network in ALL_NETWORKS.into_iter().filter(|n| *n != Network::LocalNet) {
            let constants = ConsensusConstants::for_network(network);
            let activation = activation_height(network);
            let index = last_index_at(&constants, activation);
            assert!(
                index > 0,
                "{network} has no entry below its TIP-RFC-MT-0004 activation entry"
            );
            let live = constants
                .get(index.saturating_sub(1))
                .expect("index > 0 was just asserted")
                .clone();

            let mut expected = live.clone();
            expected.effective_from_height = activation;
            expected.pow_backoff_cap = POW_BACKOFF_CAP;
            expected.difficulty_block_window = TIP004_DIFFICULTY_BLOCK_WINDOW;

            let actual = constants.get(index).expect("index came from the vector");
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
            // The same for GHSA-3qmx-q9pv-f3m4: TIP-004 must not switch the Cuckaroo verifier back.
            assert_eq!(
                actual.bipartite_cuckaroo_verification, live.bipartite_cuckaroo_verification,
                "{network} activation entry changes bipartite_cuckaroo_verification"
            );
            // And it must not relax the canonical RandomXT `pow_data` rule back to "any length up to 32".
            assert_eq!(
                actual.require_canonical_randomxt_pow_data, live.require_canonical_randomxt_pow_data,
                "{network} activation entry changes require_canonical_randomxt_pow_data"
            );
            // And for GHSA-3qmx-q9pv-f3m4 item 3: TIP-004 must not switch the canonicality check back off.
            assert_eq!(
                actual.strict_merkle_tree_parameter_decoding, live.strict_merkle_tree_parameter_decoding,
                "{network} activation entry changes strict_merkle_tree_parameter_decoding"
            );
            // Nor may it revert the Monero coinbase wire format, which would make every post-fork merge mined
            // header stop parsing.
            assert_eq!(
                actual.derive_monero_coinbase_hasher, live.derive_monero_coinbase_hasher,
                "{network} activation entry changes derive_monero_coinbase_hasher"
            );
        }
    }

    /// GHSA-3qmx-q9pv-f3m4. A `consensus_constants.json` written before `bipartite_cuckaroo_verification` existed
    /// must still deserialize, and must read as the pre-fork verifier.
    ///
    /// Without `#[serde(default)]` on the field, every upgrading node's stored file fails to parse,
    /// `ConsensusConstantsTracker::load_previous` swallows the error into a `warn!` and returns `None`, and
    /// `check_for_changes` then skips its whole body - so the consensus change alarm would be dead on exactly the
    /// upgrade it exists for.
    #[test]
    fn constants_written_before_the_c29_field_existed_still_deserialize() {
        let live = ConsensusConstants::for_network_at_height(Network::MainNet, 0);
        let json = serde_json::to_string(&live).expect("serializes");
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("round trips");
        // Exactly what an older binary wrote: the field is simply not there.
        assert!(
            value
                .as_object_mut()
                .expect("an object")
                .remove("bipartite_cuckaroo_verification")
                .is_some(),
            "the field name changed, update this test"
        );
        let without_field = serde_json::to_string(&value).expect("serializes");

        let recovered: ConsensusConstants =
            serde_json::from_str(&without_field).expect("a pre-fork constants file must still parse");
        assert!(
            !recovered.bipartite_cuckaroo_verification(),
            "a missing field must read as the pre-fork merged-namespace verifier"
        );
        // And nothing else drifted.
        let mut expected = live;
        expected.bipartite_cuckaroo_verification = false;
        assert_eq!(recovered, expected);
    }

    /// The five GHSA-3qmx-q9pv-f3m4 rules share one flag day and now live on a *single* constants entry per
    /// network, so there is one thing to assert rather than five. This replaces the five separate
    /// `..._activation_entry_only_changes_...` tests: while each rule had its own entry those were five
    /// independent statements, but once the entries were collapsed they all described the same entry and one
    /// change produced five failures for no extra information.
    ///
    /// What still matters, and is what this pins: none of the five rules may be live below the flag day, and the
    /// entry may not change anything outside that rule set - with one deliberate exception. MainNet also shortens
    /// its LWMA window to TIP-004's value on this flag day (Esmeralda already did so at its own TIP-004 height),
    /// so the fork changes block-time behaviour there as well as proof of work validation.
    #[test]
    fn the_advisory_activation_entry_changes_exactly_the_advisory_rules() {
        for network in ALL_NETWORKS.into_iter().filter(|n| *n != Network::LocalNet) {
            let constants = ConsensusConstants::for_network(network);
            let activation = c29_bipartite_activation_height(network);
            let index = constants
                .iter()
                .position(|c| c.effective_from_height == activation && c.bipartite_cuckaroo_verification)
                .unwrap_or_else(|| panic!("{network} has no advisory activation entry at height {activation}"));
            assert!(index > 0, "{network} has no entry below its advisory activation entry");
            let live = constants
                .get(index.saturating_sub(1))
                .expect("index > 0 was just asserted")
                .clone();

            assert!(
                !live.bipartite_cuckaroo_verification,
                "{network}: c29 already live below the flag day"
            );
            assert!(
                !live.aux_chain_merkle_proof_depth_binding,
                "{network}: depth binding already live"
            );
            assert!(
                !live.require_canonical_randomxt_pow_data,
                "{network}: canonical RandomXT already live"
            );
            assert!(
                !live.strict_merkle_tree_parameter_decoding,
                "{network}: strict decoding already live"
            );
            assert!(
                !live.derive_monero_coinbase_hasher,
                "{network}: derived coinbase hasher already live"
            );

            let mut expected = live;
            expected.effective_from_height = activation;
            expected.bipartite_cuckaroo_verification = true;
            expected.aux_chain_merkle_proof_depth_binding = true;
            expected.require_canonical_randomxt_pow_data = true;
            expected.strict_merkle_tree_parameter_decoding = true;
            expected.derive_monero_coinbase_hasher = true;
            if network == Network::MainNet {
                expected.difficulty_block_window = TIP004_DIFFICULTY_BLOCK_WINDOW;
            }

            assert_eq!(
                *constants.get(index).expect("index came from the vector"),
                expected,
                "{network} advisory activation entry changes something outside the advisory rule set"
            );
        }
    }
    /// The rules actually in force either side of the fork, looked up the way the node looks them up.
    #[test]
    fn c29_bipartite_verification_activates_at_the_agreed_heights() {
        // LocalNet is ephemeral, so it has the fix from height 0.
        assert!(
            ConsensusConstants::for_network_at_height(Network::LocalNet, 0).bipartite_cuckaroo_verification(),
            "LocalNet"
        );

        for network in ALL_NETWORKS.into_iter().filter(|n| *n != Network::LocalNet) {
            let activation = c29_bipartite_activation_height(network);
            // Grandfathering: everything below the activation height keeps validating exactly as it does today,
            // which is what keeps the 353 forged mainnet blocks - and the whole chain built on them - valid.
            assert!(
                !ConsensusConstants::for_network_at_height(network, 0).bipartite_cuckaroo_verification(),
                "{network} at height 0"
            );
            assert!(
                !ConsensusConstants::for_network_at_height(network, activation.saturating_sub(1))
                    .bipartite_cuckaroo_verification(),
                "{network} one block below the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, activation).bipartite_cuckaroo_verification(),
                "{network} at the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, u64::MAX).bipartite_cuckaroo_verification(),
                "{network} above the fork"
            );

            // The four assertions above sample the lookup at four heights, and on StageNet, NextNet and Igor the
            // c29 entry and the TIP-004 entry both sit at the `u64::MAX` placeholder, so the last two are answered
            // by the TIP-004 entry rather than by the c29 entry - they pass only because TIP-004's entry was
            // cloned from the c29 entry and inherited the flag. Name the entry directly instead, and pin the
            // property four samples cannot see: an entry inserted after the c29 entry could turn the verifier back
            // off over an entire range of heights and still leave all four samples green.
            let constants = ConsensusConstants::for_network(network);
            let activation_entry = constants
                .iter()
                .position(|c| c.bipartite_cuckaroo_verification)
                .unwrap_or_else(|| panic!("{network} never turns the bipartite verifier on"));
            assert_eq!(
                constants
                    .get(activation_entry)
                    .expect("index came from the vector")
                    .effective_from_height,
                activation,
                "{network}: the entry that turns the bipartite verifier on is not at the agreed activation height"
            );
            assert!(
                constants
                    .get(activation_entry..)
                    .expect("index came from the vector")
                    .iter()
                    .all(|c| c.bipartite_cuckaroo_verification),
                "{network}: an entry after the c29 activation entry turns the bipartite verifier back off again"
            );
        }
    }

    /// The aux-chain merkle proof depth binding activation height per network. LocalNet is ephemeral and carries
    /// the fix on its single height-0 entry rather than through a gated entry.
    fn aux_chain_depth_binding_activation_height(network: Network) -> u64 {
        match network {
            Network::LocalNet => 0,
            Network::Igor => IGOR_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
            Network::Esmeralda => ESMERALDA_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
            Network::NextNet => NEXTNET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
            Network::StageNet => STAGENET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
            Network::MainNet => MAINNET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
        }
    }
    /// The rules actually in force either side of the fork, looked up the way the node looks them up.
    #[test]
    fn aux_chain_depth_binding_activates_at_the_agreed_heights() {
        // LocalNet is ephemeral, so it has the fix from height 0.
        assert!(
            ConsensusConstants::for_network_at_height(Network::LocalNet, 0).aux_chain_merkle_proof_depth_binding(),
            "LocalNet"
        );

        for network in ALL_NETWORKS.into_iter().filter(|n| *n != Network::LocalNet) {
            let activation = aux_chain_depth_binding_activation_height(network);
            // Grandfathering: everything below the activation height keeps validating exactly as it does today.
            assert!(
                !ConsensusConstants::for_network_at_height(network, 0).aux_chain_merkle_proof_depth_binding(),
                "{network} at height 0"
            );
            assert!(
                !ConsensusConstants::for_network_at_height(network, activation.saturating_sub(1))
                    .aux_chain_merkle_proof_depth_binding(),
                "{network} one block below the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, activation).aux_chain_merkle_proof_depth_binding(),
                "{network} at the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, u64::MAX).aux_chain_merkle_proof_depth_binding(),
                "{network} above the fork"
            );
        }
    }

    /// The `consensus_constants.json` on disk was written by the binary this node is being upgraded *from*, so it
    /// is missing every field added since. `ConsensusConstantsTracker::load_previous` deserializes that file and
    /// *swallows* a parse error, so without a serde default the first post-upgrade start would silently skip the
    /// CRITICAL "consensus constants changed and are already active" alarm - the one start where it matters most.
    ///
    /// Building the old shape by deleting the new keys from the current serialization keeps this honest as further
    /// fields are added: it tests the actual wire shape, not a hand-copied literal that drifts.
    #[test]
    fn the_previous_releases_json_shape_still_deserializes_with_pre_fork_defaults() {
        // v5.6.0 is the last public release, and therefore the binary most nodes upgrade from; `pow_backoff_cap`
        // arrived after it, on the 5.7.0 pre-release line.
        const FIELDS_ABSENT_FROM_THE_LAST_RELEASE: [&str; 4] = [
            "bipartite_cuckaroo_verification",
            "aux_chain_merkle_proof_depth_binding",
            "derive_monero_coinbase_hasher",
            "pow_backoff_cap",
        ];

        for network in ALL_NETWORKS {
            let current = ConsensusConstants::for_network(network);
            let mut json: serde_json::Value = serde_json::to_value(&current).expect("serializes");
            for entry in json.as_array_mut().expect("a vector of constants") {
                let map = entry.as_object_mut().expect("constants are a JSON object");
                for field in FIELDS_ABSENT_FROM_THE_LAST_RELEASE {
                    assert!(
                        map.remove(field).is_some(),
                        "{network}: {field} is not in the serialized shape; update this test"
                    );
                }
            }

            let previous: Vec<ConsensusConstants> =
                serde_json::from_value(json).unwrap_or_else(|e| panic!("{network}: previous release shape: {e}"));

            assert_eq!(previous.len(), current.len(), "{network}");
            for (index, constants) in previous.iter().enumerate() {
                assert!(
                    !constants.bipartite_cuckaroo_verification(),
                    "{network} entry {index}: must default to the pre-fork verifier"
                );
                assert!(
                    !constants.aux_chain_merkle_proof_depth_binding(),
                    "{network} entry {index}: must default to the pre-fork binding"
                );
                assert!(
                    !constants.derive_monero_coinbase_hasher(),
                    "{network} entry {index}: must default to the pre-fork Monero coinbase wire format"
                );
                // A bare `#[serde(default)]` would give 0 here, which this field may never hold.
                assert_eq!(
                    constants.pow_backoff_cap(),
                    POW_BACKOFF_DISABLED,
                    "{network} entry {index}: must default to the backoff being disabled, never to 0"
                );
                // Everything else must survive the round trip untouched, or the defaults are masking a real change.
                let mut expected = current.get(index).expect("same length").clone();
                expected.bipartite_cuckaroo_verification = false;
                expected.aux_chain_merkle_proof_depth_binding = false;
                expected.derive_monero_coinbase_hasher = false;
                expected.pow_backoff_cap = POW_BACKOFF_DISABLED;
                assert_eq!(constants, &expected, "{network} entry {index}");
            }
        }
    }

    /// The agreed rollout for the aux-chain depth binding, pinned as literals - mirroring
    /// `c29_activation_matches_the_agreed_network_rollout`. Pinning matters because these are consensus values: an
    /// accidental edit is a fork, and a test that derives the expectation from the constant under test cannot catch
    /// one.
    #[test]
    fn aux_chain_depth_binding_activation_matches_the_agreed_network_rollout() {
        // The scheduled heights are aliases of the Cuckaroo ones by design, so pin them against those rather than
        // against literals: a literal here would have to be edited every time the Cuckaroo fork is rescheduled,
        // and the whole point of the alias is that no such edit is needed.
        assert_eq!(
            MAINNET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
            MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT
        );
        assert_eq!(
            ESMERALDA_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
            ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT
        );
        for unscheduled in [
            IGOR_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
            NEXTNET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
            STAGENET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
        ] {
            assert_eq!(unscheduled, UNSCHEDULED_ACTIVATION_HEIGHT);
        }
    }

    /// Both fixes ship in the same mandatory upgrade, so each network gets one flag day rather than two.
    ///
    /// The aux-chain heights alias the Cuckaroo ones, so this holds by construction today and the assertion is a
    /// guard rather than a discovery: it fails the moment somebody breaks an alias and gives this fork a literal
    /// of its own. That is the edit worth catching, because a node applying one of the two rules but not the other
    /// at a given height is a consensus split.
    ///
    /// MAINTAINER: if the two forks ever genuinely need separate flag days, delete this test *deliberately* and say
    /// so in the commit message. Do not relax it to make a reschedule compile.
    #[test]
    fn aux_chain_depth_binding_and_c29_share_a_flag_day() {
        for network in ALL_NETWORKS {
            assert_eq!(
                aux_chain_depth_binding_activation_height(network),
                c29_bipartite_activation_height(network),
                "{network}: the two GHSA-3qmx-q9pv-f3m4 fixes ship together, so their heights must coincide"
            );
        }
    }

    /// The agreed rollout: MainNet and Esmeralda are scheduled, Esmeralda first so the fork path is exercised on a
    /// live chain before mainnet reaches it. The remaining networks are deliberately unscheduled.
    #[test]
    fn c29_activation_matches_the_agreed_network_rollout() {
        assert_eq!(MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT, 350_000);
        assert_eq!(ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT, 900_000);
        for unscheduled in [
            IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT,
            NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT,
            STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT,
        ] {
            assert_eq!(unscheduled, UNSCHEDULED_ACTIVATION_HEIGHT);
        }
    }
    /// The rules actually in force either side of the fork, looked up the way the node looks them up.
    #[test]
    fn rxt_canonical_pow_data_activates_at_the_agreed_heights() {
        // LocalNet is ephemeral, so it has the rule from height 0.
        assert!(
            ConsensusConstants::for_network_at_height(Network::LocalNet, 0).require_canonical_randomxt_pow_data(),
            "LocalNet"
        );

        for network in ALL_NETWORKS.into_iter().filter(|n| *n != Network::LocalNet) {
            let activation = rxt_canonical_pow_data_activation_height(network);
            // Grandfathering: a header that validates today must still validate below the activation height.
            assert!(
                !ConsensusConstants::for_network_at_height(network, 0).require_canonical_randomxt_pow_data(),
                "{network} at height 0"
            );
            assert!(
                !ConsensusConstants::for_network_at_height(network, activation.saturating_sub(1))
                    .require_canonical_randomxt_pow_data(),
                "{network} one block below the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, activation).require_canonical_randomxt_pow_data(),
                "{network} at the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, u64::MAX).require_canonical_randomxt_pow_data(),
                "{network} above the fork"
            );
        }
    }

    /// The canonical RandomXT `pow_data` fork and the Cuckaroo fork share a flag day, for the same reason the
    /// aux-chain binding does: all three are GHSA-3qmx-q9pv-f3m4 fixes shipping in one mandatory upgrade, and a node
    /// applying one of them but not another at a given height is a consensus split. The height is an alias rather
    /// than a literal so a reschedule of the Cuckaroo fork carries this one with it; this test is what catches an
    /// edit that breaks the alias and gives this fork a height of its own.
    ///
    /// MAINTAINER: if the forks ever genuinely need separate flag days, delete this test *deliberately* and say so
    /// in the commit message. Do not relax it to make a reschedule compile.
    #[test]
    fn rxt_canonical_pow_data_and_c29_share_a_flag_day() {
        for network in ALL_NETWORKS {
            assert_eq!(
                rxt_canonical_pow_data_activation_height(network),
                c29_bipartite_activation_height(network),
                "{network}: the GHSA-3qmx-q9pv-f3m4 fixes ship together, so their heights must coincide"
            );
        }
    }

    /// The agreed rollout. MainNet and Esmeralda are scheduled; the other live networks are not. Changing a height is
    /// a deliberate edit to this test, not a silent constant change.
    ///
    /// The ordering that used to be asserted here is now structural: the heights are aliases, so the rxt entry can
    /// never sit at a different height from the Cuckaroo entry it follows in the builder, and
    /// `constants_vectors_are_sorted_by_effective_height` covers the vector as a whole.
    #[test]
    fn rxt_canonical_pow_data_activation_matches_the_agreed_network_rollout() {
        assert_eq!(rxt_canonical_pow_data_activation_height(Network::MainNet), 350_000);
        assert_eq!(rxt_canonical_pow_data_activation_height(Network::Esmeralda), 900_000);
        for network in [Network::StageNet, Network::NextNet, Network::Igor] {
            assert_eq!(
                rxt_canonical_pow_data_activation_height(network),
                UNSCHEDULED_ACTIVATION_HEIGHT,
                "{network} schedules the canonical RandomXT pow_data fork; see the note on the activation height \
                 constants before changing this"
            );
        }
        // LocalNet is ephemeral, so it carries the rule from height 0.
        assert_eq!(rxt_canonical_pow_data_activation_height(Network::LocalNet), 0);
    }
    /// The rules actually in force either side of the fork, looked up the way the node looks them up.
    #[test]
    fn strict_merkle_tree_params_decoding_activates_at_the_agreed_heights() {
        // LocalNet is ephemeral, so it has the fix from height 0.
        assert!(
            ConsensusConstants::for_network_at_height(Network::LocalNet, 0).strict_merkle_tree_parameter_decoding(),
            "LocalNet"
        );

        for network in ALL_NETWORKS.into_iter().filter(|n| *n != Network::LocalNet) {
            let activation = strict_merkle_tree_params_activation_height(network);
            // Grandfathering: every block below the activation height is decoded with the pre-fork decoder it was
            // accepted under, so nothing historical is retroactively invalidated.
            assert!(
                !ConsensusConstants::for_network_at_height(network, 0).strict_merkle_tree_parameter_decoding(),
                "{network} at height 0"
            );
            assert!(
                !ConsensusConstants::for_network_at_height(network, activation.saturating_sub(1))
                    .strict_merkle_tree_parameter_decoding(),
                "{network} one block below the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, activation).strict_merkle_tree_parameter_decoding(),
                "{network} at the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, u64::MAX).strict_merkle_tree_parameter_decoding(),
                "{network} above the fork"
            );
        }
    }

    /// The strict `MerkleTreeParameters` decoding fork and the Cuckaroo fork share a flag day, for the same reason
    /// the aux-chain binding and the canonical RandomXT `pow_data` rule do: all four are GHSA-3qmx-q9pv-f3m4 fixes
    /// shipping in one mandatory upgrade, and a node applying one of them but not another at a given height is a
    /// consensus split. The height is an alias rather than a literal so a reschedule of the Cuckaroo fork carries
    /// this one with it; this test is what catches an edit that breaks the alias and gives this fork a height of
    /// its own.
    ///
    /// MAINTAINER: if the forks ever genuinely need separate flag days, delete this test *deliberately* and say so
    /// in the commit message. Do not relax it to make a reschedule compile.
    #[test]
    fn strict_merkle_tree_params_and_c29_share_a_flag_day() {
        for network in ALL_NETWORKS {
            assert_eq!(
                strict_merkle_tree_params_activation_height(network),
                c29_bipartite_activation_height(network),
                "{network}: the GHSA-3qmx-q9pv-f3m4 fixes ship together, so their heights must coincide"
            );
        }
    }

    /// The agreed rollout. MainNet and Esmeralda are scheduled; the other live networks are not. Changing a height
    /// is a deliberate edit to this test, not a silent constant change.
    ///
    /// The ordering that the original version of this fix asserted with a `const` guard is now structural: the
    /// heights are aliases, so this entry can never sit at a different height from the Cuckaroo entry it follows in
    /// the builder, and `constants_vectors_are_sorted_by_effective_height` covers the vector as a whole.
    #[test]
    fn strict_merkle_tree_params_activation_matches_the_agreed_network_rollout() {
        assert_eq!(strict_merkle_tree_params_activation_height(Network::MainNet), 350_000);
        assert_eq!(strict_merkle_tree_params_activation_height(Network::Esmeralda), 900_000);
        for network in [Network::StageNet, Network::NextNet, Network::Igor] {
            assert_eq!(
                strict_merkle_tree_params_activation_height(network),
                UNSCHEDULED_ACTIVATION_HEIGHT,
                "{network} schedules the strict MerkleTreeParameters decoding fork; see the note on the activation \
                 height constants before changing this"
            );
        }
        // LocalNet is ephemeral, so it carries the rule from height 0.
        assert_eq!(strict_merkle_tree_params_activation_height(Network::LocalNet), 0);
    }
    /// The rules actually in force either side of the fork, looked up the way the node looks them up.
    #[test]
    fn derived_monero_coinbase_activates_at_the_agreed_heights() {
        // LocalNet is ephemeral, so it has the fix from height 0.
        assert!(
            ConsensusConstants::for_network_at_height(Network::LocalNet, 0).derive_monero_coinbase_hasher(),
            "LocalNet"
        );

        for network in ALL_NETWORKS.into_iter().filter(|n| *n != Network::LocalNet) {
            let activation = derived_monero_coinbase_activation_height(network);
            // Grandfathering: this is a wire-format change, and every merge mined header ever written carries the
            // pre-fork shape, so every one of them must keep parsing and validating exactly as it does today.
            assert!(
                !ConsensusConstants::for_network_at_height(network, 0).derive_monero_coinbase_hasher(),
                "{network} at height 0"
            );
            assert!(
                !ConsensusConstants::for_network_at_height(network, activation.saturating_sub(1))
                    .derive_monero_coinbase_hasher(),
                "{network} one block below the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, activation).derive_monero_coinbase_hasher(),
                "{network} at the fork"
            );
            assert!(
                ConsensusConstants::for_network_at_height(network, u64::MAX).derive_monero_coinbase_hasher(),
                "{network} above the fork"
            );
        }
    }

    /// The derived Monero coinbase fork and the Cuckaroo fork share a flag day, for the same reason the three rules
    /// above do: they are all GHSA-3qmx-q9pv-f3m4 fixes shipping in one mandatory upgrade, and a node applying one
    /// of them but not another at a given height is a consensus split. It matters more here than anywhere else,
    /// because this fork also changes the wire format every merge mining proxy and miner has to speak.
    ///
    /// MAINTAINER: if the forks ever genuinely need separate flag days, delete this test *deliberately* and say so
    /// in the commit message. Do not relax it to make a reschedule compile.
    #[test]
    fn derived_monero_coinbase_and_c29_share_a_flag_day() {
        for network in ALL_NETWORKS {
            assert_eq!(
                derived_monero_coinbase_activation_height(network),
                c29_bipartite_activation_height(network),
                "{network}: the GHSA-3qmx-q9pv-f3m4 fixes ship together, so their heights must coincide"
            );
        }
    }

    /// The agreed rollout. MainNet and Esmeralda are scheduled; the other live networks are not. Changing a height
    /// is a deliberate edit to this test, not a silent constant change.
    #[test]
    fn derived_monero_coinbase_activation_matches_the_agreed_network_rollout() {
        assert_eq!(derived_monero_coinbase_activation_height(Network::MainNet), 350_000);
        assert_eq!(derived_monero_coinbase_activation_height(Network::Esmeralda), 900_000);
        for network in [Network::StageNet, Network::NextNet, Network::Igor] {
            assert_eq!(
                derived_monero_coinbase_activation_height(network),
                UNSCHEDULED_ACTIVATION_HEIGHT,
                "{network} schedules the derived Monero coinbase fork; see the note on the activation height \
                 constants before changing this"
            );
        }
        // LocalNet is ephemeral, so it carries the rule from height 0.
        assert_eq!(derived_monero_coinbase_activation_height(Network::LocalNet), 0);
    }

    /// The activation height the deep reorg anchor uses is *derived* from the constants vector rather than declared
    /// next to it, so it cannot drift. This pins that derivation against the per-network constants, and in
    /// particular that an unscheduled network yields `u64::MAX` - which is what makes the anchor inert there.
    #[test]
    fn the_deep_reorg_anchor_height_is_derivable_from_the_constants() {
        for network in ALL_NETWORKS {
            let constants = ConsensusConstants::for_network(network);
            let derived = ConsensusConstants::derived_monero_coinbase_activation_height(&constants);
            assert_eq!(derived, derived_monero_coinbase_activation_height(network), "{network}");

            // And it agrees with what a height lookup says, on both sides.
            assert!(
                derived == 0 ||
                    !ConsensusConstants::for_network_at_height(network, derived.saturating_sub(1))
                        .derive_monero_coinbase_hasher(),
                "{network} below the derived height"
            );
            if derived != UNSCHEDULED_ACTIVATION_HEIGHT {
                assert!(
                    ConsensusConstants::for_network_at_height(network, derived).derive_monero_coinbase_hasher(),
                    "{network} at the derived height"
                );
            }
        }
    }

    /// `check_min_block_difficulty` selects its rules from `max(local tip, claimed height)` and leans on the
    /// advisory's flags being monotone in height: never true below the fork and never false above it. If that ever
    /// stopped holding, taking the max would stop being the stricter choice and the gossip gate would weaken.
    #[test]
    fn the_advisory_rules_are_monotone_in_height() {
        for network in ALL_NETWORKS {
            let mut seen_active = false;
            for entry in ConsensusConstants::for_network(network) {
                if entry.derive_monero_coinbase_hasher {
                    seen_active = true;
                } else {
                    assert!(
                        !seen_active,
                        "{network}: the derived Monero coinbase rule turns back off at height {}",
                        entry.effective_from_height
                    );
                }
            }
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
            // The builder skips entries still gated on the `u64::MAX` placeholder, but deliberately picks an
            // activation entry up once it has a real height. So the entry it must agree with is the newest
            // *scheduled* one, which is the activation entry itself on a network whose fork has been scheduled and
            // the entry below the fork on one where it has not. Looking it up through `runtime_lookup` rather than
            // re-implementing the builder's search keeps this test guarding `active_at_height` as well.
            let newest_scheduled = constants
                .iter()
                .map(|c| c.effective_from_height)
                .filter(|height| *height != UNSCHEDULED_ACTIVATION_HEIGHT)
                .max()
                .unwrap_or(0);
            let mut live = runtime_lookup(&constants, newest_scheduled).clone();
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

    /// The boundary the accumulated-data repair migration keys on.
    ///
    /// MainNet's GHSA-3qmx-q9pv-f3m4 entry also takes `difficulty_block_window` from 90 to 45, which makes the fix
    /// retroactive for anyone already past the fork: their stored `target_difficulty` was computed with the long
    /// window and no longer recomputes to the same value. `pow_backoff_cap` deliberately does *not* move with it -
    /// MainNet carries half a TIP-004 activation, with `MAINNET_TIP004_ACTIVATION_HEIGHT` still unscheduled - and
    /// if it is ever scheduled that is a second retroactive target change needing a second migration.
    ///
    /// `pow_backoff.rs::activation_matches_the_agreed_network_rollout` pins the window at height 0 and at the
    /// TIP-004 fork. MainNet now changes its window at a *different* height from that fork, so this boundary is
    /// not covered there.
    #[test]
    fn mainnet_narrows_the_difficulty_window_at_the_c29_fork_without_touching_the_backoff() {
        use tari_common::configuration::Network;

        use crate::consensus::consensus_constants::{
            MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT,
            POW_BACKOFF_DISABLED,
            TIP004_DIFFICULTY_BLOCK_WINDOW,
            UNSCHEDULED_ACTIVATION_HEIGHT,
        };

        const ACTIVATION: u64 = MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT;

        for height in [0, 1, ACTIVATION - 1] {
            let below = ConsensusConstants::for_network_at_height(Network::MainNet, height);
            assert_eq!(below.difficulty_block_window(), 90, "MainNet window at {height}");
            assert_eq!(below.pow_backoff_cap(), POW_BACKOFF_DISABLED, "MainNet cap at {height}");
            assert!(
                !below.bipartite_cuckaroo_verification(),
                "MainNet bipartite verifier at {height}"
            );
        }

        for height in [ACTIVATION, ACTIVATION + 1, ACTIVATION + 1_000_000] {
            let at_or_above = ConsensusConstants::for_network_at_height(Network::MainNet, height);
            assert_eq!(
                at_or_above.difficulty_block_window(),
                TIP004_DIFFICULTY_BLOCK_WINDOW,
                "MainNet window at {height}"
            );
            // Deliberately unchanged across the boundary: the C29 entry narrows the window without scheduling the
            // TIP-004 backoff.
            assert_eq!(
                at_or_above.pow_backoff_cap(),
                POW_BACKOFF_DISABLED,
                "MainNet cap at {height}"
            );
            assert!(
                at_or_above.bipartite_cuckaroo_verification(),
                "MainNet bipartite verifier at {height}"
            );
        }

        // The migration derives the fork height off the constants vector rather than off a per-network constant, so
        // it cannot go stale. Pin what it derives for every network.
        assert_eq!(
            ConsensusConstants::bipartite_cuckaroo_activation_height(&ConsensusConstants::mainnet()),
            ACTIVATION
        );
        // LocalNet is ephemeral and carries the rule on its height-0 entry, so there is nothing to repair there.
        assert_eq!(
            ConsensusConstants::bipartite_cuckaroo_activation_height(&ConsensusConstants::localnet()),
            0
        );
        assert_eq!(
            ConsensusConstants::bipartite_cuckaroo_activation_height(&ConsensusConstants::esmeralda()),
            crate::consensus::consensus_constants::ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT
        );
        // Unscheduled networks: the migration skips them entirely.
        for constants in [
            ConsensusConstants::igor(),
            ConsensusConstants::stagenet(),
            ConsensusConstants::nextnet(),
        ] {
            assert_eq!(
                ConsensusConstants::bipartite_cuckaroo_activation_height(&constants),
                UNSCHEDULED_ACTIVATION_HEIGHT
            );
        }
    }

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
