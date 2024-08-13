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
use std::convert::TryFrom;

use diesel::{
    r2d2::{ConnectionManager, Pool, PooledConnection},
    SqliteConnection,
};
use log::*;

use crate::{connection_options::ConnectionOptions, error::SqliteStorageError};

const LOG_TARGET: &str = "common_sqlite::sqlite_connection_pool";

#[derive(Clone)]
pub struct SqliteConnectionPool {
    pool: Option<Pool<ConnectionManager<SqliteConnection>>>,
    db_path: String,
    pool_size: usize,
    connection_options: ConnectionOptions,
}

impl SqliteConnectionPool {
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
            let pool = Pool::builder()
                .max_size(u32::try_from(self.pool_size)?)
                .connection_customizer(Box::new(self.connection_options.clone()))
                .build(ConnectionManager::<SqliteConnection>::new(self.db_path.as_str()))
                .map_err(|e| SqliteStorageError::DieselR2d2Error(e.to_string()));
            self.pool = Some(pool?);
        } else {
            warn!(
                target: LOG_TARGET,
                "Connection pool for {} already exists", self.db_path
            );
        }
        Ok(())
    }

    /// Return a pooled sqlite connection managed by the pool connection manager, waits for at most the configured
    /// connection timeout before returning an error.
    pub fn get_pooled_connection(
        &self,
    ) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, SqliteStorageError> {
        if let Some(pool) = self.pool.clone() {
            pool.get().map_err(|e| {
                warn!(
                    target: LOG_TARGET,
                    "Connection pool state {:?}: {}",
                    pool.state(),
                    e.to_string()
                );
                SqliteStorageError::DieselR2d2Error(e.to_string())
            })
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
            pool.get_timeout(timeout).map_err(|e| {
                warn!(
                    target: LOG_TARGET,
                    "Connection pool state {:?}: {}",
                    pool.state(),
                    e.to_string()
                );
                SqliteStorageError::DieselR2d2Error(e.to_string())
            })
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
            let connection = pool.try_get();
            if connection.is_none() {
                warn!(
                    target: LOG_TARGET,
                    "No connections available, pool state {:?}",
                    pool.state()
                );
            };
            Ok(connection)
        } else {
            Err(SqliteStorageError::DieselR2d2Error("Pool does not exist".to_string()))
        }
    }
}

pub trait PooledDbConnection: Send + Sync + Clone {
    type Error;

    fn get_pooled_connection(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, Self::Error>;
}
