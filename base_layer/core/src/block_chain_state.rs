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
//

// This file is used to store the current blockchain state

use crate::{
    block::Block,
    blockheader::BlockHeader,
    error::*,
    genesis_block::*,
    transaction::{TransactionInput, TransactionKernel},
    types::*,
};
use merklemountainrange::mmr::*;
use std::fs;
use tari_storage::{keyvalue_store::*, lmdb::*};
use tari_utilities::hash::Hashable;

/// The BlockchainState struct keeps record of the current UTXO, total kernels and headers.
pub struct BlockchainState {
    pub headers: MerkleMountainRange<BlockHeader, SignatureHash>,
    utxos: MerkleMountainRange<TransactionInput, SignatureHash>,
    kernels: MerkleMountainRange<TransactionKernel, SignatureHash>,
    store: LMDBStore,
}
#[allow(clippy::new_without_default)]
impl BlockchainState {
    /// Creates a new empty blockchainstate
    // ToDo link to some config for the pruning horizon, its currently 5000.
    pub fn new() -> Result<BlockchainState, StateError> {
        fs::create_dir("./storage/state").unwrap();
        let store = BlockchainState::build_db()?;
        let mut headers = MerkleMountainRange::new();
        headers.init_persistance_store(&"headers".to_string(), std::usize::MAX);
        let mut utxos = MerkleMountainRange::new();
        utxos.init_persistance_store(&"outputs".to_string(), 5000);
        let mut kernels = MerkleMountainRange::new();
        kernels.init_persistance_store(&"kernels".to_string(), std::usize::MAX);
        let mut block_chain_state = BlockchainState {
            headers,
            utxos,
            kernels,
            store,
        };
        block_chain_state.add_genesis_block();

        Ok(block_chain_state)
    }

    // add the genesis block
    fn add_genesis_block(&mut self) {
        let gen_block = get_genesis_block();
        for output in gen_block.body.outputs {
            self.utxos.push(output.into()).expect("genesis block failed")
        }
        self.kernels
            .append(gen_block.body.kernels)
            .expect("genesis block failed");
        self.headers.push(gen_block.header).expect("genesis block failed");

        self.check_point_state().expect("genesis block failed");
    }

    fn build_db() -> Result<LMDBStore, DatastoreError> {
        let builder = LMDBBuilder::new();
        builder
            .set_mapsize(5)
            .set_path("./storage/state/")
            //create for headers mmr
            .add_database(&"headers_mmr_checkpoints".to_string())
            .add_database(&"headers_mmr_objects".to_string())
            .add_database(&"headers_init".to_string())
            //create for outputs mmr
            .add_database(&"outputs_mmr_checkpoints".to_string())
            .add_database(&"outputs_mmr_objects".to_string())
            .add_database(&"outputs_init".to_string())
            //create for range_proofs mmr
            .add_database(&"range_proofs_mmr_checkpoints".to_string())
            .add_database(&"range_proofs_mmr_objects".to_string())
            .add_database(&"range_proofs_init".to_string())
            //create for kernels mmr
            .add_database(&"kernels_mmr_checkpoints".to_string())
            .add_database(&"kernels_mmr_objects".to_string())
            .add_database(&"kernels_init".to_string())
            .build()
    }

    /// Will update the pruning horizon to the new value for the outputs data store
    pub fn change_pruning_horizon(&mut self, new_pruning_horizon: usize) {
        self.utxos.set_pruning_horizon(new_pruning_horizon);
    }

    /// This function  will process a new block.
    /// Note the block is consumed by the function.
    pub fn process_new_block(&mut self, new_block: &Block) -> Result<(), StateError> {
        let found = self.headers.get_object(&new_block.header.hash());
        if found.is_some() {
            return Err(StateError::DuplicateBlock);
        }
        self.validate_new_block(&new_block)?;
        self.prune_all_inputs(&new_block)?;
        // All seems valid, lets add the objects to the state
        for output in &new_block.body.outputs {
            self.utxos.push(output.clone().into())?;
        }
        self.kernels.append(new_block.body.kernels.clone())?;
        self.headers.push(new_block.header.clone())?;
        // lets check states
        self.check_mmr_states()?;
        self.check_point_state()
    }

    /// This function will validate the block in terms of the current state.
    pub fn validate_new_block(&self, new_block: &Block) -> Result<(), StateError> {
        new_block
            .check_internal_consistency()
            .map_err(StateError::InvalidBlock)?;
        // we assume that we have atleast in block in the headers mmr even if this is the genesis one
        if self.headers.get_last_added_object().unwrap().hash() != new_block.header.prev_hash {
            return Err(StateError::OrphanBlock);
        }
        Ok(())
    }

    /// On validation error the chain state should be reset to the last validly saved state
    pub fn reset_chain_state(&mut self) -> Result<(), StateError> {
        self.headers.ff_to_head(&mut self.store)?;
        self.utxos.ff_to_head(&mut self.store)?;
        self.kernels.ff_to_head(&mut self.store)?;
        Ok(())
    }

    /// This function test if all the inputs are valid unpruned out puts and flags them as pruned.
    /// This will return an error if any of the inputs where pruned or unknown
    fn prune_all_inputs(&mut self, new_block: &Block) -> Result<(), StateError> {
        for input in &new_block.body.inputs {
            let hash = input.hash();
            self.utxos
                .prune_object_hash(&hash)
                .map_err(StateError::SpentUnknownCommitment)?;
        }
        Ok(())
    }

    /// This function is just a wrapper function to call checkpoint on all the MMR's
    fn check_point_state(&mut self) -> Result<(), StateError> {
        self.headers.checkpoint()?;
        self.kernels.checkpoint()?;
        self.utxos.checkpoint()?;
        Ok(())
    }

    /// This function is just a wrapper function to call checkpoint on all the MMR's
    fn check_mmr_states(&mut self) -> Result<(), StateError> {
        let last_header = self.headers.get_last_added_object().unwrap(); // if this unwrap fails there is something weird wrong as the headers did not get added.
        if (last_header.output_mr != self.utxos.get_merkle_root()[..]) ||
            (last_header.kernel_mr != self.kernels.get_merkle_root()[..])
        {
            return Err(StateError::HeaderStateMismatch);
        }
        Ok(())
    }

    /// This function will save the current blockchain state to disc
    pub fn save_state(&mut self) -> Result<(), StateError> {
        self.headers.apply_state(&mut self.store)?;
        self.kernels.apply_state(&mut self.store)?;
        self.utxos.apply_state(&mut self.store)?;
        Ok(())
    }

    /// This function will rewind the chain state by the block_count provided
    pub fn rewind_state(&mut self, block_count: usize) -> Result<(), StateError> {
        self.headers.rewind(&mut self.store, block_count)?;
        self.kernels.rewind(&mut self.store, block_count)?;
        self.utxos.rewind(&mut self.store, block_count)?;
        Ok(())
    }

    pub fn get_tip_height(&self) -> u64 {
        match self.headers.get_last_added_object() {
            Some(v) => v.height,
            None => 0,
        }
    }
}
