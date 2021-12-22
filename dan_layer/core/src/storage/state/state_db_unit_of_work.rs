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

use std::sync::{Arc, RwLock};

use bs58;

use crate::{
    models::StateRoot,
    storage::{
        state::{db_key_value::DbKeyValue, StateDbBackendAdapter},
        StorageError,
        UnitOfWorkTracker,
    },
};
use tari_mmr::{MerkleMountainRange, MemBackendVec};
use patricia_tree::node::Node;
use tari_crypto::common::Blake256;

pub trait StateDbUnitOfWork: Clone + Send + Sync {
    fn set_value(&mut self, schema: String, key: Vec<u8>, value: Vec<u8>) -> Result<(), StorageError>;
    fn get_value(&mut self, schema: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError>;
    fn get_u64(&mut self, schema: &str, key: &[u8]) -> Result<Option<u64>, StorageError>;
    fn set_u64(&mut self, schema: &str, key: &[u8], value: u64) -> Result<(), StorageError>;
    fn find_keys_by_value(&self, schema: &str, value: &[u8]) -> Result<Vec<Vec<u8>>, StorageError>;
    fn commit(&mut self) -> Result<StateRoot, StorageError>;
    fn calculate_root(&self) -> Result<StateRoot, StorageError>;
}

pub struct StateDbUnitOfWorkImpl<TBackendAdapter: StateDbBackendAdapter> {
    inner: Arc<RwLock<StateDbUnitOfWorkInner<TBackendAdapter>>>,
}

impl<TBackendAdapter: StateDbBackendAdapter> StateDbUnitOfWorkImpl<TBackendAdapter> {
    pub fn new(backend_adapter: TBackendAdapter) -> Self {
        Self {
            inner: Arc::new(RwLock::new(StateDbUnitOfWorkInner::new(backend_adapter))),
        }
    }
}

impl<TBackendAdapter: StateDbBackendAdapter> Clone for StateDbUnitOfWorkImpl<TBackendAdapter> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
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

    fn get_value(&mut self, schema: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        let mut inner = self.inner.write().unwrap();
        for v in &inner.updates {
            let inner_v = v.get();
            if inner_v.schema == schema && inner_v.key == key {
                return Ok(Some(inner_v.value.clone()));
            }
        }
        // Hit the DB.
        let value = inner
            .backend_adapter
            .get(schema, key)
            .map_err(TBackendAdapter::Error::into)?;
        if let Some(value) = value {
            inner.updates.push(UnitOfWorkTracker::new(
                DbKeyValue {
                    schema: schema.to_string(),
                    key: Vec::from(key),
                    value: value.clone(),
                },
                false,
            ));
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    fn get_u64(&mut self, schema: &str, key: &[u8]) -> Result<Option<u64>, StorageError> {
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

    fn set_u64(&mut self, schema: &str, key: &[u8], value: u64) -> Result<(), StorageError> {
        self.set_value(schema.to_string(), Vec::from(key), Vec::from(value.to_le_bytes()))
    }

    fn find_keys_by_value(&self, schema: &str, value: &[u8]) -> Result<Vec<Vec<u8>>, StorageError> {
        let inner = self.inner.read().unwrap();
        inner
            .backend_adapter
            .find_keys_by_value(schema, value)
            .map_err(TBackendAdapter::Error::into)
    }

    fn commit(&mut self) -> Result<StateRoot, StorageError> {
        let mut inner = self.inner.write().unwrap();
        let tx = inner
            .backend_adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;
        let mut current_tree = inner
            .backend_adapter
            .get_current_state_tree(&tx)
            .map_err(TBackendAdapter::Error::into)?;
        for item in &inner.updates {
            let i = item.get();
            inner
                .backend_adapter
                .update_key_value(&i.schema, &i.key, &i.value, &tx)
                .map_err(TBackendAdapter::Error::into)?;
            let key = format!("{}.{}", &i.schema, bs58::encode(&i.key).into_string());
            current_tree.insert(key, i.value.clone());
        }

        inner
            .backend_adapter
            .set_current_state_tree(current_tree, &tx)
            .map_err(TBackendAdapter::Error::into)?;

        inner
            .backend_adapter
            .commit(&tx)
            .map_err(TBackendAdapter::Error::into)?;
        inner.updates = vec![];

        Ok(StateRoot::default())
    }

    fn calculate_root(&self) -> Result<StateRoot, StorageError> {
        let inner = self.inner.read().unwrap();
        let tx = inner
            .backend_adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;
        // let root_node : Node<Vec<u8>> = inner.backend_adapter.get_current_state_tree(&tx).into();

        let hasher = HashDigest::new();
        for schema in inner.backend_adapter.get_all_schemas(&tx)? {
            let mut mmr = MerkleMountainRange::<Blake256, _>::new(MemBackendVec::new());
            for item in inner.backend_adapter.get_all_values_in_schema(schema, &tx)? {
                mmr.push(item.)
            }
        }
        let mut mmr = MerkleMountainRange::<Blake256, _>::new(MemBackendVec::new());
        generate_mmr(&mut mmr, root_node);
        Ok(StateRoot::default())
    }
}

fn generate_mmr(mmr: &mut MerkleMountainRange<Blake256, MemBackendVec<Vec<u8>>>, node: Node<Vec<u8>>) {
    if node.
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
