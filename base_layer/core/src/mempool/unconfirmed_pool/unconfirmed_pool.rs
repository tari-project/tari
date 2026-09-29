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
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet},
    sync::Arc,
};

use log::*;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{CompressedSignature, FixedHash, HashOutput, PrivateKey};
use tari_comms::protocol::rpc::RPC_MAX_FRAME_SIZE;
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    MicroMinotari,
    rpc::models::FeePerGramStat,
    transaction_components::{Transaction, TransactionError},
    weight::TransactionWeight,
};
use tokio::time::Instant;

use crate::mempool::{
    MempoolError,
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
/// from a template can be both propagated and synced. Transaction bodies are measured by their borsh-serialized size;
/// the 1 MiB margin covers the coinbase, the header and protobuf encoding overhead. This is a local block-building
/// policy, not a consensus rule.
///
/// TODO: this is a rollout value, derived from the smaller RPC frame of not-yet-upgraded nodes (5 MiB) so that they can
/// still sync every block this node builds. Raise it to `RPC_MAX_FRAME_SIZE - 1 MiB` once the network has upgraded.
pub const MAX_BLOCK_TEMPLATE_BODY_BYTES: usize = min_usize(LEGACY_RPC_MAX_FRAME_SIZE, RPC_MAX_FRAME_SIZE) - 1024 * 1024;
const _: () = assert!(MAX_BLOCK_TEMPLATE_BODY_BYTES < LEGACY_RPC_MAX_FRAME_SIZE);
const _: () = assert!(MAX_BLOCK_TEMPLATE_BODY_BYTES < RPC_MAX_FRAME_SIZE);

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

/// How an ancestor walk ended
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalkOutcome {
    Complete,
    /// The pass's [MAX_WALKED_FRAMES_PER_PASS] or [MAX_FOLLOWED_LINKS_PER_PASS] budget ran out
    OutOfFrames,
    /// The candidate has more than [MAX_UNSELECTED_ANCESTORS_PER_CANDIDATE] unselected ancestors
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
/// [UnconfirmedPool::fetch_highest_priority_txs]). Without byte pressure, and for the stored priority, only the real
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
    tx_by_priority: BTreeMap<FeePriority, TransactionKey>,
    txs_by_output: HashMap<HashOutput, Vec<TransactionKey>>,
    /// For each output spent by a pool transaction, the pool transactions spending it (from their stored input hashes)
    txs_by_spent_output: HashMap<HashOutput, Vec<TransactionKey>>,
    txs_by_unique_id: HashMap<[u8; 32], Vec<TransactionKey>>,
    /// The template byte budget used for selection and for effective weights (see [effective_weight])
    max_body_bytes: usize,
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
    /// Whether the pass reached [MAX_WALKED_FRAMES_PER_PASS]
    #[cfg_attr(not(test), allow(dead_code))]
    frame_budget_exhausted: bool,
    /// The candidate on which the walking budget ran out, if it did
    #[cfg_attr(not(test), allow(dead_code))]
    out_of_frames_candidate: Option<TransactionKey>,
    /// The transactions the pass's walks visited (see [MAX_WALKED_FRAMES_PER_PASS])
    #[cfg_attr(not(test), allow(dead_code))]
    walk_work: usize,
    /// The dependency links the pass followed (see [MAX_FOLLOWED_LINKS_PER_PASS])
    #[cfg_attr(not(test), allow(dead_code))]
    links_followed: usize,
    /// Whether the pass ended before considering every candidate
    ended_early: bool,
    /// The candidates whose ancestors this pass walked and that it then dropped (for conflicting, or for not fitting
    /// by weight or bytes). Never a candidate that was still queued.
    walked_dropped: HashSet<TransactionKey>,
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
    /// Candidates to leave out of this pass (see [UnconfirmedPool::fetch_selection])
    excluded: HashSet<TransactionKey>,
    walked_dropped: HashSet<TransactionKey>,
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
            tx_by_priority: BTreeMap::new(),
            txs_by_output: HashMap::new(),
            txs_by_spent_output: HashMap::new(),
            txs_by_unique_id: HashMap::new(),
            max_body_bytes: MAX_BLOCK_TEMPLATE_BODY_BYTES,
        }
    }

    /// Use a different template byte budget, for tests
    #[cfg(test)]
    pub(crate) fn with_max_body_bytes(mut self, max_body_bytes: usize) -> Self {
        self.max_body_bytes = max_body_bytes;
        self
    }

    /// Insert a new transaction into the UnconfirmedPool. Low priority transactions will be removed to make space for
    /// higher priority transactions. The lowest priority transactions will be removed when the maximum capacity is
    /// reached and the new transaction has a higher priority than the currently stored lowest priority transaction.
    pub fn insert(
        &mut self,
        tx: Arc<Transaction>,
        dependent_outputs: Option<Vec<HashOutput>>,
        transaction_weighting: &TransactionWeight,
    ) -> Result<(), UnconfirmedPoolError> {
        if tx
            .body
            .kernels()
            .iter()
            .all(|k| self.txs_by_signature.contains_key(k.excess_sig.get_signature()))
        {
            return Ok(());
        }

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
        for kernel in prioritized_tx.transaction.body.kernels() {
            let sig = kernel.excess_sig.get_signature();
            self.txs_by_signature.entry(sig.clone()).or_default().push(new_key);
        }

        debug!(
            target: LOG_TARGET,
            "Inserted transaction {prioritized_tx} into unconfirmed pool:"
        );
        self.tx_by_key.insert(new_key, prioritized_tx);

        Ok(())
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

    /// Returns the subset of provided output hashes that exist in the unconfirmed pool.
    pub fn filter_outputs(&self, output_hashes: &[HashOutput]) -> Vec<HashOutput> {
        output_hashes
            .iter()
            .filter(|hash| self.txs_by_output.contains_key(*hash))
            .copied()
            .collect()
    }

    /// Returns a set of the highest priority unconfirmed transactions, that can be included in a block. The selection
    /// is limited by `total_weight` (real weight) and by the template byte budget ([MAX_BLOCK_TEMPLATE_BODY_BYTES]).
    ///
    /// Selection is ordered by fee per gram of real weight. If the byte budget bound (some candidate was passed over
    /// only because its bytes did not fit), selection is run again ordered by fee per gram of effective weight (see
    /// [effective_weight]), and whichever selection earns the higher total fee is returned (the first on a tie). Under
    /// byte pressure a byte-heavy transaction must therefore pay for the share of the byte budget it consumes.
    /// `max_block_transaction_weight` is the consensus maximum, used only for that byte-to-weight ratio.
    pub fn fetch_highest_priority_txs(
        &self,
        total_weight: u64,
        max_block_transaction_weight: u64,
    ) -> Result<RetrieveResults, UnconfirmedPoolError> {
        self.fetch_highest_priority_txs_with_byte_budget(
            total_weight,
            self.max_body_bytes,
            max_block_transaction_weight,
        )
    }

    fn fetch_highest_priority_txs_with_byte_budget(
        &self,
        total_weight: u64,
        max_body_bytes: usize,
        max_block_transaction_weight: u64,
    ) -> Result<RetrieveResults, UnconfirmedPoolError> {
        Ok(self
            .fetch_selection(total_weight, max_body_bytes, max_block_transaction_weight)?
            .results)
    }

    /// Runs the selection described in [UnconfirmedPool::fetch_highest_priority_txs] and returns the chosen pass
    fn fetch_selection(
        &self,
        total_weight: u64,
        max_body_bytes: usize,
        max_block_transaction_weight: u64,
    ) -> Result<Selection, UnconfirmedPoolError> {
        let by_weight = self.select_txs_with(
            total_weight,
            max_body_bytes,
            Ranking::Weight,
            max_block_transaction_weight,
            HashSet::new(),
        )?;
        if !by_weight.byte_bound {
            return Ok(by_weight);
        }
        // If the first pass ended early, leave the candidates it walked and dropped out of the second pass, so that it
        // spends its budget on the rest
        let excluded = if by_weight.ended_early {
            by_weight.walked_dropped.clone()
        } else {
            HashSet::new()
        };
        let by_effective_weight = self.select_txs_with(
            total_weight,
            max_body_bytes,
            Ranking::EffectiveWeight {
                max_block_transaction_weight,
                max_body_bytes,
            },
            max_block_transaction_weight,
            excluded,
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
            // Cheap checks before walking the transaction's ancestors. If it spends an input that is already spent, it
            // can never be mined with the selection: drop it without counting it.
            // If it cannot fit on its own, neither can its branch
            if prioritized_transaction.body_size > state.remaining_bytes() {
                state.byte_skip();
                continue;
            }
            // If it spends an output that the selection already spends, it can never be mined with it: drop it without
            // counting it (O(1))
            if state.conflicting.contains(tx_key) {
                state.conflict_drop();
                continue;
            }
            // If its dependencies are already known to be too heavy, so is its branch
            let Some(weight_bound) = self.ancestor_weight_bound(
                prioritized_transaction,
                &state.selected_txs,
                &state.ancestor_weight_bounds,
                &mut state.links_left,
            )?
            else {
                // Out of walking budget (see the `OutOfFrames` case below)
                state.frame_budget_exhausted = true;
                state.out_of_frames_candidate = Some(*tx_key);
                state.byte_bound = true;
                exhausted = false;
                break;
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
                &mut state.ancestor_weight_bounds,
            )?;
            match outcome {
                WalkOutcome::Complete => {},
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
            if conflicts {
                // An ancestor spends an input that is already spent: the branch can never be mined with the selection
                state.conflict_drop();
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
                state.walked_dropped.insert(*tx_key);
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
                    state.walked_dropped.insert(*tx_key);
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
            frame_budget_exhausted: state.frame_budget_exhausted,
            out_of_frames_candidate: state.out_of_frames_candidate,
            walk_work: MAX_WALKED_FRAMES_PER_PASS.saturating_sub(state.frames_left),
            links_followed: MAX_FOLLOWED_LINKS_PER_PASS.saturating_sub(state.links_left),
            ended_early: !exhausted,
            walked_dropped: state.walked_dropped,
        })
    }

    /// Marks every other pool transaction that spends an output one of `branch` spends as conflicting with the
    /// selection. Bounded by the inputs of the selected branch times the spenders of each; each transaction is marked
    /// at most once per pass.
    fn mark_conflicts(
        &self,
        branch: &HashMap<TransactionKey, Arc<Transaction>>,
        state: &mut SelectionState<'_>,
    ) -> Result<(), UnconfirmedPoolError> {
        for key in branch.keys() {
            let tx = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            for input_hash in &tx.input_hashes {
                if let Some(spenders) = self.txs_by_spent_output.get(input_hash) {
                    for spender in spenders {
                        if !branch.contains_key(spender) && !state.selected_txs.contains_key(spender) {
                            state.conflicting.insert(*spender);
                        }
                    }
                }
            }
        }
        Ok(())
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
            if conflicts {
                // Spends an input that a selected transaction already spends: it can never be mined alongside it.
                // Not counted towards any limit (its walk was paid once, when it was queued).
                state.conflict_drop();
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
                state.selected_txs.extend(candidate_transactions_to_select);
                // Selecting transactions lowers the weight of their descendants' unselected ancestry
                state.ancestor_weight_bounds.clear();
            } else if fits_weight {
                // Only the bytes do not fit
                state.byte_skip();
                state.walked_dropped.insert(tx_key);
            } else {
                state.walked_dropped.insert(tx_key);
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
        ancestor_weight_bounds: &mut HashMap<TransactionKey, u64>,
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
                // Every link followed is charged to the pass's link budget
                if *links_left == 0 {
                    return Ok(WalkOutcome::OutOfFrames);
                }
                *links_left = links_left.saturating_sub(1);
                frame.next_dependency = frame.next_dependency.saturating_add(1);
                match self.txs_by_output.get(dependent_output) {
                    Some(keys) => {
                        let dependency = self.find_highest_priority_transaction(keys)?;
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
    /// is charged to `links_left`; `None` if that runs out.
    fn ancestor_weight_bound(
        &self,
        transaction: &PrioritizedTransaction,
        selected_txs: &HashMap<TransactionKey, Arc<Transaction>>,
        ancestor_weight_bounds: &HashMap<TransactionKey, u64>,
        links_left: &mut usize,
    ) -> Result<Option<u64>, UnconfirmedPoolError> {
        let mut dependencies = 0u64;
        for dependent_output in &transaction.dependent_output_hashes {
            if *links_left == 0 {
                return Ok(None);
            }
            *links_left = links_left.saturating_sub(1);
            if let Some(keys) = self.txs_by_output.get(dependent_output) {
                let dependency = self.find_highest_priority_transaction(keys)?;
                if !selected_txs.contains_key(&dependency.key) &&
                    let Some(bound) = ancestor_weight_bounds.get(&dependency.key)
                {
                    dependencies = dependencies.max(*bound);
                }
            }
        }
        Ok(Some(transaction.weight.saturating_add(dependencies)))
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
        self.tx_by_priority.clear();
        self.txs_by_output.clear();
        self.txs_by_spent_output.clear();
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

        let mut to_remove;
        let mut removed_transactions;
        {
            // Remove all transactions that contain the kernels found in this block
            let timer = Instant::now();
            to_remove = published_block
                .body
                .kernels()
                .iter()
                .map(|kernel| kernel.excess_sig.get_signature())
                .filter_map(|sig| self.txs_by_signature.get(sig))
                .flatten()
                .copied()
                .collect::<Vec<_>>();

            removed_transactions = to_remove
                .iter()
                .filter_map(|key| match self.remove_transaction(*key) {
                    Err(e) => Some(Err(e)),
                    Ok(Some(v)) => Some(Ok(v)),
                    Ok(None) => None,
                })
                .collect::<Result<Vec<_>, _>>()?;
            debug!(
                target: LOG_TARGET,
                "Found {} transactions with matching kernel sigs from unconfirmed pool in {:.2?}",
                to_remove.len(),
                timer.elapsed()
            );
        }
        // Reuse the buffer, clear is very cheap
        to_remove.clear();

        {
            // Remove all transactions that contain the inputs found in this block
            let timer = Instant::now();
            let published_block_hash_set = published_block
                .body
                .inputs()
                .iter()
                .map(|i| i.output_hash())
                .collect::<HashSet<_>>();

            to_remove.extend(
                self.tx_by_key
                    .iter()
                    .filter(|(_, tx)| UnconfirmedPool::find_matching_block_input(tx, &published_block_hash_set))
                    .map(|(key, _)| *key),
            );

            removed_transactions.extend(
                to_remove
                    .iter()
                    .filter_map(|key| match self.remove_transaction(*key) {
                        Err(e) => Some(Err(e)),
                        Ok(Some(v)) => Some(Ok(v)),
                        Ok(None) => None,
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            debug!(
                target: LOG_TARGET,
                "Found {} transactions with matching inputs from unconfirmed pool in {:.2?}",
                to_remove.len(),
                timer.elapsed()
            );
        }

        to_remove.clear();

        {
            // Remove all transactions that contain the outputs found in this block
            let timer = Instant::now();
            to_remove.extend(
                published_block
                    .body
                    .outputs()
                    .iter()
                    .filter_map(|output| self.txs_by_output.get(&output.hash()))
                    .flatten()
                    .copied(),
            );

            removed_transactions.extend(
                to_remove
                    .iter()
                    .filter_map(|key| match self.remove_transaction(*key) {
                        Err(e) => Some(Err(e)),
                        Ok(Some(v)) => Some(Ok(v)),
                        Ok(None) => None,
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            debug!(
                target: LOG_TARGET,
                "Found {} transactions with matching outputs from unconfirmed pool in {:.2?}",
                to_remove.len(),
                timer.elapsed()
            );
        }

        Ok(removed_transactions)
    }

    /// Searches a block and transaction for matching inputs
    fn find_matching_block_input(transaction: &PrioritizedTransaction, published_block: &HashSet<FixedHash>) -> bool {
        transaction
            .transaction
            .body
            .inputs()
            .iter()
            .any(|input| published_block.contains(&input.output_hash()))
    }

    /// Ensures that all transactions are safely deleted in order and from all storage
    pub(crate) fn remove_transaction(
        &mut self,
        tx_key: TransactionKey,
    ) -> Result<Option<Arc<Transaction>>, UnconfirmedPoolError> {
        let prioritized_transaction = match self.tx_by_key.remove(&tx_key) {
            Some(tx) => tx,
            None => return Ok(None),
        };

        self.tx_by_priority.remove(&prioritized_transaction.priority);

        for kernel in prioritized_transaction.transaction.body.kernels() {
            let sig = kernel.excess_sig.get_signature();
            if let Some(keys) = self.txs_by_signature.get_mut(sig) {
                let pos = keys
                    .iter()
                    .position(|k| *k == tx_key)
                    .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                keys.remove(pos);
                if keys.is_empty() {
                    self.txs_by_signature.remove(sig);
                }
            }
        }

        for output in prioritized_transaction.transaction.body.outputs() {
            let output_hash = output.hash();
            if let Some(keys) = self.txs_by_output.get_mut(&output_hash) {
                if let Some(pos) = keys.iter().position(|k| *k == tx_key) {
                    keys.remove(pos);
                }
                if keys.is_empty() {
                    self.txs_by_output.remove(&output_hash);
                }
            }
        }

        for input_hash in &prioritized_transaction.input_hashes {
            if let Some(keys) = self.txs_by_spent_output.get_mut(input_hash) {
                if let Some(pos) = keys.iter().position(|k| *k == tx_key) {
                    keys.remove(pos);
                }
                if keys.is_empty() {
                    self.txs_by_spent_output.remove(input_hash);
                }
            }
        }

        trace!(
            target: LOG_TARGET,
            "Deleted transaction: {}",
            prioritized_transaction.transaction
        );
        Ok(Some(prioritized_transaction.transaction))
    }

    /// Returns the total number of unconfirmed transactions stored in the UnconfirmedPool.
    pub fn len(&self) -> usize {
        self.txs_by_signature.len()
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
    fn check_data_consistency(&self) -> bool {
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
            self.tx_by_key.iter().all(|(key, tx)| {
                tx.input_hashes.iter().all(|hash| {
                    self.txs_by_spent_output
                        .get(hash)
                        .is_some_and(|keys| keys.contains(key))
                })
            }) &&
            self.txs_by_unique_id
                .values()
                .all(|tx_keys| tx_keys.iter().all(|tx_key| self.tx_by_key.contains_key(tx_key)))
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
        shrink_hashmap(&mut self.txs_by_output);
        shrink_hashmap(&mut self.txs_by_spent_output);
        shrink_hashmap(&mut self.txs_by_unique_id);

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
        fee::Fee,
        helpers::borsh::SerializedSize,
        key_manager::KeyManager,
        test_helpers::{TestParams, UtxoTestParams, add_outputs_with_reserved_sender_offset_keys},
        transaction_builder::TransactionBuilder,
        tx,
        weight::TransactionWeight,
    };
    use tari_utilities::ByteArray;

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
        let input = template.body.inputs().first().unwrap().clone();
        let input_size = input.get_serialized_size().unwrap();
        let num_inputs = (normal_bytes + 4 * 1024) / input_size + 1;
        let large = (0..21)
            .map(|_| {
                let kernels = tx!(MicroMinotari(500_000), fee: MicroMinotari(500), inputs: 1, outputs: 1, &key_manager)
                    .expect("Failed to get tx")
                    .0
                    .body
                    .kernels()
                    .clone();
                Arc::new(Transaction::new(
                    vec![input.clone(); num_inputs],
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
            .fetch_selection(u64::MAX, budget, max_weight())
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
            .fetch_selection(desired_weight, MAX_BLOCK_TEMPLATE_BODY_BYTES, max_weight())
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
    /// `n` (so that its hash is unique), and `base`'s first kernel with a fresh excess signature scalar and the given
    /// fee. Enough for the pool's selection logic, which does not validate.
    fn synthetic_tx(base: &Transaction, n: u64, fee: u64) -> Arc<Transaction> {
        use tari_common_types::types::CompressedSignature;
        use tari_crypto::keys::SecretKey;
        let mut output = base.body.outputs().first().unwrap().clone();
        output.features.maturity = n;
        let mut kernel = base.body.kernels().first().unwrap().clone();
        kernel.fee = MicroMinotari(fee);
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
                .select_txs_with(total_weight, usize::MAX / 2, ranking, max_weight(), HashSet::new())
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
        assert!(by_weight.frame_budget_exhausted);
        assert!(by_weight.ended_early);
        assert!(by_weight.byte_bound);
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
        let input = base.body.inputs().first().unwrap().clone();
        let input_size = input.get_serialized_size().unwrap();
        // A parent of ~5 MB (thousands of inputs), at minimal fee, that fits by weight and bytes with one tiny child
        let budget = MAX_BLOCK_TEMPLATE_BODY_BYTES;
        let template = synthetic_tx(&base, 0, 1);
        let child_size = body_size(&synthetic_tx(&base, 1, 1));
        let num_inputs = (budget - body_size(&template) - child_size - 100) / input_size;
        let parent = Arc::new(Transaction::new(
            vec![input.clone(); num_inputs],
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
            // Not empty: the parent with (at least) one child, or the honest transactions
            assert!(!selected.is_empty(), "{ranking:?}");
            assert!(
                (selected.contains(&parent) && children.iter().any(|child| selected.contains(child))) ||
                    normal.iter().all(|tx| selected.contains(tx)),
                "{ranking:?}"
            );
            assert!(!selection.frame_budget_exhausted, "{ranking:?}");
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
        assert!(by_weight.frame_budget_exhausted);
        assert_eq!(by_weight.out_of_frames_candidate, Some(target_key));
        assert!(!by_weight.results.retrieved_transactions.contains(&target));
        // It is not excluded from the second pass ...
        assert!(!by_weight.walked_dropped.contains(&target_key));
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
                by_weight.walked_dropped.clone(),
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
        // A candidate with 9,000 links (e.g. one per input) to the same parent: one transaction, but 9,000 links
        const LINKS: usize = 9_000;
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
        // The links are charged, by the pre-walk bound and by the walk, while only two transactions are visited
        assert!(selection.links_followed >= 2 * LINKS, "{}", selection.links_followed);
        assert!(selection.links_followed <= MAX_FOLLOWED_LINKS_PER_PASS);
        assert!(selection.walk_work <= 3);
    }

    #[tokio::test]
    async fn running_out_of_link_budget_ends_the_pass_but_drains_the_queue() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_weight = TransactionWeight::latest();
        let base = normal_txs(&key_manager, 1, 5).pop().unwrap();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 1_000,
            weight_tx_skip_count: 20,
            min_fee: 0,
        })
        .with_max_body_bytes(usize::MAX / 2);
        let parent = insert_chain(&mut unconfirmed_pool, &base, 0, 1, 1).pop().unwrap();
        let parent_output = parent.body.outputs().first().unwrap().hash();
        // Candidates each with 9,000 links to the parent: 18,000 links each (the pre-walk bound and the walk), so the
        // link budget runs out after ~111 of them. Decreasing fees fix the order; each has a high own rate but a low
        // branch rate, so the branches are queued rather than selected as they are found.
        const LINKS: usize = 9_000;
        let per_candidate = 2 * LINKS;
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
            .fetch_selection(u64::MAX, budget, max_weight())
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
}
