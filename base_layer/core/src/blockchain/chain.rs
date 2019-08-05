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

//! This file provides the structs and functions that persist the block chain state, and re-org logic. Because a re-org
//! can happen, we need to keep track of orphan blocks. The Merkle Mountain Range crate we use allows us to rewind
//! checkpoints. Internally it keeps track of what was changed between checkpoints. We use these rewind blocks in the
//! case where we need to do a re-org where a forked chain with a greater accumulated pow emerges. In the case of the
//! MMR, a checkpoint is equal to a block.
//!
//! The MMR also provides a method to save the MMR to disc. This is internally handled and we use LMDB to store the MMR.

use crate::{
    blockchain::{
        block_chain_state::BlockchainState,
        error::{ChainError, StateError},
    },
    blocks::block::Block,
    pow::*,
    types::*,
};
use std::collections::HashMap;
use tari_utilities::Hashable;

type BlockHash = [u8; 32];
pub const MAX_ORPHAN_AGE: u64 = 1000;

/// The Chain is the actual data structure to represent the blockchain
pub struct Chain {
    /// This the the current UTXO set, kernels and headers
    pub block_chain_state: BlockchainState,
    /// This is all valid blocks which dont have a parent trace to the genesis block
    orphans: HashMap<BlockHash, Block>,
    /// The current head's total proof of work
    pub current_total_pow: ProofOfWork,
}

impl Chain {
    pub fn new() -> Result<Chain, ChainError> {
        let chain = Chain {
            block_chain_state: BlockchainState::new()?,
            orphans: HashMap::new(),
            current_total_pow: ProofOfWork::default(),
        };
        Ok(chain)
    }

    /// This function will process a newly received block
    pub fn process_new_block(&mut self, new_block: Block) -> Result<(), ChainError> {
        let result = match self.block_chain_state.process_new_block(&new_block) {
            // block was processed fine and added to chain
            Ok(_) => {
                let height = new_block.header.height;
                self.current_total_pow = new_block.header.pow;
                self.orphans.retain(|_, b| height - b.header.height < MAX_ORPHAN_AGE);
                // todo search for new orphans that we might apply
                Ok(())
            },
            // block seems valid, but its orphaned
            Err(StateError::OrphanBlock) => self.orphaned_block(new_block),
            Err(e) => Err(ChainError::StateProcessingError(e)),
        };
        if result.is_err() {
            self.block_chain_state.reset_chain_state()?;
        }
        result
    }

    /// Internal helper function to do orphan logic
    /// The function will store the new orphan block and check if it contains a re-org
    fn orphaned_block(&mut self, new_block: Block) -> Result<(), ChainError> {
        if self.orphans.contains_key(&new_block.header.hash()[..]) {
            return Err(ChainError::StateProcessingError(StateError::DuplicateBlock));
        };
        let mut hash = [0; 32];
        hash.copy_from_slice(&new_block.header.hash());
        let pow = new_block.header.pow.clone();
        self.orphans.insert(hash, new_block);
        let mut currently_used_orphans: Vec<BlockHash> = Vec::new();
        if self.current_total_pow.has_more_accum_work_than(&pow) {
            // we have a potential re-org here
            let result = self.handle_re_org(&hash, &mut currently_used_orphans);
            let result = if result.is_err() {
                self.block_chain_state.reset_chain_state()?;
                result
            } else {
                for hash in &currently_used_orphans {
                    self.orphans.remove(hash);
                }
                self.current_total_pow = pow;
                self.block_chain_state
                    .save_state()
                    .map_err(ChainError::StateProcessingError)
            };
            result
        } else {
            Err(ChainError::StateProcessingError(StateError::OrphanBlock))
        }
    }

    /// Internal recursive function to handle re orgs
    /// The function will go and search for a known parent and if found, will apply  the list of orphan blocks to re-org
    fn handle_re_org(
        &mut self,
        block_hash: &BlockHash,
        mut unorphaned_blocks: &mut Vec<BlockHash>,
    ) -> Result<(), ChainError>
    {
        // The searched hash should always be in the orphan list
        unorphaned_blocks.push(block_hash.clone()); // save all orphans we have used
        let block = &self.orphans[block_hash].clone();
        let parent = self.block_chain_state.headers.get_object(&block.header.prev_hash);
        if parent.is_some() {
            // we know of parent so we can re-org
            let h = parent.unwrap().height;
            self.block_chain_state
                .rewind_state((self.block_chain_state.get_tip_height() - h) as usize)?;
            return self
                .block_chain_state
                .process_new_block(&block)
                .map_err(ChainError::StateProcessingError);
        };
        let prev_block = self.orphans.get(&block.header.prev_hash);
        if prev_block.is_none() {
            return Err(ChainError::StateProcessingError(StateError::OrphanBlock));
        };

        let mut hash = [0; 32];
        hash.copy_from_slice(&prev_block.unwrap().header.hash());
        let result = self.handle_re_org(&hash, &mut unorphaned_blocks);

        if result.is_ok() {
            return self
                .block_chain_state
                .process_new_block(&block)
                .map_err(ChainError::StateProcessingError);
        }

        result
    }
}
