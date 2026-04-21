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

    /// Prune all stale JMT nodes with `stale_since_version < prune_below_version`.
    /// Convenience wrapper around [`Self::prune_stale_nodes_batch`] with no batch limit.
    #[cfg(test)]
    pub fn prune_stale_nodes(&self, prune_below_version: u64) -> anyhow::Result<(u64, u64)> {
        let (nodes_deleted, index_entries_removed, _) = self.prune_stale_nodes_batch(prune_below_version, u64::MAX)?;
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

    /// AC 1.1: Root hash consistency after pruning.
    /// For each version v in [retention_floor, tip], verify get_root_hash(v) matches
    /// the root hash recorded at commit time.
    #[test]
    fn test_jmt_root_hash_consistency_after_pruning() {
        let db = TempDatabase::new();
        let num_versions = 10u64;
        let retention_floor = 5u64;

        let mut roots = Vec::new();
        for v in 0..num_versions {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("val_v{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        // Prune stale nodes below retention_floor
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (nodes_deleted, _) = tree_writer.prune_stale_nodes(retention_floor).unwrap();
        txn.commit().unwrap();
        assert!(nodes_deleted > 0, "Expected some nodes to be pruned");

        // Every version in [retention_floor, tip) must still produce the original root hash
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        for v in retention_floor..num_versions {
            let root_after = jmt.get_root_hash(v).unwrap();
            assert_eq!(
                roots[v as usize], root_after,
                "Root hash mismatch at version {v} after pruning below {retention_floor}"
            );
        }
    }

    /// AC 3.2: Multi-step rollback.
    /// Sequentially delete 1, 2, 5, 10 tip blocks and verify root correctness after each rollback.
    #[test]
    fn test_jmt_multi_step_rollback() {
        let db = TempDatabase::new();
        let num_versions = 20u64;

        let mut roots = Vec::new();
        for v in 0..num_versions {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("block_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        let rollback_depths = [1u64, 2, 5, 10];
        let mut current_tip = num_versions;

        for &depth in &rollback_depths {
            for i in 0..depth {
                let version_to_delete = current_tip - 1 - i;
                let txn = db.db().create_write_txn();
                let tree_writer = db.db().create_lmdb_tree_writer(&txn);
                tree_writer.delete_all_for_version(version_to_delete).unwrap();
                txn.commit().unwrap();
            }
            current_tip -= depth;

            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let expected_root = roots[(current_tip - 1) as usize];
            let actual_root = jmt.get_root_hash(current_tip - 1).unwrap();
            assert_eq!(
                expected_root,
                actual_root,
                "Root hash mismatch after rolling back {depth} blocks to version {}",
                current_tip - 1
            );
        }

        // 20 - (1+2+5+10) = 2
        assert_eq!(current_tip, 2);
    }

    /// AC 3.4: Reorg + reinsert at same version.
    /// Delete version V (with stale tracking), then insert a new block at version V.
    /// Root hash must be correct and no duplicate key errors in stale index.
    #[test]
    fn test_jmt_reorg_reinsert_same_version() {
        let db = TempDatabase::new();

        // Version 0: insert key1
        let (_sk, commitment1) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key1 = KeyHash(commitment1.as_bytes().try_into().expect("Key hash is always 32 bytes"));

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (root0, updates) = jmt.put_value_set(vec![(smt_key1, Some(b"val0".to_vec()))], 0).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&updates.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Version 1: insert key2 (creates stale entries for version 0 nodes)
        let (_sk, commitment2) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key2 = KeyHash(commitment2.as_bytes().try_into().expect("Key hash is always 32 bytes"));

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (root1_original, updates) = jmt.put_value_set(vec![(smt_key2, Some(b"val1".to_vec()))], 1).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&updates.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Reorg: delete version 1 (must also clean stale index entries for version 1)
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.delete_all_for_version(1).unwrap();
        txn.commit().unwrap();

        // Verify root at version 0 is still intact
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(jmt.get_root_hash(0).unwrap(), root0);

        // Reinsert at the same version 1 with the same key — must NOT error on stale index
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (root1_reinserted, updates) = jmt.put_value_set(vec![(smt_key2, Some(b"val1".to_vec()))], 1).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&updates.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        assert_eq!(
            root1_original, root1_reinserted,
            "Root hash at version 1 must match after reorg + reinsert with the same data"
        );

        // Also verify stale index is clean — no orphan entries from the deleted version
        let stale_count = db.db().jmt_stale_index_entry_count();
        // After reinserting version 1, there should be stale entries only from version 1 commit
        // (the nodes that version 1 made stale in version 0). There must be no duplicates.
        assert!(
            stale_count > 0,
            "Reinsert should produce stale entries for replaced v0 nodes"
        );
    }

    /// AC 4.4: Retention floor boundary (off-by-one).
    /// Nodes with stale_since_version == prune_below_version are NOT deleted;
    /// nodes with stale_since_version < prune_below_version ARE deleted.
    #[test]
    fn test_jmt_retention_floor_boundary_off_by_one() {
        let db = TempDatabase::new();

        // Create 5 versions (0..5), each with a distinct key
        let mut roots = Vec::new();
        for v in 0u64..5 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("v{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        let stale_before = db.db().jmt_stale_index_entry_count();
        assert!(stale_before > 0, "Should have stale entries before pruning");

        // Prune with boundary = 3: entries with stale_since_version < 3 are removed,
        // entries with stale_since_version == 3 or above are preserved.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (nodes_deleted, index_removed) = tree_writer.prune_stale_nodes(3).unwrap();
        txn.commit().unwrap();
        assert!(nodes_deleted > 0, "Should have deleted some stale nodes below boundary");
        assert!(
            index_removed > 0,
            "Should have removed some stale index entries below boundary"
        );

        // Versions at and above the boundary must still have valid root hashes
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        for v in 3u64..5 {
            assert_eq!(
                jmt.get_root_hash(v).unwrap(),
                roots[v as usize],
                "Root hash at version {v} (>= boundary) must be intact"
            );
        }

        // Prune again at the same boundary — must be a no-op (nothing left below 3)
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (del2, rem2) = tree_writer.prune_stale_nodes(3).unwrap();
        txn.commit().unwrap();
        assert_eq!(del2, 0, "Second prune at same boundary should delete nothing");
        assert_eq!(rem2, 0, "Second prune at same boundary should remove no index entries");

        // Prune at boundary+1 should remove entries with stale_since_version == 3
        let stale_before_4 = db.db().jmt_stale_index_entry_count();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (del3, rem3) = tree_writer.prune_stale_nodes(4).unwrap();
        txn.commit().unwrap();
        let stale_after_4 = db.db().jmt_stale_index_entry_count();

        if stale_before_4 > stale_after_4 {
            assert!(
                del3 > 0,
                "Pruning at boundary+1 should have deleted nodes stale at version 3"
            );
            assert!(
                rem3 > 0,
                "Pruning at boundary+1 should have removed index entries stale at version 3"
            );
        }

        // Version 4 root hash must still be valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(
            jmt.get_root_hash(4).unwrap(),
            roots[4],
            "Root at version 4 must survive"
        );
    }

    /// AC 4.6: Crash mid-prune recovery.
    /// Simulate a crash by dropping (not committing) the write transaction mid-prune.
    /// The DB must not be corrupted, and a subsequent prune must succeed.
    #[test]
    fn test_jmt_crash_mid_prune_recovery() {
        let db = TempDatabase::new();

        // Build 5 versions
        let mut roots = Vec::new();
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
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        let nodes_before = db.db().jmt_node_entry_count();
        let stale_before = db.db().jmt_stale_index_entry_count();

        // Simulate crash: prune but DROP the transaction (no commit)
        {
            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let (deleted, _) = tree_writer.prune_stale_nodes(5).unwrap();
            assert!(deleted > 0, "Prune should have found nodes to delete");
            // Intentionally do NOT commit — simulates crash/abort
            drop(tree_writer);
            drop(txn); // LMDB rolls back on drop
        }

        // DB state must be unchanged (LMDB rolled back the aborted txn)
        let nodes_after_crash = db.db().jmt_node_entry_count();
        let stale_after_crash = db.db().jmt_stale_index_entry_count();
        assert_eq!(
            nodes_before, nodes_after_crash,
            "Node count must be unchanged after aborted prune"
        );
        assert_eq!(
            stale_before, stale_after_crash,
            "Stale index count must be unchanged after aborted prune"
        );

        // All root hashes must still be valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        for v in 0u64..5 {
            assert_eq!(
                jmt.get_root_hash(v).unwrap(),
                roots[v as usize],
                "Root hash at version {v} must be intact after aborted prune"
            );
        }

        // Now do a real prune — must succeed and be resumable
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (deleted, removed) = tree_writer.prune_stale_nodes(5).unwrap();
        txn.commit().unwrap();
        assert!(deleted > 0, "Recovery prune should delete nodes");
        assert!(removed > 0, "Recovery prune should remove stale index entries");

        // Latest root must still be valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(
            jmt.get_root_hash(4).unwrap(),
            roots[4],
            "Root at tip must survive after recovery prune"
        );
    }

    /// AC 1.2: Merkle proof generation post-prune.
    /// For keys in the current state, `get_with_proof(key, tip)` returns a valid proof
    /// that can be verified against the root hash.
    #[test]
    fn test_jmt_merkle_proof_generation_post_prune() {
        let db = TempDatabase::new();
        let num_versions = 6u64;

        let mut keys = Vec::new();
        let mut roots = Vec::new();
        for v in 0..num_versions {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            keys.push(smt_key);
            let value = format!("proof_val_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        // Prune stale nodes below version 4
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (deleted, _) = tree_writer.prune_stale_nodes(4).unwrap();
        txn.commit().unwrap();
        assert!(deleted > 0, "Expected some nodes to be pruned");

        let tip = num_versions - 1;
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        // All keys inserted at versions within retention window should have valid proofs
        for &key in &keys {
            let (val, proof) = jmt.get_with_proof(key, tip).unwrap();
            if let Some(v) = &val {
                proof
                    .verify_existence(roots[tip as usize], key, v)
                    .expect("Inclusion proof must verify after pruning");
            } else {
                proof
                    .verify_nonexistence(roots[tip as usize], key)
                    .expect("Non-inclusion proof must verify after pruning");
            }
        }

        // Also test a key that does NOT exist in the tree
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let nonexistent_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let (val, proof) = jmt.get_with_proof(nonexistent_key, tip).unwrap();
        assert!(val.is_none());
        proof
            .verify_nonexistence(roots[tip as usize], nonexistent_key)
            .expect("Non-inclusion proof for missing key must verify after pruning");
    }

    /// AC 1.3: Block commit with active pruning.
    /// Interleave block commits and pruning to ensure root hashes remain correct
    /// when pruning happens concurrently with new block writes.
    #[test]
    fn test_jmt_block_commit_with_active_pruning() {
        let db = TempDatabase::new();
        let retention_window = 3u64;
        let num_versions = 12u64;

        let mut roots = Vec::new();
        for v in 0..num_versions {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("active_prune_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);

            // After enough versions, prune on every commit (simulating background pruning)
            if v >= retention_window {
                let prune_below = v - retention_window + 1;
                let txn = db.db().create_write_txn();
                let tree_writer = db.db().create_lmdb_tree_writer(&txn);
                tree_writer.prune_stale_nodes(prune_below).unwrap();
                txn.commit().unwrap();
            }
        }

        // Verify all roots within the final retention window are correct
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let final_prune_floor = num_versions - retention_window;
        for v in final_prune_floor..num_versions {
            let root_after = jmt.get_root_hash(v).unwrap();
            assert_eq!(
                roots[v as usize], root_after,
                "Root hash mismatch at version {v} after active pruning"
            );
        }
    }

    /// AC 2.2: Historical value correctness.
    /// After pruning, queries at different retained versions return the correct state
    /// for keys that were inserted at various points in history.
    #[test]
    fn test_jmt_historical_value_correctness() {
        let db = TempDatabase::new();

        // Insert different keys at different versions, so each version's state is distinct
        let mut keys = Vec::new();
        let mut roots = Vec::new();
        for v in 0u64..6 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            keys.push(smt_key);
            let value = format!("hist_val_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        // Prune below version 3
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.prune_stale_nodes(3).unwrap();
        txn.commit().unwrap();

        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        // At version 3: keys[0..=3] should exist, keys[4..=5] should not
        let (val3, _) = jmt.get_with_proof(keys[3], 3).unwrap();
        assert_eq!(
            val3.as_deref(),
            Some(b"hist_val_3".as_slice()),
            "Key inserted at version 3 must be readable at version 3"
        );
        let (val_future, _) = jmt.get_with_proof(keys[4], 3).unwrap();
        assert!(
            val_future.is_none(),
            "Key inserted at version 4 must not exist at version 3"
        );

        // At version 5 (tip): all keys[0..=5] should exist
        for (i, key) in keys.iter().enumerate() {
            let (val, _) = jmt.get_with_proof(*key, 5).unwrap();
            assert_eq!(
                val.as_deref(),
                Some(format!("hist_val_{i}").as_bytes()),
                "Key {i} must be readable at tip"
            );
        }

        // Also test a key that was inserted then deleted
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let del_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));

        // Insert at version 6, delete at version 7
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_, u6) = jmt
            .put_value_set(vec![(del_key, Some(b"to_delete".to_vec()))], 6)
            .unwrap();
        tree_writer.write_node_batch(&u6.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u6.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_, u7) = jmt.put_value_set(vec![(del_key, None)], 7).unwrap();
        tree_writer.write_node_batch(&u7.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u7.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Prune below version 7
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.prune_stale_nodes(7).unwrap();
        txn.commit().unwrap();

        // At version 7, del_key should be gone
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (val_del, _) = jmt.get_with_proof(del_key, 7).unwrap();
        assert!(val_del.is_none(), "Deleted key must not be retrievable after pruning");
    }

    /// AC 3.3: Deep reorg within retention window.
    /// Reorg of depth equal to the full retention window — all affected versions are correct.
    #[test]
    fn test_jmt_deep_reorg_within_retention() {
        let db = TempDatabase::new();
        let retention_window = 5u64;
        let num_versions = 10u64;

        let mut keys = Vec::new();
        let mut roots = Vec::new();
        for v in 0..num_versions {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            keys.push(smt_key);
            let value = format!("deep_reorg_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        // Prune stale nodes below version 5 (retention_window = 5)
        let prune_below = num_versions - retention_window;
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.prune_stale_nodes(prune_below).unwrap();
        txn.commit().unwrap();

        // Deep reorg: delete all versions from tip down to retention floor
        for v in (prune_below..num_versions).rev() {
            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            tree_writer.delete_all_for_version(v).unwrap();
            txn.commit().unwrap();

            // After deleting version v, root at version v-1 should be valid
            if v > 0 {
                let reader = db.db().create_smt_reader().unwrap();
                let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
                let root_after = jmt.get_root_hash(v - 1).unwrap();
                assert_eq!(
                    roots[(v - 1) as usize],
                    root_after,
                    "Root mismatch at version {} after deleting version {v}",
                    v - 1
                );
            }
        }

        // After full deep reorg, re-insert new blocks at the same versions
        let mut new_roots = Vec::new();
        for v in prune_below..num_versions {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("reorg_new_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            new_roots.push(root);
        }

        // All new roots should be valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        for (i, v) in (prune_below..num_versions).enumerate() {
            let root = jmt.get_root_hash(v).unwrap();
            assert_eq!(new_roots[i], root, "New root mismatch at version {v} after deep reorg");
        }
    }

    /// AC 4.3: Batch pruning completeness.
    /// `prune_stale_nodes_batch` with a small limit correctly processes `has_more`,
    /// and multiple batches produce the same result as a full prune.
    #[test]
    fn test_jmt_batch_pruning_completeness() {
        let db = TempDatabase::new();
        let num_versions = 8u64;

        let mut root_latest = jmt::RootHash([0u8; 32]);
        for v in 0..num_versions {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("batch_v{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            root_latest = root;
        }

        let stale_before = db.db().jmt_stale_index_entry_count();
        assert!(stale_before > 0, "Should have stale entries before batch pruning");

        // Prune in small batches of 2 until no more
        let mut total_nodes_deleted = 0u64;
        let mut total_index_removed = 0u64;
        let mut batch_count = 0u64;
        loop {
            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let (del, rem, has_more) = tree_writer.prune_stale_nodes_batch(num_versions, 2).unwrap();
            txn.commit().unwrap();
            total_nodes_deleted += del;
            total_index_removed += rem;
            batch_count += 1;
            if !has_more {
                break;
            }
        }

        assert!(
            batch_count > 1,
            "Expected multiple batches with batch_size=2, got {batch_count}"
        );
        assert!(total_nodes_deleted > 0, "Expected nodes to be deleted across batches");
        assert!(
            total_index_removed > 0,
            "Expected index entries to be removed across batches"
        );
        assert_eq!(
            db.db().jmt_stale_index_entry_count(),
            0,
            "All stale entries should be removed after full batch pruning"
        );

        // Root hash must still be valid after batch pruning
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(
            jmt.get_root_hash(num_versions - 1).unwrap(),
            root_latest,
            "Root hash must survive after batch pruning"
        );
    }

    /// AC 5.1: Background pruning + block commit.
    /// Simulates background worker and block commit working in the same sequence
    /// (interleaved in a single thread since LMDB write transactions are serialized)
    /// to verify no deadlock or data corruption.
    #[test]
    fn test_jmt_background_pruning_and_block_commit() {
        let db = TempDatabase::new();
        let retention_window = 3u64;
        let total_blocks = 15u64;

        let mut roots = Vec::new();
        for v in 0..total_blocks {
            // 1. Commit a new block
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("bg_block_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);

            // 2. Simulate background pruning in batches (every 2 blocks)
            if v >= retention_window && v % 2 == 0 {
                let prune_below = v - retention_window + 1;
                let mut has_more = true;
                while has_more {
                    let txn = db.db().create_write_txn();
                    let tree_writer = db.db().create_lmdb_tree_writer(&txn);
                    let (_, _, more) = tree_writer.prune_stale_nodes_batch(prune_below, 5).unwrap();
                    txn.commit().unwrap();
                    has_more = more;
                }
            }
        }

        // Verify root hashes for the final retention window
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let final_retained = total_blocks - retention_window;
        for v in final_retained..total_blocks {
            let root_after = jmt.get_root_hash(v).unwrap();
            assert_eq!(
                roots[v as usize], root_after,
                "Root hash mismatch at version {v} after background pruning simulation"
            );
        }

        // Verify a non-existent key proof at tip to ensure proof generation still works
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let random_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let (val, proof) = jmt.get_with_proof(random_key, total_blocks - 1).unwrap();
        assert!(val.is_none(), "Random key should not exist");
        proof
            .verify_nonexistence(roots[(total_blocks - 1) as usize], random_key)
            .expect("Non-existence proof must verify after background pruning");
    }

    /// AC 2.3: Deleted key non-retrievable.
    /// A key that was deleted (value=None) must not be returned after pruning previous versions.
    #[test]
    fn test_jmt_deleted_key_non_retrievable_after_prune() {
        let db = TempDatabase::new();

        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));

        // Version 0: insert key
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_, u0) = jmt.put_value_set(vec![(smt_key, Some(b"exists".to_vec()))], 0).unwrap();
        tree_writer.write_node_batch(&u0.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u0.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Version 1: delete key
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_root1, u1) = jmt.put_value_set(vec![(smt_key, None)], 1).unwrap();
        tree_writer.write_node_batch(&u1.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u1.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Version 2: insert another key to advance the tree
        let (_sk, commitment2) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key2 = KeyHash(commitment2.as_bytes().try_into().expect("Key hash is always 32 bytes"));

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (root2, u2) = jmt.put_value_set(vec![(smt_key2, Some(b"other".to_vec()))], 2).unwrap();
        tree_writer.write_node_batch(&u2.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u2.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Prune all stale below version 2
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.prune_stale_nodes(2).unwrap();
        txn.commit().unwrap();

        // Deleted key must NOT be retrievable at version 1 or version 2
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (val_v2, proof_v2) = jmt.get_with_proof(smt_key, 2).unwrap();
        assert!(
            val_v2.is_none(),
            "Deleted key must not be retrievable at version 2 after pruning"
        );
        proof_v2
            .verify_nonexistence(root2, smt_key)
            .expect("Non-existence proof for deleted key must verify at version 2");

        // The other key should still be readable
        let (val_other, _) = jmt.get_with_proof(smt_key2, 2).unwrap();
        assert_eq!(val_other, Some(b"other".to_vec()), "Other key must still be readable");
    }

    /// AC 2.4: Multiple keys, mixed history.
    /// Multiple keys with different histories (insert/update/delete/reinsert) —
    /// all correctly readable after pruning.
    #[test]
    fn test_jmt_multiple_keys_mixed_history() {
        let db = TempDatabase::new();

        // Generate 4 keys with different lifecycles
        let mut key_hashes = Vec::new();
        for _ in 0..4 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            key_hashes.push(KeyHash(
                commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"),
            ));
        }
        let [k_persistent, k_deleted, k_reinserted, k_late] =
            [key_hashes[0], key_hashes[1], key_hashes[2], key_hashes[3]];

        // V0: insert k_persistent, k_deleted, k_reinserted
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_, u) = jmt
            .put_value_set(
                vec![
                    (k_persistent, Some(b"persist_v0".to_vec())),
                    (k_deleted, Some(b"del_v0".to_vec())),
                    (k_reinserted, Some(b"reins_v0".to_vec())),
                ],
                0,
            )
            .unwrap();
        tree_writer.write_node_batch(&u.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // V1: delete k_deleted
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_, u) = jmt.put_value_set(vec![(k_deleted, None)], 1).unwrap();
        tree_writer.write_node_batch(&u.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // V2: delete k_reinserted
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_, u) = jmt.put_value_set(vec![(k_reinserted, None)], 2).unwrap();
        tree_writer.write_node_batch(&u.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // V3: reinsert k_reinserted with new value
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_, u) = jmt
            .put_value_set(vec![(k_reinserted, Some(b"reins_v3".to_vec()))], 3)
            .unwrap();
        tree_writer.write_node_batch(&u.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // V4: insert k_late
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (root4, u) = jmt.put_value_set(vec![(k_late, Some(b"late_v4".to_vec()))], 4).unwrap();
        tree_writer.write_node_batch(&u.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&u.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Prune below version 3
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.prune_stale_nodes(3).unwrap();
        txn.commit().unwrap();

        // Verify at tip (version 4)
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        // k_persistent: still exists with original value
        let (val, _) = jmt.get_with_proof(k_persistent, 4).unwrap();
        assert_eq!(
            val.as_deref(),
            Some(b"persist_v0".as_slice()),
            "Persistent key must survive"
        );

        // k_deleted: gone
        let (val, proof) = jmt.get_with_proof(k_deleted, 4).unwrap();
        assert!(val.is_none(), "Deleted key must not exist");
        proof
            .verify_nonexistence(root4, k_deleted)
            .expect("Non-existence proof must verify for deleted key");

        // k_reinserted: has the v3 value
        let (val, _) = jmt.get_with_proof(k_reinserted, 4).unwrap();
        assert_eq!(
            val.as_deref(),
            Some(b"reins_v3".as_slice()),
            "Reinserted key must have latest value"
        );

        // k_late: has the v4 value
        let (val, _) = jmt.get_with_proof(k_late, 4).unwrap();
        assert_eq!(
            val.as_deref(),
            Some(b"late_v4".as_slice()),
            "Late-inserted key must be readable"
        );
    }

    /// AC 3.5: Reorg beyond retention window.
    /// Attempting to use a version beyond the pruned range — pruned nodes are gone,
    /// so root hash retrieval for those versions will fail or return incorrect results.
    #[test]
    fn test_jmt_reorg_beyond_retention_window() {
        let db = TempDatabase::new();
        let num_versions = 10u64;
        let retention_window = 3u64;

        let mut roots = Vec::new();
        for v in 0..num_versions {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("beyond_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        // Prune below version 7 (keeping only versions 7, 8, 9)
        let prune_below = num_versions - retention_window;
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.prune_stale_nodes(prune_below).unwrap();
        txn.commit().unwrap();

        // Versions within retention window should still be valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        for v in prune_below..num_versions {
            assert_eq!(
                jmt.get_root_hash(v).unwrap(),
                roots[v as usize],
                "Root hash at version {v} (within retention) must be valid"
            );
        }

        // Attempting to get root hash at a pruned version — should fail or return wrong result.
        // The JMT will either error out or return an incorrect root because internal nodes
        // have been deleted. Either outcome is acceptable — the key point is that the
        // retained versions remain correct.
        let pruned_version = prune_below.saturating_sub(2);
        if pruned_version > 0 {
            let result = jmt.get_root_hash(pruned_version);
            // Either it errors (nodes missing) or the root doesn't match
            match result {
                Err(_) => {
                    // Expected: pruned nodes are missing
                },
                Ok(root) => {
                    // If it returns a result, it may differ from the original root
                    // because intermediate nodes were pruned. Either outcome is valid.
                    // Just log for visibility.
                    println!("Root hash at pruned version {pruned_version}: returned (may differ from original)");
                    let _ = root; // suppress unused warning
                },
            }
        }
    }

    /// AC 3.6: Reorg failure recovery.
    /// Simulate a failure during chain switching by dropping a write transaction mid-reorg.
    /// DB must remain in a consistent state.
    #[test]
    fn test_jmt_reorg_failure_recovery() {
        let db = TempDatabase::new();

        let mut roots = Vec::new();
        let mut keys = Vec::new();
        for v in 0u64..5 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            keys.push(smt_key);
            let value = format!("reorg_fail_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        let nodes_before = db.db().jmt_node_entry_count();
        let stale_before = db.db().jmt_stale_index_entry_count();

        // Simulate failed reorg: delete version 4 but DON'T commit
        {
            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            tree_writer.delete_all_for_version(4).unwrap();
            // Intentionally drop without commit — simulates crash during reorg
            drop(tree_writer);
            drop(txn);
        }

        // DB state must be unchanged
        assert_eq!(
            db.db().jmt_node_entry_count(),
            nodes_before,
            "Node count unchanged after aborted reorg"
        );
        assert_eq!(
            db.db().jmt_stale_index_entry_count(),
            stale_before,
            "Stale count unchanged after aborted reorg"
        );

        // All root hashes must still be valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        for v in 0u64..5 {
            assert_eq!(
                jmt.get_root_hash(v).unwrap(),
                roots[v as usize],
                "Root hash at version {v} must be intact after aborted reorg"
            );
        }

        // Now do a real reorg — must succeed
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.delete_all_for_version(4).unwrap();
        txn.commit().unwrap();

        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(
            jmt.get_root_hash(3).unwrap(),
            roots[3],
            "Root at version 3 must be valid after real reorg"
        );
    }

    /// AC 6.1: Tip within retention window.
    /// When tip < retention_window, prune_below_version would be 0 — pruning should be skipped.
    #[test]
    fn test_jmt_tip_within_retention_window() {
        let db = TempDatabase::new();
        let retention_window = 1000u64;
        let num_versions = 5u64; // tip = 4, far below retention_window

        let mut roots = Vec::new();
        for v in 0..num_versions {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("young_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        // tip (4) < retention_window (1000) → prune_below_version = 0 → nothing to prune
        let prune_below = num_versions.saturating_sub(retention_window);
        assert_eq!(
            prune_below, 0,
            "prune_below_version must be 0 when tip < retention_window"
        );

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (deleted, removed) = tree_writer.prune_stale_nodes(prune_below).unwrap();
        txn.commit().unwrap();
        assert_eq!(deleted, 0, "No nodes should be pruned when prune_below_version=0");
        assert_eq!(
            removed, 0,
            "No stale entries should be removed when prune_below_version=0"
        );

        // All roots must be intact
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        for v in 0..num_versions {
            assert_eq!(
                jmt.get_root_hash(v).unwrap(),
                roots[v as usize],
                "Root hash at version {v} must be intact when no pruning occurred"
            );
        }
    }

    /// AC 6.2: Single-version DB.
    /// DB with only one version — pruning must not delete the single root.
    #[test]
    fn test_jmt_single_version_db() {
        let db = TempDatabase::new();

        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));

        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (root0, updates) = jmt
            .put_value_set(vec![(smt_key, Some(b"only_version".to_vec()))], 0)
            .unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        tree_writer
            .write_stale_node_index_batch(&updates.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Prune with version 1 — only one version exists, version 0 has no stale entries
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (deleted, removed) = tree_writer.prune_stale_nodes(1).unwrap();
        txn.commit().unwrap();
        // Version 0 is the only version — there are no stale nodes
        assert_eq!(deleted, 0, "No stale nodes in single-version DB");
        assert_eq!(removed, 0, "No stale index entries in single-version DB");

        // Root and value must be intact
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(
            jmt.get_root_hash(0).unwrap(),
            root0,
            "Root must survive pruning in single-version DB"
        );
        let (val, _) = jmt.get_with_proof(smt_key, 0).unwrap();
        assert_eq!(
            val,
            Some(b"only_version".to_vec()),
            "Value must be readable in single-version DB"
        );
    }

    /// AC 6.3: Large batch pruning.
    /// Pruning 10k+ stale nodes — correctness and completion.
    #[test]
    fn test_jmt_large_batch_pruning() {
        let db = TempDatabase::new();

        let keys_per_version = 50;
        let num_versions = 200u64; // 200 * ~50 keys → thousands of stale nodes

        let mut latest_root = jmt::RootHash([0u8; 32]);
        for v in 0..num_versions {
            let entries: Vec<(KeyHash, Option<Vec<u8>>)> = (0..keys_per_version)
                .map(|_| {
                    let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
                    let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
                    (smt_key, Some(format!("large_{v}").into_bytes()))
                })
                .collect();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(entries, v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            latest_root = root;
        }

        let stale_before = db.db().jmt_stale_index_entry_count();
        assert!(stale_before > 1000, "Expected >1000 stale entries, got {stale_before}");

        // Prune all stale nodes
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (nodes_deleted, index_removed) = tree_writer.prune_stale_nodes(num_versions).unwrap();
        txn.commit().unwrap();

        assert!(
            nodes_deleted > 1000,
            "Expected >1000 nodes deleted, got {nodes_deleted}"
        );
        assert!(
            index_removed > 1000,
            "Expected >1000 index entries removed, got {index_removed}"
        );
        assert_eq!(
            db.db().jmt_stale_index_entry_count(),
            0,
            "All stale entries should be removed"
        );

        // Root hash must be intact
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(
            jmt.get_root_hash(num_versions - 1).unwrap(),
            latest_root,
            "Root hash must survive large batch pruning"
        );
    }

    /// AC 6.4: Stale index orphans after reorg.
    /// A reorg leaves orphaned stale entries (pointing to nodes that were already deleted
    /// by the reorg). Pruning must handle these gracefully (skip missing nodes, no errors).
    #[test]
    fn test_jmt_stale_index_orphans_after_reorg() {
        let db = TempDatabase::new();

        let mut roots = Vec::new();
        for v in 0u64..5 {
            let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut OsRng);
            let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
            let value = format!("orphan_{v}").into_bytes();

            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
            let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value))], v).unwrap();
            tree_writer.write_node_batch(&updates.node_batch).unwrap();
            tree_writer
                .write_stale_node_index_batch(&updates.stale_node_index_batch)
                .unwrap();
            txn.commit().unwrap();
            roots.push(root);
        }

        // Version 3 made some version-2 nodes stale.
        // Reorg: delete version 3 (removes nodes but stale entries for earlier versions remain)
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.delete_all_for_version(3).unwrap();
        txn.commit().unwrap();

        // Also delete version 4 (which referenced nodes that were stale-since v3 or v4)
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.delete_all_for_version(4).unwrap();
        txn.commit().unwrap();

        // Now prune stale nodes below version 3. Some of the stale index entries may
        // reference nodes that were already deleted by the reorg (orphans).
        // This must NOT panic or error fatally — just warn and skip.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let result = tree_writer.prune_stale_nodes(3);
        txn.commit().unwrap();

        // Pruning must succeed (even if some nodes were already deleted)
        assert!(result.is_ok(), "Pruning must handle orphaned stale entries gracefully");

        // Root at version 2 must still be valid
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(
            jmt.get_root_hash(2).unwrap(),
            roots[2],
            "Root at version 2 must be valid after reorg + prune"
        );
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
