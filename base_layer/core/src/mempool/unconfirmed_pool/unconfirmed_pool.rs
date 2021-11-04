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
    blocks::Block,
    mempool::{
        consts::{MEMPOOL_UNCONFIRMED_POOL_STORAGE_CAPACITY, MEMPOOL_UNCONFIRMED_POOL_WEIGHT_TRANSACTION_SKIP_COUNT},
        priority::{FeePriority, PrioritizedTransaction},
        unconfirmed_pool::UnconfirmedPoolError,
    },
    transactions::{transaction::Transaction, weight::TransactionWeight},
};
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tari_common_types::types::{CompressedSignature, HashOutput, Signature};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

pub const LOG_TARGET: &str = "c::mp::unconfirmed_pool::unconfirmed_pool_storage";

/// Configuration for the UnconfirmedPool
#[derive(Clone, Copy, Serialize, Deserialize)]
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
            storage_capacity: MEMPOOL_UNCONFIRMED_POOL_STORAGE_CAPACITY,
            weight_tx_skip_count: MEMPOOL_UNCONFIRMED_POOL_WEIGHT_TRANSACTION_SKIP_COUNT,
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
    txs_by_signature: HashMap<Signature, PrioritizedTransaction>,
    txs_by_priority: BTreeMap<FeePriority, Signature>,
    txs_by_output: HashMap<HashOutput, Vec<Signature>>,
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
            txs_by_signature: HashMap::new(),
            txs_by_priority: BTreeMap::new(),
            txs_by_output: HashMap::new(),
        }
    }

    fn lowest_priority(&self) -> &FeePriority {
        self.txs_by_priority.iter().next().unwrap().0
    }

    fn remove_lowest_priority_tx(&mut self) {
        if let Some((priority, sig)) = self.txs_by_priority.iter().next().map(|(p, s)| (p.clone(), s.clone())) {
            self.txs_by_signature.remove(&sig);
            self.txs_by_priority.remove(&priority);
        }
    }

    /// Insert a new transaction into the UnconfirmedPool. Low priority transactions will be removed to make space for
    /// higher priority transactions. The lowest priority transactions will be removed when the maximum capacity is
    /// reached and the new transaction has a higher priority than the currently stored lowest priority transaction.
    pub fn insert(
        &mut self,
        tx: Arc<Transaction>,
        dependent_outputs: Option<Vec<HashOutput>>,
        transaction_weight: &TransactionWeight,
    ) -> Result<(), UnconfirmedPoolError> {
        let tx_key = tx
            .first_kernel_excess_sig()
            .ok_or(UnconfirmedPoolError::TransactionNoKernels)?;

        if self.txs_by_signature.contains_key(tx_key) {
            return Ok(());
        }

        let prioritized_tx = PrioritizedTransaction::try_construct(transaction_weight, tx.clone(), dependent_outputs)?;
        if self.txs_by_signature.len() >= self.config.storage_capacity {
            if prioritized_tx.priority < *self.lowest_priority() {
                return Ok(());
            }
            self.remove_lowest_priority_tx();
        }
        self.txs_by_priority
            .insert(prioritized_tx.priority.clone(), tx_key.clone());
        self.txs_by_signature.insert(tx_key.clone(), prioritized_tx);
        for output in tx.body.outputs().clone() {
            self.txs_by_output
                .entry(output.hash())
                .or_default()
                .push(tx_key.clone());
        }
        debug!(
            target: LOG_TARGET,
            "Inserted transaction with signature {} into unconfirmed pool:",
            tx_key.get_signature().to_hex()
        );

        trace!(target: LOG_TARGET, "insert: {}", tx);
        Ok(())
    }

    /// TThis will search the unconfirmed pool for the set of outputs and return true if all of them are found
    pub fn verify_outputs_exist(&mut self, outputs: &[HashOutput]) -> bool {
        for hash in outputs {
            if !self.txs_by_output.contains_key(hash) {
                return false;
            }
        }
        true
    }

    /// Insert a set of new transactions into the UnconfirmedPool
    #[cfg(test)]
    pub fn insert_many<I: IntoIterator<Item = Arc<Transaction>>>(
        &mut self,
        txs: I,
        transaction_weight: &TransactionWeight,
    ) -> Result<(), UnconfirmedPoolError> {
        for tx in txs.into_iter() {
            self.insert(tx, None, transaction_weight)?;
        }
        Ok(())
    }

    /// Check if a transaction is available in the UnconfirmedPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &CompressedSignature) -> bool {
        self.txs_by_signature.contains_key(excess_sig)
    }

    /// Returns a set of the highest priority unconfirmed transactions, that can be included in a block
    pub fn highest_priority_txs(&mut self, total_weight: u64) -> Result<RetrieveResults, UnconfirmedPoolError> {
        let mut selected_txs = HashMap::new();
        let mut curr_weight: u64 = 0;
        let mut curr_skip_count: usize = 0;
        let mut transactions_to_remove_and_recheck = Vec::new();
        for (_, tx_key) in self.txs_by_priority.iter().rev() {
            if selected_txs.contains_key(tx_key) {
                continue;
            }
            let prioritized_transaction = self
                .txs_by_signature
                .get(tx_key)
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;

            let mut total_transaction_weight = 0;
            let mut potential_transactions_to_insert = HashMap::new();
            let mut potential_transactions_to_remove_and_recheck = Vec::new();
            self.get_all_dependant_transactions(
                prioritized_transaction,
                &mut potential_transactions_to_insert,
                &mut potential_transactions_to_remove_and_recheck,
                &selected_txs,
                &mut total_transaction_weight,
            )?;
            if curr_weight + total_transaction_weight <= total_weight &&
                potential_transactions_to_remove_and_recheck.is_empty()
            {
                if !UnconfirmedPool::find_duplicate_input(&selected_txs, &potential_transactions_to_insert) {
                    curr_weight += total_transaction_weight;
                    for (key, transaction) in potential_transactions_to_insert {
                        selected_txs.insert((key).clone(), transaction.transaction.clone());
                    }
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
        // we need to remove all transactions that need to be rechecked.
        for transaction in &transactions_to_remove_and_recheck {
            let key = transaction
                .first_kernel_excess_sig()
                .ok_or(UnconfirmedPoolError::TransactionNoKernels)?;
            debug!(
                target: LOG_TARGET,
                "Removing transaction with key {} from unconfirmed pool because it needs re-evaluation",
                key.get_signature().to_hex()
            );
            self.delete_transaction(key);
        }
        let results = RetrieveResults {
            retrieved_transactions: selected_txs.into_values().collect(),
            transactions_to_insert: transactions_to_remove_and_recheck,
        };
        Ok(results)
    }

    fn get_all_dependant_transactions(
        &self,
        transaction: &PrioritizedTransaction,
        required_transactions: &mut HashMap<Signature, PrioritizedTransaction>,
        transactions_to_delete: &mut Vec<Arc<Transaction>>,
        already_selected_txs: &HashMap<Signature, Arc<Transaction>>,
        total_weight: &mut u64,
    ) -> Result<(), UnconfirmedPoolError> {
        for dependant_output in &transaction.depended_output_hashes {
            match self.txs_by_output.get(dependant_output) {
                Some(signatures) => {
                    let highest_signature = self.find_highest_priority_transaction(signatures)?;
                    if !already_selected_txs.contains_key(&highest_signature) {
                        let dependant_transaction = self
                            .txs_by_signature
                            .get(&highest_signature)
                            .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
                        self.get_all_dependant_transactions(
                            dependant_transaction,
                            required_transactions,
                            transactions_to_delete,
                            already_selected_txs,
                            total_weight,
                        )?;
                        if !transactions_to_delete.is_empty() {
                            transactions_to_delete.push(transaction.transaction.clone());
                            break;
                        }
                    }
                },
                None => {
                    // this transactions requires an output, that the mempool does not currently have, but did have at
                    // some point. This means that we need to remove this transaction and re
                    // validate it
                    transactions_to_delete.push(transaction.transaction.clone());
                    break;
                },
            }
        }
        let key = transaction
            .transaction
            .first_kernel_excess_sig()
            .ok_or(UnconfirmedPoolError::TransactionNoKernels)?;
        if required_transactions
            .insert(key.clone(), (*transaction).clone())
            .is_none()
        {
            *total_weight += transaction.weight;
        };
        Ok(())
    }

    fn find_highest_priority_transaction(&self, signatures: &[Signature]) -> Result<Signature, UnconfirmedPoolError> {
        let mut highest_signature = signatures[0].clone();
        for signature in signatures.iter().skip(1) {
            let transaction = self
                .txs_by_signature
                .get(signature)
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            let current_transaction = self
                .txs_by_signature
                .get(&highest_signature)
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;
            if transaction.priority > current_transaction.priority {
                highest_signature = signature.clone();
            }
        }
        Ok(highest_signature)
    }

    // This will search a Vec<Arc<Transaction>> for duplicate inputs of a tx
    fn find_duplicate_input(
        current_transactions: &HashMap<Signature, Arc<Transaction>>,
        transactions_to_insert: &HashMap<Signature, PrioritizedTransaction>,
    ) -> bool {
        for (_, transaction_to_insert) in transactions_to_insert.iter() {
            for (_, transaction) in current_transactions.iter() {
                for input in transaction.body.inputs() {
                    for tx_input in transaction_to_insert.transaction.body.inputs() {
                        if tx_input.output_hash() == input.output_hash() {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Remove all current mempool transactions from the UnconfirmedPoolStorage, returning that which have been removed
    pub fn drain_all_mempool_transactions(&mut self) -> Vec<Arc<Transaction>> {
        let mempool_txs: Vec<Arc<Transaction>> = self
            .txs_by_signature
            .drain()
            .map(|(_key, val)| val.transaction)
            .collect();
        self.txs_by_priority.clear();
        self.txs_by_output.clear();

        mempool_txs
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
            published_block.header.hash().to_hex(),
        );
        // We need to make sure that none of the transactions in the block remains in the mempool
        let mut transactions_to_remove = Vec::new();
        published_block.body.kernels().iter().for_each(|kernel| {
            transactions_to_remove.push(kernel.excess_sig.clone());
        });
        let mut removed_transactions = self.delete_transactions(&transactions_to_remove);

        // Remove all other deprecated transactions that cannot be valid anymore
        removed_transactions.append(&mut self.remove_deprecated_transactions(published_block));
        removed_transactions
    }

    // Remove all deprecated transactions from the UnconfirmedPool by scanning inputs and outputs.
    fn remove_deprecated_transactions(&mut self, published_block: &Block) -> Vec<Arc<Transaction>> {
        let mut transaction_keys_to_remove = Vec::new();
        for (tx_key, ptx) in self.txs_by_signature.iter() {
            if UnconfirmedPool::find_matching_block_input(ptx, published_block) {
                transaction_keys_to_remove.push(tx_key.clone())
            }
        }
        published_block.body.outputs().iter().for_each(|output| {
            if let Some(signatures) = self.txs_by_output.get(&output.hash()) {
                for signature in signatures {
                    transaction_keys_to_remove.push(signature.clone())
                }
            }
        });
        debug!(
            target: LOG_TARGET,
            "Removing transactions containing duplicated commitments from unconfirmed pool"
        );
        self.delete_transactions(&transaction_keys_to_remove)
    }

    // This is a helper function that searches a block and transaction for matching inputs
    fn find_matching_block_input(transaction: &PrioritizedTransaction, published_block: &Block) -> bool {
        for input in transaction.transaction.body.inputs() {
            for published_input in published_block.body.inputs() {
                if published_input.output_hash() == input.output_hash() {
                    return true;
                }
            }
        }
        false
    }

    fn delete_transactions(&mut self, signature: &[Signature]) -> Vec<Arc<Transaction>> {
        let mut removed_txs: Vec<Arc<Transaction>> = Vec::new();
        for tx_key in signature {
            debug!(
                target: LOG_TARGET,
                "Removing transaction with key {} from unconfirmed pool",
                tx_key.get_signature().to_hex()
            );
            if let Some(transaction) = self.delete_transaction(tx_key) {
                removed_txs.push(transaction);
            }
        }
        removed_txs
    }

    // Helper function to ensure that all transactions are safely deleted in order and from all storage
    fn delete_transaction(&mut self, signature: &Signature) -> Option<Arc<Transaction>> {
        if let Some(prioritized_transaction) = self.txs_by_signature.remove(signature) {
            self.txs_by_priority.remove(&prioritized_transaction.priority);
            for output in prioritized_transaction.transaction.as_ref().body.outputs() {
                let key = output.hash();
                if let Some(signatures) = self.txs_by_output.get_mut(&key) {
                    signatures.retain(|x| x != signature);
                    if signatures.is_empty() {
                        self.txs_by_output.remove(&key);
                    }
                }
            }
            trace!(
                target: LOG_TARGET,
                "Deleted transaction: {}",
                &prioritized_transaction.transaction
            );
            return Some(prioritized_transaction.transaction);
        }
        None
    }

    /// Remove all unconfirmed transactions that have become time locked. This can happen when the chain height was
    /// reduced on some reorgs.
    pub fn remove_timelocked(&mut self, tip_height: u64) -> Vec<Arc<Transaction>> {
        let mut removed_tx_keys: Vec<Signature> = Vec::new();
        for (tx_key, ptx) in self.txs_by_signature.iter() {
            if ptx.transaction.min_spendable_height() > tip_height + 1 {
                removed_tx_keys.push(tx_key.clone());
            }
        }
        debug!(target: LOG_TARGET, "Removing time-locked inputs from unconfirmed pool");
        self.delete_transactions(&removed_tx_keys)
    }

    /// Returns the total number of unconfirmed transactions stored in the UnconfirmedPool.
    pub fn len(&self) -> usize {
        self.txs_by_signature.len()
    }

    /// Returns all transaction stored in the UnconfirmedPool.
    pub fn snapshot(&self) -> Vec<Arc<Transaction>> {
        self.txs_by_signature
            .iter()
            .map(|(_, ptx)| ptx.transaction.clone())
            .collect()
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self, transaction_weight: &TransactionWeight) -> u64 {
        self.txs_by_signature.iter().fold(0, |weight, (_, ptx)| {
            weight + ptx.transaction.calculate_weight(transaction_weight)
        })
    }

    #[cfg(test)]
    /// Returns false if there are any inconsistencies in the internal mempool state, otherwise true
    fn check_status(&self) -> bool {
        if self.txs_by_priority.len() != self.txs_by_signature.len() {
            return false;
        }
        self.txs_by_priority
            .iter()
            .all(|(_, tx_key)| self.txs_by_signature.contains_key(tx_key))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        consensus::ConsensusManagerBuilder,
        test_helpers::{create_consensus_constants, create_consensus_rules, create_orphan_block},
        transactions::{
            fee::Fee,
            tari_amount::MicroTari,
            test_helpers::{TestParams, UtxoTestParams},
            transaction::KernelFeatures,
            weight::TransactionWeight,
            CryptoFactories,
            SenderTransactionProtocol,
        },
        tx,
    };
    use tari_common::configuration::Network;
    use tari_common_types::types::HashDigest;

    #[test]
    fn test_find_duplicate_input() {
        let tx1 = Arc::new(tx!(MicroTari(5000), fee: MicroTari(50), inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(5000), fee: MicroTari(50), inputs: 2, outputs: 1).0);
        let tx_weight = TransactionWeight::latest();
        let mut tx_pool = HashMap::new();
        let mut tx1_pool = HashMap::new();
        let mut tx2_pool = HashMap::new();
        tx_pool.insert(tx1.first_kernel_excess_sig().unwrap().clone(), tx1.clone());
        tx1_pool.insert(
            tx1.first_kernel_excess_sig().unwrap().clone(),
            PrioritizedTransaction::try_construct(&tx_weight, tx1.clone(), None).unwrap(),
        );
        tx2_pool.insert(
            tx2.first_kernel_excess_sig().unwrap().clone(),
            PrioritizedTransaction::try_construct(&tx_weight, tx2.clone(), None).unwrap(),
        );
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
        unconfirmed_pool
            .insert_many(
                [tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()],
                &tx_weight,
            )
            .unwrap();
        // Check that lowest priority tx was removed to make room for new incoming transactions
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig),);
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig),);
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig),);
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig),);
        // Retrieve the set of highest priority unspent transactions
        let desired_weight =
            tx1.calculate_weight(&tx_weight) + tx3.calculate_weight(&tx_weight) + tx5.calculate_weight(&tx_weight);
        let results = unconfirmed_pool.highest_priority_txs(desired_weight).unwrap();
        assert_eq!(results.retrieved_transactions.len(), 3);
        assert!(results.retrieved_transactions.contains(&tx1));
        assert!(results.retrieved_transactions.contains(&tx3));
        assert!(results.retrieved_transactions.contains(&tx5));
        // Note that transaction tx5 could not be included as its weight was to big to fit into the remaining allocated
        // space, the second best transaction was then included

        assert!(unconfirmed_pool.check_status());
    }

    #[test]
    fn test_double_spend_inputs() {
        let (tx1, _, _) = tx!(MicroTari(5_000), fee: MicroTari(50), inputs: 1, outputs: 1);
        const INPUT_AMOUNT: MicroTari = MicroTari(5_000);
        let (tx2, inputs, _) = tx!(INPUT_AMOUNT, fee: MicroTari(20), inputs: 1, outputs: 1);

        let test_params = TestParams::new();

        let mut stx_builder = SenderTransactionProtocol::builder(0, create_consensus_constants(0));
        stx_builder
            .with_lock_height(0)
            .with_fee_per_gram(20.into())
            .with_offset(Default::default())
            .with_private_nonce(test_params.nonce.clone())
            .with_change_secret(test_params.change_spend_key.clone());

        // Double spend the input from tx2 in tx3
        let double_spend_utxo = tx2.body.inputs().first().unwrap().clone();
        let double_spend_input = inputs.first().unwrap().clone();

        let estimated_fee = Fee::new(TransactionWeight::latest()).calculate(20.into(), 1, 1, 1, 0);

        let utxo = test_params.create_unblinded_output(UtxoTestParams {
            value: INPUT_AMOUNT - estimated_fee,
            ..Default::default()
        });
        stx_builder
            .with_input(double_spend_utxo, double_spend_input)
            .with_output(utxo, test_params.sender_offset_private_key)
            .unwrap();

        let factories = CryptoFactories::default();
        let mut stx_protocol = stx_builder.build::<HashDigest>(&factories).unwrap();
        stx_protocol.finalize(KernelFeatures::empty(), &factories).unwrap();

        let tx3 = stx_protocol.get_transaction().unwrap().clone();

        let tx1 = Arc::new(tx1);
        let tx2 = Arc::new(tx2);
        let tx3 = Arc::new(tx3);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 4,
            weight_tx_skip_count: 3,
        });

        let tx_weight = TransactionWeight::latest();
        unconfirmed_pool
            .insert_many(vec![tx1.clone(), tx2.clone(), tx3.clone()], &tx_weight)
            .unwrap();
        assert_eq!(unconfirmed_pool.len(), 3);

        let desired_weight = tx1.calculate_weight(&tx_weight) +
            tx2.calculate_weight(&tx_weight) +
            tx3.calculate_weight(&tx_weight) +
            1000;
        let results = unconfirmed_pool.highest_priority_txs(desired_weight).unwrap();
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
        unconfirmed_pool
            .insert_many(
                vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()],
                &tx_weight,
            )
            .unwrap();
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
        let _ = unconfirmed_pool.remove_published_and_discard_deprecated_transactions(&published_block);

        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig),);
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig),);
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig),);

        assert!(unconfirmed_pool.check_status());
    }

    #[test]
    fn test_discard_double_spend_txs() {
        let consensus = create_consensus_rules();
        let tx1 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(5), inputs:2, outputs:1).0);
        let tx2 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(4), inputs:3, outputs:1).0);
        let tx3 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(20), inputs:2, outputs:1).0);
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
            .unwrap();

        // The publishing of tx1 and tx3 will be double-spends and orphan tx5 and tx6
        let published_block = create_orphan_block(0, vec![(*tx1).clone(), (*tx2).clone(), (*tx3).clone()], &consensus);

        let _ = unconfirmed_pool.remove_published_and_discard_deprecated_transactions(&published_block); // Double spends are discarded

        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig),);
        assert!(unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig),);
        assert!(!unconfirmed_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig),);

        assert!(unconfirmed_pool.check_status());
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
        unconfirmed_pool.insert_many(txns.clone(), &tx_weight).unwrap();

        for txn in txns {
            for output in txn.as_ref().body.outputs() {
                assert!(unconfirmed_pool.verify_outputs_exist(&[output.hash()]));
                let signatures_by_output = unconfirmed_pool.txs_by_output.get(&output.hash()).unwrap();
                // Each output must be referenced by two transactions
                assert_eq!(signatures_by_output.len(), 2);
                // Verify kernel signature present at least once
                let mut found = 0u8;
                for signature in signatures_by_output {
                    if signature == &txn.as_ref().body.kernels()[0].excess_sig {
                        found += 1;
                    }
                }
                assert_eq!(found, 1);
            }
        }

        // Remove some transactions
        unconfirmed_pool.delete_transaction(&tx1.body.kernels()[0].excess_sig);
        unconfirmed_pool.delete_transaction(&tx4.body.kernels()[0].excess_sig);

        let txns = vec![
            Arc::new(tx2),
            // Transactions with duplicate outputs
            Arc::new(tx3),
        ];
        for txn in txns {
            for output in txn.as_ref().body.outputs() {
                let signatures_by_output = unconfirmed_pool.txs_by_output.get(&output.hash()).unwrap();
                // Each output must be referenced by one transactions
                assert_eq!(signatures_by_output.len(), 1);
                // Verify kernel signature present exactly once
                for signature in signatures_by_output {
                    assert_eq!(signature, &txn.as_ref().body.kernels()[0].excess_sig);
                }
            }
        }
    }
}
