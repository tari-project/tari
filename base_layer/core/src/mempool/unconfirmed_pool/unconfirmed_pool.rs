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
    collections::{BTreeMap, HashMap, HashSet},
    hash::Hash,
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
        unconfirmed_pool::UnconfirmedPoolError,
        FeePerGramStat,
    },
    transactions::{tari_amount::MicroTari, transaction_components::Transaction, weight::TransactionWeight},
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
}

impl Default for UnconfirmedPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: 40_000,
            weight_tx_skip_count: 20,
        }
    }
}

/// The Unconfirmed Transaction Pool consists of all unconfirmed transactions that are ready to be included in a block
/// and they are prioritised according to the priority metric.
/// The txs_by_signature HashMap is used to find a transaction using its excess_sig, this functionality is used to match
/// transactions included in blocks with transactions stored in the pool. The txs_by_priority BTreeMap prioritise the
/// transactions in the pool according to TXPriority, it allows transactions to be inserted in sorted order by their
/// priority. The txs_by_priority BTreeMap makes it easier to select the set of highest priority transactions that can
/// be included in a block. The excess_sig of a transaction is used a key to uniquely identify a specific transaction in
/// these containers.
pub struct UnconfirmedPool {
    config: UnconfirmedPoolConfig,
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
    ) {
        if tx
            .body
            .kernels()
            .iter()
            .all(|k| self.txs_by_signature.contains_key(k.excess_sig.get_signature()))
        {
            return;
        }

        let new_key = self.get_next_key();
        let prioritized_tx = PrioritizedTransaction::new(new_key, transaction_weighting, tx, dependent_outputs);
        if self.tx_by_key.len() >= self.config.storage_capacity {
            if prioritized_tx.priority < *self.lowest_priority() {
                return;
            }
            self.remove_lowest_priority_tx();
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
    }

    /// TThis will search the unconfirmed pool for the set of outputs and return true if all of them are found
    pub fn contains_all_outputs(&mut self, outputs: &[HashOutput]) -> bool {
        outputs.iter().all(|hash| self.txs_by_output.contains_key(hash))
    }

    /// Insert a set of new transactions into the UnconfirmedPool
    #[cfg(test)]
    pub fn insert_many<I: IntoIterator<Item = Arc<Transaction>>>(
        &mut self,
        txs: I,
        transaction_weighting: &TransactionWeight,
    ) {
        for tx in txs {
            self.insert(tx, None, transaction_weighting);
        }
    }

    /// Check if a transaction is available in the UnconfirmedPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig.get_signature())
    }

    /// Returns a set of the highest priority unconfirmed transactions, that can be included in a block
    pub fn fetch_highest_priority_txs(&mut self, total_weight: u64) -> Result<RetrieveResults, UnconfirmedPoolError> {
        let mut selected_txs = HashMap::new();
        let mut curr_weight = 0;
        let mut curr_skip_count = 0;
        let mut transactions_to_remove_and_recheck = Vec::new();
        let mut potential_transactions_to_remove_and_recheck = Vec::new();
        let mut unique_ids = HashSet::new();
        for (_, tx_key) in self.tx_by_priority.iter().rev() {
            if selected_txs.contains_key(tx_key) {
                continue;
            }

            let prioritized_transaction = self
                .tx_by_key
                .get(tx_key)
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;

            let mut total_transaction_weight = 0;
            let mut candidate_transactions_to_select = HashMap::new();
            self.get_all_dependent_transactions(
                prioritized_transaction,
                &mut candidate_transactions_to_select,
                &mut potential_transactions_to_remove_and_recheck,
                &selected_txs,
                &mut total_transaction_weight,
                &mut unique_ids,
            )?;
            let total_weight_after_candidates = curr_weight + total_transaction_weight;
            if total_weight_after_candidates <= total_weight && potential_transactions_to_remove_and_recheck.is_empty()
            {
                if !UnconfirmedPool::find_duplicate_input(&selected_txs, &candidate_transactions_to_select) {
                    curr_weight += total_transaction_weight;
                    selected_txs.extend(candidate_transactions_to_select);
                }
            } else {
                transactions_to_remove_and_recheck.append(&mut potential_transactions_to_remove_and_recheck);
                // Check if some the next few txs with slightly lower priority wont fit in the remaining space.
                curr_skip_count += 1;
                if curr_skip_count >= self.config.weight_tx_skip_count {
                    break;
                }
            }
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
            self.remove_transaction(*tx_key);
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

    pub fn retrieve_by_excess_sigs(&self, excess_sigs: &[PrivateKey]) -> (Vec<Arc<Transaction>>, Vec<PrivateKey>) {
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
                    .expect("mempool indexes out of sync: transaction exists in txs_by_signature but not in tx_by_key")
            })
            .collect();

        (found, remaining)
    }

    fn get_all_dependent_transactions(
        &self,
        transaction: &PrioritizedTransaction,
        required_transactions: &mut HashMap<TransactionKey, Arc<Transaction>>,
        transactions_to_recheck: &mut Vec<(TransactionKey, Arc<Transaction>)>,
        selected_txs: &HashMap<TransactionKey, Arc<Transaction>>,
        total_weight: &mut u64,
        unique_ids: &mut HashSet<[u8; 32]>,
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
                            unique_ids,
                        )?;

                        if !transactions_to_recheck.is_empty() {
                            transactions_to_recheck.push((transaction.key, transaction.transaction.clone()));
                            break;
                        }
                    }
                },
                None => {
                    // this transactions requires an output, that the mempool does not currently have, but did have at
                    // some point. This means that we need to remove this transaction and re
                    // validate it
                    transactions_to_recheck.push((transaction.key, transaction.transaction.clone()));
                    break;
                },
            }
        }

        if required_transactions
            .insert(transaction.key, transaction.transaction.clone())
            .is_none()
        {
            *total_weight += transaction.weight;
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
        for (_, transaction) in current_transactions.iter() {
            for input in transaction.body.inputs() {
                if insert_set.contains(&input.output_hash()) {
                    return true;
                }
            }
        }
        false
    }

    fn lowest_priority(&self) -> &FeePriority {
        self.tx_by_priority
            .keys()
            .next()
            .expect("lowest_priority called on empty mempool")
    }

    fn remove_lowest_priority_tx(&mut self) {
        if let Some(tx_key) = self.tx_by_priority.values().next().copied() {
            self.remove_transaction(tx_key);
        }
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
    ) -> Vec<Arc<Transaction>> {
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
                .filter_map(|key| self.remove_transaction(*key))
                .collect::<Vec<_>>();
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

            removed_transactions.extend(to_remove.iter().filter_map(|key| self.remove_transaction(*key)));
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

            removed_transactions.extend(to_remove.iter().filter_map(|key| self.remove_transaction(*key)));
            debug!(
                target: LOG_TARGET,
                "Found {} transactions with matching outputs from unconfirmed pool in {:.2?}",
                to_remove.len(),
                timer.elapsed()
            );
        }

        removed_transactions
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
    fn remove_transaction(&mut self, tx_key: TransactionKey) -> Option<Arc<Transaction>> {
        let prioritized_transaction = self.tx_by_key.remove(&tx_key)?;

        self.tx_by_priority.remove(&prioritized_transaction.priority);

        for kernel in prioritized_transaction.transaction.body.kernels() {
            let sig = kernel.excess_sig.get_signature();
            if let Some(keys) = self.txs_by_signature.get_mut(sig) {
                let pos = keys.iter().position(|k| *k == tx_key).expect("mempool out of sync");
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
        Some(prioritized_transaction.transaction)
    }

    /// Remove all unconfirmed transactions that have become time locked. This can happen when the chain height was
    /// reduced on some reorgs.
    pub fn remove_timelocked(&mut self, tip_height: u64) {
        debug!(target: LOG_TARGET, "Removing time-locked inputs from unconfirmed pool");
        let to_remove = self
            .tx_by_key
            .iter()
            .filter(|(_, ptx)| ptx.transaction.min_spendable_height() > tip_height + 1)
            .map(|(k, _)| *k)
            .collect::<Vec<_>>();
        for tx_key in to_remove {
            self.remove_transaction(tx_key);
        }
    }

    /// Returns the total number of unconfirmed transactions stored in the UnconfirmedPool.
    pub fn len(&self) -> usize {
        self.txs_by_signature.len()
    }

    /// Returns all transaction stored in the UnconfirmedPool.
    pub fn snapshot(&self) -> Vec<Arc<Transaction>> {
        self.tx_by_key.iter().map(|(_, ptx)| ptx.transaction.clone()).collect()
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self, transaction_weight: &TransactionWeight) -> u64 {
        self.tx_by_key.values().fold(0, |weight, ptx| {
            weight + ptx.transaction.calculate_weight(transaction_weight)
        })
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
            let mut total_weight = 0;
            let mut total_fees = MicroTari::zero();
            let mut min_fee_per_gram = MicroTari::from(u64::MAX);
            let mut max_fee_per_gram = MicroTari::zero();
            for key in self.tx_by_priority.values().rev().skip(offset) {
                let tx = self.tx_by_key.get(key).ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                let weight = tx.weight;

                if total_weight + weight > target_block_weight {
                    break;
                }

                let total_tx_fee = tx.transaction.body.get_total_fee();
                offset += 1;
                let fee_per_gram = total_tx_fee / weight;
                min_fee_per_gram = min_fee_per_gram.min(fee_per_gram);
                max_fee_per_gram = max_fee_per_gram.max(fee_per_gram);
                total_fees += total_tx_fee;
                total_weight += weight;
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
        fn shrink_hashmap<K: Eq + Hash, V>(map: &mut HashMap<K, V>) -> (usize, usize) {
            let cap = map.capacity();
            let extra_cap = cap - map.len();
            if extra_cap > 100 {
                map.shrink_to(map.len() + (extra_cap / 2));
            }

            (cap, map.capacity())
        }

        let (old, new) = shrink_hashmap(&mut self.tx_by_key);
        shrink_hashmap(&mut self.txs_by_signature);
        shrink_hashmap(&mut self.txs_by_output);
        shrink_hashmap(&mut self.txs_by_unique_id);

        if old - new > 0 {
            debug!(
                target: LOG_TARGET,
                "Shrunk reorg mempool memory usage ({}/{}) ~{}%",
                new,
                old,
                (((old - new) as f32 / old as f32) * 100.0).round() as usize
            );
        }
    }
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;

    use super::*;
    use crate::{
        consensus::ConsensusManagerBuilder,
        test_helpers::{create_consensus_constants, create_consensus_rules, create_orphan_block},
        transactions::{
            fee::Fee,
            tari_amount::MicroTari,
            test_helpers::{TestParams, UtxoTestParams},
            weight::TransactionWeight,
            CryptoFactories,
            SenderTransactionProtocol,
        },
        tx,
    };

    #[test]
    fn test_find_duplicate_input() {
        let tx1 = Arc::new(tx!(MicroTari(5000), fee: MicroTari(50), inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(5000), fee: MicroTari(50), inputs: 2, outputs: 1).0);
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

    #[test]
    fn test_insert_and_retrieve_highest_priority_txs() {
        let tx1 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(5), inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(4), inputs: 4, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(20), inputs: 5, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(6), inputs: 3, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(11), inputs: 5, outputs: 1).0);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 4,
            weight_tx_skip_count: 3,
        });

        let tx_weight = TransactionWeight::latest();
        unconfirmed_pool.insert_many(
            [tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()],
            &tx_weight,
        );
        // Check that lowest priority tx was removed to make room for new incoming transactions
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig));
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig));
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig));
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig));
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig));
        // Retrieve the set of highest priority unspent transactions
        let desired_weight =
            tx1.calculate_weight(&tx_weight) + tx3.calculate_weight(&tx_weight) + tx5.calculate_weight(&tx_weight);
        let results = unconfirmed_pool.fetch_highest_priority_txs(desired_weight).unwrap();
        assert_eq!(results.retrieved_transactions.len(), 3);
        assert!(results.retrieved_transactions.contains(&tx1));
        assert!(results.retrieved_transactions.contains(&tx3));
        assert!(results.retrieved_transactions.contains(&tx5));
        // Note that transaction tx5 could not be included as its weight was to big to fit into the remaining allocated
        // space, the second best transaction was then included

        assert!(unconfirmed_pool.check_data_consistency());
    }

    #[test]
    fn test_double_spend_inputs() {
        let (tx1, _, _) = tx!(MicroTari(5_000), fee: MicroTari(10), inputs: 1, outputs: 1);
        const INPUT_AMOUNT: MicroTari = MicroTari(5_000);
        let (tx2, inputs, _) = tx!(INPUT_AMOUNT, fee: MicroTari(5), inputs: 1, outputs: 1);

        let test_params = TestParams::new();

        let mut stx_builder = SenderTransactionProtocol::builder(0, create_consensus_constants(0));
        stx_builder
            .with_lock_height(0)
            .with_fee_per_gram(5.into())
            .with_offset(Default::default())
            .with_private_nonce(test_params.nonce.clone())
            .with_change_secret(test_params.change_spend_key.clone());

        // Double spend the input from tx2 in tx3
        let double_spend_utxo = tx2.body.inputs().first().unwrap().clone();
        let double_spend_input = inputs.first().unwrap().clone();

        let estimated_fee = Fee::new(TransactionWeight::latest()).calculate(
            5.into(),
            1,
            1,
            1,
            test_params.get_size_for_default_metadata(1),
        );

        let utxo = test_params.create_unblinded_output(UtxoTestParams {
            value: INPUT_AMOUNT - estimated_fee,
            ..Default::default()
        });
        stx_builder
            .with_input(double_spend_utxo, double_spend_input)
            .with_output(utxo, test_params.sender_offset_private_key)
            .unwrap();

        let factories = CryptoFactories::default();
        let mut stx_protocol = stx_builder.build(&factories, None, u64::MAX).unwrap();
        stx_protocol.finalize(&factories, None, u64::MAX).unwrap();

        let tx3 = stx_protocol.get_transaction().unwrap().clone();

        let tx1 = Arc::new(tx1);
        let tx2 = Arc::new(tx2);
        let tx3 = Arc::new(tx3);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 4,
            weight_tx_skip_count: 3,
        });

        let tx_weight = TransactionWeight::latest();
        unconfirmed_pool.insert_many(vec![tx1.clone(), tx2.clone(), tx3.clone()], &tx_weight);
        assert_eq!(unconfirmed_pool.len(), 3);

        let desired_weight = tx1.calculate_weight(&tx_weight) +
            tx2.calculate_weight(&tx_weight) +
            tx3.calculate_weight(&tx_weight) +
            1000;
        let results = unconfirmed_pool.fetch_highest_priority_txs(desired_weight).unwrap();
        assert!(results.retrieved_transactions.contains(&tx1));
        // Whether tx2 or tx3 is selected is non-deterministic
        assert!(results.retrieved_transactions.contains(&tx2) ^ results.retrieved_transactions.contains(&tx3));
        assert_eq!(results.retrieved_transactions.len(), 2);
    }

    #[test]
    fn test_remove_reorg_txs() {
        let network = Network::LocalNet;
        let consensus = ConsensusManagerBuilder::new(network).build();
        let tx1 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), inputs:2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(20), inputs:3, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(100), inputs:2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(30), inputs:4, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), inputs:3, outputs: 1).0);
        let tx6 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(75), inputs:2, outputs: 1).0);

        let tx_weight = TransactionWeight::latest();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 3,
        });
        unconfirmed_pool.insert_many(
            vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()],
            &tx_weight,
        );
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

    #[test]
    fn test_discard_double_spend_txs() {
        let consensus = create_consensus_rules();
        let tx1 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(5), inputs:2, outputs:1).0);
        let tx2 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(4), inputs:3, outputs:1).0);
        let tx3 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(5), inputs:2, outputs:1).0);
        let tx4 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(6), inputs:2, outputs:1).0);
        let mut tx5 = tx!(MicroTari(5_000), fee:MicroTari(5), inputs:3, outputs:1).0;
        let mut tx6 = tx!(MicroTari(5_000), fee:MicroTari(13), inputs: 2, outputs: 1).0;
        // tx1 and tx5 have a shared input. Also, tx3 and tx6 have a shared input
        tx5.body.inputs_mut()[0] = tx1.body.inputs()[0].clone();
        tx6.body.inputs_mut()[1] = tx3.body.inputs()[1].clone();
        let tx5 = Arc::new(tx5);
        let tx6 = Arc::new(tx6);

        let tx_weight = TransactionWeight::latest();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 3,
        });
        unconfirmed_pool.insert_many(
            vec![
                tx1.clone(),
                tx2.clone(),
                tx3.clone(),
                tx4.clone(),
                tx5.clone(),
                tx6.clone(),
            ],
            &tx_weight,
        );

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

    #[test]
    fn test_multiple_transactions_with_same_outputs_in_mempool() {
        let (tx1, _, _) = tx!(MicroTari(150_000), fee: MicroTari(50), inputs:5, outputs:5);
        let (tx2, _, _) = tx!(MicroTari(250_000), fee: MicroTari(50), inputs:5, outputs:5);

        // Create transactions with duplicate kernels (will not pass internal validation, but that is ok)
        let mut tx3 = tx1.clone();
        let mut tx4 = tx2.clone();
        let (tx5, _, _) = tx!(MicroTari(350_000), fee: MicroTari(50), inputs:5, outputs:5);
        let (tx6, _, _) = tx!(MicroTari(450_000), fee: MicroTari(50), inputs:5, outputs:5);
        tx3.body.set_kernel(tx5.body.kernels()[0].clone());
        tx4.body.set_kernel(tx6.body.kernels()[0].clone());

        // Insert multiple transactions with the same outputs into the mempool

        let tx_weight = TransactionWeight::latest();
        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 3,
        });
        let txns = vec![
            Arc::new(tx1.clone()),
            Arc::new(tx2.clone()),
            // Transactions with duplicate outputs
            Arc::new(tx3.clone()),
            Arc::new(tx4.clone()),
        ];
        unconfirmed_pool.insert_many(txns.clone(), &tx_weight);

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
        unconfirmed_pool.remove_transaction(k);
        let k = *unconfirmed_pool
            .txs_by_signature
            .get(tx4.first_kernel_excess_sig().unwrap().get_signature())
            .unwrap()
            .first()
            .unwrap();
        unconfirmed_pool.remove_transaction(k);

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

        #[test]
        fn it_compiles_correct_stats_for_single_block() {
            let (tx1, _, _) = tx!(MicroTari(150_000), fee: MicroTari(5), inputs:5, outputs:1);
            let (tx2, _, _) = tx!(MicroTari(250_000), fee: MicroTari(5), inputs:5, outputs:5);
            let (tx3, _, _) = tx!(MicroTari(350_000), fee: MicroTari(4), inputs:2, outputs:1);
            let (tx4, _, _) = tx!(MicroTari(450_000), fee: MicroTari(4), inputs:4, outputs:5);

            let tx_weight = TransactionWeight::latest();
            let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());

            let tx1 = Arc::new(tx1);
            let tx2 = Arc::new(tx2);
            let tx3 = Arc::new(tx3);
            let tx4 = Arc::new(tx4);
            unconfirmed_pool.insert_many(vec![tx1, tx2, tx3, tx4], &tx_weight);

            let stats = unconfirmed_pool.get_fee_per_gram_stats(1, 19500).unwrap();
            assert_eq!(stats[0].order, 0);
            assert_eq!(stats[0].min_fee_per_gram, 4.into());
            assert_eq!(stats[0].max_fee_per_gram, 5.into());
            assert_eq!(stats[0].avg_fee_per_gram, 4.into());
        }

        #[test]
        fn it_compiles_correct_stats_for_multiple_blocks() {
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

            let mut transactions = (0u64..50)
                .map(|i| {
                    let (tx, _, _) = tx!(MicroTari(150_000 + i), fee: MicroTari(10), inputs: 1, outputs: 1);
                    Arc::new(tx)
                })
                .collect::<Vec<_>>();
            let (tx1, _, _) = tx!(MicroTari(150_000), fee: MicroTari(5), inputs:1, outputs: 5);
            transactions.push(Arc::new(tx1));

            let tx_weight = TransactionWeight::latest();
            let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig::default());

            unconfirmed_pool.insert_many(transactions, &tx_weight);

            let stats = unconfirmed_pool.get_fee_per_gram_stats(2, 2000).unwrap();
            assert_eq!(stats, expected_stats);
        }
    }
}
