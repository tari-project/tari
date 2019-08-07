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

use crate::{tari_amount::MicroTari, types::CONSENSUS_RULES};
use std::env;

/// This is used to control all consensus values.
#[derive(Clone, Copy)]
pub struct ConsensusRules {
    /// The min height maturity a coinbase utxo must have
    pub coinbase_lock_height: u64,
    /// The max range proof size that is allowed to be created
    pub max_range_proof_range: usize,
    /// This is emission schedule initial amount, the decay and the tail emission
    pub emission_schedule: EmissionParameters,
    /// Current version of the blockchain
    pub blockchain_version: u16,
}

#[derive(Clone, Copy)]
pub struct EmissionParameters {
    pub initial: MicroTari,
    pub decay: f64,
    pub tail: MicroTari,
}

impl Default for ConsensusRules {
    fn default() -> Self {
        ConsensusRules {
            coinbase_lock_height: 1440,
            max_range_proof_range: 64,
            emission_schedule: EmissionParameters {
                initial: MicroTari::from(10_000_000),
                decay: 0.999,
                tail: MicroTari::from(100),
            },
            blockchain_version: 0,
        }
    }
}

impl ConsensusRules {
    pub fn new_as_test() -> Self {
        ConsensusRules {
            coinbase_lock_height: 1,
            max_range_proof_range: 32,
            emission_schedule: EmissionParameters {
                initial: MicroTari::from(10_000_000),
                decay: 0.999,
                tail: MicroTari::from(100),
            },
            blockchain_version: 0,
        }
    }

    pub fn new_as_integration_test() -> Self {
        ConsensusRules {
            coinbase_lock_height: 1,
            max_range_proof_range: 64,
            emission_schedule: EmissionParameters {
                initial: MicroTari::from(10_000_000),
                decay: 0.999,
                tail: MicroTari::from(100),
            },
            blockchain_version: 0,
        }
    }

    pub fn new_as_prod() -> Self {
        ConsensusRules::default()
    }

    pub fn get_coinbase_lock_height() -> u64 {
        CONSENSUS_RULES.coinbase_lock_height
    }

    pub fn get_max_range_proof_range() -> usize {
        CONSENSUS_RULES.max_range_proof_range
    }

    pub fn get_blockchain_version() -> u16 {
        CONSENSUS_RULES.blockchain_version
    }

    pub fn set_prod() {
        let key = "CONSENSUSRULES";
        env::set_var(key, "PRODUCTION");
    }

    pub fn set_test() {
        let key = "CONSENSUSRULES";
        env::set_var(key, "UNIT_TEST");
    }

    pub fn set_integration_test() {
        let key = "CONSENSUSRULES";
        env::set_var(key, "INTEGRATION_TEST");
    }

    pub fn get_emission_parameters() -> EmissionParameters {
        CONSENSUS_RULES.emission_schedule
    }
}
#[cfg(test)]
mod test1 {
    use super::*;

    #[test]
    fn debug() {
        ConsensusRules::set_test();
        assert_eq!(ConsensusRules::get_coinbase_lock_height(), 1);
        assert_eq!(ConsensusRules::get_max_range_proof_range(), 32);
    }
}
