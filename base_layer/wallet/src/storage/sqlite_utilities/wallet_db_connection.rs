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
    r2d2::{ConnectionManager, PooledConnection},
    SqliteConnection,
};
use tari_common_sqlite::sqlite_connection_pool::SqliteConnectionPool;

use crate::error::WalletStorageError;

#[derive(Clone)]
pub struct WalletDbConnection {
    pool: SqliteConnectionPool,
    _file_lock: Arc<Option<File>>,
}

impl WalletDbConnection {
    pub fn new(pool: SqliteConnectionPool, file_lock: Option<File>) -> Self {
        Self {
            pool,
            _file_lock: Arc::new(file_lock),
        }
    }

    pub fn get_pooled_connection(
        &self,
    ) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, WalletStorageError> {
        self.pool
            .get_pooled_connection()
            .map_err(WalletStorageError::DieselR2d2Error)
    }
}
