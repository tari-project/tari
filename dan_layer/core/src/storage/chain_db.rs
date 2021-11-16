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
    models::{HotStuffMessageType, HotStuffTreeNode, Instruction, QuorumCertificate, Signature, TreeNodeHash, ViewId},
    storage::{BackendAdapter, StorageError, UnitOfWork, UnitOfWorkTracker},
};
use std::{
    fmt::{Debug, Formatter},
    ops::{Deref, DerefMut},
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

#[derive(Debug)]
pub struct DbNode {
    pub hash: TreeNodeHash,
    pub parent: TreeNodeHash,
    pub height: u32,
    pub is_committed: bool,
}

#[derive(Debug)]
pub struct DbInstruction {
    pub instruction: Instruction,
    pub node_hash: TreeNodeHash,
}

#[derive(Debug)]
pub struct DbQc {
    pub message_type: HotStuffMessageType,
    pub view_number: ViewId,
    pub node_hash: TreeNodeHash,
    pub signature: Option<Signature>,
}

pub struct ChainDbUnitOfWorkInner<TBackendAdapter: BackendAdapter> {
    backend_adapter: TBackendAdapter,
    nodes: Vec<(Option<TBackendAdapter::Id>, UnitOfWorkTracker<DbNode>)>,
    instructions: Vec<(Option<TBackendAdapter::Id>, UnitOfWorkTracker<DbInstruction>)>,
    locked_qc: Option<UnitOfWorkTracker<DbQc>>,
    prepare_qc: Option<UnitOfWorkTracker<DbQc>>,
}

impl<T: BackendAdapter> Debug for ChainDbUnitOfWorkInner<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Nodes:{:?}", self.nodes)
    }
}
impl<TBackendAdapter: BackendAdapter> ChainDbUnitOfWorkInner<TBackendAdapter> {
    pub fn new(backend_adapter: TBackendAdapter) -> Self {
        Self {
            backend_adapter,
            nodes: vec![],
            instructions: vec![],
            locked_qc: None,
            prepare_qc: None,
        }
    }

    pub fn find_proposed_node(
        &mut self,
        node_hash: &TreeNodeHash,
    ) -> Result<(Option<TBackendAdapter::Id>, UnitOfWorkTracker<DbNode>), StorageError> {
        for (id, item) in &self.nodes {
            if &item.get().hash == node_hash {
                return Ok((*id, item.clone()));
            }
        }
        // finally hit the db
        let (id, item) = self
            .backend_adapter
            .find_node_by_hash(node_hash)
            .map_err(TBackendAdapter::Error::into)?;
        let tracker = UnitOfWorkTracker::new(item, false);
        self.nodes.push((Some(id), tracker.clone()));
        Ok((Some(id), tracker))
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
        let mut inner = self.inner.write().unwrap();
        let tx = inner
            .backend_adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;

        for (id, item) in &inner.nodes {
            if item.is_dirty() {
                match id {
                    Some(i) => inner
                        .backend_adapter
                        .update_node(i, &*item.get(), &tx)
                        .map_err(TBackendAdapter::Error::into)?,
                    None => inner
                        .backend_adapter
                        .insert_node(&*item.get(), &tx)
                        .map_err(TBackendAdapter::Error::into)?,
                }
            }
        }

        for (id, item) in &inner.instructions {
            if item.is_dirty() {
                match id {
                    Some(i) => {
                        unimplemented!("Cannot update instructions");
                    },
                    None => inner
                        .backend_adapter
                        .insert_instruction(&*item.get(), &tx)
                        .map_err(TBackendAdapter::Error::into)?,
                }
            }
        }

        if let Some(ref locked_qc) = inner.locked_qc {
            if locked_qc.is_dirty() {
                inner
                    .backend_adapter
                    .update_locked_qc(&*locked_qc.get(), &tx)
                    .map_err(TBackendAdapter::Error::into);
            }
        }

        if let Some(ref prepare_qc) = inner.prepare_qc {
            if prepare_qc.is_dirty() {
                inner
                    .backend_adapter
                    .update_prepare_qc(&*prepare_qc.get(), &tx)
                    .map_err(TBackendAdapter::Error::into);
            }
        }

        inner
            .backend_adapter
            .commit(&tx)
            .map_err(TBackendAdapter::Error::into)?;

        inner.nodes = vec![];
        inner.instructions = vec![];
        Ok(())
    }

    fn add_node(&mut self, hash: TreeNodeHash, parent: TreeNodeHash, height: u32) -> Result<(), StorageError> {
        self.inner.write().unwrap().nodes.push((
            None,
            UnitOfWorkTracker::new(
                DbNode {
                    hash,
                    parent,
                    height,
                    is_committed: false,
                },
                true,
            ),
        ));
        Ok(())
    }

    fn add_instruction(&mut self, node_hash: TreeNodeHash, instruction: Instruction) -> Result<(), StorageError> {
        self.inner.write().unwrap().instructions.push((
            None,
            UnitOfWorkTracker::new(DbInstruction { node_hash, instruction }, true),
        ));
        Ok(())
    }

    fn get_locked_qc(&mut self) -> Result<QuorumCertificate, StorageError> {
        let mut inner = self.inner.write().unwrap();

        if let Some(locked_qc) = &inner.locked_qc {
            let locked_qc = locked_qc.get();
            return Ok(QuorumCertificate::new(
                locked_qc.message_type,
                locked_qc.view_number,
                locked_qc.node_hash.clone(),
                locked_qc.signature.clone(),
            ));
        }

        // finally hit the db
        let qc = inner
            .backend_adapter
            .get_locked_qc()
            .map_err(TBackendAdapter::Error::into)?;
        inner.locked_qc = Some(UnitOfWorkTracker::new(
            DbQc {
                message_type: qc.message_type(),
                view_number: qc.view_number(),
                node_hash: qc.node_hash().clone(),
                signature: qc.signature().cloned(),
            },
            false,
        ));
        Ok(qc)
    }

    fn set_locked_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError> {
        // put it in the tracker
        let _ = self.get_locked_qc()?;
        let mut inner = self.inner.write().unwrap();
        {
            let mut db_locked = inner.locked_qc.as_ref().unwrap().get_mut();

            db_locked.message_type = qc.message_type();
            db_locked.view_number = qc.view_number();
            db_locked.node_hash = qc.node_hash().clone();
            db_locked.signature = qc.signature().cloned();
        }
        let found_node = inner.find_proposed_node(qc.node_hash())?;
        let mut node = found_node.1.get_mut();
        let mut n = node.deref_mut();
        n.is_committed = true;
        dbg!(inner);
        Ok(())
    }

    fn get_prepare_qc(&mut self) -> Result<QuorumCertificate, StorageError> {
        let mut inner = self.inner.write().unwrap();

        if let Some(prepare_qc) = &inner.prepare_qc {
            let prepare_qc = prepare_qc.get();
            return Ok(QuorumCertificate::new(
                prepare_qc.message_type,
                prepare_qc.view_number,
                prepare_qc.node_hash.clone(),
                prepare_qc.signature.clone(),
            ));
        }

        // finally hit the db
        let qc = inner
            .backend_adapter
            .get_prepare_qc()
            .map_err(TBackendAdapter::Error::into)?;
        inner.prepare_qc = Some(UnitOfWorkTracker::new(
            DbQc {
                message_type: qc.message_type(),
                view_number: qc.view_number(),
                node_hash: qc.node_hash().clone(),
                signature: qc.signature().cloned(),
            },
            false,
        ));
        Ok(qc)
    }

    fn set_prepare_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError> {
        // put it in the tracker
        let _ = self.get_prepare_qc()?;
        let mut inner = self.inner.write().unwrap();
        let mut db_locked = inner.prepare_qc.as_ref().unwrap().get_mut();
        db_locked.message_type = qc.message_type();
        db_locked.view_number = qc.view_number();
        db_locked.node_hash = qc.node_hash().clone();
        db_locked.signature = qc.signature().cloned();
        Ok(())
    }

    fn commit_node(&mut self, node_hash: &TreeNodeHash) -> Result<(), StorageError> {
        let mut inner = self.inner.write().unwrap();
        let found_node = inner.find_proposed_node(node_hash)?;
        let mut node = found_node.1.get_mut();
        node.is_committed = true;
        Ok(())
    }
}
