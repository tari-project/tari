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

use crate::blocks::{
    genesis_block::{
        get_mainnet_block_hash,
        get_mainnet_genesis_block,
        get_rincewind_block_hash,
        get_rincewind_genesis_block,
    },
    Block,
};
use tari_crypto::tari_utilities::hash::Hashable;

/// Specifies the configured chain network.
pub enum Network {
    /// Mainnet of Tari, currently should panic if network is set to this.
    MainNet,
    /// Alpha net version
    Rincewind,
    /// Local network constants used inside of unit and integration tests. Contains the genesis block to be used for
    /// that chain.
    LocalNet(Box<Block>),
}

impl Network {
    /// Returns the genesis block for the selected network.
    pub fn get_genesis_block(&self) -> Block {
        match self {
            Network::MainNet => get_mainnet_genesis_block(),
            Network::Rincewind => get_rincewind_genesis_block(),
            Network::LocalNet(genesis_block) => (**genesis_block).clone(),
        }
    }

    /// Returns the genesis block hash for the selected network.
    pub fn get_genesis_block_hash(&self) -> Vec<u8> {
        match self {
            Network::MainNet => get_mainnet_block_hash(),
            Network::Rincewind => get_rincewind_block_hash(),
            Network::LocalNet(genesis_block) => (**genesis_block).hash(),
        }
    }
}
