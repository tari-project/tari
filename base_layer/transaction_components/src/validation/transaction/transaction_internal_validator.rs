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
    MicroMinotari,
    consensus::ConsensusManager,
    crypto_factories::CryptoFactories,
    transaction_components::{OutputType::Coinbase, Transaction},
    validation::{
        AggregatedBodyValidationError,
        aggregate_body::AggregateBodyInternalConsistencyValidator,
        helpers::check_no_coinbase_kernels,
    },
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
    ) -> Result<(), AggregatedBodyValidationError> {
        self.aggregate_body_validator
            .validate(&tx.body, &tx.offset, &tx.script_offset, reward, prev_header, height)
    }

    /// Validate the transaction as if it is mined in the block after `tip_metadata`.
    pub fn validate_with_current_tip(
        &self,
        tx: &Transaction,
        tip_metadata: ChainMetadata,
    ) -> Result<(), AggregatedBodyValidationError> {
        if tx.body.outputs().iter().any(|o| o.features.is_coinbase()) {
            return Err(AggregatedBodyValidationError::OutputTypeNotPermitted { output_type: Coinbase });
        }
        check_no_coinbase_kernels(&tx.body)?;

        // We can call this function with a constant value, because we've just shown that this is NOT a coinbase, and
        // only coinbases may have the extra field set (the only field that the fn argument affects).
        tx.body.check_output_features(1)?;

        // Maturity and lock height are monotonic in height, so the next height is never stricter than the tip for
        // them. Scripts and covenants are not, so they must be evaluated at the height the block will use.
        self.aggregate_body_validator.validate(
            &tx.body,
            &tx.offset,
            &tx.script_offset,
            None,
            Some(*tip_metadata.best_block_hash()),
            tip_metadata.best_block_height().saturating_add(1),
        )
    }
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_common_types::types::{FixedHash, PrivateKey};
    use tari_crypto::keys::SecretKey;

    use super::*;
    use crate::{test_helpers::create_kernel_only_transaction, transaction_components::KernelFeatures};

    #[test]
    fn validate_with_current_tip_uses_next_block_height() {
        let validator = TransactionInternalConsistencyValidator::new(
            true,
            ConsensusManager::builder(Network::LocalNet).build(),
            CryptoFactories::default(),
        );
        let h = 10;
        let tip = ChainMetadata::new(h, FixedHash::zero(), 0, 0, 1u64.into(), 0).unwrap();
        let k = PrivateKey::random(&mut rand::rng());

        let tx = create_kernel_only_transaction(&k, h + 1, KernelFeatures::empty());
        validator.validate_with_current_tip(&tx, tip.clone()).unwrap();

        let tx = create_kernel_only_transaction(&k, h + 2, KernelFeatures::empty());
        let err = validator.validate_with_current_tip(&tx, tip).unwrap_err();
        assert!(matches!(err, AggregatedBodyValidationError::MaturityError));
    }
}
