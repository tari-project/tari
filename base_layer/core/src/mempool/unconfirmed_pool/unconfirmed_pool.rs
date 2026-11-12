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

use std::{
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet},
    hash::Hash,
    sync::Arc,
};

use log::*;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    epoch::VnEpoch,
    types::{CompressedPublicKey, CompressedSignature, FixedHash, HashOutput, PrivateKey},
};
use tari_comms::protocol::rpc::RPC_MAX_FRAME_SIZE;
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    MicroMinotari,
    aggregated_body::AggregateBody,
    consensus::consensus_constants::MAX_BLOCK_BODY_BYTES,
    rpc::models::FeePerGramStat,
    transaction_components::{Transaction, TransactionError, covenants::MAX_COVENANT_WORK_PER_BODY},
    weight::TransactionWeight,
};
use tari_utilities::ByteArray;
use tokio::time::Instant;

use crate::mempool::{
    MempoolError,
    kernel_set,
    priority::{FeePriority, PrioritizedTransaction},
    shrink_hashmap::shrink_hashmap,
    unconfirmed_pool::UnconfirmedPoolError,
};
pub const LOG_TARGET: &str = "c::mp::unconfirmed_pool::unconfirmed_pool_storage";

/// The RPC frame size of nodes released before the RPC frame was raised to cover the messaging frame. A block larger
/// than this cannot be synced by those nodes.
pub const LEGACY_RPC_MAX_FRAME_SIZE: usize = 6 * 1024 * 1024;

const fn min_usize(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

/// The maximum estimated body size, in bytes, of the transactions selected for a block template, so that a block built
/// from a template can be both propagated and synced, and is never over the consensus block body byte limit.
/// Transaction bodies are measured by their borsh-serialized size (with hydrated inputs, so this over-estimates the
/// compact size that consensus measures); the 1 MiB margin covers the coinbase, the header and protobuf encoding
/// overhead. This is a local block-building policy, not a consensus rule.
///
/// TODO: this is a rollout value, derived from the smaller RPC frame of not-yet-upgraded nodes (5 MiB) so that they can
/// still sync every block this node builds. Drop the legacy frame from the minimum once the network has upgraded.
pub const MAX_BLOCK_TEMPLATE_BODY_BYTES: usize = min_usize(
    min_usize(LEGACY_RPC_MAX_FRAME_SIZE, RPC_MAX_FRAME_SIZE),
    MAX_BLOCK_BODY_BYTES,
) - 1024 * 1024;
const _: () = assert!(MAX_BLOCK_TEMPLATE_BODY_BYTES < LEGACY_RPC_MAX_FRAME_SIZE);
const _: () = assert!(MAX_BLOCK_TEMPLATE_BODY_BYTES < RPC_MAX_FRAME_SIZE);
// Every network's `max_block_body_bytes` is MAX_BLOCK_BODY_BYTES, so a template within this budget is always valid
const _: () = assert!(MAX_BLOCK_TEMPLATE_BODY_BYTES <= MAX_BLOCK_BODY_BYTES);

/// The smallest body a transaction can practically have (one input, one output and one kernel serialize to well over
/// this). Once less than this remains of the template byte budget, no further transaction can fit and selection stops.
/// This assumes at least one output; a transaction without outputs can be smaller, so stopping here costs at most this
/// much of unused budget.
pub const MIN_TRANSACTION_BODY_BYTES: usize = 1024;

/// A selection pass counts a candidate that does not fit by weight towards `weight_tx_skip_count` only once the
/// template is "full enough": when less than `max_block_transaction_weight / BLOCK_FULL_ENOUGH_DIVISOR` (5%) of the
/// weight remains. Before that, a candidate that is too heavy is no evidence that the block is full (e.g. a handful of
/// heavy transactions ranked just below a small one that was selected first), so it is passed over without counting
/// towards the skip allowance. The work it cost (walking its ancestors) is bounded by [MAX_WALKED_FRAMES_PER_PASS].
pub const BLOCK_FULL_ENOUGH_DIVISOR: u64 = 20;

/// The maximum number of transactions one selection pass visits while collecting candidates' unselected ancestors.
/// When reached, the pass considers no further candidates (but still selects from the branches it already queued, which
/// needs no further walking), and reports `byte_bound`. This is the only bound on the work of passing over candidates
/// that do not fit, since such candidates never end a pass on their own. This bounds the work and memory of a pass over
/// an adversarial pool (e.g. a long chain of low-fee transactions with many children, each of which would otherwise
/// have its whole ancestry walked and stored).
pub const MAX_WALKED_FRAMES_PER_PASS: usize = 200_000;

/// The maximum number of unselected ancestors a candidate may have for a selection pass to consider it. A candidate
/// with more is dropped when the walk reaches the limit, without counting towards any skip allowance: it is no evidence
/// that the block is full, only junk-shaped (honest wallets do not build chains of unconfirmed transactions this deep).
/// This is miner-side selection policy only - neither admission policy nor consensus: such a transaction stays in the
/// pool and becomes selectable once its ancestors are mined. It also makes exhausting [MAX_WALKED_FRAMES_PER_PASS]
/// take thousands of candidates per pass rather than hundreds.
pub const MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE: usize = 100;

/// The maximum number of dependency links one selection pass follows (by ancestor walks and by the pre-walk ancestor
/// weight bound). A transaction can have as many links as inputs, so this bounds the work that visiting a bounded
/// number of transactions can still cost. When it runs out, the pass ends as if [MAX_WALKED_FRAMES_PER_PASS] had.
pub const MAX_FOLLOWED_LINKS_PER_PASS: usize = 2_000_000;

/// The maximum number of dependency links one candidate's pre-walk ancestor weight bound may follow, and separately the
/// maximum its walk plus its branch's internal-duplicate check may cost (each link, input, output or kernel checked
/// counts as one). A candidate over either is dropped like one with too many unselected ancestors. (Looking up the
/// colliding pairs already remembered costs at most |branch|^2 per branch, also charged to the pass's link budget.) The
/// walk does not charge the pass again for the candidate's own links (the pre-walk bound already did), so a candidate
/// is charged at most about this much, and exhausting [MAX_FOLLOWED_LINKS_PER_PASS] takes about as many candidates as
/// exhausting [MAX_WALKED_FRAMES_PER_PASS] with [MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE]-deep ancestries (~2,000).
/// Producers of an output examined beyond the highest priority one (see [MAX_EXTRA_PRODUCERS_PER_LOOKUP]) count as
/// links too, in both.
pub const MAX_LINKS_PER_CANDIDATE: usize =
    MAX_FOLLOWED_LINKS_PER_PASS / (MAX_WALKED_FRAMES_PER_PASS / MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE);

/// The most producers of an output, beyond the highest priority one, that a selection pass examines to find one the
/// block template allows (see [UnconfirmedPool::find_template_producer]). An honest output has one producer, or a few
/// when a transaction was sent again with other kernels; any number can be added by conflicting transactions, so past
/// this the branch is left out instead.
pub const MAX_EXTRA_PRODUCERS_PER_LOOKUP: usize = 8;

/// The most pool transactions that may spend one output (see [UnconfirmedPool::conflict_set_victims]). Only one of them
/// can ever be mined; this bounds the pool capacity and lookup work that copies of a transaction padded with other
/// kernels can take. Mempool policy, not a consensus rule.
pub const MAX_SPENDS_PER_OUTPUT: usize = 8;

/// The outcome of a branch's internal-duplicate check
enum DuplicateCheck {
    Clean,
    /// The branch can never be in a valid block
    Invalid {
        /// Transactions that are invalid on their own (they repeat an input, output, commitment or kernel)
        self_invalid: Vec<TransactionKey>,
        /// Pairs of transactions that collide with each other (lower priority first)
        pairs: Vec<(TransactionKey, TransactionKey)>,
    },
    /// The check would cost more than the candidate's [MAX_LINKS_PER_CANDIDATE]
    TooManyLinks,
}

/// The outcome of the pre-walk ancestor weight bound
enum BoundOutcome {
    Bound(u64),
    /// The pass's [MAX_FOLLOWED_LINKS_PER_PASS] budget ran out
    OutOfLinks,
    /// The candidate has more than [MAX_LINKS_PER_CANDIDATE] links
    TooManyLinks,
}

/// How an ancestor walk ended
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalkOutcome {
    Complete,
    /// The pass's [MAX_WALKED_FRAMES_PER_PASS] or [MAX_FOLLOWED_LINKS_PER_PASS] budget ran out
    OutOfFrames,
    /// The candidate has more than [MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE] unselected ancestors, or its walk would
    /// follow more than [MAX_LINKS_PER_CANDIDATE] links
    TooManyAncestors,
}

/// A transaction's weight for ordering a block template *under byte pressure*: its real weight, or its share of the
/// template byte budget expressed in grams, whichever is larger:
///
/// `max(weight, ceil(body_size * max_block_transaction_weight / max_body_bytes))`
///
/// Transaction weight does not price input bytes (deliberately: cheap inputs are an incentive to consolidate and
/// prune), so under byte pressure a byte-heavy transaction could otherwise take the byte budget while paying for only a
/// few percent of the block's weight. When the byte budget binds, selection is also run ordered by fee per effective
/// weight, so that a byte-heavy transaction must pay for the share of the byte budget it consumes (see
/// [UnconfirmedPool::fetch_template_txs]). Without byte pressure, and for the stored priority, only the real
/// weight is used. This is miner policy only: the block is always filled by real weight.
pub fn effective_weight(
    weight: u64,
    body_size: usize,
    max_block_transaction_weight: u64,
    max_body_bytes: usize,
) -> u64 {
    if max_body_bytes == 0 {
        return weight;
    }
    let body_size = u128::try_from(body_size).unwrap_or(u128::MAX);
    let max_body_bytes = u128::try_from(max_body_bytes).unwrap_or(u128::MAX);
    let byte_weight = body_size
        .saturating_mul(u128::from(max_block_transaction_weight))
        .div_ceil(max_body_bytes);
    weight.max(u64::try_from(byte_weight).unwrap_or(u64::MAX))
}

pub type TransactionKey = usize;

/// Configuration for the UnconfirmedPool
#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct UnconfirmedPoolConfig {
    /// The maximum number of transactions that can be stored in the Unconfirmed Transaction pool
    pub storage_capacity: usize,
    /// The maximum number of transactions that can be skipped when compiling a set of highest priority transactions,
    /// skipping over large transactions are performed in an attempt to fit more transactions into the remaining space.
    pub weight_tx_skip_count: usize,
    /// The minimum fee accepted by this mempool
    pub min_fee: u64,
}

impl Default for UnconfirmedPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: 40_000,
            weight_tx_skip_count: 20,
            min_fee: 50,
        }
    }
}

/// The Unconfirmed Transaction Pool consists of all unconfirmed transactions that are ready to be included in a block
/// and they are prioritised according to the priority metric.
/// The txs_by_signature HashMap is used to find a transaction using its excess_sig, this functionality is used to match
/// transactions included in blocks with transactions stored in the pool. The txs_by_priority BTreeMap prioritise the
/// transactions in the pool according to TXPriority, it allows transactions to be inserted in sorted order by their
/// priority. The txs_by_priority BTreeMap makes it easier to select the set of highest priority transactions that can
/// be included in a block. The excess_sig of a transaction is used as a key to uniquely identify a specific transaction
/// in these containers.
pub struct UnconfirmedPool {
    pub(crate) config: UnconfirmedPoolConfig,
    key_counter: usize,
    tx_by_key: HashMap<TransactionKey, PrioritizedTransaction>,
    txs_by_signature: HashMap<PrivateKey, Vec<TransactionKey>>,
    /// The pool transaction with each kernel set (see [kernel_set]). Insert refuses an exact match, so there is one.
    tx_by_kernel_set: HashMap<Vec<PrivateKey>, TransactionKey>,
    tx_by_priority: BTreeMap<FeePriority, TransactionKey>,
    txs_by_output: HashMap<HashOutput, Vec<TransactionKey>>,
    /// For each output spent by a pool transaction, the pool transactions spending it (from their stored input hashes)
    txs_by_spent_output: HashMap<HashOutput, Vec<TransactionKey>>,
    /// For each output commitment produced by a pool transaction, the pool transactions producing it. A block may not
    /// contain two outputs with the same commitment, even if the outputs differ otherwise (and so hash differently).
    txs_by_output_commitment: HashMap<FixedHash, Vec<TransactionKey>>,
    /// For each kernel excess in a pool transaction, the pool transactions containing it. A block may not contain two
    /// kernels with the same excess, even with different signatures (the chain's kernel excess index is unique).
    txs_by_kernel_excess: HashMap<FixedHash, Vec<TransactionKey>>,
    txs_by_unique_id: HashMap<[u8; 32], Vec<TransactionKey>>,
    /// For each validator node registered or exited by a pool transaction, the (single) pool transaction doing so. A
    /// block may not register or exit the same validator node twice, and the second one only fails when the block is
    /// validated, so two such transactions in the pool would make every block template fail (see
    /// [UnconfirmedPool::validator_node_conflict_holders]).
    txs_by_validator_node: HashMap<ValidatorNodeKey, TransactionKey>,
    /// The pool transactions with an input script or covenant that reads the block height (see [reads_block_height]).
    /// Unlike every other check, these can stop being valid as the chain grows, so they are re-checked when they are
    /// selected for a block template, and in bounded batches after each block.
    txs_reading_height: BTreeMap<TransactionKey, HeightReadingEntry>,
    /// The key in `txs_reading_height` the next background re-check starts at (see
    /// [UnconfirmedPool::height_reading_sweep_batch])
    sweep_cursor: TransactionKey,
    /// The template byte budget used for selection and for effective weights (see [effective_weight])
    max_body_bytes: usize,
    /// The index entry elements visited while pruning removed transactions (see [UnconfirmedPool::prune_indexes])
    #[cfg(test)]
    prune_visits: usize,
}

/// The block a transaction with an input script or covenant that reads the height was validated for
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeightValidation {
    /// The height of the block
    pub height: u64,
    /// The keys its height-reading input scripts returned there, in input order
    pub script_keys: Vec<CompressedPublicKey>,
}

/// A pool transaction whose input scripts or covenants read the block height
#[derive(Debug, Clone)]
struct HeightReadingEntry {
    validation: HeightValidation,
    /// A bound on the work of re-checking it (see [height_check_work])
    work: u64,
    /// The previous block hash of the last block it was re-checked for and passed
    passed_at: Option<HashOutput>,
}

/// Which pool transactions a block template may contain, beyond fitting in the block. A transaction that is left out
/// leaves out the transactions that depend on it too.
#[derive(Debug, Clone)]
pub struct TemplateFilter {
    /// The height of the block: a transaction with an input that is not yet mature there, or a kernel locked above
    /// it, is left out
    pub height: u64,
    /// Transactions to leave out
    pub excluded: HashSet<TransactionKey>,
    /// If set, a transaction with an input script or covenant that reads the height is left out unless it passed a
    /// re-check for a block on top of this one
    pub only_checked_at: Option<HashOutput>,
}

impl TemplateFilter {
    /// A filter that leaves nothing out
    #[cfg(test)]
    pub fn unrestricted() -> Self {
        Self {
            height: u64::MAX,
            excluded: HashSet::new(),
            only_checked_at: None,
        }
    }
}

/// A validator node registered or exited by a transaction: (sidechain public key, validator node public key), as bytes
type ValidatorNodeKey = (Option<Vec<u8>>, Vec<u8>);

/// Whether any input of `body` has a script or covenant that reads the block height, so that whether `body` is valid
/// can change from one block to the next
pub(crate) fn reads_block_height(body: &AggregateBody) -> bool {
    body.inputs().iter().any(|input| {
        input.script().is_ok_and(|script| script.is_context_sensitive()) ||
            input.covenant().is_ok_and(|covenant| covenant.reads_block_height())
    })
}

/// The cost of running one input script, in covenant work units (see [MAX_COVENANT_WORK_PER_BODY]). A script at the
/// size limit, packed with signature checks, runs in about 0.8 ms. A covenant work unit costs about 60 ns for typical
/// outputs, and up to a few microseconds when it hashes a field at its size limit, so a script is charged as 200 units.
pub const SCRIPT_INPUT_WORK: u64 = 200;

/// A bound, in covenant work units, on the work of re-checking `body` for another height (see `height_check` in the
/// mempool storage): [SCRIPT_INPUT_WORK] for each input script that reads the height, and if any covenant reads the
/// height, the limit on evaluating all of them, [MAX_COVENANT_WORK_PER_BODY] (each is evaluated again, and the
/// outputs a height filter admits can change with the height, so the work seen at admission is no bound).
fn height_check_work(body: &AggregateBody) -> u64 {
    let mut work = 0u64;
    let mut covenant_reads_height = false;
    for input in body.inputs() {
        if input.script().is_ok_and(|script| script.is_context_sensitive()) {
            work = work.saturating_add(SCRIPT_INPUT_WORK);
        }
        if input.covenant().is_ok_and(|covenant| covenant.reads_block_height()) {
            covenant_reads_height = true;
        }
    }
    if covenant_reads_height {
        work = work.saturating_add(MAX_COVENANT_WORK_PER_BODY);
    }
    work
}

/// Removes the keys that are not `is_live` from each `touched` entry of `index`, and the entries left empty, adding the
/// keys visited to `visits`. Retaining keeps each entry's order.
fn prune_entries<K: Eq + Hash>(
    index: &mut HashMap<K, Vec<TransactionKey>>,
    touched: HashSet<K>,
    is_live: &impl Fn(&TransactionKey) -> bool,
    visits: &mut usize,
) {
    for entry_key in touched {
        if let Some(keys) = index.get_mut(&entry_key) {
            *visits = visits.saturating_add(keys.len());
            keys.retain(is_live);
            if keys.is_empty() {
                index.remove(&entry_key);
            }
        }
    }
}

/// The validator nodes `body` registers or exits. A registration and an exit for the same validator node share a key:
/// they can never both be valid against the same chain state.
/// The validator nodes `body` registers or exits (outputs), or whose registration output it spends (inputs, which
/// cancel a pending registration or reclaim an exited one's stake). Transactions sharing a key are treated as
/// conflicting in the pool: in almost all cases at most one of them can be valid in a block, and their validity depends
/// on validator node state that changes with each block. The exception (reclaiming an exited registration's stake and
/// re-registering the same validator node in one block) is only delayed, since the wallet retries a taken slot.
fn validator_node_keys(body: &AggregateBody) -> Vec<ValidatorNodeKey> {
    let inputs = body.inputs().iter().filter_map(|input| {
        let features = input.features().ok()?.sidechain_feature.as_ref()?;
        let reg = features.validator_node_registration()?;
        Some((
            features.sidechain_public_key().map(|pk| pk.as_bytes().to_vec()),
            reg.public_key().as_bytes().to_vec(),
        ))
    });
    // A single transaction may legitimately touch the same validator node twice (e.g. reclaim an exited registration's
    // stake and register the node again), so the keys are deduplicated per body.
    let mut keys = validator_node_output_keys(body);
    keys.extend(inputs);
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// The validator nodes `body` registers or exits, i.e. the ones whose state is changed by mining it. Spends of a
/// registration output are not included: they do not invalidate other transactions for the same validator node (a
/// newer registration's exit stays valid when an older, exited registration's stake is reclaimed).
fn validator_node_output_keys(body: &AggregateBody) -> Vec<ValidatorNodeKey> {
    body.outputs()
        .iter()
        .filter_map(|output| {
            let features = output.features.sidechain_feature.as_ref()?;
            let public_key = features
                .validator_node_registration()
                .map(|reg| reg.public_key())
                .or_else(|| features.validator_node_exit().map(|exit| exit.public_key()))?;
            Some((
                features.sidechain_public_key().map(|pk| pk.as_bytes().to_vec()),
                public_key.as_bytes().to_vec(),
            ))
        })
        .collect()
}

/// The earliest `max_epoch` of the validator node registrations and exits in `body`, if it has any
fn validator_node_max_epoch(body: &AggregateBody) -> Option<VnEpoch> {
    body.outputs()
        .iter()
        .filter_map(|output| {
            let features = output.features.sidechain_feature.as_ref()?;
            features
                .validator_node_registration()
                .map(|reg| reg.max_epoch())
                .or_else(|| features.validator_node_exit().map(|exit| exit.max_epoch()))
        })
        .min()
}

/// The order in which a selection pass considers transactions
#[derive(Clone, Copy, Debug)]
enum Ranking {
    /// By fee per gram of real weight (the stored priority)
    Weight,
    /// By fee per gram of effective weight (see [effective_weight])
    EffectiveWeight {
        max_block_transaction_weight: u64,
        max_body_bytes: usize,
    },
}

/// The outcome of one selection pass
struct Selection {
    results: RetrieveResults,
    total_fees: u64,
    /// Whether the byte budget bound: some candidate was passed over because its bytes did not fit, or the pass ended
    /// early because of bytes or because it ran out of a work budget
    byte_bound: bool,
    /// The number of candidates passed over because only their bytes did not fit
    #[cfg_attr(not(test), allow(dead_code))]
    byte_skips: usize,
    /// The number of candidates dropped because they conflict with a selected transaction
    #[cfg_attr(not(test), allow(dead_code))]
    conflict_drops: usize,
    /// The number of candidates that did not fit by weight before the template was full enough (see
    /// [BLOCK_FULL_ENOUGH_DIVISOR])
    #[cfg_attr(not(test), allow(dead_code))]
    early_weight_skips: usize,
    /// The number of candidates dropped for having too many unselected ancestors
    #[cfg_attr(not(test), allow(dead_code))]
    too_many_ancestors_drops: usize,
    /// The number of skips counted towards `weight_tx_skip_count`
    #[cfg_attr(not(test), allow(dead_code))]
    weight_skips_counted: usize,
    /// The candidate on which the walking budget ran out, if it did
    #[cfg_attr(not(test), allow(dead_code))]
    out_of_frames_candidate: Option<TransactionKey>,
    /// The transactions the pass's walks visited (see [MAX_WALKED_FRAMES_PER_PASS])
    #[cfg_attr(not(test), allow(dead_code))]
    walk_work: usize,
    /// The dependency links the pass followed (see [MAX_FOLLOWED_LINKS_PER_PASS])
    #[cfg_attr(not(test), allow(dead_code))]
    links_followed: usize,
    /// Branches dropped because two of their own transactions double-spend or duplicate an output
    #[cfg_attr(not(test), allow(dead_code))]
    invalid_branch_drops: usize,
    /// Branches dropped because they contain both members of a remembered colliding pair
    #[cfg_attr(not(test), allow(dead_code))]
    pair_drops: usize,
    /// The work of looking up remembered pairs
    #[cfg_attr(not(test), allow(dead_code))]
    pair_lookup_work: usize,
    /// Branches dropped by the final validity sweep (expected to be 0)
    #[cfg_attr(not(test), allow(dead_code))]
    swept_branches: usize,
    /// Whether the pass ended before considering every candidate
    ended_early: bool,
    /// Whether the pass ended because a work budget ([MAX_WALKED_FRAMES_PER_PASS] or [MAX_FOLLOWED_LINKS_PER_PASS])
    /// ran out
    budget_exhausted: bool,
    /// Candidates this pass dropped for an INTRINSIC reason, one no ordering changes: too many ancestors or links,
    /// a branch that double-spends or duplicates internally (or contains a remembered colliding pair), or a
    /// transaction invalid on its own. Never a candidate that was still queued.
    walked_dropped: HashSet<TransactionKey>,
    /// Candidates this pass walked and dropped for a STATE-DEPENDENT reason: conflicting with this pass's selection,
    /// or not fitting the weight or bytes it had left. Another ordering may select them.
    state_dropped: HashSet<TransactionKey>,
}

/// The working state of one selection pass
struct SelectionState<'a> {
    total_weight: u64,
    max_body_bytes: usize,
    ranking: Ranking,
    selected_txs: HashMap<TransactionKey, Arc<Transaction>>,
    /// Pool transactions that spend an output a selected transaction spends (found through the pool's
    /// `txs_by_spent_output` index when a branch is selected): they can never be mined alongside the selection
    conflicting: HashSet<TransactionKey>,
    /// The branches selected, in order (for the final validity sweep)
    selected_branches: Vec<Vec<TransactionKey>>,
    /// Branches dropped because two of their own transactions double-spend or duplicate an output
    invalid_branch_drops: usize,
    /// Colliding pairs found by the internal-duplicate check (both directions): a later branch containing both members
    /// of a pair is dropped without checking again. A single member on its own is never dropped for it, so a
    /// transaction built to collide with an honest one cannot censor it.
    remembered_pairs: HashMap<TransactionKey, HashSet<TransactionKey>>,
    /// The work of looking up remembered pairs (see [SelectionState::branch_has_remembered_pair])
    pair_lookup_work: usize,
    /// Branches dropped because they contain both members of a remembered pair
    pair_drops: usize,
    curr_weight: u64,
    curr_body_bytes: usize,
    /// Skips counting towards `weight_tx_skip_count`
    weight_skips: usize,
    /// Byte-only skips, which do not count towards `weight_tx_skip_count`
    byte_skips: usize,
    /// Candidates dropped for conflicting with a selected transaction; neither counted nor bounded
    conflict_drops: usize,
    /// Weight failures before the template was full enough
    early_weight_skips: usize,
    /// Candidates dropped for having more than [MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE] unselected ancestors
    too_many_ancestors_drops: usize,
    /// The remaining weight below which the template is full enough (see [BLOCK_FULL_ENOUGH_DIVISOR])
    full_enough_weight: u64,
    /// The number of transactions that may still be visited while walking ancestors (see
    /// [MAX_WALKED_FRAMES_PER_PASS])
    frames_left: usize,
    /// The number of dependency links this pass may still follow (see [MAX_FOLLOWED_LINKS_PER_PASS])
    links_left: usize,
    frame_budget_exhausted: bool,
    out_of_frames_candidate: Option<TransactionKey>,
    /// Candidates to leave out of this pass (see [UnconfirmedPool::fetch_selection]): the first pass's intrinsic
    /// drops, and also its state-dependent drops if it ran out of a work budget
    excluded: HashSet<TransactionKey>,
    /// See [Selection::walked_dropped]
    walked_dropped: HashSet<TransactionKey>,
    /// See [Selection::state_dropped]
    state_dropped: HashSet<TransactionKey>,
    /// For transactions whose ancestors were walked: a lower bound on the weight of the transaction together with its
    /// unselected ancestors, `w(X) + max(bound(dependency))`. Sound for a DAG; cleared whenever anything is selected.
    /// Lets a candidate whose dependencies are already known to be too heavy be passed over without walking them
    /// again.
    ancestor_weight_bounds: HashMap<TransactionKey, u64>,
    byte_bound: bool,
    /// Set when the pass must end without considering further candidates
    stopped: bool,
    transactions_to_remove_and_recheck: Vec<(TransactionKey, Arc<Transaction>)>,
    complete_transaction_branch: CompleteTransactionBranch,
    potentional_to_add: BinaryHeap<(u64, TransactionKey)>,
    /// For each transaction, the transactions that depend on it. When it is selected, all of them are marked for
    /// recomputing.
    depended_on: HashMap<TransactionKey, Vec<&'a TransactionKey>>,
    recompute: HashSet<&'a TransactionKey>,
}

impl SelectionState<'_> {
    fn new(
        total_weight: u64,
        max_body_bytes: usize,
        ranking: Ranking,
        max_block_transaction_weight: u64,
        excluded: HashSet<TransactionKey>,
    ) -> Self {
        Self {
            total_weight,
            max_body_bytes,
            ranking,
            selected_txs: HashMap::new(),
            conflicting: HashSet::new(),
            selected_branches: Vec::new(),
            invalid_branch_drops: 0,
            remembered_pairs: HashMap::new(),
            pair_lookup_work: 0,
            pair_drops: 0,
            curr_weight: 0,
            curr_body_bytes: 0,
            weight_skips: 0,
            byte_skips: 0,
            conflict_drops: 0,
            early_weight_skips: 0,
            too_many_ancestors_drops: 0,
            full_enough_weight: max_block_transaction_weight / BLOCK_FULL_ENOUGH_DIVISOR,
            frames_left: MAX_WALKED_FRAMES_PER_PASS,
            links_left: MAX_FOLLOWED_LINKS_PER_PASS,
            frame_budget_exhausted: false,
            out_of_frames_candidate: None,
            excluded,
            walked_dropped: HashSet::new(),
            state_dropped: HashSet::new(),
            ancestor_weight_bounds: HashMap::new(),
            byte_bound: false,
            stopped: false,
            transactions_to_remove_and_recheck: Vec::new(),
            complete_transaction_branch: CompleteTransactionBranch::new(),
            potentional_to_add: BinaryHeap::new(),
            depended_on: HashMap::new(),
            recompute: HashSet::new(),
        }
    }

    fn remaining_bytes(&self) -> usize {
        self.max_body_bytes.saturating_sub(self.curr_body_bytes)
    }

    /// Whether no further transaction can fit in the byte budget
    fn out_of_bytes(&self) -> bool {
        self.remaining_bytes() < MIN_TRANSACTION_BODY_BYTES
    }

    /// Whether so little weight remains that a candidate that does not fit by weight is evidence that the block is full
    fn full_enough(&self) -> bool {
        self.total_weight.saturating_sub(self.curr_weight) < self.full_enough_weight
    }

    /// Records a candidate that does not fit by weight. Returns whether the pass must stop (the skip allowance is used
    /// up).
    fn weight_skip(&mut self, weight_tx_skip_count: usize) -> bool {
        if self.full_enough() {
            self.weight_skips = self.weight_skips.saturating_add(1);
            self.weight_skips >= weight_tx_skip_count
        } else {
            self.early_weight_skips = self.early_weight_skips.saturating_add(1);
            false
        }
    }

    /// Records a candidate passed over only because its bytes did not fit
    fn byte_skip(&mut self) {
        self.byte_skips = self.byte_skips.saturating_add(1);
        self.byte_bound = true;
    }

    /// Remembers, for the rest of the pass, the result of an internal-duplicate check that found the branch invalid.
    /// A transaction invalid on its own can never be mined, so it is treated as conflicting (and left out of the second
    /// pass). A colliding pair is remembered as a pair: only a later branch containing both is dropped.
    fn remember_invalid(&mut self, self_invalid: Vec<TransactionKey>, pairs: Vec<(TransactionKey, TransactionKey)>) {
        for key in self_invalid {
            self.conflicting.insert(key);
            self.walked_dropped.insert(key);
        }
        for (a, b) in pairs {
            self.remembered_pairs.entry(a).or_default().insert(b);
            self.remembered_pairs.entry(b).or_default().insert(a);
        }
    }

    /// Whether `branch` contains both members of a remembered colliding pair. For each branch transaction with
    /// remembered partners, the smaller of its partner set and the branch is scanned against the other (set lookups),
    /// so the work is at most |branch|^2 however many partners a transaction has collected. The work is charged to
    /// `links_left`.
    fn branch_has_remembered_pair(&mut self, branch: &HashMap<TransactionKey, Arc<Transaction>>) -> bool {
        if self.remembered_pairs.is_empty() {
            return false;
        }
        let mut work = 0usize;
        let mut found = false;
        for key in branch.keys() {
            let Some(partners) = self.remembered_pairs.get(key) else {
                continue;
            };
            if partners.len() > branch.len() {
                work = work.saturating_add(branch.len());
                found = branch.keys().any(|other| partners.contains(other));
            } else {
                work = work.saturating_add(partners.len());
                found = partners.iter().any(|partner| branch.contains_key(partner));
            }
            if found {
                break;
            }
        }
        self.pair_lookup_work = self.pair_lookup_work.saturating_add(work);
        self.links_left = self.links_left.saturating_sub(work);
        found
    }

    /// Records a candidate dropped for conflicting with a selected transaction. This is deliberately neither counted
    /// towards any limit nor a reason to stop: it can never be mined alongside what is selected.
    fn conflict_drop(&mut self) {
        self.conflict_drops = self.conflict_drops.saturating_add(1);
    }

    /// Whether any transaction in the branch can never be mined alongside the selection. O(1) per transaction.
    fn branch_conflicts(&self, branch: &HashMap<TransactionKey, Arc<Transaction>>) -> bool {
        branch.keys().any(|key| self.conflicting.contains(key))
    }
}

// helper class to reduce type complexity
#[derive(Debug, Clone)]
pub struct RetrieveResults {
    pub retrieved_transactions: Vec<Arc<Transaction>>,
    pub transactions_to_remove_and_insert: Vec<(TransactionKey, Arc<Transaction>)>,
}

/// For each candidate branch: the transactions still to be selected, their total real weight, their total ranking
/// weight (the real weight, or the effective weight under byte pressure, see [effective_weight]) and their total fees.
pub type CompleteTransactionBranch =
    HashMap<TransactionKey, (HashMap<TransactionKey, Arc<Transaction>>, u64, u64, u64)>;

impl UnconfirmedPool {
    /// Create a new UnconfirmedPool with the specified configuration
    pub fn new(config: UnconfirmedPoolConfig) -> Self {
        Self {
            config,
            key_counter: 0,
            tx_by_key: HashMap::new(),
            txs_by_signature: HashMap::new(),
            tx_by_kernel_set: HashMap::new(),
            tx_by_priority: BTreeMap::new(),
            txs_by_output: HashMap::new(),
            txs_by_spent_output: HashMap::new(),
            txs_by_output_commitment: HashMap::new(),
            txs_by_kernel_excess: HashMap::new(),
            txs_by_unique_id: HashMap::new(),
            txs_by_validator_node: HashMap::new(),
            txs_reading_height: BTreeMap::new(),
            sweep_cursor: 0,
            max_body_bytes: MAX_BLOCK_TEMPLATE_BODY_BYTES,
            #[cfg(test)]
            prune_visits: 0,
        }
    }

    /// Use a different template byte budget, for tests
    #[cfg(test)]
    pub(crate) fn with_max_body_bytes(mut self, max_body_bytes: usize) -> Self {
        self.max_body_bytes = max_body_bytes;
        self
    }

    /// Insert a new transaction into the UnconfirmedPool, as validated for height 0 with no script keys (see
    /// [UnconfirmedPool::insert_with_height_validation])
    #[cfg(test)]
    pub fn insert(
        &mut self,
        tx: Arc<Transaction>,
        dependent_outputs: Option<Vec<HashOutput>>,
        transaction_weighting: &TransactionWeight,
    ) -> Result<(), UnconfirmedPoolError> {
        if self.has_transaction(&tx) {
            return Ok(());
        }
        self.insert_with_height_validation(
            tx,
            dependent_outputs,
            transaction_weighting,
            HeightValidation::default(),
        )
    }

    /// Insert a new transaction into the UnconfirmedPool. Low priority transactions will be removed to make space for
    /// higher priority transactions. The lowest priority transactions will be removed when the maximum capacity is
    /// reached and the new transaction has a higher priority than the currently stored lowest priority transaction.
    /// `height_validation` is the block the transaction was validated for, if it reads the height. The caller checks
    /// that `tx` is not already in the pool (see [UnconfirmedPool::has_transaction]).
    pub fn insert_with_height_validation(
        &mut self,
        tx: Arc<Transaction>,
        dependent_outputs: Option<Vec<HashOutput>>,
        transaction_weighting: &TransactionWeight,
        height_validation: HeightValidation,
    ) -> Result<(), UnconfirmedPoolError> {
        let new_key = self.get_next_key();
        let prioritized_tx = PrioritizedTransaction::new(new_key, transaction_weighting, tx, dependent_outputs)?;
        if self.tx_by_key.len() >= self.config.storage_capacity {
            if prioritized_tx.priority < *self.lowest_priority()? {
                return Ok(());
            }
            self.remove_lowest_priority_tx()?;
        }

        self.tx_by_priority.insert(prioritized_tx.priority.clone(), new_key);
        // Each producers entry is kept ordered by priority, highest first (see `find_highest_priority_transaction`)
        let tx_by_key = &self.tx_by_key;
        for output in prioritized_tx.transaction.body.outputs() {
            let producers = self.txs_by_output.entry(output.hash()).or_default();
            let position = producers.partition_point(|key| {
                tx_by_key
                    .get(key)
                    .is_some_and(|producer| producer.priority > prioritized_tx.priority)
            });
            producers.insert(position, new_key);
        }
        for input_hash in &prioritized_tx.input_hashes {
            self.txs_by_spent_output.entry(*input_hash).or_default().push(new_key);
        }
        for commitment in &prioritized_tx.output_commitments {
            self.txs_by_output_commitment
                .entry(*commitment)
                .or_default()
                .push(new_key);
        }
        for excess in &prioritized_tx.kernel_excesses {
            self.txs_by_kernel_excess.entry(*excess).or_default().push(new_key);
        }
        for kernel in prioritized_tx.transaction.body.kernels() {
            let sig = kernel.excess_sig.get_signature();
            self.txs_by_signature.entry(sig.clone()).or_default().push(new_key);
        }
        if let Some(set) = kernel_set(&prioritized_tx.transaction) {
            self.tx_by_kernel_set.insert(set, new_key);
        }

        for vn_key in validator_node_keys(&prioritized_tx.transaction.body) {
            self.txs_by_validator_node.entry(vn_key).or_insert(new_key);
        }
        if reads_block_height(&prioritized_tx.transaction.body) {
            self.txs_reading_height.insert(new_key, HeightReadingEntry {
                validation: height_validation,
                work: height_check_work(&prioritized_tx.transaction.body),
                passed_at: None,
            });
        }

        debug!(
            target: LOG_TARGET,
            "Inserted transaction {prioritized_tx} into unconfirmed pool:"
        );
        self.tx_by_key.insert(new_key, prioritized_tx);

        Ok(())
    }

    /// The pool transactions holding a validator node registration/exit slot that `tx` also registers or exits. At most
    /// one pool transaction may hold a slot: two would each be valid on their own, so both would be selected into a
    /// block template, the template would fail validation (duplicate registration/exit in one body), and re-validating
    /// them individually after the failed block would put both back, failing every later template the same way. The
    /// caller checks that `tx` is not already in the pool (see [UnconfirmedPool::has_transaction]).
    pub fn validator_node_conflict_holders(&self, tx: &Transaction) -> Vec<TransactionKey> {
        let mut holders = validator_node_keys(&tx.body)
            .iter()
            .filter_map(|vn_key| self.txs_by_validator_node.get(vn_key))
            .copied()
            .collect::<Vec<_>>();
        holders.sort_unstable();
        holders.dedup();
        holders
    }

    /// The pool transactions to evict so that `tx` can be stored, or None if it may not be stored: at most
    /// [MAX_SPENDS_PER_OUTPUT] pool transactions may spend one output. Where that many already spend an output `tx`
    /// spends, `tx` is stored only if its kernel set is a strict subset of each of theirs. Any valid spend of an output
    /// carries its owner's kernels, so the owner's transaction is a strict subset of every copy of it padded with other
    /// kernels and is always stored, and each such replacement shrinks the kernels the spenders share.
    ///
    /// It then replaces the spender with the most kernels (the lowest priority of those): a padded copy has more
    /// kernels than what it pads, so an honest transaction or aggregate is never replaced by a part of itself while a
    /// copy padding it remains, whatever fee the copy promises. A spender already chosen for one output frees every
    /// other output it spends. The caller checks that `tx` is not already in the pool (see
    /// [UnconfirmedPool::has_transaction]), and that replacing them does not orphan it (see
    /// [UnconfirmedPool::would_orphan]).
    pub fn conflict_set_victims(&self, tx: &Transaction) -> Option<Vec<TransactionKey>> {
        let mut kernel_hits = None;
        let mut victims = Vec::new();
        let mut chosen = HashSet::new();
        for input in tx.body.inputs() {
            let Some(spenders) = self.txs_by_spent_output.get(&input.output_hash()) else {
                continue;
            };
            if spenders.len() < MAX_SPENDS_PER_OUTPUT || spenders.iter().any(|key| chosen.contains(key)) {
                continue;
            }
            let (num_sigs, hits) = kernel_hits.get_or_insert_with(|| self.kernel_hits(tx)).as_ref()?;
            let mut victim: Option<&PrioritizedTransaction> = None;
            for key in spenders {
                let spender = self.tx_by_key.get(key)?;
                // Every kernel of `tx` is one of the spender's, and the spender has more
                if hits.get(key) != Some(num_sigs) || spender.kernel_signatures.len() <= *num_sigs {
                    return None;
                }
                let replaces = victim.is_none_or(|victim| {
                    spender
                        .kernel_signatures
                        .len()
                        .cmp(&victim.kernel_signatures.len())
                        .then_with(|| victim.priority.cmp(&spender.priority))
                        .is_gt()
                });
                if replaces {
                    victim = Some(spender);
                }
            }
            let victim = victim?;
            chosen.insert(victim.key);
            victims.push(victim.key);
        }
        Some(victims)
    }

    /// True if `removed` holds every pool producer of an output `tx` spends, so that removing them would leave `tx`
    /// without its parent. A producer list longer than `removed` is passed over without reading it.
    pub fn would_orphan(&self, tx: &Transaction, removed: &HashSet<TransactionKey>) -> bool {
        !removed.is_empty() &&
            tx.body.inputs().iter().any(|input| {
                self.txs_by_output.get(&input.output_hash()).is_some_and(|producers| {
                    producers.len() <= removed.len() && producers.iter().all(|key| removed.contains(key))
                })
            })
    }

    /// The number of kernels of `tx`, and for each pool transaction the number of them it has (the walk
    /// [UnconfirmedPool::has_transaction] does). None if `tx` repeats a kernel signature or has one that no pool
    /// transaction has: then no pool transaction has a strict superset of its kernels.
    fn kernel_hits(&self, tx: &Transaction) -> Option<(usize, HashMap<TransactionKey, usize>)> {
        let sigs = kernel_set(tx)?;
        let lists = sigs
            .iter()
            .map(|sig| self.txs_by_signature.get(sig))
            .collect::<Option<Vec<_>>>()?;
        let mut hits = HashMap::<TransactionKey, usize>::new();
        for keys in lists {
            // A transaction repeating this signature is listed once per repeat, next to each other (one insert pushes
            // them all), so it is counted once
            let mut previous = None;
            for key in keys {
                if previous != Some(key) {
                    let hit = hits.entry(*key).or_default();
                    *hit = hit.saturating_add(1);
                }
                previous = Some(key);
            }
        }
        Some((sigs.len(), hits))
    }

    #[cfg(test)]
    pub fn has_validator_node_conflict(&self, tx: &Transaction) -> bool {
        !self.has_transaction(tx) && !self.validator_node_conflict_holders(tx).is_empty()
    }

    /// The fee per gram (x1000, as used for the stored priority) of the pool transaction `key`
    pub fn fee_per_gram_millis(&self, key: TransactionKey) -> Option<u64> {
        self.tx_by_key.get(&key).map(|tx| tx.fee_per_byte)
    }

    /// Removes every pool transaction with a validator node registration or exit whose `max_epoch` is before `epoch`
    /// (the epoch the next block template is built for). Such a transaction can never be mined again, and block
    /// templates are not validated before mining, so leaving it would cost a block. Only transactions in the validator
    /// node index are considered, so this is cheap. Returns the removed transactions.
    pub fn remove_expired_validator_node_transactions(
        &mut self,
        epoch: VnEpoch,
    ) -> Result<Vec<Arc<Transaction>>, UnconfirmedPoolError> {
        let mut expired = self
            .txs_by_validator_node
            .values()
            .copied()
            .filter(|key| {
                self.tx_by_key
                    .get(key)
                    .and_then(|tx| validator_node_max_epoch(&tx.transaction.body))
                    .is_some_and(|max_epoch| max_epoch < epoch)
            })
            .collect::<Vec<_>>();
        expired.sort_unstable();
        expired.dedup();
        self.remove_transactions(expired)
    }

    /// Every transaction that registers, exits or cancels a validator node, with its key, so that the caller can
    /// re-check them against the new chain tip. Their validity depends on validator node state (activation and exit
    /// epochs, the registered set) that changes at epoch boundaries and with every validator node transaction mined,
    /// unlike ordinary transactions.
    pub fn validator_node_transactions(&self) -> Vec<(TransactionKey, Arc<Transaction>)> {
        let mut keys = self.txs_by_validator_node.values().copied().collect::<Vec<_>>();
        keys.sort_unstable();
        keys.dedup();
        keys.into_iter()
            .filter_map(|key| self.tx_by_key.get(&key).map(|tx| (key, tx.transaction.clone())))
            .collect()
    }

    /// The transactions of `selected` (as returned by [UnconfirmedPool::fetch_template_txs]) whose input
    /// scripts or covenants read the block height and that have not yet passed a re-check for a block on top of
    /// `prev_hash`, each with its key and the block it was validated for
    pub fn height_reading_to_check(
        &self,
        selected: &[Arc<Transaction>],
        prev_hash: &HashOutput,
    ) -> Vec<(TransactionKey, Arc<Transaction>, HeightValidation)> {
        let mut to_check = Vec::new();
        for tx in selected {
            let Some(kernel) = tx.body.kernels().first() else {
                continue;
            };
            for key in self
                .txs_by_signature
                .get(kernel.excess_sig.get_signature())
                .into_iter()
                .flatten()
            {
                let Some(entry) = self.txs_reading_height.get(key) else {
                    continue;
                };
                let is_selected = self.tx_by_key.get(key).is_some_and(|stored| stored.transaction == *tx);
                if is_selected && entry.passed_at.as_ref() != Some(prev_hash) {
                    to_check.push((*key, tx.clone(), entry.validation.clone()));
                }
            }
        }
        to_check
    }

    /// Records that the pool transaction `key` passed its re-check for a block on top of `prev_hash`, so that it is not
    /// checked again for that block
    pub fn record_height_check_passed(&mut self, key: TransactionKey, prev_hash: HashOutput) {
        if let Some(entry) = self.txs_reading_height.get_mut(&key) {
            entry.passed_at = Some(prev_hash);
        }
    }

    /// The transactions whose input scripts or covenants read the block height to re-check next for the block at
    /// `height` on top of `prev_hash` in the background, starting at the sweep cursor and wrapping around, so that
    /// successive batches visit every one of them. At most `max_entries` are visited, and those returned cost at most
    /// `max_work` to re-check in total (see [height_check_work]), except that the first one to re-check is always
    /// returned, so that every batch makes progress (its work is bounded by its block weight and covenant work limit).
    /// Transactions validated for
    /// a higher height (they may only become valid there) and those that already passed for this block are visited but
    /// not returned. Also returns the key to move the sweep cursor to (see [UnconfirmedPool::set_sweep_cursor]).
    pub fn height_reading_sweep_batch(
        &self,
        max_entries: usize,
        max_work: u64,
        height: u64,
        prev_hash: &HashOutput,
    ) -> (
        Vec<(TransactionKey, Arc<Transaction>, HeightValidation)>,
        TransactionKey,
    ) {
        let mut batch = Vec::new();
        let mut work = 0u64;
        let mut next_cursor = self.sweep_cursor;
        let visited = self
            .txs_reading_height
            .range(self.sweep_cursor..)
            .chain(self.txs_reading_height.range(..self.sweep_cursor))
            .take(max_entries);
        for (key, entry) in visited {
            let skip = entry.validation.height > height || entry.passed_at.as_ref() == Some(prev_hash);
            if !skip && !batch.is_empty() && work.saturating_add(entry.work) > max_work {
                // Left for the next batch
                break;
            }
            next_cursor = key.wrapping_add(1);
            if skip {
                continue;
            }
            if let Some(tx) = self.tx_by_key.get(key) {
                work = work.saturating_add(entry.work);
                batch.push((*key, tx.transaction.clone(), entry.validation.clone()));
            }
        }
        (batch, next_cursor)
    }

    /// The transactions whose input scripts or covenants read the height that `filter` would leave out of a template
    /// only because they have not passed a re-check for a block on top of `prev_hash`, highest priority first, costing
    /// at most `max_work` to re-check in total (see [height_check_work]; the first one is always included). This sorts
    /// the height-reading transactions, so it is linear-logarithmic in their number.
    pub fn height_reading_unchecked(
        &self,
        filter: &TemplateFilter,
        prev_hash: &HashOutput,
        max_work: u64,
    ) -> Vec<(TransactionKey, Arc<Transaction>, HeightValidation)> {
        let mut candidates = Vec::new();
        for (key, entry) in &self.txs_reading_height {
            if entry.passed_at.as_ref() == Some(prev_hash) || filter.excluded.contains(key) {
                continue;
            }
            if let Some(tx) = self.tx_by_key.get(key) &&
                tx.min_height <= filter.height
            {
                candidates.push((tx, entry));
            }
        }
        candidates.sort_by(|(a, _), (b, _)| b.priority.cmp(&a.priority));
        let mut batch = Vec::new();
        let mut work = 0u64;
        for (tx, entry) in candidates {
            if !batch.is_empty() && work.saturating_add(entry.work) > max_work {
                break;
            }
            work = work.saturating_add(entry.work);
            batch.push((tx.key, tx.transaction.clone(), entry.validation.clone()));
        }
        batch
    }

    /// Moves the sweep cursor (see [UnconfirmedPool::height_reading_sweep_batch])
    pub fn set_sweep_cursor(&mut self, cursor: TransactionKey) {
        self.sweep_cursor = cursor;
    }

    /// Removes the pool transactions `keys` and every pool transaction that spends one of their outputs, directly or
    /// through others: none of them can be valid without them. An output that another pool transaction also produces
    /// is still available, so its spenders are kept. Each is found through the spent-output index and removed once,
    /// and the index entries they share are pruned once at the end, so this is linear in the index entries touched.
    /// Returns the number removed.
    pub fn remove_transactions_and_descendants(
        &mut self,
        keys: impl IntoIterator<Item = TransactionKey>,
    ) -> Result<usize, UnconfirmedPoolError> {
        let mut to_remove = keys.into_iter().collect::<Vec<_>>();
        let mut removed = Vec::new();
        // The producers of each output still in the pool. The output index is only pruned at the end, so its entries
        // still list the producers removed so far.
        let mut live_producers = HashMap::<HashOutput, usize>::new();
        while let Some(key) = to_remove.pop() {
            let Some(tx) = self.take_transaction(key) else {
                continue;
            };
            for output in tx.transaction.body.outputs() {
                let hash = output.hash();
                let live = live_producers
                    .entry(hash)
                    .or_insert_with(|| self.txs_by_output.get(&hash).map_or(0, Vec::len));
                *live = live.saturating_sub(1);
                if *live > 0 {
                    continue;
                }
                // Every spender is removed, so the whole entry is taken here rather than pruned
                if let Some(spenders) = self.txs_by_spent_output.remove(&hash) {
                    to_remove.extend(spenders);
                }
            }
            removed.push(tx);
        }
        self.prune_indexes(&removed)?;
        Ok(removed.len())
    }

    /// Whether `filter` lets the pool transaction `key` into a block template
    fn template_allows(&self, filter: &TemplateFilter, key: TransactionKey, tx: &PrioritizedTransaction) -> bool {
        if tx.min_height > filter.height || filter.excluded.contains(&key) {
            return false;
        }
        match (&filter.only_checked_at, self.txs_reading_height.get(&key)) {
            (Some(prev_hash), Some(entry)) => entry.passed_at.as_ref() == Some(prev_hash),
            _ => true,
        }
    }

    /// Whether `body` registers, exits or cancels any validator node, i.e. whether mining it can change the validator
    /// node set
    pub fn body_touches_validator_nodes(body: &AggregateBody) -> bool {
        !validator_node_keys(body).is_empty()
    }

    /// This will search the unconfirmed pool for the set of outputs and return true if all of them are found
    pub fn contains_all_outputs(&self, outputs: &[HashOutput]) -> bool {
        outputs.iter().all(|hash| self.txs_by_output.contains_key(hash))
    }

    /// Insert a set of new transactions into the UnconfirmedPool
    #[cfg(test)]
    pub fn insert_many<I: IntoIterator<Item = Arc<Transaction>>>(
        &mut self,
        txs: I,
        transaction_weighting: &TransactionWeight,
    ) -> Result<(), UnconfirmedPoolError> {
        for tx in txs {
            self.insert(tx, None, transaction_weighting)?;
        }
        Ok(())
    }

    /// Check if a transaction is available in the UnconfirmedPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &CompressedSignature) -> bool {
        self.txs_by_signature.contains_key(excess_sig.get_signature())
    }

    /// True if `tx` counts as already in the pool: a pool transaction has exactly its kernel set, or `tx` is a union of
    /// pool transactions (every kernel of `tx` is in a pool transaction whose kernels are all in `tx`). Anyone can
    /// build such a union from other people's transactions at no cost, so it is not validated or relayed. Anything
    /// else, including a subset of a pool transaction, is validated: a subset is stored as its own producer of its
    /// outputs. One pass over the signature index lists of the kernels of `tx`.
    pub fn has_transaction(&self, tx: &Transaction) -> bool {
        let Some(set) = kernel_set(tx) else {
            return false;
        };
        let Some(lists) = set
            .iter()
            .map(|sig| self.txs_by_signature.get(sig))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        if self.tx_by_kernel_set.contains_key(&set) {
            return true;
        }
        // A pool transaction is a subset of `tx` if each of its kernels is one of the kernels of `tx`
        let mut hits = HashMap::<TransactionKey, usize>::new();
        for key in lists.iter().copied().flatten() {
            let hit = hits.entry(*key).or_default();
            *hit = hit.saturating_add(1);
        }
        let is_subset = |key: &TransactionKey| {
            self.tx_by_key
                .get(key)
                .is_some_and(|e| hits.get(key) == Some(&e.transaction.body.kernels().len()))
        };
        lists.iter().all(|keys| keys.iter().any(is_subset))
    }

    /// Returns the subset of provided output hashes that exist in the unconfirmed pool.
    pub fn filter_outputs(&self, output_hashes: &[HashOutput]) -> Vec<HashOutput> {
        output_hashes
            .iter()
            .filter(|hash| self.txs_by_output.contains_key(*hash))
            .copied()
            .collect()
    }

    /// As [UnconfirmedPool::fetch_template_txs], leaving nothing out
    #[cfg(test)]
    pub fn fetch_highest_priority_txs(
        &self,
        total_weight: u64,
        max_block_transaction_weight: u64,
    ) -> Result<RetrieveResults, UnconfirmedPoolError> {
        self.fetch_template_txs(
            total_weight,
            max_block_transaction_weight,
            &TemplateFilter::unrestricted(),
        )
    }

    /// Returns a set of the highest priority unconfirmed transactions, that can be included in a block. The selection
    /// is limited by `total_weight` (real weight) and by the template byte budget ([MAX_BLOCK_TEMPLATE_BODY_BYTES]).
    ///
    /// Selection is ordered by fee per gram of real weight. If the byte budget bound (some candidate was passed over
    /// only because its bytes did not fit), selection is run again ordered by fee per gram of effective weight (see
    /// [effective_weight]), and whichever selection earns the higher total fee is returned (the first on a tie). Under
    /// byte pressure a byte-heavy transaction must therefore pay for the share of the byte budget it consumes.
    /// `max_block_transaction_weight` is the consensus maximum, used only for that byte-to-weight ratio.
    ///
    /// A transaction that `filter` does not allow is left out, with every transaction that depends on it.
    pub fn fetch_template_txs(
        &self,
        total_weight: u64,
        max_block_transaction_weight: u64,
        filter: &TemplateFilter,
    ) -> Result<RetrieveResults, UnconfirmedPoolError> {
        Ok(self
            .fetch_selection(total_weight, self.max_body_bytes, max_block_transaction_weight, filter)?
            .results)
    }

    #[cfg(test)]
    fn fetch_highest_priority_txs_with_byte_budget(
        &self,
        total_weight: u64,
        max_body_bytes: usize,
        max_block_transaction_weight: u64,
    ) -> Result<RetrieveResults, UnconfirmedPoolError> {
        Ok(self
            .fetch_selection(
                total_weight,
                max_body_bytes,
                max_block_transaction_weight,
                &TemplateFilter::unrestricted(),
            )?
            .results)
    }

    /// Runs the selection described in [UnconfirmedPool::fetch_template_txs] and returns the chosen pass
    fn fetch_selection(
        &self,
        total_weight: u64,
        max_body_bytes: usize,
        max_block_transaction_weight: u64,
        filter: &TemplateFilter,
    ) -> Result<Selection, UnconfirmedPoolError> {
        let by_weight = self.select_txs_with(
            total_weight,
            max_body_bytes,
            Ranking::Weight,
            max_block_transaction_weight,
            HashSet::new(),
            filter,
        )?;
        if !by_weight.byte_bound {
            return Ok(by_weight);
        }
        let excluded = Self::second_pass_exclusions(&by_weight);
        let by_effective_weight = self.select_txs_with(
            total_weight,
            max_body_bytes,
            Ranking::EffectiveWeight {
                max_block_transaction_weight,
                max_body_bytes,
            },
            max_block_transaction_weight,
            excluded,
            filter,
        )?;
        let (mut chosen, other) = if by_effective_weight.total_fees > by_weight.total_fees {
            (by_effective_weight, by_weight)
        } else {
            (by_weight, by_effective_weight)
        };
        // Either pass may have found transactions whose inputs are gone; recheck all of them, once each
        let mut recheck_keys = chosen
            .results
            .transactions_to_remove_and_insert
            .iter()
            .map(|(key, _)| *key)
            .collect::<HashSet<_>>();
        for (key, tx) in other.results.transactions_to_remove_and_insert {
            if recheck_keys.insert(key) {
                chosen.results.transactions_to_remove_and_insert.push((key, tx));
            }
        }
        Ok(chosen)
    }

    /// The candidates to leave out of the second pass, given the first. If the first pass ended early, the candidates
    /// it dropped for intrinsic reasons are left out: no ordering can select them, so re-walking them would only spend
    /// the second pass's budget. Candidates it dropped for state-dependent reasons (a conflict with its choices, or not
    /// fitting what it had left) may fit under the second pass's different choices, so they are only left out when the
    /// first pass died of budget exhaustion: then re-walking the same junk would burn the second pass's budget too.
    fn second_pass_exclusions(first: &Selection) -> HashSet<TransactionKey> {
        if !first.ended_early {
            return HashSet::new();
        }
        let mut excluded = first.walked_dropped.clone();
        if first.budget_exhausted {
            excluded.extend(first.state_dropped.iter().copied());
        }
        excluded
    }

    /// The weight a transaction is ranked by for the given ordering
    fn rank_weight(transaction: &PrioritizedTransaction, ranking: Ranking) -> u64 {
        match ranking {
            Ranking::Weight => transaction.weight,
            Ranking::EffectiveWeight {
                max_block_transaction_weight,
                max_body_bytes,
            } => effective_weight(
                transaction.weight,
                transaction.body_size,
                max_block_transaction_weight,
                max_body_bytes,
            ),
        }
    }

    /// Selects the highest ranked transactions whose total real weight does not exceed `total_weight` and whose total
    /// serialized body size does not exceed `max_body_bytes`.
    ///
    /// A transaction (with its unselected dependencies) that would exceed the weight limit is skipped, counting towards
    /// `weight_tx_skip_count` once the template is full enough (see [BLOCK_FULL_ENOUGH_DIVISOR]). None of the following
    /// counts towards it or ends the pass, since none is evidence that the block is full:
    /// * a candidate that fits by weight but not by bytes: it stays in the pool for a later template;
    /// * a candidate that is too heavy while the template is not yet full enough;
    /// * a candidate (or branch) that spends an input already spent by a selected transaction: it can never be mined
    ///   alongside it;
    /// * a candidate with more than [MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE] unselected ancestors;
    /// * a candidate whose ancestry is no longer in the pool (it is rechecked instead).
    ///
    /// Otherwise a handful of large or conflicting transactions could use up the skip allowance and leave the rest of
    /// the template empty. The work of passing over candidates is bounded instead: ancestor walks by
    /// [MAX_WALKED_FRAMES_PER_PASS] and [MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE]. Conflict checks are O(1) set lookups
    /// per branch transaction: when a branch is selected, every other pool transaction spending one of its inputs is
    /// marked as conflicting (through the pool's spent-output index), so the work of marking is bounded by the inputs
    /// selected into the block, and each transaction is marked at most once per pass. When the walking budget runs out,
    /// the pass considers no further candidates but still selects from the branches it already queued, which needs no
    /// budget. The pass also ends once the remaining byte budget is smaller than
    /// [MIN_TRANSACTION_BODY_BYTES], or the pool is exhausted. A pass that ends early reports `byte_bound`, so that the
    /// effective-weight pass runs too.
    #[cfg(test)]
    fn select_txs(
        &self,
        total_weight: u64,
        max_body_bytes: usize,
        ranking: Ranking,
    ) -> Result<Selection, UnconfirmedPoolError> {
        let max_block_transaction_weight =
            crate::test_helpers::create_consensus_constants(0).max_block_transaction_weight();
        self.select_txs_with(
            total_weight,
            max_body_bytes,
            ranking,
            max_block_transaction_weight,
            HashSet::new(),
            &TemplateFilter::unrestricted(),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn select_txs_with(
        &self,
        total_weight: u64,
        max_body_bytes: usize,
        ranking: Ranking,
        max_block_transaction_weight: u64,
        excluded: HashSet<TransactionKey>,
        filter: &TemplateFilter,
    ) -> Result<Selection, UnconfirmedPoolError> {
        // The process of selection is as follows:
        // Assume that all transaction have the same weight for simplicity. A(20)->B(2) means A depends on B and A has
        // fee 20 and B has fee 2. A(20)->B(2)->C(14), D(12)
        // 1) A will be selected first, but B and C will be piggybacked on A, because overall fee_per_byte is 12, so we
        //   store it temporarily.
        // 2) We look at transaction C with fee per byte 14, it's good, nothing is better.
        // 3) We come back to transaction A with fee per byte 12, but now that C is already in, we recompute it's fee
        //   per byte to 11, and again we store it temporarily.
        // 4) Next we process transaction D, it's good, nothing is better.
        // 5) And now we proceed finally to transaction A, because there is no other possible better option.
        //
        // Note, if we store some TX_a that is dependent on some TXs including TX_b. And we remove TX_b (this should
        // trigger TX_a fee per byte recompute) before we process TX_a again, then the TX_a fee_per_byte will be lower
        // or equal, it will never be higher. Proof by contradiction we remove TX_b sooner then TX_a is process and
        // fee_per_byte(TX_a+dependents) > fee_per_byte(TX_a+dependents-TX_b), that would mean that
        // fee_per_byte(TX_b)<fee_per_byte(TX_a+dependents), but if this would be the case then we would not
        // process TX_b before TX_a.
        let mut state = SelectionState::new(
            total_weight,
            max_body_bytes,
            ranking,
            max_block_transaction_weight,
            excluded,
        );
        let order = self.selection_order(ranking)?;
        let mut exhausted = true;
        for tx_key in order {
            if state.selected_txs.contains_key(tx_key) || state.excluded.contains(tx_key) {
                continue;
            }
            if state.out_of_bytes() {
                // Nothing else can fit: the byte budget bound
                state.byte_bound = true;
                exhausted = false;
                break;
            }
            let prioritized_transaction = self
                .tx_by_key
                .get(tx_key)
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            self.check_the_potential_txs(&mut state, Self::fee_rate(prioritized_transaction, ranking)?)?;
            if state.weight_skips >= self.config.weight_tx_skip_count || state.stopped {
                exhausted = false;
                break;
            }
            // The branches added above may have included this transaction
            if state.selected_txs.contains_key(tx_key) {
                continue;
            }
            // Not allowed in this template (for this pass: the filter is the same for the next one)
            if !self.template_allows(filter, *tx_key, prioritized_transaction) {
                state.walked_dropped.insert(*tx_key);
                continue;
            }
            // Cheap checks before walking the transaction's ancestors. If it spends an input that is already spent, it
            // can never be mined with the selection: drop it without counting it.
            // If it cannot fit on its own, neither can its branch
            if prioritized_transaction.body_size > state.remaining_bytes() {
                state.byte_skip();
                state.state_dropped.insert(*tx_key);
                continue;
            }
            // If it spends an output that the selection already spends, it can never be mined with it: drop it without
            // counting it (O(1))
            if state.conflicting.contains(tx_key) {
                state.conflict_drop();
                state.state_dropped.insert(*tx_key);
                continue;
            }
            // If its dependencies are already known to be too heavy, so is its branch
            let weight_bound = match self.ancestor_weight_bound(
                prioritized_transaction,
                &state.selected_txs,
                &state.ancestor_weight_bounds,
                &mut state.links_left,
                filter,
            )? {
                BoundOutcome::Bound(bound) => bound,
                BoundOutcome::TooManyLinks => {
                    // Junk-shaped: drop it without counting it towards anything (as for too many ancestors)
                    state.too_many_ancestors_drops = state.too_many_ancestors_drops.saturating_add(1);
                    state.walked_dropped.insert(*tx_key);
                    continue;
                },
                BoundOutcome::OutOfLinks => {
                    // Out of walking budget (see the `OutOfFrames` case below)
                    state.frame_budget_exhausted = true;
                    state.out_of_frames_candidate = Some(*tx_key);
                    state.byte_bound = true;
                    exhausted = false;
                    break;
                },
            };
            if state.curr_weight.saturating_add(weight_bound) > total_weight {
                if state.weight_skip(self.config.weight_tx_skip_count) {
                    exhausted = false;
                    break;
                }
                continue;
            }
            let mut total_transaction_weight = 0;
            let mut total_transaction_rank_weight = 0;
            let mut total_transaction_fees = 0;
            let mut candidate_transactions_to_select = HashMap::new();
            let mut potential_transactions_to_remove_and_recheck = Vec::new();
            let mut candidate_links = 0usize;
            let outcome = self.get_all_dependent_transactions(
                prioritized_transaction,
                &mut candidate_transactions_to_select,
                &mut potential_transactions_to_remove_and_recheck,
                &state.selected_txs,
                ranking,
                &mut total_transaction_weight,
                &mut total_transaction_rank_weight,
                &mut total_transaction_fees,
                &mut state.frames_left,
                &mut state.links_left,
                &mut candidate_links,
                &mut state.ancestor_weight_bounds,
                filter,
            )?;
            match outcome {
                WalkOutcome::Complete => {
                    // An ancestor that is not allowed in this template leaves the branch out too
                    let allowed = candidate_transactions_to_select.keys().all(|key| {
                        self.tx_by_key
                            .get(key)
                            .is_some_and(|tx| self.template_allows(filter, *key, tx))
                    });
                    if !allowed {
                        state.walked_dropped.insert(*tx_key);
                        continue;
                    }
                },
                WalkOutcome::TooManyAncestors => {
                    // Junk-shaped: drop it without counting it towards anything
                    state.too_many_ancestors_drops = state.too_many_ancestors_drops.saturating_add(1);
                    state.walked_dropped.insert(*tx_key);
                    continue;
                },
                WalkOutcome::OutOfFrames => {
                    // Out of walking budget: consider no further candidates, but still select from the queued
                    // branches (below). The byte budget bound, as far as this pass can tell. This candidate is not
                    // excluded from the next pass: nothing about it was found wanting.
                    state.frame_budget_exhausted = true;
                    state.out_of_frames_candidate = Some(*tx_key);
                    state.byte_bound = true;
                    exhausted = false;
                    break;
                },
            }
            let total_weight_after_candidates =
                state
                    .curr_weight
                    .checked_add(total_transaction_weight)
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating transaction weights".to_string(),
                    ))?;
            let body_bytes_after_candidates = state
                .curr_body_bytes
                .checked_add(self.body_size_of(&candidate_transactions_to_select)?)
                .ok_or(UnconfirmedPoolError::InternalError(
                    "Overflow when calculating transaction body sizes".to_string(),
                ))?;
            let fits_weight = total_weight_after_candidates <= total_weight;
            let fits_bytes = body_bytes_after_candidates <= max_body_bytes;
            let needs_recheck = !potential_transactions_to_remove_and_recheck.is_empty();
            // Only a branch that would otherwise be queued is checked for conflicts
            let conflicts = !needs_recheck &&
                fits_weight &&
                fits_bytes &&
                state.branch_conflicts(&candidate_transactions_to_select);
            if !conflicts &&
                !needs_recheck &&
                fits_weight &&
                fits_bytes &&
                state.branch_has_remembered_pair(&candidate_transactions_to_select)
            {
                // Contains both members of a pair already found to collide: invalid, without checking again
                state.pair_drops = state.pair_drops.saturating_add(1);
                state.walked_dropped.insert(*tx_key);
                continue;
            }
            let invalid = if !conflicts && !needs_recheck && fits_weight && fits_bytes {
                match self.check_branch_duplicates(
                    &candidate_transactions_to_select,
                    candidate_links,
                    &mut state.links_left,
                )? {
                    DuplicateCheck::Clean => false,
                    DuplicateCheck::Invalid { self_invalid, pairs } => {
                        state.remember_invalid(self_invalid, pairs);
                        true
                    },
                    DuplicateCheck::TooManyLinks => {
                        // Junk-shaped: drop it without counting it towards anything (as for too many ancestors)
                        state.too_many_ancestors_drops = state.too_many_ancestors_drops.saturating_add(1);
                        state.walked_dropped.insert(*tx_key);
                        continue;
                    },
                }
            } else {
                false
            };
            if conflicts {
                // An ancestor spends an input that is already spent: the branch can never be mined with the selection
                state.conflict_drop();
                state.state_dropped.insert(*tx_key);
            } else if invalid {
                // Two of its own transactions double-spend or duplicate an output: never valid in a block
                state.invalid_branch_drops = state.invalid_branch_drops.saturating_add(1);
                state.walked_dropped.insert(*tx_key);
            } else if !needs_recheck && fits_weight && fits_bytes {
                for dependend_on_tx_key in candidate_transactions_to_select.keys() {
                    if dependend_on_tx_key != tx_key {
                        // Transaction is not depended on itself.
                        state
                            .depended_on
                            .entry(*dependend_on_tx_key)
                            .and_modify(|v| v.push(tx_key))
                            .or_insert_with(|| vec![tx_key]);
                    }
                }
                // Branches are ordered by fee per ranking weight; the block is filled by real weight
                let fee_per_byte = total_transaction_fees
                    .saturating_mul(1000)
                    .checked_div(total_transaction_rank_weight)
                    .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                state.complete_transaction_branch.insert(
                    *tx_key,
                    (
                        candidate_transactions_to_select,
                        total_transaction_weight,
                        total_transaction_rank_weight,
                        total_transaction_fees,
                    ),
                );
                state.potentional_to_add.push((fee_per_byte, *tx_key));
            } else if !needs_recheck && fits_weight {
                // Only the bytes do not fit
                state.byte_skip();
                state.state_dropped.insert(*tx_key);
            } else {
                let stop = if needs_recheck {
                    // A dependency is no longer in the pool: the branch is rechecked (removed and re-validated) after
                    // this template. Like a conflict, that is no evidence that the block is full, so it is not counted;
                    // the walk stopped at the first missing dependency, so it was cheap.
                    state
                        .transactions_to_remove_and_recheck
                        .append(&mut potential_transactions_to_remove_and_recheck);
                    false
                } else {
                    // Check if some the next few txs with slightly lower priority wont fit in the remaining space.
                    state.state_dropped.insert(*tx_key);
                    state.weight_skip(self.config.weight_tx_skip_count)
                };
                if stop {
                    exhausted = false;
                    break;
                }
            }
        }
        // Select from the branches already queued, however the pass ended, unless there is no room left at all
        if !state.stopped {
            self.check_the_potential_txs(&mut state, 0)?;
        }
        if !exhausted && state.out_of_bytes() {
            state.byte_bound = true;
        }
        // Defence in depth: the selection must be a valid set of transactions for one block
        let (keep, swept) = self.sweep_selection(&state.selected_branches)?;
        if swept > 0 {
            state.selected_txs.retain(|key, _| keep.contains(key));
        }

        let total_fees = state.selected_txs.values().try_fold(0u64, |total, tx| {
            total
                .checked_add(tx.body.get_total_fee()?.as_u64())
                .ok_or(UnconfirmedPoolError::InternalError(
                    "Overflow when calculating total fees".to_string(),
                ))
        })?;
        Ok(Selection {
            results: RetrieveResults {
                retrieved_transactions: state.selected_txs.into_values().collect(),
                transactions_to_remove_and_insert: state.transactions_to_remove_and_recheck,
            },
            total_fees,
            byte_bound: state.byte_bound,
            byte_skips: state.byte_skips,
            conflict_drops: state.conflict_drops,
            early_weight_skips: state.early_weight_skips,
            too_many_ancestors_drops: state.too_many_ancestors_drops,
            weight_skips_counted: state.weight_skips,
            out_of_frames_candidate: state.out_of_frames_candidate,
            walk_work: MAX_WALKED_FRAMES_PER_PASS.saturating_sub(state.frames_left),
            links_followed: MAX_FOLLOWED_LINKS_PER_PASS.saturating_sub(state.links_left),
            ended_early: !exhausted,
            walked_dropped: state.walked_dropped,
            state_dropped: state.state_dropped,
            budget_exhausted: state.frame_budget_exhausted,
            invalid_branch_drops: state.invalid_branch_drops,
            pair_drops: state.pair_drops,
            pair_lookup_work: state.pair_lookup_work,
            swept_branches: swept,
        })
    }

    /// Marks as conflicting with the selection every other pool transaction that spends an output one of `branch`
    /// spends, produces an output one of `branch` produces, produces an output with the same commitment as one of
    /// `branch`'s, or has a kernel with the same excess signature or excess as one of `branch`'s: none of them can be
    /// in the same block. Bounded by the inputs and outputs of the selected branch
    /// times the transactions sharing each; each transaction is marked at most once per pass.
    fn mark_conflicts(
        &self,
        branch: &HashMap<TransactionKey, Arc<Transaction>>,
        state: &mut SelectionState<'_>,
    ) -> Result<(), UnconfirmedPoolError> {
        for key in branch.keys() {
            let tx = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            let others = tx
                .input_hashes
                .iter()
                .filter_map(|hash| self.txs_by_spent_output.get(hash))
                .chain(tx.output_hashes.iter().filter_map(|hash| self.txs_by_output.get(hash)))
                .chain(
                    tx.output_commitments
                        .iter()
                        .filter_map(|commitment| self.txs_by_output_commitment.get(commitment)),
                )
                .chain(
                    tx.kernel_signatures
                        .iter()
                        .filter_map(|signature| self.txs_by_signature.get(signature)),
                )
                .chain(
                    tx.kernel_excesses
                        .iter()
                        .filter_map(|excess| self.txs_by_kernel_excess.get(excess)),
                );
            for keys in others {
                for other in keys {
                    if !branch.contains_key(other) && !state.selected_txs.contains_key(other) {
                        state.conflicting.insert(*other);
                    }
                }
            }
        }
        Ok(())
    }

    /// Checks whether `branch` can never be in a valid block: a transaction in it repeats an input, output,
    /// commitment, kernel signature or kernel excess on its own (a flag computed on insert, so O(1) per transaction),
    /// or two of its transactions spend the same output, produce the same output, produce outputs with the same
    /// commitment, or have kernels with the same signature or excess. The cross-transaction check of a
    /// multi-transaction branch costs one unit per input, output and kernel of the branch; together with the
    /// `candidate_links` its walk already followed, it may not exceed [MAX_LINKS_PER_CANDIDATE], and it is charged
    /// to `links_left` (saturating: the next walk notices when it has run out). The branch's transactions are
    /// examined highest priority first, so each colliding pair is reported with its lower-priority member first.
    fn check_branch_duplicates(
        &self,
        branch: &HashMap<TransactionKey, Arc<Transaction>>,
        candidate_links: usize,
        links_left: &mut usize,
    ) -> Result<DuplicateCheck, UnconfirmedPoolError> {
        let mut txs = branch
            .keys()
            .map(|key| self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync))
            .collect::<Result<Vec<_>, _>>()?;
        let self_invalid = txs
            .iter()
            .filter(|tx| tx.has_internal_duplicates)
            .map(|tx| tx.key)
            .collect::<Vec<_>>();
        if txs.len() <= 1 {
            return Ok(if self_invalid.is_empty() {
                DuplicateCheck::Clean
            } else {
                DuplicateCheck::Invalid {
                    self_invalid,
                    pairs: Vec::new(),
                }
            });
        }
        let work = txs.iter().fold(0usize, |work, tx| {
            work.saturating_add(tx.input_hashes.len())
                .saturating_add(tx.output_hashes.len())
                .saturating_add(tx.kernel_excesses.len())
        });
        if candidate_links.saturating_add(work) > MAX_LINKS_PER_CANDIDATE {
            return Ok(DuplicateCheck::TooManyLinks);
        }
        *links_left = links_left.saturating_sub(work);
        txs.sort_by(|a, b| b.priority.cmp(&a.priority));
        // Each item seen so far, with the transaction it belongs to
        let mut inputs = HashMap::new();
        let mut outputs = HashMap::new();
        let mut commitments = HashMap::new();
        let mut signatures = HashMap::new();
        let mut excesses = HashMap::new();
        let mut pairs = Vec::new();
        for tx in txs {
            if tx.has_internal_duplicates {
                continue;
            }
            let partner = tx
                .input_hashes
                .iter()
                .find_map(|hash| inputs.get(hash))
                .or_else(|| tx.output_hashes.iter().find_map(|hash| outputs.get(hash)))
                .or_else(|| {
                    tx.output_commitments
                        .iter()
                        .find_map(|commitment| commitments.get(commitment))
                })
                .or_else(|| {
                    tx.kernel_signatures
                        .iter()
                        .find_map(|signature| signatures.get(signature))
                })
                .or_else(|| tx.kernel_excesses.iter().find_map(|excess| excesses.get(excess)))
                .copied();
            if let Some(partner) = partner {
                pairs.push((tx.key, partner));
            } else {
                inputs.extend(tx.input_hashes.iter().map(|hash| (*hash, tx.key)));
                outputs.extend(tx.output_hashes.iter().map(|hash| (*hash, tx.key)));
                commitments.extend(tx.output_commitments.iter().map(|commitment| (*commitment, tx.key)));
                signatures.extend(tx.kernel_signatures.iter().map(|signature| (signature.clone(), tx.key)));
                excesses.extend(tx.kernel_excesses.iter().map(|excess| (*excess, tx.key)));
            }
        }
        if self_invalid.is_empty() && pairs.is_empty() {
            Ok(DuplicateCheck::Clean)
        } else {
            Ok(DuplicateCheck::Invalid { self_invalid, pairs })
        }
    }

    /// A final check of the whole selection, in the order its branches were selected, for any two transactions that
    /// spend the same output, produce the same output, produce outputs with the same commitment, have kernels with
    /// the same excess signature or excess, or register/exit the same validator node. The per-branch
    /// checks should make this find nothing; if it does find something, it fails closed: the later offending branch
    /// is dropped (and with it every later branch that depends on a dropped transaction), leaving a valid subset.
    /// Bounded by the selection (i.e. by the block). Returns the transactions to keep and the number of branches
    /// dropped.
    fn sweep_selection(
        &self,
        branches: &[Vec<TransactionKey>],
    ) -> Result<(HashSet<TransactionKey>, usize), UnconfirmedPoolError> {
        let mut keep = HashSet::new();
        let mut inputs = HashSet::new();
        let mut outputs = HashSet::new();
        let mut commitments = HashSet::new();
        let mut signatures = HashSet::new();
        let mut excesses = HashSet::new();
        let mut vn_keys = HashSet::new();
        let mut dropped_outputs = HashSet::new();
        let mut dropped = 0usize;
        for branch in branches {
            let mut branch_inputs = HashSet::new();
            let mut branch_outputs = HashSet::new();
            let mut branch_commitments = HashSet::new();
            let mut branch_signatures = HashSet::new();
            let mut branch_excesses = HashSet::new();
            let mut branch_vn_keys = HashSet::new();
            let mut valid = true;
            for key in branch {
                let tx = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                valid &= !tx
                    .dependent_output_hashes
                    .iter()
                    .any(|hash| dropped_outputs.contains(hash));
                valid &= tx
                    .input_hashes
                    .iter()
                    .all(|hash| !inputs.contains(hash) && branch_inputs.insert(*hash));
                valid &= tx
                    .output_hashes
                    .iter()
                    .all(|hash| !outputs.contains(hash) && branch_outputs.insert(*hash));
                valid &= tx
                    .output_commitments
                    .iter()
                    .all(|commitment| !commitments.contains(commitment) && branch_commitments.insert(*commitment));
                valid &= tx
                    .kernel_signatures
                    .iter()
                    .all(|signature| !signatures.contains(signature) && branch_signatures.insert(signature.clone()));
                valid &= tx
                    .kernel_excesses
                    .iter()
                    .all(|excess| !excesses.contains(excess) && branch_excesses.insert(*excess));
                // Insertion rejects a second pool transaction for the same validator node
                // (`validator_node_conflict_holders`), so this should never fire; it keeps a template valid regardless.
                valid &= validator_node_keys(&tx.transaction.body)
                    .into_iter()
                    .all(|vn_key| !vn_keys.contains(&vn_key) && branch_vn_keys.insert(vn_key));
            }
            if valid {
                inputs.extend(branch_inputs);
                outputs.extend(branch_outputs);
                commitments.extend(branch_commitments);
                signatures.extend(branch_signatures);
                excesses.extend(branch_excesses);
                vn_keys.extend(branch_vn_keys);
                keep.extend(branch.iter().copied());
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Block template selection produced a branch of {} transaction(s) that double-spends, or duplicates \
                     an output of, the rest of the selection; dropping it",
                    branch.len()
                );
                dropped = dropped.saturating_add(1);
                for key in branch {
                    let tx = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                    dropped_outputs.extend(tx.output_hashes.iter().copied());
                }
            }
        }
        Ok((keep, dropped))
    }

    /// The transaction keys in the order they are considered for selection: by stored priority (fee per gram of real
    /// weight), or, under byte pressure, by fee per gram of effective weight (ties broken by stored priority).
    fn selection_order(&self, ranking: Ranking) -> Result<Vec<&TransactionKey>, UnconfirmedPoolError> {
        let order = self.tx_by_priority.values().rev().collect::<Vec<_>>();
        if let Ranking::Weight = ranking {
            return Ok(order);
        }
        let mut ranked = order
            .into_iter()
            .map(|key| {
                let tx = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                Ok((Self::fee_rate(tx, ranking)?, key))
            })
            .collect::<Result<Vec<_>, UnconfirmedPoolError>>()?;
        // A stable sort, so equal rates keep their priority order
        ranked.sort_by(|(a, _), (b, _)| b.cmp(a));
        Ok(ranked.into_iter().map(|(_, key)| key).collect())
    }

    /// A transaction's fee per 1000 grams of its ranking weight
    fn fee_rate(transaction: &PrioritizedTransaction, ranking: Ranking) -> Result<u64, UnconfirmedPoolError> {
        match ranking {
            Ranking::Weight => Ok(transaction.fee_per_byte),
            Ranking::EffectiveWeight { .. } => transaction
                .transaction
                .body
                .get_total_fee()?
                .as_u64()
                .saturating_mul(1000)
                .checked_div(Self::rank_weight(transaction, ranking))
                .ok_or(UnconfirmedPoolError::StorageOutofSync),
        }
    }

    /// Returns the total serialized body size of the given transactions.
    fn body_size_of(
        &self,
        transactions: &HashMap<TransactionKey, Arc<Transaction>>,
    ) -> Result<usize, UnconfirmedPoolError> {
        transactions.keys().try_fold(0usize, |total, key| {
            let tx = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            total
                .checked_add(tx.body_size)
                .ok_or(UnconfirmedPoolError::InternalError(
                    "Overflow when calculating transaction body sizes".to_string(),
                ))
        })
    }

    #[allow(clippy::too_many_lines)]
    fn check_the_potential_txs<'a>(
        &'a self,
        state: &mut SelectionState<'a>,
        fee_per_byte_threshold: u64,
    ) -> Result<(), UnconfirmedPoolError> {
        while match state.potentional_to_add.peek() {
            Some((fee_per_byte, _)) => *fee_per_byte >= fee_per_byte_threshold,
            None => false,
        } {
            if state.out_of_bytes() {
                // Nothing else can fit: the byte budget bound
                state.byte_bound = true;
                state.stopped = true;
                break;
            }
            // If the current TXs has lower fee than the ones we already processed, we can add some.
            let (_fee_per_byte, tx_key) = state
                .potentional_to_add
                .pop()
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            if state.selected_txs.contains_key(&tx_key) {
                continue;
            }
            // Before we do anything with the top transaction we need to know if needs to be recomputed.
            if state.recompute.contains(&tx_key) {
                state.recompute.remove(&tx_key);
                // So we recompute the total fees based on updated weights and fees.
                let (_, _, total_transaction_rank_weight, total_transaction_fees) = state
                    .complete_transaction_branch
                    .get(&tx_key)
                    .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                let fee_per_byte = total_transaction_fees
                    .saturating_mul(1000)
                    .checked_div(*total_transaction_rank_weight)
                    .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                state.potentional_to_add.push((fee_per_byte, tx_key));
                continue;
            }
            let (candidate_transactions_to_select, total_transaction_weight, _, _total_transaction_fees) = state
                .complete_transaction_branch
                .remove(&tx_key)
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;

            let total_weight_after_candidates =
                state
                    .curr_weight
                    .checked_add(total_transaction_weight)
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating total weights".to_string(),
                    ))?;
            // Transactions already selected have been removed from `candidate_transactions_to_select` (see
            // `remove_transaction_from_the_dependants`), so this counts only the bytes this branch adds.
            let candidate_body_bytes = self.body_size_of(&candidate_transactions_to_select)?;
            let body_bytes_after_candidates =
                state
                    .curr_body_bytes
                    .checked_add(candidate_body_bytes)
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating transaction body sizes".to_string(),
                    ))?;
            let fits_weight = total_weight_after_candidates <= state.total_weight;
            let fits_bytes = body_bytes_after_candidates <= state.max_body_bytes;
            // Only a branch that would otherwise be selected is checked for conflicts (O(1) per transaction; never
            // limited by a budget, so the final drain always completes)
            let conflicts = fits_weight && fits_bytes && state.branch_conflicts(&candidate_transactions_to_select);
            // Defence in depth: the branch was checked when it was queued, and queued branches only shrink
            let invalid = !conflicts &&
                fits_weight &&
                fits_bytes &&
                (state.branch_has_remembered_pair(&candidate_transactions_to_select) ||
                    match self.check_branch_duplicates(&candidate_transactions_to_select, 0, &mut state.links_left)? {
                        DuplicateCheck::Clean => false,
                        DuplicateCheck::Invalid { self_invalid, pairs } => {
                            state.remember_invalid(self_invalid, pairs);
                            true
                        },
                        DuplicateCheck::TooManyLinks => true,
                    });
            if conflicts {
                // Spends an input that a selected transaction already spends: it can never be mined alongside it.
                // Not counted towards any limit (its walk was paid once, when it was queued).
                state.conflict_drop();
                state.state_dropped.insert(tx_key);
            } else if invalid {
                state.invalid_branch_drops = state.invalid_branch_drops.saturating_add(1);
                state.walked_dropped.insert(tx_key);
            } else if fits_weight && fits_bytes {
                state.curr_weight = state.curr_weight.checked_add(total_transaction_weight).ok_or(
                    UnconfirmedPoolError::InternalError("Overflow when calculating total weights".to_string()),
                )?;
                state.curr_body_bytes = body_bytes_after_candidates;
                // So we processed the transaction, let's mark the dependents to be recomputed.
                for tx_key in candidate_transactions_to_select.keys() {
                    self.remove_transaction_from_the_dependants(
                        *tx_key,
                        state.ranking,
                        &mut state.complete_transaction_branch,
                        &mut state.depended_on,
                        &mut state.recompute,
                    )?;
                }
                self.mark_conflicts(&candidate_transactions_to_select, state)?;
                state
                    .selected_branches
                    .push(candidate_transactions_to_select.keys().copied().collect());
                state.selected_txs.extend(candidate_transactions_to_select);
                // Selecting transactions lowers the weight of their descendants' unselected ancestry
                state.ancestor_weight_bounds.clear();
            } else if fits_weight {
                // Only the bytes do not fit
                state.byte_skip();
                state.state_dropped.insert(tx_key);
            } else {
                state.state_dropped.insert(tx_key);
                let stop = state.weight_skip(self.config.weight_tx_skip_count);
                // The final drain (threshold 0) selects every queued branch that still fits, whatever was skipped
                if stop && fee_per_byte_threshold > 0 {
                    break;
                }
            }
            // Some cleanup of what we don't need anymore
            state.complete_transaction_branch.remove(&tx_key);
            state.depended_on.remove(&tx_key);
        }
        Ok(())
    }

    fn remove_transaction_from_the_dependants<'a>(
        &self,
        tx_key: TransactionKey,
        ranking: Ranking,
        complete_transaction_branch: &mut CompleteTransactionBranch,
        depended_on: &mut HashMap<TransactionKey, Vec<&'a TransactionKey>>,
        recompute: &mut HashSet<&'a TransactionKey>,
    ) -> Result<(), UnconfirmedPoolError> {
        if let Some(txs) = depended_on.remove(&tx_key) {
            let prioritized_transaction = self
                .tx_by_key
                .get(&tx_key)
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            for tx in txs {
                if let Some((
                    update_candidate_transactions_to_select,
                    update_total_transaction_weight,
                    update_total_transaction_rank_weight,
                    update_total_transaction_fees,
                )) = complete_transaction_branch.get_mut(tx)
                {
                    update_candidate_transactions_to_select.remove(&tx_key);
                    *update_total_transaction_weight = update_total_transaction_weight
                        .checked_sub(prioritized_transaction.weight)
                        .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                    *update_total_transaction_rank_weight = update_total_transaction_rank_weight
                        .checked_sub(Self::rank_weight(prioritized_transaction, ranking))
                        .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                    *update_total_transaction_fees = update_total_transaction_fees
                        .checked_sub(prioritized_transaction.transaction.body.get_total_fee()?.0)
                        .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                    // We mark it as recompute, we don't have to update the Heap, because it will never be
                    // better as it was (see the note at the top of the function).
                    recompute.insert(tx);
                }
            }
        }
        Ok(())
    }

    pub fn retrieve_by_excess_sigs(
        &self,
        excess_sigs: &[PrivateKey],
    ) -> Result<(Vec<Arc<Transaction>>, Vec<PrivateKey>), MempoolError> {
        // Hashset used to prevent duplicates
        let mut found = HashSet::new();
        let mut remaining = Vec::new();

        for sig in excess_sigs {
            match self.txs_by_signature.get(sig).cloned() {
                Some(ids) => found.extend(ids),
                None => remaining.push(sig.clone()),
            }
        }

        let found = found
            .into_iter()
            .map(|id| {
                self.tx_by_key
                    .get(&id)
                    .map(|tx| tx.transaction.clone())
                    .ok_or(MempoolError::IndexOutOfSync)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok((found, remaining))
    }

    /// Collects `transaction` and every not-yet-selected pool transaction it depends on (transitively) into
    /// `required_transactions`, adding their weights and fees to the totals. If a dependency is no longer in the pool,
    /// the transactions on the path to it are added to `transactions_to_recheck` (and the branch cannot be selected).
    ///
    /// This walks the dependency graph iteratively (a post-order depth-first walk with an explicit stack), since the
    /// pool puts no limit on how deep a chain of unconfirmed transactions can be: recursion could overflow the stack
    /// of the blocking thread it runs on. Each transaction is visited at most once per call.
    fn get_all_dependent_transactions(
        &self,
        transaction: &PrioritizedTransaction,
        required_transactions: &mut HashMap<TransactionKey, Arc<Transaction>>,
        transactions_to_recheck: &mut Vec<(TransactionKey, Arc<Transaction>)>,
        selected_txs: &HashMap<TransactionKey, Arc<Transaction>>,
        ranking: Ranking,
        total_weight: &mut u64,
        total_rank_weight: &mut u64,
        total_fees: &mut u64,
        frames_left: &mut usize,
        links_left: &mut usize,
        candidate_links: &mut usize,
        ancestor_weight_bounds: &mut HashMap<TransactionKey, u64>,
        filter: &TemplateFilter,
    ) -> Result<WalkOutcome, UnconfirmedPoolError> {
        struct Frame<'b> {
            transaction: &'b PrioritizedTransaction,
            next_dependency: usize,
            rechecked: bool,
            /// The largest ancestor weight bound among this transaction's unselected dependencies so far
            dependency_bound: u64,
        }
        if *frames_left == 0 {
            return Ok(WalkOutcome::OutOfFrames);
        }
        *frames_left = frames_left.saturating_sub(1);
        let mut visited = HashSet::new();
        visited.insert(transaction.key);
        let mut stack = vec![Frame {
            transaction,
            next_dependency: 0,
            rechecked: false,
            dependency_bound: 0,
        }];
        while let Some(frame) = stack.last_mut() {
            let current = frame.transaction;
            // Descend into the next dependency, unless a missing one was found (the whole path is then rechecked)
            if transactions_to_recheck.is_empty() &&
                let Some(dependent_output) = current.dependent_output_hashes.get(frame.next_dependency)
            {
                // Every link followed counts towards the candidate's cap. It is charged to the pass's link budget too,
                // except for the candidate's own links, which its pre-walk ancestor weight bound already paid for.
                *candidate_links = candidate_links.saturating_add(1);
                if *candidate_links > MAX_LINKS_PER_CANDIDATE {
                    return Ok(WalkOutcome::TooManyAncestors);
                }
                if current.key != transaction.key {
                    if *links_left == 0 {
                        return Ok(WalkOutcome::OutOfFrames);
                    }
                    *links_left = links_left.saturating_sub(1);
                }
                frame.next_dependency = frame.next_dependency.saturating_add(1);
                match self.txs_by_output.get(dependent_output) {
                    Some(keys) => {
                        let (dependency, extra_producers) = self.find_template_producer(keys, filter, links_left)?;
                        *candidate_links = candidate_links.saturating_add(extra_producers);
                        if *candidate_links > MAX_LINKS_PER_CANDIDATE {
                            return Ok(WalkOutcome::TooManyAncestors);
                        }
                        if selected_txs.contains_key(&dependency.key) {
                            // Already in the block: adds nothing
                        } else if !visited.insert(dependency.key) {
                            // Already walked in this walk (a DAG, so it has finished)
                            if let Some(bound) = ancestor_weight_bounds.get(&dependency.key) {
                                frame.dependency_bound = frame.dependency_bound.max(*bound);
                            }
                        } else {
                            if *frames_left == 0 {
                                return Ok(WalkOutcome::OutOfFrames);
                            }
                            // `visited` includes the candidate itself
                            if visited.len() > MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE.saturating_add(1) {
                                return Ok(WalkOutcome::TooManyAncestors);
                            }
                            *frames_left = frames_left.saturating_sub(1);
                            stack.push(Frame {
                                transaction: dependency,
                                next_dependency: 0,
                                rechecked: false,
                                dependency_bound: 0,
                            });
                        }
                    },
                    None => {
                        // this transactions requires an output, that the mempool does not currently have, but did
                        // have at some point. This means that we need to remove this transaction and revalidate it
                        transactions_to_recheck.push((current.key, current.transaction.clone()));
                        frame.rechecked = true;
                    },
                }
                continue;
            }
            // All of this transaction's dependencies are done (or a missing one was found): finish it
            if !transactions_to_recheck.is_empty() && !frame.rechecked {
                transactions_to_recheck.push((current.key, current.transaction.clone()));
            }
            let bound = current.weight.saturating_add(frame.dependency_bound);
            stack.pop();
            ancestor_weight_bounds.insert(current.key, bound);
            if let Some(parent) = stack.last_mut() {
                parent.dependency_bound = parent.dependency_bound.max(bound);
            }
            if required_transactions
                .insert(current.key, current.transaction.clone())
                .is_none()
            {
                *total_fees = total_fees
                    .checked_add(current.transaction.body.get_total_fee()?.0)
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating total fees".to_string(),
                    ))?;
                *total_weight = total_weight
                    .checked_add(current.weight)
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating total weights".to_string(),
                    ))?;
                *total_rank_weight = total_rank_weight
                    .checked_add(Self::rank_weight(current, ranking))
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating total ranking weights".to_string(),
                    ))?;
            }
        }

        Ok(WalkOutcome::Complete)
    }

    /// A lower bound on the weight of `transaction` together with its unselected ancestors: its own weight plus the
    /// largest known bound of its unselected dependencies (0 for any not yet walked). Each dependency link examined
    /// is charged to `links_left`.
    fn ancestor_weight_bound(
        &self,
        transaction: &PrioritizedTransaction,
        selected_txs: &HashMap<TransactionKey, Arc<Transaction>>,
        ancestor_weight_bounds: &HashMap<TransactionKey, u64>,
        links_left: &mut usize,
        filter: &TemplateFilter,
    ) -> Result<BoundOutcome, UnconfirmedPoolError> {
        let mut links = transaction.dependent_output_hashes.len();
        if links > MAX_LINKS_PER_CANDIDATE {
            return Ok(BoundOutcome::TooManyLinks);
        }
        let mut dependencies = 0u64;
        for dependent_output in &transaction.dependent_output_hashes {
            if *links_left == 0 {
                return Ok(BoundOutcome::OutOfLinks);
            }
            *links_left = links_left.saturating_sub(1);
            if let Some(keys) = self.txs_by_output.get(dependent_output) {
                let (dependency, extra_producers) = self.find_template_producer(keys, filter, links_left)?;
                links = links.saturating_add(extra_producers);
                if links > MAX_LINKS_PER_CANDIDATE {
                    return Ok(BoundOutcome::TooManyLinks);
                }
                if !selected_txs.contains_key(&dependency.key) &&
                    let Some(bound) = ancestor_weight_bounds.get(&dependency.key)
                {
                    dependencies = dependencies.max(*bound);
                }
            }
        }
        Ok(BoundOutcome::Bound(transaction.weight.saturating_add(dependencies)))
    }

    /// The highest priority pool transaction producing an output (from `keys`, its `txs_by_output` entry) that `filter`
    /// lets into the template, so that a child is not left out because its preferred parent is, with the number of
    /// producers examined after the first. At most [MAX_EXTRA_PRODUCERS_PER_LOOKUP] are, each charged to `links_left`;
    /// if none of them is allowed, or `links_left` runs out, the highest priority one is returned (and its branch is
    /// left out).
    fn find_template_producer(
        &self,
        keys: &[TransactionKey],
        filter: &TemplateFilter,
        links_left: &mut usize,
    ) -> Result<(&PrioritizedTransaction, usize), UnconfirmedPoolError> {
        let first = self.find_highest_priority_transaction(keys)?;
        if self.template_allows(filter, first.key, first) {
            return Ok((first, 0));
        }
        let mut examined = 0usize;
        for key in keys.iter().skip(1).take(MAX_EXTRA_PRODUCERS_PER_LOOKUP) {
            if *links_left == 0 {
                break;
            }
            *links_left = links_left.saturating_sub(1);
            examined = examined.saturating_add(1);
            let producer = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            if self.template_allows(filter, *key, producer) {
                return Ok((producer, examined));
            }
        }
        Ok((first, examined))
    }

    /// The highest priority of the pool transactions producing an output. `txs_by_output` keeps each entry ordered by
    /// priority, highest first (priorities never change once a transaction is in the pool), so this is O(1).
    fn find_highest_priority_transaction(
        &self,
        keys: &[TransactionKey],
    ) -> Result<&PrioritizedTransaction, UnconfirmedPoolError> {
        let key = keys.first().ok_or(UnconfirmedPoolError::StorageOutofSync)?;
        self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)
    }

    // This will search a Vec<Arc<Transaction>> for duplicate inputs of a tx
    #[cfg(test)]
    fn find_duplicate_input(
        current_transactions: &HashMap<TransactionKey, Arc<Transaction>>,
        transactions_to_insert: &HashMap<TransactionKey, Arc<Transaction>>,
    ) -> bool {
        let insert_set = transactions_to_insert
            .values()
            .flat_map(|tx| tx.body.inputs())
            .map(|i| i.output_hash())
            .collect::<HashSet<_>>();
        for transaction in current_transactions.values() {
            for input in transaction.body.inputs() {
                if insert_set.contains(&input.output_hash()) {
                    return true;
                }
            }
        }
        false
    }

    fn lowest_priority(&self) -> Result<&FeePriority, UnconfirmedPoolError> {
        self.tx_by_priority
            .keys()
            .next()
            .ok_or(UnconfirmedPoolError::StorageOutofSync)
    }

    fn remove_lowest_priority_tx(&mut self) -> Result<(), UnconfirmedPoolError> {
        if let Some(tx_key) = self.tx_by_priority.values().next().copied() {
            self.remove_transaction(tx_key)?;
        }
        Ok(())
    }

    /// Remove all current mempool transactions from the UnconfirmedPoolStorage, returning that which have been removed
    pub fn drain_all_mempool_transactions(&mut self) -> Vec<Arc<Transaction>> {
        self.txs_by_signature.clear();
        self.tx_by_kernel_set.clear();
        self.tx_by_priority.clear();
        self.txs_by_output.clear();
        self.txs_by_spent_output.clear();
        self.txs_by_output_commitment.clear();
        self.txs_by_kernel_excess.clear();
        self.txs_by_validator_node.clear();
        self.txs_reading_height.clear();
        self.tx_by_key.drain().map(|(_, val)| val.transaction).collect()
    }

    /// Remove all published transactions from the UnconfirmedPoolStorage and discard deprecated transactions
    pub fn remove_published_and_discard_deprecated_transactions(
        &mut self,
        published_block: &Block,
    ) -> Result<Vec<Arc<Transaction>>, UnconfirmedPoolError> {
        trace!(
            target: LOG_TARGET,
            "Searching for transactions to remove from unconfirmed pool in block {} ({})",
            published_block.header.height,
            published_block.header.hash()
        );
        let timer = Instant::now();
        let body = &published_block.body;

        // The keys of every phase are removed in one batch, in key order. Keys follow insertion order, so a parent
        // comes before its child unless the parent was re-inserted later. Each phase looks up each distinct
        // block item once, as an invalid block may repeat one.
        let mut to_remove = body
            .kernels()
            .iter()
            .map(|kernel| kernel.excess_sig.get_signature())
            .collect::<HashSet<_>>()
            .into_iter()
            .filter_map(|sig| self.txs_by_signature.get(sig))
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>();
        debug!(
            target: LOG_TARGET,
            "Found {} transactions with matching kernel sigs in unconfirmed pool",
            to_remove.len()
        );

        let found = to_remove.len();
        to_remove.extend(
            body.inputs()
                .iter()
                .map(|input| input.output_hash())
                .collect::<HashSet<_>>()
                .iter()
                .filter_map(|hash| self.txs_by_spent_output.get(hash))
                .flatten()
                .copied(),
        );
        debug!(
            target: LOG_TARGET,
            "Found {} transactions with matching inputs in unconfirmed pool",
            to_remove.len().saturating_sub(found)
        );

        let found = to_remove.len();
        to_remove.extend(
            body.outputs()
                .iter()
                .map(|output| output.hash())
                .collect::<HashSet<_>>()
                .iter()
                .filter_map(|hash| self.txs_by_output.get(hash))
                .flatten()
                .copied(),
        );
        debug!(
            target: LOG_TARGET,
            "Found {} transactions with matching outputs in unconfirmed pool",
            to_remove.len().saturating_sub(found)
        );

        to_remove.extend(self.collect_commitment_and_kernel_excess_keys(published_block));
        to_remove.extend(self.collect_validator_node_keys(published_block));

        let removed = self.remove_transactions(to_remove)?;
        debug!(
            target: LOG_TARGET,
            "Removed {} transactions from unconfirmed pool for block {} in {:.2?}",
            removed.len(),
            published_block.header.height,
            timer.elapsed()
        );
        Ok(removed)
    }

    /// The pool transactions that register or exit a validator node `published_block` registers or exits (through a
    /// different transaction, e.g. a resend, fee bump or replayed exit). They can never be valid now (already
    /// registered, not registered, or exit already queued), and block templates are not validated before mining, so
    /// leaving them would cost a locally mined block. The pool holds at most one transaction per validator node.
    fn collect_validator_node_keys(&self, published_block: &Block) -> Vec<TransactionKey> {
        let keys = validator_node_output_keys(&published_block.body)
            .iter()
            .filter_map(|vn_key| self.txs_by_validator_node.get(vn_key))
            .copied()
            .collect::<Vec<_>>();
        debug!(
            target: LOG_TARGET,
            "Found {} transactions registering or exiting the same validator nodes as the block in the unconfirmed pool",
            keys.len()
        );
        keys
    }

    /// The pool transactions that produce an output commitment `published_block` produced, or contain a kernel excess
    /// `published_block` contains: they can never be mined now, even though their outputs or kernels differ in other
    /// respects.
    fn collect_commitment_and_kernel_excess_keys(&self, published_block: &Block) -> Vec<TransactionKey> {
        // A burn's commitment is always that of a burned output in the same body, so outputs alone cover burns
        let commitments = published_block
            .body
            .outputs()
            .iter()
            .filter_map(|output| FixedHash::try_from(output.commitment.as_bytes()).ok())
            .collect::<HashSet<_>>();
        let excesses = published_block
            .body
            .kernels()
            .iter()
            .filter_map(|kernel| FixedHash::try_from(kernel.excess.as_bytes()).ok())
            .collect::<HashSet<_>>();
        let keys = commitments
            .iter()
            .filter_map(|commitment| self.txs_by_output_commitment.get(commitment))
            .chain(
                excesses
                    .iter()
                    .filter_map(|excess| self.txs_by_kernel_excess.get(excess)),
            )
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        debug!(
            target: LOG_TARGET,
            "Found {} transactions with matching output commitments or kernel excesses in unconfirmed pool",
            keys.len()
        );
        keys
    }

    /// Removes the pool transaction `tx_key` from the pool and every index. This is linear in the size of the index
    /// entries it is in, so to remove many transactions use [UnconfirmedPool::remove_transactions], which prunes each
    /// shared entry once.
    pub(crate) fn remove_transaction(
        &mut self,
        tx_key: TransactionKey,
    ) -> Result<Option<Arc<Transaction>>, UnconfirmedPoolError> {
        Ok(self.remove_transactions([tx_key])?.pop())
    }

    /// Removes the pool transactions `keys` from the pool and every index, and returns them. Keys not in the pool
    /// (including repeated ones) are skipped. Each index entry the removed transactions share is pruned once, so this
    /// is linear in the index entries touched.
    pub(crate) fn remove_transactions(
        &mut self,
        keys: impl IntoIterator<Item = TransactionKey>,
    ) -> Result<Vec<Arc<Transaction>>, UnconfirmedPoolError> {
        let removed = keys
            .into_iter()
            .filter_map(|key| self.take_transaction(key))
            .collect::<Vec<_>>();
        self.prune_indexes(&removed)?;
        Ok(removed.into_iter().map(|tx| tx.transaction).collect())
    }

    /// Removes the pool transaction `tx_key` from the pool and the indexes keyed by it, leaving the indexes it shares
    /// with other transactions to [UnconfirmedPool::prune_indexes]
    fn take_transaction(&mut self, tx_key: TransactionKey) -> Option<PrioritizedTransaction> {
        let prioritized_transaction = self.tx_by_key.remove(&tx_key)?;
        self.tx_by_priority.remove(&prioritized_transaction.priority);
        if let Some(set) = kernel_set(&prioritized_transaction.transaction) &&
            self.tx_by_kernel_set.get(&set) == Some(&tx_key)
        {
            self.tx_by_kernel_set.remove(&set);
        }
        for vn_key in validator_node_keys(&prioritized_transaction.transaction.body) {
            if self.txs_by_validator_node.get(&vn_key) == Some(&tx_key) {
                self.txs_by_validator_node.remove(&vn_key);
            }
        }
        self.txs_reading_height.remove(&tx_key);
        trace!(
            target: LOG_TARGET,
            "Deleted transaction: {}",
            prioritized_transaction.transaction
        );
        Some(prioritized_transaction)
    }

    /// Prunes the keys no longer in the pool from every index entry of the `removed` transactions (see
    /// [UnconfirmedPool::take_transaction]), visiting each entry once. Retaining keeps the priority order of
    /// `txs_by_output` and the order of `txs_by_signature`.
    fn prune_indexes(&mut self, removed: &[PrioritizedTransaction]) -> Result<(), UnconfirmedPoolError> {
        let mut signatures = HashSet::new();
        let mut outputs = HashSet::new();
        let mut spent_outputs = HashSet::new();
        let mut commitments = HashSet::new();
        let mut excesses = HashSet::new();
        for tx in removed {
            let body = &tx.transaction.body;
            signatures.extend(body.kernels().iter().map(|k| k.excess_sig.get_signature().clone()));
            outputs.extend(body.outputs().iter().map(|output| output.hash()));
            spent_outputs.extend(tx.input_hashes.iter().copied());
            commitments.extend(tx.output_commitments.iter().copied());
            excesses.extend(tx.kernel_excesses.iter().copied());
        }

        // Removing a single transaction (e.g. evicting the lowest priority one on every insert into a full pool) only
        // compares keys rather than hashing each into `tx_by_key`
        let single = match removed {
            [tx] => Some(tx.key),
            _ => None,
        };
        let tx_by_key = &self.tx_by_key;
        let is_live = |k: &TransactionKey| single.map_or_else(|| tx_by_key.contains_key(k), |key| *k != key);
        let mut visits = 0usize;
        let mut out_of_sync = false;
        for sig in signatures {
            // Every pool transaction is in the signature index, so a removed one must be found there. The other indexes
            // are still pruned, so that they do not keep keys of removed transactions.
            let Some(keys) = self.txs_by_signature.get_mut(&sig) else {
                out_of_sync = true;
                continue;
            };
            let len = keys.len();
            visits = visits.saturating_add(len);
            keys.retain(is_live);
            out_of_sync |= keys.len() == len;
            if keys.is_empty() {
                self.txs_by_signature.remove(&sig);
            }
        }
        // The other indexes tolerate missing entries: the descendant walk takes whole spent-output entries
        prune_entries(&mut self.txs_by_output, outputs, &is_live, &mut visits);
        prune_entries(&mut self.txs_by_spent_output, spent_outputs, &is_live, &mut visits);
        prune_entries(&mut self.txs_by_output_commitment, commitments, &is_live, &mut visits);
        prune_entries(&mut self.txs_by_kernel_excess, excesses, &is_live, &mut visits);
        #[cfg(test)]
        {
            self.prune_visits = self.prune_visits.saturating_add(visits);
        }
        if out_of_sync {
            return Err(UnconfirmedPoolError::StorageOutofSync);
        }
        Ok(())
    }

    /// Returns the total number of unconfirmed transactions stored in the UnconfirmedPool.
    pub fn len(&self) -> usize {
        self.tx_by_key.len()
    }

    /// Returns all transaction stored in the UnconfirmedPool.
    pub fn snapshot(&self) -> Vec<Arc<Transaction>> {
        self.tx_by_key.values().map(|ptx| ptx.transaction.clone()).collect()
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self, transaction_weight: &TransactionWeight) -> Result<u64, TransactionError> {
        let weights = self
            .tx_by_key
            .values()
            .map(|ptx| ptx.transaction.calculate_weight(transaction_weight))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(weights.iter().sum())
    }

    pub fn get_fee_per_gram_stats(
        &self,
        count: usize,
        target_block_weight: u64,
    ) -> Result<Vec<FeePerGramStat>, UnconfirmedPoolError> {
        if count == 0 || target_block_weight == 0 {
            return Ok(vec![]);
        }

        if self.len() == 0 {
            return Ok(vec![]);
        }

        let mut stats = Vec::new();
        let mut offset = 0usize;
        for start in 0..count {
            let mut total_weight: u64 = 0;
            let mut total_fees = MicroMinotari::zero();
            let mut min_fee_per_gram = MicroMinotari::from(u64::MAX);
            let mut max_fee_per_gram = MicroMinotari::zero();
            for key in self.tx_by_priority.values().rev().skip(offset) {
                let tx = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                let weight = tx.weight;

                if total_weight.saturating_add(weight) > target_block_weight {
                    break;
                }

                let total_tx_fee = tx.transaction.body.get_total_fee()?;
                offset = offset.saturating_add(1);
                let fee_per_gram = total_tx_fee
                    .checked_div(MicroMinotari::from(weight))
                    .unwrap_or_default();
                min_fee_per_gram = min_fee_per_gram.min(fee_per_gram);
                max_fee_per_gram = max_fee_per_gram.max(fee_per_gram);
                total_fees = total_fees
                    .checked_add(total_tx_fee)
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating total fees".to_string(),
                    ))?;
                total_weight = total_weight
                    .checked_add(weight)
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating total weights".to_string(),
                    ))?;
            }
            if total_weight == 0 {
                break;
            }
            let stat = FeePerGramStat {
                order: start as u64,
                min_fee_per_gram,
                // The `total_weight == 0` guard above proves this cannot divide by zero.
                // The `total_weight == 0` guard above proves this cannot divide by zero.
                avg_fee_per_gram: total_fees
                    .checked_div(MicroMinotari::from(total_weight))
                    .unwrap_or_default(),
                max_fee_per_gram,
            };
            stats.push(stat);
        }

        Ok(stats)
    }

    /// Returns false if there are any inconsistencies in the internal mempool state, otherwise true
    #[cfg(test)]
    pub(crate) fn check_data_consistency(&self) -> bool {
        self.tx_by_priority.len() == self.tx_by_key.len() &&
            self.tx_by_priority
                .values()
                .all(|tx_key| self.tx_by_key.contains_key(tx_key)) &&
            self.txs_by_signature
                .values()
                .all(|tx_keys| tx_keys.iter().all(|tx_key| self.tx_by_key.contains_key(tx_key))) &&
            self.txs_by_output
                .values()
                .all(|tx_keys| tx_keys.iter().all(|tx_key| self.tx_by_key.contains_key(tx_key))) &&
            self.txs_by_spent_output
                .values()
                .all(|tx_keys| tx_keys.iter().all(|tx_key| self.tx_by_key.contains_key(tx_key))) &&
            self.txs_by_output_commitment
                .values()
                .all(|tx_keys| tx_keys.iter().all(|tx_key| self.tx_by_key.contains_key(tx_key))) &&
            self.txs_by_kernel_excess
                .values()
                .all(|tx_keys| tx_keys.iter().all(|tx_key| self.tx_by_key.contains_key(tx_key))) &&
            self.tx_by_key.iter().all(|(key, tx)| {
                tx.kernel_excesses.iter().all(|excess| {
                    self.txs_by_kernel_excess
                        .get(excess)
                        .is_some_and(|keys| keys.contains(key))
                })
            }) &&
            self.tx_by_key.iter().all(|(key, tx)| {
                tx.output_commitments.iter().all(|commitment| {
                    self.txs_by_output_commitment
                        .get(commitment)
                        .is_some_and(|keys| keys.contains(key))
                })
            }) &&
            self.tx_by_key.iter().all(|(key, tx)| {
                tx.input_hashes.iter().all(|hash| {
                    self.txs_by_spent_output
                        .get(hash)
                        .is_some_and(|keys| keys.contains(key))
                })
            }) &&
            self.txs_by_unique_id
                .values()
                .all(|tx_keys| tx_keys.iter().all(|tx_key| self.tx_by_key.contains_key(tx_key))) &&
            self.txs_by_validator_node
                .values()
                .all(|tx_key| self.tx_by_key.contains_key(tx_key)) &&
            self.tx_by_kernel_set
                .values()
                .all(|tx_key| self.tx_by_key.contains_key(tx_key)) &&
            self.tx_by_key.iter().all(|(key, tx)| {
                kernel_set(&tx.transaction).is_none_or(|set| self.tx_by_kernel_set.get(&set) == Some(key))
            }) &&
            self.txs_reading_height
                .keys()
                .all(|tx_key| self.tx_by_key.contains_key(tx_key)) &&
            self.tx_by_key.iter().all(|(key, tx)| {
                reads_block_height(&tx.transaction.body) == self.txs_reading_height.contains_key(key)
            })
    }

    fn get_next_key(&mut self) -> usize {
        let key = self.key_counter;
        self.key_counter = self.key_counter.wrapping_add(1) % usize::MAX;
        key
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    pub fn compact(&mut self) {
        let (old, new) = shrink_hashmap(&mut self.tx_by_key);
        shrink_hashmap(&mut self.txs_by_signature);
        shrink_hashmap(&mut self.tx_by_kernel_set);
        shrink_hashmap(&mut self.txs_by_output);
        shrink_hashmap(&mut self.txs_by_spent_output);
        shrink_hashmap(&mut self.txs_by_output_commitment);
        shrink_hashmap(&mut self.txs_by_kernel_excess);
        shrink_hashmap(&mut self.txs_by_unique_id);
        shrink_hashmap(&mut self.txs_by_validator_node);

        if old > new {
            debug!(
                target: LOG_TARGET,
                "Shrunk reorg mempool memory usage ({}/{}) ~{}%",
                new,
                old,
                old.saturating_sub(new).saturating_mul(100).checked_div(old).unwrap_or(0)
            );
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use tari_common::configuration::Network;
    use tari_transaction_components::{
        MicroMinotari,
        aggregated_body::AggregateBody,
        consensus::ConsensusConstants,
        fee::Fee,
        helpers::borsh::SerializedSize,
        key_manager::KeyManager,
        test_helpers::{TestParams, UtxoTestParams, add_outputs_with_reserved_sender_offset_keys},
        transaction_builder::TransactionBuilder,
        tx,
        weight::TransactionWeight,
    };

    use super::*;
    use crate::{
        consensus::BaseNodeConsensusManagerBuilder,
        test_helpers::{create_consensus_constants, create_consensus_rules, create_orphan_block},
    };

    fn max_weight() -> u64 {
        create_consensus_constants(0).max_block_transaction_weight()
    }

    #[tokio::test]
    async fn test_find_duplicate_input() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx1 = Arc::new(
            tx!(MicroMinotari(5000), fee: MicroMinotari(5), inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx2 = Arc::new(
            tx!(MicroMinotari(5000), fee: MicroMinotari(5), inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let mut tx_pool = HashMap::new();
        let mut tx1_pool = HashMap::new();
        let mut tx2_pool = HashMap::new();
        tx_pool.insert(0usize, tx1.clone());
        tx1_pool.insert(1usize, tx1);
        tx2_pool.insert(2usize, tx2);
        assert!(
            UnconfirmedPool::find_duplicate_input(&tx_pool, &tx1_pool),
            "Duplicate was not found"
        );
        assert!(
            !UnconfirmedPool::find_duplicate_input(&tx_pool, &tx2_pool),
            "Duplicate was incorrectly found as true"
        );
    }

    #[tokio::test]
    async fn test_insert_and_retrieve_highest_priority_txs() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx1 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx2 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(4), inputs: 4, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx3 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(20), inputs: 5, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx4 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(6), inputs: 3, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx5 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(11), inputs: 5, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 4,
            weight_tx_skip_count: 3,
            min_fee: 0,
        });

        let tx_weight = TransactionWeight::latest();
        unconfirmed_pool
            .insert_many(
                [tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()],
                &tx_weight,
            )
            .expect("Failed to insert many");
        // Check that lowest priority tx was removed to make room for new incoming transactions
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig));
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig));
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig));
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig));
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig));
        // Retrieve the set of highest priority unspent transactions
        let desired_weight = tx1.calculate_weight(&tx_weight).expect("Failed to get tx") +
            tx3.calculate_weight(&tx_weight).expect("Failed to get tx") +
            tx5.calculate_weight(&tx_weight).expect("Failed to get tx");
        let results = unconfirmed_pool
            .fetch_highest_priority_txs(desired_weight, max_weight())
            .unwrap();
        assert_eq!(results.retrieved_transactions.len(), 3);
        assert!(results.retrieved_transactions.contains(&tx1));
        assert!(results.retrieved_transactions.contains(&tx3));
        assert!(results.retrieved_transactions.contains(&tx5));
        // Note that transaction tx5 could not be included as its weight was to big to fit into the remaining allocated
        // space, the second best transaction was then included

        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[test]
    fn block_template_byte_budget_is_derived_from_the_rpc_frame() {
        // Rollout value: derived from the legacy 6 MiB frame, the smaller of the two
        assert_eq!(MAX_BLOCK_TEMPLATE_BODY_BYTES, 5 * 1024 * 1024);
        assert_eq!(MAX_BLOCK_TEMPLATE_BODY_BYTES, LEGACY_RPC_MAX_FRAME_SIZE - 1024 * 1024);
        const { assert!(MAX_BLOCK_TEMPLATE_BODY_BYTES < RPC_MAX_FRAME_SIZE) };
        const { assert!(MAX_BLOCK_TEMPLATE_BODY_BYTES < LEGACY_RPC_MAX_FRAME_SIZE) };
        const { assert!(MAX_BLOCK_TEMPLATE_BODY_BYTES <= MAX_BLOCK_BODY_BYTES) };
    }

    #[test]
    fn block_template_byte_budget_is_within_every_networks_block_body_limit() {
        for network in [
            Network::MainNet,
            Network::StageNet,
            Network::NextNet,
            Network::LocalNet,
            Network::Igor,
            Network::Esmeralda,
        ] {
            for constants in ConsensusConstants::for_network(network) {
                assert!(
                    MAX_BLOCK_TEMPLATE_BODY_BYTES <= constants.max_block_body_bytes(),
                    "{network} at height {}",
                    constants.effective_from_height()
                );
            }
        }
    }

    #[tokio::test]
    async fn test_highest_priority_txs_stop_at_byte_budget() {
        let key_manager = KeyManager::new_random().unwrap();
        // Similar sized transactions, in decreasing priority order
        let txs = [20u64, 15, 10, 5]
            .into_iter()
            .map(|fee| {
                Arc::new(
                    tx!(MicroMinotari(5_000), fee: MicroMinotari(fee), inputs: 2, outputs: 1, &key_manager)
                        .expect("Failed to get tx")
                        .0,
                )
            })
            .collect::<Vec<_>>();
        let size = |tx: &Arc<Transaction>| tx.body.get_serialized_size().unwrap();

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 10,
            min_fee: 0,
        });
        let tx_weight = TransactionWeight::latest();
        unconfirmed_pool.insert_many(txs.clone(), &tx_weight).unwrap();
        let min_size = txs.iter().map(size).min().unwrap();

        // Weight is not the limiting factor here, only the byte budget is
        let budget = size(&txs[0]) + size(&txs[1]) + min_size - 1;
        let results = unconfirmed_pool
            .fetch_highest_priority_txs_with_byte_budget(u64::MAX, budget, max_weight())
            .unwrap();
        assert_eq!(results.retrieved_transactions.len(), 2);
        assert!(results.retrieved_transactions.contains(&txs[0]));
        assert!(results.retrieved_transactions.contains(&txs[1]));
        let selected_bytes: usize = results.retrieved_transactions.iter().map(size).sum();
        assert!(selected_bytes <= budget);

        // Exactly at the boundary, the transaction fits
        let budget = size(&txs[0]) + size(&txs[1]) + size(&txs[2]);
        let results = unconfirmed_pool
            .fetch_highest_priority_txs_with_byte_budget(u64::MAX, budget, max_weight())
            .unwrap();
        assert_eq!(results.retrieved_transactions.len(), 3);
        assert!(!results.retrieved_transactions.contains(&txs[3]));

        // Nothing fits in an empty budget
        let results = unconfirmed_pool
            .fetch_highest_priority_txs_with_byte_budget(u64::MAX, 0, max_weight())
            .unwrap();
        assert!(results.retrieved_transactions.is_empty());

        // The default budget does not constrain a small pool
        let results = unconfirmed_pool
            .fetch_highest_priority_txs(u64::MAX, max_weight())
            .unwrap();
        assert_eq!(results.retrieved_transactions.len(), 4);
        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn test_highest_priority_txs_skip_a_large_tx_that_exceeds_the_byte_budget() {
        let key_manager = KeyManager::new_random().unwrap();
        // The highest priority transaction is also the largest
        let large = Arc::new(
            tx!(MicroMinotari(50_000), fee: MicroMinotari(50), inputs: 12, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let small1 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(10), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let small2 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let size = |tx: &Arc<Transaction>| tx.body.get_serialized_size().unwrap();
        assert!(size(&large) > size(&small1) + size(&small2));

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 10,
            min_fee: 0,
        });
        let tx_weight = TransactionWeight::latest();
        unconfirmed_pool
            .insert_many([large.clone(), small1.clone(), small2.clone()], &tx_weight)
            .unwrap();

        // The large transaction does not fit, but the lower-priority smaller ones do
        let budget = size(&small1) + size(&small2);
        let results = unconfirmed_pool
            .fetch_highest_priority_txs_with_byte_budget(u64::MAX, budget, max_weight())
            .unwrap();
        assert_eq!(results.retrieved_transactions.len(), 2);
        assert!(!results.retrieved_transactions.contains(&large));
        assert!(results.retrieved_transactions.contains(&small1));
        assert!(results.retrieved_transactions.contains(&small2));

        // With room for the large one, it is selected first
        let budget = size(&large) + size(&small1);
        let results = unconfirmed_pool
            .fetch_highest_priority_txs_with_byte_budget(u64::MAX, budget, max_weight())
            .unwrap();
        assert_eq!(results.retrieved_transactions.len(), 2);
        assert!(results.retrieved_transactions.contains(&large));
        assert!(results.retrieved_transactions.contains(&small1));
    }

    fn normal_txs(key_manager: &KeyManager, count: usize, fee_per_gram: u64) -> Vec<Arc<Transaction>> {
        (0..count)
            .map(|_| {
                Arc::new(
                    tx!(MicroMinotari(5_000), fee: MicroMinotari(fee_per_gram), inputs: 1, outputs: 1, key_manager)
                        .expect("Failed to get tx")
                        .0,
                )
            })
            .collect()
    }

    fn body_size(tx: &Arc<Transaction>) -> usize {
        tx.body.get_serialized_size().unwrap()
    }

    fn total_fee(txs: &[Arc<Transaction>]) -> u64 {
        txs.iter().map(|tx| tx.body.get_total_fee().unwrap().as_u64()).sum()
    }

    #[tokio::test]
    async fn byte_skips_do_not_use_up_the_skip_allowance() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let normal = normal_txs(&key_manager, 10, 5);
        let normal_bytes: usize = normal.iter().map(body_size).sum();

        // 21 large, high-fee transactions that all spend the same input (the mempool holds conflicting spends), each
        // bigger than half of the byte budget, so that only one of them can be selected
        let template = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let input_size = template.body.inputs().first().unwrap().get_serialized_size().unwrap();
        let num_inputs = (normal_bytes + 4 * 1024) / input_size + 1;
        // The same set of distinct inputs for all of them (so they conflict with each other, but none spends an input
        // twice)
        let filler = tx!(MicroMinotari(5_000_000), fee: MicroMinotari(5), inputs: num_inputs, outputs: 1, &key_manager)
            .expect("Failed to get tx")
            .0;
        let large = (0..21)
            .map(|_| {
                let kernels = tx!(MicroMinotari(500_000), fee: MicroMinotari(500), inputs: 1, outputs: 1, &key_manager)
                    .expect("Failed to get tx")
                    .0
                    .body
                    .kernels()
                    .clone();
                Arc::new(Transaction::new(
                    filler.body.inputs().clone(),
                    template.body.outputs().clone(),
                    kernels,
                    Default::default(),
                    Default::default(),
                ))
            })
            .collect::<Vec<_>>();
        let budget = body_size(&large[0]) + normal_bytes + MIN_TRANSACTION_BODY_BYTES / 2;
        assert!(2 * body_size(&large[0]) > budget);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(budget);
        unconfirmed_pool
            .insert_many(large.iter().chain(normal.iter()).cloned(), &tx_weight)
            .unwrap();
        let normal_fee_rate = unconfirmed_pool
            .tx_by_key
            .values()
            .find(|p| p.transaction == normal[0])
            .unwrap();
        let large_fee_rate = unconfirmed_pool
            .tx_by_key
            .values()
            .find(|p| p.transaction == large[0])
            .unwrap();
        assert!(large_fee_rate.priority > normal_fee_rate.priority);

        // One large transaction and all of the normal ones: the 20 large ones that did not fit used up no skips
        let results = unconfirmed_pool
            .fetch_highest_priority_txs(u64::MAX, max_weight())
            .unwrap();
        assert_eq!(results.retrieved_transactions.len(), 1 + normal.len());
        assert_eq!(
            results
                .retrieved_transactions
                .iter()
                .filter(|tx| large.contains(tx))
                .count(),
            1
        );
        for tx in &normal {
            assert!(results.retrieved_transactions.contains(tx));
        }
        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn byte_stuffing_is_outbid_under_byte_pressure() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let fee_per_gram = |tx: &Arc<Transaction>| {
            tx.body.get_total_fee().unwrap().as_u64() * 1000 / tx.calculate_weight(&tx_weight).unwrap()
        };
        // Byte-heavy: many inputs (cheap in weight, not in bytes), with twice the fee per gram
        let heavy = (0..3)
            .map(|_| {
                Arc::new(
                    tx!(MicroMinotari(500_000), fee: MicroMinotari(10), inputs: 12, outputs: 1, &key_manager)
                        .expect("Failed to get tx")
                        .0,
                )
            })
            .collect::<Vec<_>>();
        let normal = normal_txs(&key_manager, 10, 5);
        assert!(fee_per_gram(&heavy[0]) > fee_per_gram(&normal[0]));
        // Together, the heavy transactions exceed the byte budget, which fits exactly the normal ones
        let normal_bytes: usize = normal.iter().map(body_size).sum();
        let budget = normal_bytes + 100;
        assert!(heavy.iter().map(body_size).sum::<usize>() > budget);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(budget);
        unconfirmed_pool
            .insert_many(heavy.iter().chain(normal.iter()).cloned(), &tx_weight)
            .unwrap();

        // By fee per gram alone, a heavy transaction goes first and crowds out half of the normal ones
        let by_weight = unconfirmed_pool.select_txs(u64::MAX, budget, Ranking::Weight).unwrap();
        assert!(by_weight.byte_bound);
        assert!(
            heavy
                .iter()
                .any(|tx| by_weight.results.retrieved_transactions.contains(tx))
        );
        assert!(by_weight.results.retrieved_transactions.len() < 1 + normal.len());

        // Under byte pressure, the effective-weight ordering selects all of the normal transactions, for more fees
        let chosen = unconfirmed_pool
            .fetch_selection(u64::MAX, budget, max_weight(), &TemplateFilter::unrestricted())
            .unwrap();
        assert_eq!(chosen.results.retrieved_transactions.len(), normal.len());
        for tx in &normal {
            assert!(chosen.results.retrieved_transactions.contains(tx));
        }
        assert_eq!(chosen.total_fees, total_fee(&normal));
        assert!(chosen.total_fees >= by_weight.total_fees);
        let results = unconfirmed_pool
            .fetch_highest_priority_txs(u64::MAX, max_weight())
            .unwrap();
        assert_eq!(results.retrieved_transactions.len(), normal.len());
        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn without_byte_pressure_selection_is_by_fee_per_gram() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        // Consolidations (many cheap inputs) and ordinary spends with a range of fees
        let mut txs = Vec::new();
        for (inputs, fee) in [(12, 10u64), (1, 5), (20, 7), (2, 20), (1, 1), (5, 3)] {
            txs.push(Arc::new(
                tx!(MicroMinotari(500_000), fee: MicroMinotari(fee), inputs: inputs, outputs: 1, &key_manager)
                    .expect("Failed to get tx")
                    .0,
            ));
        }
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        unconfirmed_pool.insert_many(txs.clone(), &tx_weight).unwrap();
        // Weight is the only constraint: room for the three highest fee-per-gram transactions
        let mut by_fee_per_gram = txs.clone();
        by_fee_per_gram.sort_by_key(|tx| {
            std::cmp::Reverse(
                tx.body.get_total_fee().unwrap().as_u64() * 1000 / tx.calculate_weight(&tx_weight).unwrap(),
            )
        });
        let desired_weight: u64 = by_fee_per_gram
            .iter()
            .take(3)
            .map(|tx| tx.calculate_weight(&tx_weight).unwrap())
            .sum();

        let chosen = unconfirmed_pool
            .fetch_selection(
                desired_weight,
                MAX_BLOCK_TEMPLATE_BODY_BYTES,
                max_weight(),
                &TemplateFilter::unrestricted(),
            )
            .unwrap();
        assert!(!chosen.byte_bound);
        let by_weight = unconfirmed_pool
            .select_txs(desired_weight, MAX_BLOCK_TEMPLATE_BODY_BYTES, Ranking::Weight)
            .unwrap();
        let mut chosen_txs = chosen.results.retrieved_transactions.clone();
        let mut first_pass = by_weight.results.retrieved_transactions.clone();
        chosen_txs.sort_by_key(|tx| tx.body.kernels()[0].excess_sig.get_signature().to_vec());
        first_pass.sort_by_key(|tx| tx.body.kernels()[0].excess_sig.get_signature().to_vec());
        assert_eq!(chosen_txs, first_pass);
        assert_eq!(chosen_txs.len(), 3);
        for tx in by_fee_per_gram.iter().take(3) {
            assert!(chosen_txs.contains(tx));
        }
    }

    #[test]
    fn effective_weight_prices_the_byte_budget_share() {
        // Below the byte-to-weight ratio, the real weight is used
        assert_eq!(effective_weight(100, 1_000, 90_000, 5_000_000), 100);
        // Above it, the share of the byte budget is used, rounded up
        assert_eq!(effective_weight(100, 1_000_000, 90_000, 5_000_000), 18_000);
        assert_eq!(effective_weight(1, 1, 90_000, 5_000_000), 1);
        assert_eq!(effective_weight(0, 1, 90_000, 5_000_000), 1);
        // A whole budget's worth of bytes costs a whole block's weight
        assert_eq!(effective_weight(10, 5_000_000, 90_000, 5_000_000), 90_000);
        // No overflow
        assert_eq!(effective_weight(10, usize::MAX, u64::MAX, 1), u64::MAX);
    }

    /// A cheap synthetic transaction derived from `base`: no inputs, `base`'s first output with its maturity set to
    /// `n` and a fresh random commitment (so that its hash and commitment are unique), and `base`'s first kernel with a
    /// fresh random excess and excess signature scalar, and the given fee. Enough for the pool's selection logic, which
    /// does not validate.
    fn synthetic_tx(base: &Transaction, n: u64, fee: u64) -> Arc<Transaction> {
        use tari_common_types::types::{CompressedCommitment, CompressedSignature};
        use tari_crypto::keys::SecretKey;
        let mut output = base.body.outputs().first().unwrap().clone();
        output.features.maturity = n;
        output.commitment = CompressedCommitment::from_compressed_key(crate::test_helpers::new_public_key());
        let mut kernel = base.body.kernels().first().unwrap().clone();
        kernel.fee = MicroMinotari(fee);
        kernel.excess = CompressedCommitment::from_compressed_key(crate::test_helpers::new_public_key());
        kernel.excess_sig = CompressedSignature::new(
            kernel.excess_sig.get_compressed_public_nonce().clone(),
            PrivateKey::random(&mut rand::rng()),
        );
        Arc::new(Transaction::new(
            vec![],
            vec![output],
            vec![kernel],
            Default::default(),
            Default::default(),
        ))
    }

    /// `n` distinct inputs derived from `input` (the spent output's maturity is varied, so each spends a different
    /// output hash). Cheap stand-ins for real inputs; the pool's selection logic does not validate.
    fn distinct_inputs(
        input: &tari_transaction_components::transaction_components::TransactionInput,
        n: usize,
    ) -> Vec<tari_transaction_components::transaction_components::TransactionInput> {
        use tari_transaction_components::transaction_components::SpentOutput;
        (0..n)
            .map(|i| {
                let mut input = input.clone();
                if let SpentOutput::OutputData { features, .. } = &mut input.spent_output {
                    features.maturity = u64::try_from(i).unwrap().saturating_add(1_000_000);
                }
                input
            })
            .collect()
    }

    #[tokio::test]
    async fn conflicting_sentinels_do_not_use_up_the_skip_allowance() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let weight = |tx: &Arc<Transaction>| tx.calculate_weight(&tx_weight).unwrap();
        // S: small, with a very high fee per gram
        let s = Arc::new(
            tx!(MicroMinotari(5_000_000), fee: MicroMinotari(1000), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let normal = normal_txs(&key_manager, 10, 5);
        let total_weight = weight(&s) + normal.iter().map(weight).sum::<u64>() + 50;

        // 20 sentinels: each spends S's input, is ranked just below S, and is too heavy to fit next to S
        let input = s.body.inputs().first().unwrap().clone();
        let num_inputs = usize::try_from((total_weight - weight(&s)) / 8 + 1).unwrap();
        let sentinels = (0..20)
            .map(|_| {
                let kernels =
                    tx!(MicroMinotari(5_000_000), fee: MicroMinotari(100), inputs: 1, outputs: 1, &key_manager)
                        .expect("Failed to get tx")
                        .0
                        .body
                        .kernels()
                        .clone();
                Arc::new(Transaction::new(
                    vec![input.clone(); num_inputs],
                    normal[0].body.outputs().clone(),
                    kernels,
                    Default::default(),
                    Default::default(),
                ))
            })
            .collect::<Vec<_>>();
        assert!(weight(&sentinels[0]) > total_weight - weight(&s));

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        unconfirmed_pool
            .insert_many(
                std::iter::once(s.clone())
                    .chain(sentinels.iter().cloned())
                    .chain(normal.iter().cloned()),
                &tx_weight,
            )
            .unwrap();
        let priority = |tx: &Arc<Transaction>| {
            unconfirmed_pool
                .tx_by_key
                .values()
                .find(|p| &p.transaction == tx)
                .unwrap()
                .priority
                .clone()
        };
        assert!(priority(&s) > priority(&sentinels[0]));
        assert!(priority(&sentinels[0]) > priority(&normal[0]));

        let selection = unconfirmed_pool
            .select_txs(total_weight, MAX_BLOCK_TEMPLATE_BODY_BYTES, Ranking::Weight)
            .unwrap();
        let selected = &selection.results.retrieved_transactions;
        assert_eq!(selected.len(), 1 + normal.len());
        assert!(selected.contains(&s));
        for tx in &normal {
            assert!(selected.contains(tx));
        }
        // Recognised as conflicting before their ancestors are walked, and not counted towards any limit
        assert_eq!(selection.conflict_drops, sentinels.len());
        assert_eq!(selection.byte_skips, 0);
        // Not a byte-budget problem
        assert!(!selection.byte_bound);
    }

    #[tokio::test]
    async fn many_small_conflicting_spends_do_not_end_the_pass() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let normal = normal_txs(&key_manager, 10, 5);
        // Hundreds of small sentinels, all spending the same input, ranked above the normal
        // transactions. Each fits by weight and bytes; only one of them can ever be mined.
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let input = base.body.inputs().first().unwrap().clone();
        let sentinels = (0..510u64)
            .map(|n| {
                let tx = synthetic_tx(&base, n, 5_000);
                Arc::new(Transaction::new(
                    vec![input.clone()],
                    tx.body.outputs().clone(),
                    tx.body.kernels().clone(),
                    Default::default(),
                    Default::default(),
                ))
            })
            .collect::<Vec<_>>();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 1_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        unconfirmed_pool
            .insert_many(sentinels.iter().chain(normal.iter()).cloned(), &tx_weight)
            .unwrap();
        let priority = |tx: &Arc<Transaction>| {
            unconfirmed_pool
                .tx_by_key
                .values()
                .find(|p| &p.transaction == tx)
                .unwrap()
                .priority
                .clone()
        };
        assert!(
            sentinels
                .iter()
                .all(|sentinel| priority(sentinel) > priority(&normal[0]))
        );

        for ranking in [Ranking::Weight, Ranking::EffectiveWeight {
            max_block_transaction_weight: max_weight(),
            max_body_bytes: MAX_BLOCK_TEMPLATE_BODY_BYTES,
        }] {
            let selection = unconfirmed_pool
                .select_txs(u64::MAX, MAX_BLOCK_TEMPLATE_BODY_BYTES, ranking)
                .unwrap();
            let selected = &selection.results.retrieved_transactions;
            assert_eq!(
                selected.iter().filter(|tx| sentinels.contains(tx)).count(),
                1,
                "{ranking:?}"
            );
            for tx in &normal {
                assert!(selected.contains(tx), "{ranking:?}");
            }
            assert_eq!(selected.len(), 1 + normal.len());
            // Every other sentinel is dropped without counting towards any limit
            assert_eq!(selection.conflict_drops, sentinels.len() - 1);
            assert_eq!(selection.byte_skips, 0);
            assert!(!selection.byte_bound);
        }
    }

    #[tokio::test]
    async fn heavy_transactions_do_not_use_up_the_skip_allowance_until_the_block_is_full_enough() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let weight = |tx: &Arc<Transaction>| tx.calculate_weight(&tx_weight).unwrap();
        let s = Arc::new(
            tx!(MicroMinotari(5_000_000), fee: MicroMinotari(1000), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let normal = normal_txs(&key_manager, 10, 5);
        let normal_weight = normal.iter().map(weight).sum::<u64>();
        let total_weight = weight(&s) + normal_weight + 50;
        // A policy block weight for which the template is full enough only once the normal transactions are in
        let max_block_weight = 20 * 200;
        assert!(normal_weight > max_block_weight / BLOCK_FULL_ENOUGH_DIVISOR);

        // 20 sentinels that do NOT conflict with S (they all spend an unrelated output Y), ranked just below S, each
        // too heavy to fit next to it
        let y = normal_txs(&key_manager, 1, 5)
            .pop()
            .unwrap()
            .body
            .inputs()
            .first()
            .unwrap()
            .clone();
        let num_inputs = usize::try_from((total_weight - weight(&s)) / 8 + 1).unwrap();
        let sentinels = (0..20)
            .map(|_| {
                let kernels =
                    tx!(MicroMinotari(5_000_000), fee: MicroMinotari(100), inputs: 1, outputs: 1, &key_manager)
                        .expect("Failed to get tx")
                        .0
                        .body
                        .kernels()
                        .clone();
                Arc::new(Transaction::new(
                    vec![y.clone(); num_inputs],
                    normal[0].body.outputs().clone(),
                    kernels,
                    Default::default(),
                    Default::default(),
                ))
            })
            .collect::<Vec<_>>();
        assert!(weight(&sentinels[0]) > total_weight - weight(&s));
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        unconfirmed_pool
            .insert_many(
                std::iter::once(s.clone())
                    .chain(sentinels.iter().cloned())
                    .chain(normal.iter().cloned()),
                &tx_weight,
            )
            .unwrap();

        let selection = unconfirmed_pool
            .select_txs_with(
                total_weight,
                MAX_BLOCK_TEMPLATE_BODY_BYTES,
                Ranking::Weight,
                max_block_weight,
                HashSet::new(),
                &TemplateFilter::unrestricted(),
            )
            .unwrap();
        let selected = &selection.results.retrieved_transactions;
        assert_eq!(selected.len(), 1 + normal.len());
        assert!(selected.contains(&s));
        for tx in &normal {
            assert!(selected.contains(tx));
        }
        // Each sentinel is walked and found too heavy while the block is not yet full enough, so none of them counts
        // towards the skip allowance (they conflict only with each other, and none is selected)
        assert_eq!(selection.early_weight_skips, sentinels.len());
        assert!(!selection.byte_bound);
    }

    #[tokio::test]
    async fn weight_failures_still_count_once_the_block_is_full_enough() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let weight = |tx: &Arc<Transaction>| tx.calculate_weight(&tx_weight).unwrap();
        let fill = (0..5)
            .map(|_| {
                Arc::new(
                    tx!(MicroMinotari(500_000), fee: MicroMinotari(50), inputs: 1, outputs: 1, &key_manager)
                        .expect("Failed to get tx")
                        .0,
                )
            })
            .collect::<Vec<_>>();
        let heavy = (0..21)
            .map(|_| {
                Arc::new(
                    tx!(MicroMinotari(500_000), fee: MicroMinotari(20), inputs: 5, outputs: 1, &key_manager)
                        .expect("Failed to get tx")
                        .0,
                )
            })
            .collect::<Vec<_>>();
        let late = normal_txs(&key_manager, 1, 5).pop().unwrap();
        // After the fill, the late transaction still fits but none of the heavy ones do, and the block is full enough
        let total_weight = fill.iter().map(weight).sum::<u64>() + weight(&late) + 10;
        assert!(weight(&heavy[0]) > weight(&late) + 10);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        unconfirmed_pool
            .insert_many(
                fill.iter()
                    .cloned()
                    .chain(heavy.iter().cloned())
                    .chain(std::iter::once(late.clone())),
                &tx_weight,
            )
            .unwrap();

        let selection = unconfirmed_pool
            .select_txs(total_weight, MAX_BLOCK_TEMPLATE_BODY_BYTES, Ranking::Weight)
            .unwrap();
        let selected = &selection.results.retrieved_transactions;
        // The pass stops after 20 weight skips, before it reaches the late transaction
        assert_eq!(selected.len(), fill.len());
        assert!(!selected.contains(&late));
        assert_eq!(selection.early_weight_skips, 0);
    }

    /// Inserts a chain of `depth` low-fee synthetic transactions and returns them, root first
    fn insert_chain(
        unconfirmed_pool: &mut UnconfirmedPool,
        base: &Transaction,
        first: u64,
        depth: u64,
        fee: u64,
    ) -> Vec<Arc<Transaction>> {
        let tx_weight = TransactionWeight::latest();
        let mut chain: Vec<Arc<Transaction>> = Vec::new();
        for n in first..first.saturating_add(depth) {
            let tx = synthetic_tx(base, n, fee);
            let dependent_outputs = chain.last().map(|p| vec![p.body.outputs().first().unwrap().hash()]);
            unconfirmed_pool
                .insert(tx.clone(), dependent_outputs, &tx_weight)
                .unwrap();
            chain.push(tx);
        }
        chain
    }

    /// Inserts `count` synthetic children of `parent`, all spending `input` (so they conflict with each other)
    fn insert_children(
        unconfirmed_pool: &mut UnconfirmedPool,
        base: &Transaction,
        parent: &Transaction,
        input: &tari_transaction_components::transaction_components::TransactionInput,
        first: u64,
        count: u64,
        fee: u64,
    ) -> Vec<Arc<Transaction>> {
        let tx_weight = TransactionWeight::latest();
        let parent_output = parent.body.outputs().first().unwrap().hash();
        (first..first.saturating_add(count))
            .map(|n| {
                let tx = synthetic_tx(base, n, fee);
                let child = Arc::new(Transaction::new(
                    vec![input.clone()],
                    tx.body.outputs().clone(),
                    tx.body.kernels().clone(),
                    Default::default(),
                    Default::default(),
                ));
                unconfirmed_pool
                    .insert(child.clone(), Some(vec![parent_output]), &tx_weight)
                    .unwrap();
                child
            })
            .collect()
    }

    #[tokio::test]
    async fn children_of_heavy_parents_do_not_empty_the_template() {
        // Variant A: two chained parents, each just over half of the block weight, at minimal fee, with 500 small
        // high-rate children. Every child's branch is too heavy while the template is still empty.
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 10, 5);
        let total_weight = max_weight();
        let heavy_parent = |n: u64| {
            let tx = synthetic_tx(&base, n, 1);
            let input = base.body.inputs().first().unwrap().clone();
            let num_inputs = usize::try_from(total_weight * 51 / 100 / 8).unwrap();
            Arc::new(Transaction::new(
                vec![input; num_inputs],
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        };
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 1_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        let p1 = heavy_parent(1);
        let p2 = heavy_parent(2);
        assert!(p1.calculate_weight(&tx_weight).unwrap() > total_weight / 2);
        unconfirmed_pool.insert(p1.clone(), None, &tx_weight).unwrap();
        unconfirmed_pool
            .insert(
                p2.clone(),
                Some(vec![p1.body.outputs().first().unwrap().hash()]),
                &tx_weight,
            )
            .unwrap();
        let other_input = normal_txs(&key_manager, 1, 5)
            .pop()
            .unwrap()
            .body
            .inputs()
            .first()
            .unwrap()
            .clone();
        let children = insert_children(&mut unconfirmed_pool, &base, &p2, &other_input, 10, 500, 100_000);
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();

        for ranking in [Ranking::Weight, Ranking::EffectiveWeight {
            max_block_transaction_weight: max_weight(),
            max_body_bytes: usize::MAX / 2,
        }] {
            let selection = unconfirmed_pool
                .select_txs_with(
                    total_weight,
                    usize::MAX / 2,
                    ranking,
                    max_weight(),
                    HashSet::new(),
                    &TemplateFilter::unrestricted(),
                )
                .unwrap();
            let selected = &selection.results.retrieved_transactions;
            for tx in &normal {
                assert!(selected.contains(tx), "{ranking:?}");
            }
            assert!(children.iter().all(|child| !selected.contains(child)));
            assert!(!selection.ended_early, "{ranking:?}");
        }
        let results = unconfirmed_pool
            .fetch_highest_priority_txs(total_weight, max_weight())
            .unwrap();
        for tx in &normal {
            assert!(results.retrieved_transactions.contains(tx));
        }
    }

    #[tokio::test]
    async fn children_of_a_deep_chain_do_not_starve_honest_transactions() {
        // A 2,000-deep chain with 2 * MAX_WALKED_FRAMES_PER_PASS / 2,000 + 1 conflicting children. This no longer
        // exhausts the walking budget: MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE drops each child after 101 frames. Budget
        // exhaustion is exercised by
        // `exhausting_the_walking_budget_still_selects_queued_branches_and_honest_transactions`.
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 10, 5);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        const DEPTH: u64 = 2_000;
        let chain = insert_chain(&mut unconfirmed_pool, &base, 0, DEPTH, 1);
        let input = base.body.inputs().first().unwrap().clone();
        let num_children = 2 * u64::try_from(MAX_WALKED_FRAMES_PER_PASS).unwrap() / DEPTH + 1;
        insert_children(
            &mut unconfirmed_pool,
            &base,
            chain.last().unwrap(),
            &input,
            DEPTH,
            num_children,
            100_000,
        );
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();

        let results = unconfirmed_pool
            .fetch_highest_priority_txs(u64::MAX, max_weight())
            .unwrap();
        for tx in &normal {
            assert!(results.retrieved_transactions.contains(tx));
        }
    }

    #[tokio::test]
    async fn exhausting_the_walking_budget_still_selects_queued_branches_and_honest_transactions() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 10, 5);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        // An honest CPFP pair: a low-fee parent and a child paying for both, ranked above everything else
        let honest = insert_chain(&mut unconfirmed_pool, &base, 0, 1, 1);
        let honest_child = insert_children(
            &mut unconfirmed_pool,
            &base,
            &honest[0],
            &normal_txs(&key_manager, 1, 5)
                .pop()
                .unwrap()
                .body
                .inputs()
                .first()
                .unwrap()
                .clone(),
            1,
            1,
            1_000_000,
        )
        .pop()
        .unwrap();
        // Junk: a chain just under MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE deep, with enough conflicting children that
        // walking each one's ancestry exhausts MAX_WALKED_FRAMES_PER_PASS
        let depth = u64::try_from(MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE).unwrap() - 1;
        let chain = insert_chain(&mut unconfirmed_pool, &base, 10, depth, 1);
        let input = base.body.inputs().first().unwrap().clone();
        let num_children = u64::try_from(MAX_WALKED_FRAMES_PER_PASS).unwrap() / (depth + 1) + 20;
        insert_children(
            &mut unconfirmed_pool,
            &base,
            chain.last().unwrap(),
            &input,
            10 + depth,
            num_children,
            100_000,
        );
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();

        // The first pass runs out of walking budget, but still selects the honest branch it had already queued
        let by_weight = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        assert!(by_weight.budget_exhausted);
        assert!(by_weight.ended_early);
        assert!(by_weight.byte_bound);
        // The children dropped in the drain (conflicts with the first one selected) are state-dependent drops; since
        // the first pass died of budget exhaustion, the second pass leaves them out too
        assert!(!by_weight.state_dropped.is_empty());
        let exclusions = UnconfirmedPool::second_pass_exclusions(&by_weight);
        assert!(by_weight.state_dropped.iter().all(|key| exclusions.contains(key)));
        assert!(by_weight.results.retrieved_transactions.contains(&honest_child));
        assert!(by_weight.results.retrieved_transactions.contains(&honest[0]));

        // The final result also contains the honest transactions the first pass never reached
        let results = unconfirmed_pool
            .fetch_highest_priority_txs(u64::MAX, max_weight())
            .unwrap();
        for tx in normal.iter().chain([&honest_child, &honest[0]]) {
            assert!(results.retrieved_transactions.contains(tx));
        }
    }

    #[tokio::test]
    async fn candidates_with_too_many_unselected_ancestors_are_dropped_uncounted() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 10, 5);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 1_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        let input = base.body.inputs().first().unwrap().clone();
        // A 150-deep chain whose tip child pays a lot: too many unselected ancestors
        let deep = insert_chain(&mut unconfirmed_pool, &base, 0, 150, 1);
        let deep_child = insert_children(
            &mut unconfirmed_pool,
            &base,
            deep.last().unwrap(),
            &input,
            150,
            1,
            1_000_000,
        )
        .pop()
        .unwrap();
        // A 50-deep chain whose tip child pays a lot: still selectable, with its ancestors
        let shallow = insert_chain(&mut unconfirmed_pool, &base, 200, 50, 1);
        let other_input = normal_txs(&key_manager, 1, 5)
            .pop()
            .unwrap()
            .body
            .inputs()
            .first()
            .unwrap()
            .clone();
        let shallow_child = insert_children(
            &mut unconfirmed_pool,
            &base,
            shallow.last().unwrap(),
            &other_input,
            250,
            1,
            1_000_000,
        )
        .pop()
        .unwrap();
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();

        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        let selected = &selection.results.retrieved_transactions;
        assert!(!selected.contains(&deep_child));
        assert!(selected.contains(&shallow_child));
        for tx in shallow.iter().chain(normal.iter()) {
            assert!(selected.contains(tx));
        }
        assert!(selection.too_many_ancestors_drops > 0);
        assert_eq!(selection.weight_skips_counted, 0);
    }

    #[tokio::test]
    async fn byte_skipped_branches_are_not_checked_for_conflicts() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 10, 5);
        let input = base.body.inputs().first().unwrap().clone();
        let input_size = input.get_serialized_size().unwrap();
        // A parent with a body just under the byte budget, made of thousands of inputs, at minimal fee
        let budget = MAX_BLOCK_TEMPLATE_BODY_BYTES;
        let template = synthetic_tx(&base, 0, 1);
        let num_inputs = (budget - body_size(&template) - 600) / input_size;
        let parent = Arc::new(Transaction::new(
            vec![input.clone(); num_inputs],
            template.body.outputs().clone(),
            template.body.kernels().clone(),
            Default::default(),
            Default::default(),
        ));
        assert!(body_size(&parent) < budget);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        unconfirmed_pool.insert(parent.clone(), None, &tx_weight).unwrap();
        // Many tiny high-rate children of it: each branch (parent + child) is over the byte budget
        let parent_output = parent.body.outputs().first().unwrap().hash();
        let parent_size = body_size(&parent);
        for n in 1..=2_000u64 {
            let child = synthetic_tx(&base, n, 100_000);
            assert!(parent_size + body_size(&child) > budget);
            unconfirmed_pool
                .insert(child, Some(vec![parent_output]), &tx_weight)
                .unwrap();
        }
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();

        let selection = unconfirmed_pool.select_txs(u64::MAX, budget, Ranking::Weight).unwrap();
        for tx in &normal {
            assert!(selection.results.retrieved_transactions.contains(tx));
        }
        // Every child's branch is byte-skipped after an O(branch) size sum; conflict checks are set lookups of keys,
        // so the parent's thousands of inputs are never examined per child
        assert!(selection.byte_skips >= 2_000);
        assert_eq!(selection.conflict_drops, 0);
    }

    #[tokio::test]
    async fn a_heavy_parent_with_many_queued_children_does_not_empty_the_template() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 10, 5);
        // A parent of ~5 MB (thousands of distinct inputs), at minimal fee, that fits by weight and bytes with one
        // tiny child. Conflict checks are O(1) lookups and never charged per input; the internal-duplicate check of a
        // {child, parent} branch is charged per input, but capped per candidate (MAX_LINKS_PER_CANDIDATE), so each
        // child is dropped uncounted and cannot run any pass budget out.
        let input = base.body.inputs().first().unwrap().clone();
        let input_size = input.get_serialized_size().unwrap();
        let budget = MAX_BLOCK_TEMPLATE_BODY_BYTES;
        let template = synthetic_tx(&base, 0, 1);
        let child_size = body_size(&synthetic_tx(&base, 1, 1));
        let num_inputs = (budget - body_size(&template) - child_size - 100) / input_size;
        let parent = Arc::new(Transaction::new(
            distinct_inputs(&input, num_inputs),
            template.body.outputs().clone(),
            template.body.kernels().clone(),
            Default::default(),
            Default::default(),
        ));
        assert!(body_size(&parent) + child_size <= budget);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 1_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        unconfirmed_pool.insert(parent.clone(), None, &tx_weight).unwrap();
        // 230 tiny children with a high own rate but a low branch rate: each {child, parent} branch fits and is queued
        let parent_output = parent.body.outputs().first().unwrap().hash();
        let children = (1..=230u64)
            .map(|n| {
                let child = synthetic_tx(&base, n, 100_000);
                unconfirmed_pool
                    .insert(child.clone(), Some(vec![parent_output]), &tx_weight)
                    .unwrap();
                child
            })
            .collect::<Vec<_>>();
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();

        for ranking in [Ranking::Weight, Ranking::EffectiveWeight {
            max_block_transaction_weight: max_weight(),
            max_body_bytes: budget,
        }] {
            let selection = unconfirmed_pool.select_txs(u64::MAX, budget, ranking).unwrap();
            let selected = &selection.results.retrieved_transactions;
            // Not empty, and the pass is not ended by any budget: each {child, parent} branch is over the per-candidate
            // duplicate-check cap, so each child is dropped uncounted, and the honest transactions are selected
            for tx in &normal {
                assert!(selected.contains(tx), "{ranking:?}");
            }
            assert!(children.iter().all(|child| !selected.contains(child)), "{ranking:?}");
            assert_eq!(selection.too_many_ancestors_drops, children.len(), "{ranking:?}");
            assert!(!selection.budget_exhausted, "{ranking:?}");
            assert!(!selection.ended_early, "{ranking:?}");
        }
        let results = unconfirmed_pool
            .fetch_highest_priority_txs(u64::MAX, max_weight())
            .unwrap();
        assert!(!results.retrieved_transactions.is_empty());
    }

    #[tokio::test]
    async fn many_conflicting_spends_of_one_output_are_handled_in_linear_time() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let input = base.body.inputs().first().unwrap().clone();
        const COUNT: u64 = 40_000;
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: usize::try_from(COUNT).unwrap(),
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        for n in 0..COUNT {
            let tx = synthetic_tx(&base, n, 1_000);
            let spend = Arc::new(Transaction::new(
                vec![input.clone()],
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ));
            unconfirmed_pool.insert(spend, None, &tx_weight).unwrap();
        }
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        assert_eq!(selection.results.retrieved_transactions.len(), 1);
        assert_eq!(selection.conflict_drops, usize::try_from(COUNT).unwrap() - 1);
    }

    #[tokio::test]
    async fn the_spent_output_index_follows_inserts_and_removals() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let tx1 = Arc::new(
            tx!(MicroMinotari(500_000), fee: MicroMinotari(5), inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        // A second transaction spending one of tx1's inputs
        let tx2 = {
            let tx = synthetic_tx(&base, 0, 10);
            Arc::new(Transaction::new(
                vec![tx1.body.inputs()[0].clone()],
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        };
        let spent = tx1
            .body
            .inputs()
            .iter()
            .map(|input| input.output_hash())
            .collect::<Vec<_>>();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        unconfirmed_pool.insert(tx1.clone(), None, &tx_weight).unwrap();
        unconfirmed_pool.insert(tx2.clone(), None, &tx_weight).unwrap();
        let key_of = |pool: &UnconfirmedPool, tx: &Arc<Transaction>| {
            *pool.tx_by_key.iter().find(|(_, p)| &p.transaction == tx).unwrap().0
        };
        let (key1, key2) = (key_of(&unconfirmed_pool, &tx1), key_of(&unconfirmed_pool, &tx2));
        assert_eq!(unconfirmed_pool.txs_by_spent_output[&spent[0]], vec![key1, key2]);
        assert_eq!(unconfirmed_pool.txs_by_spent_output[&spent[1]], vec![key1]);
        assert!(unconfirmed_pool.check_data_consistency());

        unconfirmed_pool.remove_transaction(key1).unwrap();
        assert_eq!(unconfirmed_pool.txs_by_spent_output[&spent[0]], vec![key2]);
        assert!(!unconfirmed_pool.txs_by_spent_output.contains_key(&spent[1]));
        unconfirmed_pool.remove_transaction(key2).unwrap();
        assert!(unconfirmed_pool.txs_by_spent_output.is_empty());
        assert!(unconfirmed_pool.check_data_consistency());

        unconfirmed_pool.insert(tx1, None, &tx_weight).unwrap();
        let _drained = unconfirmed_pool.drain_all_mempool_transactions();
        assert!(unconfirmed_pool.txs_by_spent_output.is_empty());
    }

    #[tokio::test]
    async fn the_candidate_the_walking_budget_runs_out_on_is_not_excluded() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        // Junk: conflicting children of a 99-deep chain, exactly enough (100 visited transactions each) to use up the
        // walking budget
        let depth = u64::try_from(MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE).unwrap() - 1;
        let chain = insert_chain(&mut unconfirmed_pool, &base, 0, depth, 1);
        let input = base.body.inputs().first().unwrap().clone();
        let num_children = u64::try_from(MAX_WALKED_FRAMES_PER_PASS).unwrap() / (depth + 1);
        insert_children(
            &mut unconfirmed_pool,
            &base,
            chain.last().unwrap(),
            &input,
            depth,
            num_children,
            100_000,
        );
        // The target: an honest transaction ranked right after the junk children
        let target = Arc::new(
            tx!(MicroMinotari(500_000), fee: MicroMinotari(50), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        unconfirmed_pool.insert(target.clone(), None, &tx_weight).unwrap();
        let target_key = *unconfirmed_pool
            .tx_by_key
            .iter()
            .find(|(_, p)| p.transaction == target)
            .unwrap()
            .0;

        let by_weight = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        assert!(by_weight.budget_exhausted);
        assert_eq!(by_weight.out_of_frames_candidate, Some(target_key));
        assert!(!by_weight.results.retrieved_transactions.contains(&target));
        // It is not excluded from the second pass (the junk children dropped in the drain are, since the first pass
        // died of budget exhaustion) ...
        assert!(by_weight.budget_exhausted);
        assert!(!by_weight.walked_dropped.contains(&target_key));
        assert!(!by_weight.state_dropped.contains(&target_key));
        assert!(!UnconfirmedPool::second_pass_exclusions(&by_weight).contains(&target_key));
        // ... where it is selected
        let by_effective_weight = unconfirmed_pool
            .select_txs_with(
                u64::MAX,
                usize::MAX / 2,
                Ranking::EffectiveWeight {
                    max_block_transaction_weight: max_weight(),
                    max_body_bytes: usize::MAX / 2,
                },
                max_weight(),
                UnconfirmedPool::second_pass_exclusions(&by_weight),
                &TemplateFilter::unrestricted(),
            )
            .unwrap();
        assert!(by_effective_weight.results.retrieved_transactions.contains(&target));
    }

    #[tokio::test]
    async fn duplicate_producers_of_an_output_are_resolved_in_constant_time() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        // 5,000 pool transactions producing the very same output, with different fees
        let output = base.body.outputs().first().unwrap().clone();
        let producers = (0..5_000u64)
            .map(|n| {
                let tx = synthetic_tx(&base, n, 1 + n % 97);
                let producer = Arc::new(Transaction::new(
                    vec![],
                    vec![output.clone()],
                    tx.body.kernels().clone(),
                    Default::default(),
                    Default::default(),
                ));
                unconfirmed_pool.insert(producer.clone(), None, &tx_weight).unwrap();
                producer
            })
            .collect::<Vec<_>>();
        // The producers of the output are ordered by priority, highest first
        let keys = &unconfirmed_pool.txs_by_output[&output.hash()];
        assert_eq!(keys.len(), producers.len());
        assert!(keys.windows(2).all(|pair| {
            unconfirmed_pool.tx_by_key[&pair[0]].priority > unconfirmed_pool.tx_by_key[&pair[1]].priority
        }));
        let highest = unconfirmed_pool
            .tx_by_key
            .values()
            .max_by(|a, b| a.priority.cmp(&b.priority))
            .unwrap()
            .key;
        assert_eq!(keys[0], highest);

        // A candidate spending that output resolves its dependency without scanning the producers
        let candidate = synthetic_tx(&base, 10_000, 1_000_000);
        unconfirmed_pool
            .insert(candidate.clone(), Some(vec![output.hash()]), &tx_weight)
            .unwrap();
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        assert!(selection.results.retrieved_transactions.contains(&candidate));
        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn every_dependency_link_followed_costs_walking_work() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        let parent = insert_chain(&mut unconfirmed_pool, &base, 0, 1, 1).pop().unwrap();
        // A candidate with MAX_LINKS_PER_CANDIDATE links (e.g. one per input) to the same parent: one transaction, but
        // many links
        // (less 4 for the duplicate check of the {candidate, parent} branch: one output and one kernel each)
        const LINKS: usize = MAX_LINKS_PER_CANDIDATE - 4;
        let candidate = synthetic_tx(&base, 1, 1_000_000);
        unconfirmed_pool
            .insert(
                candidate.clone(),
                Some(vec![parent.body.outputs().first().unwrap().hash(); LINKS]),
                &tx_weight,
            )
            .unwrap();
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        assert!(selection.results.retrieved_transactions.contains(&candidate));
        // The links are charged (once: by the pre-walk bound, not again by the walk), while only two transactions are
        // visited
        assert!(selection.links_followed >= LINKS, "{}", selection.links_followed);
        assert!(selection.links_followed < 2 * LINKS, "{}", selection.links_followed);
        assert!(selection.links_followed <= MAX_FOLLOWED_LINKS_PER_PASS);
        assert!(selection.walk_work <= 3);
    }

    #[tokio::test]
    async fn running_out_of_link_budget_ends_the_pass_but_drains_the_queue() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        let parent = insert_chain(&mut unconfirmed_pool, &base, 0, 1, 1).pop().unwrap();
        let parent_output = parent.body.outputs().first().unwrap().hash();
        // Candidates each with (nearly) MAX_LINKS_PER_CANDIDATE links to the parent, so the link budget runs out after
        // ~2,000 of them. Decreasing fees fix the order; each has a high own
        // rate but a low branch rate, so the branches are queued rather than selected as they are found.
        // (less 4 for the duplicate check of the {candidate, parent} branch: one output and one kernel each)
        const LINKS: usize = MAX_LINKS_PER_CANDIDATE - 4;
        // The pre-walk bound pays for the candidate's links (the walk does not charge them again), plus 4 for the
        // branch's internal-duplicate check (one output and one kernel each for the candidate and the parent)
        let per_candidate = LINKS + 4;
        let num_candidates = MAX_FOLLOWED_LINKS_PER_PASS / per_candidate + 4;
        let candidates = (0..u64::try_from(num_candidates).unwrap())
            .map(|n| {
                let candidate = synthetic_tx(&base, 1 + n, 1_000_000 - n);
                unconfirmed_pool
                    .insert(candidate.clone(), Some(vec![parent_output; LINKS]), &tx_weight)
                    .unwrap();
                candidate
            })
            .collect::<Vec<_>>();
        let key_of = |tx: &Arc<Transaction>| {
            *unconfirmed_pool
                .tx_by_key
                .iter()
                .find(|(_, p)| &p.transaction == tx)
                .unwrap()
                .0
        };

        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        assert!(selection.ended_early);
        assert!(selection.byte_bound);
        assert!(selection.links_followed <= MAX_FOLLOWED_LINKS_PER_PASS);
        // It runs out in the pre-walk ancestor weight bound of the first candidate whose links do not fit
        let exhausted_on = MAX_FOLLOWED_LINKS_PER_PASS / per_candidate;
        let out_of_links = selection
            .out_of_frames_candidate
            .expect("no candidate ran out of links");
        assert_eq!(out_of_links, key_of(&candidates[exhausted_on]));
        assert!(!selection.walked_dropped.contains(&out_of_links));
        // Everything queued before that is still drained and selected
        let selected = &selection.results.retrieved_transactions;
        assert!(selected.contains(&parent));
        for candidate in candidates.iter().take(exhausted_on) {
            assert!(selected.contains(candidate));
        }
        assert!(!selected.contains(&candidates[exhausted_on]));
    }

    #[tokio::test]
    async fn removing_the_best_producer_of_an_output_leaves_the_next_best() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let output = base.body.outputs().first().unwrap().clone();
        let producer = |n: u64, fee: u64| {
            let tx = synthetic_tx(&base, n, fee);
            Arc::new(Transaction::new(
                vec![],
                vec![output.clone()],
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        };
        let (low, high) = (producer(0, 10), producer(1, 1_000));
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        unconfirmed_pool.insert(low.clone(), None, &tx_weight).unwrap();
        unconfirmed_pool.insert(high.clone(), None, &tx_weight).unwrap();
        let best = |pool: &UnconfirmedPool| {
            pool.find_highest_priority_transaction(&pool.txs_by_output[&output.hash()])
                .unwrap()
                .transaction
                .clone()
        };
        assert_eq!(best(&unconfirmed_pool), high);
        let high_key = *unconfirmed_pool
            .tx_by_key
            .iter()
            .find(|(_, p)| p.transaction == high)
            .unwrap()
            .0;
        unconfirmed_pool.remove_transaction(high_key).unwrap();
        assert_eq!(best(&unconfirmed_pool), low);
        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn candidates_with_too_many_links_are_dropped() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        let parent = insert_chain(&mut unconfirmed_pool, &base, 0, 1, 1).pop().unwrap();
        let parent_output = parent.body.outputs().first().unwrap().hash();
        let at_cap = synthetic_tx(&base, 1, 1_000_000);
        unconfirmed_pool
            .insert(
                at_cap.clone(),
                // At the cap, including the duplicate check of the {candidate, parent} branch (one output and one
                // kernel each)
                Some(vec![parent_output; MAX_LINKS_PER_CANDIDATE - 4]),
                &tx_weight,
            )
            .unwrap();
        let over_cap = synthetic_tx(&base, 2, 2_000_000);
        unconfirmed_pool
            .insert(
                over_cap.clone(),
                Some(vec![parent_output; MAX_LINKS_PER_CANDIDATE + 1]),
                &tx_weight,
            )
            .unwrap();
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        let selected = &selection.results.retrieved_transactions;
        assert!(!selected.contains(&over_cap));
        assert!(selected.contains(&at_cap));
        assert!(selected.contains(&parent));
        assert_eq!(selection.too_many_ancestors_drops, 1);
        assert_eq!(selection.weight_skips_counted, 0);
    }

    #[tokio::test]
    async fn a_branch_that_double_spends_internally_is_never_selected() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let x = normal_txs(&key_manager, 1, 5)
            .pop()
            .unwrap()
            .body
            .inputs()
            .first()
            .unwrap()
            .clone();
        let spend_x = |n: u64, fee: u64| {
            let tx = synthetic_tx(&base, n, fee);
            Arc::new(Transaction::new(
                vec![x.clone()],
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        };
        let (p1, p2) = (spend_x(0, 1), spend_x(1, 2));
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        unconfirmed_pool.insert(p1.clone(), None, &tx_weight).unwrap();
        unconfirmed_pool.insert(p2.clone(), None, &tx_weight).unwrap();
        // A high-rate child depending on both: its branch {child, P1, P2} spends X twice
        let child = synthetic_tx(&base, 2, 1_000_000);
        unconfirmed_pool
            .insert(
                child.clone(),
                Some(vec![
                    p1.body.outputs().first().unwrap().hash(),
                    p2.body.outputs().first().unwrap().hash(),
                ]),
                &tx_weight,
            )
            .unwrap();

        for ranking in [Ranking::Weight, Ranking::EffectiveWeight {
            max_block_transaction_weight: max_weight(),
            max_body_bytes: usize::MAX / 2,
        }] {
            let selection = unconfirmed_pool.select_txs(u64::MAX, usize::MAX / 2, ranking).unwrap();
            let selected = &selection.results.retrieved_transactions;
            assert!(!selected.contains(&child), "{ranking:?}");
            assert!(!(selected.contains(&p1) && selected.contains(&p2)), "{ranking:?}");
            assert!(selected.contains(&p1) || selected.contains(&p2), "{ranking:?}");
            assert_eq!(selection.invalid_branch_drops, 1, "{ranking:?}");
            assert_eq!(selection.swept_branches, 0, "{ranking:?}");
        }
    }

    #[tokio::test]
    async fn transactions_with_the_same_output_or_commitment_are_not_both_selected() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        let with_output = |n: u64, output: &tari_transaction_components::transaction_components::TransactionOutput| {
            let tx = synthetic_tx(&base, n, 100 + n);
            Arc::new(Transaction::new(
                vec![],
                vec![output.clone()],
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        };
        // The same output twice
        let output = synthetic_tx(&base, 100, 1).body.outputs()[0].clone();
        let (a, b) = (with_output(0, &output), with_output(1, &output));
        // Different outputs (different hashes) with the same commitment
        let mut other = output.clone();
        other.features.maturity = 12345;
        assert_ne!(other.hash(), output.hash());
        let c = with_output(2, &other);
        for tx in [&a, &b, &c] {
            unconfirmed_pool.insert(tx.clone(), None, &tx_weight).unwrap();
        }
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        let selected = &selection.results.retrieved_transactions;
        assert_eq!(selected.iter().filter(|tx| [&a, &b, &c].contains(tx)).count(), 1);
        assert_eq!(selection.conflict_drops, 2);
        assert_eq!(selection.swept_branches, 0);
        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn the_final_sweep_drops_invalid_branches_and_their_dependants() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let x = normal_txs(&key_manager, 1, 5)
            .pop()
            .unwrap()
            .body
            .inputs()
            .first()
            .unwrap()
            .clone();
        let spend_x = |n: u64| {
            let tx = synthetic_tx(&base, n, 10);
            Arc::new(Transaction::new(
                vec![x.clone()],
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        };
        let (first, second) = (spend_x(0), spend_x(1));
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        unconfirmed_pool.insert(first.clone(), None, &tx_weight).unwrap();
        unconfirmed_pool.insert(second.clone(), None, &tx_weight).unwrap();
        // A child of the second
        let child = synthetic_tx(&base, 2, 10);
        unconfirmed_pool
            .insert(child.clone(), Some(vec![second.body.outputs()[0].hash()]), &tx_weight)
            .unwrap();
        let independent = synthetic_tx(&base, 3, 10);
        unconfirmed_pool.insert(independent.clone(), None, &tx_weight).unwrap();
        let key_of = |tx: &Arc<Transaction>| {
            *unconfirmed_pool
                .tx_by_key
                .iter()
                .find(|(_, p)| &p.transaction == tx)
                .unwrap()
                .0
        };
        // A hand-built invalid selection: both spends of X, the second one's child, and an independent transaction
        let branches = vec![vec![key_of(&first)], vec![key_of(&second)], vec![key_of(&child)], vec![
            key_of(&independent),
        ]];
        let (keep, dropped) = unconfirmed_pool.sweep_selection(&branches).unwrap();
        assert_eq!(dropped, 2);
        assert_eq!(
            keep,
            [key_of(&first), key_of(&independent)]
                .into_iter()
                .collect::<HashSet<_>>()
        );
        // A valid selection passes unchanged
        let (keep, dropped) = unconfirmed_pool
            .sweep_selection(&[vec![key_of(&second), key_of(&child)], vec![key_of(&independent)]])
            .unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(keep.len(), 3);
    }

    /// Two parents spending the same `num_inputs` outputs (a double-spend), and `num_children` tiny high-rate children
    /// each depending on both, plus honest lower-rate transactions
    async fn double_spending_parents_with_children(
        num_inputs: usize,
        num_children: u64,
    ) -> (UnconfirmedPool, Vec<Arc<Transaction>>, Vec<Arc<Transaction>>) {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 10, 5);
        let inputs = distinct_inputs(base.body.inputs().first().unwrap(), num_inputs);
        let parent = |n: u64, fee: u64| {
            let tx = synthetic_tx(&base, n, fee);
            Arc::new(Transaction::new(
                inputs.clone(),
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        };
        let (p_a, p_b) = (parent(0, 1), parent(1, 2));
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 1_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        unconfirmed_pool.insert(p_a.clone(), None, &tx_weight).unwrap();
        unconfirmed_pool.insert(p_b.clone(), None, &tx_weight).unwrap();
        let dependencies = vec![p_a.body.outputs()[0].hash(), p_b.body.outputs()[0].hash()];
        let children = (0..num_children)
            .map(|n| {
                let child = synthetic_tx(&base, n.saturating_add(10), 1_000_000);
                unconfirmed_pool
                    .insert(child.clone(), Some(dependencies.clone()), &tx_weight)
                    .unwrap();
                child
            })
            .collect::<Vec<_>>();
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();
        (unconfirmed_pool, children, normal)
    }

    #[tokio::test]
    async fn children_of_double_spending_many_input_parents_do_not_end_the_pass() {
        // Each {child, P_a, P_b} branch costs more than MAX_LINKS_PER_CANDIDATE to check: dropped uncounted, uncharged
        let (unconfirmed_pool, children, normal) = double_spending_parents_with_children(2_000, 300).await;
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        assert!(!selection.ended_early);
        assert_eq!(selection.too_many_ancestors_drops, children.len());
        let selected = &selection.results.retrieved_transactions;
        for tx in &normal {
            assert!(selected.contains(tx));
        }
        assert!(children.iter().all(|child| !selected.contains(child)));
        assert!(selection.links_followed < MAX_FOLLOWED_LINKS_PER_PASS / 10);
    }

    #[tokio::test]
    async fn colliding_pairs_found_by_the_duplicate_check_are_remembered_for_the_pass() {
        // Small enough to check: the first child's branch is found to double-spend, the colliding pair of parents is
        // remembered, and every later child's branch (which contains both) is dropped without checking again
        let (unconfirmed_pool, children, normal) = double_spending_parents_with_children(100, 300).await;
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        assert!(!selection.ended_early);
        assert_eq!(selection.invalid_branch_drops, 1);
        assert_eq!(selection.pair_drops, children.len() - 1);
        let selected = &selection.results.retrieved_transactions;
        for tx in &normal {
            assert!(selected.contains(tx));
        }
        assert_eq!(selection.swept_branches, 0);
    }

    /// A synthetic transaction with the given kernels (and a fresh unique output)
    fn with_kernels(
        base: &Transaction,
        n: u64,
        kernels: Vec<tari_transaction_components::transaction_components::TransactionKernel>,
    ) -> Arc<Transaction> {
        let tx = synthetic_tx(base, n, 1);
        Arc::new(Transaction::new(
            vec![],
            tx.body.outputs().clone(),
            kernels,
            Default::default(),
            Default::default(),
        ))
    }

    #[tokio::test]
    async fn a_newcomer_at_the_spend_cap_must_have_a_strict_subset_of_every_spenders_kernels() {
        use tari_common_types::types::CompressedSignature;
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let kernel = |n: u64| synthetic_tx(&base, n, 10).body.kernels()[0].clone();
        let spend = |n: u64, kernels| {
            Arc::new(Transaction::new(
                base.body.inputs().clone(),
                synthetic_tx(&base, n, 1).body.outputs().clone(),
                kernels,
                Default::default(),
                Default::default(),
            ))
        };
        let (owner, other) = (kernel(0), kernel(1));
        // A distinct kernel with the same signature scalar as the owner's (another nonce)
        let mut owner_again = owner.clone();
        owner_again.excess_sig = CompressedSignature::new(
            crate::test_helpers::new_public_key(),
            owner.excess_sig.get_signature().clone(),
        );
        assert_ne!(owner_again, owner);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        for n in 0..u64::try_from(MAX_SPENDS_PER_OUTPUT).unwrap() {
            let kernels = vec![owner.clone(), owner_again.clone(), kernel(3 + n)];
            unconfirmed_pool
                .insert(spend(10 + n, kernels), None, &tx_weight)
                .unwrap();
        }
        // `other` is in the pool, but in none of the spenders: each has the owner's signature twice, which counts once
        let other_input = distinct_inputs(&base.body.inputs()[0], 1).pop().unwrap();
        unconfirmed_pool
            .insert(
                Arc::new(Transaction::new(
                    vec![other_input],
                    synthetic_tx(&base, 50, 1).body.outputs().clone(),
                    vec![other.clone(), kernel(3)],
                    Default::default(),
                    Default::default(),
                )),
                None,
                &tx_weight,
            )
            .unwrap();
        assert!(
            unconfirmed_pool
                .conflict_set_victims(&spend(100, vec![owner.clone(), other]))
                .is_none()
        );
        // The owner's own transaction replaces one spender
        assert_eq!(
            unconfirmed_pool
                .conflict_set_victims(&spend(101, vec![owner.clone()]))
                .unwrap()
                .len(),
            1
        );

        // A newcomer must be a strict subset of every spender, not only of most of them: `{o, a}` is one of seven
        // spenders `{o, a, x_i}` but not of `{o, b}`
        let (o, a, b) = (kernel(0), kernel(1), kernel(2));
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        for n in 0..u64::try_from(MAX_SPENDS_PER_OUTPUT - 1).unwrap() {
            unconfirmed_pool
                .insert(spend(10 + n, vec![o.clone(), a.clone(), kernel(3)]), None, &tx_weight)
                .unwrap();
        }
        unconfirmed_pool
            .insert(spend(20, vec![o.clone(), b]), None, &tx_weight)
            .unwrap();
        assert!(unconfirmed_pool.conflict_set_victims(&spend(100, vec![o, a])).is_none());
    }

    /// Spends of several inputs at the spend cap: `(a)` the same spenders on both, `(b)` different spenders, `(c)` the
    /// victim chosen for one input also spends the other, whose other spenders the newcomer is no subset of, and `(d)`
    /// a newcomer that is no subset of the spenders of one of them
    #[tokio::test]
    async fn a_newcomer_spending_several_capped_inputs_evicts_the_lowest_spender_of_each() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let inputs = distinct_inputs(&base.body.inputs()[0], 2);
        let (a, b) = (inputs[0].clone(), inputs[1].clone());
        let kernel = |fee: u64| synthetic_tx(&base, 0, fee).body.kernels()[0].clone();
        let owner = kernel(1);
        let spend = |inputs: Vec<_>, kernels| {
            Arc::new(Transaction::new(
                inputs,
                synthetic_tx(&base, 0, 1).body.outputs().clone(),
                kernels,
                Default::default(),
                Default::default(),
            ))
        };
        // `count` spenders of `inputs`, each with a kernel paying a fee rising from `first_fee`, and the owner's kernel
        // if `with_owner`; returns their keys, lowest fee first
        let fill = |pool: &mut UnconfirmedPool, inputs: Vec<_>, with_owner: bool, count: usize, first_fee: u64| {
            let mut keys = Vec::new();
            for fee in (0..count).map(|i| first_fee + 100 * u64::try_from(i).unwrap()) {
                let kernels = if with_owner {
                    vec![owner.clone(), kernel(fee)]
                } else {
                    vec![kernel(fee)]
                };
                let tx = spend(inputs.clone(), kernels);
                pool.insert(tx.clone(), None, &tx_weight).unwrap();
                keys.push(*pool.tx_by_key.iter().find(|(_, p)| p.transaction == tx).unwrap().0);
            }
            keys
        };
        let new_pool = || UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        let spenders =
            |pool: &UnconfirmedPool, input: &tari_transaction_components::transaction_components::TransactionInput| {
                pool.txs_by_spent_output[&input.output_hash()].len()
            };
        let newcomer = spend(vec![a.clone(), b.clone()], vec![owner.clone()]);

        // (a)
        let mut pool = new_pool();
        let keys = fill(
            &mut pool,
            vec![a.clone(), b.clone()],
            true,
            MAX_SPENDS_PER_OUTPUT,
            1_000,
        );
        let victims = pool.conflict_set_victims(&newcomer).unwrap();
        assert_eq!(victims, vec![keys[0]]);
        pool.remove_transactions(victims).unwrap();
        pool.insert(newcomer.clone(), None, &tx_weight).unwrap();
        assert_eq!(spenders(&pool, &a), MAX_SPENDS_PER_OUTPUT);
        assert_eq!(spenders(&pool, &b), MAX_SPENDS_PER_OUTPUT);

        // (a), with the lowest fee inserted last: it is still the one replaced
        let mut pool = new_pool();
        let keys = (0..MAX_SPENDS_PER_OUTPUT)
            .rev()
            .flat_map(|i| {
                let fee = 1_000 + 100 * u64::try_from(i).unwrap();
                fill(&mut pool, vec![a.clone(), b.clone()], true, 1, fee)
            })
            .collect::<Vec<_>>();
        assert_eq!(pool.conflict_set_victims(&newcomer).unwrap(), vec![
            *keys.last().unwrap()
        ]);

        // (b)
        let mut pool = new_pool();
        let a_keys = fill(&mut pool, vec![a.clone()], true, MAX_SPENDS_PER_OUTPUT, 1_000);
        let b_keys = fill(&mut pool, vec![b.clone()], true, MAX_SPENDS_PER_OUTPUT, 1_000);
        assert_eq!(pool.conflict_set_victims(&newcomer).unwrap(), vec![
            a_keys[0], b_keys[0]
        ]);

        // (c)
        let mut pool = new_pool();
        let shared = fill(&mut pool, vec![a.clone(), b.clone()], true, 1, 100);
        fill(&mut pool, vec![a.clone()], true, MAX_SPENDS_PER_OUTPUT - 1, 1_000);
        fill(&mut pool, vec![b.clone()], false, MAX_SPENDS_PER_OUTPUT - 1, 1_000);
        assert_eq!(pool.conflict_set_victims(&newcomer).unwrap(), shared);

        // (d), with the input it is no subset of spent first or second
        for no_subset_first in [true, false] {
            let mut pool = new_pool();
            fill(&mut pool, vec![a.clone()], true, MAX_SPENDS_PER_OUTPUT, 1_000);
            fill(&mut pool, vec![b.clone()], false, MAX_SPENDS_PER_OUTPUT, 1_000);
            let inputs = if no_subset_first {
                vec![b.clone(), a.clone()]
            } else {
                vec![a.clone(), b.clone()]
            };
            assert!(
                pool.conflict_set_victims(&spend(inputs, vec![owner.clone()])).is_none(),
                "{no_subset_first}"
            );
        }
    }

    /// A capped spender set of an honest aggregate `{t1, t2}` and copies of it padded with a kernel each (promising
    /// higher fees), and sets whose replaced spender produces an output the newcomer also spends
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn the_spender_replaced_at_the_cap_is_the_largest_unless_that_orphans_the_newcomer() {
        use tari_transaction_components::transaction_components::TransactionInput;
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let input = base.body.inputs()[0].clone();
        let kernel = |fee: u64| synthetic_tx(&base, 0, fee).body.kernels()[0].clone();
        let spend = |inputs: Vec<TransactionInput>, n: u64, kernels| {
            Arc::new(Transaction::new(
                inputs,
                synthetic_tx(&base, n, 1).body.outputs().clone(),
                kernels,
                Default::default(),
                Default::default(),
            ))
        };
        let key_of = |pool: &UnconfirmedPool, tx: &Arc<Transaction>| {
            *pool.tx_by_key.iter().find(|(_, p)| &p.transaction == tx).unwrap().0
        };

        let (t1, t2) = (kernel(1), kernel(5_000));
        let mut pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        let aggregate = spend(vec![input.clone()], 0, vec![t1.clone(), t2.clone()]);
        pool.insert(aggregate.clone(), None, &tx_weight).unwrap();
        let mut padded = Vec::new();
        for i in 1..u64::try_from(MAX_SPENDS_PER_OUTPUT).unwrap() {
            let tx = spend(vec![input.clone()], i, vec![
                t1.clone(),
                t2.clone(),
                kernel(100_000 * i),
            ]);
            pool.insert(tx.clone(), None, &tx_weight).unwrap();
            padded.push(key_of(&pool, &tx));
        }
        let victims = pool
            .conflict_set_victims(&spend(vec![input.clone()], 100, vec![t1.clone()]))
            .unwrap();
        assert_eq!(victims.len(), 1);
        assert!(padded.contains(&victims[0]));

        let output = |n: u64| synthetic_tx(&base, n, 1).body.outputs()[0].clone();
        let input_of = |output: &tari_transaction_components::transaction_components::TransactionOutput| {
            TransactionInput::new_with_output_hash(output.hash(), Default::default(), Default::default())
        };
        let tx = |inputs: Vec<TransactionInput>, outputs, kernels| {
            Arc::new(Transaction::new(
                inputs,
                outputs,
                kernels,
                Default::default(),
                Default::default(),
            ))
        };
        // Stores `tx` as the mempool does: None if it is not stored, else the spenders it replaced
        let admit = |pool: &mut UnconfirmedPool, tx: &Arc<Transaction>| {
            let victims = pool.conflict_set_victims(tx)?;
            if pool.would_orphan(tx, &victims.iter().copied().collect()) {
                return None;
            }
            pool.remove_transactions(victims.clone()).unwrap();
            pool.insert(tx.clone(), None, &tx_weight).unwrap();
            Some(victims)
        };
        let honest = kernel(1);

        // The replaced spender `v` is the only producer of an output `y` the newcomer also spends
        let mut pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        let y = output(0);
        let v = tx(vec![input.clone()], vec![y.clone()], vec![
            honest.clone(),
            kernel(1),
            kernel(1),
        ]);
        pool.insert(v.clone(), None, &tx_weight).unwrap();
        for i in 1..u64::try_from(MAX_SPENDS_PER_OUTPUT).unwrap() {
            pool.insert(
                spend(vec![input.clone()], i, vec![honest.clone(), kernel(1)]),
                None,
                &tx_weight,
            )
            .unwrap();
        }
        let orphaned = spend(vec![input.clone(), input_of(&y)], 100, vec![honest.clone()]);
        assert!(admit(&mut pool, &orphaned).is_none());
        assert_eq!(pool.len(), MAX_SPENDS_PER_OUTPUT);
        assert!(pool.tx_by_key.values().any(|p| p.transaction == v));

        // Copies `p + h + k_i` of a parent `p` and its child `h` each spend and produce `u`; `h` replaces one, since
        // the others still produce `u`
        let mut pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        let (u, h_output, p) = (output(0), output(1), kernel(1));
        let h = tx(vec![input_of(&u)], vec![h_output.clone()], vec![honest.clone()]);
        for _ in 0..MAX_SPENDS_PER_OUTPUT {
            let copy = tx(
                vec![input.clone(), input_of(&u)],
                vec![u.clone(), h_output.clone()],
                vec![p.clone(), honest.clone(), kernel(1)],
            );
            pool.insert(copy, None, &tx_weight).unwrap();
        }
        assert_eq!(admit(&mut pool, &h).unwrap().len(), 1);
        assert_eq!(pool.txs_by_output[&u.hash()].len(), MAX_SPENDS_PER_OUTPUT - 1);
        assert!(pool.check_data_consistency());

        // `h` spends `x` and the outputs `o1` and `o2` of pool parents `p1` and `p2`, and copies `p1 + h + x_i` and
        // `p2 + h + x_j` fill the spenders of `x`; `h` replaces one, since `p1` and `p2` stay
        let mut pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        let chain_inputs = distinct_inputs(&input, 3);
        let (x, p1_input, p2_input) = (&chain_inputs[0], &chain_inputs[1], &chain_inputs[2]);
        let (o1, o2, h_output) = (output(0), output(1), output(2));
        let (k1, k2) = (kernel(1), kernel(1));
        let p1 = tx(vec![p1_input.clone()], vec![o1.clone()], vec![k1.clone()]);
        let p2 = tx(vec![p2_input.clone()], vec![o2.clone()], vec![k2.clone()]);
        let h = tx(
            vec![x.clone(), input_of(&o1), input_of(&o2)],
            vec![h_output.clone()],
            vec![honest.clone()],
        );
        for (parent, parent_input, parent_output, parent_kernel) in
            [(&p1, p1_input, &o1, &k1), (&p2, p2_input, &o2, &k2)]
        {
            pool.insert(parent.clone(), None, &tx_weight).unwrap();
            for _ in 0..MAX_SPENDS_PER_OUTPUT / 2 {
                let x_i = output(3);
                let copy = tx(
                    [vec![parent_input.clone()], h.body.inputs().clone(), vec![input_of(
                        &x_i,
                    )]]
                    .concat(),
                    vec![parent_output.clone(), h_output.clone()],
                    vec![parent_kernel.clone(), honest.clone(), kernel(1)],
                );
                pool.insert(copy, None, &tx_weight).unwrap();
            }
        }
        assert_eq!(pool.txs_by_spent_output[&x.output_hash()].len(), MAX_SPENDS_PER_OUTPUT);
        assert_eq!(admit(&mut pool, &h).unwrap().len(), 1);
        for parent in [&p1, &p2] {
            assert!(pool.tx_by_key.values().any(|p| &p.transaction == parent));
        }
        assert!(pool.check_data_consistency());
    }

    #[tokio::test]
    async fn transactions_sharing_a_kernel_or_excess_are_not_both_selected() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let kernel = |n: u64| synthetic_tx(&base, n, 10 + n).body.kernels()[0].clone();
        // Same kernel signature: T1 = {K1}, T2 = {K1, K2} (T2 is admitted, since not all its kernels are pooled)
        let (k1, k2) = (kernel(0), kernel(1));
        let t1 = with_kernels(&base, 10, vec![k1.clone()]);
        let t2 = with_kernels(&base, 11, vec![k1.clone(), k2]);
        // Same excess, different signature (nonce): sorts as a distinct kernel, but the excess must be unique
        let k3 = kernel(2);
        let mut k3_again = k3.clone();
        k3_again.excess_sig = kernel(3).excess_sig;
        assert_eq!(k3.excess, k3_again.excess);
        assert_ne!(k3.excess_sig, k3_again.excess_sig);
        let t3 = with_kernels(&base, 12, vec![k3]);
        let t4 = with_kernels(&base, 13, vec![k3_again]);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        for tx in [&t1, &t2, &t3, &t4] {
            unconfirmed_pool.insert(tx.clone(), None, &tx_weight).unwrap();
        }
        assert_eq!(unconfirmed_pool.tx_by_key.len(), 4);
        assert!(unconfirmed_pool.check_data_consistency());
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        let selected = &selection.results.retrieved_transactions;
        assert_eq!(selected.iter().filter(|tx| [&t1, &t2].contains(tx)).count(), 1);
        assert_eq!(selected.iter().filter(|tx| [&t3, &t4].contains(tx)).count(), 1);
        assert_eq!(selection.swept_branches, 0);

        // The final sweep catches a hand-built selection with a duplicate kernel or excess
        let key_of = |tx: &Arc<Transaction>| {
            *unconfirmed_pool
                .tx_by_key
                .iter()
                .find(|(_, p)| &p.transaction == tx)
                .unwrap()
                .0
        };
        let (keep, dropped) = unconfirmed_pool
            .sweep_selection(&[vec![key_of(&t1)], vec![key_of(&t2)], vec![key_of(&t3)], vec![key_of(
                &t4,
            )]])
            .unwrap();
        assert_eq!(dropped, 2);
        assert_eq!(keep, [key_of(&t1), key_of(&t3)].into_iter().collect::<HashSet<_>>());
    }

    #[tokio::test]
    async fn publishing_a_block_removes_transactions_sharing_its_commitments_or_excesses() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let consensus = create_consensus_rules();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        // A mined transaction X with an output commitment C and a kernel excess E
        let mined = synthetic_tx(&base, 0, 10);
        // A pool transaction producing a different output (a different hash) with the same commitment C
        let mut same_commitment_output = mined.body.outputs()[0].clone();
        same_commitment_output.features.maturity += 1;
        assert_ne!(same_commitment_output.hash(), mined.body.outputs()[0].hash());
        let same_commitment = Arc::new(Transaction::new(
            vec![],
            vec![same_commitment_output],
            synthetic_tx(&base, 1, 10).body.kernels().clone(),
            Default::default(),
            Default::default(),
        ));
        // A pool transaction with a kernel with the same excess E (but a different signature)
        let mut same_excess_kernel = mined.body.kernels()[0].clone();
        same_excess_kernel.excess_sig = synthetic_tx(&base, 2, 10).body.kernels()[0].excess_sig.clone();
        let same_excess = with_kernels(&base, 3, vec![same_excess_kernel]);
        let unrelated = synthetic_tx(&base, 4, 10);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        for tx in [&same_commitment, &same_excess, &unrelated] {
            unconfirmed_pool.insert(tx.clone(), None, &tx_weight).unwrap();
        }

        let published_block = create_orphan_block(0, vec![(*mined).clone()], &consensus);
        let removed = unconfirmed_pool
            .remove_published_and_discard_deprecated_transactions(&published_block)
            .unwrap();
        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&same_commitment));
        assert!(removed.contains(&same_excess));
        assert_eq!(unconfirmed_pool.tx_by_key.len(), 1);
        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn publishing_a_block_matches_burns_by_output_commitment_only() {
        use tari_transaction_components::transaction_components::{KernelFeatures, OutputFeatures};
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let consensus = create_consensus_rules();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let burn_tx = |n: u64| {
            let tx = synthetic_tx(&base, n, 10);
            let mut output = tx.body.outputs()[0].clone();
            output.features = OutputFeatures::create_burn_output();
            let mut kernel = tx.body.kernels()[0].clone();
            kernel.features = KernelFeatures::create_burn();
            kernel.burn_commitment = Some(output.commitment.clone());
            (output, kernel)
        };

        let pending = synthetic_tx(&base, 0, 10);
        let (burned_output, burn_kernel) = burn_tx(1);
        let burn = Arc::new(Transaction::new(
            vec![],
            vec![burned_output.clone()],
            vec![burn_kernel],
            Default::default(),
            Default::default(),
        ));
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
        for tx in [&pending, &burn] {
            unconfirmed_pool.insert(tx.clone(), None, &tx_weight).unwrap();
        }

        // A non-burn kernel naming a pending output's commitment does not remove the pending transaction
        let mut kernel = synthetic_tx(&base, 2, 10).body.kernels()[0].clone();
        kernel.burn_commitment = Some(pending.body.outputs()[0].commitment.clone());
        let other = with_kernels(&base, 3, vec![kernel]);
        // This body fails block validation; it only exercises the pool
        let published_block = create_orphan_block(0, vec![(*other).clone()], &consensus);
        let removed = unconfirmed_pool
            .remove_published_and_discard_deprecated_transactions(&published_block)
            .unwrap();
        assert!(removed.is_empty());
        assert_eq!(unconfirmed_pool.tx_by_key.len(), 2);

        // A mined burn of the same output (under a different kernel) still removes the pooled burn
        let (_, mut other_burn_kernel) = burn_tx(4);
        other_burn_kernel.burn_commitment = Some(burned_output.commitment.clone());
        let mined_burn = Transaction::new(
            vec![],
            vec![burned_output],
            vec![other_burn_kernel],
            Default::default(),
            Default::default(),
        );
        let published_block = create_orphan_block(1, vec![mined_burn], &consensus);
        crate::validation::aggregate_body::check_total_burned(&published_block.body).unwrap();
        let removed = unconfirmed_pool
            .remove_published_and_discard_deprecated_transactions(&published_block)
            .unwrap();
        assert_eq!(removed, vec![burn]);
        assert_eq!(unconfirmed_pool.tx_by_key.len(), 1);
        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn a_transaction_built_to_collide_with_an_honest_one_cannot_censor_it() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 5, 5);
        // An honest transaction H paying an output O (commitment C) to a payee
        let honest = synthetic_tx(&base, 0, 1_000);
        // The payee (who knows O's opening) builds a high-rate transaction spending O that also creates an output with
        // O's commitment (a different output, so a different hash): the branch {collider, H} can never be valid
        let mut colliding_output = honest.body.outputs()[0].clone();
        colliding_output.features.maturity += 1;
        let collider = Arc::new(Transaction::new(
            vec![],
            vec![colliding_output],
            synthetic_tx(&base, 1, 1_000_000).body.kernels().clone(),
            Default::default(),
            Default::default(),
        ));
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        unconfirmed_pool.insert(honest.clone(), None, &tx_weight).unwrap();
        unconfirmed_pool
            .insert(
                collider.clone(),
                Some(vec![honest.body.outputs()[0].hash()]),
                &tx_weight,
            )
            .unwrap();
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();

        for ranking in [Ranking::Weight, Ranking::EffectiveWeight {
            max_block_transaction_weight: max_weight(),
            max_body_bytes: usize::MAX / 2,
        }] {
            let selection = unconfirmed_pool.select_txs(u64::MAX, usize::MAX / 2, ranking).unwrap();
            let selected = &selection.results.retrieved_transactions;
            // The collider's branch is dropped, but H itself is still selected
            assert!(!selected.contains(&collider), "{ranking:?}");
            assert!(selected.contains(&honest), "{ranking:?}");
            for tx in &normal {
                assert!(selected.contains(tx), "{ranking:?}");
            }
            assert_eq!(selection.invalid_branch_drops, 1, "{ranking:?}");
            assert_eq!(selection.swept_branches, 0, "{ranking:?}");
        }
    }

    #[tokio::test]
    async fn a_branch_whose_transactions_share_a_kernel_is_never_selected() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let parent = synthetic_tx(&base, 0, 10);
        // A high-rate child of the parent that carries the parent's kernel as well as its own
        let child = {
            let tx = synthetic_tx(&base, 1, 1_000_000);
            Arc::new(Transaction::new(
                vec![],
                tx.body.outputs().clone(),
                vec![tx.body.kernels()[0].clone(), parent.body.kernels()[0].clone()],
                Default::default(),
                Default::default(),
            ))
        };
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        unconfirmed_pool.insert(parent.clone(), None, &tx_weight).unwrap();
        unconfirmed_pool
            .insert(child.clone(), Some(vec![parent.body.outputs()[0].hash()]), &tx_weight)
            .unwrap();
        assert_eq!(unconfirmed_pool.tx_by_key.len(), 2);
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        let selected = &selection.results.retrieved_transactions;
        assert!(!selected.contains(&child));
        assert!(selected.contains(&parent));
        assert_eq!(selection.invalid_branch_drops, 1);
        assert_eq!(selection.swept_branches, 0);
    }

    #[tokio::test]
    async fn a_transaction_repeating_a_kernel_excess_is_never_selected() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 10, 5);
        // One transaction whose two kernels share an excess (different signatures), with a high rate. (Admission
        // rejects it; here it is inserted into the pool directly.)
        let kernel = synthetic_tx(&base, 0, 1_000_000).body.kernels()[0].clone();
        let mut again = kernel.clone();
        again.excess_sig = synthetic_tx(&base, 1, 1).body.kernels()[0].excess_sig.clone();
        let repeating = with_kernels(&base, 2, vec![kernel, again]);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        unconfirmed_pool.insert(repeating.clone(), None, &tx_weight).unwrap();
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();
        let selection = unconfirmed_pool
            .select_txs(u64::MAX, MAX_BLOCK_TEMPLATE_BODY_BYTES, Ranking::Weight)
            .unwrap();
        let selected = &selection.results.retrieved_transactions;
        assert!(!selected.contains(&repeating));
        // It is found before it is selected, so the template still fills with the honest transactions
        assert_eq!(selection.swept_branches, 0);
        for tx in &normal {
            assert!(selected.contains(tx));
        }
    }

    #[tokio::test]
    async fn a_hub_with_many_colliding_partners_does_not_slow_the_pair_lookup() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let normal = normal_txs(&key_manager, 10, 5);
        let u = normal_txs(&key_manager, 1, 5).pop().unwrap().body.inputs()[0].clone();
        let spending_u = |n: u64, fee: u64| {
            let tx = synthetic_tx(&base, n, fee);
            Arc::new(Transaction::new(
                vec![u.clone()],
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        };
        // A min-fee hub P spending U, and many high-rate children each spending P's output and double-spending U:
        // every {child, P} branch reports a new colliding pair, so P's partner set grows with every candidate
        const CHILDREN: u64 = 5_000;
        let hub = spending_u(0, 1);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        unconfirmed_pool.insert(hub.clone(), None, &tx_weight).unwrap();
        let hub_output = hub.body.outputs()[0].hash();
        for n in 1..=CHILDREN {
            unconfirmed_pool
                .insert(spending_u(n, 1_000_000), Some(vec![hub_output]), &tx_weight)
                .unwrap();
        }
        unconfirmed_pool.insert_many(normal.clone(), &tx_weight).unwrap();

        let selection = unconfirmed_pool
            .select_txs(u64::MAX, usize::MAX / 2, Ranking::Weight)
            .unwrap();
        assert!(!selection.ended_early);
        for tx in &normal {
            assert!(selection.results.retrieved_transactions.contains(tx));
        }
        assert_eq!(selection.invalid_branch_drops, usize::try_from(CHILDREN).unwrap());
        // Each lookup scans the smaller of the partner set and the branch: bounded by |branch|^2 per branch, not by
        // the number of partners the hub has collected
        let bound = usize::try_from(CHILDREN).unwrap() * (MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE + 1).pow(2);
        assert!(selection.pair_lookup_work <= bound);
        assert!(
            selection.pair_lookup_work <= 4 * usize::try_from(CHILDREN).unwrap(),
            "{}",
            selection.pair_lookup_work
        );
        assert!(selection.links_followed < 100_000, "{}", selection.links_followed);
    }

    #[tokio::test]
    async fn state_dependent_drops_stay_selectable_in_the_second_pass() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        // A: byte-heavy, the highest fee per gram
        let a = Arc::new(
            tx!(MicroMinotari(500_000), fee: MicroMinotari(10), inputs: 12, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        // B: small, spends one of A's inputs (conflicts with A), a lower fee per gram but a higher fee per effective
        // gram
        let b = {
            let tx = tx!(MicroMinotari(500_000), fee: MicroMinotari(8), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0;
            Arc::new(Transaction::new(
                vec![a.body.inputs()[0].clone()],
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        };
        // Normal transactions: the first pass fits A and the first 10 of them, then runs out of bytes with the rest
        // unvisited (ending early, but not for lack of budget)
        let normal = normal_txs(&key_manager, 15, 5);
        let budget = body_size(&a) + normal.iter().take(10).map(body_size).sum::<usize>() + 100;
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(budget);
        unconfirmed_pool
            .insert_many(
                [a.clone(), b.clone()].into_iter().chain(normal.iter().cloned()),
                &tx_weight,
            )
            .unwrap();
        let key_of = |tx: &Arc<Transaction>| {
            *unconfirmed_pool
                .tx_by_key
                .iter()
                .find(|(_, p)| &p.transaction == tx)
                .unwrap()
                .0
        };

        let by_weight = unconfirmed_pool.select_txs(u64::MAX, budget, Ranking::Weight).unwrap();
        assert!(by_weight.ended_early);
        assert!(!by_weight.budget_exhausted);
        assert!(by_weight.results.retrieved_transactions.contains(&a));
        assert!(!by_weight.results.retrieved_transactions.contains(&b));
        // B was dropped because of the first pass's choice of A: a state-dependent drop, not an intrinsic one
        assert!(by_weight.state_dropped.contains(&key_of(&b)));
        assert!(!by_weight.walked_dropped.contains(&key_of(&b)));
        assert!(!UnconfirmedPool::second_pass_exclusions(&by_weight).contains(&key_of(&b)));

        // So the second pass can select B, and does so for more fees
        let chosen = unconfirmed_pool
            .fetch_selection(u64::MAX, budget, max_weight(), &TemplateFilter::unrestricted())
            .unwrap();
        assert!(chosen.results.retrieved_transactions.contains(&b));
        assert!(!chosen.results.retrieved_transactions.contains(&a));
        assert!(chosen.total_fees > by_weight.total_fees);
    }

    #[tokio::test]
    async fn a_pass_ended_by_weight_skips_still_selects_queued_branches() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let weight = |tx: &Arc<Transaction>| tx.calculate_weight(&tx_weight).unwrap();
        let rate = |fee: u64, weight: u64| fee * 1000 / weight;
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        });
        let fill = (0..5)
            .map(|_| {
                Arc::new(
                    tx!(MicroMinotari(500_000), fee: MicroMinotari(50), inputs: 1, outputs: 1, &key_manager)
                        .expect("Failed to get tx")
                        .0,
                )
            })
            .collect::<Vec<_>>();
        // An honest CPFP pair: its child's own rate is the highest, but the pair's rate is below everything else
        let cpfp = insert_chain(&mut unconfirmed_pool, &base, 0, 1, 1);
        let other_input = normal_txs(&key_manager, 1, 5)
            .pop()
            .unwrap()
            .body
            .inputs()
            .first()
            .unwrap()
            .clone();
        let cpfp_child = insert_children(&mut unconfirmed_pool, &base, &cpfp[0], &other_input, 1, 1, 4_000)
            .pop()
            .unwrap();
        // 21 heavy transactions ranked between the pair's own rate and its combined rate, too heavy for what is left
        let heavy = (0..21)
            .map(|_| {
                Arc::new(
                    tx!(MicroMinotari(500_000), fee: MicroMinotari(45), inputs: 20, outputs: 1, &key_manager)
                        .expect("Failed to get tx")
                        .0,
                )
            })
            .collect::<Vec<_>>();
        let cpfp_weight = weight(&cpfp[0]) + weight(&cpfp_child);
        let total_weight = fill.iter().map(weight).sum::<u64>() + cpfp_weight + 10;
        assert!(weight(&heavy[0]) > cpfp_weight + 10);
        let heavy_rate = rate(heavy[0].body.get_total_fee().unwrap().as_u64(), weight(&heavy[0]));
        assert!(rate(4_000, weight(&cpfp_child)) > heavy_rate);
        assert!(rate(4_001, cpfp_weight) < heavy_rate);
        unconfirmed_pool
            .insert_many(fill.iter().chain(heavy.iter()).cloned(), &tx_weight)
            .unwrap();

        let selection = unconfirmed_pool
            .select_txs(total_weight, MAX_BLOCK_TEMPLATE_BODY_BYTES, Ranking::Weight)
            .unwrap();
        // The pass is ended by 20 counted weight skips (the block is full enough), but the queued pair still fits and
        // is selected
        assert_eq!(selection.weight_skips_counted, 20);
        let selected = &selection.results.retrieved_transactions;
        assert!(selected.contains(&cpfp_child));
        assert!(selected.contains(&cpfp[0]));
        for tx in &fill {
            assert!(selected.contains(tx));
        }
    }

    #[tokio::test]
    async fn filling_the_byte_budget_triggers_the_effective_weight_pass() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        // Byte-heavy stuffers with twice the fee per gram of the normal transactions
        let stuffers = (0..2)
            .map(|_| {
                Arc::new(
                    tx!(MicroMinotari(500_000), fee: MicroMinotari(10), inputs: 12, outputs: 1, &key_manager)
                        .expect("Failed to get tx")
                        .0,
                )
            })
            .collect::<Vec<_>>();
        // A sentinel spending a stuffer's input, ranked between the stuffers and the normal transactions
        let sentinel = {
            let kernels = tx!(MicroMinotari(500_000), fee: MicroMinotari(8), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0
                .body
                .kernels()
                .clone();
            Arc::new(Transaction::new(
                vec![stuffers[0].body.inputs().first().unwrap().clone()],
                stuffers[0].body.outputs().clone(),
                kernels,
                Default::default(),
                Default::default(),
            ))
        };
        let normal = normal_txs(&key_manager, 10, 5);
        // The stuffers fill the budget to within MIN_TRANSACTION_BODY_BYTES, so no candidate is ever byte-skipped
        let budget = stuffers.iter().map(body_size).sum::<usize>() + MIN_TRANSACTION_BODY_BYTES / 2;
        assert!(normal.iter().map(body_size).sum::<usize>() <= budget);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 100,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(budget);
        unconfirmed_pool
            .insert_many(
                stuffers
                    .iter()
                    .cloned()
                    .chain(std::iter::once(sentinel.clone()))
                    .chain(normal.iter().cloned()),
                &tx_weight,
            )
            .unwrap();

        // The first pass selects only the stuffers, and reports that the byte budget bound
        let by_weight = unconfirmed_pool.select_txs(u64::MAX, budget, Ranking::Weight).unwrap();
        assert_eq!(by_weight.results.retrieved_transactions.len(), stuffers.len());
        assert!(by_weight.byte_bound);

        // So the effective-weight pass runs, and the normal-sized transactions win (the sentinel is one of them in this
        // pass, since nothing it conflicts with is selected)
        let chosen = unconfirmed_pool
            .fetch_selection(u64::MAX, budget, max_weight(), &TemplateFilter::unrestricted())
            .unwrap();
        assert!(chosen.total_fees > by_weight.total_fees);
        let selected = &chosen.results.retrieved_transactions;
        assert!(stuffers.iter().all(|tx| !selected.contains(tx)));
        assert!(selected.len() >= normal.len() - 1);
        assert!(selected.iter().all(|tx| normal.contains(tx) || *tx == sentinel));
    }

    #[tokio::test]
    async fn byte_skips_do_not_end_the_pass() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let txs = (0..600u64).map(|n| synthetic_tx(&base, n, 1_000)).collect::<Vec<_>>();
        // Room for none of them, but more than MIN_TRANSACTION_BODY_BYTES, so that every one is byte-skipped
        let budget = body_size(&txs[0]) - 1;
        assert!(budget >= MIN_TRANSACTION_BODY_BYTES);
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: txs.len(),
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(budget);
        let num_txs = txs.len();
        unconfirmed_pool.insert_many(txs, &tx_weight).unwrap();

        let selection = unconfirmed_pool.select_txs(u64::MAX, budget, Ranking::Weight).unwrap();
        assert!(selection.results.retrieved_transactions.is_empty());
        assert!(selection.byte_bound);
        // Each is passed over in O(1) (its own body cannot fit), and none of them ends the pass
        assert_eq!(selection.byte_skips, num_txs);
        assert!(!selection.ended_early);
    }

    #[test]
    fn a_deep_chain_of_pool_transactions_does_not_overflow_the_stack() {
        const DEPTH: u64 = 5_000;
        // A small stack: a recursive walk of a chain this deep would overflow it
        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
                let key_manager = KeyManager::new_random().unwrap();
                let base = runtime.block_on(async { normal_txs(&key_manager, 1, 5).pop().unwrap() });
                let tx_weight = TransactionWeight::latest();
                let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
                    storage_capacity: 10_000,
                    weight_tx_skip_count: 20,
                    min_fee: 0,
                })
                .with_max_body_bytes(usize::MAX / 2);
                // Each transaction spends the previous one's output; the tip pays by far the most, so it is visited
                // first and its branch is the whole chain
                let mut parent: Option<Arc<Transaction>> = None;
                for n in 0..DEPTH {
                    let fee = if n == DEPTH - 1 { 1_000_000_000 } else { 10 };
                    let tx = synthetic_tx(&base, n, fee);
                    let dependent_outputs = parent.as_ref().map(|p| vec![p.body.outputs().first().unwrap().hash()]);
                    unconfirmed_pool
                        .insert(tx.clone(), dependent_outputs, &tx_weight)
                        .unwrap();
                    parent = Some(tx);
                }
                let results = unconfirmed_pool
                    .fetch_highest_priority_txs(u64::MAX, max_weight())
                    .unwrap();
                // The tip has far more than MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE unselected ancestors, so its walk
                // stops at the cap and it is dropped; the chain's lower transactions are still selectable
                assert!(!results.retrieved_transactions.is_empty());
                assert!(results.retrieved_transactions.len() < usize::try_from(DEPTH).unwrap());
                assert!(results.transactions_to_remove_and_insert.is_empty());
            })
            .unwrap()
            .join()
            .expect("selection over a deep chain panicked (or overflowed its stack)");
    }

    #[tokio::test]
    async fn test_double_spend_inputs() {
        let key_manager = KeyManager::new_random().unwrap();
        let (tx1, _, _) = tx!(MicroMinotari(5_000), fee: MicroMinotari(10), inputs: 1, outputs: 1, &key_manager)
            .expect("Failed to get tx");
        const INPUT_AMOUNT: MicroMinotari = MicroMinotari(5_000);
        let (tx2, inputs, _) =
            tx!(INPUT_AMOUNT, fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager).expect("Failed to get tx");

        let mut tx_builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();

        tx_builder.with_lock_height(0).with_fee_per_gram(5.into());

        let test_params = TestParams::new(&key_manager);
        // Double spend the input from tx2 in tx3
        let double_spend_input = inputs.first().unwrap().clone();

        let estimated_fee = Fee::new(TransactionWeight::latest()).calculate(
            5.into(),
            1,
            1,
            1,
            test_params
                .get_size_for_default_features_and_scripts(1)
                .expect("Failed to get size for default features and scripts"),
        );

        let utxo = test_params
            .create_output(
                UtxoTestParams {
                    value: INPUT_AMOUNT - estimated_fee,
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap();
        tx_builder.with_input(double_spend_input).unwrap();
        add_outputs_with_reserved_sender_offset_keys(&mut tx_builder, vec![utxo]).unwrap();

        let finalized = tx_builder.build().expect("Failed to finalize transaction");

        let tx3 = finalized.transaction;

        let tx1 = Arc::new(tx1);
        let tx2 = Arc::new(tx2);
        let tx3 = Arc::new(tx3);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 4,
            weight_tx_skip_count: 3,
            min_fee: 0,
        });

        let tx_weight = TransactionWeight::latest();
        unconfirmed_pool
            .insert_many(vec![tx1.clone(), tx2.clone(), tx3.clone()], &tx_weight)
            .expect("Failed to insert many");
        assert_eq!(unconfirmed_pool.len(), 3);

        let desired_weight = tx1.calculate_weight(&tx_weight).expect("Failed to get tx") +
            tx2.calculate_weight(&tx_weight).expect("Failed to get tx") +
            tx3.calculate_weight(&tx_weight).expect("Failed to get tx") +
            1000;
        let results = unconfirmed_pool
            .fetch_highest_priority_txs(desired_weight, max_weight())
            .unwrap();
        assert!(results.retrieved_transactions.contains(&tx1));
        // Whether tx2 or tx3 is selected is non-deterministic
        assert!(results.retrieved_transactions.contains(&tx2) ^ results.retrieved_transactions.contains(&tx3));
        assert_eq!(results.retrieved_transactions.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_reorg_txs() {
        let key_manager = KeyManager::new_random().unwrap();
        let network = Network::LocalNet;
        let consensus = BaseNodeConsensusManagerBuilder::new(network).build().unwrap();
        let tx1 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(5), inputs:2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx2 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(2), inputs:3, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx3 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(1), inputs:2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx4 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(3), inputs:4, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx5 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(5), inputs:3, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx6 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(7), inputs:2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );

        let tx_weight = TransactionWeight::latest();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 3,
            min_fee: 0,
        });
        unconfirmed_pool
            .insert_many(
                vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()],
                &tx_weight,
            )
            .expect("Failed to insert many");
        // utx6 should not be added to unconfirmed_pool as it is an unknown transactions that was included in the block
        // by another node

        let snapshot_txs = unconfirmed_pool.snapshot();
        assert_eq!(snapshot_txs.len(), 5);
        assert!(snapshot_txs.contains(&tx1));
        assert!(snapshot_txs.contains(&tx2));
        assert!(snapshot_txs.contains(&tx3));
        assert!(snapshot_txs.contains(&tx4));
        assert!(snapshot_txs.contains(&tx5));

        let published_block = create_orphan_block(0, vec![(*tx1).clone(), (*tx3).clone(), (*tx5).clone()], &consensus);
        let _result = unconfirmed_pool.remove_published_and_discard_deprecated_transactions(&published_block);

        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig),);
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig),);
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig),);

        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn test_discard_double_spend_txs() {
        let key_manager = KeyManager::new_random().unwrap();
        let consensus = create_consensus_rules();
        let tx1 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs:2, outputs:1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx2 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(4), inputs:3, outputs:1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx3 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs:2, outputs:1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx4 = Arc::new(
            tx!(MicroMinotari(5_000), fee: MicroMinotari(6), inputs:2, outputs:1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let mut tx5 = tx!(MicroMinotari(5_000), fee:MicroMinotari(5), inputs:3, outputs:1, &key_manager)
            .expect("Failed to get tx")
            .0;
        let mut tx6 = tx!(MicroMinotari(5_000), fee:MicroMinotari(13), inputs: 2, outputs: 1, &key_manager)
            .expect("Failed to get tx")
            .0;
        // tx1 and tx5 have a shared input. Also, tx3 and tx6 have a shared input
        let mut inputs = tx5.body.inputs().clone();
        inputs[0] = tx1.body.inputs()[0].clone();
        tx5.body = AggregateBody::new_unsorted(inputs, tx5.body().outputs().clone(), tx5.body().kernels().clone());
        let mut inputs = tx6.body.inputs().clone();
        inputs[0] = tx3.body.inputs()[1].clone();
        tx6.body = AggregateBody::new_unsorted(inputs, tx6.body().outputs().clone(), tx6.body().kernels().clone());
        let tx5 = Arc::new(tx5);
        let tx6 = Arc::new(tx6);

        let tx_weight = TransactionWeight::latest();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 3,
            min_fee: 0,
        });
        unconfirmed_pool
            .insert_many(
                vec![
                    tx1.clone(),
                    tx2.clone(),
                    tx3.clone(),
                    tx4.clone(),
                    tx5.clone(),
                    tx6.clone(),
                ],
                &tx_weight,
            )
            .expect("Failed to insert many");

        // The publishing of tx1 and tx3 will be double-spends and orphan tx5 and tx6
        let published_block = create_orphan_block(0, vec![(*tx1).clone(), (*tx2).clone(), (*tx3).clone()], &consensus);

        let _result = unconfirmed_pool.remove_published_and_discard_deprecated_transactions(&published_block); // Double spends are discarded

        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig));
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig));
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig));
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig));
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig));
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig));

        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn test_multiple_transactions_with_same_outputs_in_mempool() {
        let key_manager = KeyManager::new_random().unwrap();
        let (tx1, _, _) = tx!(MicroMinotari(150_000), fee: MicroMinotari(50), inputs:5, outputs:5, &key_manager)
            .expect("Failed to get tx");
        let (tx2, _, _) = tx!(MicroMinotari(250_000), fee: MicroMinotari(50), inputs:5, outputs:5, &key_manager)
            .expect("Failed to get tx");

        // Create transactions with duplicate kernels (will not pass internal validation, but that is ok)
        let mut tx3 = tx1.clone();
        let mut tx4 = tx2.clone();
        let (tx5, _, _) = tx!(MicroMinotari(350_000), fee: MicroMinotari(50), inputs:5, outputs:5, &key_manager)
            .expect("Failed to get tx");
        let (tx6, _, _) = tx!(MicroMinotari(450_000), fee: MicroMinotari(50), inputs:5, outputs:5, &key_manager)
            .expect("Failed to get tx");
        tx3.body.set_kernel(tx5.body.kernels()[0].clone());
        tx4.body.set_kernel(tx6.body.kernels()[0].clone());

        // Insert multiple transactions with the same outputs into the mempool

        let tx_weight = TransactionWeight::latest();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 3,
            min_fee: 0,
        });
        let txns = vec![
            Arc::new(tx1.clone()),
            Arc::new(tx2.clone()),
            // Transactions with duplicate outputs
            Arc::new(tx3.clone()),
            Arc::new(tx4.clone()),
        ];
        unconfirmed_pool
            .insert_many(txns.clone(), &tx_weight)
            .expect("Failed to insert many");

        for txn in txns {
            for output in txn.as_ref().body.outputs() {
                assert!(unconfirmed_pool.contains_all_outputs(&[output.hash()]));
                let keys_by_output = unconfirmed_pool.txs_by_output.get(&output.hash()).unwrap();
                // Each output must be referenced by two transactions
                assert_eq!(keys_by_output.len(), 2);
                // Verify kernel signature present exactly once
                let mut found = 0u8;
                for key in keys_by_output {
                    let found_tx = &unconfirmed_pool.tx_by_key.get(key).unwrap().transaction;
                    if *found_tx == txn {
                        found = found.saturating_add(1);
                    }
                }
                assert_eq!(found, 1);
            }
        }

        // Remove some transactions
        let k = *unconfirmed_pool
            .txs_by_signature
            .get(tx1.first_kernel_excess_sig().unwrap().get_signature())
            .unwrap()
            .first()
            .unwrap();
        unconfirmed_pool.remove_transaction(k).unwrap();
        let k = *unconfirmed_pool
            .txs_by_signature
            .get(tx4.first_kernel_excess_sig().unwrap().get_signature())
            .unwrap()
            .first()
            .unwrap();
        unconfirmed_pool.remove_transaction(k).unwrap();

        let txns = vec![
            Arc::new(tx2),
            // Transactions with duplicate outputs
            Arc::new(tx3),
        ];
        for txn in txns {
            for output in txn.as_ref().body.outputs() {
                let keys_by_output = unconfirmed_pool.txs_by_output.get(&output.hash()).unwrap();
                // Each output must be referenced by one transactions
                assert_eq!(keys_by_output.len(), 1);
                // Verify kernel signature present exactly once
                let key = keys_by_output.first().unwrap();
                let found_tx = &unconfirmed_pool.tx_by_key.get(key).unwrap().transaction;
                assert_eq!(
                    found_tx.first_kernel_excess_sig().unwrap(),
                    txn.first_kernel_excess_sig().unwrap()
                );
            }
        }
    }

    mod get_fee_per_gram_stats {

        use super::*;

        #[test]
        fn it_returns_empty_stats_for_empty_mempool() {
            let unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());
            let stats = unconfirmed_pool.get_fee_per_gram_stats(1, 19500).unwrap();
            assert!(stats.is_empty());
        }

        #[tokio::test]
        async fn it_compiles_correct_stats_for_single_block() {
            let key_manager = KeyManager::new_random().unwrap();
            let (tx1, _, _) = tx!(MicroMinotari(150_000), fee: MicroMinotari(5), inputs:5, outputs:1, &key_manager)
                .expect("Failed to get tx");
            let (tx2, _, _) = tx!(MicroMinotari(250_000), fee: MicroMinotari(5), inputs:5, outputs:5, &key_manager)
                .expect("Failed to get tx");
            let (tx3, _, _) = tx!(MicroMinotari(350_000), fee: MicroMinotari(4), inputs:2, outputs:1, &key_manager)
                .expect("Failed to get tx");
            let (tx4, _, _) = tx!(MicroMinotari(450_000), fee: MicroMinotari(4), inputs:4, outputs:5, &key_manager)
                .expect("Failed to get tx");

            let tx_weight = TransactionWeight::latest();
            let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());

            let tx1 = Arc::new(tx1);
            let tx2 = Arc::new(tx2);
            let tx3 = Arc::new(tx3);
            let tx4 = Arc::new(tx4);
            unconfirmed_pool
                .insert_many(vec![tx1, tx2, tx3, tx4], &tx_weight)
                .expect("Failed to insert many");

            let stats = unconfirmed_pool.get_fee_per_gram_stats(1, 19500).unwrap();
            assert_eq!(stats[0].order, 0);
            assert_eq!(stats[0].min_fee_per_gram, 4.into());
            assert_eq!(stats[0].max_fee_per_gram, 5.into());
            assert_eq!(stats[0].avg_fee_per_gram, 4.into());
        }

        #[tokio::test]
        async fn it_compiles_correct_stats_for_multiple_blocks() {
            let key_manager = KeyManager::new_random().unwrap();
            let expected_stats = [
                FeePerGramStat {
                    order: 0,
                    min_fee_per_gram: 10.into(),
                    avg_fee_per_gram: 10.into(),
                    max_fee_per_gram: 10.into(),
                },
                FeePerGramStat {
                    order: 1,
                    min_fee_per_gram: 5.into(),
                    avg_fee_per_gram: 9.into(),
                    max_fee_per_gram: 10.into(),
                },
            ];
            let mut transactions = Vec::new();
            for i in 0..50 {
                let (tx, _, _) =
                    tx!(MicroMinotari(150_000 + i), fee: MicroMinotari(10), inputs: 1, outputs: 1, &key_manager)
                        .expect("Failed to get tx");
                transactions.push(Arc::new(tx));
            }

            let (tx1, _, _) = tx!(MicroMinotari(150_000), fee: MicroMinotari(5), inputs:1, outputs: 5, &key_manager)
                .expect("Failed to get tx");
            transactions.push(Arc::new(tx1));

            let tx_weight = TransactionWeight::latest();
            let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());

            unconfirmed_pool
                .insert_many(transactions, &tx_weight)
                .expect("Failed to insert many");

            let stats = unconfirmed_pool.get_fee_per_gram_stats(2, 2000).unwrap();
            assert_eq!(stats, expected_stats);
        }
    }

    mod validator_node_conflicts {
        use tari_common_types::{epoch::VnEpoch, types::CompressedPublicKey};
        use tari_crypto::keys::SecretKey;
        use tari_transaction_components::transaction_components::{OutputFeatures, ValidatorNodeSignature};

        use super::*;

        const NETWORK: u8 = 0x10;

        #[test]
        fn validator_node_keys_include_inputs_that_spend_a_registration() {
            use tari_script::ExecutionStack;
            use tari_transaction_components::{
                aggregated_body::AggregateBody,
                transaction_components::{SpentOutput, TransactionInput, TransactionOutput},
            };

            let vn_secret_key = PrivateKey::random(&mut rand::rng());
            let claim = CompressedPublicKey::from_secret_key(&vn_secret_key);
            let signature =
                ValidatorNodeSignature::sign_for_registration(&vn_secret_key, NETWORK, None, &claim, VnEpoch(10));
            let registration = TransactionOutput {
                features: OutputFeatures::for_validator_node_registration(signature, claim.clone(), None, VnEpoch(10)),
                ..Default::default()
            };
            // Spending the registration output (cancelling a pending registration) keys the transaction by the
            // validator node, just like registering or exiting it does
            let input = TransactionInput::new_current_version(
                SpentOutput::create_from_output(registration),
                ExecutionStack::default(),
                Default::default(),
            );
            let body = AggregateBody::new_unsorted(vec![input], vec![], vec![]);
            assert_eq!(validator_node_keys(&body), vec![(None, claim.as_bytes().to_vec())]);
            // A body with no validator node features has no keys
            assert!(validator_node_keys(&AggregateBody::new_unsorted(vec![], vec![], vec![])).is_empty());
        }

        fn vn_exit_tx(vn_secret_key: &PrivateKey, fee: u64, key_manager: &KeyManager) -> Arc<Transaction> {
            let signature =
                ValidatorNodeSignature::sign_for_exit(vn_secret_key, NETWORK, None, VnEpoch(0), VnEpoch(10));
            let features = OutputFeatures::for_validator_node_exit(signature, None, VnEpoch(0), VnEpoch(10));
            Arc::new(
                tx!(MicroMinotari(5_000), fee: MicroMinotari(fee), inputs: 1, outputs: 1, features: features, key_manager)
                    .expect("Failed to get tx")
                    .0,
            )
        }

        fn vn_registration_tx(vn_secret_key: &PrivateKey, key_manager: &KeyManager) -> Arc<Transaction> {
            let claim = CompressedPublicKey::from_secret_key(vn_secret_key);
            let signature =
                ValidatorNodeSignature::sign_for_registration(vn_secret_key, NETWORK, None, &claim, VnEpoch(10));
            let features = OutputFeatures::for_validator_node_registration(signature, claim, None, VnEpoch(10));
            Arc::new(
                tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, features: features, key_manager)
                    .expect("Failed to get tx")
                    .0,
            )
        }

        fn pool() -> UnconfirmedPool {
            UnconfirmedPool::new(UnconfirmedPoolConfig {
                storage_capacity: 10,
                weight_tx_skip_count: 3,
                min_fee: 0,
            })
        }

        #[test]
        fn it_reports_a_conflict_for_a_second_transaction_for_the_same_validator_node() {
            let key_manager = KeyManager::new_random().unwrap();
            let vn_secret_key = PrivateKey::random(&mut rand::rng());
            let exit = vn_exit_tx(&vn_secret_key, 5, &key_manager);
            let mut pool = pool();
            assert!(!pool.has_validator_node_conflict(&exit));
            pool.insert(exit.clone(), None, &TransactionWeight::latest()).unwrap();

            // The same transaction again is not a conflict (insert deduplicates it)
            assert!(!pool.has_validator_node_conflict(&exit));
            // A different exit, or a registration, for the same validator node is
            assert!(pool.has_validator_node_conflict(&vn_exit_tx(&vn_secret_key, 6, &key_manager)));
            assert!(pool.has_validator_node_conflict(&vn_registration_tx(&vn_secret_key, &key_manager)));
            // Another validator node is not
            let other = PrivateKey::random(&mut rand::rng());
            assert!(!pool.has_validator_node_conflict(&vn_exit_tx(&other, 5, &key_manager)));

            // Removing the holder releases the key
            let key = *pool.txs_by_validator_node.values().next().unwrap();
            pool.remove_transaction(key).unwrap();
            assert!(pool.txs_by_validator_node.is_empty());
            assert!(!pool.has_validator_node_conflict(&vn_exit_tx(&vn_secret_key, 6, &key_manager)));
            assert!(pool.check_data_consistency());
        }

        fn block_with(tx: &Transaction) -> Block {
            Block::new(tari_node_components::blocks::BlockHeader::new(0), tx.body.clone())
        }

        #[test]
        fn a_published_block_evicts_pool_transactions_for_the_same_validator_node() {
            // The pool holds T1 exiting validator node A; a peer's block exits A through a different transaction T2
            // (e.g. a resend or a replayed exit). T1 can never be valid now and must not wait for a locally mined
            // block to fail.
            let key_manager = KeyManager::new_random().unwrap();
            let vn_a = PrivateKey::random(&mut rand::rng());
            let vn_b = PrivateKey::random(&mut rand::rng());
            let t1 = vn_exit_tx(&vn_a, 5, &key_manager);
            let unrelated = vn_exit_tx(&vn_b, 5, &key_manager);
            let mut pool = pool();
            let weight = TransactionWeight::latest();
            pool.insert(t1.clone(), None, &weight).unwrap();
            pool.insert(unrelated.clone(), None, &weight).unwrap();

            let t2 = vn_exit_tx(&vn_a, 7, &key_manager);
            let removed = pool
                .remove_published_and_discard_deprecated_transactions(&block_with(&t2))
                .unwrap();

            assert_eq!(removed, vec![t1.clone()]);
            assert!(!pool.has_tx_with_excess_sig(&t1.body.kernels()[0].excess_sig));
            assert!(pool.has_tx_with_excess_sig(&unrelated.body.kernels()[0].excess_sig));
            assert_eq!(pool.txs_by_validator_node.len(), 1);
            assert!(pool.check_data_consistency());

            // Registration variant: a published registration for B evicts the pool's exit for B
            let registration = vn_registration_tx(&vn_b, &key_manager);
            pool.remove_published_and_discard_deprecated_transactions(&block_with(&registration))
                .unwrap();
            assert!(!pool.has_tx_with_excess_sig(&unrelated.body.kernels()[0].excess_sig));
            assert!(pool.txs_by_validator_node.is_empty());
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn it_never_selects_two_transactions_for_the_same_validator_node() {
            // `MempoolStorage` refuses to store the second one; inserting both straight into the pool exercises the
            // defensive check in the final selection sweep.
            let key_manager = KeyManager::new_random().unwrap();
            let vn_secret_key = PrivateKey::random(&mut rand::rng());
            let exit1 = vn_exit_tx(&vn_secret_key, 5, &key_manager);
            let exit2 = vn_exit_tx(&vn_secret_key, 6, &key_manager);
            let mut pool = pool();
            let weight = TransactionWeight::latest();
            pool.insert(exit1.clone(), None, &weight).unwrap();
            pool.insert(exit2.clone(), None, &weight).unwrap();
            assert!(pool.check_data_consistency());

            let results = pool.fetch_highest_priority_txs(max_weight(), max_weight()).unwrap();
            assert_eq!(results.retrieved_transactions.len(), 1);
            // The higher fee one is selected first
            assert!(results.retrieved_transactions.contains(&exit2));
        }
    }

    mod height_reading_index {
        use tari_script::{TariScript, script};
        use tari_transaction_components::{
            covenant,
            transaction_components::{SpentOutput, covenants::Covenant},
        };

        use super::*;

        /// A fresh transaction whose first input has the given script and covenant
        fn tx_with_input(script: TariScript, covenant: Covenant, key_manager: &KeyManager) -> Arc<Transaction> {
            let tx = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, key_manager)
                .expect("Failed to get tx")
                .0;
            let mut inputs = tx.body.inputs().clone();
            match &mut inputs[0].spent_output {
                SpentOutput::OutputData {
                    script: s, covenant: c, ..
                } => {
                    *s = script;
                    *c = covenant;
                },
                SpentOutput::OutputHash(_) => panic!("Expected a full input"),
            }
            Arc::new(Transaction::new(
                inputs,
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                tx.offset.clone(),
                tx.script_offset.clone(),
            ))
        }

        fn indexed(pool: &UnconfirmedPool) -> Vec<Arc<Transaction>> {
            pool.txs_reading_height
                .keys()
                .map(|key| pool.tx_by_key[key].transaction.clone())
                .collect()
        }

        #[test]
        fn it_indexes_transactions_whose_input_script_or_covenant_reads_the_height() {
            let key_manager = KeyManager::new_random().unwrap();
            let plain = tx_with_input(script!(Nop).unwrap(), Covenant::default(), &key_manager);
            let script_reads = tx_with_input(
                script!(CheckHeightVerify(10)).unwrap(),
                Covenant::default(),
                &key_manager,
            );
            let covenant_reads = tx_with_input(
                script!(Nop).unwrap(),
                covenant!(not(absolute_height(@uint(10)))).unwrap(),
                &key_manager,
            );

            let mut pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
                storage_capacity: 10,
                weight_tx_skip_count: 3,
                min_fee: 0,
            });
            let weight = TransactionWeight::latest();
            for tx in [&plain, &script_reads, &covenant_reads] {
                pool.insert(tx.clone(), None, &weight).unwrap();
            }
            assert_eq!(indexed(&pool), vec![script_reads.clone(), covenant_reads.clone()]);
            assert!(pool.check_data_consistency());

            // Remove
            let key = *pool.txs_reading_height.keys().next().unwrap();
            pool.remove_transaction(key).unwrap();
            assert_eq!(indexed(&pool), vec![covenant_reads.clone()]);
            assert!(pool.check_data_consistency());

            // Compact
            pool.compact();
            assert_eq!(indexed(&pool), vec![covenant_reads.clone()]);
            assert!(pool.check_data_consistency());

            // The consistency check covers the index
            let key = *pool.txs_reading_height.keys().next().unwrap();
            let entry = pool.txs_reading_height.remove(&key).unwrap();
            assert!(!pool.check_data_consistency());
            pool.txs_reading_height.insert(key, entry);
            assert!(pool.check_data_consistency());

            // Drain
            assert_eq!(pool.drain_all_mempool_transactions().len(), 2);
            assert!(indexed(&pool).is_empty());
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn only_selected_height_reading_transactions_not_yet_passed_at_the_tip_are_checked() {
            let key_manager = KeyManager::new_random().unwrap();
            let plain = tx_with_input(script!(Nop).unwrap(), Covenant::default(), &key_manager);
            let selected = tx_with_input(
                script!(CheckHeightVerify(10)).unwrap(),
                Covenant::default(),
                &key_manager,
            );
            let not_selected = tx_with_input(
                script!(CheckHeightVerify(10)).unwrap(),
                Covenant::default(),
                &key_manager,
            );
            let mut pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
                storage_capacity: 10,
                weight_tx_skip_count: 3,
                min_fee: 0,
            });
            let weight = TransactionWeight::latest();
            for tx in [&plain, &selected, &not_selected] {
                pool.insert(tx.clone(), None, &weight).unwrap();
            }
            let tip = FixedHash::from([1u8; 32]);
            let to_check = pool.height_reading_to_check(&[plain.clone(), selected.clone()], &tip);
            assert_eq!(to_check.len(), 1);
            let (key, tx, _) = to_check[0].clone();
            assert_eq!(tx, selected);
            // An equal copy of the stored transaction is checked too
            let copy = Arc::new((*selected).clone());
            assert_eq!(pool.height_reading_to_check(&[copy], &tip).len(), 1);

            // Once passed at a tip, it is not checked again at that tip, only at another
            pool.record_height_check_passed(key, tip);
            assert!(
                pool.height_reading_to_check(std::slice::from_ref(&selected), &tip)
                    .is_empty()
            );
            let next_tip = FixedHash::from([2u8; 32]);
            assert_eq!(
                pool.height_reading_to_check(std::slice::from_ref(&selected), &next_tip)
                    .len(),
                1
            );
            assert!(pool.check_data_consistency());
        }

        fn pool() -> UnconfirmedPool {
            UnconfirmedPool::new(UnconfirmedPoolConfig {
                storage_capacity: 10_000,
                weight_tx_skip_count: 3,
                min_fee: 0,
            })
        }

        /// A synthetic transaction (see `synthetic_tx`) spending the first output of `parent`
        fn child_of(base: &Transaction, parent: &Transaction, n: u64) -> Arc<Transaction> {
            use tari_script::ExecutionStack;
            use tari_transaction_components::transaction_components::TransactionInput;
            let tx = synthetic_tx(base, n, 5);
            let input = TransactionInput::new_current_version(
                SpentOutput::create_from_output(parent.body.outputs()[0].clone()),
                ExecutionStack::default(),
                Default::default(),
            );
            Arc::new(Transaction::new(
                vec![input],
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        }

        #[test]
        fn removing_a_transaction_removes_its_descendants_in_linear_time() {
            const DEPTH: u64 = 2_000;
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let root = synthetic_tx(&base, 0, 5);
            pool.insert(root.clone(), None, &weight).unwrap();
            let mut parent = root.clone();
            for n in 1..DEPTH {
                let child = child_of(&base, &parent, n);
                let dependent_outputs = Some(vec![parent.body.outputs()[0].hash()]);
                pool.insert(child.clone(), dependent_outputs, &weight).unwrap();
                parent = child;
            }
            // A second child of the root, and an unrelated transaction
            let sibling = child_of(&base, &root, DEPTH);
            pool.insert(sibling, None, &weight).unwrap();
            let unrelated = synthetic_tx(&base, DEPTH + 1, 5);
            pool.insert(unrelated.clone(), None, &weight).unwrap();
            let root_key = *pool.txs_by_signature[root.body.kernels()[0].excess_sig.get_signature()]
                .first()
                .unwrap();

            let timer = std::time::Instant::now();
            let removed = pool.remove_transactions_and_descendants([root_key]).unwrap();
            println!("Removed a chain of {DEPTH} transactions in {:.2?}", timer.elapsed());
            assert_eq!(removed, usize::try_from(DEPTH + 1).unwrap());
            assert_eq!(pool.len(), 1);
            assert!(pool.has_tx_with_excess_sig(&unrelated.body.kernels()[0].excess_sig));
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn removing_a_transaction_removes_many_spenders_of_one_output_in_linear_time() {
            const SPENDERS: u64 = 9_000;
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let root = synthetic_tx(&base, 0, 5);
            pool.insert(root.clone(), None, &weight).unwrap();
            for n in 1..=SPENDERS {
                pool.insert(child_of(&base, &root, n), None, &weight).unwrap();
            }
            let root_key = *pool.txs_by_signature[root.body.kernels()[0].excess_sig.get_signature()]
                .first()
                .unwrap();

            let timer = std::time::Instant::now();
            let removed = pool.remove_transactions_and_descendants([root_key]).unwrap();
            println!("Removed {SPENDERS} spenders of one output in {:.2?}", timer.elapsed());
            assert_eq!(removed, usize::try_from(SPENDERS + 1).unwrap());
            assert_eq!(pool.len(), 0);
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn the_sweep_visits_every_height_reading_transaction_in_bounded_batches() {
            let key_manager = KeyManager::new_random().unwrap();
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            pool.insert(
                tx_with_input(script!(Nop).unwrap(), Covenant::default(), &key_manager),
                None,
                &weight,
            )
            .unwrap();
            for _ in 0..5 {
                let tx = tx_with_input(
                    script!(CheckHeightVerify(1)).unwrap(),
                    Covenant::default(),
                    &key_manager,
                );
                pool.insert(tx, None, &weight).unwrap();
            }
            let all = pool.txs_reading_height.keys().copied().collect::<Vec<_>>();
            let tip = FixedHash::from([1u8; 32]);

            // Batches of 2 visit every one of the 5, then wrap around
            let mut visited = Vec::new();
            for _ in 0..3 {
                let (batch, cursor) = pool.height_reading_sweep_batch(2, u64::MAX, 10, &tip);
                visited.extend(batch.iter().map(|(key, ..)| *key));
                pool.set_sweep_cursor(cursor);
            }
            assert_eq!(visited.len(), 6);
            assert_eq!(visited[..5], all[..]);
            assert_eq!(visited[5], all[0]);

            // One that passed for this block, and one validated for a later height, are visited but not returned
            pool.record_height_check_passed(all[0], tip);
            let later = tx_with_input(
                script!(CheckHeightVerify(1)).unwrap(),
                Covenant::default(),
                &key_manager,
            );
            pool.insert_with_height_validation(later, None, &weight, HeightValidation {
                height: 11,
                script_keys: Vec::new(),
            })
            .unwrap();
            pool.set_sweep_cursor(0);
            let (batch, _) = pool.height_reading_sweep_batch(100, u64::MAX, 10, &tip);
            assert_eq!(batch.iter().map(|(key, ..)| *key).collect::<Vec<_>>(), all[1..]);
        }

        #[test]
        fn a_sweep_batch_is_bounded_by_re_check_work() {
            let key_manager = KeyManager::new_random().unwrap();
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            // A transaction with 3 height-reading input scripts, then 3 with one each, then one with a height-reading
            // covenant
            let tx = tx!(MicroMinotari(15_000), fee: MicroMinotari(5), inputs: 3, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let mut inputs = tx.body.inputs().clone();
            for input in &mut inputs {
                match &mut input.spent_output {
                    SpentOutput::OutputData { script, .. } => *script = script!(CheckHeightVerify(1)).unwrap(),
                    SpentOutput::OutputHash(_) => panic!("Expected a full input"),
                }
            }
            let multi = Arc::new(Transaction::new(
                inputs,
                tx.body.outputs().clone(),
                tx.body.kernels().clone(),
                tx.offset.clone(),
                tx.script_offset.clone(),
            ));
            pool.insert(multi.clone(), None, &weight).unwrap();
            for _ in 0..3 {
                let single = tx_with_input(
                    script!(CheckHeightVerify(1)).unwrap(),
                    Covenant::default(),
                    &key_manager,
                );
                pool.insert(single, None, &weight).unwrap();
            }
            let covenant_reads = tx_with_input(
                script!(Nop).unwrap(),
                covenant!(not(absolute_height(@uint(10)))).unwrap(),
                &key_manager,
            );
            pool.insert(covenant_reads, None, &weight).unwrap();
            let all = pool.txs_reading_height.keys().copied().collect::<Vec<_>>();
            assert_eq!(pool.txs_reading_height[&all[0]].work, 3 * SCRIPT_INPUT_WORK);
            assert_eq!(pool.txs_reading_height[&all[1]].work, SCRIPT_INPUT_WORK);
            assert_eq!(pool.txs_reading_height[&all[4]].work, MAX_COVENANT_WORK_PER_BODY);
            let tip = FixedHash::from([1u8; 32]);
            let next = |pool: &mut UnconfirmedPool, max_work| {
                let (batch, cursor) = pool.height_reading_sweep_batch(100, max_work, 10, &tip);
                pool.set_sweep_cursor(cursor);
                batch.iter().map(|(key, ..)| *key).collect::<Vec<_>>()
            };

            // The 3 scripts and one more fit a budget of 4 scripts, the next one does not
            assert_eq!(next(&mut pool, 4 * SCRIPT_INPUT_WORK), all[..2]);
            assert_eq!(next(&mut pool, 4 * SCRIPT_INPUT_WORK), all[2..4]);
            // The covenant evaluation takes a whole budget, alone
            assert_eq!(next(&mut pool, 4 * SCRIPT_INPUT_WORK), all[4..]);
            // Over the budget on its own, a transaction is still taken, alone
            assert_eq!(next(&mut pool, 2 * SCRIPT_INPUT_WORK), all[..1]);
            assert_eq!(next(&mut pool, 2 * SCRIPT_INPUT_WORK), all[1..3]);
            assert_eq!(next(&mut pool, MAX_COVENANT_WORK_PER_BODY + SCRIPT_INPUT_WORK), vec![
                all[3], all[4]
            ]);
        }

        #[test]
        fn the_template_filter_leaves_out_transactions_and_their_descendants() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            // A parent whose kernel is locked until height 5, and its child
            let mut locked = (*synthetic_tx(&base, 0, 5)).clone();
            let mut kernel = locked.body.kernels()[0].clone();
            kernel.lock_height = 5;
            locked = Transaction::new(
                vec![],
                locked.body.outputs().clone(),
                vec![kernel],
                Default::default(),
                Default::default(),
            );
            let locked = Arc::new(locked);
            let child = child_of(&base, &locked, 1);
            pool.insert(locked.clone(), None, &weight).unwrap();
            pool.insert(child.clone(), Some(vec![locked.body.outputs()[0].hash()]), &weight)
                .unwrap();
            // A transaction whose input script reads the height
            let height_reading = tx_with_input(
                script!(CheckHeightVerify(1)).unwrap(),
                Covenant::default(),
                &key_manager,
            );
            pool.insert(height_reading.clone(), None, &weight).unwrap();
            let height_reading_key = *pool.txs_reading_height.keys().next().unwrap();
            let select = |pool: &UnconfirmedPool, filter: &TemplateFilter| {
                pool.fetch_template_txs(u64::MAX, max_weight(), filter)
                    .unwrap()
                    .retrieved_transactions
            };

            let mut filter = TemplateFilter::unrestricted();
            filter.height = 4;
            assert_eq!(select(&pool, &filter), vec![height_reading.clone()]);
            filter.height = 5;
            assert_eq!(select(&pool, &filter).len(), 3);

            // An excluded parent leaves out its child
            let locked_key = *pool.txs_by_signature[locked.body.kernels()[0].excess_sig.get_signature()]
                .first()
                .unwrap();
            filter.excluded.insert(locked_key);
            assert_eq!(select(&pool, &filter), vec![height_reading.clone()]);
            filter.excluded.clear();

            // Only checked height-reading transactions, once it passed for this block
            let tip = FixedHash::from([1u8; 32]);
            filter.only_checked_at = Some(tip);
            assert_eq!(select(&pool, &filter).len(), 2);
            pool.record_height_check_passed(height_reading_key, tip);
            assert_eq!(select(&pool, &filter).len(), 3);
        }

        /// A transaction producing the same outputs as `tx`, with the kernel of another synthetic transaction with fee
        /// `fee`
        fn same_outputs_as(base: &Transaction, tx: &Transaction, n: u64, fee: u64) -> Arc<Transaction> {
            let other = synthetic_tx(base, n, fee);
            Arc::new(Transaction::new(
                vec![],
                tx.body.outputs().clone(),
                other.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        }

        fn key_of(pool: &UnconfirmedPool, tx: &Transaction) -> TransactionKey {
            *pool.txs_by_signature[tx.body.kernels()[0].excess_sig.get_signature()]
                .first()
                .unwrap()
        }

        #[test]
        fn removing_one_producer_of_an_output_keeps_its_spenders() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let parent = synthetic_tx(&base, 0, 5);
            let duplicate = same_outputs_as(&base, &parent, 1, 5);
            let child = child_of(&base, &parent, 2);
            pool.insert(parent.clone(), None, &weight).unwrap();
            pool.insert(duplicate.clone(), None, &weight).unwrap();
            pool.insert(child.clone(), Some(vec![parent.body.outputs()[0].hash()]), &weight)
                .unwrap();

            // The parent still produces the child's input
            let removed = pool
                .remove_transactions_and_descendants([key_of(&pool, &duplicate)])
                .unwrap();
            assert_eq!(removed, 1);
            assert!(pool.has_tx_with_excess_sig(&parent.body.kernels()[0].excess_sig));
            assert!(pool.has_tx_with_excess_sig(&child.body.kernels()[0].excess_sig));
            assert!(pool.check_data_consistency());

            // Its last producer gone, the child goes too
            let removed = pool
                .remove_transactions_and_descendants([key_of(&pool, &parent)])
                .unwrap();
            assert_eq!(removed, 2);
            assert_eq!(pool.len(), 0);
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn a_child_is_selected_through_a_producer_the_template_allows() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            // Two producers of the same output; the higher priority one is left out of the template
            let preferred = synthetic_tx(&base, 0, 50);
            let allowed = same_outputs_as(&base, &preferred, 1, 5);
            let child = child_of(&base, &preferred, 2);
            pool.insert(preferred.clone(), None, &weight).unwrap();
            pool.insert(allowed.clone(), None, &weight).unwrap();
            pool.insert(child.clone(), Some(vec![preferred.body.outputs()[0].hash()]), &weight)
                .unwrap();
            let mut filter = TemplateFilter::unrestricted();
            filter.excluded.insert(key_of(&pool, &preferred));

            let selected = pool
                .fetch_template_txs(u64::MAX, max_weight(), &filter)
                .unwrap()
                .retrieved_transactions;
            assert_eq!(selected.len(), 2);
            assert!(selected.contains(&allowed));
            assert!(selected.contains(&child));
        }

        #[test]
        fn looking_for_an_allowed_producer_stops_after_a_few() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            // Producers of the same output in priority order, all left out of the template but the last, which comes
            // after the cap
            let first = synthetic_tx(&base, 0, 100);
            pool.insert(first.clone(), None, &weight).unwrap();
            let mut filter = TemplateFilter::unrestricted();
            filter.excluded.insert(key_of(&pool, &first));
            for n in 1..=u64::try_from(MAX_EXTRA_PRODUCERS_PER_LOOKUP).unwrap() {
                let producer = same_outputs_as(&base, &first, n, 100 - n);
                pool.insert(producer.clone(), None, &weight).unwrap();
                filter.excluded.insert(key_of(&pool, &producer));
            }
            let allowed = same_outputs_as(&base, &first, 50, 5);
            pool.insert(allowed.clone(), None, &weight).unwrap();
            let child = child_of(&base, &first, 51);
            pool.insert(child.clone(), Some(vec![first.body.outputs()[0].hash()]), &weight)
                .unwrap();

            let keys = pool.txs_by_output[&first.body.outputs()[0].hash()].clone();
            let mut links_left = 1_000;
            let (producer, examined) = pool.find_template_producer(&keys, &filter, &mut links_left).unwrap();
            assert_eq!(producer.key, key_of(&pool, &first));
            assert_eq!(examined, MAX_EXTRA_PRODUCERS_PER_LOOKUP);
            assert_eq!(links_left, 1_000 - MAX_EXTRA_PRODUCERS_PER_LOOKUP);

            // So the child is left out, and the allowed producer is selected on its own
            let selected = pool
                .fetch_template_txs(u64::MAX, max_weight(), &filter)
                .unwrap()
                .retrieved_transactions;
            assert_eq!(selected, vec![allowed]);
        }

        /// `N` children of `root` (see `child_of`) that each also spend the same `M` shared inputs, with the shared
        /// inputs
        fn children_sharing_inputs(
            base: &Transaction,
            root: &Transaction,
        ) -> (
            Vec<Arc<Transaction>>,
            Vec<tari_transaction_components::transaction_components::TransactionInput>,
        ) {
            let shared = distinct_inputs(&base.body.inputs()[0], SHARED_INPUTS);
            let children = (1..=u64::try_from(CHILDREN).unwrap())
                .map(|n| {
                    let child = child_of(base, root, n);
                    let mut inputs = child.body.inputs().clone();
                    inputs.extend(shared.iter().cloned());
                    Arc::new(Transaction::new(
                        inputs,
                        child.body.outputs().clone(),
                        child.body.kernels().clone(),
                        Default::default(),
                        Default::default(),
                    ))
                })
                .collect();
            (children, shared)
        }

        const CHILDREN: usize = 2_000;
        const SHARED_INPUTS: usize = 20;
        /// Linear in the index entries touched, not quadratic in the transactions sharing them
        const MAX_PRUNE_VISITS: usize = 2 * CHILDREN * (SHARED_INPUTS + 2);

        #[test]
        fn removing_descendants_sharing_many_inputs_prunes_each_shared_entry_once() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let root = synthetic_tx(&base, 0, 5);
            pool.insert(root.clone(), None, &weight).unwrap();
            let (children, _) = children_sharing_inputs(&base, &root);
            for child in children {
                pool.insert(child, None, &weight).unwrap();
            }

            let removed = pool
                .remove_transactions_and_descendants([key_of(&pool, &root)])
                .unwrap();
            assert_eq!(removed, CHILDREN + 1);
            assert_eq!(pool.len(), 0);
            assert!(pool.check_data_consistency());
            assert!(pool.prune_visits <= MAX_PRUNE_VISITS, "{} visits", pool.prune_visits);
        }

        #[test]
        fn publishing_a_spend_shared_by_many_pool_transactions_prunes_each_shared_entry_once() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let root = synthetic_tx(&base, 0, 5);
            pool.insert(root.clone(), None, &weight).unwrap();
            let (children, shared) = children_sharing_inputs(&base, &root);
            for child in &children {
                pool.insert(child.clone(), None, &weight).unwrap();
            }

            let block = Block::new(
                tari_node_components::blocks::BlockHeader::new(0),
                AggregateBody::new_unsorted(vec![shared[0].clone()], vec![], vec![]),
            );
            let removed = pool
                .remove_published_and_discard_deprecated_transactions(&block)
                .unwrap();
            assert_eq!(removed.len(), CHILDREN);
            assert!(children.iter().all(|child| removed.contains(child)));
            assert_eq!(pool.len(), 1);
            assert!(pool.has_tx_with_excess_sig(&root.body.kernels()[0].excess_sig));
            assert!(pool.check_data_consistency());
            assert!(pool.prune_visits <= MAX_PRUNE_VISITS, "{} visits", pool.prune_visits);
        }

        #[test]
        fn removing_producers_in_a_batch_keeps_the_rest_in_priority_order() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let first = synthetic_tx(&base, 0, 30);
            pool.insert(first.clone(), None, &weight).unwrap();
            for (n, fee) in [(1, 10), (2, 50), (3, 20), (4, 40)] {
                pool.insert(same_outputs_as(&base, &first, n, fee), None, &weight)
                    .unwrap();
            }
            let hash = first.body.outputs()[0].hash();
            let producers = pool.txs_by_output[&hash].clone();
            let fees = |pool: &UnconfirmedPool, keys: &[TransactionKey]| {
                keys.iter()
                    .map(|key| pool.tx_by_key[key].transaction.body.get_total_fee().unwrap().0)
                    .collect::<Vec<_>>()
            };
            assert!(fees(&pool, &producers).is_sorted_by(|a, b| a >= b));

            let removed = pool.remove_transactions([producers[1], producers[3]]).unwrap();
            assert_eq!(removed.len(), 2);
            assert_eq!(pool.txs_by_output[&hash], vec![
                producers[0],
                producers[2],
                producers[4]
            ]);
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn removing_a_transaction_missing_from_the_signature_index_fails() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let parent = synthetic_tx(&base, 0, 5);
            let tx = child_of(&base, &parent, 1);
            pool.insert(tx.clone(), None, &weight).unwrap();
            let key = key_of(&pool, &tx);
            pool.txs_by_signature.clear();
            assert!(matches!(
                pool.remove_transaction(key),
                Err(UnconfirmedPoolError::StorageOutofSync)
            ));
            // The other indexes are still pruned
            assert!(pool.txs_by_output.is_empty());
            assert!(pool.txs_by_spent_output.is_empty());
            assert!(pool.txs_by_output_commitment.is_empty());
            assert!(pool.txs_by_kernel_excess.is_empty());
        }

        #[test]
        fn removing_many_roots_sharing_inputs_prunes_each_shared_entry_once() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let root = synthetic_tx(&base, 0, 5);
            pool.insert(root.clone(), None, &weight).unwrap();
            let (children, _) = children_sharing_inputs(&base, &root);
            for child in &children {
                pool.insert(child.clone(), None, &weight).unwrap();
            }

            let keys = children.iter().map(|child| key_of(&pool, child)).collect::<Vec<_>>();
            let removed = pool.remove_transactions_and_descendants(keys).unwrap();
            assert_eq!(removed, CHILDREN);
            assert_eq!(pool.len(), 1);
            assert!(pool.has_tx_with_excess_sig(&root.body.kernels()[0].excess_sig));
            assert!(pool.check_data_consistency());
            assert!(pool.prune_visits <= MAX_PRUNE_VISITS, "{} visits", pool.prune_visits);
        }

        /// A transaction with the same inputs and outputs as `tx`, and the kernel of another synthetic transaction
        fn same_spend_and_outputs_as(base: &Transaction, tx: &Transaction, n: u64) -> Arc<Transaction> {
            let other = synthetic_tx(base, n, 5);
            Arc::new(Transaction::new(
                tx.body.inputs().clone(),
                tx.body.outputs().clone(),
                other.body.kernels().clone(),
                Default::default(),
                Default::default(),
            ))
        }

        #[test]
        fn an_output_whose_producers_are_all_removed_in_one_walk_loses_its_spenders() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let root = synthetic_tx(&base, 0, 5);
            // Two children of the root that both produce the output the grandchild spends
            let first = child_of(&base, &root, 1);
            let second = same_spend_and_outputs_as(&base, &first, 2);
            let grandchild = child_of(&base, &first, 3);
            for tx in [&root, &first, &second, &grandchild] {
                pool.insert(tx.clone(), None, &weight).unwrap();
            }

            let removed = pool
                .remove_transactions_and_descendants([key_of(&pool, &root)])
                .unwrap();
            assert_eq!(removed, 4);
            assert_eq!(pool.len(), 0);
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn an_output_with_a_producer_left_after_a_walk_keeps_its_spenders() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let root = synthetic_tx(&base, 0, 5);
            let first = child_of(&base, &root, 1);
            let second = same_spend_and_outputs_as(&base, &first, 2);
            let grandchild = child_of(&base, &first, 3);
            // An unrelated producer of the same output
            let other_producer = same_outputs_as(&base, &first, 4, 5);
            for tx in [&root, &first, &second, &grandchild, &other_producer] {
                pool.insert(tx.clone(), None, &weight).unwrap();
            }

            let removed = pool
                .remove_transactions_and_descendants([key_of(&pool, &root)])
                .unwrap();
            assert_eq!(removed, 3);
            assert_eq!(pool.len(), 2);
            assert!(pool.has_tx_with_excess_sig(&grandchild.body.kernels()[0].excess_sig));
            assert!(pool.has_tx_with_excess_sig(&other_producer.body.kernels()[0].excess_sig));
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn a_block_returns_the_transactions_it_removes_parents_first() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let parent = synthetic_tx(&base, 0, 5);
            let child = child_of(&base, &parent, 1);
            pool.insert(parent.clone(), None, &weight).unwrap();
            pool.insert(child.clone(), None, &weight).unwrap();

            // The child is matched by the block's input (and repeatedly), before the parent is matched by its output
            let input = child.body.inputs()[0].clone();
            let block = Block::new(
                tari_node_components::blocks::BlockHeader::new(0),
                AggregateBody::new_unsorted(
                    vec![input.clone(), input],
                    vec![parent.body.outputs()[0].clone()],
                    vec![],
                ),
            );
            let removed = pool
                .remove_published_and_discard_deprecated_transactions(&block)
                .unwrap();
            assert_eq!(removed, vec![parent, child]);
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn a_block_matching_a_pool_transaction_in_every_phase_removes_it_once() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let parent = synthetic_tx(&base, 0, 5);
            // Its input, output, kernel signature, commitment and excess are all in the block
            let tx = child_of(&base, &parent, 1);
            let unrelated = synthetic_tx(&base, 2, 5);
            pool.insert(tx.clone(), None, &weight).unwrap();
            pool.insert(unrelated.clone(), None, &weight).unwrap();

            let block = Block::new(tari_node_components::blocks::BlockHeader::new(0), tx.body.clone());
            let removed = pool
                .remove_published_and_discard_deprecated_transactions(&block)
                .unwrap();
            assert_eq!(removed, vec![tx]);
            assert_eq!(pool.len(), 1);
            assert!(pool.has_tx_with_excess_sig(&unrelated.body.kernels()[0].excess_sig));
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn only_a_transaction_with_the_same_kernel_set_is_a_duplicate() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let h = synthetic_tx(&base, 0, 5);
            let x = synthetic_tx(&base, 1, 5);
            #[allow(clippy::arithmetic_side_effects)] // Transaction aggregation, not integer arithmetic
            let mut a = (*h).clone() + (*x).clone();
            a.body.sort();
            let a = Arc::new(a);
            let c = child_of(&base, &h, 2);
            let h_output = h.body.outputs()[0].hash();

            for (first, second) in [(&a, &h), (&h, &a)] {
                let mut pool = pool();
                pool.insert(first.clone(), None, &weight).unwrap();
                assert!(pool.has_transaction(first));
                assert!(!pool.has_transaction(second));
                assert!(!pool.has_transaction(&x));
                pool.insert(first.clone(), None, &weight).unwrap();
                assert_eq!(pool.tx_by_key.len(), 1);

                pool.insert(second.clone(), None, &weight).unwrap();
                assert_eq!(pool.tx_by_key.len(), 2);
                assert!(pool.has_transaction(second));
                assert_eq!(pool.txs_by_output[&h_output].len(), 2);
                pool.insert(c.clone(), Some(vec![h_output]), &weight).unwrap();
                assert!(pool.check_data_consistency());

                let a_key = pool.tx_by_kernel_set[&kernel_set(&a).unwrap()];
                assert_eq!(pool.remove_transactions_and_descendants([a_key]).unwrap(), 1);
                assert!(pool.has_transaction(&h));
                assert!(pool.has_transaction(&c));
                assert!(!pool.has_transaction(&a));
                assert!(pool.check_data_consistency());
            }
        }

        #[test]
        fn the_exact_kernel_set_is_found_among_many_entries_sharing_kernels() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let shared = synthetic_tx(&base, 0, 5).body.kernels()[0].clone();
            let with_kernels =
                |n: u64, kernels: Vec<tari_transaction_components::transaction_components::TransactionKernel>| {
                    let tx = synthetic_tx(&base, n, 5);
                    Arc::new(Transaction::new(
                        vec![],
                        tx.body.outputs().clone(),
                        kernels,
                        Default::default(),
                        Default::default(),
                    ))
                };
            let mut pool = pool();
            let entries = (1..=300)
                .map(|n| {
                    let own = synthetic_tx(&base, n, 5).body.kernels()[0].clone();
                    let tx = with_kernels(n, vec![shared.clone(), own]);
                    pool.insert(tx.clone(), None, &weight).unwrap();
                    tx
                })
                .collect::<Vec<_>>();
            assert_eq!(pool.tx_by_key.len(), 300);

            let target = &entries[150];
            assert!(pool.has_transaction(target));
            assert!(!pool.has_transaction(&with_kernels(0, vec![shared.clone()])));
            let mut superset = target.body.kernels().clone();
            superset.push(entries[151].body.kernels()[1].clone());
            // A union of pool transactions counts as stored
            assert!(pool.has_transaction(&with_kernels(0, superset)));
            // A query repeating a kernel does not match an entry with that kernel and another one
            assert!(!pool.has_transaction(&with_kernels(0, vec![shared.clone(), shared])));
        }

        #[test]
        fn a_union_of_pool_transactions_counts_as_stored() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let t1 = synthetic_tx(&base, 0, 5);
            let t2 = synthetic_tx(&base, 1, 5);
            pool.insert(t1.clone(), None, &weight).unwrap();
            pool.insert(t2.clone(), None, &weight).unwrap();

            #[allow(clippy::arithmetic_side_effects)] // Transaction aggregation, not integer arithmetic
            let union = Arc::new((*t1).clone() + (*t2).clone());
            assert!(pool.has_transaction(&union));
            pool.insert(union, None, &weight).unwrap();
            assert_eq!(pool.tx_by_key.len(), 2);

            // With a kernel that is not in the pool, it is a new transaction
            #[allow(clippy::arithmetic_side_effects)] // Transaction aggregation, not integer arithmetic
            let with_new_kernel = Arc::new((*t1).clone() + (*t2).clone() + (*synthetic_tx(&base, 2, 5)).clone());
            assert!(!pool.has_transaction(&with_new_kernel));
            pool.insert(with_new_kernel, None, &weight).unwrap();
            assert_eq!(pool.tx_by_key.len(), 3);
            assert!(pool.check_data_consistency());

            // Still a union of `t1` and `t2`, although one pool transaction also contains it
            #[allow(clippy::arithmetic_side_effects)] // Transaction aggregation, not integer arithmetic
            let union = Arc::new((*t1).clone() + (*t2).clone());
            assert!(pool.has_transaction(&union));
            pool.insert(union, None, &weight).unwrap();
            assert_eq!(pool.tx_by_key.len(), 3);
        }

        #[test]
        fn a_union_is_found_among_large_pool_transactions_sharing_its_kernels() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let singles = (0..200).map(|n| synthetic_tx(&base, n, 5)).collect::<Vec<_>>();
            let kernels = singles
                .iter()
                .map(|tx| tx.body.kernels()[0].clone())
                .collect::<Vec<_>>();
            let with_kernels = |n: u64, kernels: Vec<_>| {
                Arc::new(Transaction::new(
                    vec![],
                    synthetic_tx(&base, n, 5).body.outputs().clone(),
                    kernels,
                    Default::default(),
                    Default::default(),
                ))
            };
            // Large pool transactions with every kernel of the query and one more each, so not subsets of it
            for n in 0..3 {
                let mut large = kernels.clone();
                large.push(synthetic_tx(&base, 1_000 + n, 5).body.kernels()[0].clone());
                pool.insert(with_kernels(2_000 + n, large), None, &weight).unwrap();
            }
            let query = with_kernels(3_000, kernels);
            assert!(!pool.has_transaction(&query));

            for single in &singles {
                pool.insert(single.clone(), None, &weight).unwrap();
            }
            assert!(pool.has_transaction(&query));

            let key = pool.tx_by_kernel_set[&kernel_set(&singles[100]).unwrap()];
            pool.remove_transaction(key).unwrap();
            assert!(!pool.has_transaction(&query));
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn kernels_spread_over_pool_transactions_that_are_not_subsets_are_not_a_union() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = pool();
            let [h1, x1, h2, x2] = [0, 1, 2, 3].map(|n| (*synthetic_tx(&base, n, 5)).clone());
            #[allow(clippy::arithmetic_side_effects)] // Transaction aggregation, not integer arithmetic
            let (p1, p2, t) = (h1.clone() + x1, h2.clone() + x2, Arc::new(h1 + h2));
            pool.insert(Arc::new(p1), None, &weight).unwrap();
            pool.insert(Arc::new(p2), None, &weight).unwrap();

            // No pool transaction is a subset of `t`, so it is validated and stored
            assert!(!pool.has_transaction(&t));
            pool.insert(t.clone(), None, &weight).unwrap();
            assert_eq!(pool.len(), 3);
            assert!(pool.has_transaction(&t));
            assert!(pool.check_data_consistency());
        }

        #[test]
        fn the_kernel_set_index_follows_every_removal() {
            let key_manager = KeyManager::new_random().unwrap();
            let base = tx!(MicroMinotari(5_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .unwrap()
                .0;
            let weight = TransactionWeight::latest();
            let mut pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
                storage_capacity: 3,
                weight_tx_skip_count: 3,
                min_fee: 0,
            });
            for n in 0..3 {
                pool.insert(synthetic_tx(&base, n, 5), None, &weight).unwrap();
            }
            let assert_index_consistent = |pool: &UnconfirmedPool| {
                assert!(pool.check_data_consistency());
                assert_eq!(pool.tx_by_kernel_set.len(), pool.tx_by_key.len());
            };

            // Eviction of the lowest priority transaction from a full pool
            let high_fee = synthetic_tx(&base, 3, 50);
            pool.insert(high_fee.clone(), None, &weight).unwrap();
            assert_eq!(pool.tx_by_key.len(), 3);
            assert_index_consistent(&pool);

            let high_fee_key = pool.tx_by_kernel_set[&kernel_set(&high_fee).unwrap()];
            pool.remove_transactions_and_descendants([high_fee_key]).unwrap();
            assert!(!pool.has_transaction(&high_fee));
            assert_index_consistent(&pool);

            let published = pool.snapshot().remove(0);
            let block = Block::new(
                tari_node_components::blocks::BlockHeader::new(0),
                published.body.clone(),
            );
            pool.remove_published_and_discard_deprecated_transactions(&block)
                .unwrap();
            assert!(!pool.has_transaction(&published));
            assert_index_consistent(&pool);

            // A removed transaction can be stored again
            pool.insert(published.clone(), None, &weight).unwrap();
            assert!(pool.has_transaction(&published));
            assert_index_consistent(&pool);

            pool.drain_all_mempool_transactions();
            assert!(pool.tx_by_kernel_set.is_empty());
            assert_index_consistent(&pool);
        }
    }
}
