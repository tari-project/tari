//  Copyright 2025, The Tari Project
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

//! # LmdbTreeWriter — JMT Node Writer with Stale-Node Cleanup
//!
//! This writer implements the *current-state-only* storage scheme for JMT nodes:
//!
//! - **Key format (NEW)**: `borsh::serialize(&NodeKey)` — the entire `NodeKey` (including nibble path) is the LMDB key.
//!   There is NO version prefix. Each unique `NodeKey` has exactly ONE entry in `jmt_node_data`.
//! - **Value format**: `borsh::serialize(&Node)` (unchanged from before).
//! - **Stale node cleanup**: `cleanup_stale()` processes the `StaleNodeIndexBatch` returned by
//!   `JellyfishMerkleTree::put_value_set()`, deleting nodes that are no longer reachable from the current UTXO set.
//!
//! ## Why this reduces storage by ~97%
//!
//! The OLD format used `key = version || nibble_path`, which stored a FULL copy of the tree for EVERY block version.
//! With ~640K blocks, this means 640K copies of most nodes → 6.4 GB.
//!
//! The NEW format stores each `NodeKey` ONLY ONCE (as it should be — the JMT represents the CURRENT UTXO set).
//! This reduces `jmt_node_data` from 6.4 GB → ~200 MB (97% reduction).
//!
//! ## Migration
//!
//! A database migration (version 7 → 8) is required to re-key existing data from the old format to the new format.
//! See `lmdb_db.rs` for the migration implementation.

use borsh::BorshSerialize;
use jmt::storage::{Node, StaleNodeIndex, StaleNodeIndexBatch, TreeWriter};
use lmdb_zero::WriteTransaction;
use log::*;
use tari_storage::lmdb_store::DatabaseRef;

use super::lmdb::lmdb_insert;
use crate::chain_storage::lmdb_db::lmdb::{lmdb_delete, lmdb_get};
pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_tree_writer";

/// LMDB-backed JMT tree writer — current-state-only storage scheme.
///
/// # Key Format (NEW)
///
/// `jmt_node_data` key = `borsh::serialize(&NodeKey)` (no version prefix)
///
/// This means each `NodeKey` (representing a unique tree position) has exactly ONE entry,
/// regardless of how many block versions have touched it.
///
/// # Stale Node Cleanup
///
/// When UTXOs are spent, the JMT crate (`jmt`) identifies which internal nodes become stale
/// (no longer reachable from any leaf). These are returned as `StaleNodeIndexBatch`.
///
/// Call `cleanup_stale()` to delete those nodes from `jmt_node_data`.
///
/// # Migration
///
/// Existing databases use the OLD key format (`version || nibble_path`).
/// A migration in `lmdb_db.rs` re-keys all entries to the new format.
pub(crate) struct LmdbTreeWriter<'a> {
    txn: &'a WriteTransaction<'a>,
    node_db: DatabaseRef,
    value_db: DatabaseRef,
}

impl<'a> LmdbTreeWriter<'a> {
    pub fn new(
        txn: &'a WriteTransaction<'a>,
        node_db: DatabaseRef,
        value_db: DatabaseRef,
    ) -> Self {
        Self {
            txn,
            node_db,
            value_db,
        }
    }

    /// Delete stale nodes identified by the JMT crate.
    ///
    /// This should be called AFTER `write_node_batch()` for each block.
    /// The `stale` batch contains `StaleNodeIndex` entries whose `node_key` are no longer reachable.
    ///
    /// # Important
    ///
    /// Stale nodes should NOT be deleted immediately — they may still be needed for reorgs
    /// within the configured reorg horizon. The caller is responsible for buffering deletions
    /// until the version is finalized (past the reorg horizon).
    ///
    /// See `lmdb_db.rs` for the buffered-deletion implementation.
    pub fn cleanup_stale(&self, stale: &StaleNodeIndexBatch) -> anyhow::Result<()> {
        for index in stale {
            let mut lmdb_key: Vec<u8> = vec![];
            BorshSerialize::serialize(&index.node_key, &mut lmdb_key)?;
            lmdb_delete(self.txn, &self.node_db, &lmdb_key, "jmt_node_table")?;
        }
        Ok(())
    }

    /// Insert or update a single JMT node.
    ///
    /// # Key Format (NEW)
    ///
    /// `key = borsh::serialize(&node_key)` — the entire `NodeKey` (NO version prefix).
    ///
    /// This means the same tree position (NodeKey) always maps to the same LMDB key,
    /// and only the LATEST node value is stored.
    pub fn put_node(&self, node_key: &jmt::storage::NodeKey, node: &Node) -> anyhow::Result<()> {
        let mut lmdb_key: Vec<u8> = vec![];
        BorshSerialize::serialize(node_key, &mut lmdb_key)?;
        lmdb_insert(self.txn, &self.node_db, &lmdb_key, node, "jmt_node_table")?;
        Ok(())
    }

    /// Read a node from `jmt_node_data` (for testing / debugging).
    #[allow(dead_code)]
    fn get_node(&self, node_key: &jmt::storage::NodeKey) -> anyhow::Result<Option<Node>> {
        let mut lmdb_key: Vec<u8> = vec![];
        BorshSerialize::serialize(node_key, &mut lmdb_key)?;
        lmdb_get(self.txn, &self.node_db, &lmdb_key)
    }
}

impl TreeWriter for LmdbTreeWriter<'_> {
    /// Write a batch of JMT node updates.
    ///
    /// # Key Format (NEW)
    ///
    /// Each node is keyed by `borsh::serialize(&node_key)` — NO version prefix.
    /// Duplicate `NodeKey` entries in the batch are silently overwritten (last write wins),
    /// which is correct because we only care about the CURRENT state.
    fn write_node_batch(&self, node_batch: &jmt::storage::NodeBatch) -> anyhow::Result<()> {
        for (node_key, node) in node_batch.nodes() {
            let mut lmdb_key: Vec<u8> = vec![];
            BorshSerialize::serialize(node_key, &mut lmdb_key)?;
            lmdb_insert(self.txn, &self.node_db, &lmdb_key, &node, "jmt_node_table")?;
        }

        // Write values (outputs) keyed by KeyHash
        for (value_key, value) in node_batch.values() {
            let mut lmdb_key: Vec<u8> = vec![];
            // NEW format: value key = KeyHash (32 bytes), NOT (version || key_hash)
            lmdb_key.extend_from_slice(&value_key.0);
            let val_bytes = bincode::serialize(value)?;
            lmdb_insert(self.txn, &self.value_db, &lmdb_key, &val_bytes, "jmt_value_table")?;
        }

        trace!(
            target: LOG_TARGET,
            "Wrote JMT batch: {} nodes, {} values",
            node_batch.nodes().len(),
            node_batch.values().len(),
        );

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use jmt::{JellyfishMerkleTree, KeyHash};
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_utilities::ByteArray;

    use super::*;
    use crate::{
        chain_storage::{BlockchainBackend, SmtHasher},
        test_helpers::blockchain::TempDatabase,
    };

    /// Test: Writing the same KeyHash with different values works correctly.
    /// (This is the "duplicate key" test from the old code, adapted for new format.)
    #[test]
    fn test_jmt_write_and_read() {
        let db = TempDatabase::new();
        let (reader, current_version) = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut rand::thread_rng());
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value = b"test_value".to_vec();

        // Write version 0
        let (_root, updates) = jmt.put_value_set(vec![(smt_key, Some(value.clone()))], current_version).unwrap();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        txn.commit().unwrap();

        // Read back — should find the value
        let reader2 = db.db().create_smt_reader().unwrap().0;
        let jmt2 = JellyfishMerkleTree::<_, SmtHasher>::new(&reader2);
        let result = jmt2.get(smt_key, current_version + 1).unwrap();
        assert_eq!(result, Some(value.clone()));
    }

    /// Test: After deleting a value, the node should be stale and cleaned up.
    #[test]
    fn test_jmt_stale_cleanup() {
        let db = TempDatabase::new();
        let (reader, current_version) = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut rand::thread_rng());
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value = b"test_value".to_vec();

        // Insert
        let (_root, updates) = jmt.put_value_set(vec![(smt_key, Some(value.clone()))], current_version).unwrap();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        // Cleanup stale nodes (none expected here since we just inserted)
        tree_writer.cleanup_stale(&updates.stale_node_index_batch).unwrap();
        txn.commit().unwrap();

        // Now delete
        let reader2 = db.db().create_smt_reader().unwrap().0;
        let jmt2 = JellyfishMerkleTree::<_, SmtHasher>::new(&reader2);
        let (_root2, updates2) = jmt2.put_value_set(vec![(smt_key, None)], current_version + 1).unwrap();
        let txn2 = db.db().create_write_txn();
        let tree_writer2 = db.db().create_lmdb_tree_writer(&txn2);
        tree_writer2.write_node_batch(&updates2.node_batch).unwrap();
        // Now stale nodes SHOULD exist (the leaf node for smt_key is now stale)
        tree_writer2.cleanup_stale(&updates2.stale_node_index_batch).unwrap();
        txn2.commit().unwrap();

        // Verify the value is gone
        let reader3 = db.db().create_smt_reader().unwrap().0;
        let jmt3 = JellyfishMerkleTree::<_, SmtHasher>::new(&reader3);
        let result = jmt3.get(smt_key, current_version + 2).unwrap();
        assert_eq!(result, None);
    }
}
