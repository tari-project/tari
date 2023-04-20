//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{convert::TryInto, path::PathBuf};

use diesel::{Connection, SqliteConnection};
use tari_common_sqlite::{
    connection::{DbConnection, DbConnectionUrl},
    error::StorageError,
};
use tari_storage::lmdb_store::{LMDBBuilder, LMDBConfig};
use tari_test_utils::random::string;

pub fn create_chat_storage(base_path: PathBuf) -> Result<DbConnection, StorageError> {
    std::fs::create_dir_all(&base_path).unwrap();
    let db_name = format!("{}.sqlite3", string(8).as_str());
    let db_path = format!("{}/{}", base_path.to_str().unwrap(), db_name);
    let url: DbConnectionUrl = db_path.clone().try_into().unwrap();

    // Create the db
    let _db = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

    DbConnection::connect_url(&url)
}

pub fn create_peer_storage(base_path: PathBuf) {
    std::fs::create_dir_all(&base_path).unwrap();

    LMDBBuilder::new()
        .set_path(&base_path)
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(1)
        .add_database("peerdb", lmdb_zero::db::CREATE)
        .build()
        .unwrap();
}
