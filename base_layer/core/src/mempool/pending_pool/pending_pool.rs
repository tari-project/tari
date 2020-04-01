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
        consts::MEMPOOL_PENDING_POOL_STORAGE_CAPACITY,
        pending_pool::PendingPoolError,
        priority::{FeePriority, TimelockPriority, TimelockedTransaction},
    },
    transactions::{transaction::Transaction, types::Signature},
};
use log::*;
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    sync::Arc,
};
use tari_crypto::tari_utilities::hex::Hex;

pub const LOG_TARGET: &str = "c::mp::pending_pool::pending_pool_storage";

/// Configuration for the PendingPool.
#[derive(Clone, Copy)]
pub struct PendingPoolConfig {
    /// The maximum number of transactions that can be stored in the Pending pool.
    pub storage_capacity: usize,
}

impl Default for PendingPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: MEMPOOL_PENDING_POOL_STORAGE_CAPACITY,
        }
    }
}

/// The Pending Pool contains all transactions that are restricted by time-locks. Once the time-locks have expired then
/// the transactions can be moved to the Unconfirmed Transaction Pool for inclusion in future blocks.
/// The txs_by_signature HashMap is used to find a transaction using its excess_sig, this functionality is used to match
/// transactions included in blocks with transactions stored in the pool.
/// The txs_by_fee_priority BTreeMap prioritize the transactions in the pool according to FeePriority, it allows
/// transactions to be inserted in sorted order based on their priority. The txs_by_timelock_priority BTreeMap
/// prioritize the transactions in the pool according to TimelockPriority, it allows transactions to be inserted in
/// sorted order based on the expiry of their time-locks.
pub struct PendingPool {
    config: PendingPoolConfig,
    txs_by_signature: HashMap<Signature, TimelockedTransaction>,
    txs_by_fee_priority: BTreeMap<FeePriority, Signature>,
    txs_by_timelock_priority: BTreeMap<TimelockPriority, Signature>,
}

impl PendingPool {
    /// Create a new PendingPool with the specified configuration.
    pub fn new(config: PendingPoolConfig) -> Self {
        Self {
            config,
            txs_by_signature: HashMap::new(),
            txs_by_fee_priority: BTreeMap::new(),
            txs_by_timelock_priority: BTreeMap::new(),
        }
    }

    fn lowest_fee_priority(&self) -> &FeePriority {
        self.txs_by_fee_priority.iter().next().unwrap().0
    }

    fn remove_tx_with_lowest_fee_priority(&mut self) {
        if let Some((_, tx_key)) = self
            .txs_by_fee_priority
            .iter()
            .next()
            .map(|(p, s)| (p.clone(), s.clone()))
        {
            if let Some(removed_tx) = self.txs_by_signature.remove(&tx_key) {
                trace!(
                    target: LOG_TARGET,
                    "Removing tx from pending pool: {:?}, {:?}",
                    removed_tx.fee_priority,
                    removed_tx.timelock_priority
                );
                self.txs_by_fee_priority.remove(&removed_tx.fee_priority);
                self.txs_by_timelock_priority.remove(&removed_tx.timelock_priority);
            }
        }
    }

    /// Insert a new transaction into the PendingPool. Low priority transactions will be removed to make space for
    /// higher priority transactions. The lowest priority transactions will be removed when the maximum capacity is
    /// reached and the new transaction has a higher priority than the currently stored lowest priority transaction.
    #[allow(clippy::map_entry)]
    pub fn insert(&mut self, tx: Arc<Transaction>) -> Result<(), PendingPoolError> {
        let tx_key = tx.body.kernels()[0].excess_sig.clone();
        if !self.txs_by_signature.contains_key(&tx_key) {
            debug!(
                target: LOG_TARGET,
                "Inserting tx into pending pool: {}",
                tx_key.get_signature().to_hex()
            );
            trace!(target: LOG_TARGET, "Transaction inserted: {}", tx);
            let prioritized_tx = TimelockedTransaction::try_from((*tx).clone())?;
            if self.txs_by_signature.len() >= self.config.storage_capacity {
                if prioritized_tx.fee_priority < *self.lowest_fee_priority() {
                    return Ok(());
                }
                self.remove_tx_with_lowest_fee_priority();
            }
            self.txs_by_fee_priority
                .insert(prioritized_tx.fee_priority.clone(), tx_key.clone());
            self.txs_by_timelock_priority
                .insert(prioritized_tx.timelock_priority.clone(), tx_key.clone());
            self.txs_by_signature.insert(tx_key, prioritized_tx);
        }
        Ok(())
    }

    /// Insert a set of new transactions into the PendingPool.
    pub fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) -> Result<(), PendingPoolError> {
        for tx in txs.into_iter() {
            self.insert(tx)?;
        }
        Ok(())
    }

    /// Check if a specific transaction is available in the PendingPool.
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig)
    }

    /// Remove double-spends from the PendingPoolStorage. These transactions were orphaned by the provided published
    /// block. Check if any of the unspent transactions in the PendingPool has inputs that was spent by the provided
    /// published block.
    fn discard_double_spends(&mut self, published_block: &Block) {
        let mut removed_tx_keys: Vec<Signature> = Vec::new();
        for (tx_key, ptx) in self.txs_by_signature.iter() {
            for input in ptx.transaction.body.inputs() {
                if published_block.body.inputs().contains(input) {
                    self.txs_by_fee_priority.remove(&ptx.fee_priority);
                    self.txs_by_timelock_priority.remove(&ptx.timelock_priority);
                    removed_tx_keys.push(tx_key.clone());
                }
            }
        }

        for tx_key in &removed_tx_keys {
            trace!(target: LOG_TARGET, "Removed double spends: {:?}", tx_key);
            self.txs_by_signature.remove(&tx_key);
        }
    }

    /// Remove transactions with expired time-locks so that they can be move to the UnconfirmedPool. Double spend
    /// transactions are also removed.
    pub fn remove_unlocked_and_discard_double_spends(
        &mut self,
        published_block: &Block,
    ) -> Result<Vec<Arc<Transaction>>, PendingPoolError>
    {
        self.discard_double_spends(published_block);

        let mut removed_txs: Vec<Arc<Transaction>> = Vec::new();
        let mut removed_tx_keys: Vec<TimelockPriority> = Vec::new();
        for (_, tx_key) in self.txs_by_timelock_priority.iter() {
            if self
                .txs_by_signature
                .get(tx_key)
                .ok_or(PendingPoolError::StorageOutofSync)?
                .max_timelock_height >
                published_block.header.height
            {
                break;
            }

            if let Some(removed_ptx) = self.txs_by_signature.remove(&tx_key) {
                self.txs_by_fee_priority.remove(&removed_ptx.fee_priority);
                removed_tx_keys.push(removed_ptx.timelock_priority);
                removed_txs.push(removed_ptx.transaction);
            }
        }

        for tx_key in &removed_tx_keys {
            trace!(target: LOG_TARGET, "Removed unlocked and double spends: {:?}", tx_key);
            self.txs_by_timelock_priority.remove(&tx_key);
        }

        Ok(removed_txs)
    }

    /// Returns the total number of time-locked transactions stored in the PendingPool.
    pub fn len(&self) -> usize {
        self.txs_by_signature.len()
    }

    /// Returns all transaction stored in the PendingPool.
    pub fn snapshot(&self) -> Vec<Arc<Transaction>> {
        self.txs_by_signature
            .iter()
            .map(|(_, ptx)| ptx.transaction.clone())
            .collect()
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self) -> u64 {
        self.txs_by_signature
            .iter()
            .fold(0, |weight, (_, ptx)| weight + ptx.transaction.calculate_weight())
    }

    #[cfg(test)]
    /// Checks the consistency status of the Hashmap and the BtreeMaps
    pub fn check_status(&self) -> bool {
        if (self.txs_by_fee_priority.len() != self.txs_by_signature.len()) ||
            (self.txs_by_timelock_priority.len() != self.txs_by_signature.len())
        {
            return false;
        }
        self.txs_by_fee_priority
            .iter()
            .all(|(_, tx_key)| self.txs_by_signature.contains_key(tx_key)) &&
            self.txs_by_timelock_priority
                .iter()
                .all(|(_, tx_key)| self.txs_by_signature.contains_key(tx_key))
    }
}

#[cfg(test)]
mod test {
    use crate::{
        consensus::Network,
        helpers::create_orphan_block,
        mempool::pending_pool::{PendingPool, PendingPoolConfig},
        transactions::tari_amount::MicroTari,
        tx,
    };
    use std::sync::Arc;

    #[test]
    fn test_insert_and_lru() {
        let tx1 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), lock: 500, inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(20), lock: 2150, inputs: 1, outputs: 2).0);
        let tx3 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(100), lock: 1000, inputs: 2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(30), lock: 2450, inputs: 2, outputs: 2).0);
        let tx5 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), lock: 1000, inputs: 3, outputs: 3).0);
        let tx6 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(75), lock: 1850, inputs: 2, outputs: 2).0);

        let mut pending_pool = PendingPool::new(PendingPoolConfig { storage_capacity: 3 });
        pending_pool
            .insert_txs(vec![
                tx1.clone(),
                tx2.clone(),
                tx3.clone(),
                tx4.clone(),
                tx5.clone(),
                tx6.clone(),
            ])
            .unwrap();
        // Check that lowest priority txs were removed to make room for higher priority transactions
        assert_eq!(pending_pool.len(), 3);
        assert_eq!(
            pending_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig),
            true
        );
        assert_eq!(
            pending_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            pending_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig),
            true
        );
        assert_eq!(
            pending_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            pending_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            pending_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig),
            true
        );

        assert!(pending_pool.check_status());
    }

    #[test]
    fn test_remove_unlocked_and_discard_double_spends() {
        let network = Network::LocalNet;
        let consensus_constants = network.create_consensus_constants();
        let tx1 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), lock: 500, inputs: 2, outputs: 1).0);
        let tx2 =
            Arc::new(tx!(MicroTari(10_000), fee: MicroTari(20), lock: 0, inputs: 1, maturity: 2150, outputs: 2).0);
        let tx3 = Arc::new(
            tx!(MicroTari(10_000), fee: MicroTari(100), lock: 0, inputs: 2, maturity: 1000, outputs:
        1)
            .0,
        );
        let tx4 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(30), lock: 2450, inputs: 2, outputs: 2).0);
        let tx5 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), lock: 1000, inputs: 3, outputs: 3).0);
        let tx6 =
            Arc::new(tx!(MicroTari(10_000), fee: MicroTari(75), lock: 1450, inputs: 2, maturity: 1400, outputs: 2).0);

        let mut pending_pool = PendingPool::new(PendingPoolConfig { storage_capacity: 10 });
        pending_pool
            .insert_txs(vec![
                tx1.clone(),
                tx2.clone(),
                tx3.clone(),
                tx4.clone(),
                tx5.clone(),
                tx6.clone(),
            ])
            .unwrap();
        assert_eq!(pending_pool.len(), 6);

        let snapshot_txs = pending_pool.snapshot();
        assert_eq!(snapshot_txs.len(), 6);
        assert!(snapshot_txs.contains(&tx1));
        assert!(snapshot_txs.contains(&tx2));
        assert!(snapshot_txs.contains(&tx3));
        assert!(snapshot_txs.contains(&tx4));
        assert!(snapshot_txs.contains(&tx5));
        assert!(snapshot_txs.contains(&tx6));

        let published_block = create_orphan_block(1500, vec![(*tx6).clone()], &consensus_constants);
        let unlocked_txs = pending_pool
            .remove_unlocked_and_discard_double_spends(&published_block)
            .unwrap();

        assert_eq!(pending_pool.len(), 2);
        assert_eq!(
            pending_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig),
            true
        );
        assert_eq!(
            pending_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig),
            true
        );

        assert_eq!(unlocked_txs.len(), 3);
        assert!(unlocked_txs.contains(&tx1));
        assert!(unlocked_txs.contains(&tx3));
        assert!(unlocked_txs.contains(&tx5));

        assert!(pending_pool.check_status());
    }
}
