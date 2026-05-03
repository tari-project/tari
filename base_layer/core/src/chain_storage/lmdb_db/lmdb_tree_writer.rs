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
use lmdb_zero::WriteTransaction;
use log::*;
use tari_storage::lmdb_store::DatabaseRef;
use tari_utilities::hex::Hex;

use super::lmdb::lmdb_insert;
use crate::chain_storage::lmdb_db::lmdb::{lmdb_delete, lmdb_delete_keys_starting_with, lmdb_fetch_matching_after};
pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_tree_writer";

/// LMDB-backed JMT tree writer.
///
/// Honours the full `TreeUpdateBatch` returned by `JellyfishMerkleTree::put_value_set`:
///
/// * `node_batch`              — appended to `jmt_node_data` / `jmt_value_data` / `jmt_unique_key_data`.
/// * `stale_node_index_batch`  — recorded in `jmt_stale_node_data` for *deferred* deletion.
///
/// Prior to this change the stale-node batch was silently dropped, which is the root cause of the
/// unbounded growth in `jmt_node_data` reported in #7745.
///
/// Stale entries are not deleted immediately — that would break rewinds within the consensus reorg
/// horizon. Instead they are buffered keyed by `stale_since_version`, and only when their version
/// is finalised (i.e. older than the configured reorg buffer) does the buffered NodeKey get removed
/// from `jmt_node_data` and the buffer entry dropped.
pub(crate) struct LmdbTreeWriter<'a> {
    txn: &'a WriteTransaction<'a>,
    node_db: DatabaseRef,
    value_db: DatabaseRef,
    unique_key_db: DatabaseRef,
    stale_node_db: DatabaseRef,
}

impl<'a> LmdbTreeWriter<'a> {
    pub fn new(
        txn: &'a WriteTransaction<'a>,
        node_db: DatabaseRef,
        value_db: DatabaseRef,
        unique_key_db: DatabaseRef,
        stale_node_db: DatabaseRef,
    ) -> Self {
        Self {
            txn,
            node_db,
            value_db,
            unique_key_db,
            stale_node_db,
        }
    }

    /// Build the LMDB key used in `jmt_node_data` for a given JMT NodeKey.
    /// Format: `[version_be_bytes (8) || borsh_serialised_nibble_path]`.
    /// Capacity reserves space for the worst-case nibble path (64 nibbles + borsh length prefix)
    /// plus the 8-byte version, avoiding reallocations for any reachable NodeKey.
    fn lmdb_node_key(node_key: &jmt::storage::NodeKey) -> anyhow::Result<Vec<u8>> {
        let mut buf: Vec<u8> = Vec::with_capacity(8 + 64);
        buf.extend_from_slice(&node_key.version().to_be_bytes());
        borsh::BorshSerialize::serialize(&node_key.nibble_path(), &mut buf)?;
        Ok(buf)
    }

    /// Build the buffered-stale-entry key.
    ///
    /// Format: `[stale_since_version_be (8) || jmt_node_data lmdb key]`.
    ///
    /// The `stale_since_version_be` prefix lets us range-scan finalised entries efficiently; the
    /// suffix is the same key shape used in `jmt_node_data`, so we can strip the prefix and pass
    /// it straight back to LMDB delete.
    fn stale_lookup_key(stale_since_version: u64, node_key: &jmt::storage::NodeKey) -> anyhow::Result<Vec<u8>> {
        let node_lmdb_key = Self::lmdb_node_key(node_key)?;
        let mut buf: Vec<u8> = Vec::with_capacity(8 + node_lmdb_key.len());
        buf.extend_from_slice(&stale_since_version.to_be_bytes());
        buf.extend_from_slice(&node_lmdb_key);
        Ok(buf)
    }

    /// Delete every JMT node and value committed at the given version.
    ///
    /// Used by the rewind/reorg path. Also discards any buffered stale-node records that became
    /// stale *because of* this version: those nodes are still on disk (we deferred their deletion)
    /// and after the rewind they are reachable again from the previous tip.
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

        // Discard any buffered stale-node records that became stale *because of* this version.
        // The referenced nodes are still on disk (we deferred their deletion); after the rewind
        // they are reachable again from the previous tip and must NOT be pruned later.
        //
        // Custom loop because `lmdb_delete_keys_starting_with` deserialises values, but our
        // stale-node entries are keyed-only (empty values) - serde would error on EOF.
        let discarded = self.discard_buffered_stale_for_version(&key)?;
        if discarded > 0 {
            debug!(
                target: LOG_TARGET,
                "Discarded {} buffered stale-node records that became stale at version {} (rewind)",
                discarded,
                version
            );
        }

        Ok(())
    }

    /// Delete every entry in `jmt_stale_node_data` whose key starts with the given prefix.
    /// Returns the number deleted. Used by `delete_all_for_version` during rewinds.
    fn discard_buffered_stale_for_version(&self, version_be_prefix: &[u8]) -> anyhow::Result<usize> {
        let mut to_delete: Vec<Vec<u8>> = Vec::new();
        {
            let access = self.txn.access();
            let mut cursor = self
                .txn
                .cursor(&*self.stale_node_db)
                .map_err(|e| anyhow::anyhow!("stale-node cursor error during rewind: {e}"))?;
            let row = cursor.seek_range_k::<[u8], [u8]>(&access, version_be_prefix);
            let mut current = row.map(|r| (r.0.to_vec(), r.1.to_vec())).ok();
            while let Some((k, _v)) = current {
                if !k.starts_with(version_be_prefix) {
                    break;
                }
                to_delete.push(k);
                current = cursor
                    .next::<[u8], [u8]>(&access)
                    .map(|r| (r.0.to_vec(), r.1.to_vec()))
                    .ok();
            }
        }
        let mut access = self.txn.access();
        for k in &to_delete {
            access
                .del_key(&self.stale_node_db, k.as_slice())
                .map_err(|e| anyhow::anyhow!("failed to delete buffered stale-node record: {e}"))?;
        }
        Ok(to_delete.len())
    }

    pub fn put_node(&self, node_key: &jmt::storage::NodeKey, node: &jmt::storage::Node) -> anyhow::Result<()> {
        let lmdb_key = Self::lmdb_node_key(node_key)?;
        lmdb_insert(self.txn, &self.node_db, &lmdb_key, node, "jmt_node_table")?;
        Ok(())
    }

    /// Record JMT nodes that became stale in the most recent `put_value_set` call.
    ///
    /// Stale nodes are *not* deleted immediately; their deletion is deferred until
    /// `prune_stale_nodes_finalised_before` is called with a version threshold past the consensus
    /// reorg horizon. This makes rewinds within the reorg window lossless.
    pub fn record_stale_nodes(&self, stale_batch: &StaleNodeIndexBatch) -> anyhow::Result<()> {
        if stale_batch.is_empty() {
            return Ok(());
        }
        let mut access = self.txn.access();
        for stale in stale_batch {
            let key = Self::stale_lookup_key(stale.stale_since_version, &stale.node_key)?;
            // Empty value — the key carries everything we need. Use `lmdb_zero` access directly
            // because `lmdb_insert` requires a serialisable value.
            let empty: &[u8] = &[];
            match access.put::<[u8], [u8]>(&self.stale_node_db, key.as_slice(), empty, lmdb_zero::put::NOOVERWRITE) {
                Ok(()) => {},
                Err(lmdb_zero::Error::Code(lmdb_zero::error::KEYEXIST)) => {
                    // The same stale-node entry can re-appear across crash recovery boundaries
                    // (the BTreeSet of stale indices is deterministic for a given version).
                    // It's safe to silently accept duplicates.
                    trace!(
                        target: LOG_TARGET,
                        "Stale-node record already buffered at version {}",
                        stale.stale_since_version
                    );
                },
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to buffer stale-node record at version {}: {e}",
                        stale.stale_since_version
                    ));
                },
            }
        }
        trace!(
            target: LOG_TARGET,
            "Buffered {} stale-node records",
            stale_batch.len()
        );
        Ok(())
    }

    /// Delete buffered stale nodes whose `stale_since_version` is strictly less than `threshold`.
    ///
    /// Returns the number of nodes physically removed from `jmt_node_data`. The caller chooses
    /// `threshold = current_tip.saturating_sub(reorg_buffer)`, so any stale entry processed here
    /// is guaranteed to be older than the consensus reorg horizon and thus safe to delete.
    pub fn prune_stale_nodes_finalised_before(&self, threshold: u64) -> anyhow::Result<usize> {
        if threshold == 0 {
            return Ok(0);
        }

        // LMDB stores keys in lexicographic order, so a single forward cursor walk over
        // `jmt_stale_node_data` visits entries in order of `stale_since_version_be`. We can stop as
        // soon as we see a key whose 8-byte prefix is >= threshold_be.
        let threshold_be = threshold.to_be_bytes();

        // Collect the keys to delete first; then delete in a second pass to avoid mutating the
        // cursor we're iterating over.
        let mut to_delete: Vec<Vec<u8>> = Vec::new();
        {
            let access = self.txn.access();
            let mut cursor = self
                .txn
                .cursor(&*self.stale_node_db)
                .map_err(|e| anyhow::anyhow!("stale-node cursor error: {e}"))?;
            let mut row = cursor
                .first::<[u8], [u8]>(&access)
                .map(|r| (r.0.to_vec(), r.1.to_vec()))
                .ok();
            while let Some((k, _v)) = row {
                if k.len() < 8 {
                    return Err(anyhow::anyhow!("malformed stale-node key (length {})", k.len()));
                }
                // Compare first 8 bytes (BE-encoded `stale_since_version`) against threshold_be.
                let prefix: &[u8] = &k[..8];
                let threshold_slice: &[u8] = &threshold_be;
                if prefix >= threshold_slice {
                    break;
                }
                to_delete.push(k);
                row = cursor
                    .next::<[u8], [u8]>(&access)
                    .map(|r| (r.0.to_vec(), r.1.to_vec()))
                    .ok();
            }
        }

        let mut deleted = 0usize;
        let mut access = self.txn.access();
        for buffer_key in to_delete {
            // Strip the 8-byte stale_since_version prefix to recover the jmt_node_data key.
            let node_lmdb_key = &buffer_key[8..];
            // The buffered node may already be absent (for instance, a prior rewind+rebuild).
            // Tolerate NOTFOUND gracefully so pruning is idempotent.
            match access.del_key(&self.node_db, node_lmdb_key) {
                Ok(()) => {
                    deleted += 1;
                },
                Err(lmdb_zero::Error::Code(lmdb_zero::error::NOTFOUND)) => {
                    trace!(
                        target: LOG_TARGET,
                        "Buffered stale node {} already absent from jmt_node_data",
                        hex::encode(node_lmdb_key)
                    );
                },
                Err(e) => {
                    return Err(anyhow::anyhow!("failed to delete stale node from jmt_node_data: {e}"));
                },
            }
            // Always drop the buffer entry once we've handled (or determined absence of) the node.
            access
                .del_key(&self.stale_node_db, buffer_key.as_slice())
                .map_err(|e| anyhow::anyhow!("failed to delete buffered stale-node record: {e}"))?;
        }

        if deleted > 0 {
            debug!(
                target: LOG_TARGET,
                "Pruned {} finalised stale JMT nodes (threshold version {})",
                deleted,
                threshold
            );
        }
        Ok(deleted)
    }
}

impl TreeWriter for LmdbTreeWriter<'_> {
    fn write_node_batch(&self, node_batch: &jmt::storage::NodeBatch) -> anyhow::Result<()> {
        for (node_key, node) in node_batch.nodes() {
            let lmdb_key = Self::lmdb_node_key(node_key)?;
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
                lmdb_fetch_matching_after(self.txn, &self.unique_key_db, &value_key.1 .0)?;
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
                    lmdb_key.extend_from_slice(value_key.1 .0.as_slice());
                    lmdb_key.extend_from_slice(&value_key.0.to_be_bytes());
                    lmdb_insert(self.txn, &self.unique_key_db, &lmdb_key, value, "jmt_unique_key_table")?;
                },
                (Some(_v), Some(_x)) => {
                    trace!(target: LOG_TARGET, "Found existing unique key {} for version {}", value_key.1 .0.to_hex(), value_key.0);
                    return Err(anyhow::anyhow!("Duplicate value key found in batch"));
                },
                (Some(_v), None) => {
                    let mut lmdb_key: Vec<u8> = vec![];
                    lmdb_key.extend_from_slice(value_key.1 .0.as_slice());
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
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut rand::rng());
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
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut rand::rng());
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
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let smt_key = KeyHash(commitment.as_bytes().try_into().expect("Key hash is always 32 bytes"));
        let value = b"test_value".to_vec();
        let (root, updates) = jmt.put_value_set(vec![(smt_key, Some(value.clone()))], 0).unwrap();
        tree_writer.write_node_batch(&updates.node_batch).unwrap();

        txn.commit().unwrap();
        // Try again for new version.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut rand::rng());
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

    /// Counts entries currently stored in `jmt_node_data` across all versions.
    fn count_jmt_nodes(db: &TempDatabase) -> usize {
        use lmdb_zero::ReadTransaction;
        let read_txn = ReadTransaction::new(db.db().env_for_test()).unwrap();
        let access = read_txn.access();
        let mut cursor = read_txn.cursor(&**db.db().jmt_node_data_for_test()).unwrap();
        let mut count = 0usize;
        let mut row = cursor.first::<[u8], [u8]>(&access);
        while row.is_ok() {
            count += 1;
            row = cursor.next::<[u8], [u8]>(&access);
        }
        count
    }

    /// Counts entries currently buffered in `jmt_stale_node_data`.
    fn count_buffered_stale(db: &TempDatabase) -> usize {
        use lmdb_zero::ReadTransaction;
        let read_txn = ReadTransaction::new(db.db().env_for_test()).unwrap();
        let access = read_txn.access();
        let mut cursor = read_txn.cursor(&**db.db().jmt_stale_node_data_for_test()).unwrap();
        let mut count = 0usize;
        let mut row = cursor.first::<[u8], [u8]>(&access);
        while row.is_ok() {
            count += 1;
            row = cursor.next::<[u8], [u8]>(&access);
        }
        count
    }

    /// PoC for issue #7745 — verify that
    ///   1. `record_stale_nodes` buffers the entries that the jmt crate flagged as stale, and
    ///   2. `prune_stale_nodes_finalised_before` actually removes them from `jmt_node_data` once
    ///      they are past the configured reorg buffer.
    ///
    /// Without this PR, `jmt_node_data` grows unboundedly because the `stale_node_index_batch`
    /// returned from `JellyfishMerkleTree::put_value_set` is silently dropped on the floor. With
    /// this PR, it is drained on every block apply once the version is finalised.
    #[test]
    fn test_stale_node_buffer_records_and_prunes() {
        let db = TempDatabase::new();

        // ── Version 0: insert a key. The first put produces a single leaf root and no stale nodes.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment_a) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let key_a = KeyHash(commitment_a.as_bytes().try_into().expect("32 bytes"));
        let value = b"v".to_vec();
        let (_root_v0, batch_v0) = jmt.put_value_set(vec![(key_a, Some(value.clone()))], 0).unwrap();
        tree_writer.write_node_batch(&batch_v0.node_batch).unwrap();
        tree_writer
            .record_stale_nodes(&batch_v0.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        let nodes_after_v0 = count_jmt_nodes(&db);
        assert!(nodes_after_v0 >= 1);
        assert_eq!(
            count_buffered_stale(&db),
            0,
            "the very first put should not produce any stale nodes"
        );

        // ── Version 1: insert another key, forcing the JMT to split internal nodes. The previous
        //    single-leaf root becomes stale and should appear in `stale_node_index_batch`.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment_b) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let key_b = KeyHash(commitment_b.as_bytes().try_into().expect("32 bytes"));
        let (_root_v1, batch_v1) = jmt.put_value_set(vec![(key_b, Some(value.clone()))], 1).unwrap();
        assert!(
            !batch_v1.stale_node_index_batch.is_empty(),
            "splitting from 1 leaf to 2 must produce at least one stale node"
        );
        tree_writer.write_node_batch(&batch_v1.node_batch).unwrap();
        tree_writer
            .record_stale_nodes(&batch_v1.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        let buffered_after_v1 = count_buffered_stale(&db);
        assert_eq!(
            buffered_after_v1,
            batch_v1.stale_node_index_batch.len(),
            "every stale entry from put_value_set should now be in the buffer"
        );

        // ── Calling prune with threshold == 1 finalises only versions strictly < 1, i.e. nothing
        //    (because the stale entries at version=1 are still within the conceptual reorg buffer).
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let pruned = tree_writer.prune_stale_nodes_finalised_before(1).unwrap();
        txn.commit().unwrap();
        assert_eq!(
            pruned, 0,
            "stale entries at version 1 should not be pruned when threshold == 1"
        );
        assert_eq!(count_buffered_stale(&db), buffered_after_v1);

        // ── Calling prune with threshold == 2 finalises all stale entries with stale_since_version
        //    < 2, i.e. our version=1 entries. They should be deleted from both buffer and node data.
        let nodes_before_prune = count_jmt_nodes(&db);
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let pruned = tree_writer.prune_stale_nodes_finalised_before(2).unwrap();
        txn.commit().unwrap();
        assert_eq!(
            pruned, buffered_after_v1,
            "all buffered stale entries should now be pruned"
        );
        assert_eq!(count_buffered_stale(&db), 0);
        assert_eq!(
            count_jmt_nodes(&db),
            nodes_before_prune - pruned,
            "jmt_node_data shrinks by exactly the number pruned"
        );

        // ── The latest root (version 1) must still be queryable — we only deleted UNREACHABLE
        //    historical nodes.
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert!(jmt.get_root_hash(1).is_ok());
    }

    /// Verifies that a rewind discards the buffered stale-node records that became stale because
    /// of the rewound version. Without this, future block applies would erroneously delete nodes
    /// that are once again reachable from the new tip.
    #[test]
    fn test_rewind_discards_buffered_stale_records() {
        let db = TempDatabase::new();

        // Apply v0.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment_a) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let key_a = KeyHash(commitment_a.as_bytes().try_into().expect("32 bytes"));
        let (_, batch_v0) = jmt.put_value_set(vec![(key_a, Some(b"x".to_vec()))], 0).unwrap();
        tree_writer.write_node_batch(&batch_v0.node_batch).unwrap();
        tree_writer
            .record_stale_nodes(&batch_v0.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        // Apply v1 — produces stale entries at stale_since_version=1.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        let (_sk, commitment_b) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let key_b = KeyHash(commitment_b.as_bytes().try_into().expect("32 bytes"));
        let (_, batch_v1) = jmt.put_value_set(vec![(key_b, Some(b"y".to_vec()))], 1).unwrap();
        tree_writer.write_node_batch(&batch_v1.node_batch).unwrap();
        tree_writer
            .record_stale_nodes(&batch_v1.stale_node_index_batch)
            .unwrap();
        txn.commit().unwrap();

        let buffered_with_v1 = count_buffered_stale(&db);
        assert!(buffered_with_v1 > 0);

        // Rewind v1.
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        tree_writer.delete_all_for_version(1).unwrap();
        txn.commit().unwrap();

        assert_eq!(
            count_buffered_stale(&db),
            0,
            "rewinding v1 must drop all buffered stale records that became stale at v1"
        );

        // Critically — the v0 root must still be queryable. The nodes referenced by the v1 stale
        // entries were never deleted from disk (deletion was deferred), so they're still there.
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert!(jmt.get_root_hash(0).is_ok());
    }

    /// End-to-end — apply many puts that churn the tree, then prune everything older than the
    /// latest version. The number of remaining `jmt_node_data` entries must drop meaningfully.
    #[test]
    fn test_apply_churn_then_prune_bounds_growth() {
        let db = TempDatabase::new();
        const ROUNDS: usize = 40;

        let mut active_keys: Vec<KeyHash> = Vec::new();

        for round in 0..ROUNDS {
            let txn = db.db().create_write_txn();
            let tree_writer = db.db().create_lmdb_tree_writer(&txn);
            let reader = db.db().create_smt_reader().unwrap();
            let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);

            // Each round inserts 3 new keys and (after the warmup) deletes 2 old keys, churning
            // the internal node set heavily.
            let mut value_set: Vec<(KeyHash, Option<Vec<u8>>)> = Vec::new();
            for _ in 0..3 {
                let (_sk, commitment) = RistrettoPublicKey::random_keypair(&mut rand::rng());
                let key = KeyHash(commitment.as_bytes().try_into().expect("32 bytes"));
                value_set.push((key, Some(b"v".to_vec())));
                active_keys.push(key);
            }
            if round > 5 {
                for _ in 0..2 {
                    if active_keys.is_empty() {
                        break;
                    }
                    let key = active_keys.remove(0);
                    value_set.push((key, None));
                }
            }
            let (_root, batch) = jmt.put_value_set(value_set, round as u64).unwrap();
            tree_writer.write_node_batch(&batch.node_batch).unwrap();
            tree_writer.record_stale_nodes(&batch.stale_node_index_batch).unwrap();
            txn.commit().unwrap();
        }

        let nodes_unpruned = count_jmt_nodes(&db);
        let buffered = count_buffered_stale(&db);
        assert!(buffered > 0);

        // Finalise everything except the very last round (i.e. simulate reorg buffer of 1 round).
        let txn = db.db().create_write_txn();
        let tree_writer = db.db().create_lmdb_tree_writer(&txn);
        let pruned = tree_writer.prune_stale_nodes_finalised_before(ROUNDS as u64).unwrap();
        txn.commit().unwrap();

        let nodes_pruned = count_jmt_nodes(&db);
        assert_eq!(pruned, buffered, "every buffered stale entry should be deleted");
        assert_eq!(
            nodes_pruned + pruned,
            nodes_unpruned,
            "node count drops by exactly the prune count: {nodes_unpruned} - {pruned} != {nodes_pruned}"
        );
        // Sanity — pruning should yield a meaningful reduction in this churn workload.
        let reduction_pct = (nodes_unpruned - nodes_pruned) * 100 / nodes_unpruned.max(1);
        assert!(
            reduction_pct >= 30,
            "expected a non-trivial reduction; got {reduction_pct}% ({nodes_unpruned} -> {nodes_pruned})"
        );

        // The latest root must remain queryable.
        let reader = db.db().create_smt_reader().unwrap();
        let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&reader);
        assert!(jmt.get_root_hash((ROUNDS - 1) as u64).is_ok());
    }
}
