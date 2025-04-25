use std::ops::Deref;

use jmt::storage::TreeReader;
use lmdb_zero::{ConstTransaction, ReadTransaction};
use tari_storage::lmdb_store::DatabaseRef;

use crate::chain_storage::lmdb_db::lmdb::lmdb_get;

pub struct LmdbTreeReader<'a> {
    txn: &'a ConstTransaction<'a>,
    node_db: DatabaseRef,
    node_table_name: &'static str,
    value_db: DatabaseRef,
    value_table_name: &'static str,
}

impl<'a> LmdbTreeReader<'a> {
    pub fn new<T: Deref<Target = ConstTransaction<'a>>>(
        txn: &'a T,
        node_db: DatabaseRef,
        node_table_name: &'static str,
        value_db: DatabaseRef,
        value_table_name: &'static str,
    ) -> Self {
        Self {
            txn: txn.deref(),
            node_db,
            node_table_name,
            value_db,
            value_table_name,
        }
    }
}

impl<'a> TreeReader for LmdbTreeReader<'a> {
    fn get_node_option(&self, node_key: &jmt::storage::NodeKey) -> anyhow::Result<Option<jmt::storage::Node>> {
        let mut lmdb_key: Vec<u8> = vec![];
        lmdb_key.extend_from_slice(&node_key.version().to_be_bytes());
        borsh::BorshSerialize::serialize(&node_key.nibble_path(), &mut lmdb_key)?;
        // dbg!(&lmdb_key);
        // dbg!(&node_key);
        let node = lmdb_get(&self.txn, &self.node_db, &lmdb_key)?;
        // dbg!(&node);
        Ok(node)
    }

    fn get_value_option(
        &self,
        max_version: jmt::Version,
        key_hash: jmt::KeyHash,
    ) -> anyhow::Result<Option<jmt::OwnedValue>> {
        todo!()
        // TODO: implement after saving
        // Ok(None)
    }

    fn get_rightmost_leaf(&self) -> anyhow::Result<Option<(jmt::storage::NodeKey, jmt::storage::LeafNode)>> {
        todo!()
        // Ok(None)
    }
}

pub struct OwnedLmdbTreeReader<'a> {
    txn: ReadTransaction<'a>,
    node_db: DatabaseRef,
    node_table_name: &'static str,
    value_db: DatabaseRef,
    value_table_name: &'static str,
}

impl<'a> OwnedLmdbTreeReader<'a> {
    pub fn new(
        txn: ReadTransaction<'a>,
        node_db: DatabaseRef,
        node_table_name: &'static str,
        value_db: DatabaseRef,
        value_table_name: &'static str,
    ) -> Self {
        Self {
            txn,
            node_db,
            node_table_name,
            value_db,
            value_table_name,
        }
    }
}

impl<'a> TreeReader for OwnedLmdbTreeReader<'a> {
    fn get_node_option(&self, node_key: &jmt::storage::NodeKey) -> anyhow::Result<Option<jmt::storage::Node>> {
        let inner = LmdbTreeReader::new(
            &self.txn,
            self.node_db.clone(),
            self.node_table_name,
            self.value_db.clone(),
            self.value_table_name,
        );
        inner.get_node_option(node_key)
    }

    fn get_value_option(
        &self,
        max_version: jmt::Version,
        key_hash: jmt::KeyHash,
    ) -> anyhow::Result<Option<jmt::OwnedValue>> {
        todo!()
    }

    fn get_rightmost_leaf(&self) -> anyhow::Result<Option<(jmt::storage::NodeKey, jmt::storage::LeafNode)>> {
        todo!()
    }
}
