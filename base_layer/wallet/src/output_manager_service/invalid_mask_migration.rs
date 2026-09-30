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

use std::str::FromStr;

use log::*;
use tari_common_types::types::CompressedCommitment;
use tari_transaction_components::key_manager::TariKeyId;
use tari_transaction_key_manager::legacy_key_manager::{LegacyTariKeyId, LegacyTransactionKeyManagerInterface};
use tari_utilities::{ByteArray, hex::to_hex};
use thiserror::Error;

use crate::{
    error::WalletStorageError,
    output_manager_service::{
        commitment_mask::{MaskCheck, check_commitment_mask},
        error::OutputManagerStorageError,
        storage::{
            OutputStatus,
            database::{OutputManagerBackend, OutputManagerDatabase, OutputMaskVerificationRow},
        },
    },
    storage::database::{WalletBackend, WalletDatabase},
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
    /// Outputs whose status was changed to `Invalid`
    pub marked_invalid: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidMaskMigrationOutcome {
    /// The completion flag was already set; nothing was scanned
    AlreadyCompleted,
    /// The scan ran to completion and the flag has now been set
    Completed(InvalidMaskMigrationSummary),
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

/// Run the migration unless it has already completed for this wallet.
///
/// Every output whose status is not already `Invalid` is checked. An output is marked `Invalid` if `verify_mask`
/// returns `Ok(false)` or if the check cannot be performed (`Err`); the two cases are logged and counted
/// separately. The status update for all failures happens in one database transaction after the scan, and the
/// completion flag is written only after that transaction commits.
///
/// This is CPU-bound and synchronous; call it from a blocking context.
pub fn run_invalid_mask_migration<F, B, KM>(
    flag_store: &F,
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
                        "Commitment mask migration: output id={} commitment {} (status {}, value {}) does not open to \
                         its stored value and mask; marking Invalid",
                        row.id,
                        to_hex(&row.commitment),
                        status_name(row.status),
                        row.value
                    );
                    to_invalidate.push(row.id);
                },
                MaskCheck::Unverifiable(e) => {
                    summary.verification_errors = summary.verification_errors.saturating_add(1);
                    error!(
                        target: LOG_TARGET,
                        "Commitment mask migration: could not verify output id={} commitment {} (status {}, value {}): \
                         {e}; marking Invalid",
                        row.id,
                        to_hex(&row.commitment),
                        status_name(row.status),
                        row.value
                    );
                    to_invalidate.push(row.id);
                },
            }
        }
    }

    summary.marked_invalid = output_db.mark_outputs_invalid(to_invalidate)?;
    flag_store.set_migration_flag(INVALID_MASK_MIGRATION_KEY, summary.marked_invalid.to_string())?;

    info!(
        target: LOG_TARGET,
        "Commitment mask migration complete: {} output(s) scanned, {} mask mismatch(es), {} verification error(s), {} \
         marked Invalid",
        summary.scanned,
        summary.mask_mismatches,
        summary.verification_errors,
        summary.marked_invalid
    );
    Ok(InvalidMaskMigrationOutcome::Completed(summary))
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
                Err(e) => return MaskCheck::Unverifiable(format!("unrecognised commitment mask key id: {e}")),
            };
            match key_manager.convert_legacy_tari_key_id_to_current(&legacy) {
                Ok(key_id) => key_id,
                Err(e) => {
                    return MaskCheck::Unverifiable(format!("could not convert legacy commitment mask key id: {e}"));
                },
            }
        },
    };
    let commitment = match CompressedCommitment::from_vec(&row.commitment) {
        Ok(commitment) => commitment,
        Err(e) => return MaskCheck::Unverifiable(format!("malformed commitment: {e}")),
    };
    let Ok(value) = u64::try_from(row.value) else {
        return MaskCheck::Unverifiable(format!("negative stored value {}", row.value));
    };
    check_commitment_mask(key_manager, &commitment, &key_id, value)
}

fn status_name(status: i32) -> String {
    OutputStatus::try_from(status).map_or_else(|_| format!("unknown({status})"), |s| s.to_string())
}
