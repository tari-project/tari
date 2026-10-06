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
use std::{convert::TryFrom, fmt::Display, path::PathBuf};

use diesel::{
    SqliteConnection,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use log::*;

use crate::{connection_options::ConnectionOptions, error::SqliteStorageError};

const LOG_TARGET: &str = "common_sqlite::sqlite_connection_pool";

/// The default timeout for acquiring an R2D2 pool connection over and above the PRAGMA busy timeout.
pub const R2D2_POOL_CONNECTION_DELTA: Duration = Duration::from_secs(5);
/// The default timeout for acquiring an R2D2 pool connection.
/// Note: The default R2D2 connection timeout is 30s.
pub const R2D2_POOL_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Thin wrapper around an r2d2 `Pool<SqliteConnection>` with standard SQLite
/// configuration (WAL, FKs, busy timeout) applied to each connection.
///
/// The pool is created lazily by calling [`create_pool`] once. After that,
/// callers can obtain connections via [`get_pooled_connection`], a timed
/// variant, or a non-blocking `try_get_pooled_connection`.
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
    /// Create a wrapper with the given target database, pool size and
    /// connection options (WAL, FKs, busy timeout). The r2d2 pool is not built
    /// until [`create_pool`] is called.
    pub fn new(
        db_path: String,
        pool_size: usize,
        enable_wal: bool,
        enable_foreign_keys: bool,
        busy_timeout: Duration,
    ) -> Self {
        Self {
            pool: None,
            db_path,
            pool_size,
            connection_options: ConnectionOptions::new(enable_wal, enable_foreign_keys, busy_timeout),
        }
    }

    /// Create an sqlite connection pool managed by the pool connection manager
    pub fn create_pool(&mut self) -> Result<(), SqliteStorageError> {
        if self.pool.is_none() {
            let mut builder = Pool::builder()
                .max_size(u32::try_from(self.pool_size)?)
                .connection_customizer(Box::new(self.connection_options.clone()));
            if let Some(timeout) = self.connection_options.get_busy_timeout() {
                // When we get a pooled connection, we want the pool connection timeout to be longer
                // than the database busy timeout (set by `PRAGMA busy_timeout`). Here we set the
                // connection timeout to whatever the busy timeout is plus a delta (5s).
                builder = builder.connection_timeout(timeout.saturating_add(R2D2_POOL_CONNECTION_DELTA));
            } else {
                // If no busy timeout is set, we use the default value.
                builder = builder.connection_timeout(R2D2_POOL_CONNECTION_TIMEOUT);
            }
            let pool = builder
                .build(ConnectionManager::<SqliteConnection>::new(self.db_path.as_str()))
                .map_err(|e| SqliteStorageError::DieselR2d2Error(e.to_string()))?;
            self.pool = Some(pool);
        } else {
            warn!(target: LOG_TARGET, "Connection pool for {} already exists", self.db_path);
        }
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

    /// Returns the database path / connection string used by this pool.
    pub fn db_path(&self) -> PathBuf {
        PathBuf::from(&self.db_path)
    }

    /// Perform cleanup on the connection pool. This will drop the pool and return the state of the pool.
    pub fn cleanup(&mut self) -> Option<String> {
        if let Some(pool) = self.pool.take() {
            let state = format!("{:?}", pool.state());
            drop(pool);
            return Some(state);
        }
        None
    }
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
