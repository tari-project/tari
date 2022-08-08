// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    convert::TryInto,
    ops::Deref,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use digest::Digest;
use log::*;
use tari_common_types::types::FixedHash;
use tari_crypto::hash::blake2::Blake256;
use tari_dan_common_types::storage::UnitOfWorkTracker;
use tari_mmr::{MemBackendVec, MerkleMountainRange};
use tari_utilities::hex::Hex;

use crate::state::{
    db_key_value::DbKeyValue,
    error::StateStorageError,
    models::{KeyValue, SchemaState, StateOpLogEntry, StateRoot},
    DbStateOpLogEntry,
    StateDbBackendAdapter,
};

const LOG_TARGET: &str = "tari::dan::state_db";

pub trait StateDbUnitOfWork: StateDbUnitOfWorkReader {
    fn set_value(&mut self, schema: String, key: Vec<u8>, value: Vec<u8>) -> Result<(), StateStorageError>;
    fn set_u64(&mut self, schema: &str, key: &[u8], value: u64) -> Result<(), StateStorageError>;
    fn commit(&mut self) -> Result<(), StateStorageError>;
    fn clear_all_state(&self) -> Result<(), StateStorageError>;
}

pub trait StateDbUnitOfWorkReader: Clone + Send + Sync {
    fn context(&self) -> &UnitOfWorkContext;
    fn get_value(&self, schema: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StateStorageError>;
    fn get_u64(&self, schema: &str, key: &[u8]) -> Result<Option<u64>, StateStorageError>;
    fn find_keys_by_value(&self, schema: &str, value: &[u8]) -> Result<Vec<Vec<u8>>, StateStorageError>;
    fn calculate_root(&self) -> Result<StateRoot, StateStorageError>;
    fn get_all_state(&self) -> Result<Vec<SchemaState>, StateStorageError>;
    fn get_op_logs_for_height(&self, height: u64) -> Result<Vec<StateOpLogEntry>, StateStorageError>;
}

#[derive(Debug, Clone)]
pub struct UnitOfWorkContext {
    contract_id: FixedHash,
    height: u64,
}

impl UnitOfWorkContext {
    pub fn new(height: u64, contract_id: FixedHash) -> Self {
        Self { height, contract_id }
    }

    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn contract_id(&self) -> &FixedHash {
        &self.contract_id
    }
}

pub struct StateDbUnitOfWorkImpl<TBackendAdapter> {
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
    fn set_value(&mut self, schema: String, key: Vec<u8>, value: Vec<u8>) -> Result<(), StateStorageError> {
        let mut inner = self.inner.write()?;
        inner
            .updates
            .push(UnitOfWorkTracker::new(DbKeyValue { schema, key, value }, true));

        Ok(())
    }

    fn set_u64(&mut self, schema: &str, key: &[u8], value: u64) -> Result<(), StateStorageError> {
        self.set_value(schema.to_string(), Vec::from(key), Vec::from(value.to_le_bytes()))
    }

    fn commit(&mut self) -> Result<(), StateStorageError> {
        let mut inner = self.inner.write()?;
        if !inner.is_dirty() {
            return Ok(());
        }
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
    fn clear_all_state(&self) -> Result<(), StateStorageError> {
        let inner = self.inner.write()?;
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

    fn get_value(&self, schema: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StateStorageError> {
        let inner = self.inner.read()?;
        // Hit the DB.
        inner
            .backend_adapter
            .get(schema, key)
            .map_err(TBackendAdapter::Error::into)
    }

    fn get_u64(&self, schema: &str, key: &[u8]) -> Result<Option<u64>, StateStorageError> {
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

    fn find_keys_by_value(&self, schema: &str, value: &[u8]) -> Result<Vec<Vec<u8>>, StateStorageError> {
        let inner = self.inner.read()?;
        inner
            .backend_adapter
            .find_keys_by_value(schema, value)
            .map_err(TBackendAdapter::Error::into)
    }

    // TODO: Needs to keep a merkle proof of the latest state and append all updates onto that to get the merkle root
    // TODO: This does not include _new_ keys that are to be added in the updates
    fn calculate_root(&self) -> Result<StateRoot, StateStorageError> {
        let inner = self.inner.read()?;
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
                    let hasher = Blake256::new();
                    mmr.push(hasher.chain(&key_value.key).chain(updated_value).finalize().to_vec())?;
                } else {
                    let hasher = Blake256::new();
                    mmr.push(hasher.chain(&key_value.key).chain(&key_value.value).finalize().to_vec())?;
                }
            }
            let hasher = Blake256::new();
            top_level_mmr.push(hasher.chain(schema).chain(mmr.get_merkle_root()?).finalize().to_vec())?;
        }

        Ok(StateRoot::new(
            top_level_mmr
                .get_merkle_root()?
                .try_into()
                .expect("MMR output incorrect size"),
        ))
    }

    fn get_all_state(&self) -> Result<Vec<SchemaState>, StateStorageError> {
        let inner = self.inner.read()?;
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

    fn get_op_logs_for_height(&self, height: u64) -> Result<Vec<StateOpLogEntry>, StateStorageError> {
        let inner = self.inner.read()?;
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

pub struct StateDbUnitOfWorkInner<TBackendAdapter> {
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

    pub fn is_dirty(&self) -> bool {
        !self.updates.is_empty()
    }
}
