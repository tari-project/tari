// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::{
    transaction::TxId,
    types::{Commitment, FixedHash},
};
use tari_core::transactions::transaction_components::{OutputType, TransactionOutput};

use crate::output_manager_service::{
    error::OutputManagerStorageError,
    input_selection::UtxoSelectionCriteria,
    service::Balance,
    storage::{
        database::{DbKey, DbValue, OutputBackendQuery, WriteOperation},
        models::DbWalletOutput,
    },
};

/// This trait defines the required behaviour that a storage backend must provide for the Output Manager service.
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. If new data types are
/// required to be supported by the backends then these enums can be updated to reflect this requirement and the trait
/// will remain the same
pub trait OutputManagerBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError>;
    /// Fetch outputs with specific features
    fn fetch_with_features(&self, features: OutputType) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Retrieve unspent outputs.
    fn fetch_sorted_unspent_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Retrieve outputs that have been mined but not spent yet (have not been deleted)
    fn fetch_mined_unspent_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Retrieve outputs that are invalid
    fn fetch_invalid_outputs(&self, timestamp: i64) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Retrieve outputs that have not been found or confirmed in the block chain yet
    fn fetch_unspent_mined_unconfirmed_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError>;
    fn fetch_pending_incoming_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;

    fn set_received_output_mined_height_and_status(
        &self,
        hash: FixedHash,
        mined_height: u64,
        mined_in_block: FixedHash,
        confirmed: bool,
        mined_timestamp: u64,
    ) -> Result<(), OutputManagerStorageError>;

    fn set_output_to_unmined_and_invalid(&self, hash: FixedHash) -> Result<(), OutputManagerStorageError>;
    fn update_last_validation_timestamp(&self, hash: FixedHash) -> Result<(), OutputManagerStorageError>;
    fn set_outputs_to_be_revalidated(&self) -> Result<(), OutputManagerStorageError>;

    fn mark_output_as_spent(
        &self,
        hash: FixedHash,
        mark_deleted_at_height: u64,
        mark_deleted_in_block: FixedHash,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError>;

    fn mark_output_as_unspent(&self, hash: FixedHash) -> Result<(), OutputManagerStorageError>;
    /// This method encumbers the specified outputs into a `PendingTransactionOutputs` record. This is a short term
    /// encumberance in case the app is closed or crashes before transaction neogtiation is complete. These will be
    /// cleared on startup of the service.
    fn short_term_encumber_outputs(
        &self,
        tx_id: TxId,
        outputs_to_send: &[DbWalletOutput],
        outputs_to_receive: &[DbWalletOutput],
    ) -> Result<(), OutputManagerStorageError>;
    /// This method confirms that a transaction negotiation is complete and outputs can be fully encumbered. This
    /// reserves these outputs until the transaction is confirmed or cancelled
    fn confirm_encumbered_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// Clear all pending transaction encumberances marked as short term. These are the result of an unfinished
    /// transaction negotiation
    fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError>;
    /// This method must take all the `outputs_to_be_spent` from the specified transaction and move them back into the
    /// `UnspentOutputs` pool. The `outputs_to_be_received`'` will be marked as cancelled inbound outputs in case they
    /// need to be recovered.
    fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// This method will update an output's metadata signature, akin to 'finalize output'
    fn update_output_metadata_signature(&self, output: &TransactionOutput) -> Result<(), OutputManagerStorageError>;
    /// If an invalid output is found to be valid this function will turn it back into an unspent output
    fn revalidate_unspent_output(&self, spending_key: &Commitment) -> Result<(), OutputManagerStorageError>;

    /// Get the output that was most recently mined, ordered descending by mined height
    fn get_last_mined_output(&self) -> Result<Option<DbWalletOutput>, OutputManagerStorageError>;
    /// Get the output that was most recently spent, ordered descending by mined height
    fn get_last_spent_output(&self) -> Result<Option<DbWalletOutput>, OutputManagerStorageError>;
    /// Set if a coinbase output is abandoned or not
    fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerStorageError>;
    /// Reinstate a cancelled inbound output
    fn reinstate_cancelled_inbound_output(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// Return the available, time locked, pending incoming and pending outgoing balance
    fn get_balance(&self, tip: Option<u64>) -> Result<Balance, OutputManagerStorageError>;
    /// Import unvalidated output
    fn add_unvalidated_output(&self, output: DbWalletOutput, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    fn fetch_unspent_outputs_for_spending(
        &self,
        selection_criteria: &UtxoSelectionCriteria,
        amount: u64,
        current_tip_height: Option<u64>,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    fn fetch_outputs_by_tx_id(&self, tx_id: TxId) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    fn fetch_outputs_by(&self, q: OutputBackendQuery) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
}
