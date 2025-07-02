//  Copyright 2025, The Tari Project
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

use std::{collections::BTreeMap, sync::Arc};

use lmdb_zero::{Environment, ReadTransaction, WriteTransaction};
use log::info;
use tari_storage::lmdb_store::{DatabaseRef, LMDBConfig, LMDBStore};

use crate::chain_storage::{
    lmdb_db::{
        lmdb::{lmdb_get, lmdb_replace},
        lmdb_db::{MetadataKey, MetadataValue},
    },
    ChainStorageError,
    LMDBDatabase,
};

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db::migrations";

/// Trait for defining migrations in the LMDB database
pub trait Migration {
    fn version(&self) -> u64;

    fn run(&self, db: Option<&mut LMDBDatabase>) -> MigrationStatus;

    fn requires_backend(&self) -> bool;
}

/// A context for running migrations, containing the environment and metadata database reference
#[derive(Clone, Debug)]
pub struct MigrationContext {
    pub env: Arc<Environment>,
    pub env_config: LMDBConfig,
    pub metadata_db: DatabaseRef,
}

impl MigrationContext {
    /// Provides a read-only reference to the LMDB environment
    pub fn read_transaction(&self) -> Result<ReadTransaction<'_>, ChainStorageError> {
        ReadTransaction::new(&*self.env).map_err(Into::into)
    }

    /// Provides a write reference to the LMDB environment
    pub fn write_transaction(&self) -> Result<WriteTransaction<'_>, ChainStorageError> {
        WriteTransaction::new(&*self.env).map_err(Into::into)
    }

    pub fn resize_if_required(&self) -> Result<(), ChainStorageError> {
        unsafe { Ok(LMDBStore::resize_if_required(&self.env, &self.env_config, None)?) }
    }

    /// Validates the migration status by checking the in-progress and completed metadata in the database amd returns
    /// the lists of completed and in-progress migrations.
    pub fn validate_migration_status(&self) -> Result<(Vec<u64>, Vec<u64>), ChainStorageError> {
        let txn = self.read_transaction()?;
        let legacy = lmdb_get::<_, MetadataValue>(&txn, &self.metadata_db, &MetadataKey::MigrationVersion.as_u32())?;
        let in_progress_migrations =
            lmdb_get::<_, MetadataValue>(&txn, &self.metadata_db, &MetadataKey::InProgressMigrations.as_u32())?
                .unwrap_or(MetadataValue::InProgressMigrations(Vec::new()));
        let completed_migrations =
            lmdb_get::<_, MetadataValue>(&txn, &self.metadata_db, &MetadataKey::CompletedMigrations.as_u32())?
                .unwrap_or(MetadataValue::InProgressMigrations(Vec::new()));
        drop(txn);

        let legacy_migrated_version = match legacy {
            Some(MetadataValue::MigrationVersion(n)) => Some(n),
            _ => None,
        };
        let in_progress = match in_progress_migrations {
            MetadataValue::InProgressMigrations(mut vec) => {
                vec.sort();
                vec
            },
            _ => Vec::new(),
        };
        let mut completed = match completed_migrations {
            MetadataValue::CompletedMigrations(mut vec) => {
                vec.sort();
                vec
            },
            _ => Vec::new(),
        };

        if in_progress.iter().any(|v| completed.contains(v)) {
            return Err(ChainStorageError::CriticalError(format!(
                "Completed migrations ({:?}) cannot be in progress ({:?})",
                completed, in_progress
            )));
        }

        if completed.is_empty() && in_progress.is_empty() {
            if let Some(legacy_version) = legacy_migrated_version {
                let txn = self.write_transaction()?;
                // legacy_version indicates the next migration to run; completed is therefore one less than that
                completed = (0..legacy_version).collect::<Vec<_>>();
                lmdb_replace(
                    &txn,
                    &self.metadata_db,
                    &MetadataKey::CompletedMigrations.as_u32(),
                    &MetadataValue::CompletedMigrations(completed.clone()),
                    None,
                )?;
            }
        }

        Ok((completed, in_progress))
    }

    /// Updates the migration status in the database by replacing the metadata for completed and in-progress migrations.
    pub(crate) fn update_migration_status(&self, status: MigrationStatus) -> Result<(), ChainStorageError> {
        if let Some(completed_version) = status.completed_version {
            self.update_completed_status(completed_version)?;
        }

        if let Some(in_progress_version) = status.in_progress_version {
            self.update_in_progress_status(in_progress_version)?;
        }

        if let Some(migration_error) = status.migration_error {
            self.update_error_status(migration_error)?;
        }

        Ok(())
    }

    fn update_completed_status(&self, completed_version: u64) -> Result<(), ChainStorageError> {
        info!(
            target: LOG_TARGET,
            "[MIGRATIONS] Completed migration v{}",
            completed_version
        );
        let txn = self.write_transaction()?;

        // Update the completed migrations in the metadata database
        let completed_migrations =
            lmdb_get::<_, MetadataValue>(&txn, &self.metadata_db, &MetadataKey::CompletedMigrations.as_u32())?
                .unwrap_or(MetadataValue::CompletedMigrations(Vec::new()));
        let migrations = match completed_migrations {
            MetadataValue::CompletedMigrations(mut vec) => {
                if !vec.contains(&completed_version) {
                    vec.push(completed_version);
                }
                vec.sort();
                vec
            },
            _ => {
                vec![completed_version]
            },
        };
        lmdb_replace(
            &txn,
            &self.metadata_db,
            &MetadataKey::CompletedMigrations.as_u32(),
            &MetadataValue::CompletedMigrations(migrations),
            None,
        )?;

        // If there was a pending migration error in db key MigrationsWithErrors(Vec<u64>), we remove it
        let migrations_with_errors =
            lmdb_get::<_, MetadataValue>(&txn, &self.metadata_db, &MetadataKey::MigrationsWithErrors.as_u32())?
                .unwrap_or(MetadataValue::MigrationsWithErrors(Vec::new()));
        if let MetadataValue::MigrationsWithErrors(errors) = migrations_with_errors {
            let updated_errors: Vec<(u64, String)> = errors
                .into_iter()
                .filter(|(version, _)| *version != completed_version)
                .collect();
            lmdb_replace(
                &txn,
                &self.metadata_db,
                &MetadataKey::MigrationsWithErrors.as_u32(),
                &MetadataValue::MigrationsWithErrors(updated_errors),
                None,
            )?;
        }

        txn.commit()?;

        Ok(())
    }

    fn update_in_progress_status(&self, in_progress_version: u64) -> Result<(), ChainStorageError> {
        info!(
            target: LOG_TARGET,
            "[MIGRATIONS] Migration v{} is in progress",
            in_progress_version
        );
        let txn = self.write_transaction()?;
        let in_progress_migrations =
            lmdb_get::<_, MetadataValue>(&txn, &self.metadata_db, &MetadataKey::InProgressMigrations.as_u32())?
                .unwrap_or(MetadataValue::InProgressMigrations(Vec::new()));
        let migrations = match in_progress_migrations {
            MetadataValue::InProgressMigrations(mut vec) => {
                if !vec.contains(&in_progress_version) {
                    vec.push(in_progress_version);
                }
                vec.sort();
                vec
            },
            _ => {
                vec![in_progress_version]
            },
        };
        lmdb_replace(
            &txn,
            &self.metadata_db,
            &MetadataKey::InProgressMigrations.as_u32(),
            &MetadataValue::InProgressMigrations(migrations),
            None,
        )?;
        txn.commit()?;

        Ok(())
    }

    fn update_error_status(&self, migration_error: (u64, String)) -> Result<(), ChainStorageError> {
        // If there was a pending migration error in db key MigrationsWithErrors(Vec<u64>) for the same version, we
        // replace it, otherwise we add it
        info!(
            target: LOG_TARGET,
            "[MIGRATIONS] Migration v{} encountered an error: {}",
            migration_error.0, migration_error.1
        );
        let txn = self.write_transaction()?;
        let migrations_with_errors =
            lmdb_get::<_, MetadataValue>(&txn, &self.metadata_db, &MetadataKey::MigrationsWithErrors.as_u32())?
                .unwrap_or(MetadataValue::MigrationsWithErrors(Vec::new()));
        let errors = match migrations_with_errors {
            MetadataValue::MigrationsWithErrors(mut vec) => {
                if let Some(pos) = vec.iter().position(|(version, _)| *version == migration_error.0) {
                    vec[pos] = migration_error;
                } else {
                    vec.push(migration_error);
                }
                vec
            },
            _ => {
                vec![migration_error]
            },
        };
        lmdb_replace(
            &txn,
            &self.metadata_db,
            &MetadataKey::MigrationsWithErrors.as_u32(),
            &MetadataValue::MigrationsWithErrors(errors),
            None,
        )?;

        Ok(())
    }

    /// Checks if a migration with the given version has been completed
    pub fn is_migration_completed(&self, version: u64) -> Result<bool, ChainStorageError> {
        let txn = self.read_transaction()?;
        let completed_migrations =
            lmdb_get::<_, MetadataValue>(&txn, &self.metadata_db, &MetadataKey::CompletedMigrations.as_u32())?
                .unwrap_or(MetadataValue::CompletedMigrations(Vec::new()));
        let completed = match completed_migrations {
            MetadataValue::CompletedMigrations(vec) => vec,
            _ => Vec::new(),
        };
        Ok(completed.contains(&version))
    }

    /// Check if a migration with the given version reported any error
    pub fn does_migration_have_error(&self, version: u64) -> Result<Option<String>, ChainStorageError> {
        let txn = self.read_transaction()?;
        let migrations_with_errors =
            lmdb_get::<_, MetadataValue>(&txn, &self.metadata_db, &MetadataKey::MigrationsWithErrors.as_u32())?
                .unwrap_or(MetadataValue::MigrationsWithErrors(Vec::new()));
        let errors = match migrations_with_errors {
            MetadataValue::MigrationsWithErrors(vec) => vec,
            _ => Vec::new(),
        };
        for (err_version, error_message) in errors {
            if err_version == version {
                return Ok(Some(error_message));
            }
        }
        Ok(None)
    }
}

/// Represents the status of a migration, indicating whether it has been completed or is in progress
#[derive(Clone, Debug)]
pub(crate) struct MigrationStatus {
    pub(crate) completed_version: Option<u64>,
    pub(crate) in_progress_version: Option<u64>,
    pub(crate) migration_error: Option<(u64, String)>,
}

/// Manages the migrations for the LMDB database
pub struct MigrationManager {
    migrations: BTreeMap<u64, Box<dyn Migration>>,
    ctx: MigrationContext,
}

impl MigrationManager {
    /// Creates a new instance of `MigrationManager`
    pub fn new(ctx: MigrationContext) -> Self {
        Self {
            migrations: BTreeMap::new(),
            ctx,
        }
    }

    /// Adds a migration to the manager
    pub fn add_migration(&mut self, migration: Box<dyn Migration>) -> Result<(), ChainStorageError> {
        if self.migrations.contains_key(&migration.version()) {
            return Err(ChainStorageError::DuplicateMigrationVersion {
                version: migration.version(),
            });
        }
        self.migrations.insert(migration.version(), migration);
        Ok(())
    }

    /// Runs all migrations that have not been completed yet
    pub fn run_migrations(&self, mut db: Option<&mut LMDBDatabase>) -> Result<(), ChainStorageError> {
        let (completed, in_progress) = self.ctx.validate_migration_status()?;
        info!(
            target: LOG_TARGET,
            "[MIGRATIONS]: Completed migrations: {}, In-progress migrations: {}",
            completed.iter().map(|v|format!("v{}", v)).collect::<Vec<_>>().join(", "),
            in_progress.iter().map(|v|format!("v{}", v)).collect::<Vec<_>>().join(", "),
        );

        // Filter migrations to exclude completed ones
        let migrations_to_run: BTreeMap<u64, &Box<dyn Migration>> = self
            .migrations
            .iter()
            .filter(|(version, _)| !completed.contains(version))
            .map(|(version, migration)| (*version, migration))
            .collect();
        if !migrations_to_run.is_empty() {
            info!(
                target: LOG_TARGET,
                "[MIGRATIONS]: Migrations to run: {}",
                migrations_to_run.keys().map(|v|format!("v{}", v)).collect::<Vec<_>>().join(", "),
            );
        }

        for (_key, migration) in migrations_to_run {
            self.ctx.resize_if_required()?;
            let db_ref = if migration.requires_backend() {
                if let Some(ref db) = db {
                    db.stats_collector().set_current_db_version(migration.version());
                }
                db.as_deref_mut()
            } else {
                None
            };
            let status = migration.run(db_ref);
            self.ctx.update_migration_status(status.clone())?;
            if let Some(error) = status.migration_error {
                return Err(ChainStorageError::GeneralMigrationError(error.1));
            }
        }

        Ok(())
    }
}
