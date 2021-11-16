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

pub use store::{AssetDataStore, AssetStore};

mod chain_db;
pub use chain_db::{ChainDb, ChainDbUnitOfWork, DbInstruction, DbNode, DbQc};
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

mod chain_storage_service;
mod error;
pub mod lmdb;
mod store;

// feature sql

pub trait DbFactory<TBackendAdapter: BackendAdapter> {
    fn create(&self) -> Result<ChainDb<TBackendAdapter>, StorageError>;
    fn create_state_db(&self) -> Result<StateDb<StateDbUnitOfWorkImpl>, StorageError>;
}

#[derive(Debug)]
pub struct UnitOfWorkTracker<TItem> {
    item: Arc<RwLock<TItem>>,
    is_dirty: Arc<AtomicBool>,
}

impl<TItem> Clone for UnitOfWorkTracker<TItem> {
    fn clone(&self) -> Self {
        Self {
            item: self.item.clone(),
            is_dirty: self.is_dirty.clone(),
        }
    }
}

impl<TItem> UnitOfWorkTracker<TItem> {
    pub fn new(item: TItem, is_dirty: bool) -> Self {
        Self {
            item: Arc::new(RwLock::new(item)),
            is_dirty: Arc::new(AtomicBool::new(is_dirty)),
        }
    }

    pub fn get(&self) -> RwLockReadGuard<TItem> {
        self.item.read().unwrap()
    }

    pub fn get_mut(&self) -> RwLockWriteGuard<TItem> {
        self.is_dirty.store(true, Ordering::SeqCst);
        self.item.write().unwrap()
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty.load(Ordering::SeqCst)
    }
}

// // TODO: I don't really like the matches on this struct, so it would be better to have individual types, e.g.
// // NodeDataTracker, QcDataTracker
// #[derive(Clone, Debug, PartialEq)]
// pub enum UnitOfWorkTracker {
//     SidechainMetadata,
//     LockedQc {
//         message_type: HotStuffMessageType,
//         view_number: ViewId,
//         node_hash: TreeNodeHash,
//         signature: Option<Signature>,
//     },
//     PrepareQc {
//         message_type: HotStuffMessageType,
//         view_number: ViewId,
//         node_hash: TreeNodeHash,
//         signature: Option<Signature>,
//     },
//     Node {
//         hash: TreeNodeHash,
//         parent: TreeNodeHash,
//         height: u32,
//         is_committed: bool,
//     },
//     Instruction {
//         instruction: Instruction,
//         node_hash: TreeNodeHash,
//     },
// }

pub trait BackendAdapter: Send + Sync + Clone {
    type BackendTransaction;
    type Error: Into<StorageError>;
    type Id: Copy + Send + Sync + Debug + PartialEq;
    type Payload: Payload;

    fn is_empty(&self) -> Result<bool, Self::Error>;
    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error>;
    fn insert_node(&self, item: &DbNode, transaction: &Self::BackendTransaction) -> Result<(), Self::Error>;
    fn update_node(
        &self,
        id: &Self::Id,
        item: &DbNode,
        transaction: &Self::BackendTransaction,
    ) -> Result<(), Self::Error>;
    fn insert_instruction(
        &self,
        item: &DbInstruction,
        transaction: &Self::BackendTransaction,
    ) -> Result<(), Self::Error>;
    fn commit(&self, transaction: &Self::BackendTransaction) -> Result<(), Self::Error>;
    fn locked_qc_id(&self) -> Self::Id;
    fn prepare_qc_id(&self) -> Self::Id;
    fn find_highest_prepared_qc(&self) -> Result<QuorumCertificate, Self::Error>;
    fn get_locked_qc(&self) -> Result<QuorumCertificate, Self::Error>;
    fn get_prepare_qc(&self) -> Result<QuorumCertificate, Self::Error>;
    fn find_node_by_hash(&self, node_hash: &TreeNodeHash) -> Result<(Self::Id, DbNode), Self::Error>;
    fn update_prepare_qc(&self, item: &DbQc, transaction: &Self::BackendTransaction) -> Result<(), Self::Error>;
    fn update_locked_qc(&self, locked_qc: &DbQc, transaction: &Self::BackendTransaction) -> Result<(), Self::Error>;
}

pub trait UnitOfWork: Clone + Send + Sync {
    fn commit(&mut self) -> Result<(), StorageError>;
    fn add_node(&mut self, hash: TreeNodeHash, parent: TreeNodeHash, height: u32) -> Result<(), StorageError>;
    fn add_instruction(&mut self, node_hash: TreeNodeHash, instruction: Instruction) -> Result<(), StorageError>;
    fn get_locked_qc(&mut self) -> Result<QuorumCertificate, StorageError>;
    fn set_locked_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError>;
    fn get_prepare_qc(&mut self) -> Result<QuorumCertificate, StorageError>;
    fn set_prepare_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError>;
    fn commit_node(&mut self, node_hash: &TreeNodeHash) -> Result<(), StorageError>;
    // fn find_proposed_node(&mut self, node_hash: TreeNodeHash) -> Result<(Self::Id, UnitOfWorkTracker), StorageError>;
}

pub trait StateDbUnitOfWork: Clone + Sized + Send + Sync {
    fn set_value(&mut self, schema: String, key: Vec<u8>, value: Vec<u8>) -> Result<(), StorageError>;
    fn commit(&mut self) -> Result<StateRoot, StorageError>;
    fn calculate_root(&self) -> Result<StateRoot, StorageError>;
}

#[derive(Clone)]
pub struct StateDbUnitOfWorkImpl {
    // hashmap rather?
    updates: Vec<(String, Vec<u8>, Vec<u8>)>,
}

impl StateDbUnitOfWork for StateDbUnitOfWorkImpl {
    fn set_value(&mut self, schema: String, key: Vec<u8>, value: Vec<u8>) -> Result<(), StorageError> {
        self.updates.push((schema, key, value));
        Ok(())
    }

    fn commit(&mut self) -> Result<StateRoot, StorageError> {
        // todo!("actually commit")
        Ok(StateRoot::default())
    }

    fn calculate_root(&self) -> Result<StateRoot, StorageError> {
        Ok(StateRoot::default())
    }
}

pub struct StateDb<TStateDbUnitOfWork: StateDbUnitOfWork> {
    pd: PhantomData<TStateDbUnitOfWork>,
}

impl StateDb<StateDbUnitOfWorkImpl> {
    pub fn new() -> Self {
        Self { pd: Default::default() }
    }

    pub fn new_unit_of_work(&self) -> StateDbUnitOfWorkImpl {
        StateDbUnitOfWorkImpl { updates: vec![] }
        // let mut unit_of_work = self.current_unit_of_work_mut();
        // if unit_of_work.is_none() {
        //     self.unit_of_work = Some(StateDbUnitOfWork {});
        //     unit_of_work = self.unit_of_work
        // };
        // unit_of_work.as_mut().unwrap()
    }
}
