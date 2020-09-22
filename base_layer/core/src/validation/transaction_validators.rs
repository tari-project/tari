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
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::{transaction::Transaction, types::CryptoFactories},
    validation::{
        helpers::{is_stxo, is_utxo},
        Validation,
        ValidationError,
    },
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

impl Validation<Transaction> for TxInternalConsistencyValidator {
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

impl<B: BlockchainBackend> Validation<Transaction> for TxInputAndMaturityValidator<B> {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        let db = self.db.db_read_access()?;
        verify_not_stxos(tx, &*db)?;
        verify_inputs_are_utxos(tx, &*db)?;
        let tip_height = db.fetch_chain_metadata()?.height_of_longest_chain();
        verify_timelocks(tx, tip_height)?;
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
    for input in tx.body.inputs() {
        if is_stxo(db, input.hash())? {
            // we dont want to log this as a node or wallet might retransmit a transaction
            debug!(
                target: LOG_TARGET,
                "Transaction validation failed due to already spent input: {}", input
            );
            return Err(ValidationError::ContainsSTxO);
        }
    }
    for output in tx.body.outputs() {
        if is_stxo(db, output.hash())? {
            debug!(
                target: LOG_TARGET,
                "Transaction validation failed due to previously spent output: {}", output
            );
            return Err(ValidationError::ContainsSTxO);
        }
    }
    Ok(())
}

// This function checks that all inputs in the transaction are valid UTXO's to be spend.
fn verify_inputs_are_utxos<B: BlockchainBackend>(tx: &Transaction, db: &B) -> Result<(), ValidationError> {
    for input in tx.body.inputs() {
        if !is_utxo(db, input.hash())? {
            debug!(
                target: LOG_TARGET,
                "Transaction validation failed due to unknown input: {}", input
            );
            return Err(ValidationError::UnknownInputs);
        }
    }
    Ok(())
}
