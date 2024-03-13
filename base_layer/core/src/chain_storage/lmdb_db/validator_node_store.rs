//  Copyright 2022. The Tari Project
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

use std::{collections::HashMap, ops::Deref};

use lmdb_zero::{ConstTransaction, WriteTransaction};
use tari_common_types::types::{Commitment, PublicKey};
use tari_storage::lmdb_store::DatabaseRef;
use tari_utilities::ByteArray;

use crate::chain_storage::{
    lmdb_db::{
        composite_key::CompositeKey,
        cursors::{FromKeyBytes, LmdbReadCursor},
        lmdb::{lmdb_delete, lmdb_insert},
    },
    ChainStorageError,
    ValidatorNodeEntry,
};

pub type ShardKey = [u8; 32];
// <h, pk, output_hash>
type ValidatorNodeStoreKey = CompositeKey<72>;
// <pk, h, output_hash>
type ShardIdIndexKey = CompositeKey<72>;

pub struct ValidatorNodeStore<'a, Txn> {
    txn: &'a Txn,
    db_validator_nodes: DatabaseRef,
    db_validator_nodes_mapping: DatabaseRef,
}

impl<'a, Txn: Deref<Target = ConstTransaction<'a>>> ValidatorNodeStore<'a, Txn> {
    pub fn new(txn: &'a Txn, db_height_to_vn: DatabaseRef, idx_public_key_to_shard: DatabaseRef) -> Self {
        Self {
            txn,
            db_validator_nodes: db_height_to_vn,
            db_validator_nodes_mapping: idx_public_key_to_shard,
        }
    }
}

impl ValidatorNodeStore<'_, WriteTransaction<'_>> {
    pub fn insert(&self, height: u64, validator: &ValidatorNodeEntry) -> Result<(), ChainStorageError> {
        let key = ValidatorNodeStoreKey::try_from_parts(&[
            height.to_be_bytes().as_slice(),
            validator.public_key.as_bytes(),
            validator.commitment.as_bytes(),
        ])
        .expect("insert: Composite key length is incorrect");
        lmdb_insert(self.txn, &self.db_validator_nodes, &key, &validator, "Validator node")?;

        let key = ShardIdIndexKey::try_from_parts(&[
            validator.public_key.as_bytes(),
            height.to_be_bytes().as_slice(),
            validator.commitment.as_bytes(),
        ])
        .expect("insert: Composite key length is incorrect");
        lmdb_insert(
            self.txn,
            &self.db_validator_nodes_mapping,
            &key,
            &validator.shard_key,
            "Validator node",
        )?;
        Ok(())
    }

    pub fn delete(
        &self,
        height: u64,
        public_key: &PublicKey,
        commitment: &Commitment,
    ) -> Result<(), ChainStorageError> {
        let key = ValidatorNodeStoreKey::try_from_parts(&[
            height.to_be_bytes().as_slice(),
            public_key.as_bytes(),
            commitment.as_bytes(),
        ])
        .expect("delete: Composite key length is incorrect");
        lmdb_delete(self.txn, &self.db_validator_nodes, &key, "validator_nodes")?;

        let key = ShardIdIndexKey::try_from_parts(&[
            public_key.as_bytes(),
            height.to_be_bytes().as_slice(),
            commitment.as_bytes(),
        ])
        .expect("delete: Composite key length is incorrect");
        lmdb_delete(
            self.txn,
            &self.db_validator_nodes_mapping,
            &key,
            "validator_nodes_mapping",
        )?;
        Ok(())
    }
}

impl<'a, Txn: Deref<Target = ConstTransaction<'a>>> ValidatorNodeStore<'a, Txn> {
    fn db_read_cursor(&self) -> Result<LmdbReadCursor<'a, ValidatorNodeEntry>, ChainStorageError> {
        let cursor = self.txn.cursor(self.db_validator_nodes.clone())?;
        let access = self.txn.access();
        let cursor = LmdbReadCursor::new(cursor, access);
        Ok(cursor)
    }

    fn index_read_cursor(&self) -> Result<LmdbReadCursor<'a, ShardKey>, ChainStorageError> {
        let cursor = self.txn.cursor(self.db_validator_nodes_mapping.clone())?;
        let access = self.txn.access();
        let cursor = LmdbReadCursor::new(cursor, access);
        Ok(cursor)
    }

    /// Returns a set of <public key, shard id> tuples ordered by height of registration.
    /// This set contains no duplicates. If a duplicate registration is found, the last registration is included.
    pub fn get_vn_set(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<(PublicKey, Option<PublicKey>, ShardKey)>, ChainStorageError> {
        let mut cursor = self.db_read_cursor()?;

        let mut nodes = Vec::new();
        // Public key does not mutate once compressed and will always produce the same hash
        #[allow(clippy::mutable_key_type)]
        let mut dedup_map = HashMap::new();
        match cursor.seek_range::<ValidatorNodeStoreKey>(&start_height.to_be_bytes())? {
            Some((key, vn)) => {
                let height = u64::from_key_bytes(&key[0..8])?;
                if height > end_height {
                    return Ok(Vec::new());
                }
                dedup_map.insert(vn.public_key.clone(), 0);
                nodes.push(Some((vn.public_key, vn.validator_network, vn.shard_key)));
            },
            None => return Ok(Vec::new()),
        }

        // Start from index 1 because we already have the first entry
        let mut i = 1;
        while let Some((key, vn)) = cursor.next_dup::<ValidatorNodeStoreKey>()? {
            let height = u64::from_key_bytes(&key[0..8])?;
            if height > end_height {
                break;
            }
            if let Some(dup_idx) = dedup_map.insert(vn.public_key.clone(), i) {
                // Remove duplicate registrations within the set without changing index order
                let node_mut = nodes
                    .get_mut(dup_idx)
                    .expect("get_vn_set: internal dedeup map is not in sync with nodes");
                *node_mut = None;
            }
            nodes.push(Some((vn.public_key, vn.validator_network, vn.shard_key)));
            i += 1;
        }

        let mut vn_set = nodes.into_iter().flatten().collect::<Vec<_>>();
        vn_set.sort_by(|(_, a, c), (_, b, d)| a.cmp(b).then(c.cmp(d)));
        Ok(vn_set)
    }

    pub fn get_shard_key(
        &self,
        start_height: u64,
        end_height: u64,
        public_key: &PublicKey,
    ) -> Result<Option<ShardKey>, ChainStorageError> {
        let mut cursor = self.index_read_cursor()?;
        let key = ShardIdIndexKey::try_from_parts(&[public_key.as_bytes(), &start_height.to_be_bytes()])
            .expect("fetch_shard_key: Composite key length is incorrect");

        // Find the first entry at or above start_height
        let mut shard_key = match cursor.seek_range::<ShardIdIndexKey>(key.as_bytes())? {
            Some((key, s)) => {
                if key[0..32] != *public_key.as_bytes() {
                    return Ok(None);
                }
                let height = u64::from_key_bytes(&key[32..40])?;
                if height > end_height {
                    return Ok(None);
                }
                Some(s)
            },
            None => return Ok(None),
        };

        // If there are any subsequent entries less than the end height, use that instead.
        while let Some((key, s)) = cursor.next::<ShardIdIndexKey>()? {
            if key[0..32] != *public_key.as_bytes() {
                break;
            }
            let height = u64::from_key_bytes(&key[32..40])?;
            if height > end_height {
                break;
            }
            shard_key = Some(s);
        }
        Ok(shard_key)
    }
}

#[cfg(test)]
mod tests {
    use tari_test_utils::unpack_enum;

    use super::*;
    use crate::{
        chain_storage::tests::temp_db::TempLmdbDatabase,
        test_helpers::{make_hash, new_public_key},
    };

    const DBS: &[&str] = &["validator_node_store", "validator_node_index"];

    fn create_store<'a, Txn: Deref<Target = ConstTransaction<'a>>>(
        db: &TempLmdbDatabase,
        txn: &'a Txn,
    ) -> ValidatorNodeStore<'a, Txn> {
        let store_db = db.get_db(DBS[0]).clone();
        let index_db = db.get_db(DBS[1]).clone();
        ValidatorNodeStore::new(txn, store_db, index_db)
    }

    fn insert_n_vns(
        store: &ValidatorNodeStore<'_, WriteTransaction<'_>>,
        start_height: u64,
        n: usize,
    ) -> Vec<(PublicKey, ShardKey)> {
        let mut nodes = Vec::new();
        for i in 0..n {
            let public_key = new_public_key();
            let shard_key = make_hash(public_key.as_bytes());
            store
                .insert(start_height + i as u64, &ValidatorNodeEntry {
                    public_key: public_key.clone(),
                    shard_key,
                    commitment: Commitment::from_public_key(&new_public_key()),
                    ..Default::default()
                })
                .unwrap();
            nodes.push((public_key, shard_key));
        }
        nodes.sort_by(|(_, a), (_, b)| a.cmp(b));
        nodes
    }

    mod insert {
        use super::*;

        #[test]
        fn it_inserts_validator_nodes() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 3);
            let set = store.get_vn_set(1, 3).unwrap();
            assert_eq!(set[0], nodes[0]);
            assert_eq!(set[1], nodes[1]);
            assert_eq!(set[2], nodes[2]);
        }

        #[test]
        fn it_does_not_allow_duplicate_entries() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let p1 = new_public_key();
            let entry = ValidatorNodeEntry {
                shard_key: make_hash(p1.as_bytes()),
                public_key: p1,
                commitment: Commitment::from_public_key(&new_public_key()),
                ..Default::default()
            };
            store.insert(1, &entry).unwrap();
            let err = store.insert(1, &entry).unwrap_err();
            unpack_enum!(ChainStorageError::KeyExists { .. } = err);
        }
    }

    mod get_vn_set {
        use super::*;

        #[test]
        fn it_returns_a_deduped_set_of_validator_nodes() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 3);
            // Node 0 and 1 re-register at height 4

            let s0 = make_hash(nodes[0].1);
            store
                .insert(4, &ValidatorNodeEntry {
                    public_key: nodes[0].0.clone(),
                    shard_key: s0,
                    commitment: Commitment::from_public_key(&new_public_key()),
                    ..Default::default()
                })
                .unwrap();

            let s1 = make_hash(nodes[1].1);
            // The commitment is used last in the key and so changes the order they appear in the LMDB btree.
            // We insert them in reverse order to demonstrate that insert order does not necessarily match the vn set
            // order.
            let mut ordered_commitments = vec![
                Commitment::from_public_key(&new_public_key()),
                Commitment::from_public_key(&new_public_key()),
            ];
            ordered_commitments.sort();
            store
                .insert(5, &ValidatorNodeEntry {
                    public_key: nodes[1].0.clone(),
                    shard_key: make_hash(s1),
                    commitment: ordered_commitments[1].clone(),
                    ..Default::default()
                })
                .unwrap();
            // This insert is counted as before the previous one because the commitment is "less"
            store
                .insert(5, &ValidatorNodeEntry {
                    public_key: nodes[1].0.clone(),
                    shard_key: s1,
                    commitment: ordered_commitments[0].clone(),
                    ..Default::default()
                })
                .unwrap();

            let set = store.get_vn_set(1, 5).unwrap();
            // s1 and s2 have replaced the previous shard keys, and are now ordered last since they come after node2
            assert_eq!(set.len(), 3);
            assert_eq!(set.iter().filter(|s| s.0 == nodes[1].0).count(), 1);
        }
    }

    mod get_shard_key {
        use super::*;

        #[test]
        fn it_returns_latest_shard_key() {
            let db = TempLmdbDatabase::with_dbs(DBS);
            let txn = db.write_transaction();
            let store = create_store(&db, &txn);
            let nodes = insert_n_vns(&store, 1, 3);
            let new_shard_key = make_hash(nodes[0].1);
            store
                .insert(4, &ValidatorNodeEntry {
                    public_key: nodes[0].0.clone(),
                    shard_key: new_shard_key,
                    commitment: Commitment::from_public_key(&new_public_key()),

                    ..Default::default()
                })
                .unwrap();

            // Height 0-3 has original shard key
            let s = store.get_shard_key(0, 3, &nodes[0].0).unwrap().unwrap();
            assert_eq!(s, nodes[0].1);
            // Height 0-4 has shard key that was replaced at height 4
            let s = store.get_shard_key(0, 4, &nodes[0].0).unwrap().unwrap();
            assert_eq!(s, new_shard_key);
            assert!(store.get_shard_key(5, 5, &nodes[0].0).unwrap().is_none());
            let s = store.get_shard_key(0, 3, &nodes[1].0).unwrap().unwrap();
            assert_eq!(s, nodes[1].1);
            let s = store.get_shard_key(0, 3, &nodes[2].0).unwrap().unwrap();
            assert_eq!(s, nodes[2].1);
        }
    }
}
