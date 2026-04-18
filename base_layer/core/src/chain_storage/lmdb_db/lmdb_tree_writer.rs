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

use jmt::storage::{Node, StaleNodeIndexBatch, TreeWriter};
use lmdb_zero::{WriteTransaction, error::LmdbResultExt};
use log::*;
use tari_storage::lmdb_store::DatabaseRef;
use tari_utilities::hex::Hex;

use super::lmdb::lmdb_insert;
use crate::chain_storage::lmdb_db::lmdb::{lmdb_delete, lmdb_delete_keys_starting_with, lmdb_fetch_matching_after};
pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_tree_writer";

pub(crate) struct LmdbTreeWriter<'a> {
    txn: &'a WriteTransaction<'a>,
    node_db: DatabaseRef,
    value_db: DatabaseRef,
    unique_key_db: DatabaseRef,
    stale_node_index_db: DatabaseRef,
}

impl<'a> LmdbTreeWriter<'a> {
    pub fn new(
        txn: &'a WriteTransaction<'a>,
        node_db: DatabaseRef,
        value_db: DatabaseRef,
        unique_key_db: DatabaseRef,
        stale_node_index_db: DatabaseRef,
    ) -> Self {
        Self {
            txn,
            node_db,
            value_db,
            unique_key_db,
            stale_node_index_db,
        }
    }

    pub fn delete_all_for_version(&self, version: u64) -> anyhow::Result<()> {
        let key = version.to_be_bytes();
        let nodes = lmdb_delete_keys_starting_with::<Node>(self.txn, &self.node_db, &key)?;
        warn!(target: LOG_TARGET, "Deleted {} nodes for version {}", nodes.len(), version);
        let values = lmdb_delete_keys_starting_with::<Vec<u8>>(self.txn, &self.value_db, &key)?;
        warn!(target: LOG_TARGET, "Deleted {} values for version {}", values.len(), version);

        for (value_key, _) in values {
            let mut lmdb_key: Vec<u8> = vec![];
            lmdb_key.extend_from_slice(value_key.get(8..).ok_or(anyhow::anyhow!("Value key is too short"))?);
            lmdb_key.extend_from_slice(value_key.get(0..8).ok_or(anyhow::anyhow!("Value key is too short"))?);
            match lmdb_delete(self.txn, &self.unique_key_db, &lmdb_key, "jmt_unique_key_table") {
                Ok(_) => {
                    debug!(target: LOG_TARGET, "Deleted unique key {} for version {}", lmdb_key.to_hex(), version);
                },
                Err(e) => {
                    debug!(target: LOG_TARGET, "Failed to delete unique key {} for version {}: {}", lmdb_key.to_hex(), version, e);
                },
            }
        }

        let stale_entries =
            lmdb_delete_keys_starting_with::<u8>(self.txn, &self.stale_node_index_db, &key)?;
        warn!(target: LOG_TARGET, "Deleted {} stale index entries for version {}", stale_entries.len(), version);

        Ok(())
    }

    pub fn put_node(&self, node_key: &jmt::storage::NodeKey, node: &jmt::storage::Node) -> anyhow::Result<()> {
        let mut lmdb_key: Vec<u8> = vec![];
        lmdb_key.extend_from_slice(&node_key.version().to_be_bytes());
        borsh::BorshSerialize::serialize(&node_key.nibble_path(), &mut lmdb_key)?;

        lmdb_insert(self.txn, &self.node_db, &lmdb_key, node, "jmt_node_table")?;
        Ok(())
    }

    pub fn write_stale_node_index_batch(&self, stale_node_index_batch: &StaleNodeIndexBatch) -> anyhow::Result<()> {
        for stale_index in stale_node_index_batch {
            // Key format: stale_since_version(8 bytes BE) || node_version(8 bytes BE) || nibble_path(borsh)
            let mut lmdb_key: Vec<u8> = Vec::new();
            lmdb_key.extend_from_slice(&stale_index.stale_since_version.to_be_bytes());
            lmdb_key.extend_from_slice(&stale_index.node_key.version().to_be_bytes());
            borsh::BorshSerialize::serialize(&stale_index.node_key.nibble_path(), &mut lmdb_key)?;
            // Value: empty, the key itself encodes all needed info
            lmdb_insert(self.txn, &self.stale_node_index_db, &lmdb_key, &0u8, "jmt_stale_node_index")?;
        }
        trace!(target: LOG_TARGET, "Wrote {} stale node indices", stale_node_index_batch.len());
        Ok(())
    }

    /// Prune stale JMT nodes that became stale at versions strictly below `prune_below_version`.
    ///
    /// This scans `jmt_stale_node_index_data` from the beginning, and for each entry whose
    /// `stale_since_version < prune_below_version`, deletes the corresponding node from `jmt_node_data`
    /// and removes the stale index entry itself.
    ///
    /// Returns `(nodes_deleted, index_entries_removed)`.
    pub fn prune_stale_nodes(&self, prune_below_version: u64) -> anyhow::Result<(u64, u64)> {
        let access = self.txn.access();
        let mut cursor = self.txn.cursor(self.stale_node_index_db.as_ref()).map_err(|e| {
            anyhow::anyhow!("Could not get cursor for jmt_stale_node_index: {e}")
        })?;

        // Collect keys to process: we need to collect because we can't mutate while iterating
        let mut stale_keys: Vec<Vec<u8>> = Vec::new();
        let mut row = match cursor.first::<[u8], [u8]>(&access).to_opt()? {
            Some(r) => r,
            None => {
                trace!(target: LOG_TARGET, "JMT prune: stale node index is empty, nothing to prune");
                return Ok((0, 0));
            },
        };

        loop {
            let key = row.0;
            // The first 8 bytes are stale_since_version (big-endian)
            if key.len() < 8 {
                break;
            }
            let stale_since_version = u64::from_be_bytes(key[..8].try_into()?);
            if stale_since_version >= prune_below_version {
                break;
            }
            stale_keys.push(key.to_vec());
            row = match cursor.next::<[u8], [u8]>(&access).to_opt()? {
                Some(r) => r,
                None => break,
            };
        }
        // Drop the cursor and access before mutating
        drop(cursor);
        drop(access);

        let mut nodes_deleted: u64 = 0;
        let mut index_entries_removed: u64 = 0;

        for stale_key in &stale_keys {
            if stale_key.len() < 16 {
                warn!(target: LOG_TARGET, "JMT prune: stale index key too short ({} bytes), skipping", stale_key.len());
                continue;
            }
            // Extract the node key: node_version(8 bytes) || nibble_path(rest) — bytes [8..]
            let node_key_bytes = &stale_key[8..];

            // Delete the node from jmt_node_data
            match lmdb_delete(self.txn, self.node_db.as_ref(), node_key_bytes, "jmt_node_table") {
                Ok(()) => {
                    nodes_deleted += 1;
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "JMT prune: node not found in jmt_node_data (may have been deleted by reorg): {e}");
                },
            }

            // Delete the stale index entry
            match lmdb_delete(self.txn, self.stale_node_index_db.as_ref(), stale_key.as_slice(), "jmt_stale_node_index") {
                Ok(()) => {
                    index_entries_removed += 1;
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "JMT prune: failed to delete stale index entry: {e}");
                },
            }
        }

        Ok((nodes_deleted, index_entries_removed))
    }

    /// Prune up to `max_batch_size` stale JMT nodes with `stale_since_version < prune_below_version`.
    ///
    /// Returns `(nodes_deleted, index_entries_removed, has_more)` where `has_more` is `true`
    /// if there are still stale entries remaining below the target version.
    pub fn prune_stale_nodes_batch(
        &self,
        prune_below_version: u64,
        max_batch_size: u64,
    ) -> anyhow::Result<(u64, u64, bool)> {
        let access = self.txn.access();
        let mut cursor = self.txn.cursor(self.stale_node_index_db.as_ref()).map_err(|e| {
            anyhow::anyhow!("Could not get cursor for jmt_stale_node_index: {e}")
        })?;

        let mut stale_keys: Vec<Vec<u8>> = Vec::new();
        let mut has_more = false;
        let mut row = match cursor.first::<[u8], [u8]>(&access).to_opt()? {
            Some(r) => r,
            None => {
                return Ok((0, 0, false));
            },
        };

        loop {
            let key = row.0;
            if key.len() < 8 {
                break;
            }
            let stale_since_version = u64::from_be_bytes(key[..8].try_into()?);
            if stale_since_version >= prune_below_version {
                break;
            }
            if stale_keys.len() as u64 >= max_batch_size {
                has_more = true;
                break;
            }
            stale_keys.push(key.to_vec());
            row = match cursor.next::<[u8], [u8]>(&access).to_opt()? {
                Some(r) => r,
                None => break,
            };
        }
        drop(cursor);
        drop(access);

        let mut nodes_deleted: u64 = 0;
        let mut index_entries_removed: u64 = 0;

        for stale_key in &stale_keys {
            if stale_key.len() < 16 {
                warn!(target: LOG_TARGET, "JMT prune: stale index key too short ({} bytes), skipping", stale_key.len());
                continue;
            }
            let node_key_bytes = &stale_key[8..];

            match lmdb_delete(self.txn, self.node_db.as_ref(), node_key_bytes, "jmt_node_table") {
                Ok(()) => {
                    nodes_deleted += 1;
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "JMT prune: node not found in jmt_node_data (may have been deleted by reorg): {e}");
                },
            }

            match lmdb_delete(self.txn, self.stale_node_index_db.as_ref(), stale_key.as_slice(), "jmt_stale_node_index") {
                Ok(()) => {
                    index_entries_removed += 1;
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "JMT prune: failed to delete stale index entry: {e}");
                },
            }
        }

        Ok((nodes_deleted, index_entries_removed, has_more))
    }
}

impl TreeWriter for LmdbTreeWriter<'_> {
    fn write_node_batch(&self, node_batch: &jmt::storage::NodeBatch) -> anyhow::Result<()> {
        for (node_key, node) in node_batch.nodes() {
            let mut lmdb_key: Vec<u8> = vec![];
            lmdb_key.extend_from_slice(&node_key.version().to_be_bytes());
            borsh::BorshSerialize::serialize(&node_key.nibble_path(), &mut lmdb_key)?;
            lmdb_insert(self.txn, &self.node_db, &lmdb_key, &node, "jmt_node_table")?;
        }
        // let mut duplicates = HashMap::new();
        for (value_key, value) in node_batch.values() {
            let mut lmdb_key: Vec<u8> = vec![];
            lmdb_key.extend_from_slice(&value_key.0.to_be_bytes());
            lmdb_key.extend_from_slice(&value_key.1.0);
            let val_bytes = bincode::serialize(value)?;
            lmdb_insert(self.txn, &self.value_db, &lmdb_key, &val_bytes, "jmt_value_table")?;

            // see if there are any values already.
            let existing_values: Vec<(Vec<u8>, Option<Vec<u8>>)> =
                lmdb_fetch_matching_after(self.txn, &self.unique_key_db, &value_key.1.0)?;
            let mut existing_history = vec![];
            for (key, x) in existing_values {
                let version = u64::from_be_bytes(key.get(32..).ok_or(anyhow::anyhow!("invalid bytes"))?.try_into()?);
                existing_history.push((version, x));
            }
            // sort by version
            existing_history.sort_by_key(|a| a.0);

            let latest_value = existing_history.last().and_then(|x| x.1.clone());
            match (value, &latest_value) {
                (None, _) => {
                    if latest_value.is_none() {
                        trace!(target: LOG_TARGET, "Found no existing JMT unique key for version {}, creating it as None", value_key.0);
                    }
                    let mut lmdb_key: Vec<u8> = vec![];
                    lmdb_key.extend_from_slice(value_key.1.0.as_slice());
                    lmdb_key.extend_from_slice(&value_key.0.to_be_bytes());
                    lmdb_insert(self.txn, &self.unique_key_db, &lmdb_key, value, "jmt_unique_key_table")?;
                },
                (Some(_v), Some(_x)) => {
                    trace!(target: LOG_TARGET, "Found existing unique key {} for version {}", value_key.1 .0.to_hex(), value_key.0);
                    return Err(anyhow::anyhow!("Duplicate value key found in batch"));
                },
                (Some(_v), None) => {
                    let mut lmdb_key: Vec<u8> = vec![];
                    lmdb_key.extend_from_slice(value_key.1.0.as_slice());
                    lmdb_key.extend_from_slice(&value_key.0.to_be_bytes());
                    lmdb_insert(self.txn, &self.unique_key_db, &lmdb_key, value, "jmt_unique_key_table")?;
                },
            };
        }
        trace!(target: LOG_TARGET, "Wrote JMT batch of {} nodes and {} values", node_batch.nodes().len(), node_batch.values().len());
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use jmt::{JellyfishMerkleTree, KeyHash};
    use rand::rngs::OsRng;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_utilities::ByteArray;

    use super::*;
    use crate::{
        chain_storage::{BlockchainBackend, SmtHasher},
        test_helpers::blockchain::TempDatabase,
    };

    #[test]
    fn test_jmt_does_not_accept_duplicates() {
        let db = TempDatabase::new();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value = b"test_value".to_vec();
        let (_root, updates) = jmt.put_value_set(vec![(smt_key, Some(value.clone()))], 0).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();

        txn.commit().unwrap();
        // Try again for new version.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (_root, update2) = jmt.put_value_set(vec![(smt_key, Some(value))], 1).unwrap();
        assert!(
            tree_writer.write_node_batch(&update2.node_batch).is_err(),
            "Duplicate key error expected"
        );
    }

    #[test]
    fn test_jmt_does_accept_duplicate_if_deleted() {
        // If a key in the jmt is deleted, it can be added later.
        let db = TempDatabase::new();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value = b"test_value".to_vec();
        let (_root, updates) = jmt.put_value_set(vec![(smt_key, Some(value.clone()))], 0).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();

        txn.commit().unwrap();
        // Try again for new version.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (_root, update2) = jmt.put_value_set(vec![(smt_key, None)], 1).unwrap();
        tree_writer.write_node_batch(&update2.node_batch).unwrap();

        txn.commit().unwrap();

        // Try again for version 2.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (_root, update2) = jmt.put_value_set(vec![(smt_key, Some(value))], 2).unwrap();
        tree_writer.write_node_batch(&update2.node_batch).unwrap();

        txn.commit().unwrap();
    }

    #[test]
    fn test_jmt_deletes_block_on_reorg() {
        let db = TempDatabase::new();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value = b"test_value".to_vec();
        let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value.clone()))], 0).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();

        txn.commit().unwrap();
        // Try again for new version.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key2 = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (root1, update2) = jmt.put_value_set(vec![(smt_key2, Some(value.clone()))], 1).unwrap();
        tree_writer.write_node_batch(&update2.node_batch).unwrap();
        txn.commit().unwrap();

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.delete_all_for_version(1).unwrap();
        txn.commit().unwrap();

        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let root2 = jmt.get_root_hash(0).unwrap();

        assert_eq!(root, root2);

        // Test that you can add it back again.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (root1_v2, update2) = jmt.put_value_set(vec![(smt_key2, Some(value))], 1).unwrap();
        tree_writer.write_node_batch(&update2.node_batch).unwrap();
        txn.commit().unwrap();

        assert_eq!(root1, root1_v2);
    }

    #[test]
    fn test_jmt_prune_stale_nodes_batch_returns_has_more() {
        let db = TempDatabase::new();

        let mut keys = Vec::new();
        // Insert 5 versions with distinct keys to build up stale nodes
        for v in 0u64..5 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            keys.push(smt_key);
            let value = format!("value_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (_root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer.write_stale_node_index_batch(&updates.stale_node_index_batch).unwrap();
            txn.commit().unwrap();
        }

        // Prune with batch size 1 – should signal has_more=true
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (nodes_deleted, index_removed, has_more) = tree_writer.prune_stale_nodes_batch(5, 1).unwrap();
        txn.commit().unwrap();
        assert!(nodes_deleted <= 1);
        assert!(index_removed <= 1);
        assert!(has_more, "Expected has_more=true when batch_size=1 and multiple stale entries exist");

        // Prune remaining with large batch
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (_, _, has_more) = tree_writer.prune_stale_nodes_batch(5, 100_000).unwrap();
        txn.commit().unwrap();
        assert!(!has_more, "Expected has_more=false after draining all stale entries");
    }

    #[test]
    fn test_jmt_prune_respects_retention_boundary() {
        // Pruning with prune_below_version=V should NOT remove nodes stale at version V or above.
        let db = TempDatabase::new();

        let mut roots = Vec::new();
        for v in 0u64..4 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("val_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer.write_stale_node_index_batch(&updates.stale_node_index_batch).unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        // Prune below version 2 – versions 2 and 3 roots should still work
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.prune_stale_nodes(2).unwrap();
        txn.commit().unwrap();

        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        // Versions at and above the boundary must be intact
        assert_eq!(jmt.get_root_hash(2).unwrap(), roots[2]);
        assert_eq!(jmt.get_root_hash(3).unwrap(), roots[3]);
    }

    #[test]
    fn test_jmt_root_hash_unchanged_after_pruning() {
        // The latest root hash must be identical before and after pruning stale nodes.
        let db = TempDatabase::new();

        let mut latest_root = jmt::RootHash([0u8; 32]);
        for v in 0u64..5 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("data_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer.write_stale_node_index_batch(&updates.stale_node_index_batch).unwrap();
            txn.commit().unwrap();
            latest_root = root;
        }

        // Prune everything below version 5
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (nodes_deleted, _) = tree_writer.prune_stale_nodes(5).unwrap();
        txn.commit().unwrap();
        assert!(nodes_deleted > 0);

        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let root_after = jmt.get_root_hash(4).unwrap();
        assert_eq!(latest_root, root_after, "Root hash at the latest version must not change after pruning");
    }

    #[test]
    fn test_jmt_reorg_within_retention_window_after_pruning() {
        // Simulate: versions 0-4 are built, stale entries below version 2 are pruned,
        // then a reorg happens at version 4 (within the retention window).
        // The reorg at the tip should succeed because its nodes weren't pruned.
        let db = TempDatabase::new();

        let mut keys = Vec::new();
        let mut roots = Vec::new();
        for v in 0u64..5 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            keys.push(smt_key);
            let value = format!("v{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer.write_stale_node_index_batch(&updates.stale_node_index_batch).unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        // Prune stale entries below version 2 (old history)
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.prune_stale_nodes(2).unwrap();
        txn.commit().unwrap();

        // Reorg: delete version 4 (within retention window, not pruned)
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.delete_all_for_version(4).unwrap();
        txn.commit().unwrap();

        // Root at version 3 must still be valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let root3_after = jmt.get_root_hash(3).unwrap();
        assert_eq!(roots[3], root3_after, "Root at version 3 must survive after reorg of version 4");
    }

    #[test]
    fn test_jmt_prune_empty_db_is_noop() {
        let db = TempDatabase::new();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (deleted, removed) = tree_writer.prune_stale_nodes(100).unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(removed, 0);
        txn.commit().unwrap();

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (deleted, removed, has_more) = tree_writer.prune_stale_nodes_batch(100, 10).unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(removed, 0);
        assert!(!has_more);
        txn.commit().unwrap();
    }

    #[test]
    fn test_jmt_prune_idempotent() {
        // Pruning twice with the same version should be a no-op the second time.
        let db = TempDatabase::new();

        for v in 0u64..3 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (_, updates) = jmt.put_value_set(vec![(smt_key, Some(vec![v as u8]))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer.write_stale_node_index_batch(&updates.stale_node_index_batch).unwrap();
            txn.commit().unwrap();
        }

        // First prune
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (del1, rem1) = tree_writer.prune_stale_nodes(3).unwrap();
        txn.commit().unwrap();

        // Second prune with same version – should delete nothing
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (del2, rem2) = tree_writer.prune_stale_nodes(3).unwrap();
        txn.commit().unwrap();
        assert_eq!(del2, 0, "Second prune should delete no nodes");
        assert_eq!(rem2, 0, "Second prune should remove no index entries");
        assert!(del1 > 0, "First prune should have deleted something");
        assert!(rem1 > 0, "First prune should have removed stale entries");
    }

    #[test]
    fn test_jmt_value_readable_after_prune() {
        // After pruning, the latest value for a key must still be readable.
        let db = TempDatabase::new();

        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));

        // Version 0: insert value
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_, u0) = jmt.put_value_set(vec![(smt_key, Some(b"original".to_vec()))], 0).unwrap();
        tree_writer.write_node_batch(&u0.node_batch).unwrap();
        tree_writer.write_stale_node_index_batch(&u0.stale_node_index_batch).unwrap();
        txn.commit().unwrap();

        // Version 1: update value (makes version-0 nodes stale)
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_, u1) = jmt.put_value_set(vec![(smt_key, None)], 1).unwrap();
        tree_writer.write_node_batch(&u1.node_batch).unwrap();
        tree_writer.write_stale_node_index_batch(&u1.stale_node_index_batch).unwrap();
        txn.commit().unwrap();

        // Version 2: re-insert with new value
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (root2, u2) = jmt.put_value_set(vec![(smt_key, Some(b"updated".to_vec()))], 2).unwrap();
        tree_writer.write_node_batch(&u2.node_batch).unwrap();
        tree_writer.write_stale_node_index_batch(&u2.stale_node_index_batch).unwrap();
        txn.commit().unwrap();

        // Prune all stale below version 2
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.prune_stale_nodes(2).unwrap();
        txn.commit().unwrap();

        // Verify root and value at version 2 still valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(jmt.get_root_hash(2).unwrap(), root2);
        let (val, _proof) = jmt.get_with_proof(smt_key, 2).unwrap();
        assert_eq!(val, Some(b"updated".to_vec()));
    }

    #[test]
    fn test_jmt_benchmark_storage_reduction_after_pruning() {
        // Benchmark: measure before/after storage size to verify ≥50% reduction from pruning.
        //
        // Simulates a realistic workload: many versions updating the same keys, which produces
        // a large number of stale (historical) nodes. After pruning, only the latest nodes remain.
        let db = TempDatabase::new();

        let keys_per_version = 20;
        let num_versions = 50u64;

        // Build up JMT over many versions, inserting new distinct keys each version.
        // This creates a large tree with many stale internal nodes as the tree shape changes.
        for v in 0..num_versions {
            let entries: Vec<(KeyHash, Option<Vec<u8>>)> = (0..keys_per_version)
                .map(|_| {
                    let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
                    let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
                    (smt_key, Some(format!("value_v{v}").into_bytes()))
                })
                .collect();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (_root, updates) = jmt.put_value_set(entries, v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer.write_stale_node_index_batch(&updates.stale_node_index_batch).unwrap();
            txn.commit().unwrap();
        }

        // Measure BEFORE pruning
        let nodes_before = db.db().jmt_node_entry_count();
        let size_before = db.db().jmt_node_total_size();
        let stale_entries = db.db().jmt_stale_index_entry_count();

        assert!(nodes_before > 0, "Expected nodes in jmt_node_data before pruning");
        assert!(stale_entries > 0, "Expected stale index entries before pruning");

        // Prune everything below the latest version (keep only latest)
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (nodes_deleted, index_removed) = tree_writer.prune_stale_nodes(num_versions).unwrap();
        txn.commit().unwrap();

        // Measure AFTER pruning
        let nodes_after = db.db().jmt_node_entry_count();
        let size_after = db.db().jmt_node_total_size();
        let stale_after = db.db().jmt_stale_index_entry_count();

        // Verify root hash is still valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt_check = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let _root = jmt_check.get_root_hash(num_versions - 1).unwrap();

        let reduction_pct = if size_before > 0 {
            ((size_before - size_after) as f64 / size_before as f64) * 100.0
        } else {
            0.0
        };

        println!("=== JMT Pruning Benchmark ===");
        println!("Versions: {num_versions}, Keys per version: {keys_per_version}");
        println!("BEFORE pruning:");
        println!("  jmt_node_data entries: {nodes_before}");
        println!("  jmt_node_data total size: {size_before} bytes ({:.2} KB)", size_before as f64 / 1024.0);
        println!("  jmt_stale_node_index entries: {stale_entries}");
        println!("AFTER pruning:");
        println!("  jmt_node_data entries: {nodes_after}");
        println!("  jmt_node_data total size: {size_after} bytes ({:.2} KB)", size_after as f64 / 1024.0);
        println!("  jmt_stale_node_index entries: {stale_after}");
        println!("RESULTS:");
        println!("  Nodes deleted: {nodes_deleted}");
        println!("  Index entries removed: {index_removed}");
        println!("  Node count reduction: {nodes_before} -> {nodes_after} ({:.1}%)", (1.0 - nodes_after as f64 / nodes_before as f64) * 100.0);
        println!("  Storage reduction: {reduction_pct:.1}%");
        println!("=============================");

        // Acceptance criteria: pruning reduces storage by at least 50%
        assert!(
            reduction_pct >= 50.0,
            "Expected ≥50% storage reduction, got {reduction_pct:.1}% (before={size_before}, after={size_after})"
        );
        assert!(nodes_deleted > 0, "Expected nodes to be deleted");
        assert_eq!(stale_after, 0, "All stale index entries should be removed after full prune");
    }

    #[test]
    fn test_jmt_prune_stale_nodes() {
        let db = TempDatabase::new();

        // Version 0: insert key1
        let (_sk, commitment1) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key1 = KeyHash(commitment1.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value1 = b"value1".to_vec();

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_root0, updates0) = jmt.put_value_set(vec![(smt_key1, Some(value1.clone()))], 0).unwrap();
        tree_writer.write_node_batch(&updates0.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&updates0.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Version 1: insert key2 (makes some version-0 nodes stale)
        let (_sk, commitment2) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key2 = KeyHash(commitment2.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value2 = b"value2".to_vec();

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_root1, updates1) = jmt.put_value_set(vec![(smt_key2, Some(value2.clone()))], 1).unwrap();
        tree_writer.write_node_batch(&updates1.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&updates1.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Version 2: insert key3 (makes some version-1 nodes stale)
        let (_sk, commitment3) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key3 = KeyHash(commitment3.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value3 = b"value3".to_vec();

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (root2, updates2) = jmt.put_value_set(vec![(smt_key3, Some(value3.clone()))], 2).unwrap();
        tree_writer.write_node_batch(&updates2.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&updates2.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        let total_stale = updates0.stale_node_index_batch.len()
            + updates1.stale_node_index_batch.len()
            + updates2.stale_node_index_batch.len();
        assert!(total_stale > 0, "Expected some stale nodes to prune");

        // Prune stale nodes with stale_since_version < 2
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (nodes_deleted, index_entries_removed) = tree_writer.prune_stale_nodes(2).unwrap();
        txn.commit().unwrap();

        assert!(nodes_deleted > 0, "Expected some nodes to be deleted");
        assert!(
            index_entries_removed > 0,
            "Expected some stale index entries to be removed"
        );

        // Verify the latest root is still valid (current state intact)
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let root2_after_prune = jmt.get_root_hash(2).unwrap();
        assert_eq!(root2, root2_after_prune, "Root hash at version 2 should be unchanged after pruning");
    }
}
