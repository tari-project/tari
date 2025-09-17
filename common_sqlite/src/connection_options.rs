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

use diesel::{connection::SimpleConnection, dsl::sql, sql_types::Text, RunQueryDsl, SqliteConnection};
use log::trace;

use crate::connection::DbConnection;

const LOG_TARGET: &str = "common_sqlite::connection_options";

/// A default busy timeout for SQLite pool connection to throw a 'database is locked' error.
pub const PRAGMA_BUSY_TIMEOUT: Duration = Duration::from_secs(60);

/// Connection-level options that are applied to **every** SQLite connection
/// as it is acquired from the r2d2 pool (via the `CustomizeConnection` hook).
///
/// # Timeout interplay
/// `busy_timeout` controls how long a **checked-out** connection will wait
/// *inside SQLite* when it encounters a lock (e.g. another writer). This is
/// independent of the r2d2 **pool checkout** timeout
/// (`Pool::builder().connection_timeout(...)`).
///
/// To avoid spurious “timed out waiting for connection” pool errors under
/// write contention, prefer `connection_timeout >= busy_timeout + small buffer`,
/// or keep `busy_timeout` lower and implement retries with backoff.
#[derive(Debug, Clone)]
pub struct ConnectionOptions {
    /// If `true`, enables Write-Ahead Logging and sets
    /// `PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;`
    /// for a good durability/throughput trade-off.
    enable_wal: bool,
    /// If `true`, enforces SQLite foreign key constraints
    /// (`PRAGMA foreign_keys = ON;`).
    enable_foreign_keys: bool,
    /// How long SQLite should wait for locks before returning `SQLITE_BUSY`.
    /// When `None`, no busy timeout is configured.
    busy_timeout: Option<Duration>,
}

impl ConnectionOptions {
    /// Construct a new set of connection options.
    ///
    /// * `enable_wal` — enable WAL + `synchronous=NORMAL`
    /// * `enable_foreign_keys` — enforce FK constraints
    /// * `busy_timeout` — SQLite lock wait duration
    pub fn new(enable_wal: bool, enable_foreign_keys: bool, busy_timeout: Duration) -> Self {
        Self {
            enable_wal,
            enable_foreign_keys,
            busy_timeout: Some(busy_timeout),
        }
    }

    /// Returns the busy timeout duration, if set.
    pub fn get_busy_timeout(&self) -> Option<Duration> {
        self.busy_timeout
    }
}

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ConnectionOptions {
    /// Applies PRAGMAs on each acquired connection:
    /// - `PRAGMA busy_timeout`
    /// - `PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;` (if enabled)
    /// - `PRAGMA foreign_keys = ON;` (if enabled)
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        (|| {
            let start = std::time::Instant::now();
            if let Some(d) = self.busy_timeout {
                conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
            }

            if self.enable_wal {
                // Read current mode (cheap, read-only)
                let current: String = sql::<Text>("PRAGMA journal_mode;").get_result(conn)?;

                if current.eq_ignore_ascii_case("wal") {
                    // Already WAL: just ensure the sync level
                    conn.batch_execute("PRAGMA synchronous = NORMAL;")?;
                } else if DbConnection::migration_lock_active() {
                    // Only attempt the WAL flip when we know we hold the process-wide write lock.
                    // This avoids racy WAL changes from concurrent connection acquisitions.
                    conn.batch_execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
                } else {
                    // Not WAL and no migration lock: do nothing (avoid SQLITE_BUSY on startup).
                    // Next time on_acquire runs under the lock (during migrate), it will flip.
                }
            }

            if self.enable_foreign_keys {
                conn.batch_execute("PRAGMA foreign_keys = ON;")?;
            }

            trace!(target: LOG_TARGET, "Applied SQLite PRAGMAs on_acquire in {:.2?}", start.elapsed());
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}
