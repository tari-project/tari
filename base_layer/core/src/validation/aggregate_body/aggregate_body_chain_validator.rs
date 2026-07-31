//  Copyright 2022, The Tari Project
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

use std::collections::HashSet;

use log::*;
use tari_common_types::epoch::VnEpoch;
use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::{
    aggregated_body::AggregateBody,
    consensus::ConsensusConstants,
    transaction_components::{
        OutputType,
        SideChainId,
        SpentOutput,
        TransactionError,
        TransactionInput,
        ValidatorNodeRegistration,
    },
    validation::helpers::{check_tari_encrypted_data_byte_size, check_tari_script_byte_size},
};
use tari_utilities::hex::Hex;

use crate::{
    chain_storage::BlockchainBackend,
    consensus::BaseNodeConsensusManager,
    validation::{
        ValidationError,
        helpers::{
            check_eviction_proof,
            check_input_is_utxo,
            check_not_duplicate_txo,
            check_validator_node_exit,
            check_validator_node_registration,
        },
    },
};

pub const LOG_TARGET: &str = "c::val::aggregate_body_chain_linked_validator";

/// This validator assumes that the body was already validated for internal consistency and it will skip that step.
#[derive(Clone)]
pub struct AggregateBodyChainLinkedValidator {
    consensus_manager: BaseNodeConsensusManager,
}

impl AggregateBodyChainLinkedValidator {
    pub fn new(consensus_manager: BaseNodeConsensusManager) -> Self {
        Self { consensus_manager }
    }

    /// Validate a body (block or transaction) against the current chain state.
    ///
    /// The body's inputs are expected to already be hydrated, i.e. each input carries the full data of the output it
    /// spends rather than just a reference to it. Block sync hydrates compact inputs in
    /// [`BlockBodyFullValidator::validate_body`](crate::validation::block_body::BlockBodyFullValidator) before calling
    /// this, and propagated blocks are hydrated on receipt, so no cloning of the body is required here. Passing a body
    /// with compact (un-hydrated) inputs is a programming error and will be rejected during input validation.
    pub fn validate<B: BlockchainBackend>(
        &self,
        body: &AggregateBody,
        header: &BlockHeader,
        db: &B,
    ) -> Result<(), ValidationError> {
        let constants = self.consensus_manager.consensus_constants(header.height);

        self.validate_consensus(body, db)?;
        self.validate_body(body, db, constants, header)?;

        Ok(())
    }

    fn validate_consensus<B: BlockchainBackend>(&self, body: &AggregateBody, db: &B) -> Result<(), ValidationError> {
        validate_excess_sig_not_in_db(body, db)?;
        validate_burn_commitment_not_in_db(body, db)?;
        Ok(())
    }

    /// Validate the body against the chain state. See [`Self::validate`] for the input-hydration contract.
    pub fn validate_body<B: BlockchainBackend>(
        &self,
        body: &AggregateBody,
        db: &B,
        constants: &ConsensusConstants,
        header: &BlockHeader,
    ) -> Result<(), ValidationError> {
        validate_input_maturity(body, header.height)?;
        check_inputs_are_spendable(db, constants, header.height, body)?;
        check_outputs(db, constants, body, header.height)?;
        verify_no_duplicated_inputs_outputs(body)?;
        verify_no_duplicate_validator_node_registrations(body)?;
        check_total_burned(body)?;
        verify_timelocks(body, header.height)?;

        Ok(())
    }
}

/// Hydrate any compact inputs in `body` in place by resolving the referenced output.
///
/// A compact ("slim") input only references the output it spends by hash. This resolves that reference against the
/// database first, and failing that against the body's own outputs (for an output that is created and spent within
/// the same block), and populates the input with the full output data. The inputs are mutated in place, so the body
/// itself, its outputs and its kernels are never cloned; only an output created and spent in the same block is
/// cloned, since the block must keep its own copy of it.
///
/// Returns [`ValidationError::UnknownInput`] if a referenced output cannot be found, meaning the block/transaction is
/// invalid.
pub fn hydrate_compact_inputs<B: BlockchainBackend>(body: &mut AggregateBody, db: &B) -> Result<(), ValidationError> {
    let (inputs, outputs) = body.inputs_mut_with_outputs();
    for input in inputs.iter_mut() {
        if !input.is_compact() {
            continue;
        }
        let input_output_hash = input.output_hash();
        let output = match db.fetch_outputs(&input_output_hash) {
            Ok(val) => match val.into_iter().next() {
                Some(output_mined_info) => output_mined_info.output,
                None => {
                    // Input is found in this block
                    if let Some(found) = outputs.iter().find(|o| o.hash() == input_output_hash) {
                        found.clone()
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            "Input not found in database or block, commitment: {}, hash: {}",
                            input.commitment()?.to_hex(), input_output_hash,
                        );
                        return Err(ValidationError::UnknownInput);
                    }
                },
            },
            Err(e) => return Err(ValidationError::from(e)),
        };

        input.add_output_data(output);
    }

    Ok(())
}

pub fn validate_input_maturity(body: &AggregateBody, height: u64) -> Result<(), ValidationError> {
    for input in body.inputs() {
        if !input.is_mature_at(height)? {
            return Err(TransactionError::InputMaturity.into());
        }
    }

    Ok(())
}

fn validate_excess_sig_not_in_db<B: BlockchainBackend>(body: &AggregateBody, db: &B) -> Result<(), ValidationError> {
    for kernel in body.kernels() {
        if let Some((db_kernel, header_hash)) = db.fetch_kernel_by_excess_sig(&kernel.excess_sig)? {
            let msg = format!(
                "Aggregate body contains kernel excess: {} which matches already existing excess signature in chain \
                 database block hash: {}. Existing kernel excess: {}, excess sig nonce: {}, excess signature: {}",
                kernel.excess.to_hex(),
                header_hash.to_hex(),
                db_kernel.excess.to_hex(),
                db_kernel.excess_sig.get_compressed_public_nonce().to_hex(),
                db_kernel.excess_sig.get_signature().to_hex(),
            );
            return Err(ValidationError::DuplicateKernelError(msg));
        };
    }
    Ok(())
}

/// Ensures that no burn kernel reuses a burn commitment that already exists in the chain, and that a single block does
/// not contain two burns sharing a commitment.
///
/// Burn commitments are not part of the UTXO set (burned outputs are deliberately excluded from the UTXO commitment
/// uniqueness check), so without this rule the same burn commitment could be committed to the chain more than once and
/// then claimed multiple times on a sidechain. Burn commitments are carried on kernels, which are retained permanently
/// even by pruned nodes, so this check behaves identically on archival and pruned nodes.
// The `mutable_key_type` lint fires because `CompressedCommitment` caches its decompressed form. That interior
// mutability does not affect the `Hash`/`Eq` used here, so the set behaves correctly.
#[allow(clippy::mutable_key_type)]
fn validate_burn_commitment_not_in_db<B: BlockchainBackend>(
    body: &AggregateBody,
    db: &B,
) -> Result<(), ValidationError> {
    let mut seen_in_block = HashSet::new();
    for kernel in body.kernels() {
        if !kernel.is_burned() {
            continue;
        }
        let burn_commitment = kernel.get_burn_commitment()?;

        // Reject two burns in the same block that share a commitment; the database check below only sees committed
        // blocks, not the block currently being validated.
        if !seen_in_block.insert(burn_commitment.clone()) {
            return Err(ValidationError::InvalidBurnError(format!(
                "Block contains more than one burn kernel with burn commitment: {}",
                burn_commitment.to_hex()
            )));
        }

        // Reject a burn whose commitment already exists in a previously committed block.
        if let Some((_, header_hash)) = db.fetch_kernel_by_burn_commitment(burn_commitment)? {
            return Err(ValidationError::InvalidBurnError(format!(
                "Aggregate body contains burn commitment: {} which already exists in chain database block hash: {}",
                burn_commitment.to_hex(),
                header_hash.to_hex(),
            )));
        }
    }
    Ok(())
}

// This function checks that all inputs in the blocks are valid UTXO's to be spent
fn check_inputs_are_spendable<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    current_height: u64,
    body: &AggregateBody,
) -> Result<(), ValidationError> {
    let mut not_found_inputs = Vec::new();
    let mut output_hashes = None;

    for input in body.inputs() {
        check_output_feature_rules_for_input(db, constants, current_height, input)?;
        // If spending a unique_id, a new output must contain the unique id
        match check_input_is_utxo(db, input) {
            Ok(_) => continue,
            Err(ValidationError::UnknownInput) => {
                // Lazily allocate and hash outputs as needed
                if output_hashes.is_none() {
                    output_hashes = Some(body.outputs().iter().map(|output| output.hash()).collect::<Vec<_>>());
                }

                let output_hashes = output_hashes.as_ref().unwrap();
                let input_output_hash = input.output_hash();
                if output_hashes.iter().any(|val| val == &input_output_hash) {
                    continue;
                }
                debug!(
                    target: LOG_TARGET,
                    "Input not found in database, commitment: {}, hash: {}",
                    input.commitment()?.to_hex(), input_output_hash.to_hex()
                );
                not_found_inputs.push(input_output_hash);
            },
            Err(err) => {
                return Err(err);
            },
        }
    }

    if !not_found_inputs.is_empty() {
        return Err(ValidationError::UnknownInputs(not_found_inputs));
    }

    Ok(())
}

/// This function checks:
/// 1. that the output type is permitted
/// 2. the byte size of TariScript does not exceed the maximum
/// 3. that the outputs do not already exist in the UTxO set.
pub fn check_outputs<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    body: &AggregateBody,
    height: u64,
) -> Result<(), ValidationError> {
    let max_script_size = constants.max_script_byte_size();
    let max_encrypted_data_size = constants.max_extra_encrypted_data_byte_size();
    for output in body.outputs() {
        let epoch = constants.block_height_to_epoch(height);
        check_tari_script_byte_size(&output.script, max_script_size)?;
        check_tari_encrypted_data_byte_size(&output.encrypted_data, max_encrypted_data_size)?;
        check_not_duplicate_txo(db, output)?;
        check_validator_node_registration(db, output, epoch)?;
        check_validator_node_exit(db, output, epoch)?;
        check_eviction_proof(db, output, constants)?;
    }
    Ok(())
}

/// This function checks the body contains no duplicated inputs or outputs.
///
/// Inputs and outputs are deduplicated by their (canonical) output hash in a single linear pass. This does not rely on
/// the body being sorted: the `sorted` flag is `#[borsh(skip)]` and so is always false for a body deserialized from the
/// wire or database, which is exactly the block-sync and propagation case. Two inputs spending the same output are a
/// duplicate regardless of their signatures; the stricter `TransactionInput` equality (which also compares the
/// signature and script input data) is enforced by the sorted-and-unique consensus check.
pub fn verify_no_duplicated_inputs_outputs(body: &AggregateBody) -> Result<(), ValidationError> {
    let mut seen_inputs = HashSet::with_capacity(body.inputs().len());
    if !body
        .inputs()
        .iter()
        .all(|input| seen_inputs.insert(input.output_hash()))
    {
        warn!(
            target: LOG_TARGET,
            "AggregateBody validation failed due to double input"
        );
        return Err(ValidationError::UnsortedOrDuplicateInput);
    }
    let mut seen_outputs = HashSet::with_capacity(body.outputs().len());
    if !body.outputs().iter().all(|output| seen_outputs.insert(output.hash())) {
        warn!(
            target: LOG_TARGET,
            "AggregateBody validation failed due to double output"
        );
        return Err(ValidationError::UnsortedOrDuplicateOutput);
    }
    Ok(())
}

/// Ensures the body does not contain more than one validator node registration for the same validator node (scoped by
/// sidechain id).
///
/// `check_validator_node_registration` only rejects a registration that duplicates one already in the validator node
/// set as of the *parent* block. It cannot see other registrations in the same body. Without this check, a single block
/// carrying two registrations for the same validator node would pass validation and then fail when the second
/// registration is applied to the validator node set (the set is keyed by sidechain id + public key and inserted with
/// `NO_OVERWRITE`), aborting the block at commit time rather than rejecting it cleanly during validation.
// The `mutable_key_type` lint fires because `CompressedPublicKey` caches its decompressed form in a `OnceLock`. That
// interior mutability does not affect the `Hash`/`Eq` used here, so the set behaves correctly.
#[allow(clippy::mutable_key_type)]
pub fn verify_no_duplicate_validator_node_registrations(body: &AggregateBody) -> Result<(), ValidationError> {
    let mut seen = HashSet::new();
    for output in body.outputs() {
        let Some(sidechain_features) = output.features.sidechain_feature.as_ref() else {
            continue;
        };
        let Some(vn_reg) = sidechain_features.validator_node_registration() else {
            continue;
        };
        if !seen.insert((sidechain_features.sidechain_public_key(), vn_reg.public_key())) {
            warn!(
                target: LOG_TARGET,
                "AggregateBody validation failed due to duplicate validator node registration for {}",
                vn_reg.public_key()
            );
            return Err(ValidationError::DuplicateValidatorNodeRegistration {
                public_key: vn_reg.public_key().to_string(),
            });
        }
    }
    Ok(())
}

/// This function checks the total burned sum in the header ensuring that every burned output is counted in the total
/// sum.
#[allow(clippy::mutable_key_type)]
pub fn check_total_burned(body: &AggregateBody) -> Result<(), ValidationError> {
    let mut burned_outputs = HashSet::new();
    for output in body.outputs() {
        if output.is_burned() {
            // we dont care about duplicate commitments are they should have already been checked
            burned_outputs.insert(output.commitment.clone());
        }
    }
    for kernel in body.kernels() {
        if kernel.is_burned() && !burned_outputs.remove(kernel.get_burn_commitment()?) {
            return Err(ValidationError::InvalidBurnError(
                "Burned kernel does not match burned output".to_string(),
            ));
        }
    }

    if !burned_outputs.is_empty() {
        return Err(ValidationError::InvalidBurnError(
            "Burned output has no matching burned kernel".to_string(),
        ));
    }
    Ok(())
}

// This function checks that all the timelocks in the provided transaction pass. It checks kernel lock heights and
// input maturities
pub fn verify_timelocks(body: &AggregateBody, current_height: u64) -> Result<(), ValidationError> {
    if body.min_spendable_height()? > current_height.saturating_add(1) {
        warn!(
            target: LOG_TARGET,
            "AggregateBody has a min spend height higher than the current tip"
        );
        return Err(ValidationError::MaturityError);
    }
    Ok(())
}

/// If applicable, check any spend rules for output features including sidechain features
fn check_output_feature_rules_for_input<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    current_height: u64,
    input: &TransactionInput,
) -> Result<(), ValidationError> {
    match &input.spent_output {
        SpentOutput::OutputHash(_) => unreachable!("check_output_feature_rules_for_input: SpentOutput not hydrated"),
        SpentOutput::OutputData { features, .. } => {
            match features.output_type {
                OutputType::Standard | OutputType::Coinbase => {
                    // no special spend rules
                },

                OutputType::ValidatorNodeRegistration => {
                    // Prevents validator node registration output from being spent if the validator is still active.
                    // Effectively locking the funds in the UTXO until the validator exits/is evicted.
                    let reg = features.validator_node_registration().ok_or_else(|| {
                        ValidationError::OutputTypeNotMatchSidechainData {
                            output_type: features.output_type,
                            details: "Expected OutputType::ValidatorNodeRegistration to have validator node \
                                      registration sidechain data"
                                .to_string(),
                        }
                    })?;
                    let epoch = constants.block_height_to_epoch(current_height);
                    check_validator_node_registration_spend(db, reg, features.sidechain_id(), epoch)?
                },
                OutputType::ValidatorNodeExit => {
                    // should we disallow this? Since this UTXO has been processed w.r.t the active validator set, there
                    // is no reason to keep it in the UTXO set
                },
                OutputType::Burn => {
                    return Err(ValidationError::OutputSpendRuleDisallow {
                        output_type: features.output_type,
                        details: "Burn outputs cannot be spent".to_string(),
                    });
                },
                OutputType::CodeTemplateRegistration => {
                    return Err(ValidationError::OutputSpendRuleDisallow {
                        output_type: features.output_type,
                        details: "CodeTemplateRegistration cannot be spent".to_string(),
                    });
                },
                OutputType::SidechainCheckpoint => {
                    return Err(ValidationError::OutputSpendRuleDisallow {
                        output_type: features.output_type,
                        details: "SidechainCheckpoint cannot be spent".to_string(),
                    });
                },
                OutputType::SidechainProof => {
                    return Err(ValidationError::OutputSpendRuleDisallow {
                        output_type: features.output_type,
                        details: "SidechainProof cannot be spent".to_string(),
                    });
                },
            }
        },
    }
    Ok(())
}

fn check_validator_node_registration_spend<B: BlockchainBackend>(
    db: &B,
    reg: &ValidatorNodeRegistration,
    sidechain_id: Option<&SideChainId>,
    epoch: VnEpoch,
) -> Result<(), ValidationError> {
    if db.validator_node_is_active(sidechain_id.map(|id| id.public_key()), epoch, reg.public_key())? {
        return Err(ValidationError::OutputSpendRuleDisallow {
            output_type: OutputType::ValidatorNodeRegistration,
            details: format!(
                "Validator node registration {} is active and cannot be spent",
                reg.public_key()
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use tari_common_types::types::{CompressedCommitment, CompressedPublicKey, PrivateKey};
    use tari_crypto::keys::SecretKey;
    use tari_transaction_components::transaction_components::{
        KernelFeatures,
        OutputFeatures,
        TransactionKernel,
        TransactionOutput,
        ValidatorNodeSignature,
    };

    use super::*;
    use crate::test_helpers::blockchain::TempDatabase;

    fn registration_output(vn_secret_key: &PrivateKey) -> TransactionOutput {
        let claim_public_key = CompressedPublicKey::from_secret_key(vn_secret_key);
        let max_epoch = VnEpoch(10);
        let signature =
            ValidatorNodeSignature::sign_for_registration(vn_secret_key, None, &claim_public_key, max_epoch);
        TransactionOutput {
            features: OutputFeatures::for_validator_node_registration(signature, claim_public_key, None, max_epoch),
            ..Default::default()
        }
    }

    #[test]
    fn it_allows_distinct_validator_node_registrations() {
        let outputs = vec![
            registration_output(&PrivateKey::random(&mut rand::rng())),
            registration_output(&PrivateKey::random(&mut rand::rng())),
            // A non-registration output should be ignored.
            TransactionOutput::default(),
        ];
        let body = AggregateBody::new_unsorted(vec![], outputs, vec![]);
        assert!(verify_no_duplicate_validator_node_registrations(&body).is_ok());
    }

    #[test]
    fn it_rejects_two_registrations_for_the_same_validator_node() {
        // Two distinct outputs (different commitments) registering the same validator node public key. Each passes the
        // chain-state check individually but they collide when applied to the validator node set.
        let vn_secret_key = PrivateKey::random(&mut rand::rng());
        let outputs = vec![registration_output(&vn_secret_key), registration_output(&vn_secret_key)];
        let body = AggregateBody::new_unsorted(vec![], outputs, vec![]);
        let err = verify_no_duplicate_validator_node_registrations(&body).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::DuplicateValidatorNodeRegistration { .. }
        ));
    }

    fn random_commitment() -> CompressedCommitment {
        let public_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng()));
        CompressedCommitment::from_public_key(public_key.to_public_key().unwrap())
    }

    fn burn_kernel(burn_commitment: CompressedCommitment) -> TransactionKernel {
        TransactionKernel {
            features: KernelFeatures::BURN_KERNEL,
            burn_commitment: Some(burn_commitment),
            ..Default::default()
        }
    }

    #[test]
    fn it_allows_a_single_burn_commitment_not_in_the_db() {
        let db = TempDatabase::new();
        let body = AggregateBody::new_unsorted(vec![], vec![], vec![burn_kernel(random_commitment())]);
        assert!(validate_burn_commitment_not_in_db(&body, &db).is_ok());
    }

    #[test]
    fn it_allows_distinct_burn_commitments_in_one_block() {
        let db = TempDatabase::new();
        let body = AggregateBody::new_unsorted(vec![], vec![], vec![
            burn_kernel(random_commitment()),
            burn_kernel(random_commitment()),
        ]);
        assert!(validate_burn_commitment_not_in_db(&body, &db).is_ok());
    }

    #[test]
    fn it_rejects_two_burns_with_the_same_commitment_in_one_block() {
        let db = TempDatabase::new();
        let commitment = random_commitment();
        let body = AggregateBody::new_unsorted(vec![], vec![], vec![
            burn_kernel(commitment.clone()),
            burn_kernel(commitment),
        ]);
        let err = validate_burn_commitment_not_in_db(&body, &db).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidBurnError(_)));
    }
}
