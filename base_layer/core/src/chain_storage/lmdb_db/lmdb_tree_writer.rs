use jmt::storage::TreeWriter;
use lmdb_zero::WriteTransaction;

use crate::chain_storage::DbTransaction;
pub(crate) struct LmdbTreeWriter<'a> {
    txn: &'a WriteTransaction<'a>,
}

impl<'a> LmdbTreeWriter<'a> {
    pub fn new(txn: &'a WriteTransaction<'a>) -> Self {
        Self { txn }
    }

    pub fn delete_all_for_version(&self, version: u64) -> anyhow::Result<()> {
        todo!("implement delete all for version")
    }
}

impl<'a> TreeWriter for LmdbTreeWriter<'a> {
    fn write_node_batch(&self, node_batch: &jmt::storage::NodeBatch) -> anyhow::Result<()> {
        todo!()
    }
}
