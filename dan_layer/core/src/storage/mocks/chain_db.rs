//  Copyright 2022, The Tari Project
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

use super::MemoryChainDb;
use crate::{
    models::{QuorumCertificate, TreeNodeHash},
    storage::{
        chain::{ChainDbBackendAdapter, DbInstruction, DbNode, DbQc},
        StorageError,
    },
};

#[derive(Debug, Clone, Default)]
pub struct MockChainDbBackupAdapter {
    db: Arc<RwLock<MemoryChainDb>>,
}

impl MockChainDbBackupAdapter {
    pub fn new() -> Self {
        Self { db: Default::default() }
    }
}

impl ChainDbBackendAdapter for MockChainDbBackupAdapter {
    type BackendTransaction = ();
    type Error = StorageError;
    type Id = usize;
    type Payload = String;

    fn is_empty(&self) -> Result<bool, Self::Error> {
        let lock = self.db.read().unwrap();
        Ok(lock.nodes.is_empty())
    }

    fn node_exists(&self, node_hash: &TreeNodeHash) -> Result<bool, Self::Error> {
        let lock = self.db.read().unwrap();
        let exists = lock.nodes.rows().any(|rec| rec.hash == *node_hash);
        Ok(exists)
    }

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error> {
        Ok(())
    }

    fn insert_node(&self, node: &DbNode, _: &Self::BackendTransaction) -> Result<(), Self::Error> {
        let mut lock = self.db.write().unwrap();
        lock.nodes.insert(node.clone());
        Ok(())
    }

    fn update_node(&self, id: &Self::Id, item: &DbNode, _: &Self::BackendTransaction) -> Result<(), Self::Error> {
        let mut lock = self.db.write().unwrap();
        if lock.nodes.update(*id, item.clone()) {
            Ok(())
        } else {
            Err(StorageError::NotFound)
        }
    }

    fn insert_instruction(&self, item: &DbInstruction, _: &Self::BackendTransaction) -> Result<(), Self::Error> {
        let mut lock = self.db.write().unwrap();
        lock.instructions.insert(item.clone());
        Ok(())
    }

    fn commit(&self, _: &Self::BackendTransaction) -> Result<(), Self::Error> {
        Ok(())
    }

    fn locked_qc_id(&self) -> Self::Id {
        1
    }

    fn prepare_qc_id(&self) -> Self::Id {
        1
    }

    fn find_highest_prepared_qc(&self) -> Result<QuorumCertificate, Self::Error> {
        let lock = self.db.read().unwrap();
        let highest = lock
            .prepare_qc
            .rows()
            .fold(None, |found: Option<&DbQc>, rec| match found {
                Some(r) if rec.view_number > r.view_number => Some(rec),
                Some(r) => Some(r),
                None => Some(rec),
            })
            .ok_or(StorageError::NotFound)?;

        Ok(highest.clone().into())
    }

    fn get_locked_qc(&self) -> Result<QuorumCertificate, Self::Error> {
        let lock = self.db.read().unwrap();
        // FIXME: when this implementation is finalized in sqlite/lmdb impl
        let rec = lock.locked_qc.rows().next().cloned().map(Into::into).unwrap();
        Ok(rec)
    }

    fn get_prepare_qc(&self) -> Result<Option<QuorumCertificate>, Self::Error> {
        let lock = self.db.read().unwrap();
        // FIXME: when this implementation is finalized in sqlite/lmdb impl
        let rec = lock.prepare_qc.rows().next().cloned().map(Into::into);
        Ok(rec)
    }

    fn find_node_by_hash(&self, node_hash: &TreeNodeHash) -> Result<Option<(Self::Id, DbNode)>, Self::Error> {
        let lock = self.db.read().unwrap();
        let recs = lock
            .nodes
            .records()
            .find(|(_, rec)| rec.hash == *node_hash)
            .map(|(id, node)| (id, node.clone()));
        Ok(recs)
    }

    fn find_node_by_parent_hash(&self, parent_hash: &TreeNodeHash) -> Result<Option<(Self::Id, DbNode)>, Self::Error> {
        let lock = self.db.read().unwrap();
        let rec = lock
            .nodes
            .records()
            .find(|(_, rec)| rec.parent == *parent_hash)
            .map(|(id, node)| (id, node.clone()));
        Ok(rec)
    }

    fn find_all_instructions_by_node(&self, node_id: Self::Id) -> Result<Vec<DbInstruction>, Self::Error> {
        let lock = self.db.read().unwrap();
        let node = lock.nodes.get(node_id).ok_or(StorageError::NotFound)?;
        let recs = lock
            .instructions
            .rows()
            .filter(|rec| rec.node_hash == node.hash)
            .cloned()
            .collect();
        Ok(recs)
    }

    fn update_prepare_qc(&self, item: &DbQc, _transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        let mut lock = self.db.write().unwrap();
        let id = lock
            .prepare_qc
            .records()
            .next()
            .map(|(id, _)| id)
            .ok_or(StorageError::NotFound)?;
        lock.prepare_qc.update(id, item.clone());
        Ok(())
    }

    fn update_locked_qc(&self, locked_qc: &DbQc, _transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        let mut lock = self.db.write().unwrap();
        let id = lock
            .locked_qc
            .records()
            .next()
            .map(|(id, _)| id)
            .ok_or(StorageError::NotFound)?;
        lock.locked_qc.update(id, locked_qc.clone());
        Ok(())
    }

    fn get_tip_node(&self) -> Result<Option<DbNode>, Self::Error> {
        let lock = self.db.read().unwrap();
        let found = lock
            .nodes
            .rows()
            .fold(None, |val: Option<&DbNode>, row| match val {
                Some(v) if v.height < row.height => Some(row),
                Some(v) => Some(v),
                None => Some(row),
            })
            .cloned();

        Ok(found)
    }
}
