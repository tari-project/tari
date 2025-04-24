use jmt::storage::TreeWriter;
use lmdb_zero::{Database, WriteTransaction};
use log::info;
use tari_storage::lmdb_store::DatabaseRef;

use crate::chain_storage::lmdb_db::lmdb::lmdb_delete;

use super::lmdb::lmdb_insert;
pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_tree_writer";

pub(crate) struct LmdbTreeWriter<'a> {
    txn: &'a WriteTransaction<'a>,
    node_db: DatabaseRef,
    node_table_name: &'static str,
    value_db: DatabaseRef,
    value_table_name: &'static str,
}

impl<'a> LmdbTreeWriter<'a> {
    pub fn new(
        txn: &'a WriteTransaction<'a>,
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

    pub fn delete_all_for_version(&self, version: u64) -> anyhow::Result<()> {
        todo!("implement delete all for version")
    }
}

impl<'a> TreeWriter for LmdbTreeWriter<'a> {
    fn write_node_batch(&self, node_batch: &jmt::storage::NodeBatch) -> anyhow::Result<()> {
        for (node_key, node) in node_batch.nodes() {
            let mut lmdb_key: Vec<u8> = vec![];
            lmdb_key.extend_from_slice(&node_key.version().to_be_bytes());
            borsh::BorshSerialize::serialize(&node_key.nibble_path(), &mut lmdb_key)?;
            dbg!(&lmdb_key);
            dbg!(&node_key);
            dbg!(&node);
            // match node {
            //     jmt::storage::Node::Leaf(ref leaf) => {
            //         // let val_bytes = bincode::serialize(leaf)?;
            //         lmdb_insert(&self.txn, &self.node_db, &lmdb_key, &node, &self.node_table_name)?;
            //     },
            //     jmt::storage::Node::Internal(ref branch) => {
            //         // let val_bytes = bincode::serialize(branch)?;
            //         lmdb_insert(&self.txn, &self.node_db, &lmdb_key, &node, &self.node_table_name)?;
            //     },
            //     jmt::storage::Node::Null => {
            //         // delete
            //         // lmdb_delete(&self.txn, &self.node_db, &lmdb_key, &self.node_table_name)?;
            //     },
            // }
            // let val_bytes = bincode::serialize(node)?;
            // let val = lmdb_zero::Value::from(val_bytes);
            lmdb_insert(&self.txn, &self.node_db, &lmdb_key, &node, &self.node_table_name)?;
        }
        for (value_key, value) in node_batch.values() {
            let mut lmdb_key: Vec<u8> = vec![];
            lmdb_key.extend_from_slice(&value_key.0.to_be_bytes());
            lmdb_key.extend_from_slice(&value_key.1 .0);
            dbg!(value_key);
            dbg!(&lmdb_key);
            match value {
                Some(v) => {
                    let val_bytes = bincode::serialize(v)?;
                    lmdb_insert(&self.txn, &self.value_db, &lmdb_key, &val_bytes, &self.value_table_name)?;
                },
                None => {
                    todo!("delete value");
                    // lmdb_delete(txn, db, key, table_name);
                },
            };
        }
        info!(target: LOG_TARGET, "Wrote JMT batch of {} nodes and {} values", node_batch.nodes().len(), node_batch.values().len());
        Ok(())
    }
}
