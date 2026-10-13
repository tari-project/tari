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

use core::time::Duration;
use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

use diesel::{
    Connection,
    RunQueryDsl,
    SqliteConnection,
    connection::SimpleConnection,
    dsl::sql,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    sql_types::{Integer, Text},
};
use log::*;

use crate::{
    connection_options::{ConnectionOptions, DbProfile},
    error::SqliteStorageError,
};

const LOG_TARGET: &str = "common_sqlite::sqlite_connection_pool";

/// The default timeout for acquiring an R2D2 pool connection over and above the PRAGMA busy timeout.
pub const R2D2_POOL_CONNECTION_DELTA: Duration = Duration::from_secs(5);
/// The default timeout for acquiring an R2D2 pool connection.
/// Note: The default R2D2 connection timeout is 30s.
pub const R2D2_POOL_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);
/// The largest pool [`SqliteConnectionPool::new`] accepts. SQLite has a single writer, so connections beyond this only
/// queue on its lock.
pub const MAX_POOL_SIZE: usize = 64;

/// Thin wrapper around an r2d2 `Pool<SqliteConnection>` with standard SQLite
/// configuration (WAL, profile pragmas, busy timeout) applied to each connection.
///
/// The pool is created lazily by calling [`create_pool`] once. After that,
/// callers can obtain connections via [`get_pooled_connection`], a timed
/// variant, or a non-blocking `try_get_pooled_connection`.
///
/// For a file-backed database [`create_pool`] first restricts the file and its directory to the owner and switches
/// the database to WAL; for an in-memory database it pins the pool's connections so none is ever reaped (each
/// `:memory:` connection is its own database, and a reaped one would take the schema with it).
///
/// # Concurrency notes
/// SQLite allows only a single writer at a time (even in WAL mode). Keep write
/// transactions short and consider limiting concurrent writers at the
/// application layer (e.g. a semaphore) to reduce lock contention.
///
/// # Timeout interplay
/// The r2d2 **pool checkout** timeout is configured via
/// `Pool::builder().connection_timeout(...)` inside [`create_pool`]. This is
/// separate from SQLite’s `PRAGMA busy_timeout` set by [`ConnectionOptions`].
/// Prefer `connection_timeout >= busy_timeout` (plus a little headroom) to
/// avoid premature pool timeouts while connections are waiting on SQLite locks.
#[derive(Clone)]
pub struct SqliteConnectionPool {
    /// The underlying r2d2 pool. `None` until [`create_pool`] is called.
    pool: Option<Pool<ConnectionManager<SqliteConnection>>>,
    /// Database path / connection string (`:memory:`, filesystem path, or
    /// `file:NAME?mode=memory&cache=shared`).
    db_path: String,
    /// Maximum number of concurrently open connections managed by the pool.
    pool_size: usize,
    /// Per-connection SQLite options applied on acquisition.
    connection_options: ConnectionOptions,
}

impl SqliteConnectionPool {
    /// Create a wrapper with the given target database, pool size and profile. The profile fixes the pragmas every
    /// connection gets and the default busy timeout (see [`Self::with_busy_timeout`]). The r2d2 pool is not built
    /// until [`create_pool`] is called.
    ///
    /// Returns [`SqliteStorageError::PoolSize`] unless `1 <= pool_size <= MAX_POOL_SIZE`.
    pub fn new(db_path: String, pool_size: usize, profile: DbProfile) -> Result<Self, SqliteStorageError> {
        if pool_size == 0 || pool_size > MAX_POOL_SIZE {
            return Err(SqliteStorageError::PoolSize(pool_size));
        }
        Ok(Self {
            pool: None,
            db_path,
            pool_size,
            connection_options: ConnectionOptions::from_profile(profile),
        })
    }

    /// Override the profile's default `PRAGMA busy_timeout`. Call before [`create_pool`].
    pub fn with_busy_timeout(mut self, busy_timeout: Duration) -> Self {
        self.connection_options.set_busy_timeout(busy_timeout);
        self
    }

    /// True for `:memory:` and for shared-cache in-memory URIs (`file:NAME?mode=memory&cache=shared`).
    fn is_in_memory(&self) -> bool {
        self.db_path == ":memory:" || self.db_path.contains("mode=memory")
    }

    /// Create an sqlite connection pool managed by the pool connection manager
    pub fn create_pool(&mut self) -> Result<(), SqliteStorageError> {
        if self.pool.is_some() {
            warn!(target: LOG_TARGET, "Connection pool for {} already exists", self.db_path);
            return Ok(());
        }

        if self.db_path == ":memory:" && self.pool_size > 1 {
            return Err(SqliteStorageError::InMemoryPoolSize(self.pool_size));
        }

        let max_size = u32::try_from(self.pool_size).map_err(|_| SqliteStorageError::PoolSize(self.pool_size))?;
        let busy_timeout = self.connection_options.get_busy_timeout();
        if !self.is_in_memory() {
            prepare_database_path(Path::new(&self.db_path))?;
            switch_to_wal(&self.db_path, busy_timeout)?;
        }

        // When we get a pooled connection, we want the pool connection timeout to be longer than the database busy
        // timeout (set by `PRAGMA busy_timeout`). Here we set the connection timeout to whatever the busy timeout is
        // plus a delta (5s).
        let mut builder = Pool::builder()
            .max_size(max_size)
            .connection_customizer(Box::new(self.connection_options.clone()))
            .connection_timeout(busy_timeout.saturating_add(R2D2_POOL_CONNECTION_DELTA));
        if self.is_in_memory() {
            // In-memory connections are never reaped or lazily created: each `:memory:` connection is its own
            // database, and a shared-cache in-memory database is dropped with its last connection, so a reaped
            // connection takes the schema with it. File-backed pools keep r2d2's defaults.
            builder = builder.max_lifetime(None).idle_timeout(None).min_idle(Some(max_size));
        }
        let pool = builder
            .build(ConnectionManager::<SqliteConnection>::new(self.db_path.as_str()))
            .map_err(|e| SqliteStorageError::DieselR2d2Error(e.to_string()))?;
        self.pool = Some(pool);
        Ok(())
    }

    /// Return a pooled sqlite connection managed by the pool connection manager, waits for at most the configured
    /// connection timeout before returning an error.
    pub fn get_pooled_connection(
        &self,
    ) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, SqliteStorageError> {
        if let Some(pool) = self.pool.as_ref() {
            let start = std::time::Instant::now();
            let connection = pool.get().map_err(|e| {
                warn!(target: LOG_TARGET, "Connection pool state {:?}: {}", pool.state(), e);
                SqliteStorageError::DieselR2d2Error(e.to_string())
            });
            let timing = start.elapsed();
            if timing > Duration::from_millis(100) {
                debug!(target: LOG_TARGET, "Acquired 'get_pooled_connection' from pool in {:.2?}", timing);
            }
            connection
        } else {
            Err(SqliteStorageError::DieselR2d2Error("Pool does not exist".to_string()))
        }
    }

    /// Return a pooled sqlite connection managed by the pool connection manager, waits for at most supplied
    /// connection timeout before returning an error.
    pub fn get_pooled_connection_timeout(
        &self,
        timeout: Duration,
    ) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, SqliteStorageError> {
        if let Some(pool) = self.pool.clone() {
            let start = std::time::Instant::now();
            let connection = pool.get_timeout(timeout).map_err(|e| {
                warn!(target: LOG_TARGET, "Connection pool state {:?}: {}", pool.state(), e);
                SqliteStorageError::DieselR2d2Error(e.to_string())
            });
            let timing = start.elapsed();
            if timing > Duration::from_millis(100) {
                debug!(target: LOG_TARGET, "Acquired 'get_pooled_connection_timeout' from pool in {:.2?}", timing);
            }
            connection
        } else {
            Err(SqliteStorageError::DieselR2d2Error("Pool does not exist".to_string()))
        }
    }

    /// Return a pooled sqlite connection managed by the pool connection manager, returns None if there are no idle
    /// connections available in the pool. This method will not block waiting to establish a new connection.
    pub fn try_get_pooled_connection(
        &self,
    ) -> Result<Option<PooledConnection<ConnectionManager<SqliteConnection>>>, SqliteStorageError> {
        if let Some(pool) = self.pool.clone() {
            let start = std::time::Instant::now();
            let connection = pool.try_get();
            if connection.is_none() {
                warn!(target: LOG_TARGET, "No connections available, pool state {:?}", pool.state());
            } else {
                let timing = start.elapsed();
                if timing > Duration::from_millis(100) {
                    debug!(target: LOG_TARGET, "Acquired 'try_get_pooled_connection' from pool in {:.2?}", timing);
                }
            }
            Ok(connection)
        } else {
            Err(SqliteStorageError::DieselR2d2Error("Pool does not exist".to_string()))
        }
    }

    /// The built r2d2 pool, so tests can check how it was configured.
    #[cfg(test)]
    pub(crate) fn r2d2_pool(&self) -> Option<&Pool<ConnectionManager<SqliteConnection>>> {
        self.pool.as_ref()
    }

    /// Returns the database path / connection string used by this pool.
    pub fn db_path(&self) -> PathBuf {
        PathBuf::from(&self.db_path)
    }

    /// Perform cleanup on the connection pool. This will drop the pool and return the state of the pool.
    ///
    /// For a [`DbProfile::KeyMaterial`] file database, first runs a best-effort `PRAGMA wal_checkpoint(TRUNCATE)` so
    /// the `-wal` file does not keep copies of freed pages after the database is closed. The checkpoint never waits:
    /// if no connection is idle or another connection holds the database, it is skipped and logged at debug. Safe to
    /// call from `Drop`.
    pub fn cleanup(&mut self) -> Option<String> {
        if let Some(pool) = self.pool.take() {
            let state = format!("{:?}", pool.state());
            if self.connection_options.profile() == DbProfile::KeyMaterial && !self.is_in_memory() {
                truncate_wal(&pool, &self.db_path);
            }
            drop(pool);
            return Some(state);
        }
        None
    }
}

/// Best-effort `PRAGMA wal_checkpoint(TRUNCATE)` on an idle connection of `pool`, without waiting on any lock.
fn truncate_wal(pool: &Pool<ConnectionManager<SqliteConnection>>, db_path: &str) {
    let Some(mut conn) = pool.try_get() else {
        debug!(target: LOG_TARGET, "No idle connection to checkpoint {} on cleanup", db_path);
        return;
    };
    // This runs while the database is being closed, often from `Drop`, so it must not sit out the busy timeout.
    let result = conn.batch_execute("PRAGMA busy_timeout = 0;").and_then(|_| {
        sql::<(Integer, Integer, Integer)>("PRAGMA wal_checkpoint(TRUNCATE);").get_result::<(i32, i32, i32)>(&mut conn)
    });
    match result {
        Ok((0, _, _)) => {
            debug!(target: LOG_TARGET, "Truncated the WAL of {} on cleanup", db_path);
        },
        Ok((_, wal_frames, checkpointed)) => {
            debug!(
                target: LOG_TARGET,
                "WAL checkpoint of {} on cleanup was blocked by another connection ({} of {} frames checkpointed)",
                db_path,
                checkpointed,
                wal_frames
            );
        },
        Err(e) => {
            debug!(target: LOG_TARGET, "WAL checkpoint on cleanup of {} failed: {}", db_path, e);
        },
    }
}

/// Create the database's directory and file owner-only, and remove group and other permission bits from a file or
/// sidecar that already exists: a co-tenant with only read access to a SQLite file can still take a shared lock on it
/// and stall every write. An existing parent directory is only restricted when it is safe to (see
/// [`restrict_existing_dir`]). Called for file-backed databases before the first connection is opened.
#[cfg(unix)]
fn prepare_database_path(path: &Path) -> Result<(), SqliteStorageError> {
    use std::{
        fs::{DirBuilder, OpenOptions},
        os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    };

    let permissions_error = |p: &Path, source| SqliteStorageError::FilePermissions {
        path: p.to_path_buf(),
        source,
    };

    if let Some(parent) = path.parent() &&
        !parent.as_os_str().is_empty()
    {
        if parent.exists() {
            restrict_existing_dir(parent)?;
        } else {
            DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(|e| permissions_error(parent, e))?;
        }
    }

    // No O_EXCL: the file normally exists already, and then `mode` is ignored, which is why it is restricted below.
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|e| permissions_error(path, e))?;
    remove_group_other_bits(path)?;

    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_owned();
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        if sidecar.exists() {
            remove_group_other_bits(&sidecar)?;
        }
    }
    Ok(())
}

/// Remove group and other permission bits from an existing parent directory, but only from one owned by the effective
/// user that has no sticky bit. A directory owned by someone else cannot be changed (and may be shared on purpose,
/// e.g. a data directory other users read the chain database from), and a sticky directory such as `/tmp` is shared
/// by design; those are logged and left alone, since the 0600 database file still protects the data.
#[cfg(unix)]
fn restrict_existing_dir(dir: &Path) -> Result<(), SqliteStorageError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(dir).map_err(|source| SqliteStorageError::FilePermissions {
        path: dir.to_path_buf(),
        source,
    })?;
    let mode = metadata.mode() & 0o7777;
    if mode & 0o077 == 0 {
        return Ok(());
    }
    // SAFETY: geteuid has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    if metadata.uid() != euid {
        warn!(
            target: LOG_TARGET,
            "Database directory '{}' has mode {:o} and is owned by uid {}, not this process (uid {}); leaving its \
             permissions unchanged",
            dir.display(),
            mode,
            metadata.uid(),
            euid
        );
        return Ok(());
    }
    if mode & 0o1000 != 0 {
        warn!(
            target: LOG_TARGET,
            "Database directory '{}' has mode {:o} with the sticky bit set (a shared directory); leaving its \
             permissions unchanged",
            dir.display(),
            mode
        );
        return Ok(());
    }
    remove_group_other_bits(dir)
}

/// Clear the group and other permission bits of `path`, keeping every other bit as it is.
#[cfg(unix)]
fn remove_group_other_bits(path: &Path) -> Result<(), SqliteStorageError> {
    use std::os::unix::fs::PermissionsExt;

    let permissions_error = |source| SqliteStorageError::FilePermissions {
        path: path.to_path_buf(),
        source,
    };
    let current = fs::metadata(path).map_err(permissions_error)?.permissions().mode() & 0o7777;
    if current & 0o077 != 0 {
        let restricted = current & !0o077;
        fs::set_permissions(path, fs::Permissions::from_mode(restricted)).map_err(permissions_error)?;
        info!(
            target: LOG_TARGET,
            "Restricted permissions of '{}' from {:o} to {:o}",
            path.display(),
            current,
            restricted
        );
    }
    Ok(())
}

/// Create the database's directory and file. Windows has no Unix modes to set.
#[cfg(not(unix))]
fn prepare_database_path(path: &Path) -> Result<(), SqliteStorageError> {
    let permissions_error = |p: &Path, source| SqliteStorageError::FilePermissions {
        path: p.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent() &&
        !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| permissions_error(parent, e))?;
    }
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| permissions_error(path, e))?;
    Ok(())
}

/// Switch a file database to WAL on a connection of its own, before the pool opens any. The journal mode is stored
/// in the file, so this is a no-op for a database that is already WAL. Converting a rollback-journal database needs
/// every other connection to the file closed; if one holds it past `busy_timeout`, this fails rather than leaving the
/// database in rollback mode, where a reader can block writers.
fn switch_to_wal(db_path: &str, busy_timeout: Duration) -> Result<(), SqliteStorageError> {
    let journal_mode_error = |actual: String| SqliteStorageError::JournalMode {
        expected: "wal",
        actual,
    };

    let mut conn = SqliteConnection::establish(db_path)?;
    conn.batch_execute(&format!("PRAGMA busy_timeout = {};", busy_timeout.as_millis()))
        .map_err(|e| journal_mode_error(e.to_string()))?;
    let mode: String = sql::<Text>("PRAGMA journal_mode = WAL;")
        .get_result(&mut conn)
        .map_err(|e| journal_mode_error(e.to_string()))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(journal_mode_error(mode));
    }
    Ok(())
}

impl Display for SqliteConnectionPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pool_state = if let Some(pool) = self.pool.clone() {
            format!("{:?}", pool.state())
        } else {
            "None".to_string()
        };
        write!(
            f,
            "SqliteConnectionPool {{ pool state: {}, db_path: {}, pool_size: {}, connection_options: {:?} }}",
            pool_state, self.db_path, self.pool_size, self.connection_options
        )
    }
}

/// Helper trait for components that need a pooled SQLite connection.
pub trait PooledDbConnection: Send + Sync + Clone {
    /// Acquire a pooled connection, or return an error if the pool is
    /// unavailable or the checkout times out.
    type Error;

    fn get_pooled_connection(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, Self::Error>;
}

#[cfg(test)]
mod test {
    use diesel::sql_types::BigInt;

    use super::*;

    fn temp_db_path(dir: &tempfile::TempDir, name: &str) -> String {
        dir.path().join(name).to_str().unwrap().to_string()
    }

    fn query_text(conn: &mut SqliteConnection, query: &str) -> String {
        sql::<Text>(query).get_result(conn).unwrap()
    }

    fn query_int(conn: &mut SqliteConnection, query: &str) -> i64 {
        sql::<BigInt>(query).get_result(conn).unwrap()
    }

    /// Regression: WAL used to be switched on only while some database in the process held the migration lock, so a
    /// wallet (which migrates without it) stayed in rollback-journal mode, where a reader blocks every writer.
    #[test]
    fn create_pool_sets_wal_without_migration_lock() {
        let dir = tempfile::tempdir().unwrap();
        let mut pool = SqliteConnectionPool::new(temp_db_path(&dir, "wallet.db"), 4, DbProfile::KeyMaterial).unwrap();
        pool.create_pool().unwrap();
        let mut conn = pool.get_pooled_connection().unwrap();
        assert_eq!(query_text(&mut conn, "PRAGMA journal_mode;").to_lowercase(), "wal");
    }

    #[test]
    fn create_pool_fails_when_wal_switch_is_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_db_path(&dir, "held.db");
        let mut holder = SqliteConnection::establish(&path).unwrap();
        holder
            .batch_execute("CREATE TABLE t (id INTEGER); BEGIN EXCLUSIVE; INSERT INTO t VALUES (1);")
            .unwrap();
        assert_eq!(query_text(&mut holder, "PRAGMA journal_mode;").to_lowercase(), "delete");

        let mut pool = SqliteConnectionPool::new(path, 1, DbProfile::KeyMaterial)
            .unwrap()
            .with_busy_timeout(Duration::from_millis(200));
        let err = pool.create_pool().unwrap_err();
        assert!(
            matches!(err, SqliteStorageError::JournalMode { expected: "wal", .. }),
            "unexpected error: {err:?}"
        );
        holder.batch_execute("COMMIT;").unwrap();
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[cfg(unix)]
    #[test]
    fn create_pool_tightens_modes_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        let db = dir.join("wallet.db");
        let wal = dir.join("wallet.db-wal");
        for file in [&db, &wal] {
            fs::write(file, b"").unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o644)).unwrap();
        }

        let mut pool = SqliteConnectionPool::new(db.to_str().unwrap().to_string(), 1, DbProfile::KeyMaterial).unwrap();
        pool.create_pool().unwrap();

        assert_eq!(mode_of(&dir), 0o700);
        assert_eq!(mode_of(&db), 0o600);
        assert_eq!(mode_of(&wal), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn create_pool_creates_owner_only_paths_on_unix() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("a").join("b");
        let db = dir.join("peers.db");

        let mut pool = SqliteConnectionPool::new(db.to_str().unwrap().to_string(), 2, DbProfile::Cache).unwrap();
        pool.create_pool().unwrap();
        // Write through a pooled connection, which leaves the WAL open with the pool.
        let mut conn = pool.get_pooled_connection().unwrap();
        conn.batch_execute("CREATE TABLE t (id INTEGER);").unwrap();

        assert_eq!(mode_of(&root.path().join("a")), 0o700);
        assert_eq!(mode_of(&dir), 0o700);
        assert_eq!(mode_of(&db), 0o600);
        // SQLite gives the sidecars the main file's mode.
        assert_eq!(mode_of(&dir.join("peers.db-wal")), 0o600);
    }

    /// An existing directory owned by this process loses its group and other bits; files keep their owner bits.
    #[cfg(unix)]
    #[test]
    fn create_pool_tightens_owned_parent_dir_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        let db = dir.join("wallet.db");
        fs::write(&db, b"").unwrap();
        // Only the group and other bits go; the owner's (unusual) execute bit stays.
        fs::set_permissions(&db, fs::Permissions::from_mode(0o764)).unwrap();

        let mut pool = SqliteConnectionPool::new(db.to_str().unwrap().to_string(), 1, DbProfile::KeyMaterial).unwrap();
        pool.create_pool().unwrap();

        assert_eq!(mode_of(&dir), 0o700);
        assert_eq!(mode_of(&db), 0o700);
    }

    /// A sticky directory such as `/tmp` is shared by design: it is left as it is, and the database still opens.
    #[cfg(unix)]
    #[test]
    fn create_pool_leaves_sticky_parent_dir_unchanged_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("shared");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o1777)).unwrap();
        let db = dir.join("peers.db");

        let mut pool = SqliteConnectionPool::new(db.to_str().unwrap().to_string(), 1, DbProfile::Cache).unwrap();
        pool.create_pool().unwrap();

        assert_eq!(mode_of(&dir), 0o1777);
        assert_eq!(mode_of(&db), 0o600);
    }

    #[test]
    fn memory_url_rejects_pool_above_one() {
        let mut pool = SqliteConnectionPool::new(":memory:".to_string(), 2, DbProfile::Cache).unwrap();
        let err = pool.create_pool().unwrap_err();
        assert!(
            matches!(err, SqliteStorageError::InMemoryPoolSize(2)),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn profile_pragmas_applied() {
        // (profile, secure_delete, synchronous, journal_size_limit, busy_timeout ms, foreign_keys)
        let expected = [
            (DbProfile::KeyMaterial, 1, 2, 4_194_304, 60_000, 1),
            (DbProfile::Cache, 0, 1, 67_108_864, 10_000, 1),
        ];
        for (profile, secure_delete, synchronous, journal_size_limit, busy_timeout, foreign_keys) in expected {
            let dir = tempfile::tempdir().unwrap();
            let mut pool = SqliteConnectionPool::new(temp_db_path(&dir, "profile.db"), 1, profile).unwrap();
            pool.create_pool().unwrap();
            let mut conn = pool.get_pooled_connection().unwrap();
            assert_eq!(
                query_int(&mut conn, "PRAGMA secure_delete;"),
                secure_delete,
                "{profile:?}"
            );
            assert_eq!(query_int(&mut conn, "PRAGMA synchronous;"), synchronous, "{profile:?}");
            assert_eq!(
                query_int(&mut conn, "PRAGMA journal_size_limit;"),
                journal_size_limit,
                "{profile:?}"
            );
            assert_eq!(
                query_int(&mut conn, "PRAGMA busy_timeout;"),
                busy_timeout,
                "{profile:?}"
            );
            assert_eq!(
                query_int(&mut conn, "PRAGMA foreign_keys;"),
                foreign_keys,
                "{profile:?}"
            );
        }
    }

    #[test]
    fn pool_size_zero_is_error() {
        let result = SqliteConnectionPool::new(":memory:".to_string(), 0, DbProfile::Cache);
        assert!(matches!(result, Err(SqliteStorageError::PoolSize(0))));
    }

    #[test]
    fn pool_size_above_cap_is_error() {
        let result = SqliteConnectionPool::new(":memory:".to_string(), MAX_POOL_SIZE + 1, DbProfile::Cache);
        assert!(matches!(result, Err(SqliteStorageError::PoolSize(65))));
        assert!(SqliteConnectionPool::new(":memory:".to_string(), MAX_POOL_SIZE, DbProfile::Cache).is_ok());
    }
}
