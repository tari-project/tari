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

use crate::error::WalletStorageError;
use diesel::{Connection, SqliteConnection};
use std::{
    io,
    path::Path,
    sync::{Arc, Mutex},
};

pub type WalletDbConnection = Arc<Mutex<SqliteConnection>>;

pub fn run_migration_and_create_sqlite_connection<P: AsRef<Path>>(
    db_path: P,
) -> Result<WalletDbConnection, WalletStorageError> {
    let db_exists = db_path.as_ref().exists();
    let path_str = db_path
        .as_ref()
        .to_str()
        .ok_or_else(|| WalletStorageError::InvalidUnicodePath)?;
    let connection = SqliteConnection::establish(path_str)?;
    connection.execute("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 60000;")?;

    if !db_exists {
        embed_migrations!("./migrations");
        embedded_migrations::run_with_output(&connection, &mut io::stdout())
            .map_err(|err| WalletStorageError::DatabaseMigrationError(format!("Database migration failed {}", err)))?;
    }

    Ok(Arc::new(Mutex::new(connection)))
}
