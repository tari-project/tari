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

use std::collections::HashMap;

use tari_common::configuration::Network;

use crate::{
    consensus::{consensus_constants::PowAlgorithmConstants, ConsensusConstants, NetworkConsensus},
    proof_of_work::PowAlgorithm,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{OutputType, RangeProofType},
    },
};

/// Test class to create custom consensus constants
pub struct TestConsensusConstantsBuilder {
    consensus: ConsensusConstants,
}

impl TestConsensusConstantsBuilder {
    pub fn new(network: Network) -> Self {
        Self {
            consensus: NetworkConsensus::from(network)
                .create_consensus_constants()
                .pop()
                .expect("Empty consensus constants"),
        }
    }

    pub fn clear_proof_of_work(mut self) -> Self {
        self.consensus.proof_of_work = HashMap::new();
        self
    }

    pub fn add_proof_of_work(mut self, proof_of_work: PowAlgorithm, constants: PowAlgorithmConstants) -> Self {
        self.consensus.proof_of_work.insert(proof_of_work, constants);
        self
    }

    pub fn with_coinbase_lockheight(mut self, height: u64) -> Self {
        self.consensus.coinbase_min_maturity = height;
        self
    }

    pub fn with_max_script_byte_size(mut self, byte_size: usize) -> Self {
        self.consensus.max_script_byte_size = byte_size;
        self
    }

    pub fn with_max_block_transaction_weight(mut self, weight: u64) -> Self {
        self.consensus.max_block_transaction_weight = weight;
        self
    }

    pub fn with_consensus_constants(mut self, consensus: ConsensusConstants) -> Self {
        self.consensus = consensus;
        self
    }

    pub fn with_max_randomx_seed_height(mut self, height: u64) -> Self {
        self.consensus.max_randomx_seed_height = height;
        self
    }

    pub fn with_faucet_value(mut self, value: MicroTari) -> Self {
        self.consensus.faucet_value = value;
        self
    }

    pub fn with_emission_amounts(
        mut self,
        intial_amount: MicroTari,
        decay: &'static [u64],
        tail_amount: MicroTari,
    ) -> Self {
        self.consensus.emission_initial = intial_amount;
        self.consensus.emission_decay = decay;
        self.consensus.emission_tail = tail_amount;
        self
    }

    pub fn with_permitted_output_types(mut self, permitted_output_types: &'static [OutputType]) -> Self {
        self.consensus.permitted_output_types = permitted_output_types;
        self
    }

    pub fn with_permitted_range_proof_types(mut self, permitted_range_proof_types: &'static [RangeProofType]) -> Self {
        self.consensus.permitted_range_proof_types = permitted_range_proof_types;
        self
    }

    pub fn with_blockchain_version(mut self, version: u16) -> Self {
        self.consensus.blockchain_version = version;
        self
    }

    pub fn build(self) -> ConsensusConstants {
        self.consensus
    }
}
