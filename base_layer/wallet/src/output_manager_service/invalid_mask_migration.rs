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
//! history as an audit trail. Order of operations: scan, cancel coupled transactions, mark outputs Invalid, write the
//! flag. A crash anywhere before the flag re-runs the whole migration; cancelling is idempotent, and the outputs are
//! still found by the re-run because they are only marked Invalid after the cancellations. If the sink fails on a
//! transaction, that transaction's outputs are left unmarked and the flag is not written, so the next start retries
//! them. No events are published:
//! this runs at startup before anything subscribes, and the UI / FFI read transaction state from the database on
//! launch.

use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use log::*;
use tari_common_types::{transaction::TxId, types::CompressedCommitment};
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
    transaction_service::storage::{
        database::{TransactionBackend, TransactionDatabase},
        models::{TxCancellationReason, WalletTransaction},
    },
};

const LOG_TARGET: &str = "wallet::output_manager_service::invalid_mask_migration";

/// Client key/value store key recording that the migration has completed. Its value is the number of outputs that
/// were marked invalid.
pub const INVALID_MASK_MIGRATION_KEY: &str = "migration.invalid_commitment_mask.v1";

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
    /// Coupled transactions that were not cancelled but do not hold the migration up (not found, or not a
    /// cancellable kind); their outputs are still marked Invalid
    pub transactions_not_reconciled: usize,
    /// Coupled transactions the sink failed on (storage, decryption, ...). Their outputs are left as they are and the
    /// flag is not written, so the next start retries them
    pub reconciliation_errors: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidMaskMigrationOutcome {
    /// The completion flag was already set; nothing was scanned
    AlreadyCompleted,
    /// The scan ran to completion and the flag has now been set
    Completed(InvalidMaskMigrationSummary),
    /// Some coupled transactions could not be reconciled (`reconciliation_errors > 0`). Everything else was applied,
    /// but the outputs of those transactions were left unmarked and the flag was not written, so the next start
    /// rescans and retries them
    Incomplete(InvalidMaskMigrationSummary),
}

/// Persistent store for the one-off completion flag. Implemented by [`WalletDatabase`], which keeps it in the wallet's
/// (encrypted) client key/value table.
pub trait MigrationFlagStore: Send + Sync {
    fn get_migration_flag(&self, key: &str) -> Result<Option<String>, WalletStorageError>;
    fn set_migration_flag(&self, key: &str, value: String) -> Result<(), WalletStorageError>;
}

impl<T: WalletBackend + 'static> MigrationFlagStore for WalletDatabase<T> {
    fn get_migration_flag(&self, key: &str) -> Result<Option<String>, WalletStorageError> {
        self.get_client_key_value(key.to_string())
    }

    fn set_migration_flag(&self, key: &str, value: String) -> Result<(), WalletStorageError> {
        self.set_client_key_value(key.to_string(), value)
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
    /// The transaction is of a kind this reconciliation does not cancel
    NotReconciled(&'static str),
}

/// Cancels the transaction coupled to an output whose value is definitely wrong. Implemented by
/// [`TransactionDatabase`]; injected into the output manager service so it does not depend on the transaction service.
/// Must be idempotent: the migration may run again after a crash.
pub trait InvalidOutputTransactionSink: Send + Sync {
    fn cancel_for_invalid_encrypted_value(&self, tx_id: TxId) -> Result<InvalidOutputTxOutcome, String>;
}

impl<T: TransactionBackend + 'static> InvalidOutputTransactionSink for TransactionDatabase<T> {
    fn cancel_for_invalid_encrypted_value(&self, tx_id: TxId) -> Result<InvalidOutputTxOutcome, String> {
        match self.get_any_transaction(tx_id).map_err(|e| e.to_string())? {
            None => Ok(InvalidOutputTxOutcome::NotFound),
            Some(WalletTransaction::Completed(tx)) => {
                if tx.cancelled.is_some() {
                    return Ok(InvalidOutputTxOutcome::AlreadyCancelled);
                }
                self.reject_completed_transaction(
                    tx_id,
                    TxCancellationReason::InvalidEncryptedValue,
                    Some("An output of this transaction does not open to its encrypted value".to_string()),
                )
                .map_err(|e| e.to_string())?;
                Ok(InvalidOutputTxOutcome::CancelledCompleted)
            },
            Some(WalletTransaction::PendingInbound(tx)) => {
                if tx.cancelled {
                    return Ok(InvalidOutputTxOutcome::AlreadyCancelled);
                }
                self.cancel_pending_transaction(tx_id).map_err(|e| e.to_string())?;
                Ok(InvalidOutputTxOutcome::CancelledPendingInbound)
            },
            Some(WalletTransaction::PendingOutbound(_)) => {
                Ok(InvalidOutputTxOutcome::NotReconciled("pending outbound transaction"))
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
/// Before any output is marked, the transaction each mask-mismatch output was received in is cancelled through
/// `tx_sink` (see the module docs). Unverifiable outputs never have their transaction touched: they may verify once
/// the key manager error clears. A missing or non-cancellable (pending outbound) transaction is logged and counted and
/// its outputs are still marked Invalid. If the sink fails on a transaction (storage, decryption, ...), that
/// transaction's outputs are left unmarked, the flag is not written and [`InvalidMaskMigrationOutcome::Incomplete`] is
/// returned, so the next start rescans the still non-Invalid rows and retries just those.
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
        debug!(target: LOG_TARGET, "Commitment mask migration already completed, skipping");
        return Ok(InvalidMaskMigrationOutcome::AlreadyCompleted);
    }

    info!(target: LOG_TARGET, "Commitment mask migration: checking all stored outputs");
    let mut summary = InvalidMaskMigrationSummary::default();
    let mut to_invalidate = Vec::new();
    // Mask-mismatch outputs grouped by the transaction they were received in; they are only marked Invalid once that
    // transaction has been reconciled
    let mut mismatched_by_tx: BTreeMap<u64, Vec<i32>> = BTreeMap::new();
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
                    match row.received_in_tx_id {
                        Some(tx_id) => mismatched_by_tx.entry(tx_id.as_u64()).or_default().push(row.id),
                        None => to_invalidate.push(row.id),
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

    apply_scan_results(flag_store, tx_sink, output_db, to_invalidate, mismatched_by_tx, summary)
}

/// Second half of the migration, in order: reconcile coupled transactions, mark outputs Invalid, write the flag
/// (only if every reconciliation succeeded).
fn apply_scan_results<F, B>(
    flag_store: &F,
    tx_sink: Option<&dyn InvalidOutputTransactionSink>,
    output_db: &OutputManagerDatabase<B>,
    mut to_invalidate: Vec<i32>,
    mismatched_by_tx: BTreeMap<u64, Vec<i32>>,
    mut summary: InvalidMaskMigrationSummary,
) -> Result<InvalidMaskMigrationOutcome, InvalidMaskMigrationError>
where
    F: MigrationFlagStore + ?Sized,
    B: OutputManagerBackend + 'static,
{
    let tx_ids: BTreeSet<u64> = mismatched_by_tx.keys().copied().collect();
    let failed = reconcile_transactions(tx_sink, &tx_ids, &mut summary);
    for (tx_id, output_ids) in mismatched_by_tx {
        if !failed.contains(&tx_id) {
            to_invalidate.extend(output_ids);
        }
    }
    summary.marked_invalid = output_db.mark_outputs_invalid(to_invalidate)?;
    if summary.reconciliation_errors > 0 {
        warn!(
            target: LOG_TARGET,
            "Commitment mask migration incomplete: {} coupled transaction(s) could not be reconciled; their outputs are \
             left unchanged and will be retried on the next start ({} output(s) marked Invalid this run)",
            summary.reconciliation_errors,
            summary.marked_invalid
        );
        return Ok(InvalidMaskMigrationOutcome::Incomplete(summary));
    }
    flag_store.set_migration_flag(INVALID_MASK_MIGRATION_KEY, summary.marked_invalid.to_string())?;

    info!(
        target: LOG_TARGET,
        "Commitment mask migration complete: {} output(s) scanned, {} mask mismatch(es), {} verification error(s) ({} \
         left unchanged as not spendable), {} marked Invalid, {} transaction(s) cancelled, {} not reconciled",
        summary.scanned,
        summary.mask_mismatches,
        summary.verification_errors,
        summary.unverifiable_left_unchanged,
        summary.marked_invalid,
        summary.transactions_cancelled,
        summary.transactions_not_reconciled
    );
    Ok(InvalidMaskMigrationOutcome::Completed(summary))
}

/// Cancel the transactions coupled to mask-mismatch outputs. Returns the transactions the sink failed on, whose
/// outputs must not be marked Invalid this run; everything else (including a missing or non-cancellable transaction)
/// lets its outputs be marked.
fn reconcile_transactions(
    tx_sink: Option<&dyn InvalidOutputTransactionSink>,
    tx_ids: &BTreeSet<u64>,
    summary: &mut InvalidMaskMigrationSummary,
) -> BTreeSet<u64> {
    let mut failed = BTreeSet::new();
    if tx_ids.is_empty() {
        return failed;
    }
    let Some(sink) = tx_sink else {
        warn!(
            target: LOG_TARGET,
            "Commitment mask migration: no transaction sink configured, {} coupled transaction(s) not reconciled",
            tx_ids.len()
        );
        summary.transactions_not_reconciled = summary.transactions_not_reconciled.saturating_add(tx_ids.len());
        return failed;
    };
    for tx_id in tx_ids.iter().copied().map(TxId::from) {
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
            Ok(InvalidOutputTxOutcome::NotReconciled(kind)) => {
                summary.transactions_not_reconciled = summary.transactions_not_reconciled.saturating_add(1);
                warn!(
                    target: LOG_TARGET,
                    "Commitment mask migration: coupled transaction {tx_id} is a {kind}, not cancelled"
                );
            },
            Err(e) => {
                summary.reconciliation_errors = summary.reconciliation_errors.saturating_add(1);
                failed.insert(tx_id.as_u64());
                error!(
                    target: LOG_TARGET,
                    "Commitment mask migration: could not cancel coupled transaction {tx_id}, its outputs are left for \
                     the next start: {e}"
                );
            },
        }
    }
    failed
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
