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

//! Common test helper functions that are small and useful enough to be included in the main crate, rather than the
//! integration test folder.

mod mock_backend;

use crate::{
    blocks::{Block, BlockBuilder, BlockHeader},
    chain_storage::{BlockchainDatabase, MemoryDatabase, Validators},
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::{transaction::Transaction, types::HashDigest},
    validation::mocks::MockValidator,
};

pub use mock_backend::MockBackend;

/// Create a partially constructed block using the provided set of transactions
/// is chain_block, or rename it to `create_orphan_block` and drop the prev_block argument
pub fn create_orphan_block(
    block_height: u64,
    transactions: Vec<Transaction>,
    consensus_constants: &ConsensusConstants,
) -> Block
{
    let mut header = BlockHeader::new(consensus_constants.blockchain_version());
    header.height = block_height;
    header.into_builder().with_transactions(transactions).build()
}

pub fn create_mem_db(
    consensus_manager: &ConsensusManager<MemoryDatabase<HashDigest>>,
) -> BlockchainDatabase<MemoryDatabase<HashDigest>> {
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let db = MemoryDatabase::<HashDigest>::default();
    let mut db = BlockchainDatabase::new(db, consensus_manager).unwrap();
    db.set_validators(validators);
    db
}
