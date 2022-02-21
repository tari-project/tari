//  Copyright 2021. The Tari Project
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

use std::{
    convert::TryInto,
    ops::Deref,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use digest::Digest;
use log::*;
use tari_common_types::types::{HashDigest, PublicKey};
use tari_crypto::common::Blake256;
use tari_mmr::{MemBackendVec, MerkleMountainRange};
use tari_utilities::hex::Hex;

use crate::{
    models::{KeyValue, SchemaState, StateOpLogEntry, StateRoot},
    storage::{
        state::{db_key_value::DbKeyValue, DbStateOpLogEntry, StateDbBackendAdapter},
        StorageError,
        UnitOfWorkTracker,
    },
};

const LOG_TARGET: &str = "tari::dan::state_db";

pub trait StateDbUnitOfWork: StateDbUnitOfWorkReader {
    fn set_value(&mut self, schema: String, key: Vec<u8>, value: Vec<u8>) -> Result<(), StorageError>;
    fn set_u64(&mut self, schema: &str, key: &[u8], value: u64) -> Result<(), StorageError>;
    fn commit(&mut self) -> Result<(), StorageError>;
    fn clear_all_state(&self) -> Result<(), StorageError>;
}

pub trait StateDbUnitOfWorkReader: Clone + Send + Sync {
    fn context(&self) -> &UnitOfWorkContext;
    fn get_value(&self, schema: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError>;
    fn get_u64(&self, schema: &str, key: &[u8]) -> Result<Option<u64>, StorageError>;
    fn find_keys_by_value(&self, schema: &str, value: &[u8]) -> Result<Vec<Vec<u8>>, StorageError>;
    fn calculate_root(&self) -> Result<StateRoot, StorageError>;
    fn get_all_state(&self) -> Result<Vec<SchemaState>, StorageError>;
    fn get_op_logs_for_height(&self, height: u64) -> Result<Vec<StateOpLogEntry>, StorageError>;
}

#[derive(Debug, Clone)]
pub struct UnitOfWorkContext {
    asset_public_key: PublicKey,
    height: u64,
}

impl UnitOfWorkContext {
    pub fn new(height: u64, asset_public_key: PublicKey) -> Self {
        Self {
            height,
            asset_public_key,
        }
    }

    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn asset_public_key(&self) -> &PublicKey {
        &self.asset_public_key
    }
}

pub struct StateDbUnitOfWorkImpl<TBackendAdapter: StateDbBackendAdapter> {
    inner: Arc<RwLock<StateDbUnitOfWorkInner<TBackendAdapter>>>,
    context: UnitOfWorkContext,
}

impl<TBackendAdapter: StateDbBackendAdapter> StateDbUnitOfWorkImpl<TBackendAdapter> {
    pub fn new(context: UnitOfWorkContext, backend_adapter: TBackendAdapter) -> Self {
        Self {
            inner: Arc::new(RwLock::new(StateDbUnitOfWorkInner::new(backend_adapter))),
            context,
        }
    }
}

impl<TBackendAdapter: StateDbBackendAdapter> Clone for StateDbUnitOfWorkImpl<TBackendAdapter> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            context: self.context.clone(),
        }
    }
}

impl<TBackendAdapter: StateDbBackendAdapter> StateDbUnitOfWork for StateDbUnitOfWorkImpl<TBackendAdapter> {
    fn set_value(&mut self, schema: String, key: Vec<u8>, value: Vec<u8>) -> Result<(), StorageError> {
        let mut inner = self.inner.write().unwrap();
        inner
            .updates
            .push(UnitOfWorkTracker::new(DbKeyValue { schema, key, value }, true));

        Ok(())
    }

    fn set_u64(&mut self, schema: &str, key: &[u8], value: u64) -> Result<(), StorageError> {
        self.set_value(schema.to_string(), Vec::from(key), Vec::from(value.to_le_bytes()))
    }

    fn commit(&mut self) -> Result<(), StorageError> {
        let mut inner = self.inner.write().unwrap();
        let tx = inner
            .backend_adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;
        // let mut current_tree = inner
        //     .backend_adapter
        //     .get_current_state_tree(&tx)
        //     .map_err(TBackendAdapter::Error::into)?;
        debug!(target: LOG_TARGET, "Committing {} state update(s)", inner.updates.len());
        for item in &inner.updates {
            let i = item.get();
            inner
                .backend_adapter
                .update_key_value(&i.schema, &i.key, &i.value, &tx)
                .map_err(TBackendAdapter::Error::into)?;

            inner
                .backend_adapter
                .add_state_oplog_entry(
                    DbStateOpLogEntry::set_operation(self.context.height, i.deref().clone()),
                    &tx,
                )
                .map_err(TBackendAdapter::Error::into)?;
            // let key = format!("{}.{}", &i.schema, bs58::encode(&i.key).into_string());
            // current_tree.insert(key, i.value.clone());
        }

        // inner
        //     .backend_adapter
        //     .set_current_state_tree(current_tree, &tx)
        //     .map_err(TBackendAdapter::Error::into)?;

        inner
            .backend_adapter
            .commit(&tx)
            .map_err(TBackendAdapter::Error::into)?;
        inner.updates = vec![];

        Ok(())
    }

    /// Clears the state db immediately (before commit) - this will not be needed in future when build up the state from
    /// instructions/op logs
    fn clear_all_state(&self) -> Result<(), StorageError> {
        let inner = self.inner.write().unwrap();
        let tx = inner
            .backend_adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;
        inner
            .backend_adapter
            .clear_all_state(&tx)
            .map_err(TBackendAdapter::Error::into)
    }
}

impl<TBackendAdapter: StateDbBackendAdapter> StateDbUnitOfWorkReader for StateDbUnitOfWorkImpl<TBackendAdapter> {
    fn context(&self) -> &UnitOfWorkContext {
        &self.context
    }

    fn get_value(&self, schema: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        let inner = self.inner.read().unwrap();
        // Hit the DB.
        inner
            .backend_adapter
            .get(schema, key)
            .map_err(TBackendAdapter::Error::into)
    }

    fn get_u64(&self, schema: &str, key: &[u8]) -> Result<Option<u64>, StorageError> {
        let data = self.get_value(schema, key)?;
        match data {
            Some(data) => {
                let mut data2: [u8; 8] = [0; 8];
                data2.copy_from_slice(&data);
                Ok(Some(u64::from_le_bytes(data2)))
            },
            None => Ok(None),
        }
    }

    fn find_keys_by_value(&self, schema: &str, value: &[u8]) -> Result<Vec<Vec<u8>>, StorageError> {
        let inner = self.inner.read().unwrap();
        inner
            .backend_adapter
            .find_keys_by_value(schema, value)
            .map_err(TBackendAdapter::Error::into)
    }

    fn calculate_root(&self) -> Result<StateRoot, StorageError> {
        let inner = self.inner.read().unwrap();
        let tx = inner
            .backend_adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;
        // let root_node : Node<Vec<u8>> = inner.backend_adapter.get_current_state_tree(&tx).into();

        // omg it's an MMR of MMRs
        let mut top_level_mmr = MerkleMountainRange::<Blake256, _>::new(MemBackendVec::new());
        let schemas = inner
            .backend_adapter
            .get_all_schemas(&tx)
            .map_err(TBackendAdapter::Error::into)?;
        debug!(
            target: LOG_TARGET,
            "calculate_root: {} key value schemas loaded",
            schemas.len()
        );

        for schema in schemas {
            let mut mmr = MerkleMountainRange::<Blake256, _>::new(MemBackendVec::new());
            for key_value in inner
                .backend_adapter
                .get_all_values_for_schema(&schema, &tx)
                .map_err(TBackendAdapter::Error::into)?
            {
                debug!(
                    target: LOG_TARGET,
                    "schema = {}, key = {}, value = {}",
                    schema,
                    key_value.key.to_hex(),
                    key_value.value.to_hex()
                );
                if let Some(updated_value) = find_update(&inner, &schema, &key_value.key) {
                    let hasher = HashDigest::new();
                    mmr.push(hasher.chain(&key_value.key).chain(updated_value).finalize().to_vec())?;
                } else {
                    let hasher = HashDigest::new();
                    mmr.push(hasher.chain(&key_value.key).chain(&key_value.value).finalize().to_vec())?;
                }
            }
            let hasher = HashDigest::new();
            top_level_mmr.push(hasher.chain(schema).chain(mmr.get_merkle_root()?).finalize().to_vec())?;
        }
        Ok(StateRoot::new(
            top_level_mmr
                .get_merkle_root()?
                .try_into()
                .expect("MMR output incorrect size"),
        ))
    }

    fn get_all_state(&self) -> Result<Vec<SchemaState>, StorageError> {
        let inner = self.inner.read().unwrap();
        let tx = inner
            .backend_adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;

        let schemas = inner
            .backend_adapter
            .get_all_schemas(&tx)
            .map_err(TBackendAdapter::Error::into)?;
        let mut schema_state = Vec::with_capacity(schemas.len());
        for schema in schemas {
            let key_values = inner
                .backend_adapter
                .get_all_values_for_schema(&schema, &tx)
                .map_err(TBackendAdapter::Error::into)?;

            let key_values = key_values
                .into_iter()
                .map(|kv| {
                    let value = find_update(&inner, &schema, &kv.key).unwrap_or(kv.value);
                    KeyValue { key: kv.key, value }
                })
                .collect();

            schema_state.push(SchemaState::new(schema, key_values));
        }
        Ok(schema_state)
    }

    fn get_op_logs_for_height(&self, height: u64) -> Result<Vec<StateOpLogEntry>, StorageError> {
        let inner = self.inner.read().unwrap();
        let tx = inner
            .backend_adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;

        let op_logs = inner
            .backend_adapter
            .get_state_op_logs_by_height(height, &tx)
            .map_err(TBackendAdapter::Error::into)?;

        let op_logs = op_logs.into_iter().map(Into::into).collect();
        Ok(op_logs)
    }
}

fn find_update<TBackendAdapter: StateDbBackendAdapter>(
    inner: &RwLockReadGuard<StateDbUnitOfWorkInner<TBackendAdapter>>,
    schema: &str,
    key: &[u8],
) -> Option<Vec<u8>> {
    for update in &inner.updates {
        let update = update.get();
        if update.schema == schema && update.key == key {
            return Some(update.value.clone());
        }
    }
    None
}

pub struct StateDbUnitOfWorkInner<TBackendAdapter: StateDbBackendAdapter> {
    backend_adapter: TBackendAdapter,
    updates: Vec<UnitOfWorkTracker<DbKeyValue>>,
}

impl<TBackendAdapter: StateDbBackendAdapter> StateDbUnitOfWorkInner<TBackendAdapter> {
    pub fn new(backend_adapter: TBackendAdapter) -> Self {
        Self {
            updates: vec![],
            backend_adapter,
        }
    }
}
