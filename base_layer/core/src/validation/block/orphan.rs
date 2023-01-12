//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use super::BlockInternalConsistencyValidator;
use crate::{
    blocks::Block,
    consensus::ConsensusManager,
    transactions::CryptoFactories,
    validation::{InternalConsistencyValidator, ValidationError},
};

/// This validator tests whether a candidate block is internally consistent
#[derive(Clone)]
pub struct OrphanBlockValidator {
    validator: BlockInternalConsistencyValidator,
}

impl OrphanBlockValidator {
    pub fn new(rules: ConsensusManager, bypass_range_proof_verification: bool, factories: CryptoFactories) -> Self {
        let validator = BlockInternalConsistencyValidator::new(rules, bypass_range_proof_verification, factories);
        Self { validator }
    }
}

impl InternalConsistencyValidator for OrphanBlockValidator {
    fn validate_internal_consistency(&self, block: &Block) -> Result<(), ValidationError> {
        self.validator.validate(block)
    }
}
