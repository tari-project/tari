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

use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase, MmrTree},
    tari_utilities::hex::Hex,
    transactions::{transaction::Transaction, types::CryptoFactories},
    validation::{MempoolTransactionValidation, ValidationError},
};
use log::*;
use tari_crypto::tari_utilities::hash::Hashable;

pub const LOG_TARGET: &str = "c::val::transaction_validators";

/// This validator will check the internal consistency of the transaction.
///
/// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
/// 1. The signature signs the canonical message with the private excess
/// 1. Range proofs of the outputs are valid
///
/// This function does NOT check that inputs come from the UTXO set
pub struct TxInternalConsistencyValidator {
    factories: CryptoFactories,
}

impl TxInternalConsistencyValidator {
    pub fn new(factories: CryptoFactories) -> Self {
        Self { factories }
    }
}

impl MempoolTransactionValidation for TxInternalConsistencyValidator {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        tx.validate_internal_consistency(&self.factories, None)
            .map_err(ValidationError::TransactionError)?;
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

impl<B: BlockchainBackend> MempoolTransactionValidation for TxInputAndMaturityValidator<B> {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        let db = self.db.db_read_access()?;
        verify_not_stxos(tx, &*db)?;
        // verify_inputs_are_utxos(tx, &*db)?;
        let tip_height = db.fetch_chain_metadata()?.height_of_longest_chain();
        verify_timelocks(tx, tip_height)?;
        verify_no_duplicated_inputs_outputs(tx)?;
        Ok(())
    }
}

// This function checks that all the timelocks in the provided transaction pass. It checks kernel lock heights and
// input maturities
fn verify_timelocks(tx: &Transaction, current_height: u64) -> Result<(), ValidationError> {
    if tx.min_spendable_height() > current_height + 1 {
        return Err(ValidationError::MaturityError);
    }
    Ok(())
}

// This function checks that the inputs and outputs do not exist in the STxO set.
fn verify_not_stxos<B: BlockchainBackend>(tx: &Transaction, db: &B) -> Result<(), ValidationError> {
    // `ChainMetadata::best_block` must always have the hash of the tip block.
    // NOTE: the backend makes no guarantee that the tip header has a corresponding full body (interrupted header sync,
    // pruned node) however the chain metadata best height MUST always correspond to the highest full block
    // this node can provide
    let metadata = db.fetch_chain_metadata()?;
    let data = db
        .fetch_block_accumulated_data(metadata.best_block())?
        .unwrap_or_else(|| {
            panic!(
                "Expected best block `{}` to have corresponding accumulated block data, but none was found",
                metadata.best_block().to_hex()
            )
        });
    for input in tx.body.inputs() {
        if let Some(index) = db.fetch_mmr_leaf_index(MmrTree::Utxo, &input.hash())? {
            if data.deleted().contains(index) {
                warn!(
                    target: LOG_TARGET,
                    "Transaction validation failed due to already spent input: {}", input
                );
                return Err(ValidationError::ContainsSTxO);
            }
        } else {
            warn!(
                target: LOG_TARGET,
                "Transaction validation failed because the block has invalid input: {} which does not exist", input
            );
            return Err(ValidationError::UnknownInputs);
        }
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
    validators: Vec<Box<dyn MempoolTransactionValidation>>,
}

impl MempoolValidator {
    pub fn new(validators: Vec<Box<dyn MempoolTransactionValidation>>) -> Self {
        Self { validators }
    }
}

impl MempoolTransactionValidation for MempoolValidator {
    fn validate(&self, transaction: &Transaction) -> Result<(), ValidationError> {
        for v in &self.validators {
            v.validate(transaction)?;
        }
        Ok(())
    }
}
