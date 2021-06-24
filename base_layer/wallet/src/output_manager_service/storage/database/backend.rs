use crate::output_manager_service::storage::database::{DbKey, DbValue, WriteOperation};
use crate::output_manager_service::error::OutputManagerStorageError;
use crate::output_manager_service::storage::models::DbUnblindedOutput;
use std::time::Duration;
use tari_core::transactions::types::{Commitment, PublicKey};
use aes_gcm::Aes256Gcm;
use tari_core::transactions::transaction::OutputFlags;
use tari_core::transactions::transaction_protocol::TxId;

/// This trait defines the required behaviour that a storage backend must provide for the Output Manager service.
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. If new data types are
/// required to be supported by the backends then these enums can be updated to reflect this requirement and the trait
/// will remain the same
pub trait OutputManagerBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError>;

    /// Fetch outputs that can be spent
    fn fetch_spendable_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;

    /// Fetch outputs with specific features
    fn fetch_with_features(&self, features: OutputFlags) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;

    fn fetch_by_features_asset_public_key(&self, public_key: PublicKey) -> Result<DbUnblindedOutput, OutputManagerStorageError>;

    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError>;
    /// This method is called when a pending transaction is to be confirmed. It must move the `outputs_to_be_spent` and
    /// `outputs_to_be_received` from a `PendingTransactionOutputs` record into the `unspent_outputs` and
    /// `spent_outputs` collections.
    fn confirm_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// This method encumbers the specified outputs into a `PendingTransactionOutputs` record. This is a short term
    /// encumberance in case the app is closed or crashes before transaction neogtiation is complete. These will be
    /// cleared on startup of the service.
    fn short_term_encumber_outputs(
        &self,
        tx_id: TxId,
        outputs_to_send: &[DbUnblindedOutput],
        outputs_to_receive: &[DbUnblindedOutput],
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
    /// This method must run through all the `PendingTransactionOutputs` and test if any have existed for longer that
    /// the specified duration. If they have they should be cancelled.
    fn timeout_pending_transactions(&self, period: Duration) -> Result<(), OutputManagerStorageError>;
    /// This method will increment the currently stored key index for the key manager config. Increment this after each
    /// key is generated
    fn increment_key_index(&self) -> Result<(), OutputManagerStorageError>;
    /// This method will set the currently stored key index for the key manager
    fn set_key_index(&self, index: u64) -> Result<(), OutputManagerStorageError>;
    /// If an unspent output is detected as invalid (i.e. not available on the blockchain) then it should be moved to
    /// the invalid outputs collection. The function will return the last recorded TxId associated with this output.
    fn invalidate_unspent_output(&self, output: &DbUnblindedOutput) -> Result<Option<TxId>, OutputManagerStorageError>;
    /// If an invalid output is found to be valid this function will turn it back into an unspent output
    fn revalidate_unspent_output(&self, spending_key: &Commitment) -> Result<(), OutputManagerStorageError>;
    /// Check to see if there exist any pending transaction with a blockheight equal that provided and cancel those
    /// pending transaction outputs.
    fn cancel_pending_transaction_at_block_height(&self, block_height: u64) -> Result<(), OutputManagerStorageError>;
    /// Apply encryption to the backend.
    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), OutputManagerStorageError>;
    /// Remove encryption from the backend.
    fn remove_encryption(&self) -> Result<(), OutputManagerStorageError>;
    /// Update a Spent output to be Unspent
    fn update_spent_output_to_unspent(
        &self,
        commitment: &Commitment,
    ) -> Result<DbUnblindedOutput, OutputManagerStorageError>;
    /// Update the mined height for all outputs for this tx_id
    fn update_mined_height(&self, tx_id: TxId, height: u64) -> Result<(), OutputManagerStorageError>;
}
