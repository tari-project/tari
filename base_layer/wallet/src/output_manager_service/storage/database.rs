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

use crate::output_manager_service::{error::OutputManagerStorageError, TxId};
use chrono::NaiveDateTime;
use log::*;
use std::{
    collections::HashMap,
    fmt::{Display, Error, Formatter},
    time::Duration,
};
use tari_core::{tari_amount::MicroTari, transaction::UnblindedOutput, types::BlindingFactor};

const LOG_TARGET: &'static str = "wallet::output_manager_service::database";

/// This trait defines the required behaviour that a storage backend must provide for the Output Manager service.
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. If new data types are
/// required to be supported by the backends then these enums can be updated to reflect this requirement and the trait
/// will remain the same
pub trait OutputManagerBackend {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError>;
    /// Check if a record with the provided key exists in the backend.
    fn contains(&self, key: &DbKey) -> Result<bool, OutputManagerStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&mut self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError>;
    /// This method is called when a pending transaction is to be confirmed. It must move the `outputs_to_be_spent` and
    /// `outputs_to_be_received` from a `PendingTransactionOutputs` record into the `unspent_outputs` and
    /// `spent_outputs` collections.
    fn confirm_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// This method encumbers the specified outputs into a `PendingTransactionOutputs` record. This reserves these
    /// outputs until the transaction is confirmed or cancelled
    fn encumber_outputs(
        &mut self,
        tx_id: TxId,
        outputs_to_send: Vec<UnblindedOutput>,
        change_output: Option<UnblindedOutput>,
    ) -> Result<(), OutputManagerStorageError>;
    /// This method must take all the `outputs_to_be_spent` from the specified transaction and move them back into the
    /// `UnspentOutputs` pool.
    fn cancel_pending_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// This method must run through all the `PendingTransactionOutputs` and test if any have existed for longer that
    /// the specified duration. If they have they should be cancelled.
    fn timeout_pending_transactions(&mut self, period: Duration) -> Result<(), OutputManagerStorageError>;
}

/// Holds the outputs that have been selected for a given pending transaction waiting for confirmation
#[derive(Debug, Clone)]
pub struct PendingTransactionOutputs {
    pub tx_id: u64,
    pub outputs_to_be_spent: Vec<UnblindedOutput>,
    pub outputs_to_be_received: Vec<UnblindedOutput>,
    pub timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    SpentOutput(BlindingFactor),
    UnspentOutput(BlindingFactor),
    PendingTransactionOutputs(TxId),
    UnspentOutputs,
    SpentOutputs,
    AllPendingTransactionOutputs,
}

#[derive(Debug)]
pub enum DbValue {
    SpentOutput(Box<UnblindedOutput>),
    UnspentOutput(Box<UnblindedOutput>),
    PendingTransactionOutputs(Box<PendingTransactionOutputs>),
    UnspentOutputs(Box<Vec<UnblindedOutput>>),
    SpentOutputs(Box<Vec<UnblindedOutput>>),
    AllPendingTransactionOutputs(Box<HashMap<TxId, PendingTransactionOutputs>>),
}

pub enum DbKeyValuePair {
    SpentOutput(BlindingFactor, Box<UnblindedOutput>),
    UnspentOutput(BlindingFactor, Box<UnblindedOutput>),
    PendingTransactionOutputs(TxId, Box<PendingTransactionOutputs>),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($self:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $self.db.fetch(&key) {
            Ok(None) => Err(OutputManagerStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
}

/// This structure holds an inner type that implements the `OutputManagerBackend` trait and contains the more complex
/// data access logic required by the module built onto the functionality defined by the trait
pub struct OutputManagerDatabase<T>
where T: OutputManagerBackend
{
    db: T,
}

impl<T> OutputManagerDatabase<T>
where T: OutputManagerBackend
{
    pub fn new(db: T) -> Self {
        Self { db }
    }

    pub fn add_unspent_output(&mut self, output: UnblindedOutput) -> Result<(), OutputManagerStorageError> {
        self.db.write(WriteOperation::Insert(DbKeyValuePair::UnspentOutput(
            output.spending_key.clone(),
            Box::new(output),
        )))?;

        Ok(())
    }

    pub fn get_balance(&self) -> Result<MicroTari, OutputManagerStorageError> {
        if let DbValue::UnspentOutputs(uo) =
            self.db
                .fetch(&DbKey::UnspentOutputs)?
                .ok_or(OutputManagerStorageError::UnexpectedResult(
                    "Unspent Outputs cannot be retrieved".to_string(),
                ))?
        {
            Ok(uo.iter().fold(MicroTari::from(0), |acc, x| acc + x.value))
        } else {
            Err(OutputManagerStorageError::UnexpectedResult(
                "Unexpected result from database backend".to_string(),
            ))
        }
    }

    pub fn add_pending_transaction_outputs(
        &mut self,
        pending_transaction_ouputs: PendingTransactionOutputs,
    ) -> Result<(), OutputManagerStorageError>
    {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::PendingTransactionOutputs(
                pending_transaction_ouputs.tx_id.clone(),
                Box::new(pending_transaction_ouputs),
            )))?;

        Ok(())
    }

    pub fn fetch_pending_transaction_outputs(
        &self,
        tx_id: TxId,
    ) -> Result<PendingTransactionOutputs, OutputManagerStorageError>
    {
        fetch!(self, tx_id, PendingTransactionOutputs)
    }

    /// This method is called when a pending transaction is confirmed. It moves the `outputs_to_be_spent` and
    /// `outputs_to_be_received` from a `PendingTransactionOutputs` record into the `unspent_outputs` and
    /// `spent_outputs` collections.
    pub fn confirm_pending_transaction_outputs(&mut self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        self.db.confirm_transaction(tx_id)
    }

    /// This method is called when a transaction is built to be sent. It will encumber unspent outputs against a pending
    /// transaction
    pub fn encumber_outputs(
        &mut self,
        tx_id: TxId,
        outputs_to_send: Vec<UnblindedOutput>,
        change_output: Option<UnblindedOutput>,
    ) -> Result<(), OutputManagerStorageError>
    {
        self.db.encumber_outputs(tx_id, outputs_to_send, change_output)
    }

    /// When a pending transaction is cancelled the encumbered outputs are moved back to the `unspent_outputs`
    /// collection.
    pub fn cancel_pending_transaction_outputs(&mut self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let pending_tx = fetch!(self, tx_id, PendingTransactionOutputs)?;

        for o in pending_tx.outputs_to_be_spent.iter() {
            self.db.write(WriteOperation::Insert(DbKeyValuePair::UnspentOutput(
                o.spending_key.clone(),
                Box::new(o.clone()),
            )))?;
        }

        // REMOVE PENDING TX
        Ok(())
    }

    /// This method is check all pending transactions to see if any are older that the provided duration. If they are
    /// they will be cancelled.
    pub fn timeout_pending_transaction_outputs(&mut self, period: Duration) -> Result<(), OutputManagerStorageError> {
        self.db.timeout_pending_transactions(period)
    }

    pub fn fetch_sorted_unspent_outputs(&self) -> Result<Vec<UnblindedOutput>, OutputManagerStorageError> {
        let mut uo = match self.db.fetch(&DbKey::UnspentOutputs) {
            Ok(None) => log_error(
                DbKey::UnspentOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve unspent outputs".to_string()),
            ),
            Ok(Some(DbValue::UnspentOutputs(uo))) => Ok(*uo),
            Ok(Some(other)) => unexpected_result(DbKey::UnspentOutputs, other),
            Err(e) => log_error(DbKey::UnspentOutputs, e),
        }?;

        uo.sort();

        Ok(uo)
    }

    pub fn fetch_spent_outputs(&self) -> Result<Vec<UnblindedOutput>, OutputManagerStorageError> {
        let uo = match self.db.fetch(&DbKey::SpentOutputs) {
            Ok(None) => log_error(
                DbKey::UnspentOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve spent outputs".to_string()),
            ),
            Ok(Some(DbValue::SpentOutputs(uo))) => Ok(*uo),
            Ok(Some(other)) => unexpected_result(DbKey::SpentOutputs, other),
            Err(e) => log_error(DbKey::SpentOutputs, e),
        }?;
        Ok(uo)
    }

    pub fn fetch_all_pending_transaction_outputs(
        &self,
    ) -> Result<HashMap<u64, PendingTransactionOutputs>, OutputManagerStorageError> {
        let uo = match self.db.fetch(&DbKey::AllPendingTransactionOutputs) {
            Ok(None) => log_error(
                DbKey::AllPendingTransactionOutputs,
                OutputManagerStorageError::UnexpectedResult(
                    "Could not retrieve pending transaction outputs".to_string(),
                ),
            ),
            Ok(Some(DbValue::AllPendingTransactionOutputs(pt))) => Ok(*pt),
            Ok(Some(other)) => unexpected_result(DbKey::AllPendingTransactionOutputs, other),
            Err(e) => log_error(DbKey::AllPendingTransactionOutputs, e),
        }?;
        Ok(uo)
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
            DbKey::SpentOutput(_) => f.write_str(&format!("Spent Output Key")),
            DbKey::UnspentOutput(_) => f.write_str(&format!("Unspent Output Key")),
            DbKey::PendingTransactionOutputs(tx_id) => {
                f.write_str(&format!("Pending Transaction Outputs TX_ID: {}", tx_id))
            },
            DbKey::UnspentOutputs => f.write_str(&format!("Unspent Outputs Key")),
            DbKey::SpentOutputs => f.write_str(&format!("Spent Outputs Key")),
            DbKey::AllPendingTransactionOutputs => f.write_str(&format!("All Pending Transaction Outputs")),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::SpentOutput(_) => f.write_str("Spent Output"),
            DbValue::UnspentOutput(_) => f.write_str("Unspent Output"),
            DbValue::PendingTransactionOutputs(_) => f.write_str("Pending Transaction Outputs"),
            DbValue::UnspentOutputs(_) => f.write_str("Unspent Outputs"),
            DbValue::SpentOutputs(_) => f.write_str("Spent Outputs"),
            DbValue::AllPendingTransactionOutputs(_) => f.write_str("All Pending Transaction Outputs"),
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
