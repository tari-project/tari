// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! One-off migration that re-checks the commitment mask of every stored output.
//!
//! Before the `verify_mask` result was honoured (it returns `Result<bool>` and callers only looked at the `Err`
//! case), an output whose commitment does not open to `value*H + mask*G` for its stored value and commitment mask
//! key could be accepted and stored as a normal wallet output. This migration finds those rows and marks them
//! [`OutputStatus::Invalid`] so they are no longer counted in the balance or selected for spending.
//!
//! It is gated by a flag in the wallet's client key/value store (see [`MigrationFlagStore`]). The flag is only
//! written after the scan and the update have both completed, so a crash part way through simply re-runs the whole
//! scan on the next start.
//!
//! The output manager service runs it from `OutputManagerService::start`, before its request loop, so it completes
//! before any TXO validation task can be started and nothing can overwrite its result.
//!
//! Transactions coupled to an output whose value is definitely wrong (mask mismatch) are reconciled so the wallet
//! looks as a fresh recovery would leave it: recovery rejects such outputs, so it never creates a transaction for
//! them. The coupled transaction (the output's `received_in_tx_id`) is cancelled with
//! [`TxCancellationReason::InvalidEncryptedValue`] through an [`InvalidOutputTransactionSink`], keeping it in the
//! history as an audit trail.
//!
//! The outputs and the transactions are tracked separately so that a transaction that cannot be cancelled never keeps
//! a wrong-value output spendable. Order of operations:
//! 1. scan;
//! 2. persist the coupled transaction ids under [`INVALID_MASK_PENDING_TXS_KEY`];
//! 3. cancel them, then rewrite that key with only the ids that failed with a retryable error (removed when empty);
//! 4. mark every mismatch output Invalid, whatever happened to its transaction;
//! 5. write [`INVALID_MASK_MIGRATION_KEY`].
//!
//! A crash before step 5 re-runs the scan (cancelling is idempotent, and the outputs are still found because they are
//! marked in step 4). Once the main flag is set, later starts skip the scan and only retry the ids still pending. No
//! events are published: this runs at startup before anything subscribes, and the UI / FFI read transaction state
//! from the database on launch.

use std::{collections::BTreeSet, str::FromStr};

use log::*;
use tari_common_types::{
    transaction::{TransactionDirection, TxId},
    types::CompressedCommitment,
};
use tari_transaction_components::key_manager::TariKeyId;
use tari_transaction_key_manager::legacy_key_manager::{LegacyTariKeyId, LegacyTransactionKeyManagerInterface};
use tari_utilities::{ByteArray, hex::to_hex};
use thiserror::Error;

use crate::{
    error::WalletStorageError,
    output_manager_service::{
        commitment_mask::{MaskCheck, check_commitment_mask, error_kind},
        error::OutputManagerStorageError,
        storage::{
            OutputStatus,
            database::{OutputManagerBackend, OutputManagerDatabase, OutputMaskVerificationRow},
        },
    },
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        error::TransactionStorageError,
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::{TxCancellationReason, WalletTransaction},
        },
    },
};

const LOG_TARGET: &str = "wallet::output_manager_service::invalid_mask_migration";

/// Client key/value store key recording that the migration has completed. Its value is the number of outputs that
/// were marked invalid.
pub const INVALID_MASK_MIGRATION_KEY: &str = "migration.invalid_commitment_mask.v1";

/// Client key/value store key holding the ids of coupled transactions that still need cancelling (comma separated).
/// Removed when empty.
pub const INVALID_MASK_PENDING_TXS_KEY: &str = "migration.invalid_commitment_mask.v1.pending_txs";

/// Number of output rows fetched per page.
const BATCH_SIZE: i64 = 1000;

#[derive(Debug, Error)]
pub enum InvalidMaskMigrationError {
    #[error("Wallet storage error: {0}")]
    WalletStorage(#[from] WalletStorageError),
    #[error("Output manager storage error: {0}")]
    OutputManagerStorage(#[from] OutputManagerStorageError),
}

/// Counts from a completed scan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InvalidMaskMigrationSummary {
    /// Outputs whose status was not already `Invalid` and so were checked
    pub scanned: usize,
    /// Outputs whose commitment did not open to the stored value and mask (`verify_mask` returned `Ok(false)`)
    pub mask_mismatches: usize,
    /// Outputs that could not be checked at all (bad key id, bad commitment bytes, key derivation failure, ...)
    pub verification_errors: usize,
    /// Of `verification_errors`, outputs left unchanged because they are spent or not stored (see
    /// [`unverifiable_may_be_invalidated`])
    pub unverifiable_left_unchanged: usize,
    /// Outputs whose status was changed to `Invalid`
    pub marked_invalid: usize,
    /// Transactions coupled to a mask-mismatch output that were cancelled by this run
    pub transactions_cancelled: usize,
    /// Coupled transactions that were not cancelled and will not be retried: not found, or a non-retryable error (the
    /// transaction row itself cannot be read)
    pub transactions_not_reconciled: usize,
    /// Transaction ids that were due for cancelling but must not be cancelled, and are dropped: either no output
    /// received in them definitely fails the mask check (e.g. an id injected into [`INVALID_MASK_PENDING_TXS_KEY`],
    /// which lives in the app-writable client key/value store; never passed to the sink), or the transaction is not
    /// inbound (a change output carries its outbound payment's id; the sink refuses to cancel it)
    pub transactions_rejected_unverified: usize,
    /// Coupled transactions the sink failed on with a retryable error (I/O, connection, ...). They stay in
    /// [`INVALID_MASK_PENDING_TXS_KEY`] and are retried on the next start
    pub reconciliation_errors: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidMaskMigrationOutcome {
    /// The completion flag was already set; nothing was scanned
    AlreadyCompleted,
    /// The scan ran to completion and the flag has now been set
    Completed(InvalidMaskMigrationSummary),
    /// The outputs are done and the flag is set, but some coupled transactions failed with a retryable error
    /// (`reconciliation_errors > 0`); they stay pending and are retried, without a rescan, on the next start
    Incomplete(InvalidMaskMigrationSummary),
}

/// Persistent store for the one-off completion flag. Implemented by [`WalletDatabase`], which keeps it in the wallet's
/// (encrypted) client key/value table.
pub trait MigrationFlagStore: Send + Sync {
    fn get_migration_flag(&self, key: &str) -> Result<Option<String>, WalletStorageError>;
    fn set_migration_flag(&self, key: &str, value: String) -> Result<(), WalletStorageError>;
    fn clear_migration_flag(&self, key: &str) -> Result<(), WalletStorageError>;
}

impl<T: WalletBackend + 'static> MigrationFlagStore for WalletDatabase<T> {
    fn get_migration_flag(&self, key: &str) -> Result<Option<String>, WalletStorageError> {
        self.get_client_key_value(key.to_string())
    }

    fn set_migration_flag(&self, key: &str, value: String) -> Result<(), WalletStorageError> {
        self.set_client_key_value(key.to_string(), value)
    }

    fn clear_migration_flag(&self, key: &str) -> Result<(), WalletStorageError> {
        self.clear_client_value(key.to_string()).map(|_| ())
    }
}

/// What [`InvalidOutputTransactionSink::cancel_for_invalid_encrypted_value`] did with a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidOutputTxOutcome {
    /// A completed transaction was rejected with [`TxCancellationReason::InvalidEncryptedValue`]
    CancelledCompleted,
    /// A pending inbound transaction was cancelled (pending transactions do not store a cancellation reason)
    CancelledPendingInbound,
    /// The transaction was already cancelled; left as it is
    AlreadyCancelled,
    /// No transaction with this id exists
    NotFound,
    /// The transaction is not inbound (e.g. the output is change of an outbound payment); it is left untouched,
    /// because cancelling it would reject a real payment
    NotInbound(&'static str),
}

/// Cancels the transaction coupled to an output whose value is definitely wrong. Implemented by
/// [`TransactionDatabase`]; injected into the output manager service so it does not depend on the transaction service.
/// Must be idempotent: the migration may run again after a crash.
pub trait InvalidOutputTransactionSink: Send + Sync {
    fn cancel_for_invalid_encrypted_value(&self, tx_id: TxId) -> Result<InvalidOutputTxOutcome, InvalidOutputTxError>;
}

/// A failure to cancel a coupled transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidOutputTxError {
    /// `true` if trying again later may succeed (I/O, connection, ...); `false` if the transaction row itself cannot
    /// be read or updated, so retrying would fail forever
    pub retryable: bool,
    pub message: String,
}

impl InvalidOutputTxError {
    /// Classify a transaction storage error: only database access / connection failures are worth retrying
    pub fn from_storage_error(error: &TransactionStorageError) -> Self {
        let retryable = match error {
            TransactionStorageError::DieselError(diesel::result::Error::NotFound) => false,
            TransactionStorageError::DieselError(_) |
            TransactionStorageError::DieselConnectionError(_) |
            TransactionStorageError::SqliteStorageError(_) |
            TransactionStorageError::BlockingTaskSpawnError(_) => true,
            _ => false,
        };
        Self {
            retryable,
            message: error.to_string(),
        }
    }
}

impl<T: TransactionBackend + 'static> InvalidOutputTransactionSink for TransactionDatabase<T> {
    fn cancel_for_invalid_encrypted_value(&self, tx_id: TxId) -> Result<InvalidOutputTxOutcome, InvalidOutputTxError> {
        let error = |e: TransactionStorageError| InvalidOutputTxError::from_storage_error(&e);
        match self.get_any_transaction(tx_id).map_err(error)? {
            None => Ok(InvalidOutputTxOutcome::NotFound),
            Some(WalletTransaction::Completed(tx)) => {
                // Checked here, not by the caller, so an injected id cannot get an outbound payment rejected either
                if tx.direction != TransactionDirection::Inbound {
                    return Ok(InvalidOutputTxOutcome::NotInbound("completed non-inbound transaction"));
                }
                if tx.cancelled.is_some() {
                    return Ok(InvalidOutputTxOutcome::AlreadyCancelled);
                }
                self.reject_completed_transaction(
                    tx_id,
                    TxCancellationReason::InvalidEncryptedValue,
                    Some("An output of this transaction does not open to its encrypted value".to_string()),
                )
                .map_err(error)?;
                Ok(InvalidOutputTxOutcome::CancelledCompleted)
            },
            Some(WalletTransaction::PendingInbound(tx)) => {
                if tx.cancelled {
                    return Ok(InvalidOutputTxOutcome::AlreadyCancelled);
                }
                self.cancel_pending_transaction(tx_id).map_err(error)?;
                Ok(InvalidOutputTxOutcome::CancelledPendingInbound)
            },
            Some(WalletTransaction::PendingOutbound(_)) => {
                Ok(InvalidOutputTxOutcome::NotInbound("pending outbound transaction"))
            },
        }
    }
}

/// Run the migration unless it has already completed for this wallet.
///
/// Every output whose status is not already `Invalid` is checked. An output is marked `Invalid` if `verify_mask`
/// returns `Ok(false)`, or if the check cannot be performed and the output is spendable or pending (see
/// [`unverifiable_may_be_invalidated`]); unverifiable spent / not-stored outputs are left unchanged. The cases are
/// logged and counted separately; stored values are only logged at debug. The status update for all failures happens in
/// one database transaction after the scan, and the completion flag is written only after that transaction commits.
///
/// Before any output is marked, the transaction each mask-mismatch output was received in is recorded as pending and
/// cancelled through `tx_sink` (see the module docs). Unverifiable outputs never have their transaction touched: they
/// may verify once the key manager error clears. Every mismatch output is marked Invalid whatever happens to its
/// transaction. A transaction that fails with a retryable error stays pending, the flag is still written and
/// [`InvalidMaskMigrationOutcome::Incomplete`] is returned; the next start skips the scan and retries only the pending
/// transactions. Missing, non-inbound and non-retryable ones are logged, counted and dropped; only inbound
/// transactions are ever cancelled.
///
/// This is CPU-bound and synchronous; call it from a blocking context.
pub fn run_invalid_mask_migration<F, B, KM>(
    flag_store: &F,
    tx_sink: Option<&dyn InvalidOutputTransactionSink>,
    output_db: &OutputManagerDatabase<B>,
    key_manager: &KM,
) -> Result<InvalidMaskMigrationOutcome, InvalidMaskMigrationError>
where
    F: MigrationFlagStore + ?Sized,
    B: OutputManagerBackend + 'static,
    KM: LegacyTransactionKeyManagerInterface,
{
    if flag_store.get_migration_flag(INVALID_MASK_MIGRATION_KEY)?.is_some() {
        let pending = load_pending_tx_ids(flag_store)?;
        if pending.is_empty() {
            debug!(target: LOG_TARGET, "Commitment mask migration already completed, skipping");
            return Ok(InvalidMaskMigrationOutcome::AlreadyCompleted);
        }
        info!(
            target: LOG_TARGET,
            "Commitment mask migration: retrying {} pending transaction cancellation(s)",
            pending.len()
        );
        let mut summary = InvalidMaskMigrationSummary::default();
        let remaining = reconcile_transactions(tx_sink, output_db, key_manager, &pending, &mut summary);
        store_pending_tx_ids(flag_store, &remaining)?;
        return Ok(outcome(summary));
    }

    info!(target: LOG_TARGET, "Commitment mask migration: checking all stored outputs");
    let mut summary = InvalidMaskMigrationSummary::default();
    let mut to_invalidate = Vec::new();
    // Transactions the mask-mismatch outputs were received in
    let mut mismatched_tx_ids = BTreeSet::new();
    let mut last_id = 0;
    loop {
        let batch = output_db.fetch_outputs_for_mask_verification(last_id, BATCH_SIZE)?;
        let Some(last) = batch.last() else {
            break;
        };
        last_id = last.id;

        for row in &batch {
            summary.scanned = summary.scanned.saturating_add(1);
            match verify_row(row, key_manager) {
                MaskCheck::Valid => {},
                MaskCheck::Mismatch => {
                    summary.mask_mismatches = summary.mask_mismatches.saturating_add(1);
                    warn!(
                        target: LOG_TARGET,
                        "Commitment mask migration: output id={} commitment {} (status {}) does not open to its \
                         stored value and mask; marking Invalid",
                        row.id,
                        to_hex(&row.commitment),
                        status_name(row.status),
                    );
                    debug!(target: LOG_TARGET, "Commitment mask migration: output id={} stored value {}", row.id, row.value);
                    to_invalidate.push(row.id);
                    if let Some(tx_id) = row.received_in_tx_id {
                        mismatched_tx_ids.insert(tx_id.as_u64());
                    }
                    if let Some(tx_id) = row.spent_in_tx_id {
                        // A spend of this output cannot have had a valid kernel, so the base node will have rejected
                        // it; the spending transaction is left to the transaction service
                        debug!(
                            target: LOG_TARGET,
                            "Commitment mask migration: output id={} is also recorded as spent in transaction {tx_id}; \
                             not reconciled",
                            row.id
                        );
                    }
                },
                MaskCheck::Unverifiable(reason) => {
                    summary.verification_errors = summary.verification_errors.saturating_add(1);
                    if unverifiable_may_be_invalidated(row.status) {
                        error!(
                            target: LOG_TARGET,
                            "Commitment mask migration: could not verify output id={} commitment {} (status {}): \
                             {reason}; marking Invalid",
                            row.id,
                            to_hex(&row.commitment),
                            status_name(row.status),
                        );
                        to_invalidate.push(row.id);
                    } else {
                        // A spent or not-stored row cannot be spent again, and an undecodable row in Invalid would be
                        // fetched by every TXO revalidation run. Leave it where it is.
                        summary.unverifiable_left_unchanged = summary.unverifiable_left_unchanged.saturating_add(1);
                        error!(
                            target: LOG_TARGET,
                            "Commitment mask migration: could not verify output id={} commitment {} (status {}): \
                             {reason}; not spendable, leaving unchanged",
                            row.id,
                            to_hex(&row.commitment),
                            status_name(row.status),
                        );
                    }
                    debug!(target: LOG_TARGET, "Commitment mask migration: output id={} stored value {}", row.id, row.value);
                },
            }
        }
    }

    apply_scan_results(
        flag_store,
        tx_sink,
        output_db,
        key_manager,
        to_invalidate,
        &mismatched_tx_ids,
        summary,
    )
}

/// Second half of the migration, in order: persist the coupled transaction ids, cancel them (keeping only retryable
/// failures pending), mark every mismatch output Invalid, write the flag.
fn apply_scan_results<F, B, KM>(
    flag_store: &F,
    tx_sink: Option<&dyn InvalidOutputTransactionSink>,
    output_db: &OutputManagerDatabase<B>,
    key_manager: &KM,
    to_invalidate: Vec<i32>,
    mismatched_tx_ids: &BTreeSet<u64>,
    mut summary: InvalidMaskMigrationSummary,
) -> Result<InvalidMaskMigrationOutcome, InvalidMaskMigrationError>
where
    F: MigrationFlagStore + ?Sized,
    B: OutputManagerBackend + 'static,
    KM: LegacyTransactionKeyManagerInterface,
{
    // Ids left pending by an earlier interrupted run are carried along
    let mut pending = load_pending_tx_ids(flag_store)?;
    pending.extend(mismatched_tx_ids.iter().copied());
    store_pending_tx_ids(flag_store, &pending)?;
    let remaining = reconcile_transactions(tx_sink, output_db, key_manager, &pending, &mut summary);
    store_pending_tx_ids(flag_store, &remaining)?;

    summary.marked_invalid = output_db.mark_outputs_invalid(to_invalidate)?;
    flag_store.set_migration_flag(INVALID_MASK_MIGRATION_KEY, summary.marked_invalid.to_string())?;

    info!(
        target: LOG_TARGET,
        "Commitment mask migration complete: {} output(s) scanned, {} mask mismatch(es), {} verification error(s) ({} \
         left unchanged as not spendable), {} marked Invalid, {} transaction(s) cancelled, {} not reconciled, {} \
         dropped as unverified, {} pending retry",
        summary.scanned,
        summary.mask_mismatches,
        summary.verification_errors,
        summary.unverifiable_left_unchanged,
        summary.marked_invalid,
        summary.transactions_cancelled,
        summary.transactions_not_reconciled,
        summary.transactions_rejected_unverified,
        summary.reconciliation_errors
    );
    Ok(outcome(summary))
}

fn outcome(summary: InvalidMaskMigrationSummary) -> InvalidMaskMigrationOutcome {
    if summary.reconciliation_errors > 0 {
        InvalidMaskMigrationOutcome::Incomplete(summary)
    } else {
        InvalidMaskMigrationOutcome::Completed(summary)
    }
}

/// Read the pending transaction ids; unparseable entries are logged and dropped.
fn load_pending_tx_ids<F: MigrationFlagStore + ?Sized>(flag_store: &F) -> Result<BTreeSet<u64>, WalletStorageError> {
    let Some(value) = flag_store.get_migration_flag(INVALID_MASK_PENDING_TXS_KEY)? else {
        return Ok(BTreeSet::new());
    };
    Ok(value
        .split(',')
        .filter(|entry| !entry.trim().is_empty())
        .filter_map(|entry| {
            entry
                .trim()
                .parse::<u64>()
                .inspect_err(|_| {
                    error!(target: LOG_TARGET, "Commitment mask migration: dropping malformed pending tx id '{entry}'");
                })
                .ok()
        })
        .collect())
}

/// Write the pending transaction ids, removing the key when there are none.
fn store_pending_tx_ids<F: MigrationFlagStore + ?Sized>(
    flag_store: &F,
    tx_ids: &BTreeSet<u64>,
) -> Result<(), WalletStorageError> {
    if tx_ids.is_empty() {
        return flag_store.clear_migration_flag(INVALID_MASK_PENDING_TXS_KEY);
    }
    let value = tx_ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
    flag_store.set_migration_flag(INVALID_MASK_PENDING_TXS_KEY, value)
}

/// Cancel the given coupled transactions. Returns those that failed with a retryable error (or all of them if no sink
/// is configured), which must stay pending; everything else is done or deliberately dropped.
///
/// Each id is first re-verified against the outputs table (see [`is_coupled_to_mismatch`]) because the pending list
/// is stored in the app-writable client key/value store; an id that does not verify is dropped and never reaches the
/// sink.
fn reconcile_transactions<B, KM>(
    tx_sink: Option<&dyn InvalidOutputTransactionSink>,
    output_db: &OutputManagerDatabase<B>,
    key_manager: &KM,
    tx_ids: &BTreeSet<u64>,
    summary: &mut InvalidMaskMigrationSummary,
) -> BTreeSet<u64>
where
    B: OutputManagerBackend + 'static,
    KM: LegacyTransactionKeyManagerInterface,
{
    let mut retry = BTreeSet::new();
    if tx_ids.is_empty() {
        return retry;
    }
    let Some(sink) = tx_sink else {
        warn!(
            target: LOG_TARGET,
            "Commitment mask migration: no transaction sink configured, {} coupled transaction(s) left pending",
            tx_ids.len()
        );
        summary.reconciliation_errors = summary.reconciliation_errors.saturating_add(tx_ids.len());
        return tx_ids.clone();
    };
    for tx_id in tx_ids.iter().copied().map(TxId::from) {
        match is_coupled_to_mismatch(output_db, key_manager, tx_id) {
            Ok(true) => {},
            Ok(false) => {
                summary.transactions_rejected_unverified = summary.transactions_rejected_unverified.saturating_add(1);
                warn!(
                    target: LOG_TARGET,
                    "Commitment mask migration: transaction {tx_id} has no received output that fails the mask \
                     check; dropping it without cancelling"
                );
                continue;
            },
            Err(e) => {
                // Could not read the outputs; keep it pending rather than cancel unverified
                summary.reconciliation_errors = summary.reconciliation_errors.saturating_add(1);
                retry.insert(tx_id.as_u64());
                error!(
                    target: LOG_TARGET,
                    "Commitment mask migration: could not verify transaction {tx_id}, will retry on the next start: {e}"
                );
                continue;
            },
        }
        match sink.cancel_for_invalid_encrypted_value(tx_id) {
            Ok(InvalidOutputTxOutcome::CancelledCompleted | InvalidOutputTxOutcome::CancelledPendingInbound) => {
                summary.transactions_cancelled = summary.transactions_cancelled.saturating_add(1);
                info!(
                    target: LOG_TARGET,
                    "Commitment mask migration: cancelled transaction {tx_id} (invalid encrypted value)"
                );
            },
            Ok(InvalidOutputTxOutcome::AlreadyCancelled) => {
                debug!(target: LOG_TARGET, "Commitment mask migration: transaction {tx_id} already cancelled");
            },
            Ok(InvalidOutputTxOutcome::NotFound) => {
                summary.transactions_not_reconciled = summary.transactions_not_reconciled.saturating_add(1);
                warn!(target: LOG_TARGET, "Commitment mask migration: coupled transaction {tx_id} not found");
            },
            Ok(InvalidOutputTxOutcome::NotInbound(kind)) => {
                summary.transactions_rejected_unverified = summary.transactions_rejected_unverified.saturating_add(1);
                warn!(
                    target: LOG_TARGET,
                    "Commitment mask migration: coupled transaction {tx_id} is a {kind}; only inbound transactions are \
                     cancelled, leaving it untouched"
                );
            },
            Err(e) if e.retryable => {
                summary.reconciliation_errors = summary.reconciliation_errors.saturating_add(1);
                retry.insert(tx_id.as_u64());
                error!(
                    target: LOG_TARGET,
                    "Commitment mask migration: could not cancel coupled transaction {tx_id}, will retry on the next \
                     start: {}",
                    e.message
                );
            },
            Err(e) => {
                summary.transactions_not_reconciled = summary.transactions_not_reconciled.saturating_add(1);
                error!(
                    target: LOG_TARGET,
                    "Commitment mask migration: cannot cancel coupled transaction {tx_id}, not retrying: {}",
                    e.message
                );
            },
        }
    }
    retry
}

/// `true` if at least one output received in `tx_id` (in any status) definitely fails the commitment mask check
/// (`MaskCheck::Mismatch`). An Invalid status alone, or an output that cannot be checked, is not enough.
fn is_coupled_to_mismatch<B, KM>(
    output_db: &OutputManagerDatabase<B>,
    key_manager: &KM,
    tx_id: TxId,
) -> Result<bool, OutputManagerStorageError>
where
    B: OutputManagerBackend + 'static,
    KM: LegacyTransactionKeyManagerInterface,
{
    Ok(output_db
        .fetch_outputs_for_mask_verification_by_received_tx(tx_id)?
        .iter()
        .any(|row| verify_row(row, key_manager) == MaskCheck::Mismatch))
}

/// Parse the raw row and run the shared commitment mask check on it. Parse failures are `Unverifiable`.
fn verify_row<KM: LegacyTransactionKeyManagerInterface>(
    row: &OutputMaskVerificationRow,
    key_manager: &KM,
) -> MaskCheck {
    let key_id = match TariKeyId::from_str(&row.spending_key) {
        Ok(key_id) => key_id,
        Err(_) => {
            let legacy = match LegacyTariKeyId::from_str(&row.spending_key) {
                Ok(legacy) => legacy,
                Err(_) => return MaskCheck::Unverifiable("unrecognised commitment mask key id format".to_string()),
            };
            match key_manager.convert_legacy_tari_key_id_to_current(&legacy) {
                Ok(key_id) => key_id,
                Err(e) => {
                    return MaskCheck::Unverifiable(format!(
                        "could not convert legacy commitment mask key id ({})",
                        error_kind(&e)
                    ));
                },
            }
        },
    };
    let commitment = match CompressedCommitment::from_vec(&row.commitment) {
        Ok(commitment) => commitment,
        Err(e) => return MaskCheck::Unverifiable(format!("malformed commitment ({})", error_kind(&e))),
    };
    let Ok(value) = u64::try_from(row.value) else {
        return MaskCheck::Unverifiable("negative stored value".to_string());
    };
    check_commitment_mask(key_manager, &commitment, &key_id, value)
}

/// Whether an output whose mask cannot be checked is marked Invalid: only if it is spendable or pending. Spent,
/// SpentMinedUnconfirmed, NotStored (and unknown) rows are left alone.
pub fn unverifiable_may_be_invalidated(status: i32) -> bool {
    matches!(
        OutputStatus::try_from(status),
        Ok(OutputStatus::Unspent |
            OutputStatus::UnspentMinedUnconfirmed |
            OutputStatus::EncumberedToBeReceived |
            OutputStatus::ShortTermEncumberedToBeReceived |
            OutputStatus::EncumberedToBeSpent |
            OutputStatus::ShortTermEncumberedToBeSpent |
            OutputStatus::CancelledInbound)
    )
}

fn status_name(status: i32) -> String {
    OutputStatus::try_from(status).map_or_else(|_| format!("unknown({status})"), |s| s.to_string())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::transaction_service::storage::database::DbKey;

    #[test]
    fn only_database_access_errors_are_retryable() {
        let retryable = |e: TransactionStorageError| InvalidOutputTxError::from_storage_error(&e).retryable;
        assert!(retryable(TransactionStorageError::DieselConnectionError(
            diesel::ConnectionError::BadConnection("gone".to_string())
        )));
        assert!(retryable(TransactionStorageError::BlockingTaskSpawnError(
            "busy".to_string()
        )));
        assert!(!retryable(TransactionStorageError::DieselError(
            diesel::result::Error::NotFound
        )));
        assert!(!retryable(TransactionStorageError::ValueNotFound(
            DbKey::CompletedTransaction(1u64.into())
        )));
        assert!(!retryable(TransactionStorageError::AeadError("bad tag".to_string())));
        assert!(!retryable(TransactionStorageError::ByteArrayError(
            "bad bytes".to_string()
        )));
    }
}
