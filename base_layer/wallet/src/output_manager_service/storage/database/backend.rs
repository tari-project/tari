// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::ops::Range;

use tari_common_types::{
    transaction::TxId,
    types::{CompressedCommitment, CompressedSignature, FixedHash},
};
use tari_transaction_components::{
    MicroMinotari,
    transaction_components::{OutputType, TransactionOutput},
};
use tari_transaction_key_manager::legacy_key_manager::LegacyTransactionKeyManagerInterface;

use crate::output_manager_service::{
    error::OutputManagerStorageError,
    input_selection::UtxoSelectionCriteria,
    service::Balance,
    storage::{
        database::{DbKey, DbValue, OutputBackendQuery, WriteOperation},
        models::DbWalletOutput,
        sqlite_db::{CoinBucket, ReceivedOutputInfoForBatch, SpentOutputInfoForBatch},
    },
};

/// This trait defines the required behaviour that a storage backend must provide for the Output Manager service.
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. If new data types are
/// required to be supported by the backends then these enums can be updated to reflect this requirement and the trait
/// will remain the same
pub trait OutputManagerBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        key: &DbKey,
        key_manager: &KM,
    ) -> Result<Option<DbValue>, OutputManagerStorageError>;
    /// Fetch outputs with specific features
    fn fetch_with_features<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        features: OutputType,
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Retrieve unspent outputs.
    fn fetch_sorted_unspent_outputs<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Retrieve outputs that have been mined but not spent yet (have not been deleted)
    fn fetch_mined_unspent_outputs<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Retrieve outputs that are invalid
    fn fetch_invalid_outputs<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        timestamp: i64,
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Retrieve all outputs matching the provided hashes
    fn fetch_many_outputs<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        outputs: &[FixedHash],
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Retrieve outputs that have not been found or confirmed in the block chain yet
    fn fetch_unspent_mined_unconfirmed_outputs<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        op: WriteOperation,
        key_manager: &KM,
    ) -> Result<Option<DbValue>, OutputManagerStorageError>;
    fn fetch_pending_incoming_outputs<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Perform a batch update of the received outputs' mined height and status
    fn set_received_outputs_mined_height_and_statuses(
        &self,
        updates: Vec<ReceivedOutputInfoForBatch>,
    ) -> Result<(), OutputManagerStorageError>;
    /// Perform a batch update of the outputs' unmined and invalid state
    fn set_outputs_to_unmined_and_invalid(&self, hashes: Vec<FixedHash>) -> Result<(), OutputManagerStorageError>;
    /// Fetch kernel signature (nonce, key) for a completed transaction by tx_id.
    /// Used to verify mempool presence of encumbered outputs' parent transactions.
    fn fetch_kernel_signature_for_tx(
        &self,
        tx_id: TxId,
    ) -> Result<Option<CompressedSignature>, OutputManagerStorageError>;
    /// Restore invalid outputs that are found in the mempool back to EncumberedToBeReceived status.
    fn set_outputs_to_encumbered_to_be_received(
        &self,
        commitments: Vec<CompressedCommitment>,
    ) -> Result<(), OutputManagerStorageError>;
    /// Perform a batch update of the outputs' last validation timestamp
    fn update_last_validation_timestamps(
        &self,
        commitments: Vec<CompressedCommitment>,
    ) -> Result<(), OutputManagerStorageError>;
    fn set_outputs_to_be_revalidated(&self) -> Result<(), OutputManagerStorageError>;
    /// Perform a batch update of the outputs' spent status
    fn mark_outputs_as_spent(&self, updates: Vec<SpentOutputInfoForBatch>) -> Result<(), OutputManagerStorageError>;
    /// Perform a batch update of the outputs' unspent status
    fn mark_outputs_as_unspent(&self, hashes: Vec<(FixedHash, bool)>) -> Result<(), OutputManagerStorageError>;
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
    fn confirm_encumbered_outputs(
        &self,
        tx_id: TxId,
        tx_id_update: Option<TxId>,
        change_outputs_to_update: &[DbWalletOutput],
    ) -> Result<(), OutputManagerStorageError>;
    /// Clear all pending transaction encumberances marked as short term. These are the result of an unfinished
    /// transaction negotiation
    fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError>;
    /// This method must take all the `outputs_to_be_spent` from the specified transaction and move them back into the
    /// `UnspentOutputs` pool. The `outputs_to_be_received`'` will be marked as cancelled inbound outputs in case they
    /// need to be recovered.
    fn cancel_pending_or_completed_transaction(
        &self,
        tx_id: TxId,
        pending: bool,
    ) -> Result<(), OutputManagerStorageError>;
    /// This method will update an output's metadata signature, akin to 'finalize output'
    fn update_output_metadata_signature(&self, output: &TransactionOutput) -> Result<(), OutputManagerStorageError>;
    /// If an invalid output is found to be valid this function will turn it back into an unspent output
    fn revalidate_unspent_output(&self, spending_key: &CompressedCommitment) -> Result<(), OutputManagerStorageError>;

    /// Get the output that was most recently mined, ordered descending by mined height
    fn get_last_mined_output<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<Option<DbWalletOutput>, OutputManagerStorageError>;
    /// Get the output that was most recently spent, ordered descending by mined height
    fn get_last_spent_output<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<Option<DbWalletOutput>, OutputManagerStorageError>;
    fn get_last_scanned_height(&self) -> Result<Option<u64>, OutputManagerStorageError>;
    fn save_last_scanned_height(
        &self,
        scanned_block: crate::utxo_scanner_service::service::ScannedBlock,
    ) -> Result<(), OutputManagerStorageError>;
    /// Reinstate a cancelled inbound output
    fn reinstate_cancelled_inbound_output(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// Return the available, time locked, pending incoming and pending outgoing balance
    fn get_balance(&self, tip: Option<u64>) -> Result<Balance, OutputManagerStorageError>;
    /// Count outputs in the specified ranges
    fn count_outputs_in_ranges(
        &self,
        ranges: Vec<Range<u64>>,
        tip_height: Option<u64>,
    ) -> Result<Vec<CoinBucket>, OutputManagerStorageError>;
    /// Return the available, time locked, pending incoming and pending outgoing balance only matching the payment id
    fn get_balance_payment_id(
        &self,
        tip: Option<u64>,
        payment_id: Vec<u8>,
    ) -> Result<Balance, OutputManagerStorageError>;
    /// Import unvalidated output
    fn add_unvalidated_output(&self, output: DbWalletOutput, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// Retrieves UTXOs within a specified limited range with minimum target amount for spending
    fn get_range_limited_outputs_for_spending<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        selection_criteria: &UtxoSelectionCriteria,
        tip_height: Option<u64>,
        key_manager: &KM,
    ) -> Result<(Vec<DbWalletOutput>, MicroMinotari), OutputManagerStorageError>;
    fn fetch_unspent_outputs_for_spending<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        selection_criteria: &UtxoSelectionCriteria,
        amount: u64,
        current_tip_height: Option<u64>,
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    fn fetch_outputs_by_tx_id<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        tx_id: TxId,
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    fn fetch_outputs_by_query<KM: LegacyTransactionKeyManagerInterface>(
        &self,
        q: OutputBackendQuery,
        key_manager: &KM,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerStorageError>;
    /// Fetch a batch of outputs whose `spending_key` or `script_private_key` columns still hold a legacy key-id
    /// string. Only the three columns required for migration are returned (no BLOBs).
    ///
    /// Keyset pagination: rows are filtered by `id > last_id` and returned ordered ascending. Callers pass
    /// `last_id = 0` on the first call, then the last id from the previous batch. This guarantees forward
    /// progress through the table even if some rows fail to convert and remain in the filter.
    fn fetch_outputs_with_legacy_key_ids(
        &self,
        last_id: i32,
        batch_size: i64,
    ) -> Result<Vec<(i32, String, String)>, OutputManagerStorageError>;
    /// Update the `spending_key` and `script_private_key` columns for the output identified by `output_id`.
    fn update_output_key_ids(
        &self,
        output_id: i32,
        spending_key: String,
        script_private_key: String,
    ) -> Result<(), OutputManagerStorageError>;
}
