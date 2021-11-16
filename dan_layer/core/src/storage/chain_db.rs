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

use crate::{
    models::{
        Instruction,
        QuorumCertificate,
        TreeNodeHash,
    },
    storage::{BackendAdapter, StorageError, UnitOfWork, UnitOfWorkTracker},
};
use std::{
    fmt::{Debug, Formatter},
    sync::{Arc, RwLock},
};

pub struct ChainDb<TBackendAdapter: BackendAdapter> {
    adapter: TBackendAdapter,
}

impl<TBackendAdapter: BackendAdapter> ChainDb<TBackendAdapter> {
    pub fn new(adapter: TBackendAdapter) -> ChainDb<TBackendAdapter> {
        ChainDb { adapter }
    }

    pub fn find_highest_prepared_qc(&self) -> Result<QuorumCertificate, StorageError> {
        self.adapter
            .find_highest_prepared_qc()
            .map_err(TBackendAdapter::Error::into)
    }

    pub fn get_locked_qc(&self) -> Result<QuorumCertificate, StorageError> {
        self.adapter.get_locked_qc().map_err(TBackendAdapter::Error::into)
    }
}

impl<TBackendAdapter: BackendAdapter + Clone + Send + Sync> ChainDb<TBackendAdapter> {
    pub fn new_unit_of_work(&self) -> ChainDbUnitOfWork<TBackendAdapter> {
        ChainDbUnitOfWork {
            inner: Arc::new(RwLock::new(ChainDbUnitOfWorkInner::new(self.adapter.clone()))),
        }
    }
}
impl<TBackendAdapter: BackendAdapter> ChainDb<TBackendAdapter> {
    pub fn is_empty(&self) -> Result<bool, StorageError> {
        self.adapter.is_empty().map_err(TBackendAdapter::Error::into)
    }
}

// Cloneable, Send, Sync wrapper
pub struct ChainDbUnitOfWork<TBackendAdapter: BackendAdapter> {
    inner: Arc<RwLock<ChainDbUnitOfWorkInner<TBackendAdapter>>>,
}

impl<TBackendAdapter: BackendAdapter> ChainDbUnitOfWork<TBackendAdapter> {
    fn set_dirty(
        &mut self,
        _item: (Option<TBackendAdapter::Id>, UnitOfWorkTracker),
    ) -> Result<(Option<TBackendAdapter::Id>, &mut UnitOfWorkTracker), StorageError> {
        //     let inner = self.inner.write().unwrap();
        // for (clean_id, clean_item) in inner.drain(..) {
        //     if clean_id == item.0 {
        //
        //     }
        // }
        // for (dirty_id, dirty_item) in &inner.dirty_items {
        // match dirty_item {
        // UnitOfWorkTracker::Node { hash, .. } => {
        // if hash == node_hash {
        // return Ok((*dirty_id, dirty_item.clone()));
        // }
        // },
        // _ => (),
        // };
        todo!()
    }

    fn find_proposed_node(
        &mut self,
        node_hash: &TreeNodeHash,
    ) -> Result<(Option<TBackendAdapter::Id>, UnitOfWorkTracker), StorageError> {
        let mut inner = self.inner.write().unwrap();
        dbg!(&inner);

        for (id, item, _is_dirty) in &inner.items {
            match item {
                UnitOfWorkTracker::Node { hash, .. } => {
                    if hash == node_hash {
                        return Ok((*id, item.clone()));
                    }
                },
                _ => (),
            };
        }
        // finally hit the db
        let (id, item) = inner
            .backend_adapter
            .find_node_by_hash(node_hash)
            .map_err(TBackendAdapter::Error::into)?;
        inner.items.push((Some(id), item.clone(), IsDirty::Clean));
        Ok((Some(id), item))
    }
}

#[derive(Debug, Copy, Clone)]
pub enum IsDirty {
    Clean,
    Dirty,
}

impl IsDirty {
    pub fn is_dirty(&self) -> bool {
        matches!(self, _Dirty)
    }
}

pub struct ChainDbUnitOfWorkInner<TBackendAdapter: BackendAdapter> {
    backend_adapter: TBackendAdapter,
    items: Vec<(Option<TBackendAdapter::Id>, UnitOfWorkTracker, IsDirty)>,
}

impl<T: BackendAdapter> Debug for ChainDbUnitOfWorkInner<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Items:{:?}", self.items)
    }
}
impl<TBackendAdapter: BackendAdapter> ChainDbUnitOfWorkInner<TBackendAdapter> {
    pub fn new(backend_adapter: TBackendAdapter) -> Self {
        Self {
            backend_adapter,
            items: vec![],
        }
    }

    pub fn set_dirty(&mut self, id: Option<TBackendAdapter::Id>, item: &UnitOfWorkTracker) {
        for i in 0..self.items.len() {
            if self.items[i].0 == id && &self.items[i].1 == item {
                self.items[i].2 = IsDirty::Dirty;
            }
        }
    }
}

impl<TBackendAdapter: BackendAdapter> Clone for ChainDbUnitOfWork<TBackendAdapter> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<TBackendAdapter: BackendAdapter> UnitOfWork for ChainDbUnitOfWork<TBackendAdapter> {
    // pub fn register_clean(&mut self, item: UnitOfWorkTracker) {
    //     self.clean.push(item);
    // }

    fn commit(&mut self) -> Result<(), StorageError> {
        let inner = self.inner.write().unwrap();
        let tx = inner
            .backend_adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;

        for (id, item, is_dirty) in inner.items.iter() {
            if is_dirty.is_dirty() {
                match id {
                    Some(i) => inner
                        .backend_adapter
                        .update(i, item, &tx)
                        .map_err(TBackendAdapter::Error::into)?,
                    None => inner
                        .backend_adapter
                        .insert(item, &tx)
                        .map_err(TBackendAdapter::Error::into)?,
                }
            }
        }

        inner
            .backend_adapter
            .commit(&tx)
            .map_err(TBackendAdapter::Error::into)?;
        Ok(())
    }

    fn add_node(&mut self, hash: TreeNodeHash, parent: TreeNodeHash, height: u32) -> Result<(), StorageError> {
        self.inner.write().unwrap().items.push((
            None,
            UnitOfWorkTracker::Node {
                hash,
                parent,
                height,
                is_committed: false,
            },
            IsDirty::Dirty,
        ));
        Ok(())
    }

    fn add_instruction(&mut self, node_hash: TreeNodeHash, instruction: Instruction) -> Result<(), StorageError> {
        self.inner.write().unwrap().items.push((
            None,
            UnitOfWorkTracker::Instruction { node_hash, instruction },
            IsDirty::Dirty,
        ));
        Ok(())
    }

    fn get_locked_qc(&mut self) -> Result<QuorumCertificate, StorageError> {
        let mut inner = self.inner.write().unwrap();
        let _id = inner.backend_adapter.locked_qc_id();

        for (_, item, _) in &inner.items {
            match item {
                UnitOfWorkTracker::LockedQc {
                    message_type,
                    view_number,
                    node_hash,
                    signature,
                } => {
                    return Ok(QuorumCertificate::new(
                        *message_type,
                        *view_number,
                        node_hash.clone(),
                        signature.clone(),
                    ));
                },
                _ => (),
            };
        }

        // finally hit the db
        let qc = inner
            .backend_adapter
            .get_locked_qc()
            .map_err(TBackendAdapter::Error::into)?;
        let id = inner.backend_adapter.locked_qc_id();
        inner.items.push((
            Some(id),
            UnitOfWorkTracker::LockedQc {
                message_type: qc.message_type(),
                view_number: qc.view_number(),
                node_hash: qc.node_hash().clone(),
                signature: qc.signature().cloned(),
            },
            IsDirty::Clean,
        ));
        Ok(qc)
    }

    fn set_locked_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError> {
        let mut inner = self.inner.write().unwrap();
        let id = inner.backend_adapter.locked_qc_id();
        inner.items.push((
            Some(id),
            UnitOfWorkTracker::LockedQc {
                message_type: qc.message_type(),
                view_number: qc.view_number(),
                node_hash: qc.node_hash().clone(),
                signature: qc.signature().cloned(),
            },
            IsDirty::Dirty,
        ));
        Ok(())
    }

    fn set_prepare_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError> {
        let mut inner = self.inner.write().unwrap();
        let id = inner.backend_adapter.prepare_qc_id();
        inner.items.push((
            Some(id),
            UnitOfWorkTracker::PrepareQc {
                message_type: qc.message_type(),
                view_number: qc.view_number(),
                node_hash: qc.node_hash().clone(),
                signature: qc.signature().cloned(),
            },
            IsDirty::Dirty,
        ));
        Ok(())
    }

    fn commit_node(&mut self, node_hash: &TreeNodeHash) -> Result<(), StorageError> {
        let (id, mut item) = self.find_proposed_node(node_hash)?;
        let mut inner = self.inner.write().unwrap();
        inner.set_dirty(id, &item);
        match &mut item {
            UnitOfWorkTracker::Node { is_committed, .. } => *is_committed = true,
            _ => return Err(StorageError::InvalidUnitOfWorkTrackerType),
        }
        Ok(())
    }
}
