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
    ChainHeight,
    HotStuffMessageType,
    HotStuffTreeNode,
    Instruction,
    Payload,
    QuorumCertificate,
    SidechainMetadata,
    Signature,
    TreeNodeHash,
    ViewId,
};
pub use chain_storage_service::ChainStorageService;
pub use error::StorageError;
pub use lmdb::{LmdbAssetBackend, LmdbAssetStore};
use std::sync::{Arc, RwLock};
pub use store::{AssetDataStore, AssetStore};

mod chain_db;
pub use chain_db::{ChainDb, ChainDbUnitOfWork};
mod chain_storage_service;
mod error;
pub mod lmdb;
mod store;

// feature sql

pub trait DbFactory<TBackendAdapter: BackendAdapter> {
    fn create(&self) -> Result<ChainDb<TBackendAdapter>, StorageError>;
}

pub enum UnitOfWorkTracker {
    SidechainMetadata,
    LockedQc {
        message_type: HotStuffMessageType,
        view_number: ViewId,
        node_hash: TreeNodeHash,
        signature: Option<Signature>,
    },
}

pub enum NewUnitOfWorkTracker {
    Node {
        hash: TreeNodeHash,
        parent: TreeNodeHash,
    },
    Instruction {
        instruction: Instruction,
        node_hash: TreeNodeHash,
    },
}

pub trait BackendAdapter: Send + Sync + Clone {
    type BackendTransaction;
    type Error: Into<StorageError>;
    type Id: Send + Sync;
    type Payload: Payload;

    fn is_empty(&self) -> Result<bool, Self::Error>;
    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error>;
    fn insert(&self, item: &NewUnitOfWorkTracker, transaction: &Self::BackendTransaction) -> Result<(), Self::Error>;
    fn update(
        &self,
        id: &Self::Id,
        item: &UnitOfWorkTracker,
        transaction: &Self::BackendTransaction,
    ) -> Result<(), Self::Error>;
    fn commit(&self, transaction: &Self::BackendTransaction) -> Result<(), Self::Error>;
    fn locked_qc_id(&self) -> Self::Id;
    fn find_highest_prepared_qc(&self) -> Result<QuorumCertificate, Self::Error>;
}

pub trait UnitOfWork: Clone + Send + Sync {
    fn commit(&mut self) -> Result<(), StorageError>;
    fn add_node(&mut self, hash: TreeNodeHash, parent: TreeNodeHash) -> Result<(), StorageError>;
    fn add_instruction(&mut self, node_hash: TreeNodeHash, instruction: Instruction) -> Result<(), StorageError>;
    fn set_locked_qc(
        &mut self,
        message_type: HotStuffMessageType,
        view_number: ViewId,
        node_hash: TreeNodeHash,
        signature: Option<Signature>,
    ) -> Result<(), StorageError>;
}

pub struct StateDb {
    unit_of_work: Option<StateDbUnitOfWork>,
}

impl StateDb {
    pub fn new_unit_of_work(&mut self) -> &mut StateDbUnitOfWork {
        self.unit_of_work = Some(StateDbUnitOfWork { child: None });
        self.unit_of_work.as_mut().unwrap()
        // let mut unit_of_work = self.current_unit_of_work_mut();
        // if unit_of_work.is_none() {
        //     self.unit_of_work = Some(StateDbUnitOfWork {});
        //     unit_of_work = self.unit_of_work
        // };
        // unit_of_work.as_mut().unwrap()
    }

    fn current_unit_of_work_mut(&mut self) -> Option<&mut StateDbUnitOfWork> {
        unimplemented!()
        // let mut result = self.unit_of_work.as_mut();
        // let mut child = result;
        // while let Some(c) = child {
        //     result = child;
        //     child = c.child.as_mut();
        // }
        //
        // return result;
    }
}

pub struct StateDbUnitOfWork {
    child: Option<Arc<RwLock<StateDbUnitOfWork>>>,
}

impl StateDbUnitOfWork {
    pub fn new_unit_of_work(&mut self) -> &mut StateDbUnitOfWork {
        // TODO: better implementation
        self
    }

    pub fn commit(&mut self) -> Result<(), StorageError> {
        Ok(())
    }
}
