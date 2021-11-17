// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::models::{
    HotStuffMessageType,
    Instruction,
    Payload,
    QuorumCertificate,
    Signature,
    StateRoot,
    TreeNodeHash,
    ViewId,
};
pub use chain_storage_service::ChainStorageService;
pub use error::StorageError;
pub use lmdb::{LmdbAssetBackend, LmdbAssetStore};
use std::{
    fmt::Debug,
    marker::PhantomData,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
        RwLock,
        RwLockReadGuard,
        RwLockWriteGuard,
    },
};
pub use store::{AssetDataStore, AssetStore};
pub mod chain;
mod chain_storage_service;
mod db_factory;
mod error;
pub mod lmdb;
mod state;
mod store;
mod unit_of_work_tracker;

use crate::storage::chain::{DbInstruction, DbNode, DbQc};
pub use db_factory::DbFactory;
pub use unit_of_work_tracker::UnitOfWorkTracker;

pub trait StateDbUnitOfWork: Clone + Sized + Send + Sync {
    fn set_value(&mut self, schema: String, key: Vec<u8>, value: Vec<u8>) -> Result<(), StorageError>;
    fn commit(&mut self) -> Result<StateRoot, StorageError>;
    fn calculate_root(&self) -> Result<StateRoot, StorageError>;
}

#[derive(Debug)]
pub struct DbKeyValue {
    pub schema: String,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Clone)]
pub struct StateDbUnitOfWorkImpl<TBackendAdapter: StateDbBackendAdapter> {
    updates: Vec<UnitOfWorkTracker<DbKeyValue>>,
    backend_adapter: TBackendAdapter,
}

impl<TBackendAdapter: StateDbBackendAdapter> StateDbUnitOfWorkImpl<TBackendAdapter> {
    pub fn new(backend_adapter: TBackendAdapter) -> Self {
        Self {
            updates: vec![],
            backend_adapter,
        }
    }
}

pub trait StateDbBackendAdapter {
    type BackendTransaction;
    type Error;

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error>;
    fn update_key_value(
        &self,
        schema: &str,
        key: &[u8],
        value: &[u8],
        tx: &Self::BackendTransaction,
    ) -> Result<(), Self::Error>;
    fn commit(&self, tx: &Self::BackendTransaction) -> Result<(), Self::Error>;
}

impl<TBackendAdapter: StateDbBackendAdapter> StateDbUnitOfWork for StateDbUnitOfWorkImpl<TBackendAdapter> {
    fn set_value(&mut self, schema: String, key: Vec<u8>, value: Vec<u8>) -> Result<(), StorageError> {
        self.updates
            .push(UnitOfWorkTracker::new(DbKeyValue { schema, key, value }, true));

        Ok(())
    }

    fn commit(&mut self) -> Result<StateRoot, StorageError> {
        let tx = self.backend_adapter.create_transaction()?;
        for item in &self.updates {
            self.backend_adapter
                .update_key_value(&item.schema, &item.key, &item.value, &tx);
        }

        self.backend_adapter.commit(&tx)?;

        Ok(StateRoot::default())
    }

    fn calculate_root(&self) -> Result<StateRoot, StorageError> {
        Ok(StateRoot::default())
    }
}

pub struct StateDb<TStateDbBackendAdapter: StateDbBackendAdapter> {
    pd: PhantomData<TStateDbBackendAdapter>,
}

impl<TStateDbBackendAdapter: StateDbBackendAdapter> StateDb<TStateDbBackendAdapter> {
    pub fn new() -> Self {
        Self { pd: Default::default() }
    }

    pub fn new_unit_of_work(&self) -> StateDbUnitOfWorkImpl<TStateDbBackendAdapter> {
        StateDbUnitOfWorkImpl { updates: vec![] }
        // let mut unit_of_work = self.current_unit_of_work_mut();
        // if unit_of_work.is_none() {
        //     self.unit_of_work = Some(StateDbUnitOfWork {});
        //     unit_of_work = self.unit_of_work
        // };
        // unit_of_work.as_mut().unwrap()
    }
}
