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

use diesel::{SqliteConnection, connection::SimpleConnection};
use log::trace;

const LOG_TARGET: &str = "common_sqlite::connection_options";

/// The default busy timeout for a [`DbProfile::KeyMaterial`] database: how long a connection waits on a lock before
/// SQLite returns 'database is locked'.
pub const PRAGMA_BUSY_TIMEOUT: Duration = Duration::from_secs(60);

/// The default busy timeout for a [`DbProfile::Cache`] database. Caches sit on hot networking paths, where a call that
/// waits a minute for a lock has long outlived the request that made it.
pub const PRAGMA_BUSY_TIMEOUT_CACHE: Duration = Duration::from_secs(10);

/// What a database holds, which decides the pragmas every connection to it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbProfile {
    /// Wallet and key-manager databases: durability and residue control over throughput.
    KeyMaterial,
    /// Peer, DHT and other caches: throughput over durability; nothing secret inside.
    Cache,
}

impl DbProfile {
    /// The `PRAGMA busy_timeout` used unless the caller overrides it.
    pub fn default_busy_timeout(self) -> Duration {
        match self {
            DbProfile::KeyMaterial => PRAGMA_BUSY_TIMEOUT,
            DbProfile::Cache => PRAGMA_BUSY_TIMEOUT_CACHE,
        }
    }
}

/// Connection-level options that are applied to **every** SQLite connection
/// as it is acquired from the r2d2 pool (via the `CustomizeConnection` hook).
///
/// The journal mode is not set here: `SqliteConnectionPool::create_pool` switches a file-backed database to WAL once,
/// before the pool is built, because the mode is persistent and a per-connection switch races other connections.
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
    /// Which pragma set to apply.
    profile: DbProfile,
    /// How long SQLite should wait for locks before returning `SQLITE_BUSY`.
    busy_timeout: Duration,
}

impl ConnectionOptions {
    /// The options for `profile`, with the profile's default busy timeout.
    pub fn from_profile(profile: DbProfile) -> Self {
        Self {
            profile,
            busy_timeout: profile.default_busy_timeout(),
        }
    }

    /// Override the profile's default busy timeout.
    pub fn set_busy_timeout(&mut self, busy_timeout: Duration) {
        self.busy_timeout = busy_timeout;
    }

    /// Returns the busy timeout duration.
    pub fn get_busy_timeout(&self) -> Duration {
        self.busy_timeout
    }

    /// Returns the profile these options were built from.
    pub fn profile(&self) -> DbProfile {
        self.profile
    }
}

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ConnectionOptions {
    /// Applies the profile's PRAGMAs on each new connection:
    ///
    /// | Pragma               | `KeyMaterial` | `Cache`  |
    /// |----------------------|---------------|----------|
    /// | `busy_timeout`       | 60 s default  | 10 s default |
    /// | `foreign_keys`       | ON            | ON       |
    /// | `secure_delete`      | ON            | OFF      |
    /// | `synchronous`        | FULL          | NORMAL   |
    /// | `journal_size_limit` | 4 MiB         | 64 MiB   |
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        let start = std::time::Instant::now();
        let pragmas = match self.profile {
            // `secure_delete` overwrites freed pages in the main database file, so a deleted row's plaintext is not
            // left legible in a free page. It does not reach the `-wal` or `-journal` files: residue there is only
            // bounded, by `journal_size_limit` and by a best-effort TRUNCATE checkpoint when the pool is cleaned up
            // (`SqliteConnectionPool::cleanup`; the wallet does this when its last database handle is dropped). That
            // checkpoint is skipped if another connection holds the database at that moment.
            DbProfile::KeyMaterial => {
                "PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON; PRAGMA synchronous = FULL; PRAGMA \
                 journal_size_limit = 4194304;"
            },
            DbProfile::Cache => {
                "PRAGMA foreign_keys = ON; PRAGMA secure_delete = OFF; PRAGMA synchronous = NORMAL; PRAGMA \
                 journal_size_limit = 67108864;"
            },
        };
        conn.batch_execute(&format!(
            "PRAGMA busy_timeout = {}; {}",
            self.busy_timeout.as_millis(),
            pragmas
        ))
        .map_err(diesel::r2d2::Error::QueryError)?;

        trace!(target: LOG_TARGET, "Applied SQLite PRAGMAs on_acquire in {:.2?}", start.elapsed());
        Ok(())
    }
}
