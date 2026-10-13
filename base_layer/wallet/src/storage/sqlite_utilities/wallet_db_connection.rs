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

use std::{fs::File, sync::Arc};

use diesel::{
    SqliteConnection,
    r2d2::{ConnectionManager, PooledConnection},
};
use log::*;
use tari_common_sqlite::{
    error::SqliteStorageError,
    sqlite_connection_pool::{PooledDbConnection, SqliteConnectionPool},
};

/// A handle to the wallet database. Cheap to clone; every clone shares one pool and one [`WalletDbLastHandle`].
#[derive(Clone)]
pub struct WalletDbConnection {
    pool: SqliteConnectionPool,
    _last_handle: Arc<WalletDbLastHandle>,
}

/// Shared by every clone of a [`WalletDbConnection`] and dropped with the last one, which is when the wallet database
/// is closed: it truncates the WAL (best effort, see [`SqliteConnectionPool::cleanup`]) while the wallet's file lock is
/// still held, then releases the lock.
struct WalletDbLastHandle {
    pool: SqliteConnectionPool,
    _file_lock: Option<File>,
}

impl Drop for WalletDbLastHandle {
    fn drop(&mut self) {
        let pool_state = self.pool.cleanup();
        debug!(target: LOG_TARGET, "Closed the wallet database, pool state before cleanup: {pool_state:?}");
    }
}

impl WalletDbConnection {
    pub fn new(pool: SqliteConnectionPool, file_lock: Option<File>) -> Self {
        Self {
            _last_handle: Arc::new(WalletDbLastHandle {
                pool: pool.clone(),
                _file_lock: file_lock,
            }),
            pool,
        }
    }
}

const LOG_TARGET: &str = "wallet::storage::wallet_db_connection";

impl PooledDbConnection for WalletDbConnection {
    type Error = SqliteStorageError;

    fn get_pooled_connection(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, Self::Error> {
        let conn = self.pool.get_pooled_connection()?;
        Ok(conn)
    }
}
