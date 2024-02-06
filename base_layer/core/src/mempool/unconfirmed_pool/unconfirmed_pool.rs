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
use tari_common_types::types::{FixedHash, HashOutput, PrivateKey, Signature};
use tokio::time::Instant;

use crate::{
    blocks::Block,
    mempool::{
        priority::{FeePriority, PrioritizedTransaction},
        shrink_hashmap::shrink_hashmap,
        unconfirmed_pool::UnconfirmedPoolError,
        FeePerGramStat,
        MempoolError,
    },
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{Transaction, TransactionError},
        weight::TransactionWeight,
    },
};

pub const LOG_TARGET: &str = "c::mp::unconfirmed_pool::unconfirmed_pool_storage";

type TransactionKey = usize;

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
            min_fee: 0,
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
    txs_by_unique_id: HashMap<[u8; 32], Vec<TransactionKey>>,
}

// helper class to reduce type complexity
pub struct RetrieveResults {
    pub retrieved_transactions: Vec<Arc<Transaction>>,
    pub transactions_to_insert: Vec<Arc<Transaction>>,
}

pub type CompleteTransactionBranch = HashMap<TransactionKey, (HashMap<TransactionKey, Arc<Transaction>>, u64, u64)>;

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
            txs_by_unique_id: HashMap::new(),
        }
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
        for output in prioritized_tx.transaction.body.outputs() {
            self.txs_by_output.entry(output.hash()).or_default().push(new_key);
        }
        for kernel in prioritized_tx.transaction.body.kernels() {
            let sig = kernel.excess_sig.get_signature();
            self.txs_by_signature.entry(sig.clone()).or_default().push(new_key);
        }

        debug!(
            target: LOG_TARGET,
            "Inserted transaction {} into unconfirmed pool:", prioritized_tx
        );
        self.tx_by_key.insert(new_key, prioritized_tx);

        Ok(())
    }

    /// This will search the unconfirmed pool for the set of outputs and return true if all of them are found
    pub fn contains_all_outputs(&mut self, outputs: &[HashOutput]) -> bool {
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
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig.get_signature())
    }

    /// Returns a set of the highest priority unconfirmed transactions, that can be included in a block
    #[allow(clippy::too_many_lines)]
    pub fn fetch_highest_priority_txs(&mut self, total_weight: u64) -> Result<RetrieveResults, UnconfirmedPoolError> {
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

        let mut selected_txs = HashMap::new();
        let mut curr_weight = 0;
        let mut curr_skip_count = 0;
        let mut transactions_to_remove_and_recheck = Vec::new();
        let mut unique_ids = HashSet::new();
        let mut complete_transaction_branch = CompleteTransactionBranch::new();
        let mut potentional_to_add = BinaryHeap::<(u64, TransactionKey)>::new();
        // For each transaction we store transactions that depends on it. So when we process it, we can mark all of them
        // for recomputing.
        let mut depended_on: HashMap<TransactionKey, Vec<&TransactionKey>> = HashMap::new();
        let mut recompute = HashSet::new();
        for (_, tx_key) in self.tx_by_priority.iter().rev() {
            if selected_txs.contains_key(tx_key) {
                continue;
            }
            let prioritized_transaction = self
                .tx_by_key
                .get(tx_key)
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            self.check_the_potential_txs(
                total_weight,
                &mut selected_txs,
                &mut curr_weight,
                &mut curr_skip_count,
                &mut complete_transaction_branch,
                &mut potentional_to_add,
                &mut depended_on,
                &mut recompute,
                prioritized_transaction.fee_per_byte,
            )?;
            if curr_skip_count >= self.config.weight_tx_skip_count {
                break;
            }
            let mut total_transaction_weight = 0;
            let mut total_transaction_fees = 0;
            let mut candidate_transactions_to_select = HashMap::new();
            let mut potential_transactions_to_remove_and_recheck = Vec::new();
            self.get_all_dependent_transactions(
                prioritized_transaction,
                &mut candidate_transactions_to_select,
                &mut potential_transactions_to_remove_and_recheck,
                &selected_txs,
                &mut total_transaction_weight,
                &mut total_transaction_fees,
                &mut unique_ids,
            )?;
            let total_weight_after_candidates =
                curr_weight
                    .checked_add(total_transaction_weight)
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating transaction weights".to_string(),
                    ))?;
            if total_weight_after_candidates <= total_weight && potential_transactions_to_remove_and_recheck.is_empty()
            {
                for dependend_on_tx_key in candidate_transactions_to_select.keys() {
                    if dependend_on_tx_key != tx_key {
                        // Transaction is not depended on itself.
                        depended_on
                            .entry(*dependend_on_tx_key)
                            .and_modify(|v| v.push(tx_key))
                            .or_insert_with(|| vec![tx_key]);
                    }
                }
                let fee_per_byte = total_transaction_fees.saturating_mul(1000) / total_transaction_weight;
                complete_transaction_branch.insert(
                    *tx_key,
                    (
                        candidate_transactions_to_select.clone(),
                        total_transaction_weight,
                        total_transaction_fees,
                    ),
                );
                potentional_to_add.push((fee_per_byte, *tx_key));
            } else {
                transactions_to_remove_and_recheck.append(&mut potential_transactions_to_remove_and_recheck);
                // Check if some the next few txs with slightly lower priority wont fit in the remaining space.
                curr_skip_count += 1;
                if curr_skip_count >= self.config.weight_tx_skip_count {
                    break;
                }
            }
        }
        if curr_skip_count < self.config.weight_tx_skip_count {
            self.check_the_potential_txs(
                total_weight,
                &mut selected_txs,
                &mut curr_weight,
                &mut curr_skip_count,
                &mut complete_transaction_branch,
                &mut potentional_to_add,
                &mut depended_on,
                &mut recompute,
                0,
            )?;
        }
        if !transactions_to_remove_and_recheck.is_empty() {
            // we need to remove all transactions that need to be rechecked.
            debug!(
                target: LOG_TARGET,
                "Removing {} transaction(s) from unconfirmed pool because they need re-evaluation",
                transactions_to_remove_and_recheck.len()
            );
        }
        for (tx_key, _) in &transactions_to_remove_and_recheck {
            self.remove_transaction(*tx_key)?;
        }

        let results = RetrieveResults {
            retrieved_transactions: selected_txs.into_values().collect(),
            transactions_to_insert: transactions_to_remove_and_recheck
                .into_iter()
                .map(|(_, tx)| tx)
                .collect(),
        };
        Ok(results)
    }

    fn check_the_potential_txs<'a>(
        &self,
        total_weight: u64,
        selected_txs: &mut HashMap<TransactionKey, Arc<Transaction>>,
        curr_weight: &mut u64,
        curr_skip_count: &mut usize,
        complete_transaction_branch: &mut CompleteTransactionBranch,
        potentional_to_add: &mut BinaryHeap<(u64, TransactionKey)>,
        depended_on: &mut HashMap<TransactionKey, Vec<&'a TransactionKey>>,
        recompute: &mut HashSet<&'a TransactionKey>,
        fee_per_byte_threshold: u64,
    ) -> Result<(), UnconfirmedPoolError> {
        while match potentional_to_add.peek() {
            Some((fee_per_byte, _)) => *fee_per_byte >= fee_per_byte_threshold,
            None => false,
        } {
            // If the current TXs has lower fee than the ones we already processed, we can add some.
            let (_fee_per_byte, tx_key) = potentional_to_add.pop().ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            if selected_txs.contains_key(&tx_key) {
                continue;
            }
            // Before we do anything with the top transaction we need to know if needs to be recomputed.
            if recompute.contains(&tx_key) {
                recompute.remove(&tx_key);
                // So we recompute the total fees based on updated weights and fees.
                let (_, total_transaction_weight, total_transaction_fees) = complete_transaction_branch
                    .get(&tx_key)
                    .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                let fee_per_byte = total_transaction_fees.saturating_mul(1000) / *total_transaction_weight;
                potentional_to_add.push((fee_per_byte, tx_key));
                continue;
            }
            let (candidate_transactions_to_select, total_transaction_weight, _total_transaction_fees) =
                complete_transaction_branch
                    .remove(&tx_key)
                    .ok_or(UnconfirmedPoolError::StorageOutofSync)?;

            let total_weight_after_candidates =
                curr_weight
                    .checked_add(total_transaction_weight)
                    .ok_or(UnconfirmedPoolError::InternalError(
                        "Overflow when calculating total weights".to_string(),
                    ))?;
            if total_weight_after_candidates <= total_weight {
                if !UnconfirmedPool::find_duplicate_input(selected_txs, &candidate_transactions_to_select) {
                    *curr_weight = curr_weight.checked_add(total_transaction_weight).ok_or(
                        UnconfirmedPoolError::InternalError("Overflow when calculating total weights".to_string()),
                    )?;
                    // So we processed the transaction, let's mark the dependents to be recomputed.
                    for tx_key in candidate_transactions_to_select.keys() {
                        self.remove_transaction_from_the_dependants(
                            *tx_key,
                            complete_transaction_branch,
                            depended_on,
                            recompute,
                        )?;
                    }
                    selected_txs.extend(candidate_transactions_to_select);
                }
            } else {
                *curr_skip_count += 1;
                if *curr_skip_count >= self.config.weight_tx_skip_count {
                    break;
                }
            }
            // Some cleanup of what we don't need anymore
            complete_transaction_branch.remove(&tx_key);
            depended_on.remove(&tx_key);
        }
        Ok(())
    }

    fn remove_transaction_from_the_dependants<'a>(
        &self,
        tx_key: TransactionKey,
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
                    update_total_transaction_fees,
                )) = complete_transaction_branch.get_mut(tx)
                {
                    update_candidate_transactions_to_select.remove(&tx_key);
                    *update_total_transaction_weight = update_total_transaction_weight
                        .checked_sub(prioritized_transaction.weight)
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

    fn get_all_dependent_transactions(
        &self,
        transaction: &PrioritizedTransaction,
        required_transactions: &mut HashMap<TransactionKey, Arc<Transaction>>,
        transactions_to_recheck: &mut Vec<(TransactionKey, Arc<Transaction>)>,
        selected_txs: &HashMap<TransactionKey, Arc<Transaction>>,
        total_weight: &mut u64,
        total_fees: &mut u64,
        _unique_ids: &mut HashSet<[u8; 32]>,
    ) -> Result<(), UnconfirmedPoolError> {
        for dependent_output in &transaction.dependent_output_hashes {
            match self.txs_by_output.get(dependent_output) {
                Some(signatures) => {
                    let dependent_transaction = self.find_highest_priority_transaction(signatures)?;
                    if !selected_txs.contains_key(&dependent_transaction.key) {
                        self.get_all_dependent_transactions(
                            dependent_transaction,
                            required_transactions,
                            transactions_to_recheck,
                            selected_txs,
                            total_weight,
                            total_fees,
                            _unique_ids,
                        )?;

                        if !transactions_to_recheck.is_empty() {
                            transactions_to_recheck.push((transaction.key, transaction.transaction.clone()));
                            break;
                        }
                    }
                },
                None => {
                    // this transactions requires an output, that the mempool does not currently have, but did have at
                    // some point. This means that we need to remove this transaction and revalidate it
                    transactions_to_recheck.push((transaction.key, transaction.transaction.clone()));
                    break;
                },
            }
        }

        if required_transactions
            .insert(transaction.key, transaction.transaction.clone())
            .is_none()
        {
            *total_fees = total_fees
                .checked_add(transaction.transaction.body.get_total_fee()?.0)
                .ok_or(UnconfirmedPoolError::InternalError(
                    "Overflow when calculating total fees".to_string(),
                ))?;
            *total_weight = total_weight
                .checked_add(transaction.weight)
                .ok_or(UnconfirmedPoolError::InternalError(
                    "Overflow when calculating total weights".to_string(),
                ))?;
        }

        Ok(())
    }

    fn find_highest_priority_transaction(
        &self,
        keys: &[TransactionKey],
    ) -> Result<&PrioritizedTransaction, UnconfirmedPoolError> {
        if keys.is_empty() {
            return Err(UnconfirmedPoolError::StorageOutofSync);
        }

        let mut highest_transaction = self
            .tx_by_key
            .get(&keys[0])
            .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
        for key in keys.iter().skip(1) {
            let transaction = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            if transaction.priority > highest_transaction.priority {
                highest_transaction = transaction;
            }
        }
        Ok(highest_transaction)
    }

    // This will search a Vec<Arc<Transaction>> for duplicate inputs of a tx
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
    fn remove_transaction(&mut self, tx_key: TransactionKey) -> Result<Option<Arc<Transaction>>, UnconfirmedPoolError> {
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

        trace!(
            target: LOG_TARGET,
            "Deleted transaction: {}",
            &prioritized_transaction.transaction
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
                offset += 1;
                let fee_per_gram = total_tx_fee / weight;
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
                avg_fee_per_gram: total_fees / total_weight,
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
            self.txs_by_unique_id
                .values()
                .all(|tx_keys| tx_keys.iter().all(|tx_key| self.tx_by_key.contains_key(tx_key)))
    }

    fn get_next_key(&mut self) -> usize {
        let key = self.key_counter;
        self.key_counter = (self.key_counter + 1) % usize::MAX;
        key
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    pub fn compact(&mut self) {
        let (old, new) = shrink_hashmap(&mut self.tx_by_key);
        shrink_hashmap(&mut self.txs_by_signature);
        shrink_hashmap(&mut self.txs_by_output);
        shrink_hashmap(&mut self.txs_by_unique_id);

        if old > new {
            debug!(
                target: LOG_TARGET,
                "Shrunk reorg mempool memory usage ({}/{}) ~{}%",
                new,
                old,
                (old - new).saturating_mul(100) / old
            );
        }
    }
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_script::{ExecutionStack, TariScript};

    use super::*;
    use crate::{
        consensus::ConsensusManagerBuilder,
        covenants::Covenant,
        test_helpers::{create_consensus_constants, create_consensus_rules, create_orphan_block},
        transactions::{
            aggregated_body::AggregateBody,
            fee::Fee,
            key_manager::create_memory_db_key_manager,
            tari_amount::MicroMinotari,
            test_helpers::{TestParams, UtxoTestParams},
            weight::TransactionWeight,
            SenderTransactionProtocol,
        },
        tx,
    };

    #[tokio::test]
    async fn test_find_duplicate_input() {
        let key_manager = create_memory_db_key_manager();
        let tx1 = Arc::new(
            tx!(MicroMinotari(5000), fee: MicroMinotari(50), inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx2 = Arc::new(
            tx!(MicroMinotari(5000), fee: MicroMinotari(50), inputs: 2, outputs: 1, &key_manager)
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
        let key_manager = create_memory_db_key_manager();
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
        let results = unconfirmed_pool.fetch_highest_priority_txs(desired_weight).unwrap();
        assert_eq!(results.retrieved_transactions.len(), 3);
        assert!(results.retrieved_transactions.contains(&tx1));
        assert!(results.retrieved_transactions.contains(&tx3));
        assert!(results.retrieved_transactions.contains(&tx5));
        // Note that transaction tx5 could not be included as its weight was to big to fit into the remaining allocated
        // space, the second best transaction was then included

        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[tokio::test]
    async fn test_double_spend_inputs() {
        let key_manager = create_memory_db_key_manager();
        let (tx1, _, _) = tx!(MicroMinotari(5_000), fee: MicroMinotari(10), inputs: 1, outputs: 1, &key_manager)
            .expect("Failed to get tx");
        const INPUT_AMOUNT: MicroMinotari = MicroMinotari(5_000);
        let (tx2, inputs, _) =
            tx!(INPUT_AMOUNT, fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager).expect("Failed to get tx");

        let mut stx_builder = SenderTransactionProtocol::builder(create_consensus_constants(0), key_manager.clone());

        let change = TestParams::new(&key_manager).await;
        stx_builder
            .with_lock_height(0)
            .with_fee_per_gram(5.into())
            .with_change_data(
                TariScript::default(),
                ExecutionStack::default(),
                change.script_key_id.clone(),
                change.spend_key_id.clone(),
                Covenant::default(),
            );

        let test_params = TestParams::new(&key_manager).await;
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
            .await
            .unwrap();
        stx_builder
            .with_input(double_spend_input)
            .await
            .unwrap()
            .with_output(utxo, test_params.sender_offset_key_id)
            .await
            .unwrap();

        let mut stx_protocol = stx_builder.build().await.unwrap();
        stx_protocol.finalize(&key_manager).await.unwrap();

        let tx3 = stx_protocol.get_transaction().unwrap().clone();

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
        let results = unconfirmed_pool.fetch_highest_priority_txs(desired_weight).unwrap();
        assert!(results.retrieved_transactions.contains(&tx1));
        // Whether tx2 or tx3 is selected is non-deterministic
        assert!(results.retrieved_transactions.contains(&tx2) ^ results.retrieved_transactions.contains(&tx3));
        assert_eq!(results.retrieved_transactions.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_reorg_txs() {
        let key_manager = create_memory_db_key_manager();
        let network = Network::LocalNet;
        let consensus = ConsensusManagerBuilder::new(network).build().unwrap();
        let tx1 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(50), inputs:2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx2 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(20), inputs:3, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx3 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(100), inputs:2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx4 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(30), inputs:4, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx5 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(50), inputs:3, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx6 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(75), inputs:2, outputs: 1, &key_manager)
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
        let key_manager = create_memory_db_key_manager();
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
        tx5.body = AggregateBody::new(inputs, tx5.body().outputs().clone(), tx5.body().kernels().clone());
        let mut inputs = tx6.body.inputs().clone();
        inputs[0] = tx3.body.inputs()[1].clone();
        tx6.body = AggregateBody::new(inputs, tx6.body().outputs().clone(), tx6.body().kernels().clone());
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
        let key_manager = create_memory_db_key_manager();
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
                        found += 1;
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
            let key_manager = create_memory_db_key_manager();
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
            let key_manager = create_memory_db_key_manager();
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
