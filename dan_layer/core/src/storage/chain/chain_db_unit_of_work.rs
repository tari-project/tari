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
    fmt::{Debug, Formatter},
    ops::DerefMut,
    sync::{Arc, RwLock},
};

use log::*;

use crate::{
    models::{Instruction, Node, QuorumCertificate, TreeNodeHash},
    storage::{
        chain::{db_node::DbNode, ChainDbBackendAdapter, DbInstruction, DbQc},
        unit_of_work_tracker::UnitOfWorkTracker,
        StorageError,
    },
};

const LOG_TARGET: &str = "tari::dan::chain_db::unit_of_work";

pub trait ChainDbUnitOfWork: Clone + Send + Sync {
    fn commit(&mut self) -> Result<(), StorageError>;
    fn add_node(&mut self, hash: TreeNodeHash, parent: TreeNodeHash, height: u32) -> Result<(), StorageError>;
    fn add_instruction(&mut self, node_hash: TreeNodeHash, instruction: Instruction) -> Result<(), StorageError>;
    fn get_locked_qc(&mut self) -> Result<QuorumCertificate, StorageError>;
    fn set_locked_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError>;
    fn get_prepare_qc(&mut self) -> Result<Option<QuorumCertificate>, StorageError>;
    fn set_prepare_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError>;
    fn commit_node(&mut self, node_hash: &TreeNodeHash) -> Result<(), StorageError>;
    // fn find_proposed_node(&mut self, node_hash: TreeNodeHash) -> Result<(Self::Id, UnitOfWorkTracker), StorageError>;
    fn get_tip_node(&self) -> Result<Option<Node>, StorageError>;
}

// Cloneable, Send, Sync wrapper
pub struct ChainDbUnitOfWorkImpl<TBackendAdapter: ChainDbBackendAdapter> {
    inner: Arc<RwLock<ChainDbUnitOfWorkInner<TBackendAdapter>>>,
}

impl<TBackendAdapter: ChainDbBackendAdapter> ChainDbUnitOfWorkImpl<TBackendAdapter> {
    pub fn new(adapter: TBackendAdapter) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ChainDbUnitOfWorkInner::new(adapter))),
        }
    }
}

impl<TBackendAdapter: ChainDbBackendAdapter> Clone for ChainDbUnitOfWorkImpl<TBackendAdapter> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<TBackendAdapter: ChainDbBackendAdapter> ChainDbUnitOfWork for ChainDbUnitOfWorkImpl<TBackendAdapter> {
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
                    Some(_i) => {
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
                    .map_err(TBackendAdapter::Error::into)?;
            }
        }

        if let Some(ref prepare_qc) = inner.prepare_qc {
            if prepare_qc.is_dirty() {
                inner
                    .backend_adapter
                    .update_prepare_qc(&*prepare_qc.get(), &tx)
                    .map_err(TBackendAdapter::Error::into)?;
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
                locked_qc.node_hash,
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
                node_hash: *qc.node_hash(),
                signature: qc.signature().cloned(),
            },
            false,
        ));
        Ok(qc)
    }

    fn set_locked_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError> {
        let mut inner = self.inner.write().unwrap();

        if let Some(locked_qc) = &inner.locked_qc.as_ref() {
            let mut locked_qc = locked_qc.get_mut();
            locked_qc.message_type = qc.message_type();
            locked_qc.view_number = qc.view_number();
            locked_qc.node_hash = *qc.node_hash();
            locked_qc.signature = qc.signature().cloned();
        } else {
            inner.locked_qc = Some(UnitOfWorkTracker::new(
                DbQc {
                    message_type: qc.message_type(),
                    view_number: qc.view_number(),
                    node_hash: *qc.node_hash(),
                    signature: qc.signature().cloned(),
                },
                true,
            ));
        }

        debug!(
            target: LOG_TARGET,
            "Marking proposed node '{}' as committed",
            qc.node_hash()
        );
        let found_node = inner.find_proposed_node(qc.node_hash())?;
        let mut node = found_node.1.get_mut();
        let mut n = node.deref_mut();
        n.is_committed = true;
        Ok(())
    }

    fn get_prepare_qc(&mut self) -> Result<Option<QuorumCertificate>, StorageError> {
        let mut inner = self.inner.write().unwrap();

        if let Some(prepare_qc) = &inner.prepare_qc {
            let prepare_qc = prepare_qc.get();
            return Ok(Some(QuorumCertificate::new(
                prepare_qc.message_type,
                prepare_qc.view_number,
                prepare_qc.node_hash,
                prepare_qc.signature.clone(),
            )));
        }

        // finally hit the db
        let qc = inner
            .backend_adapter
            .get_prepare_qc()
            .map_err(TBackendAdapter::Error::into)?;

        inner.prepare_qc = qc.as_ref().map(|qc| {
            UnitOfWorkTracker::new(
                DbQc {
                    message_type: qc.message_type(),
                    view_number: qc.view_number(),
                    node_hash: *qc.node_hash(),
                    signature: qc.signature().cloned(),
                },
                false,
            )
        });
        Ok(qc)
    }

    fn set_prepare_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError> {
        // put it in the tracker
        let _ = self.get_prepare_qc()?;
        let mut inner = self.inner.write().unwrap();
        match inner.prepare_qc.as_mut() {
            None => {
                inner.prepare_qc = Some(UnitOfWorkTracker::new(
                    DbQc {
                        message_type: qc.message_type(),
                        view_number: qc.view_number(),
                        node_hash: *qc.node_hash(),
                        signature: qc.signature().cloned(),
                    },
                    true,
                ));
            },
            Some(db_qc) => {
                let mut db_qc = db_qc.get_mut();
                db_qc.message_type = qc.message_type();
                db_qc.view_number = qc.view_number();
                db_qc.node_hash = *qc.node_hash();
                db_qc.signature = qc.signature().cloned();
            },
        }

        Ok(())
    }

    fn commit_node(&mut self, node_hash: &TreeNodeHash) -> Result<(), StorageError> {
        let mut inner = self.inner.write().unwrap();
        let found_node = inner.find_proposed_node(node_hash)?;
        let mut node = found_node.1.get_mut();
        node.is_committed = true;
        Ok(())
    }

    fn get_tip_node(&self) -> Result<Option<Node>, StorageError> {
        let inner = self.inner.read().unwrap();
        inner.get_tip_node()
    }
}

pub struct ChainDbUnitOfWorkInner<TBackendAdapter: ChainDbBackendAdapter> {
    backend_adapter: TBackendAdapter,
    nodes: Vec<(Option<TBackendAdapter::Id>, UnitOfWorkTracker<DbNode>)>,
    instructions: Vec<(Option<TBackendAdapter::Id>, UnitOfWorkTracker<DbInstruction>)>,
    locked_qc: Option<UnitOfWorkTracker<DbQc>>,
    prepare_qc: Option<UnitOfWorkTracker<DbQc>>,
}

impl<T: ChainDbBackendAdapter> Debug for ChainDbUnitOfWorkInner<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Nodes:{:?}", self.nodes)
    }
}
impl<TBackendAdapter: ChainDbBackendAdapter> ChainDbUnitOfWorkInner<TBackendAdapter> {
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
            .map_err(TBackendAdapter::Error::into)?
            .ok_or(StorageError::NotFound)?;
        let tracker = UnitOfWorkTracker::new(item, false);
        self.nodes.push((Some(id), tracker.clone()));
        Ok((Some(id), tracker))
    }

    pub fn get_tip_node(&self) -> Result<Option<Node>, StorageError> {
        let node = self
            .backend_adapter
            .get_tip_node()
            .map_err(TBackendAdapter::Error::into)?;
        Ok(node.map(Into::into))
    }
}
