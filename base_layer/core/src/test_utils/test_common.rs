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

// Used in tests only

use crate::{
    blocks::Block,
    chain_storage::{BlockchainBackend, ChainStorageError, DbKey, DbTransaction, DbValue, MmrTree, MutableMmrState},
};
use rand::{CryptoRng, Rng};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PK, SecretKey},
};
use tari_mmr::{Hash, MerkleCheckPoint, MerkleProof, MutableMmrLeafNodes};
use tari_transactions::{
    tari_amount::*,
    transaction::{OutputFeatures, TransactionInput, UnblindedOutput},
    types::{CommitmentFactory, HashOutput, PrivateKey, PublicKey},
};

pub struct TestParams {
    pub spend_key: PrivateKey,
    pub change_key: PrivateKey,
    pub offset: PrivateKey,
    pub nonce: PrivateKey,
    pub public_nonce: PublicKey,
}

impl TestParams {
    pub fn new<R: Rng + CryptoRng>(rng: &mut R) -> TestParams {
        let r = PrivateKey::random(rng);
        TestParams {
            spend_key: PrivateKey::random(rng),
            change_key: PrivateKey::random(rng),
            offset: PrivateKey::random(rng),
            public_nonce: PublicKey::from_secret_key(&r),
            nonce: r,
        }
    }
}

pub fn make_input<R: Rng + CryptoRng>(
    rng: &mut R,
    val: MicroTari,
    factory: &CommitmentFactory,
) -> (TransactionInput, UnblindedOutput)
{
    let key = PrivateKey::random(rng);
    let v = PrivateKey::from(val);
    let commitment = factory.commit(&key, &v);
    let input = TransactionInput::new(OutputFeatures::default(), commitment);
    (input, UnblindedOutput::new(val, key, None))
}

// This is a test backend. This is used so that the ConsensusManager can be called without actually having a backend.
// Calling this backend will result in a panic.
pub struct MockBackend;

impl BlockchainBackend for MockBackend {
    fn write(&self, _tx: DbTransaction) -> Result<(), ChainStorageError> {
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

    fn fetch_pruning_horizon(&self) -> Result<u64, ChainStorageError> {
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

    fn fetch_mmr_checkpoint(&self, _tree: MmrTree, _index: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_node(&self, _tree: MmrTree, _pos: u32) -> Result<(Hash, bool), ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_base_leaf_nodes(
        &self,
        _tree: MmrTree,
        _index: usize,
        _count: usize,
    ) -> Result<MutableMmrState, ChainStorageError>
    {
        unimplemented!()
    }

    fn fetch_mmr_base_leaf_node_count(&self, _tree: MmrTree) -> Result<usize, ChainStorageError> {
        unimplemented!()
    }

    fn restore_mmr(&self, _tree: MmrTree, _base_state: MutableMmrLeafNodes) -> Result<(), ChainStorageError> {
        unimplemented!()
    }

    fn for_each_orphan<F>(&self, _f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, Block), ChainStorageError>),
    {
        unimplemented!()
    }
}
