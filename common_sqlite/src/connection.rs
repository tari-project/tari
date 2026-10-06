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
    SqliteConnection,
    r2d2::{ConnectionManager, PooledConnection},
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use log::*;
use rand::{RngExt, distr::Alphanumeric};
use serde::{Deserialize, Serialize};

use crate::{
    connection_options::PRAGMA_BUSY_TIMEOUT,
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
            MemoryShared(identifier) => format!("file:{identifier}?mode=memory&cache=shared"),
            File(path) => path
                .to_str()
                .expect("Invalid non-UTF8 character in database path")
                .to_owned(),
        }
    }

    /// Sets relative paths to use a common base path
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if let DbConnectionUrl::File(inner) = self &&
            !inner.is_absolute()
        {
            *inner = base_path.as_ref().join(inner.as_path());
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
    /// Set only for the temp-file databases created by [`Self::connect_temp_file_and_migrate`].
    /// Shared across clones so the directory is removed exactly once - see [`TempDatabaseDir`].
    temp_dir: Option<Arc<TempDatabaseDir>>,
}

/// Owns the temporary directory backing a temp-file database and removes it when the last
/// [`DbConnection`] sharing it is dropped.
///
/// `DbConnection` is `Clone`, and one database is routinely held through several handles at once:
/// `DhtActor`, for instance, hands one to `DedupCacheDatabase` and another to `DhtDatabase`, which
/// then makes further clones per spawned task. Removing the directory in `DbConnection::drop` let
/// whichever handle went out of scope first delete the file out from under all the others, which
/// surfaced as `disk I/O error` and silently empty query results in the surviving handles.
struct TempDatabaseDir {
    pool: SqliteConnectionPool,
    dir: PathBuf,
}

impl Drop for TempDatabaseDir {
    fn drop(&mut self) {
        if !self.dir.exists() {
            return;
        }
        // Release the pool's connections before unlinking the files they are open on.
        let pool_state = self.pool.cleanup();
        debug!(target: LOG_TARGET, "DbConnection - Pool stats before cleanup: {pool_state:?}");
        debug!(target: LOG_TARGET, "DbConnection - Cleaning up tempdir: {}", self.dir.display());
        if let Err(e) = fs::remove_dir_all(&self.dir) {
            error!(target: LOG_TARGET, "Failed to clean up temp dir: {e}");
        } else {
            debug!(target: LOG_TARGET, "Temp dir cleaned up: {}", self.dir.display());
        }
    }
}

impl DbConnection {
    /// Connect using the given [DbConnectionUrl](self::DbConnectionUrl), optionally using the given pool size to
    /// override the default setting of 1.
    /// Note: See https://github.com/launchbadge/sqlx/issues/362#issuecomment-636661146
    pub fn connect_url(db_url: &DbConnectionUrl, sqlite_pool_size: Option<usize>) -> Result<Self, StorageError> {
        Self::connect_url_with_busy_timeout(db_url, sqlite_pool_size, PRAGMA_BUSY_TIMEOUT)
    }

    /// As [`Self::connect_url`], but with an explicit `PRAGMA busy_timeout`.
    ///
    /// The default ([`PRAGMA_BUSY_TIMEOUT`], 60s) suits databases where losing a write is worse than
    /// waiting - the wallet, for instance. It is a poor fit for databases on a hot networking path,
    /// where a call that waits a minute for a lock has long outlived the request that made it and,
    /// even when run on a blocking thread pool, occupies a thread in it for that whole minute. Such
    /// callers should pass something far shorter.
    pub fn connect_url_with_busy_timeout(
        db_url: &DbConnectionUrl,
        sqlite_pool_size: Option<usize>,
        busy_timeout: Duration,
    ) -> Result<Self, StorageError> {
        debug!(target: LOG_TARGET, "Connecting to database using '{db_url:?}' (busy_timeout {busy_timeout:.0?})");

        // Ensure the path exists
        if let DbConnectionUrl::File(path) = db_url &&
            let Some(parent) = path.parent()
        {
            std::fs::create_dir_all(parent)?;
        }

        let mut pool = SqliteConnectionPool::new(
            db_url.to_url_string(),
            sqlite_pool_size.unwrap_or(1),
            true,
            true,
            busy_timeout,
        );
        pool.create_pool()?;

        debug!(target: LOG_TARGET, "{}", pool);

        Ok(Self::new(pool))
    }

    fn acquire_migration_write_lock() -> Result<RwLockWriteGuard<'static, ()>, StorageError> {
        match DB_WRITE_LOCK.write() {
            Ok(value) => Ok(value),
            Err(err) => Err(StorageError::DatabaseMigrationLockError(format!(
                "Failed to acquire write lock for database migration: {err}"
            ))),
        }
    }

    /// Returns true **if** the migration write lock is currently held by *some* writer in this
    /// process. We detect this by attempting a non-blocking read; it fails while a write lock is
    /// held.
    #[inline]
    pub fn migration_lock_active() -> bool {
        DB_WRITE_LOCK.try_read().is_err()
    }

    /// Connect and migrate the database, once complete, then return a handle to the migrated database.
    pub fn connect_and_migrate(
        db_url: &DbConnectionUrl,
        migrations: EmbeddedMigrations,
        sqlite_pool_size: Option<usize>,
    ) -> Result<Self, StorageError> {
        Self::connect_and_migrate_with_busy_timeout(db_url, migrations, sqlite_pool_size, PRAGMA_BUSY_TIMEOUT)
    }

    /// As [`Self::connect_and_migrate`], but with an explicit `PRAGMA busy_timeout`. See
    /// [`Self::connect_url_with_busy_timeout`] for when to override the default.
    pub fn connect_and_migrate_with_busy_timeout(
        db_url: &DbConnectionUrl,
        migrations: EmbeddedMigrations,
        sqlite_pool_size: Option<usize>,
        busy_timeout: Duration,
    ) -> Result<Self, StorageError> {
        let _lock = Self::acquire_migration_write_lock()?;
        let conn = Self::connect_url_with_busy_timeout(db_url, sqlite_pool_size, busy_timeout)?;
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
            let mut rng = rand::rng();
            let rand_str = iter::repeat(())
                .map(|_| rng.sample(Alphanumeric) as char)
                .take(len)
                .collect::<String>();
            format!("{prefix}{rand_str}")
        }

        let path = DbConnection::temp_db_dir().join(prefixed_string("data-", 20));
        fs::create_dir_all(&path)?;
        let db_url = DbConnectionUrl::File(path.join("my_temp.db"));
        let mut conn = DbConnection::connect_and_migrate(&db_url, migrations, Some(10))?;
        conn.temp_dir = Some(Arc::new(TempDatabaseDir {
            pool: conn.pool.clone(),
            dir: path,
        }));
        Ok(conn)
    }

    fn new(pool: SqliteConnectionPool) -> Self {
        Self { pool, temp_dir: None }
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
            .map(|v| v.into_iter().map(|b| format!("Running migration {b}")).collect())
            .map_err(|err| StorageError::DatabaseMigrationFailed(format!("Database migration failed {err}")))?;

        Ok(result.join("\r\n"))
    }

    #[cfg(test)]
    pub(crate) fn db_path(&self) -> PathBuf {
        self.pool.db_path()
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
    use std::sync::Arc;

    use diesel::{
        RunQueryDsl,
        connection::SimpleConnection,
        dsl::sql,
        sql_types::{Integer, Text},
    };
    use diesel_migrations::embed_migrations;
    use tokio::{sync::Barrier, task::JoinSet};

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

    /// A temp database is routinely held through several clones at once - `DhtActor`, for instance,
    /// hands one to `DedupCacheDatabase` and another to `DhtDatabase`, which then clones further per
    /// spawned task. Cleaning up in `DbConnection::drop` let whichever clone went out of scope first
    /// delete the file out from under the rest, which surfaced as `disk I/O error` and silently
    /// empty results in the survivors.
    #[tokio::test]
    async fn temp_dir_survives_until_the_last_clone_is_dropped() {
        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./test/migrations");

        let db_conn = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let path = db_conn.db_path();
        assert!(path.exists());

        // Write through a clone, then drop the clone. This is the point at which the old `Drop`
        // removed the directory.
        let clone = db_conn.clone();
        {
            let mut pool_conn = clone.get_pooled_connection().unwrap();
            pool_conn
                .batch_execute("INSERT INTO test_table (id) VALUES (1);")
                .unwrap();
        }
        drop(clone);

        assert!(path.exists(), "the database was removed while another handle was live");

        // ...and the original handle can still read what the clone wrote.
        let mut pool_conn = db_conn
            .get_pooled_connection()
            .expect("surviving handle lost its database");
        let count: i32 = sql::<Integer>("SELECT COUNT(*) FROM test_table")
            .get_result(&mut pool_conn)
            .expect("query through the surviving handle failed");
        assert_eq!(count, 1);

        // The last handle going away is what cleans up.
        drop(pool_conn);
        drop(db_conn);
        assert!(!path.exists(), "temp dir was not cleaned up by the last handle");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn stress_connect_and_migrate_contention() {
        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./test/migrations");
        let db = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();

        // Force very frequent WAL checkpoints to increase write pressure.
        // The SQLite "PRAGMA wal_autocheckpoint = 1;" executes a SQLite PRAGMA that will checkpoint
        // the Write-Ahead Log (WAL) after every transaction. This increases the frequency of
        // checkpointing, which can help test write contention and durability in high-concurrency
        // scenarios.
        let mut c = db.get_pooled_connection().unwrap();
        sql::<Integer>("PRAGMA wal_autocheckpoint = 1;")
            .execute(&mut c)
            .unwrap();
        let mode: String = sql::<Text>("PRAGMA journal_mode;").get_result(&mut c).unwrap();
        assert!(mode.eq_ignore_ascii_case("wal"));

        let busy: String = sql::<Text>("PRAGMA busy_timeout;").get_result(&mut c).unwrap();
        assert!(busy.parse::<u128>().unwrap() >= PRAGMA_BUSY_TIMEOUT.as_millis());

        // We have 'sqlite_pool_size = Some(10))', so '160 writers + 320 readers' must queue.
        const WRITERS: usize = 160;
        const READERS: usize = 320;
        const HOLD_MS: u64 = 100;

        let barrier = Arc::new(Barrier::new(WRITERS + READERS));
        let mut tasks = JoinSet::new();

        // Writers
        for _ in 0..WRITERS {
            // Let each spawned async task gets its own reference to the same synchronization barrier.
            let synchronization_barrier = barrier.clone();
            let db2 = db.clone();
            tasks.spawn(async move {
                // The synchronization barrier allows all tasks to wait at the barrier and proceed together once all
                // have reached it, enabling coordinated concurrent execution for this test.
                synchronization_barrier.wait().await;
                // IMPORTANT: await the blocking job
                tokio::task::spawn_blocking(move || {
                    let mut conn = db2.get_pooled_connection().expect("writer checkout");
                    // Acquires an immediate exclusive lock on the database for this write
                    conn.batch_execute("BEGIN EXCLUSIVE;").unwrap();
                    sql::<Integer>("INSERT INTO test_table DEFAULT VALUES;")
                        .execute(&mut conn)
                        .unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(HOLD_MS));
                    conn.batch_execute("COMMIT;").unwrap();
                })
                .await
                .expect("writer join");
            });
        }
        // Readers
        for _ in 0..READERS {
            let b = barrier.clone();
            let db2 = db.clone();
            tasks.spawn(async move {
                b.wait().await;
                tokio::task::spawn_blocking(move || {
                    let mut conn = db2.get_pooled_connection().expect("reader checkout");
                    for _ in 0..3 {
                        let _: i32 = sql::<Integer>("SELECT COUNT(*) FROM test_table")
                            .get_result(&mut conn)
                            .expect("reader select");
                        // Small pause between reads (async sleep outside blocking isn’t usable here)
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                })
                .await
                .expect("reader join");
            });
        }

        while let Some(res) = tasks.join_next().await {
            res.expect("task panicked");
        }

        // Verify row count
        let mut c = db.get_pooled_connection().unwrap();
        let count: i32 = sql::<Integer>("SELECT COUNT(*) FROM test_table")
            .get_result(&mut c)
            .unwrap();
        assert_eq!(count as usize, WRITERS);
    }
}
