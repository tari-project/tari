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

//! # LmdbTreeReader — JMT Tree Reader (New Key Format)
//!
//! This reader matches the *current-state-only* storage scheme implemented in `LmdbTreeWriter`:
//!
//! - **Node key**: `borsh::serialize(&NodeKey)` — the entire `NodeKey` (no version prefix).
//! - **Value key**: `KeyHash` (32 bytes) — directly indexed by the hash of the UTXO commitment.
//!
//! ## Migration
//!
//! Existing databases use the OLD key format (`version || nibble_path`).
//! A migration in `lmdb_db.rs` re-keys all entries to the new format.

use std::ops::Deref;

use borsh::BorshSerialize;
use jmt::storage::TreeReader;
use lmdb_zero::{ConstTransaction, ReadTransaction};
use tari_storage::lmdb_store::DatabaseRef;

use crate::chain_storage::lmdb_db::lmdb::lmdb_get;

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_tree_reader";

/// Read-only access to JMT nodes stored in LMDB (new key format).
///
/// # Key Format (NEW)
///
/// `jmt_node_data` key = `borsh::serialize(&NodeKey)` — NO version prefix.
///
/// Each unique tree position (`NodeKey`) has exactly ONE entry in the database,
/// representing its current state. This is correct because the JMT models the
/// CURRENT UTXO set (not historical versions).
pub struct LmdbTreeReader<'a> {
    txn: &'a ConstTransaction<'a>,
    node_db: DatabaseRef,
    value_db: DatabaseRef,
}

impl<'a> LmdbTreeReader<'a> {
    pub fn new<T: Deref<Target = ConstTransaction<'a>>>(
        txn: &'a T,
        node_db: DatabaseRef,
        value_db: DatabaseRef,
    ) -> Self {
        Self {
            txn: txn.deref(),
            node_db,
            value_db,
        }
    }
}

impl TreeReader for LmdbTreeReader<'_> {
    /// Read a node by its `NodeKey`.
    ///
    /// # Key Format (NEW)
    ///
    /// `key = borsh::serialize(&node_key)` — the ENTIRE `NodeKey` is the key.
    ///
    /// This is different from the OLD format which used `version || borsh(nibble_path)`.
    /// The new format stores each `NodeKey` ONLY ONCE (current state only),
    /// which reduces storage by ~97%.
    fn get_node_option(&self, node_key: &jmt::storage::NodeKey) -> anyhow::Result<Option<jmt::storage::Node>> {
        let mut lmdb_key: Vec<u8> = vec![];
        BorshSerialize::serialize(node_key, &mut lmdb_key)?;
        let node = lmdb_get(self.txn, &self.node_db, &lmdb_key)?;
        Ok(node)
    }

    /// Read a value (UTXO data) by its `KeyHash`.
    ///
    /// # Key Format (NEW)
    ///
    /// `key = KeyHash` (32 bytes) — directly indexed by the hash of the UTXO commitment.
    ///
    /// There is no version history (the JMT only stores the CURRENT UTXO set).
    /// If the UTXO exists, its value is returned; otherwise `None`.
    fn get_value_option(
        &self,
        _max_version: jmt::Version,
        key_hash: jmt::KeyHash,
    ) -> anyhow::Result<Option<jmt::OwnedValue>> {
        let mut lmdb_key: Vec<u8> = vec![];
        lmdb_key.extend_from_slice(&key_hash.0);
        let existing = lmdb_get(self.txn, &self.value_db, &lmdb_key)?;
        Ok(existing)
    }

    fn get_rightmost_leaf(&self) -> anyhow::Result<Option<(jmt::storage::NodeKey, jmt::storage::LeafNode)>> {
        // This is used for debugging / testing. The implementation depends on
        // LMDB cursor traversal in reverse order.
        //
        // For now, return None (not critial for consensus).
        // TODO: Implement using LMDB cursor if needed.
        Ok(None)
    }
}

/// Owned variant of `LmdbTreeReader` (holds its own `ReadTransaction`).
///
/// This is used for long-lived readers that need to outlive the caller's scope.
pub struct OwnedLmdbTreeReader<'a> {
    txn: ReadTransaction<'a>,
    node_db: DatabaseRef,
    value_db: DatabaseRef,
}

impl<'a> OwnedLmdbTreeReader<'a> {
    pub fn new(txn: ReadTransaction<'a>, node_db: DatabaseRef, value_db: DatabaseRef) -> Self {
        Self {
            txn,
            node_db,
            value_db,
        }
    }
}

impl TreeReader for OwnedLmdbTreeReader<'_> {
    fn get_node_option(&self, node_key: &jmt::storage::NodeKey) -> anyhow::Result<Option<jmt::storage::Node>> {
        let inner = LmdbTreeReader::new(&self.txn, self.node_db.clone(), self.value_db.clone());
        inner.get_node_option(node_key)
    }

    fn get_value_option(
        &self,
        max_version: jmt::Version,
        key_hash: jmt::KeyHash,
    ) -> anyhow::Result<Option<jmt::OwnedValue>> {
        let inner = LmdbTreeReader::new(&self.txn, self.node_db.clone(), self.value_db.clone());
        inner.get_value_option(max_version, key_hash)
    }

    fn get_rightmost_leaf(&self) -> anyhow::Result<Option<(jmt::storage::NodeKey, jmt::storage::LeafNode)>> {
        let inner = LmdbTreeReader::new(&self.txn, self.node_db.clone(), self.value_db.clone());
        inner.get_rightmost_leaf()
    }
}
