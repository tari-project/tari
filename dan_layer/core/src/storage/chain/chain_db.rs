// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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
    models::{Node, QuorumCertificate, SideChainBlock, TreeNodeHash},
    storage::{
        chain::{chain_db_unit_of_work::ChainDbUnitOfWorkImpl, ChainDbBackendAdapter, ChainDbMetadataKey},
        MetadataBackendAdapter,
        StorageError,
    },
};

pub struct ChainDb<TBackendAdapter> {
    adapter: TBackendAdapter,
}

impl<TBackendAdapter: ChainDbBackendAdapter> ChainDb<TBackendAdapter> {
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

    pub fn is_empty(&self) -> Result<bool, StorageError> {
        self.adapter.is_empty().map_err(TBackendAdapter::Error::into)
    }

    pub fn sidechain_block_exists(&self, hash: &TreeNodeHash) -> Result<bool, StorageError> {
        self.adapter.node_exists(hash).map_err(TBackendAdapter::Error::into)
    }

    pub fn find_sidechain_block_by_node_hash(
        &self,
        hash: &TreeNodeHash,
    ) -> Result<Option<SideChainBlock>, StorageError> {
        let maybe_block = self
            .adapter
            .find_node_by_hash(hash)
            .map_err(TBackendAdapter::Error::into)?;

        let (block_id, node) = match maybe_block {
            Some(v) => v,
            None => return Ok(None),
        };

        let instructions = self
            .adapter
            .find_all_instructions_by_node(block_id)
            .map_err(TBackendAdapter::Error::into)?;
        let instructions = instructions.into_iter().map(|i| i.instruction).collect();

        Ok(Some(SideChainBlock::new(node.into(), instructions)))
    }

    pub fn find_sidechain_block_by_parent_node_hash(
        &self,
        parent_hash: &TreeNodeHash,
    ) -> Result<Option<SideChainBlock>, StorageError> {
        let maybe_block = self
            .adapter
            .find_node_by_parent_hash(parent_hash)
            .map_err(TBackendAdapter::Error::into)?;
        let (block_id, node) = match maybe_block {
            Some(v) => v,
            None => return Ok(None),
        };

        let instructions = self
            .adapter
            .find_all_instructions_by_node(block_id)
            .map_err(TBackendAdapter::Error::into)?;
        let instructions = instructions.into_iter().map(|i| i.instruction).collect();

        Ok(Some(SideChainBlock::new(node.into(), instructions)))
    }

    pub fn get_tip_node(&self) -> Result<Option<Node>, StorageError> {
        let db_node = self.adapter.get_tip_node().map_err(TBackendAdapter::Error::into)?;
        Ok(db_node.map(Into::into))
    }
}

impl<TBackendAdapter> ChainDb<TBackendAdapter>
where TBackendAdapter: MetadataBackendAdapter<ChainDbMetadataKey>
{
    pub fn get_current_checkpoint_number(&self) -> Result<u64, StorageError> {
        let tx = self
            .adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;
        let number = self
            .adapter
            .get_metadata(&ChainDbMetadataKey::CheckpointNumber, &tx)
            .map_err(TBackendAdapter::Error::into)?
            .unwrap_or(0);
        Ok(number)
    }

    /// Increments checkpoint number and returns the incremented value. If the key did not previously exist, it
    /// is created and set to 1.
    pub fn increment_checkpoint_number(&self) -> Result<u64, StorageError> {
        let tx = self
            .adapter
            .create_transaction()
            .map_err(TBackendAdapter::Error::into)?;
        const KEY: ChainDbMetadataKey = ChainDbMetadataKey::CheckpointNumber;
        let n = self
            .adapter
            .get_metadata::<u64>(&KEY, &tx)
            .map_err(TBackendAdapter::Error::into)?
            .unwrap_or(0);
        let next = n + 1;
        self.adapter
            .set_metadata(KEY, next, &tx)
            .map_err(TBackendAdapter::Error::into)?;
        self.adapter.commit(&tx).map_err(TBackendAdapter::Error::into)?;
        Ok(next)
    }
}

impl<TBackendAdapter: ChainDbBackendAdapter + Clone + Send + Sync> ChainDb<TBackendAdapter> {
    pub fn new_unit_of_work(&self) -> ChainDbUnitOfWorkImpl<TBackendAdapter> {
        ChainDbUnitOfWorkImpl::new(self.adapter.clone())
    }
}
