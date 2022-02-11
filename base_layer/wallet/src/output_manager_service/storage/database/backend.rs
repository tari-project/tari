use aes_gcm::Aes256Gcm;
use tari_common_types::{
    transaction::TxId,
    types::{Commitment, PublicKey},
};
use tari_core::transactions::transaction_components::{OutputFlags, TransactionOutput};

use crate::output_manager_service::{
    error::OutputManagerStorageError,
    service::{Balance, UTXOSelectionStrategy},
    storage::{
        database::{DbKey, DbValue, WriteOperation},
        models::DbUnblindedOutput,
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
    fn fetch_with_features(&self, features: OutputFlags) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;
    /// Fetch outputs with specific features for a given asset public key
    fn fetch_by_features_asset_public_key(
        &self,
        public_key: PublicKey,
    ) -> Result<DbUnblindedOutput, OutputManagerStorageError>;
    /// Retrieve outputs that have been mined but not spent yet (have not been deleted)
    fn fetch_mined_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;
    /// Retrieve outputs that have not been found or confirmed in the block chain yet
    fn fetch_unconfirmed_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError>;
    fn fetch_pending_incoming_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;

    fn set_received_output_mined_height(
        &self,
        hash: Vec<u8>,
        mined_height: u64,
        mined_in_block: Vec<u8>,
        mmr_position: u64,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError>;

    fn set_output_to_unmined(&self, hash: Vec<u8>) -> Result<(), OutputManagerStorageError>;
    fn set_outputs_to_be_revalidated(&self) -> Result<(), OutputManagerStorageError>;

    fn mark_output_as_spent(
        &self,
        hash: Vec<u8>,
        mark_deleted_at_height: u64,
        mark_deleted_in_block: Vec<u8>,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError>;

    fn mark_output_as_unspent(&self, hash: Vec<u8>) -> Result<(), OutputManagerStorageError>;
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
    /// This method will increment the currently stored key index for the key manager config. Increment this after each
    /// key is generated
    fn increment_key_index(&self) -> Result<(), OutputManagerStorageError>;
    /// This method will set the currently stored key index for the key manager
    fn set_key_index(&self, index: u64) -> Result<(), OutputManagerStorageError>;
    /// This method will update an output's metadata signature, akin to 'finalize output'
    fn update_output_metadata_signature(&self, output: &TransactionOutput) -> Result<(), OutputManagerStorageError>;
    /// If an invalid output is found to be valid this function will turn it back into an unspent output
    fn revalidate_unspent_output(&self, spending_key: &Commitment) -> Result<(), OutputManagerStorageError>;
    /// Apply encryption to the backend.
    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), OutputManagerStorageError>;
    /// Remove encryption from the backend.
    fn remove_encryption(&self) -> Result<(), OutputManagerStorageError>;

    /// Get the output that was most recently mined, ordered descending by mined height
    fn get_last_mined_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError>;
    /// Get the output that was most recently spent, ordered descending by mined height
    fn get_last_spent_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError>;
    /// Check if there is a pending coinbase transaction at this block height, if there is clear it.
    fn clear_pending_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
    ) -> Result<(), OutputManagerStorageError>;
    /// Set if a coinbase output is abandoned or not
    fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerStorageError>;
    /// Reinstate a cancelled inbound output
    fn reinstate_cancelled_inbound_output(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// Return the available, time locked, pending incoming and pending outgoing balance
    fn get_balance(&self, tip: Option<u64>) -> Result<Balance, OutputManagerStorageError>;
    /// Import unvalidated output
    fn add_unvalidated_output(&self, output: DbUnblindedOutput, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    fn fetch_unspent_outputs_for_spending(
        &self,
        strategy: UTXOSelectionStrategy,
        amount: u64,
        current_tip_height: Option<u64>,
    ) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;
    fn fetch_outputs_by_tx_id(&self, tx_id: TxId) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;
}
