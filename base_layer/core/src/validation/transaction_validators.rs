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
    chain_storage::{is_stxo, is_utxo, BlockchainBackend},
    transactions::{transaction::Transaction, types::CryptoFactories},
    validation::{StatelessValidation, Validation, ValidationError},
};
use log::*;
use tari_crypto::tari_utilities::hash::Hashable;

pub const LOG_TARGET: &str = "c::val::transaction_validators";

/// This validator will only check that a transaction is internally consistent. It requires no state information.
pub struct StatelessTxValidator {
    factories: CryptoFactories,
}

impl StatelessTxValidator {
    pub fn new(factories: CryptoFactories) -> Self {
        Self { factories }
    }
}

impl StatelessValidation<Transaction> for StatelessTxValidator {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        verify_tx(tx, &self.factories)?;
        Ok(())
    }
}

/// This validator will perform a full verification of the transaction. In order the following will be checked:
/// Transaction integrity, All inputs exist in the backend, All timelocks (kernel lock heights and output maturities)
/// have passed
pub struct FullTxValidator {
    factories: CryptoFactories,
}

impl FullTxValidator {
    pub fn new(factories: CryptoFactories) -> Self {
        Self { factories }
    }
}

impl<B: BlockchainBackend> Validation<Transaction, B> for FullTxValidator {
    fn validate(&self, tx: &Transaction, db: &B) -> Result<(), ValidationError> {
        verify_tx(tx, &self.factories)?;
        verify_not_stxos(tx, db)?;
        verify_inputs_are_utxos(tx, db)?;
        let tip_height = db
            .fetch_metadata()
            .map_err(|e| ValidationError::CustomError(e.to_string()))?
            .height_of_longest_chain
            .unwrap_or(0);
        verify_timelocks(tx, tip_height)?;
        Ok(())
    }
}

/// This validator assumes that the transaction was already validated and it will skip this step. It will only check, in
/// order,: All inputs exist in the backend, All timelocks (kernel lock heights and output maturities) have passed
pub struct TxInputAndMaturityValidator {}

impl<B: BlockchainBackend> Validation<Transaction, B> for TxInputAndMaturityValidator {
    fn validate(&self, tx: &Transaction, db: &B) -> Result<(), ValidationError> {
        verify_not_stxos(tx, db)?;
        verify_inputs_are_utxos(tx, db)?;
        let tip_height = db
            .fetch_metadata()
            .map_err(|e| ValidationError::CustomError(e.to_string()))?
            .height_of_longest_chain
            .unwrap_or(0);
        verify_timelocks(tx, tip_height)?;
        Ok(())
    }
}

/// This validator will only check that inputs exists in the backend.
pub struct InputTxValidator {}

impl<B: BlockchainBackend> Validation<Transaction, B> for InputTxValidator {
    fn validate(&self, tx: &Transaction, db: &B) -> Result<(), ValidationError> {
        verify_not_stxos(tx, db)?;
        verify_inputs_are_utxos(tx, db)?;
        Ok(())
    }
}

/// This validator will only check timelocks, it will check that kernel lock heights and output maturities have passed.
pub struct TimeLockTxValidator {}

impl<B: BlockchainBackend> Validation<Transaction, B> for TimeLockTxValidator {
    fn validate(&self, tx: &Transaction, db: &B) -> Result<(), ValidationError> {
        let tip_height = db
            .fetch_metadata()
            .map_err(|e| ValidationError::CustomError(e.to_string()))?
            .height_of_longest_chain
            .unwrap_or(0);
        verify_timelocks(tx, tip_height)?;
        Ok(())
    }
}

// This function verifies that the provided transaction is internally sound and that no funds were created in the
// transaction.
fn verify_tx(tx: &Transaction, factories: &CryptoFactories) -> Result<(), ValidationError> {
    tx.validate_internal_consistency(factories, None)
        .map_err(ValidationError::TransactionError)
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
        if is_stxo(db, input.hash()).map_err(|e| ValidationError::CustomError(e.to_string()))? {
            // we dont want to log this as a node or wallet might retransmit a transaction
            debug!(
                target: LOG_TARGET,
                "Transaction validation failed due to already spent input: {}", input
            );
            return Err(ValidationError::ContainsSTxO);
        }
    }
    for output in tx.body.outputs() {
        if is_stxo(db, output.hash()).map_err(|e| ValidationError::CustomError(e.to_string()))? {
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
        if !(is_utxo(db, input.hash())).map_err(|e| ValidationError::CustomError(e.to_string()))? {
            warn!(
                target: LOG_TARGET,
                "Transaction validation failed due to unknown input: {}", input
            );
            return Err(ValidationError::UnknownInputs);
        }
    }
    Ok(())
}
