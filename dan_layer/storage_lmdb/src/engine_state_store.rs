//  Copyright 2022. The Tari Project
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

use std::{ops::Deref, path::Path, sync::Arc};

use lmdb_zero::{db, put, ConstTransaction, LmdbResultExt, ReadTransaction, WriteTransaction};
use tari_dan_engine::state_store::{AtomicDb, StateReader, StateStoreError, StateWriter};
use tari_storage::lmdb_store::{DatabaseRef, LMDBBuilder};

pub struct LmdbTransaction<T> {
    db: DatabaseRef,
    tx: T,
}

pub struct LmdbStateStore {
    pub env: Arc<lmdb_zero::Environment>,
    pub db: DatabaseRef,
}

impl LmdbStateStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let flags = db::CREATE;
        std::fs::create_dir_all(&path).unwrap();

        let store = LMDBBuilder::new()
            .set_path(path)
            // .set_env_config(config)
            .set_max_number_of_databases(1)
            .add_database("test_db", flags )
            .build().unwrap();

        let handle = store.get_handle("test_db").unwrap();
        let db = handle.db();
        Self { env: store.env(), db }
    }
}

impl<'a> AtomicDb<'a> for LmdbStateStore {
    type Error = lmdb_zero::Error;
    type ReadAccess = LmdbTransaction<ReadTransaction<'a>>;
    type WriteAccess = LmdbTransaction<WriteTransaction<'a>>;

    fn read_access(&'a self) -> Result<Self::ReadAccess, Self::Error> {
        let tx = ReadTransaction::new(self.env.clone())?;

        Ok(LmdbTransaction {
            db: self.db.clone(),
            tx,
        })
    }

    fn write_access(&'a self) -> Result<Self::WriteAccess, Self::Error> {
        let tx = WriteTransaction::new(self.env.clone())?;

        Ok(LmdbTransaction {
            db: self.db.clone(),
            tx,
        })
    }

    fn commit(&self, tx: Self::WriteAccess) -> Result<(), Self::Error> {
        tx.tx.commit()
    }
}

impl<'a, T: Deref<Target = ConstTransaction<'a>>> StateReader for LmdbTransaction<T> {
    fn get_state_raw(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StateStoreError> {
        let access = self.tx.access();
        access
            .get::<_, [u8]>(&*self.db, key)
            .map(|data| data.to_vec())
            .to_opt()
            .map_err(StateStoreError::custom)
    }

    fn exists(&self, key: &[u8]) -> Result<bool, StateStoreError> {
        Ok(self.get_state_raw(key)?.is_some())
    }
}

impl<'a> StateWriter for LmdbTransaction<WriteTransaction<'a>> {
    fn set_state_raw(&mut self, key: &[u8], value: Vec<u8>) -> Result<(), StateStoreError> {
        let mut access = self.tx.access();
        access
            .put(&*self.db, key, &value, put::Flags::empty())
            .map_err(StateStoreError::custom)
    }
}

#[cfg(test)]
mod tests {

    use borsh::{BorshDeserialize, BorshSerialize};
    use tempfile::tempdir;

    use super::*;

    #[derive(Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq, Clone)]
    struct UserData {
        name: String,
        age: u8,
    }

    #[test]
    fn read_write_rollback_commit() {
        let user_data = UserData {
            name: "Foo".to_string(),
            age: 99,
        };

        let path = tempdir().unwrap();
        let store = LmdbStateStore::new(&path);
        {
            let mut access = store.write_access().unwrap();
            access.set_state(b"abc", user_data.clone()).unwrap();
            assert!(access.exists(b"abc").unwrap());
            let res = access.get_state(b"abc").unwrap();
            assert_eq!(res, Some(user_data.clone()));
            let res = access.get_state::<_, UserData>(b"def").unwrap();
            assert_eq!(res, None);
            // Drop without commit rolls back
        }

        {
            let access = store.read_access().unwrap();
            let res = access.get_state::<_, UserData>(b"abc").unwrap();
            assert_eq!(res, None);
            assert!(!access.exists(b"abc").unwrap());
        }

        {
            let mut access = store.write_access().unwrap();
            access.set_state(b"abc", user_data.clone()).unwrap();
            store.commit(access).unwrap();
        }

        let access = store.read_access().unwrap();
        let res = access.get_state(b"abc").unwrap();
        assert_eq!(res, Some(user_data));
    }
}
