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
use tari_utilities::hex::Hex;

use super::valid_header::InternallyValidHeader;
use crate::{
    blocks::{BlockHeader, BlockHeaderValidationError},
    consensus::{ConsensusConstants, ConsensusManager},
    validation::ValidationError,
};

pub const LOG_TARGET: &str = "c::val::internal_consistency_header_validator";

pub struct InternalConsistencyHeaderValidator {
    rules: ConsensusManager,
}

impl InternalConsistencyHeaderValidator {
    pub fn new(rules: ConsensusManager) -> Self {
        Self { rules }
    }

    /// The consensus checks that are done in order of cheapest to verify to most expensive
    #[allow(dead_code)]
    pub fn validate(&self, header: &BlockHeader) -> Result<InternallyValidHeader, ValidationError> {
        let constants = self.rules.consensus_constants(header.height);
        check_blockchain_version(constants, header.version)?;

        check_timestamp_ftl(header, &self.rules)?;

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
