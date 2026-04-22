//  Copyright 2026, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{fs, path::Path};

use log::*;
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_core::{
    chain_storage::{BlockchainBackend, CompactReport, compact_lmdb_database, create_lmdb_database},
    consensus::BaseNodeConsensusManager,
};
use tari_storage::lmdb_store::BYTES_PER_MB;

use crate::BaseNodeConfig;

const LOG_TARGET: &str = "minotari::base_node::compaction";

/// Run the full compaction cycle: compact → validate → swap.
/// Called before the node starts when `--compact-db` is passed.
pub fn run_compaction(config: &BaseNodeConfig) -> Result<CompactReport, ExitError> {
    let db_path = &config.lmdb_path;
    if !db_path.is_absolute() {
        return Err(ExitError::new(
            ExitCode::DatabaseError,
            format!(
                "Database path must be absolute, got: {}. Ensure set_base_path() was called.",
                db_path.display()
            ),
        ));
    }
    let db_parent = db_path.parent().ok_or_else(|| {
        ExitError::new(
            ExitCode::DatabaseError,
            "Database path has no parent directory".to_string(),
        )
    })?;
    let compact_path = db_parent.join("db_compact");
    let backup_path = db_parent.join("db_backup");

    info!(target: LOG_TARGET, "Starting database compaction: {:?}", db_path);

    // Phase 0: Pre-flight checks
    if !db_path.exists() {
        return Err(ExitError::new(
            ExitCode::DatabaseError,
            format!("Database path does not exist: {}", db_path.display()),
        ));
    }

    let lock_file = db_path.join(".chain_storage_file.lock");
    if lock_file.exists() {
        warn!(
            target: LOG_TARGET,
            "Lock file exists at {}. Ensure no other node instance is running.",
            lock_file.display()
        );
    }

    let data_file = db_path.join("data.mdb");
    let db_size = fs::metadata(&data_file).map(|m| m.len()).unwrap_or(0);

    info!(
        target: LOG_TARGET,
        "Current database size: {:.2} MB",
        db_size as f64 / BYTES_PER_MB as f64
    );

    // Clean up leftover directories from previous failed attempts
    for leftover in [&compact_path, &backup_path] {
        if leftover.exists() {
            info!(target: LOG_TARGET, "Removing leftover directory: {:?}", leftover);
            fs::remove_dir_all(leftover).map_err(|e| {
                ExitError::new(
                    ExitCode::DatabaseError,
                    format!("Failed to remove leftover directory '{}': {e}", leftover.display()),
                )
            })?;
        }
    }

    // Phase 1: Compact
    info!(target: LOG_TARGET, "Phase 1/3: Compacting database to {:?}", compact_path);
    let report = compact_lmdb_database(db_path, &compact_path).map_err(|e| {
        let _ = fs::remove_dir_all(&compact_path);
        ExitError::new(ExitCode::DatabaseError, format!("Compaction failed: {e}"))
    })?;

    // Phase 2: Validate
    info!(target: LOG_TARGET, "Phase 2/3: Validating compacted database");
    if let Err(e) = validate_compacted_db(config, db_path, &compact_path) {
        error!(target: LOG_TARGET, "Validation failed: {e}");
        let _ = fs::remove_dir_all(&compact_path);
        return Err(e);
    }
    info!(target: LOG_TARGET, "Validation passed: compacted database is consistent");

    // Phase 3: Swap
    info!(target: LOG_TARGET, "Phase 3/3: Swapping databases");
    swap_databases(db_path, &compact_path, &backup_path)?;

    let reduction = if report.original_size > 0 {
        (1.0 - report.compacted_size as f64 / report.original_size as f64) * 100.0
    } else {
        0.0
    };
    info!(
        target: LOG_TARGET,
        "Compaction complete!\n  Before: {:.2} MB\n  After:  {:.2} MB\n  Reduction: {:.1}%\n  Duration: {:.2?}",
        report.original_size as f64 / BYTES_PER_MB as f64,
        report.compacted_size as f64 / BYTES_PER_MB as f64,
        reduction,
        report.duration,
    );

    Ok(report)
}

fn validate_compacted_db(config: &BaseNodeConfig, original_path: &Path, compact_path: &Path) -> Result<(), ExitError> {
    let rules = BaseNodeConsensusManager::builder(config.network)
        .build()
        .map_err(|e| ExitError::new(ExitCode::UnknownError, e))?;

    let original_db = create_lmdb_database(original_path, config.lmdb.clone(), rules.clone())
        .map_err(|e| ExitError::new(ExitCode::DatabaseError, format!("Failed to open original DB: {e}")))?;
    let original_metadata = original_db.fetch_chain_metadata().map_err(|e| {
        ExitError::new(
            ExitCode::DatabaseError,
            format!("Failed to read original metadata: {e}"),
        )
    })?;
    drop(original_db);

    let compact_db = create_lmdb_database(compact_path, config.lmdb.clone(), rules)
        .map_err(|e| ExitError::new(ExitCode::DatabaseError, format!("Failed to open compacted DB: {e}")))?;
    let compact_metadata = compact_db.fetch_chain_metadata().map_err(|e| {
        ExitError::new(
            ExitCode::DatabaseError,
            format!("Failed to read compacted metadata: {e}"),
        )
    })?;
    drop(compact_db);

    if original_metadata.best_block_height() != compact_metadata.best_block_height() {
        return Err(ExitError::new(
            ExitCode::DatabaseError,
            format!(
                "Tip height mismatch: original={}, compacted={}",
                original_metadata.best_block_height(),
                compact_metadata.best_block_height(),
            ),
        ));
    }

    if original_metadata.best_block_hash() != compact_metadata.best_block_hash() {
        return Err(ExitError::new(
            ExitCode::DatabaseError,
            format!(
                "Best block hash mismatch: original={}, compacted={}",
                original_metadata.best_block_hash(),
                compact_metadata.best_block_hash(),
            ),
        ));
    }

    info!(
        target: LOG_TARGET,
        "Tip height: #{}, Best block: {}",
        original_metadata.best_block_height(),
        original_metadata.best_block_hash(),
    );

    Ok(())
}

fn swap_databases(original: &Path, compact: &Path, backup: &Path) -> Result<(), ExitError> {
    fs::rename(original, backup).map_err(|e| {
        ExitError::new(
            ExitCode::DatabaseError,
            format!("Failed to rename original DB to backup: {e}"),
        )
    })?;

    if let Err(e) = fs::rename(compact, original) {
        error!(
            target: LOG_TARGET,
            "Failed to rename compacted DB to original: {e}. Attempting rollback..."
        );
        if let Err(rollback_err) = fs::rename(backup, original) {
            error!(
                target: LOG_TARGET,
                "CRITICAL: Rollback also failed: {rollback_err}. Manual intervention required!\n\
                 The original database is at: {}\n\
                 Move it back to: {}",
                backup.display(),
                original.display(),
            );
            return Err(ExitError::new(
                ExitCode::DatabaseError,
                format!(
                    "CRITICAL: Both swap and rollback failed. Original DB at '{}', move to '{}'.",
                    backup.display(),
                    original.display(),
                ),
            ));
        }
        info!(target: LOG_TARGET, "Rollback successful. Original database restored.");
        return Err(ExitError::new(
            ExitCode::DatabaseError,
            format!("Failed to swap compacted DB into place: {e}"),
        ));
    }

    info!(target: LOG_TARGET, "Removing backup directory: {:?}", backup);
    if let Err(e) = fs::remove_dir_all(backup) {
        warn!(
            target: LOG_TARGET,
            "Failed to remove backup directory '{}': {e}. It can be manually removed.",
            backup.display()
        );
    }

    Ok(())
}
