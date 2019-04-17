// Copyright 2018 The Tari Project
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

use crate::{blockchainstate::BlockchainState, error::ChainError, store::Store};
use tari_core::block::Block;

use std::collections::HashMap;

type BlockHash = [u8; 32];

/// The Chain is the actual data structure to represent the blockchain
pub struct Chain {
    /// This is the database used to storepersistentt data in
    pub store: Store,
    /// This the the current UTXO set, kernels and headers
    pub blockchainstate: BlockchainState,
    /// This is all valid blocks which dont have a parent trace to the genesis block
    pub orphans: HashMap<BlockHash, Block>,
    /// This is our pruning horizon
    pub pruning_horizon: Option<u64>,
}

impl Chain {
    pub fn new(dbstore: Store, pruning_horizon: Option<u64>) -> Chain {
        Chain {
            store: dbstore,
            blockchainstate: BlockchainState::new(),
            orphans: HashMap::new(),
            pruning_horizon,
        }
    }

    /// This function will process a newly receivedd block
    pub fn process_new_block(&self, new_block: &Block) -> Result<(), ChainError> {
        self.validate_new_block(new_block)
    }

    /// This block will validate the block and enforce the consensus rules on the block that dont require looking at
    /// state, the all transactions have been signed, count up to zero commitments etc)
    pub fn validate_new_block(&self, _new_block: &Block) -> Result<(), ChainError> {
        Ok(())
    }
}
