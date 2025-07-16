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
use tari_crypto::ristretto::bulletproofs_plus::RistrettoAggregatedPublicStatement;
use tari_transaction_components::transaction_components::RangeProofType;
mod aggregate_body_internal_validator;
pub use aggregate_body_internal_validator::{validate_individual_output, AggregateBodyInternalConsistencyValidator};
use tari_crypto::{errors::RangeProofError, extended_range_proof::Statement};
mod aggregate_body_chain_validator;
pub use aggregate_body_chain_validator::AggregateBodyChainLinkedValidator;
use tari_common_types::types::RangeProofService;
use tari_crypto::extended_range_proof::ExtendedRangeProofService;
use tari_transaction_components::transaction_components::TransactionOutput;

pub(crate) fn batch_verify_range_proofs(
    prover: &RangeProofService,
    outputs: &[&TransactionOutput],
) -> Result<(), RangeProofError> {
    let bulletproof_plus_proofs = outputs
        .iter()
        .filter(|o| o.features.range_proof_type == RangeProofType::BulletProofPlus)
        .copied()
        .collect::<Vec<&TransactionOutput>>();
    if !bulletproof_plus_proofs.is_empty() {
        let mut statements = Vec::with_capacity(bulletproof_plus_proofs.len());
        let mut proofs = Vec::with_capacity(bulletproof_plus_proofs.len());
        for output in &bulletproof_plus_proofs {
            statements.push(RistrettoAggregatedPublicStatement {
                statements: vec![Statement {
                    commitment: output
                        .commitment
                        .to_commitment()
                        .map_err(|_e| RangeProofError::InvalidRangeProof {
                            reason: "Invalid commitment".to_string(),
                        })?,
                    minimum_value_promise: output.minimum_value_promise.into(),
                }],
            });
            proofs.push(output.proof_result()?.as_vec());
        }

        // Attempt to verify the range proofs in a batch
        prover.verify_batch(proofs, statements.iter().collect())?;
    }

    let revealed_value_proofs = outputs
        .iter()
        .filter(|o| o.features.range_proof_type == RangeProofType::RevealedValue)
        .copied()
        .collect::<Vec<&TransactionOutput>>();
    for output in revealed_value_proofs {
        output.revealed_value_range_proof_check()?;
    }

    // An empty batch is valid
    Ok(())
}
