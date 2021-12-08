//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::path::Path;

use lmdb_zero::db;
use tari_storage::lmdb_store::{LMDBBuilder, LMDBConfig, LMDBStore};

use crate::storage::StorageError;

const DATABASES: &[(&str, db::Flags)] = &[("metadata", db::INTEGERKEY)];

pub fn create_lmdb_store<P: AsRef<Path>>(path: P, config: LMDBConfig) -> Result<LMDBStore, StorageError> {
    const CREATE_FLAG: db::Flags = db::CREATE;

    let mut lmdb_store = LMDBBuilder::new()
        .set_path(path)
        .set_env_config(config)
        .set_max_number_of_databases(10);

    for (db_name, flags) in DATABASES {
        lmdb_store = lmdb_store.add_database(db_name, CREATE_FLAG | *flags);
    }

    let lmdb_store = lmdb_store.build()?;
    Ok(lmdb_store)
}
