//  Copyright 2022. The Taiji Project
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

use std::{collections::HashMap, path::PathBuf};

use lmdb_zero::{db, ReadTransaction, WriteTransaction};
use taiji_storage::lmdb_store::{DatabaseRef, LMDBBuilder, LMDBConfig};
use taiji_test_utils::paths::create_temporary_data_path;

pub struct TempLmdbDatabase {
    temp_path: PathBuf,
    default_db: Option<DatabaseRef>,
    dbs: HashMap<&'static str, DatabaseRef>,
}

impl TempLmdbDatabase {
    pub fn new() -> Self {
        Self::with_dbs(&[])
    }

    pub fn with_dbs(dbs: &[&'static str]) -> Self {
        let temp_path = create_temporary_data_path();

        let mut builder = LMDBBuilder::new()
            .set_path(&temp_path)
            .set_env_config(LMDBConfig::default())
            .set_max_number_of_databases(1)
            .add_database("__default", db::CREATE);

        for db in dbs {
            builder = builder.add_database(db, db::CREATE)
        }
        let lmdb_store = builder.build().unwrap();

        let default_db = lmdb_store.get_handle("__default").unwrap();

        Self {
            temp_path,
            default_db: Some(default_db.db()),
            dbs: dbs
                .iter()
                .map(|db| (*db, lmdb_store.get_handle(db).unwrap().db()))
                .collect(),
        }
    }

    pub fn default_db(&self) -> &DatabaseRef {
        self.default_db.as_ref().unwrap()
    }

    pub fn get_db(&self, name: &'static str) -> &DatabaseRef {
        self.dbs.get(name).unwrap()
    }

    pub fn write_transaction(&self) -> WriteTransaction<'_> {
        WriteTransaction::new(self.default_db().env()).unwrap()
    }

    pub fn read_transaction(&self) -> ReadTransaction<'_> {
        ReadTransaction::new(self.default_db().env()).unwrap()
    }
}

impl Drop for TempLmdbDatabase {
    fn drop(&mut self) {
        drop(self.default_db.take());
        drop(self.dbs.drain());
        let _ignore = std::fs::remove_dir_all(&self.temp_path);
    }
}
