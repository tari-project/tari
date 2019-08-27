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
    consts::{MEMPOOL_ORPHAN_POOL_CACHE_TTL, MEMPOOL_ORPHAN_POOL_STORAGE_CAPACITY},
    transaction::{Transaction, TransactionInput},
    types::{Signature, SignatureHash},
};
use merklemountainrange::mmr::MerkleMountainRange;
use std::time::Duration;
use ttl_cache::TtlCache;

/// Configuration for the OrphanPool
#[derive(Clone, Copy)]
pub struct OrphanPoolConfig {
    /// The maximum number of transactions that can be stored in the Orphan pool
    pub storage_capacity: usize,
    /// The Time-to-live for each stored transaction
    pub tx_ttl: Duration,
}

impl Default for OrphanPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: MEMPOOL_ORPHAN_POOL_STORAGE_CAPACITY,
            tx_ttl: MEMPOOL_ORPHAN_POOL_CACHE_TTL,
        }
    }
}

/// The Orphan Pool contains all the received transactions that attempt to spend UTXOs that don't exist. These UTXOs
/// might exist in the future if these transactions are from a series or set of transactions that need to be processed
/// in a specific order. Some of these transactions might still be constrained by pending time-locks.
pub struct OrphanPool {
    config: OrphanPoolConfig,
    txs_by_signature: TtlCache<Signature, Transaction>,
}

impl OrphanPool {
    /// Create a new OrphanPool with the specified configuration
    pub fn new(config: OrphanPoolConfig) -> Self {
        Self {
            config,
            txs_by_signature: TtlCache::new(config.storage_capacity),
        }
    }

    /// Insert a new transaction into the OrphanPool. Orphaned transactions will have a limited Time-to-live and will be
    /// discarded if the UTXOs they require are not created before the Time-to-live threshold is reached.
    pub fn insert(&mut self, tx: Transaction) {
        let tx_key = tx.body.kernels[0].excess_sig.clone();
        let _ = self.txs_by_signature.insert(tx_key, tx, self.config.tx_ttl);
    }

    /// Insert a set of new transactions into the OrphanPool
    pub fn insert_txs(&mut self, txs: Vec<Transaction>) {
        for tx in txs.into_iter() {
            self.insert(tx);
        }
    }

    /// Check if a transaction is stored in the OrphanPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig)
    }

    /// Check if the required UTXOs have been created and if the status of any of the transactions in the OrphanPool has
    /// changed. Remove valid transactions and valid transactions with time-locks from the OrphanPool.
    // TODO: A reference to the UTXO set should not be passed in like this, but rather a handle or stream to the Chain
    // (BlockchainDatabase or BlockchainService) should be provided to the OrphanPool during creation allowing a
    // Chain call to be used to query the current block_height and UTXO set
    pub fn scan_for_and_remove_unorphaned_txs(
        &mut self,
        block_height: u64,
        utxos: &MerkleMountainRange<TransactionInput, SignatureHash>,
    ) -> (Vec<Transaction>, Vec<Transaction>)
    {
        let mut removed_tx_keys: Vec<Signature> = Vec::new();
        let mut removed_timelocked_tx_keys: Vec<Signature> = Vec::new();
        for (tx_key, tx) in self.txs_by_signature.iter() {
            if tx.body.inputs.iter().all(|input| utxos.contains(input)) {
                if tx.body.kernels[0].lock_height <= block_height {
                    removed_tx_keys.push(tx_key.clone());
                } else {
                    removed_timelocked_tx_keys.push(tx_key.clone());
                }
            }
        }

        let mut removed_txs: Vec<Transaction> = Vec::with_capacity(removed_tx_keys.len());
        removed_tx_keys.iter().for_each(|tx_key| {
            if let Some(tx) = self.txs_by_signature.remove(&tx_key) {
                removed_txs.push(tx);
            }
        });

        let mut removed_timelocked_txs: Vec<Transaction> = Vec::with_capacity(removed_timelocked_tx_keys.len());
        removed_timelocked_tx_keys.iter().for_each(|tx_key| {
            if let Some(tx) = self.txs_by_signature.remove(&tx_key) {
                removed_timelocked_txs.push(tx);
            }
        });

        (removed_txs, removed_timelocked_txs)
    }

    /// Returns the total number of orphaned transactions stored in the OrphanPool
    pub fn len(&mut self) -> usize {
        let mut count = 0;
        self.txs_by_signature.iter().for_each(|_| count += 1);
        (count)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        tari_amount::MicroTari,
        test_utils::builders::{create_test_block, create_test_tx, create_test_utxos, extend_test_utxos},
        transaction::TransactionInput,
    };
    use std::{thread, time::Duration};

    #[test]
    fn test_insert_rlu_and_ttl() {
        let tx1 = create_test_tx(MicroTari(10_000), MicroTari(500), 4000, 2, 1);
        let tx2 = create_test_tx(MicroTari(10_000), MicroTari(300), 3000, 2, 1);
        let tx3 = create_test_tx(MicroTari(10_000), MicroTari(100), 2500, 2, 1);
        let tx4 = create_test_tx(MicroTari(10_000), MicroTari(200), 1000, 2, 1);
        let tx5 = create_test_tx(MicroTari(10_000), MicroTari(500), 2000, 2, 1);
        let tx6 = create_test_tx(MicroTari(10_000), MicroTari(600), 5500, 2, 1);

        let mut orphan_pool = OrphanPool::new(OrphanPoolConfig {
            storage_capacity: 3,
            tx_ttl: Duration::from_millis(50),
        });
        orphan_pool.insert_txs(vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone()]);
        // Check that oldest utx was removed to make room for new incoming transaction
        assert!(!orphan_pool.has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig));
        assert!(orphan_pool.has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig));
        assert!(orphan_pool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig));
        assert!(orphan_pool.has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig));

        // Check that transactions that have been in the pool for longer than their Time-to-live have been removed
        thread::sleep(Duration::from_millis(51));
        orphan_pool.insert_txs(vec![tx5.clone(), tx6.clone()]);
        assert!(!orphan_pool.has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig));
        assert!(!orphan_pool.has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig));
        assert!(!orphan_pool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig));
        assert!(!orphan_pool.has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig));
        assert!(orphan_pool.has_tx_with_excess_sig(&tx5.body.kernels[0].excess_sig));
        assert!(orphan_pool.has_tx_with_excess_sig(&tx6.body.kernels[0].excess_sig));
        assert_eq!(orphan_pool.len(), 2);
    }

    #[test]
    fn remove_remove_valid() {
        let tx1 = create_test_tx(MicroTari(10_000), MicroTari(500), 1100, 2, 1);
        let tx2 = create_test_tx(MicroTari(10_000), MicroTari(300), 1700, 2, 1);
        let tx3 = create_test_tx(MicroTari(10_000), MicroTari(100), 2500, 1, 1);
        let mut tx4 = create_test_tx(MicroTari(10_000), MicroTari(200), 3100, 2, 1);
        let mut tx5 = create_test_tx(MicroTari(10_000), MicroTari(500), 4500, 2, 1);
        let tx6 = create_test_tx(MicroTari(10_000), MicroTari(600), 5200, 2, 1);
        // Publishing of tx1 and tx2 will create the UTXOs required by tx4 and tx5
        tx4.body.inputs.clear();
        tx1.body
            .outputs
            .iter()
            .for_each(|output| tx4.body.inputs.push(TransactionInput::from(output.clone())));

        tx5.body.inputs.clear();
        tx2.body
            .outputs
            .iter()
            .for_each(|output| tx5.body.inputs.push(TransactionInput::from(output.clone())));

        let mut orphan_pool = OrphanPool::new(OrphanPoolConfig::default());
        orphan_pool.insert_txs(vec![tx3.clone(), tx4.clone(), tx5.clone()]);

        let published_block = create_test_block(3000, vec![tx6.clone()]);
        let mut utxos = create_test_utxos(&published_block);
        let (txs, timelocked_txs) =
            orphan_pool.scan_for_and_remove_unorphaned_txs(published_block.header.height, &utxos);
        assert_eq!(orphan_pool.len(), 3);
        assert_eq!(txs.len(), 0);
        assert_eq!(timelocked_txs.len(), 0);
        assert!(orphan_pool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig));
        assert!(orphan_pool.has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig));
        assert!(orphan_pool.has_tx_with_excess_sig(&tx5.body.kernels[0].excess_sig));

        let published_block = create_test_block(3500, vec![tx1.clone(), tx2.clone()]);
        extend_test_utxos(&mut utxos, &published_block);
        let (txs, timelocked_txs) =
            orphan_pool.scan_for_and_remove_unorphaned_txs(published_block.header.height, &utxos);
        assert_eq!(orphan_pool.len(), 1);
        assert_eq!(txs.len(), 1);
        assert_eq!(timelocked_txs.len(), 1);
        assert!(orphan_pool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig));
        assert!(txs.contains(&tx4));
        assert!(timelocked_txs.contains(&tx5));
    }
}
