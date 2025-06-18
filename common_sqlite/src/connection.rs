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
    convert::TryFrom,
    env::temp_dir,
    fs,
    iter,
    path::{Path, PathBuf},
    sync::{Arc, RwLock, RwLockWriteGuard},
    time::Duration,
};

use diesel::{
    r2d2::{ConnectionManager, PooledConnection},
    SqliteConnection,
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use log::*;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{Deserialize, Serialize};

use crate::{
    error::{SqliteStorageError, StorageError},
    sqlite_connection_pool::{PooledDbConnection, SqliteConnectionPool},
};

const LOG_TARGET: &str = "common_sqlite::connection";

/// Describes how to connect to the database (currently, SQLite).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum DbConnectionUrl {
    /// In-memory database. Each connection has it's own database
    Memory,
    /// In-memory database shared with more than one in-process connection according to the given identifier
    MemoryShared(String),
    /// Database persisted on disk
    File(PathBuf),
}

impl DbConnectionUrl {
    /// Use a file to store the database
    pub fn file<P: AsRef<Path>>(path: P) -> Self {
        DbConnectionUrl::File(path.as_ref().to_path_buf())
    }

    /// Returns a database connection string
    pub fn to_url_string(&self) -> String {
        use DbConnectionUrl::{File, Memory, MemoryShared};
        match self {
            Memory => ":memory:".to_owned(),
            MemoryShared(identifier) => format!("file:{}?mode=memory&cache=shared", identifier),
            File(path) => path
                .to_str()
                .expect("Invalid non-UTF8 character in database path")
                .to_owned(),
        }
    }

    /// Sets relative paths to use a common base path
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if let DbConnectionUrl::File(inner) = self {
            if !inner.is_absolute() {
                *inner = base_path.as_ref().join(inner.as_path());
            }
        }
    }
}

impl From<DbConnectionUrl> for String {
    fn from(source: DbConnectionUrl) -> Self {
        source.to_url_string()
    }
}

impl TryFrom<String> for DbConnectionUrl {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.as_str() == ":memory:" {
            Ok(Self::Memory)
        } else {
            Ok(Self::File(PathBuf::from(value)))
        }
    }
}

lazy_static::lazy_static! {
    static ref DB_WRITE_LOCK: Arc<RwLock<()>> = Arc::new(RwLock::new(()));
}

/// An SQLite database connection using the Diesel ORM with its r2d2 connection pool and SQLite WAL backend.
/// --------------------------------------------------------------------------------------------------------------------
/// Notes on SQLite’s Concurrency Limitations (causes of intermittent “Database is Locked” errors)
///
/// SQLite allows only one writer at a time, even in WAL mode. Under high concurrency (e.g. many threads doing writes),
/// collisions are inevitable – one transaction holds an exclusive write lock while others must wait. If a write lock
/// cannot be acquired within the busy_timeout, SQLite returns a SQLITE_BUSY (“database is locked”) error. In WAL mode,
/// readers don’t block writers and vice versa, but still only one writer can commit at any given moment. This
/// single-writer bottleneck means that bursts of simultaneous writes can lead to contention. If a transaction takes too
/// long (holding the lock), queued writers may time out (even with a 60s timeout). In short, heavy write concurrency
/// can exceed SQLite’s design limits, causing intermittent “database is locked” errors during high load.
///
/// “Busy Timeout” Not Always Honored – Deferred Write Pitfall: Even with WAL + a busy timeout, you can still get
/// immediate lock errors in certain cases. A known scenario involves deferred transactions upgrading to writes, often
/// called the “write-after-read” pattern. By default, BEGIN in SQLite is deferred – the transaction starts as read-only
/// if the first statement is a SELECT. If you later issue a write in that same transaction, SQLite will try to upgrade
/// it to a write transaction.
///
/// Mitigations and Best Practices for Write Concurrency with SQLite
/// - Use WAL Mode and Busy Timeout
/// - Start Write Transactions in IMMEDIATE Mode (`SqliteConnection::immediate_transaction(...)`)
/// - Keep Transactions Short and Optimize Write Duration
/// - Limit Write Concurrency & Pool Sizing
/// - Handle and Retry Busy Errors Gracefully
/// -
/// --------------------------------------------------------------------------------------------------------------------
#[derive(Clone)]
pub struct DbConnection {
    pool: SqliteConnectionPool,
}

impl DbConnection {
    /// Connect using the given [DbConnectionUrl](self::DbConnectionUrl), optionally using the given pool size to
    /// override the default setting of 1.
    /// Note: See https://github.com/launchbadge/sqlx/issues/362#issuecomment-636661146
    pub fn connect_url(db_url: &DbConnectionUrl, sqlite_pool_size: Option<usize>) -> Result<Self, StorageError> {
        debug!(target: LOG_TARGET, "Connecting to database using '{:?}'", db_url);

        // Ensure the path exists
        if let DbConnectionUrl::File(ref path) = db_url {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut pool = SqliteConnectionPool::new(
            db_url.to_url_string(),
            sqlite_pool_size.unwrap_or(1),
            true,
            true,
            Duration::from_secs(60),
        );
        pool.create_pool()?;

        Ok(Self::new(pool))
    }

    fn acquire_migration_write_lock() -> Result<RwLockWriteGuard<'static, ()>, StorageError> {
        match DB_WRITE_LOCK.write() {
            Ok(value) => Ok(value),
            Err(err) => Err(StorageError::DatabaseMigrationLockError(format!(
                "Failed to acquire write lock for database migration: {}",
                err
            ))),
        }
    }

    /// Connect and migrate the database, once complete, then return a handle to the migrated database.
    pub fn connect_and_migrate(
        db_url: &DbConnectionUrl,
        migrations: EmbeddedMigrations,
        sqlite_pool_size: Option<usize>,
    ) -> Result<Self, StorageError> {
        let _lock = Self::acquire_migration_write_lock()?;
        let conn = Self::connect_url(db_url, sqlite_pool_size)?;
        let output = conn.migrate(migrations)?;
        debug!(target: LOG_TARGET, "Database migration: {}", output.trim());
        Ok(conn)
    }

    fn temp_db_dir() -> PathBuf {
        temp_dir().join("tari-temp")
    }

    /// Connect and migrate the database in a temporary location, then return a handle to the migrated database.
    pub fn connect_temp_file_and_migrate(migrations: EmbeddedMigrations) -> Result<Self, StorageError> {
        fn prefixed_string(prefix: &str, len: usize) -> String {
            let mut rng = thread_rng();
            let rand_str = iter::repeat(())
                .map(|_| rng.sample(Alphanumeric) as char)
                .take(len)
                .collect::<String>();
            format!("{}{}", prefix, rand_str)
        }

        let path = DbConnection::temp_db_dir().join(prefixed_string("data-", 20));
        fs::create_dir_all(&path)?;
        let db_url = DbConnectionUrl::File(path.join("my_temp.db"));
        DbConnection::connect_and_migrate(&db_url, migrations, Some(10))
    }

    fn new(pool: SqliteConnectionPool) -> Self {
        Self { pool }
    }

    /// Fetch a connection from the pool. This function synchronously blocks the current thread for up to 60 seconds or
    /// until a connection is available.
    pub fn get_pooled_connection(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, StorageError> {
        self.pool.get_pooled_connection().map_err(StorageError::DieselR2d2Error)
    }

    /// Run database migrations
    pub fn migrate(&self, migrations: EmbeddedMigrations) -> Result<String, StorageError> {
        let mut conn = self.get_pooled_connection()?;
        let result: Vec<String> = conn
            .run_pending_migrations(migrations)
            .map(|v| v.into_iter().map(|b| format!("Running migration {}", b)).collect())
            .map_err(|err| StorageError::DatabaseMigrationFailed(format!("Database migration failed {}", err)))?;

        Ok(result.join("\r\n"))
    }

    #[cfg(test)]
    pub(crate) fn db_path(&self) -> PathBuf {
        self.pool.db_path()
    }
}

impl Drop for DbConnection {
    fn drop(&mut self) {
        let path = self.pool.db_path();

        if path.exists() {
            if let Some(parent) = path.parent() {
                if parent.starts_with(DbConnection::temp_db_dir()) {
                    debug!(target: LOG_TARGET, "DbConnection - Dropping database: {}", path.display());
                    // Explicitly cleanup and drop the connection pool to ensure all connections are released
                    let pool_state = self.pool.cleanup();
                    debug!(target: LOG_TARGET, "DbConnection - Pool stats before cleanup: {:?}", pool_state);
                    debug!(target: LOG_TARGET, "DbConnection - Cleaning up tempdir: {}", parent.display());
                    if let Err(e) = fs::remove_dir_all(parent) {
                        error!(target: LOG_TARGET, "Failed to clean up temp dir: {}", e);
                    } else {
                        debug!(target: LOG_TARGET, "Temp dir cleaned up: {}", parent.display());
                    }
                }
            }
        }
    }
}

impl PooledDbConnection for DbConnection {
    type Error = SqliteStorageError;

    fn get_pooled_connection(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, Self::Error> {
        let conn = self.pool.get_pooled_connection()?;
        Ok(conn)
    }
}

#[cfg(test)]
mod test {
    use diesel::{dsl::sql, sql_types::Integer, RunQueryDsl};
    use diesel_migrations::embed_migrations;

    use super::*;

    #[tokio::test]
    async fn connect_and_migrate() {
        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./test/migrations");

        let db_conn = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let path = db_conn.db_path();
        let mut pool_conn = db_conn.get_pooled_connection().unwrap();
        let count: i32 = sql::<Integer>("SELECT COUNT(*) FROM test_table")
            .get_result(&mut pool_conn)
            .unwrap();
        assert_eq!(count, 0);

        // Test temporary file cleanup
        assert!(path.exists());
        drop(pool_conn);
        drop(db_conn);
        assert!(!path.exists());
    }
}
