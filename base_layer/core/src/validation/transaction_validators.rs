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

use log::*;
use tari_utilities::hex::Hex;

use super::aggregated_body::InternalConsistencyAggregateBodyValidator;
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::{transaction_components::Transaction, CryptoFactories},
    validation::{
        helpers::{
            check_inputs_are_utxos,
            check_outputs,
            check_permitted_output_types,
            check_total_burned,
            check_validator_node_registration_utxo,
            validate_versions,
        },
        MempoolTransactionValidator,
        ValidationError,
    },
};

pub const LOG_TARGET: &str = "c::val::transaction_validators";

/// This validator will check the internal consistency of the transaction.
///
/// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
/// 1. The signature signs the canonical message with the private excess
/// 1. Range proofs of the outputs are valid
///
/// This function does NOT check that inputs come from the UTXO set
pub struct TxInternalConsistencyValidator<B> {
    db: BlockchainDatabase<B>,
    factories: CryptoFactories,
    bypass_range_proof_verification: bool,
    aggregate_body_validator: InternalConsistencyAggregateBodyValidator,
}

impl<B: BlockchainBackend> TxInternalConsistencyValidator<B> {
    pub fn new(factories: CryptoFactories, bypass_range_proof_verification: bool, db: BlockchainDatabase<B>) -> Self {
        Self {
            db,
            factories,
            bypass_range_proof_verification,
            aggregate_body_validator: InternalConsistencyAggregateBodyValidator::default(),
        }
    }
}

impl<B: BlockchainBackend> MempoolTransactionValidator for TxInternalConsistencyValidator<B> {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        if tx.body.outputs().iter().any(|o| o.features.is_coinbase()) {
            return Err(ValidationError::ErroneousCoinbaseOutput);
        }

        // We can call this function with a constant value, because we've just shown that this is NOT a coinbase, and
        // only coinbases may have the extra field set (the only field that the fn argument affects).
        tx.body.check_output_features(1)?;

        let tip = {
            let db = self.db.db_read_access()?;
            db.fetch_chain_metadata()
        }?;

        self.aggregate_body_validator
            .validate_internal_consistency(
                &tx.body,
                &tx.offset,
                &tx.script_offset,
                self.bypass_range_proof_verification,
                None,
                &self.factories,
                Some(*tip.best_block()),
                tip.height_of_longest_chain(),
            )
            .map_err(ValidationError::TransactionError)?;
        Ok(())
    }
}

/// This validator will check the transaction against the current consensus rules.
///
/// 1. The transaction weight should not exceed the maximum weight for 1 block
/// 1. Input, output, and kernel versions are valid according to consensus
/// 1. All of the outputs should have a unique asset id in the transaction
/// 1. All of the outputs should have a unique asset id not already on chain (unless spent to a new output)
#[derive(Clone)]
pub struct TxConsensusValidator<B> {
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> TxConsensusValidator<B> {
    pub fn new(db: BlockchainDatabase<B>) -> Self {
        Self { db }
    }

    fn validate_excess_sig_not_in_db(&self, tx: &Transaction) -> Result<(), ValidationError> {
        for kernel in tx.body.kernels() {
            if let Some((db_kernel, header_hash)) = self.db.fetch_kernel_by_excess_sig(kernel.excess_sig.to_owned())? {
                let msg = format!(
                    "Aggregate body contains kernel excess: {} which matches already existing excess signature in \
                     chain database block hash: {}. Existing kernel excess: {}, excess sig nonce: {}, excess \
                     signature: {}",
                    kernel.excess.to_hex(),
                    header_hash.to_hex(),
                    db_kernel.excess.to_hex(),
                    db_kernel.excess_sig.get_public_nonce().to_hex(),
                    db_kernel.excess_sig.get_signature().to_hex(),
                );
                return Err(ValidationError::DuplicateKernelError(msg));
            };
        }
        Ok(())
    }
}

impl<B: BlockchainBackend> MempoolTransactionValidator for TxConsensusValidator<B> {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        let consensus_constants = self.db.consensus_constants()?;
        // validate maximum tx weight
        if tx.calculate_weight(consensus_constants.transaction_weight()) >
            consensus_constants.get_max_block_weight_excluding_coinbase()
        {
            return Err(ValidationError::MaxTransactionWeightExceeded);
        }

        self.validate_excess_sig_not_in_db(tx)?;

        validate_versions(tx.body(), consensus_constants)?;
        for output in tx.body.outputs() {
            check_permitted_output_types(consensus_constants, output)?;
            check_validator_node_registration_utxo(consensus_constants, output)?;
        }
        Ok(())
    }
}

/// This validator assumes that the transaction was already validated and it will skip this step. It will only check, in
/// order,: All inputs exist in the backend, All timelocks (kernel lock heights and output maturities) have passed
#[derive(Clone)]
pub struct TxInputAndMaturityValidator<B> {
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> TxInputAndMaturityValidator<B> {
    pub fn new(db: BlockchainDatabase<B>) -> Self {
        Self { db }
    }
}

impl<B: BlockchainBackend> MempoolTransactionValidator for TxInputAndMaturityValidator<B> {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        let constants = self.db.consensus_constants()?;
        let tip_height = {
            let db = self.db.db_read_access()?;
            check_inputs_are_utxos(&*db, tx.body())?;
            check_outputs(&*db, constants, tx.body())?;
            db.fetch_chain_metadata()?.height_of_longest_chain()
        };

        verify_timelocks(tx, tip_height)?;
        verify_no_duplicated_inputs_outputs(tx)?;
        check_total_burned(&tx.body)?;
        Ok(())
    }
}

// This function checks that all the timelocks in the provided transaction pass. It checks kernel lock heights and
// input maturities
fn verify_timelocks(tx: &Transaction, current_height: u64) -> Result<(), ValidationError> {
    if tx.min_spendable_height() > current_height + 1 {
        warn!(
            target: LOG_TARGET,
            "Transaction has a min spend height higher than the current tip"
        );
        return Err(ValidationError::MaturityError);
    }
    Ok(())
}

/// This function checks the at the tx contains no duplicated inputs or outputs.
fn verify_no_duplicated_inputs_outputs(tx: &Transaction) -> Result<(), ValidationError> {
    if tx.body.contains_duplicated_inputs() {
        warn!(target: LOG_TARGET, "Transaction validation failed due to double input");
        return Err(ValidationError::UnsortedOrDuplicateInput);
    }
    if tx.body.contains_duplicated_outputs() {
        warn!(target: LOG_TARGET, "Transaction validation failed due to double output");
        return Err(ValidationError::UnsortedOrDuplicateOutput);
    }
    Ok(())
}

pub struct MempoolValidator {
    validators: Vec<Box<dyn MempoolTransactionValidator>>,
}

impl MempoolValidator {
    pub fn new(validators: Vec<Box<dyn MempoolTransactionValidator>>) -> Self {
        Self { validators }
    }
}

impl MempoolTransactionValidator for MempoolValidator {
    fn validate(&self, transaction: &Transaction) -> Result<(), ValidationError> {
        for v in &self.validators {
            v.validate(transaction)?;
        }
        Ok(())
    }
}
