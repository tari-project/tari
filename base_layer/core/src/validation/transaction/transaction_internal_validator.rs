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

use tari_common_types::{chain_metadata::ChainMetadata, types::HashOutput};

use crate::{
    consensus::ConsensusManager,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{OutputType::Coinbase, Transaction},
        CryptoFactories,
    },
    validation::{aggregate_body::AggregateBodyInternalConsistencyValidator, ValidationError},
};

pub struct TransactionInternalConsistencyValidator {
    aggregate_body_validator: AggregateBodyInternalConsistencyValidator,
}

impl TransactionInternalConsistencyValidator {
    pub fn new(
        bypass_range_proof_verification: bool,
        consensus_manager: ConsensusManager,
        factories: CryptoFactories,
    ) -> Self {
        Self {
            aggregate_body_validator: AggregateBodyInternalConsistencyValidator::new(
                bypass_range_proof_verification,
                consensus_manager,
                factories,
            ),
        }
    }

    /// Validate this transaction by checking the following:
    /// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
    /// 1. The signature signs the canonical message with the private excess
    /// 1. Range proofs of the outputs are valid
    ///
    /// This function does NOT check that inputs come from the UTXO set
    pub fn validate(
        &self,
        tx: &Transaction,
        reward: Option<MicroMinotari>,
        prev_header: Option<HashOutput>,
        height: u64,
    ) -> Result<(), ValidationError> {
        self.aggregate_body_validator
            .validate(&tx.body, &tx.offset, &tx.script_offset, reward, prev_header, height)
    }

    pub fn validate_with_current_tip(
        &self,
        tx: &Transaction,
        tip_metadata: ChainMetadata,
    ) -> Result<(), ValidationError> {
        if tx.body.outputs().iter().any(|o| o.features.is_coinbase()) {
            return Err(ValidationError::OutputTypeNotPermitted { output_type: Coinbase });
        }

        // We can call this function with a constant value, because we've just shown that this is NOT a coinbase, and
        // only coinbases may have the extra field set (the only field that the fn argument affects).
        tx.body.check_output_features(1)?;

        self.aggregate_body_validator.validate(
            &tx.body,
            &tx.offset,
            &tx.script_offset,
            None,
            Some(*tip_metadata.best_block_hash()),
            tip_metadata.best_block_height(),
        )
    }
}
