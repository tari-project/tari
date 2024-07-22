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

use diesel::{connection::SimpleConnection, SqliteConnection};

#[derive(Debug, Clone)]
pub struct ConnectionOptions {
    enable_wal: bool,
    enable_foreign_keys: bool,
    busy_timeout: Option<Duration>,
}

impl ConnectionOptions {
    pub fn new(enable_wal: bool, enable_foreign_keys: bool, busy_timeout: Duration) -> Self {
        Self {
            enable_wal,
            enable_foreign_keys,
            busy_timeout: Some(busy_timeout),
        }
    }
}

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ConnectionOptions {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        (|| {
            if let Some(d) = self.busy_timeout {
                conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
            }
            if self.enable_wal {
                conn.batch_execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
            }
            if self.enable_foreign_keys {
                conn.batch_execute("PRAGMA foreign_keys = ON;")?;
            }
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}
