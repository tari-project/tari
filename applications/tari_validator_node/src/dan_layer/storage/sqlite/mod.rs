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

use crate::dan_layer::{
    models::{HotStuffTreeNode, QuorumCertificate, SidechainMetadata, TariDanPayload},
    storage::{BackendAdapter, ChainDbUnitOfWork, ChainStorageService, NewUnitOfWorkTracker, StorageError, UnitOfWork},
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SqliteStorageService {}

#[async_trait]
impl ChainStorageService<TariDanPayload> for SqliteStorageService {
    async fn get_metadata(&self) -> Result<SidechainMetadata, StorageError> {
        todo!()
    }

    async fn save_node<TUnitOfWork: UnitOfWork>(
        &self,
        node: &HotStuffTreeNode<TariDanPayload>,
        db: TUnitOfWork,
    ) -> Result<(), StorageError> {
        let mut db = db;
        for instruction in node.payload().instructions() {
            db.add_instruction(node.hash().clone(), instruction.clone())?;
        }
        db.add_node(node.hash().clone(), node.parent().clone())?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct SqliteBackendAdapter {}

pub struct SqliteTransaction {}

impl BackendAdapter for SqliteBackendAdapter {
    type BackendTransaction = SqliteTransaction;

    fn create_transaction(&self) -> Self::BackendTransaction {
        SqliteTransaction {}
    }

    fn insert(&self, item: &NewUnitOfWorkTracker, transaction: &Self::BackendTransaction) -> Result<(), StorageError> {
        todo!()
    }

    fn commit(&self, transaction: &Self::BackendTransaction) -> Result<(), StorageError> {
        todo!()
    }
}
