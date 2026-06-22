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

use jmt::storage::{StaleNodeIndexBatch, TreeWriter};
use lmdb_zero::WriteTransaction;
use log::*;
use tari_storage::lmdb_store::DatabaseRef;
use tari_utilities::hex::Hex;

use super::lmdb::{lmdb_get, lmdb_insert, lmdb_replace};
use crate::chain_storage::{
    Optional,
    lmdb_db::{
        lmdb::lmdb_delete,
        lmdb_db::{MetadataKey, MetadataValue},
    },
};

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_tree_writer";

pub(crate) struct LmdbTreeWriter<'a> {
    txn: &'a WriteTransaction<'a>,
    node_db: DatabaseRef,
    value_db: DatabaseRef,
    metabase_db: DatabaseRef,
}

impl<'a> LmdbTreeWriter<'a> {
    pub fn new(
        txn: &'a WriteTransaction<'a>,
        node_db: DatabaseRef,
        value_db: DatabaseRef,
        metabase_db: DatabaseRef,
    ) -> Self {
        Self {
            txn,
            node_db,
            value_db,
            metabase_db,
        }
    }

    pub fn cleanup_stale(&self, stale: &StaleNodeIndexBatch) -> anyhow::Result<()> {
        for index in stale {
            let mut lmdb_key: Vec<u8> = vec![];
            borsh::BorshSerialize::serialize(&index.node_key, &mut lmdb_key)?;
            lmdb_delete(self.txn, &self.node_db, &lmdb_key, "jmt_node_table").optional()?;
        }
        Ok(())
    }
}

impl TreeWriter for LmdbTreeWriter<'_> {
    fn write_node_batch(&self, node_batch: &jmt::storage::NodeBatch) -> anyhow::Result<()> {
        for (node_key, node) in node_batch.nodes() {
            let mut lmdb_key: Vec<u8> = vec![];
            borsh::BorshSerialize::serialize(node_key, &mut lmdb_key)?;
            match node {
                jmt::storage::Node::Null => {
                    trace!(target: LOG_TARGET, "Deleting node with key {}", lmdb_key.to_hex());
                    lmdb_delete(self.txn, &self.node_db, &lmdb_key, "jmt_node_table").optional()?;
                },
                _ => {
                    lmdb_delete(self.txn, &self.node_db, &lmdb_key, "jmt_node_table").optional()?;
                    lmdb_insert(self.txn, &self.node_db, &lmdb_key, &node, "jmt_node_table")?;
                },
            }
        }
        for (value_key, value) in node_batch.values() {
            let mut lmdb_key: Vec<u8> = vec![];
            lmdb_key.extend_from_slice(&value_key.1.0);
            match value {
                Some(_v) => {
                    let existing: Option<Vec<u8>> = lmdb_get(self.txn, &self.value_db, &lmdb_key)?;
                    if existing.is_some() {
                        warn!(target: LOG_TARGET, "Found existing unique key {} for version {}", value_key.1 .0.to_hex(), value_key.0);
                        return Err(anyhow::anyhow!("Duplicate value key found in batch"));
                    }
                    let val_bytes = bincode::serialize(value)?;

                    lmdb_insert(self.txn, &self.value_db, &lmdb_key, &val_bytes, "jmt_value_table")?;
                },
                None => {
                    lmdb_delete(self.txn, &self.value_db, &lmdb_key, "jmt_value_table").optional()?;
                },
            }
        }
        let k = MetadataKey::JMTVersion;
        let written_version = node_batch.nodes().keys().map(|node_key| node_key.version()).max();
        if let Some(written_version) = written_version {
            // Invariant: the version we wrote at must be exactly one past the previously-saved
            // version (0 for the first write). A mismatch means JMTVersion had desynced from the
            // persisted tree before this write.
            let current = match lmdb_get(self.txn, &self.metabase_db, &k.as_u32())? {
                Some(MetadataValue::JMTVersion(v)) => Some(v),
                _ => None,
            };
            let expected = current.map(|v| v + 1).unwrap_or(0);
            if written_version != expected {
                warn!(
                    target: LOG_TARGET,
                    "JMTVersion DESYNC: writing JMT nodes at version {written_version} but the saved version is \
                     {current:?} (expected to write at {expected}). The persisted tree and JMTVersion were out of step."
                );
            }
            lmdb_replace(
                self.txn,
                &self.metabase_db,
                &k.as_u32(),
                &MetadataValue::JMTVersion(written_version),
                None,
            )?;
        }

        trace!(target: LOG_TARGET, "Wrote JMT batch of {} nodes and {} values", node_batch.nodes().len(), node_batch.values().len());
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

    #[test]
    fn test_jmt_does_not_accept_duplicates() {
        let db = TempDatabase::new();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let (reader, _current_version) = db.db().create_smt_reader().unwrap();

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value = b"test_value".to_vec();
        let (_root, updates) = jmt.put_value_set(vec![(smt_key, Some(value.clone()))], 0).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();

        txn.commit().unwrap();
        // Try again for new version.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap().0;

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (_root, update2) = jmt.put_value_set(vec![(smt_key, Some(value))], 1).unwrap();
        assert!(
            tree_writer.write_node_batch(&update2.node_batch).is_err(),
            "Duplicate key error expected"
        );
    }

    #[test]
    fn test_jmt_insert_delete() {
        let db = TempDatabase::new();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap().0;

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let smt_key_0 = KeyHash(vec![0; 32].try_into().expect("Key hash is always 32 bytes"));
        let value_0 = smt_key_0.0.to_vec();
        let smt_key_1 = KeyHash(vec![1; 32].try_into().expect("Key hash is always 32 bytes"));
        let value_1 = smt_key_1.0.to_vec();
        let smt_key_2 = KeyHash(vec![2; 32].try_into().expect("Key hash is always 32 bytes"));
        let value_2 = smt_key_2.0.to_vec();

        let (_root, updates) = jmt.put_value_set(vec![(smt_key_0, Some(value_0.clone()))], 0).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        txn.commit().unwrap();
        // Try again for new version.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap().0;

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (root1, updates) = jmt.put_value_set(vec![(smt_key_1, Some(value_1.clone()))], 1).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        tree_writer.cleanup_stale(&updates.stale_node_index_batch).unwrap();
        txn.commit().unwrap();

        // // Try again for new version.
        let reader = db.db().create_smt_reader().unwrap().0;

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(jmt.get_leaf_count(1).unwrap(), 2);
        assert!(jmt.get(smt_key_0, 1).unwrap().is_some());
        assert!(jmt.get(smt_key_1, 1).unwrap().is_some());
        // Try again for new version.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap().0;

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (_root, updates) = jmt
            .put_value_set(vec![(smt_key_2, Some(value_2.clone())), (smt_key_0, None)], 2)
            .unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        tree_writer.cleanup_stale(&updates.stale_node_index_batch).unwrap();
        txn.commit().unwrap();

        let reader = db.db().create_smt_reader().unwrap().0;

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert_eq!(jmt.get_leaf_count(2).unwrap(), 2);
        assert!(jmt.get(smt_key_0, 5).unwrap().is_none());
        assert!(jmt.get(smt_key_1, 5).unwrap().is_some());
        assert!(jmt.get(smt_key_2, 5).unwrap().is_some());

        // Try again for new version.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap().0;
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (root2, updates) = jmt
            .put_value_set(vec![(smt_key_2, None), (smt_key_0, Some(value_0))], 3)
            .unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();
        tree_writer.cleanup_stale(&updates.stale_node_index_batch).unwrap();
        txn.commit().unwrap();

        let reader = db.db().create_smt_reader().unwrap().0;
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert!(jmt.get(smt_key_0, 3).unwrap().is_some());
        assert!(jmt.get(smt_key_1, 3).unwrap().is_some());
        assert!(jmt.get(smt_key_2, 3).unwrap().is_none());
        assert_eq!(jmt.get_leaf_count(3).unwrap(), 2);
        assert_eq!(root1, root2);
    }

    #[test]
    fn test_jmt_does_accept_duplicate_if_deleted() {
        // If a key in the jmt is deleted, it can be added later.
        let db = TempDatabase::new();
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap().0;

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value = b"test_value".to_vec();
        let (_root, updates) = jmt.put_value_set(vec![(smt_key, Some(value.clone()))], 0).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();

        txn.commit().unwrap();
        // Try again for new version.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap().0;

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (_root, update2) = jmt.put_value_set(vec![(smt_key, None)], 1).unwrap();
        tree_writer.write_node_batch(&update2.node_batch).unwrap();

        txn.commit().unwrap();

        // Try again for version 2.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap().0;

        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

        let (_root, update2) = jmt.put_value_set(vec![(smt_key, Some(value))], 2).unwrap();
        tree_writer.write_node_batch(&update2.node_batch).unwrap();

        txn.commit().unwrap();
    }
}
