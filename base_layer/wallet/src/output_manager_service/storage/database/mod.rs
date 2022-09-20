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

mod backend;
use std::{
    fmt::{Debug, Display, Error, Formatter},
    sync::Arc,
};

pub use backend::OutputManagerBackend;
use chacha20poly1305::XChaCha20Poly1305;
use log::*;
use tari_common_types::{
    transaction::TxId,
    types::{BlindingFactor, Commitment, HashOutput},
};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction_components::{OutputType, TransactionOutput},
};
use tari_utilities::hex::Hex;

use crate::output_manager_service::{
    error::OutputManagerStorageError,
    input_selection::UtxoSelectionCriteria,
    service::Balance,
    storage::{
        models::{DbUnblindedOutput, KnownOneSidedPaymentScript},
        OutputStatus,
    },
};

const LOG_TARGET: &str = "wallet::output_manager_service::database";

#[derive(Debug, Copy, Clone)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
pub struct OutputBackendQuery {
    pub tip_height: i64,
    pub status: Vec<OutputStatus>,
    pub commitments: Vec<Commitment>,
    pub pagination: Option<(i64, i64)>,
    pub value_min: Option<(i64, bool)>,
    pub value_max: Option<(i64, bool)>,
    pub sorting: Vec<(&'static str, SortDirection)>,
}

impl Default for OutputBackendQuery {
    fn default() -> Self {
        Self {
            tip_height: i64::MAX,
            status: vec![OutputStatus::Spent],
            commitments: vec![],
            pagination: None,
            value_min: None,
            value_max: None,
            sorting: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    SpentOutput(BlindingFactor),
    UnspentOutput(BlindingFactor),
    UnspentOutputHash(HashOutput),
    AnyOutputByCommitment(Commitment),
    TimeLockedUnspentOutputs(u64),
    UnspentOutputs,
    SpentOutputs,
    InvalidOutputs,
    KnownOneSidedPaymentScripts,
    OutputsByTxIdAndStatus(TxId, OutputStatus),
}

#[derive(Debug)]
pub enum DbValue {
    SpentOutput(Box<DbUnblindedOutput>),
    UnspentOutput(Box<DbUnblindedOutput>),
    UnspentOutputs(Vec<DbUnblindedOutput>),
    SpentOutputs(Vec<DbUnblindedOutput>),
    InvalidOutputs(Vec<DbUnblindedOutput>),
    KnownOneSidedPaymentScripts(Vec<KnownOneSidedPaymentScript>),
    AnyOutput(Box<DbUnblindedOutput>),
    AnyOutputs(Vec<DbUnblindedOutput>),
}

pub enum DbKeyValuePair {
    UnspentOutput(Commitment, Box<DbUnblindedOutput>),
    UnspentOutputWithTxId(Commitment, (TxId, Box<DbUnblindedOutput>)),
    OutputToBeReceived(Commitment, (TxId, Box<DbUnblindedOutput>, Option<u64>)),
    KnownOneSidedPaymentScripts(KnownOneSidedPaymentScript),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

/// This structure holds an inner type that implements the `OutputManagerBackend` trait and contains the more complex
/// data access logic required by the module built onto the functionality defined by the trait
#[derive(Clone)]
pub struct OutputManagerDatabase<T> {
    db: Arc<T>,
}

impl<T> OutputManagerDatabase<T>
where T: OutputManagerBackend + 'static
{
    pub fn new(db: T) -> Self {
        Self { db: Arc::new(db) }
    }

    pub fn add_unspent_output(&self, output: DbUnblindedOutput) -> Result<(), OutputManagerStorageError> {
        self.db.write(WriteOperation::Insert(DbKeyValuePair::UnspentOutput(
            output.commitment.clone(),
            Box::new(output),
        )))?;

        Ok(())
    }

    pub fn add_unspent_output_with_tx_id(
        &self,
        tx_id: TxId,
        output: DbUnblindedOutput,
    ) -> Result<(), OutputManagerStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::UnspentOutputWithTxId(
                output.commitment.clone(),
                (tx_id, Box::new(output)),
            )))?;

        Ok(())
    }

    pub fn add_unvalidated_output(
        &self,
        tx_id: TxId,
        output: DbUnblindedOutput,
    ) -> Result<(), OutputManagerStorageError> {
        self.db.add_unvalidated_output(output, tx_id)?;

        Ok(())
    }

    pub fn add_output_to_be_received(
        &self,
        tx_id: TxId,
        output: DbUnblindedOutput,
        coinbase_block_height: Option<u64>,
    ) -> Result<(), OutputManagerStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::OutputToBeReceived(
                output.commitment.clone(),
                (tx_id, Box::new(output), coinbase_block_height),
            )))?;

        Ok(())
    }

    pub fn get_balance(
        &self,
        current_tip_for_time_lock_calculation: Option<u64>,
    ) -> Result<Balance, OutputManagerStorageError> {
        self.db.get_balance(current_tip_for_time_lock_calculation)
    }

    /// This method is called when a transaction is built to be sent. It will encumber unspent outputs against a pending
    /// transaction in the short term.
    pub fn encumber_outputs(
        &self,
        tx_id: TxId,
        outputs_to_send: Vec<DbUnblindedOutput>,
        outputs_to_receive: Vec<DbUnblindedOutput>,
    ) -> Result<(), OutputManagerStorageError> {
        self.db
            .short_term_encumber_outputs(tx_id, &outputs_to_send, &outputs_to_receive)
    }

    /// This method is called when a transaction is finished being negotiated. This will fully encumber the outputs
    /// against a pending transaction.
    pub fn confirm_encumbered_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        self.db.confirm_encumbered_outputs(tx_id)
    }

    /// Clear all pending transaction encumberances marked as short term. These are the result of an unfinished
    /// transaction negotiation
    pub fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError> {
        self.db.clear_short_term_encumberances()
    }

    /// When a pending transaction is cancelled the encumbered outputs are moved back to the `unspent_outputs`
    /// collection.
    pub fn cancel_pending_transaction_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        self.db.cancel_pending_transaction(tx_id)
    }

    pub fn fetch_all_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let result = match self.db.fetch(&DbKey::UnspentOutputs)? {
            Some(DbValue::UnspentOutputs(outputs)) => outputs,
            Some(other) => return unexpected_result(DbKey::UnspentOutputs, other),
            None => vec![],
        };
        Ok(result)
    }

    pub fn fetch_by_commitment(
        &self,
        commitment: Commitment,
    ) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let result = match self.db.fetch(&DbKey::AnyOutputByCommitment(commitment))? {
            Some(DbValue::UnspentOutputs(outputs)) => outputs,
            Some(other) => return unexpected_result(DbKey::UnspentOutputs, other),
            None => vec![],
        };
        Ok(result)
    }

    pub fn fetch_with_features(
        &self,
        feature: OutputType,
    ) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        self.db.fetch_with_features(feature)
    }

    /// Retrieves UTXOs than can be spent, sorted by priority, then value from smallest to largest.
    pub fn fetch_unspent_outputs_for_spending(
        &self,
        selection_criteria: &UtxoSelectionCriteria,
        amount: MicroTari,
        tip_height: Option<u64>,
    ) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let utxos = self
            .db
            .fetch_unspent_outputs_for_spending(selection_criteria, amount.as_u64(), tip_height)?;
        Ok(utxos)
    }

    pub fn fetch_spent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let uo = match self.db.fetch(&DbKey::SpentOutputs) {
            Ok(None) => log_error(
                DbKey::SpentOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve spent outputs".to_string()),
            ),
            Ok(Some(DbValue::SpentOutputs(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::SpentOutputs, other),
            Err(e) => log_error(DbKey::SpentOutputs, e),
        }?;
        Ok(uo)
    }

    pub fn fetch_unconfirmed_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let utxos = self.db.fetch_unspent_mined_unconfirmed_outputs()?;
        Ok(utxos)
    }

    pub fn fetch_sorted_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let mut utxos = self.db.fetch_sorted_unspent_outputs()?;
        utxos.sort();
        Ok(utxos)
    }

    pub fn fetch_mined_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let utxos = self.db.fetch_mined_unspent_outputs()?;
        Ok(utxos)
    }

    pub fn get_timelocked_outputs(&self, tip: u64) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let uo = match self.db.fetch(&DbKey::TimeLockedUnspentOutputs(tip)) {
            Ok(None) => log_error(
                DbKey::UnspentOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve unspent outputs".to_string()),
            ),
            Ok(Some(DbValue::UnspentOutputs(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::UnspentOutputs, other),
            Err(e) => log_error(DbKey::UnspentOutputs, e),
        }?;
        Ok(uo)
    }

    pub fn get_invalid_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let uo = match self.db.fetch(&DbKey::InvalidOutputs) {
            Ok(None) => log_error(
                DbKey::InvalidOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve invalid outputs".to_string()),
            ),
            Ok(Some(DbValue::InvalidOutputs(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::InvalidOutputs, other),
            Err(e) => log_error(DbKey::InvalidOutputs, e),
        }?;
        Ok(uo)
    }

    pub fn update_output_metadata_signature(&self, output: TransactionOutput) -> Result<(), OutputManagerStorageError> {
        self.db.update_output_metadata_signature(&output)
    }

    pub fn revalidate_output(&self, commitment: Commitment) -> Result<(), OutputManagerStorageError> {
        self.db.revalidate_unspent_output(&commitment)
    }

    pub fn reinstate_cancelled_inbound_output(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        self.db.reinstate_cancelled_inbound_output(tx_id)
    }

    pub fn apply_encryption(&self, cipher: XChaCha20Poly1305) -> Result<(), OutputManagerStorageError> {
        self.db.apply_encryption(cipher)
    }

    pub fn remove_encryption(&self) -> Result<(), OutputManagerStorageError> {
        self.db.remove_encryption()
    }

    pub fn get_all_known_one_sided_payment_scripts(
        &self,
    ) -> Result<Vec<KnownOneSidedPaymentScript>, OutputManagerStorageError> {
        let scripts = match self.db.fetch(&DbKey::KnownOneSidedPaymentScripts) {
            Ok(None) => log_error(
                DbKey::KnownOneSidedPaymentScripts,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve known scripts".to_string()),
            ),
            Ok(Some(DbValue::KnownOneSidedPaymentScripts(scripts))) => Ok(scripts),
            Ok(Some(other)) => unexpected_result(DbKey::KnownOneSidedPaymentScripts, other),
            Err(e) => log_error(DbKey::KnownOneSidedPaymentScripts, e),
        }?;
        Ok(scripts)
    }

    pub fn get_unspent_output(&self, output: HashOutput) -> Result<DbUnblindedOutput, OutputManagerStorageError> {
        let uo = match self.db.fetch(&DbKey::UnspentOutputHash(output)) {
            Ok(None) => log_error(
                DbKey::UnspentOutputHash(output),
                OutputManagerStorageError::UnexpectedResult(
                    "Could not retrieve unspent output: ".to_string() + &output.to_hex(),
                ),
            ),
            Ok(Some(DbValue::UnspentOutput(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::UnspentOutputHash(output), other),
            Err(e) => log_error(DbKey::UnspentOutputHash(output), e),
        }?;
        Ok(*uo)
    }

    pub fn get_last_mined_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError> {
        self.db.get_last_mined_output()
    }

    pub fn get_last_spent_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError> {
        self.db.get_last_spent_output()
    }

    pub fn add_known_script(&self, known_script: KnownOneSidedPaymentScript) -> Result<(), OutputManagerStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::KnownOneSidedPaymentScripts(
                known_script,
            )))?;

        Ok(())
    }

    pub fn remove_output_by_commitment(&self, commitment: Commitment) -> Result<(), OutputManagerStorageError> {
        match self
            .db
            .write(WriteOperation::Remove(DbKey::AnyOutputByCommitment(commitment.clone())))
        {
            Ok(None) => Ok(()),
            Ok(Some(DbValue::AnyOutput(_))) => Ok(()),
            Ok(Some(other)) => unexpected_result(DbKey::AnyOutputByCommitment(commitment), other),
            Err(e) => log_error(DbKey::AnyOutputByCommitment(commitment), e),
        }?;
        Ok(())
    }

    pub fn set_received_output_mined_height_and_status(
        &self,
        hash: HashOutput,
        mined_height: u64,
        mined_in_block: HashOutput,
        mmr_position: u64,
        confirmed: bool,
        mined_timestamp: u64,
    ) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        db.set_received_output_mined_height_and_status(
            hash,
            mined_height,
            mined_in_block,
            mmr_position,
            confirmed,
            mined_timestamp,
        )?;
        Ok(())
    }

    pub fn set_output_to_unmined_and_invalid(&self, hash: HashOutput) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        db.set_output_to_unmined_and_invalid(hash)?;
        Ok(())
    }

    pub fn set_outputs_to_be_revalidated(&self) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        db.set_outputs_to_be_revalidated()?;
        Ok(())
    }

    pub fn mark_output_as_spent(
        &self,
        hash: HashOutput,
        deleted_height: u64,
        deleted_in_block: HashOutput,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        db.mark_output_as_spent(hash, deleted_height, deleted_in_block, confirmed)?;
        Ok(())
    }

    pub fn mark_output_as_unspent(&self, hash: HashOutput) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        db.mark_output_as_unspent(hash)?;
        Ok(())
    }

    pub fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        db.set_coinbase_abandoned(tx_id, abandoned)?;
        Ok(())
    }

    pub fn fetch_outputs_by_tx_id(&self, tx_id: TxId) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let outputs = self.db.fetch_outputs_by_tx_id(tx_id)?;
        Ok(outputs)
    }

    pub fn fetch_outputs_by(&self, q: OutputBackendQuery) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        self.db.fetch_outputs_by(q)
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, OutputManagerStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(OutputManagerStorageError::UnexpectedResult(msg))
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::SpentOutput(_) => f.write_str("Spent Output Key"),
            DbKey::UnspentOutput(_) => f.write_str("Unspent Output Key"),
            DbKey::UnspentOutputHash(_) => f.write_str("Unspent Output Hash Key"),
            DbKey::UnspentOutputs => f.write_str("Unspent Outputs Key"),
            DbKey::SpentOutputs => f.write_str("Spent Outputs Key"),
            DbKey::InvalidOutputs => f.write_str("Invalid Outputs Key"),
            DbKey::TimeLockedUnspentOutputs(_t) => f.write_str("Timelocked Outputs"),
            DbKey::KnownOneSidedPaymentScripts => f.write_str("Known claiming scripts"),
            DbKey::AnyOutputByCommitment(_) => f.write_str("AnyOutputByCommitment"),
            DbKey::OutputsByTxIdAndStatus(_, _) => f.write_str("OutputsByTxIdAndStatus"),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::SpentOutput(_) => f.write_str("Spent Output"),
            DbValue::UnspentOutput(_) => f.write_str("Unspent Output"),
            DbValue::UnspentOutputs(_) => f.write_str("Unspent Outputs"),
            DbValue::SpentOutputs(_) => f.write_str("Spent Outputs"),
            DbValue::InvalidOutputs(_) => f.write_str("Invalid Outputs"),
            DbValue::KnownOneSidedPaymentScripts(_) => f.write_str("Known claiming scripts"),
            DbValue::AnyOutput(_) => f.write_str("Any Output"),
            DbValue::AnyOutputs(_) => f.write_str("Any Outputs"),
        }
    }
}

fn log_error<T>(req: DbKey, err: OutputManagerStorageError) -> Result<T, OutputManagerStorageError> {
    error!(
        target: LOG_TARGET,
        "Database access error on request: {}: {}",
        req,
        err.to_string()
    );
    Err(err)
}
