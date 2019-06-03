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

// This file is used to store the current blockchain state

use crate::error::StateError;
use tari_core::block::Block;
// TODO add back in MMR
/// The BlockchainState struct keeps record of the current UTXO, total kernels and headers.
pub struct BlockchainState {
    _headers: i32, // MerkleMountainRange<BlockHeader, SignatureHash>,
    _outputs: i32, // MerkleMountainRange<TransactionOutput, SignatureHash>,
    _kernels: i32, // MerkleMountainRange<TransactionKernel, SignatureHash>,
}

impl BlockchainState {
    /// Creates a new empty blockchainstate
    pub fn new() -> BlockchainState {
        BlockchainState {
            _headers: 0, // MerkleMountainRange::new(),
            _outputs: 0, // MerkleMountainRange::new(),
            _kernels: 0, // MerkleMountainRange::new(),
        }
    }

    /// This function  will process a new block.
    /// Note the block must have been validated by the chainstate before.
    pub fn process_new_block(&self, new_block: &Block) -> Result<(), StateError> {
        self.validate_new_block(new_block)
    }

    /// This function will validate the block in terms of the current state.
    pub fn validate_new_block(&self, _new_block: &Block) -> Result<(), StateError> {
        Ok(())
    }
}
