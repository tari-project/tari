// Copyright 2019. The Tari Project
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

use std::path::PathBuf;

use tari_test_utils::random;
use tari_wallet::storage::sqlite_utilities::{run_migration_and_create_sqlite_connection, WalletDbConnection};
use tempfile::{tempdir, TempDir};

pub fn get_path(name: Option<&str>) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data");
    path.push(name.unwrap_or(""));
    path.to_str().unwrap().to_string()
}

pub fn clean_up_sql_database(name: &str) {
    if std::fs::metadata(get_path(Some(name))).is_ok() {
        std::fs::remove_file(get_path(Some(name))).unwrap();
    }
}

pub fn init_sql_database(name: &str) {
    clean_up_sql_database(name);
    let path = get_path(None);
    let _ = std::fs::create_dir(&path).unwrap_or_default();
}

pub fn get_temp_sqlite_database_connection() -> (WalletDbConnection, TempDir) {
    let db_name = format!("{}.sqlite3", random::string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    // let db_path = "/tmp/test.sqlite3".to_string();
    let connection = run_migration_and_create_sqlite_connection(&db_path, 16).unwrap();

    (connection, db_tempdir)
}
