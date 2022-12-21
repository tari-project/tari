// Copyright 2022. The Tari Project
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

use std::sync::Arc;

use log::*;
use tari_common_types::types::FixedHash;
use tari_utilities::hex::Hex;

use super::valid_header::InternallyValidHeader;
use crate::{
    blocks::{BlockHeader, BlockHeaderValidationError},
    chain_storage::BlockchainBackend,
    consensus::{ConsensusConstants, ConsensusManager},
    proof_of_work::{monero_rx::MoneroPowData, PowAlgorithm},
    validation::ValidationError,
};

pub const LOG_TARGET: &str = "c::val::internal_consistency_header_validator";

pub struct InternalConsistencyHeaderValidator<TBackend> {
    rules: ConsensusManager,
    backend: TBackend,
}

impl<TBackend: BlockchainBackend> InternalConsistencyHeaderValidator<TBackend> {
    pub fn new(rules: ConsensusManager, backend: TBackend) -> Self {
        Self { rules, backend }
    }

    /// The consensus checks that are done in order of cheapest to verify to most expensive
    #[allow(dead_code)]
    pub fn validate(&self, header: &BlockHeader) -> Result<InternallyValidHeader, ValidationError> {
        let constants = self.rules.consensus_constants(header.height);
        check_blockchain_version(constants, header.version)?;

        check_timestamp_ftl(header, &self.rules)?;

        check_pow_data(header, &self.rules, &self.backend)?;

        check_not_bad_block(&self.backend, header.hash())?;

        // TODO: can we do more validations here?

        Ok(InternallyValidHeader(Arc::new(header.clone())))
    }
}

pub fn check_blockchain_version(constants: &ConsensusConstants, version: u16) -> Result<(), ValidationError> {
    if constants.valid_blockchain_version_range().contains(&version) {
        Ok(())
    } else {
        Err(ValidationError::InvalidBlockchainVersion { version })
    }
}

/// This function tests that the block timestamp is less than the FTL
pub fn check_timestamp_ftl(
    block_header: &BlockHeader,
    consensus_manager: &ConsensusManager,
) -> Result<(), ValidationError> {
    if block_header.timestamp > consensus_manager.consensus_constants(block_header.height).ftl() {
        warn!(
            target: LOG_TARGET,
            "Invalid Future Time Limit on block:{}",
            block_header.hash().to_hex()
        );
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestampFutureTimeLimit,
        ));
    }
    Ok(())
}

/// Check the PoW data in the BlockHeader. This currently only applies to blocks merged mined with Monero.
pub fn check_pow_data<B: BlockchainBackend>(
    block_header: &BlockHeader,
    rules: &ConsensusManager,
    db: &B,
) -> Result<(), ValidationError> {
    use PowAlgorithm::{Monero, Sha3};
    match block_header.pow.pow_algo {
        Monero => {
            let monero_data =
                MoneroPowData::from_header(block_header).map_err(|e| ValidationError::CustomError(e.to_string()))?;
            let seed_height = db.fetch_monero_seed_first_seen_height(&monero_data.randomx_key)?;
            if seed_height != 0 {
                // Saturating sub: subtraction can underflow in reorgs / rewind-blockchain command
                let seed_used_height = block_header.height.saturating_sub(seed_height);
                if seed_used_height > rules.consensus_constants(block_header.height).max_randomx_seed_height() {
                    return Err(ValidationError::BlockHeaderError(
                        BlockHeaderValidationError::OldSeedHash,
                    ));
                }
            }

            Ok(())
        },
        Sha3 => {
            if !block_header.pow.pow_data.is_empty() {
                return Err(ValidationError::CustomError(
                    "Proof of work data must be empty for Sha3 blocks".to_string(),
                ));
            }
            Ok(())
        },
    }
}

pub fn check_not_bad_block<B: BlockchainBackend>(db: &B, hash: FixedHash) -> Result<(), ValidationError> {
    if db.bad_block_exists(hash)? {
        return Err(ValidationError::BadBlockFound { hash: hash.to_hex() });
    }
    Ok(())
}
