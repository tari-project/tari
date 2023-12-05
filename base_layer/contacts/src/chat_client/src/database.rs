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

use std::path::{Path, PathBuf};

use diesel::{Connection, SqliteConnection};
use tari_common_sqlite::{
    connection::{DbConnection, DbConnectionUrl},
    error::StorageError as SqliteStorageError,
};
use tari_contacts::contacts_service::storage::sqlite_db::ContactsServiceSqliteDatabase;
use tari_storage::lmdb_store::{LMDBBuilder, LMDBConfig};

use crate::error::StorageError;

pub fn connect_to_db(db_path: PathBuf) -> Result<ContactsServiceSqliteDatabase<DbConnection>, SqliteStorageError> {
    let url: DbConnectionUrl = DbConnectionUrl::File(db_path);
    let connection = DbConnection::connect_url(&url)?;
    Ok(ContactsServiceSqliteDatabase::init(connection))
}

pub fn create_chat_storage(db_file_path: &Path) -> Result<(), StorageError> {
    // Create the storage db
    std::fs::create_dir_all(db_file_path.parent().ok_or(StorageError::FilePathError)?)?;
    let _db = SqliteConnection::establish(db_file_path.as_os_str().to_str().ok_or(StorageError::FilePathError)?)?;
    Ok(())
}

pub fn create_peer_storage(base_path: &PathBuf) -> Result<(), StorageError> {
    std::fs::create_dir_all(base_path)?;

    LMDBBuilder::new()
        .set_path(base_path)
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(1)
        .add_database("peerdb", lmdb_zero::db::CREATE)
        .build()?;

    Ok(())
}
