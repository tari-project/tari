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

use tari_common::configuration::Network;

use super::consensus_constants::ConsensusConstants;

/// Represents the consensus used for a given network
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct NetworkConsensus(Network);

impl NetworkConsensus {
    pub fn create_consensus_constants(&self) -> Vec<ConsensusConstants> {
        use Network::{Esmeralda, Igor, LocalNet, MainNet, NextNet, StageNet};
        match self.as_network() {
            MainNet => ConsensusConstants::mainnet(),
            StageNet => ConsensusConstants::stagenet(),
            NextNet => ConsensusConstants::nextnet(),
            LocalNet => ConsensusConstants::localnet(),
            Igor => ConsensusConstants::igor(),
            Esmeralda => ConsensusConstants::esmeralda(),
        }
    }

    #[inline]
    pub fn as_network(self) -> Network {
        self.0
    }
}

impl From<Network> for NetworkConsensus {
    fn from(global_network: Network) -> Self {
        Self(global_network)
    }
}
