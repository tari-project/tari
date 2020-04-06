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

use crate::storage::error::StorageError;
use diesel::{Connection, SqliteConnection};
use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::task;

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
    inner: Arc<Mutex<SqliteConnection>>,
}

impl DbConnection {
    #[cfg(test)]
    pub async fn connect_memory(name: String) -> Result<Self, StorageError> {
        Self::connect_url(DbConnectionUrl::MemoryShared(name)).await
    }

    pub async fn connect_url(db_url: DbConnectionUrl) -> Result<Self, StorageError> {
        let conn = task::spawn_blocking(move || {
            let conn = SqliteConnection::establish(&db_url.to_url_string())?;
            conn.execute("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 60000;")?;
            Result::<_, StorageError>::Ok(conn)
        })
        .await??;

        Ok(Self::new(conn))
    }

    fn new(conn: SqliteConnection) -> Self {
        Self {
            inner: Arc::new(Mutex::new(conn)),
        }
    }

    pub async fn migrate(&self) -> Result<String, StorageError> {
        embed_migrations!("./migrations");

        self.with_connection_async(|conn| {
            let mut buf = io::Cursor::new(Vec::new());
            embedded_migrations::run_with_output(conn, &mut buf)
                .map_err(|err| StorageError::DatabaseMigrationFailed(format!("Database migration failed {}", err)))?;
            Ok(String::from_utf8_lossy(&buf.into_inner()).to_string())
        })
        .await
    }

    pub async fn with_connection_async<F, R>(&self, f: F) -> Result<R, StorageError>
    where
        F: FnOnce(&SqliteConnection) -> Result<R, StorageError> + Send + 'static,
        R: Send + 'static,
    {
        let conn_mutex = self.inner.clone();
        let ret = task::spawn_blocking(move || {
            let lock = acquire_lock!(conn_mutex);
            f(&*lock)
        })
        .await??;
        Ok(ret)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_test_utils::random;

    #[tokio_macros::test_basic]
    async fn connect_and_migrate() {
        let conn = DbConnection::connect_memory(random::string(8)).await.unwrap();
        let output = conn.migrate().await.unwrap();
        assert!(output.starts_with("Running migration"));
    }
}
