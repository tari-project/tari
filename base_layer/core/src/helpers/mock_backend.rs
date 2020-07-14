// Copyright 2019. The Tari Project
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

use crate::{
    blocks::{Block, BlockHeader},
    chain_storage::{BlockchainBackend, ChainMetadata, ChainStorageError, DbKey, DbTransaction, DbValue, MmrTree},
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use tari_crypto::tari_utilities::epoch_time::EpochTime;
use tari_mmr::{Hash, MerkleCheckPoint, MerkleProof};

// This is a test backend. This is used so that the ConsensusManager can be called without actually having a backend.
// Calling this backend will result in a panic.
pub struct MockBackend;

impl BlockchainBackend for MockBackend {
    fn write(&mut self, _tx: DbTransaction) -> Result<(), ChainStorageError> {
        unimplemented!()
    }

    fn fetch(&self, _key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        unimplemented!()
    }

    fn contains(&self, _key: &DbKey) -> Result<bool, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_root(&self, _tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_only_root(&self, _tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        unimplemented!()
    }

    fn calculate_mmr_root(
        &self,
        _tree: MmrTree,
        _additions: Vec<HashOutput>,
        _deletions: Vec<HashOutput>,
    ) -> Result<HashOutput, ChainStorageError>
    {
        unimplemented!()
    }

    fn fetch_mmr_proof(&self, _tree: MmrTree, _pos: usize) -> Result<MerkleProof, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_checkpoint(&self, _tree: MmrTree, _index: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_node_count(&self, _tree: MmrTree, _height: u64) -> Result<u32, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_node(
        &self,
        _tree: MmrTree,
        _pos: u32,
        _hist_height: Option<u64>,
    ) -> Result<(Hash, bool), ChainStorageError>
    {
        unimplemented!()
    }

    fn fetch_mmr_nodes(
        &self,
        _tree: MmrTree,
        _pos: u32,
        _count: u32,
        _hist_height: Option<u64>,
    ) -> Result<Vec<(Vec<u8>, bool)>, ChainStorageError>
    {
        unimplemented!()
    }

    fn insert_mmr_node(&mut self, _tree: MmrTree, _hash: Hash, _deleted: bool) -> Result<(), ChainStorageError> {
        unimplemented!()
    }

    fn delete_mmr_node(&mut self, _tree: MmrTree, _hash: &Hash) -> Result<(), ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_leaf_index(&self, _tree: MmrTree, _hash: &Hash) -> Result<Option<u32>, ChainStorageError> {
        unimplemented!()
    }

    fn for_each_orphan<F>(&self, _f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, Block), ChainStorageError>),
    {
        unimplemented!()
    }

    fn get_orphan_count(&self) -> Result<usize, ChainStorageError> {
        unimplemented!()
    }

    fn for_each_kernel<F>(&self, _f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>),
    {
        unimplemented!()
    }

    fn for_each_header<F>(&self, _f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(u64, BlockHeader), ChainStorageError>),
    {
        unimplemented!()
    }

    fn for_each_utxo<F>(&self, _f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>),
    {
        unimplemented!()
    }

    fn fetch_last_header(&self) -> Result<Option<BlockHeader>, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_target_difficulties(
        &self,
        _pow_algo: PowAlgorithm,
        _height: u64,
        _block_window: usize,
    ) -> Result<Vec<(EpochTime, Difficulty)>, ChainStorageError>
    {
        unimplemented!()
    }

    fn count_utxos(&self) -> Result<usize, ChainStorageError> {
        unimplemented!()
    }

    fn count_kernels(&self) -> Result<usize, ChainStorageError> {
        unimplemented!()
    }
}
