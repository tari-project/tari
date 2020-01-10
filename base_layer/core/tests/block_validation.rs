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

use std::sync::Arc;
use tari_core::{
    blocks::genesis_block::get_genesis_block,
    chain_storage::{BlockchainDatabase, MemoryDatabase, Validators},
    consensus::ConsensusManager,
    proof_of_work::DiffAdjManager,
    validation::block_validators::{FullConsensusValidator, StatelessValidator},
};
use tari_transactions::types::{CryptoFactories, HashDigest};

#[test]
fn test_genesis_block() {
    let factories = Arc::new(CryptoFactories::default());
    let rules = ConsensusManager::default();
    let backend = MemoryDatabase::<HashDigest>::default();
    let mut db = BlockchainDatabase::new(backend).unwrap();
    let validators = Validators::new(
        FullConsensusValidator::new(rules.clone(), factories.clone(), db.clone()),
        StatelessValidator::new(factories.clone()),
    );
    db.set_validators(validators);
    let diff_adj_manager = DiffAdjManager::new(db.clone()).unwrap();
    rules.set_diff_manager(diff_adj_manager).unwrap();
    let block = get_genesis_block();
    let result = db.add_block(block);
    dbg!(&result);
    assert!(result.is_ok());
}
