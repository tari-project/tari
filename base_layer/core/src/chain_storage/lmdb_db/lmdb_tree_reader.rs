use jmt::storage::TreeReader;

pub struct LmdbTreeReader {}

impl LmdbTreeReader {
    pub fn new() -> Self {
        Self {}
    }
}

impl TreeReader for LmdbTreeReader {
    fn get_node_option(&self, node_key: &jmt::storage::NodeKey) -> anyhow::Result<Option<jmt::storage::Node>> {
        todo!()
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
