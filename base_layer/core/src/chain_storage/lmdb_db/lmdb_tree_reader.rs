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

use std::ops::Deref;

use borsh::BorshSerialize;
use jmt::storage::TreeReader;
use lmdb_zero::{ConstTransaction, ReadTransaction};
use log::warn;
use tari_storage::lmdb_store::DatabaseRef;

use crate::chain_storage::lmdb_db::lmdb::{lmdb_fetch_matching_after, lmdb_get};

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db";

pub struct LmdbTreeReader<'a> {
    txn: &'a ConstTransaction<'a>,
    node_db: DatabaseRef,
    unique_key_db: DatabaseRef,
}

impl<'a> LmdbTreeReader<'a> {
    pub fn new<T: Deref<Target = ConstTransaction<'a>>>(
        txn: &'a T,
        node_db: DatabaseRef,
        unique_key_db: DatabaseRef,
    ) -> Self {
        Self {
            txn: txn.deref(),
            node_db,
            unique_key_db,
        }
    }
}

impl TreeReader for LmdbTreeReader<'_> {
    fn get_node_option(&self, node_key: &jmt::storage::NodeKey) -> anyhow::Result<Option<jmt::storage::Node>> {
        let mut lmdb_key: Vec<u8> = vec![];
        lmdb_key.extend_from_slice(&node_key.version().to_be_bytes());
        BorshSerialize::serialize(&node_key.nibble_path(), &mut lmdb_key)?;
        let node = lmdb_get(self.txn, &self.node_db, &lmdb_key)?;
        Ok(node)
    }

    fn get_value_option(
        &self,
        _max_version: jmt::Version,
        key_hash: jmt::KeyHash,
    ) -> anyhow::Result<Option<jmt::OwnedValue>> {
        // see if there are any values already.
        let existing_values: Vec<(Vec<u8>, Option<Vec<u8>>)> =
            lmdb_fetch_matching_after(self.txn, &self.unique_key_db, &key_hash.0)?;
        let mut existing_history = vec![];
        for (key, x) in existing_values {
            let version = u64::from_be_bytes(key[32..].try_into().unwrap());
            existing_history.push((version, x));
            warn!(target: LOG_TARGET, "found version {} for key {:?}", version, key);
        }
        // sort by version
        existing_history.sort_by(|a, b| a.0.cmp(&b.0));

        let latest_value = existing_history.last().and_then(|x| x.1.clone());

        Ok(latest_value)
    }

    fn get_rightmost_leaf(&self) -> anyhow::Result<Option<(jmt::storage::NodeKey, jmt::storage::LeafNode)>> {
        todo!()
        // Ok(None)
    }
}

pub struct OwnedLmdbTreeReader<'a> {
    txn: ReadTransaction<'a>,
    node_db: DatabaseRef,
    unique_key_db: DatabaseRef,
}

impl<'a> OwnedLmdbTreeReader<'a> {
    pub fn new(txn: ReadTransaction<'a>, node_db: DatabaseRef, unique_key_db: DatabaseRef) -> Self {
        Self {
            txn,
            node_db,
            unique_key_db,
        }
    }
}

impl TreeReader for OwnedLmdbTreeReader<'_> {
    fn get_node_option(&self, node_key: &jmt::storage::NodeKey) -> anyhow::Result<Option<jmt::storage::Node>> {
        let inner = LmdbTreeReader::new(&self.txn, self.node_db.clone(), self.unique_key_db.clone());
        inner.get_node_option(node_key)
    }

    fn get_value_option(
        &self,
        max_version: jmt::Version,
        key_hash: jmt::KeyHash,
    ) -> anyhow::Result<Option<jmt::OwnedValue>> {
        let inner = LmdbTreeReader::new(&self.txn, self.node_db.clone(), self.unique_key_db.clone());
        inner.get_value_option(max_version, key_hash)
    }

    fn get_rightmost_leaf(&self) -> anyhow::Result<Option<(jmt::storage::NodeKey, jmt::storage::LeafNode)>> {
        todo!()
    }
}
