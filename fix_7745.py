// base_layer/core/src/chain_storage/lmdb_db/jmt_serializer.rs

use lmdb_zero::{Database, RwTransaction, WriteFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
struct JmtNode {
    key: String,
    value: String,
    children: HashMap<String, JmtNode>,
}

impl JmtNode {
    fn prune(&mut self, depth: usize, max_depth: usize) {
        if depth >= max_depth {
            self.children.clear();
        } else {
            for child in self.children.values_mut() {
                child.prune(depth + 1, max_depth);
            }
        }
    }

    fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn deserialize(data: &[u8]) -> Self {
        bincode::deserialize(data).unwrap()
    }
}

pub fn optimize_jmt_data(db: &Database, txn: &mut RwTransaction, max_depth: usize) {
    let mut jmt_data = HashMap::new();
    let mut cursor = txn.open_ro_cursor(db).unwrap();

    while let Some((key, value)) = cursor.get::<&[u8], &[u8]>(None) {
        let mut node = JmtNode::deserialize(value);
        node.prune(0, max_depth);
        jmt_data.insert(key.to_vec(), node.serialize());
    }

    cursor.close();

    txn.clear(db).unwrap();
    for (key, value) in jmt_data {
        txn.put(db, &key, &value, WriteFlags::empty()).unwrap();
    }
}