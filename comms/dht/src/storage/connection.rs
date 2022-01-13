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

use std::{io, path::PathBuf, time::Duration};

use diesel::{
    r2d2::{ConnectionManager, PooledConnection},
    SqliteConnection,
};
use log::*;
use tari_common_sqlite::sqlite_connection_pool::SqliteConnectionPool;

use crate::storage::error::StorageError;

const LOG_TARGET: &str = "comms::dht::storage::connection";
const SQLITE_POOL_SIZE: usize = 16;

#[derive(Clone, Debug)]
pub enum DbConnectionUrl {
    /// In-memory database. Each connection has it's own database
    Memory,
    /// In-memory database shared with more than one in-process connection according to the given identifier
    MemoryShared(String),
    /// Database persisted on disk
    File(PathBuf),
}

impl DbConnectionUrl {
    pub fn to_url_string(&self) -> String {
        use DbConnectionUrl::*;
        match self {
            Memory => ":memory:".to_owned(),
            MemoryShared(identifier) => format!("file:{}?mode=memory&cache=shared", identifier),
            File(path) => path
                .to_str()
                .expect("Invalid non-UTF8 character in database path")
                .to_owned(),
        }
    }
}

#[derive(Clone)]
pub struct DbConnection {
    pool: SqliteConnectionPool,
}

impl DbConnection {
    #[cfg(test)]
    pub fn connect_memory(name: String) -> Result<Self, StorageError> {
        Self::connect_url(DbConnectionUrl::MemoryShared(name))
    }

    pub fn connect_url(db_url: DbConnectionUrl) -> Result<Self, StorageError> {
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

    pub fn connect_and_migrate(db_url: DbConnectionUrl) -> Result<Self, StorageError> {
        let conn = Self::connect_url(db_url)?;
        let output = conn.migrate()?;
        debug!(target: LOG_TARGET, "DHT database migration: {}", output.trim());
        Ok(conn)
    }

    fn new(pool: SqliteConnectionPool) -> Self {
        Self { pool }
    }

    pub fn get_pooled_connection(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, StorageError> {
        self.pool.get_pooled_connection().map_err(StorageError::DieselR2d2Error)
    }

    pub fn migrate(&self) -> Result<String, StorageError> {
        embed_migrations!("./migrations");

        let mut buf = io::Cursor::new(Vec::new());
        let conn = self.get_pooled_connection()?;
        embedded_migrations::run_with_output(&conn, &mut buf)
            .map_err(|err| StorageError::DatabaseMigrationFailed(format!("Database migration failed {}", err)))?;
        Ok(String::from_utf8_lossy(&buf.into_inner()).to_string())
    }
}

#[cfg(test)]
mod test {
    use diesel::{expression::sql_literal::sql, sql_types::Integer, RunQueryDsl};
    use tari_comms::runtime;
    use tari_test_utils::random;

    use super::*;

    #[runtime::test]
    async fn connect_and_migrate() {
        let conn = DbConnection::connect_memory(random::string(8)).unwrap();
        let output = conn.migrate().unwrap();
        assert!(output.starts_with("Running migration"));
    }

    #[runtime::test]
    async fn memory_connections() {
        let id = random::string(8);
        let conn = DbConnection::connect_memory(id.clone()).unwrap();
        conn.migrate().unwrap();
        let conn = DbConnection::connect_memory(id).unwrap();
        let conn = conn.get_pooled_connection().unwrap();
        let count: i32 = sql::<Integer>("SELECT COUNT(*) FROM stored_messages")
            .get_result(&conn)
            .unwrap();
        assert_eq!(count, 0);
    }
}
