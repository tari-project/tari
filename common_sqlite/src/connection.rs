// Copyright 2020. The Taiji Project
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
    path::{Path, PathBuf},
    time::Duration,
};

use diesel::{
    r2d2::{ConnectionManager, PooledConnection},
    SqliteConnection,
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use log::*;
use serde::{Deserialize, Serialize};

use crate::{
    error::{SqliteStorageError, StorageError},
    sqlite_connection_pool::{PooledDbConnection, SqliteConnectionPool},
};

const LOG_TARGET: &str = "common_sqlite::connection";
const SQLITE_POOL_SIZE: usize = 16;

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

/// A SQLite database connection
#[derive(Clone)]
pub struct DbConnection {
    pool: SqliteConnectionPool,
}

impl DbConnection {
    /// Connect to an ephemeral database in memory
    pub fn connect_memory(name: String) -> Result<Self, StorageError> {
        Self::connect_url(&DbConnectionUrl::MemoryShared(name))
    }

    /// Connect using the given [DbConnectionUrl](self::DbConnectionUrl).
    pub fn connect_url(db_url: &DbConnectionUrl) -> Result<Self, StorageError> {
        debug!(target: LOG_TARGET, "Connecting to database using '{:?}'", db_url);

        let mut pool = SqliteConnectionPool::new(
            db_url.to_url_string(),
            SQLITE_POOL_SIZE,
            true,
            true,
            Duration::from_secs(60),
        );
        pool.create_pool()?;

        Ok(Self::new(pool))
    }

    /// Connect and migrate the database, once complete, a handle to the migrated database is returned.
    pub fn connect_and_migrate(db_url: &DbConnectionUrl, migrations: EmbeddedMigrations) -> Result<Self, StorageError> {
        let conn = Self::connect_url(db_url)?;
        let output = conn.migrate(migrations)?;
        debug!(target: LOG_TARGET, "DHT database migration: {}", output.trim());
        Ok(conn)
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
    use taiji_test_utils::random;

    use super::*;

    #[tokio::test]
    async fn connect_and_migrate() {
        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./test/migrations");

        let conn = DbConnection::connect_memory(random::string(8)).unwrap();
        let output = conn.migrate(MIGRATIONS).unwrap();
        assert!(output.starts_with("Running migration"));
    }

    #[tokio::test]
    async fn memory_connections() {
        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./test/migrations");

        let id = random::string(8);
        let conn = DbConnection::connect_memory(id.clone()).unwrap();
        conn.migrate(MIGRATIONS).unwrap();
        let conn = DbConnection::connect_memory(id).unwrap();
        let mut conn = conn.get_pooled_connection().unwrap();
        let count: i32 = sql::<Integer>("SELECT COUNT(*) FROM test_table")
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(count, 0);
    }
}
