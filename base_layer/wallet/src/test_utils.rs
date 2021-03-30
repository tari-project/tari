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

use crate::{
    output_manager_service::storage::sqlite_db::OutputManagerSqliteDatabase,
    storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
    transaction_service::storage::sqlite_db::TransactionServiceSqliteDatabase,
};
use core::iter;
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::path::Path;
use tempfile::{tempdir, TempDir};

pub fn random_string(len: usize) -> String {
    iter::repeat(()).map(|_| OsRng.sample(Alphanumeric)).take(len).collect()
}

/// A test helper to create a temporary wallet service databases
pub fn make_wallet_databases(
    path: Option<String>,
) -> (
    TransactionServiceSqliteDatabase,
    OutputManagerSqliteDatabase,
    Option<TempDir>,
) {
    let (path_string, temp_dir): (String, Option<TempDir>) = if let Some(p) = path {
        (p, None)
    } else {
        let temp_dir = tempdir().unwrap();
        let path_string = temp_dir.path().to_str().unwrap().to_string();
        (path_string, Some(temp_dir))
    };

    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_path = Path::new(&path_string).join(db_name);

    let connection =
        run_migration_and_create_sqlite_connection(&db_path.to_str().expect("Should be able to make path")).unwrap();
    (
        TransactionServiceSqliteDatabase::new(connection.clone(), None),
        OutputManagerSqliteDatabase::new(connection, None),
        temp_dir,
    )
}
