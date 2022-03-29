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

use std::fmt::Debug;

use crate::{
    models::{Payload, QuorumCertificate, TreeNodeHash},
    storage::{
        chain::{DbInstruction, DbNode, DbQc},
        StorageError,
    },
};

pub trait ChainDbBackendAdapter: Send + Sync + Clone {
    type BackendTransaction;
    type Error: Into<StorageError>;
    type Id: Copy + Send + Sync + Debug + PartialEq;
    type Payload: Payload;

    fn is_empty(&self) -> Result<bool, Self::Error>;
    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error>;
    fn node_exists(&self, node_hash: &TreeNodeHash) -> Result<bool, Self::Error>;
    fn get_tip_node(&self) -> Result<Option<DbNode>, Self::Error>;
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
    fn get_prepare_qc(&self) -> Result<Option<QuorumCertificate>, Self::Error>;
    fn find_node_by_hash(&self, node_hash: &TreeNodeHash) -> Result<Option<(Self::Id, DbNode)>, Self::Error>;
    fn find_node_by_parent_hash(&self, parent_hash: &TreeNodeHash) -> Result<Option<(Self::Id, DbNode)>, Self::Error>;
    fn find_all_instructions_by_node(&self, node_id: Self::Id) -> Result<Vec<DbInstruction>, Self::Error>;
    fn update_prepare_qc(&self, item: &DbQc, transaction: &Self::BackendTransaction) -> Result<(), Self::Error>;
    fn update_locked_qc(&self, locked_qc: &DbQc, transaction: &Self::BackendTransaction) -> Result<(), Self::Error>;
}
