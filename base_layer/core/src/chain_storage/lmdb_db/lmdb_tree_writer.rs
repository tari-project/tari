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

use jmt::storage::{Node, TreeWriter};
use lmdb_zero::WriteTransaction;
use log::{info, warn};
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
}

impl<'a> LmdbTreeWriter<'a> {
    pub fn new(
        txn: &'a WriteTransaction<'a>,
        node_db: DatabaseRef,
        value_db: DatabaseRef,
        unique_key_db: DatabaseRef,
    ) -> Self {
        Self {
            txn,
            node_db,
            value_db,
            unique_key_db,
        }
    }

    pub fn delete_all_for_version(&self, version: u64) -> anyhow::Result<()> {
        let key = version.to_be_bytes();
        let nodes = lmdb_delete_keys_starting_with::<Node>(&self.txn, &self.node_db, &key)?;
        warn!(target: LOG_TARGET, "Deleted {} nodes for version {}", nodes.len(), version);
        let values = lmdb_delete_keys_starting_with::<Vec<u8>>(&self.txn, &self.value_db, &key)?;
        warn!(target: LOG_TARGET, "Deleted {} values for version {}", values.len(), version);

        for (value_key, _) in values {
            let mut lmdb_key: Vec<u8> = vec![];
            // version is first 8 bytes
            if value_key.len() < 8 {
                return Err(anyhow::anyhow!("Value key is too short"));
            }
            lmdb_key.extend_from_slice(&value_key[8..]);
            lmdb_key.extend_from_slice(&value_key[0..8]);
            match lmdb_delete(&self.txn, &self.unique_key_db, &lmdb_key, "jmt_unique_key_table") {
                Ok(_) => {
                    warn!(target: LOG_TARGET, "Deleted unique key {} for version {}", lmdb_key.to_hex(), version);
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "Failed to delete unique key {} for version {}: {}", lmdb_key.to_hex(), version, e);
                },
            }
        }

        Ok(())
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
            lmdb_key.extend_from_slice(&value_key.1 .0);
            let val_bytes = bincode::serialize(value)?;
            lmdb_insert(self.txn, &self.value_db, &lmdb_key, &val_bytes, "jmt_value_table")?;

            // see if there are any values already.
            let existing_values: Vec<(Vec<u8>, Option<Vec<u8>>)> =
                lmdb_fetch_matching_after(&self.txn, &self.unique_key_db, &value_key.1 .0)?;
            let mut existing_history = vec![];
            for (key, x) in existing_values {
                let version = u64::from_be_bytes(key[32..].try_into().unwrap());
                existing_history.push((version, x));
            }
            // sort by version
            existing_history.sort_by(|a, b| a.0.cmp(&b.0));

            let latest_value = existing_history.last().map(|x| x.1.clone()).flatten();
            match (value, &latest_value) {
                (None, _) => {
                    if latest_value.is_none() {
                        warn!(target: LOG_TARGET, "Found no existing JMT unique key for version {}, creating it as None", value_key.0);
                    }
                    let mut lmdb_key: Vec<u8> = vec![];
                    lmdb_key.extend_from_slice(value_key.1 .0.as_slice());
                    lmdb_key.extend_from_slice(&value_key.0.to_be_bytes());
                    lmdb_insert(self.txn, &self.unique_key_db, &lmdb_key, value, "jmt_unique_key_table")?;
                    // warn!(target: LOG_TARGET, "Deleted unique key {} effective from version {}", value_key.1
                    // .0.to_hex(), value_key.0);
                },
                (Some(_v), Some(_x)) => {
                    warn!(target: LOG_TARGET, "Found existing unique key {} for version {}", value_key.1 .0.to_hex(), value_key.0);
                    return Err(anyhow::anyhow!("Duplicate value key found in batch"));
                },
                // (None, None) => {
                // warn!(target: LOG_TARGET, "Found no existing unique key for version {}", value_key.0);
                // Technically this is allowed
                // return Err(anyhow::anyhow!("Duplicate value key found in batch"));
                // },
                (Some(_v), None) => {
                    let mut lmdb_key: Vec<u8> = vec![];
                    lmdb_key.extend_from_slice(value_key.1 .0.as_slice());
                    lmdb_key.extend_from_slice(&value_key.0.to_be_bytes());
                    lmdb_insert(self.txn, &self.unique_key_db, &lmdb_key, value, "jmt_unique_key_table")?;
                    // warn!(target: LOG_TARGET, "Inserted unique key {} for version {}", value_key.1 .0.to_hex(),
                    // value_key.0);
                },
            };
        }
        info!(target: LOG_TARGET, "Wrote JMT batch of {} nodes and {} values", node_batch.nodes().len(), node_batch.values().len());
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
}
