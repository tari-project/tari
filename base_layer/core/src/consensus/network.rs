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

use super::consensus_constants::ConsensusConstants;
use tari_common::configuration::Network as GlobalNetwork;
/// Specifies the configured chain network.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Network {
    /// Mainnet of Tari, currently should panic if network is set to this.
    MainNet,
    /// Alpha net version
    Rincewind,
    /// Local network constants used inside of unit and integration tests. Contains the genesis block to be used for
    /// that chain.
    LocalNet,
}

impl Network {
    pub fn create_consensus_constants(self) -> Vec<ConsensusConstants> {
        match self {
            Network::MainNet => ConsensusConstants::mainnet(),
            Network::Rincewind => ConsensusConstants::rincewind(),
            Network::LocalNet => ConsensusConstants::localnet(),
        }
    }
}

impl From<GlobalNetwork> for Network {
    fn from(global_network: GlobalNetwork) -> Self {
        match global_network {
            GlobalNetwork::MainNet => Network::MainNet,
            GlobalNetwork::Rincewind => Network::Rincewind,
        }
    }
}
