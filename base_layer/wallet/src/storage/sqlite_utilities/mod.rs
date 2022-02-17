// Copyright 2020. The Tari Project
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

use std::{
    fs::File,
    path::{Path, PathBuf},
    time::Duration,
};

use fs2::FileExt;
use log::*;
use tari_common_sqlite::sqlite_connection_pool::SqliteConnectionPool;
pub use wallet_db_connection::WalletDbConnection;

use crate::{
    contacts_service::storage::sqlite_db::ContactsServiceSqliteDatabase,
    error::WalletStorageError,
    output_manager_service::storage::sqlite_db::OutputManagerSqliteDatabase,
    storage::{database::WalletDatabase, sqlite_db::wallet::WalletSqliteDatabase},
    transaction_service::storage::sqlite_db::TransactionServiceSqliteDatabase,
};

pub(crate) mod wallet_db_connection;

const LOG_TARGET: &str = "wallet::storage:sqlite_utilities";

pub fn run_migration_and_create_sqlite_connection<P: AsRef<Path>>(
    db_path: P,
    sqlite_pool_size: usize,
) -> Result<WalletDbConnection, WalletStorageError> {
    let file_lock = acquire_exclusive_file_lock(&db_path.as_ref().to_path_buf())?;

    let path_str = db_path
        .as_ref()
        .to_str()
        .ok_or(WalletStorageError::InvalidUnicodePath)?;

    let mut pool = SqliteConnectionPool::new(
        String::from(path_str),
        sqlite_pool_size,
        true,
        true,
        Duration::from_secs(60),
    );
    pool.create_pool()?;
    let connection = pool.get_pooled_connection()?;

    embed_migrations!("./migrations");
    embedded_migrations::run(&connection)
        .map_err(|err| WalletStorageError::DatabaseMigrationError(format!("Database migration failed {}", err)))?;

    Ok(WalletDbConnection::new(pool, Some(file_lock)))
}

/// This function will copy a wallet database to the provided path and then clear the Master Private Key from the
/// database.
pub async fn partial_wallet_backup<P: AsRef<Path>>(current_db: P, backup_path: P) -> Result<(), WalletStorageError> {
    // Copy the current db to the backup path
    let db_path = current_db
        .as_ref()
        .to_str()
        .ok_or(WalletStorageError::InvalidUnicodePath)?;
    let backup_path = backup_path
        .as_ref()
        .to_str()
        .ok_or(WalletStorageError::InvalidUnicodePath)?;
    std::fs::copy(db_path, backup_path)
        .map_err(|_| WalletStorageError::FileError("Could not copy database file for backup".to_string()))?;

    // open a connection and clear the Master Secret Key
    let connection = run_migration_and_create_sqlite_connection(backup_path, 16)?;
    let db = WalletDatabase::new(WalletSqliteDatabase::new(connection, None)?);
    db.clear_master_seed().await?;

    Ok(())
}

pub fn acquire_exclusive_file_lock(db_path: &Path) -> Result<File, WalletStorageError> {
    let lock_file_path = match db_path.file_name() {
        None => {
            return Err(WalletStorageError::FileError(
                "Database path should be to a file".to_string(),
            ))
        },
        Some(filename) => match db_path.parent() {
            Some(p) => p.join(format!(
                ".{}.lock",
                filename
                    .to_str()
                    .ok_or_else(|| WalletStorageError::FileError("Could not acquire database filename".to_string()))?
            )),
            None => return Err(WalletStorageError::DatabasePathIsRootPath),
        },
    };

    let file = File::create(lock_file_path)?;

    // Attempt to acquire exclusive OS level Write Lock
    if let Err(e) = file.try_lock_exclusive() {
        error!(
            target: LOG_TARGET,
            "Could not acquire exclusive write lock on database lock file: {:?}", e
        );
        return Err(WalletStorageError::CannotAcquireFileLock);
    }

    Ok(file)
}

pub fn initialize_sqlite_database_backends(
    db_path: PathBuf,
    passphrase: Option<String>,
    sqlite_pool_size: usize,
) -> Result<
    (
        WalletSqliteDatabase,
        TransactionServiceSqliteDatabase,
        OutputManagerSqliteDatabase,
        ContactsServiceSqliteDatabase,
    ),
    WalletStorageError,
> {
    let connection = run_migration_and_create_sqlite_connection(&db_path, sqlite_pool_size).map_err(|e| {
        error!(
            target: LOG_TARGET,
            "Error creating Sqlite Connection in Wallet: {:?}", e
        );
        e
    })?;

    let wallet_backend = WalletSqliteDatabase::new(connection.clone(), passphrase)?;
    let transaction_backend = TransactionServiceSqliteDatabase::new(connection.clone(), wallet_backend.cipher());
    let output_manager_backend = OutputManagerSqliteDatabase::new(connection.clone(), wallet_backend.cipher());
    let contacts_backend = ContactsServiceSqliteDatabase::new(connection);

    Ok((
        wallet_backend,
        transaction_backend,
        output_manager_backend,
        contacts_backend,
    ))
}
