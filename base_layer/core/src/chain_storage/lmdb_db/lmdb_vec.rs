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

use crate::chain_storage::{
    error::ChainStorageError,
    lmdb_db::lmdb::{lmdb_delete, lmdb_get, lmdb_insert, lmdb_len},
};
use derive_error::Error;
use lmdb_zero::{Database, Environment, WriteTransaction};
use std::{marker::PhantomData, sync::Arc};
use tari_mmr::{error::MerkleMountainRangeError, ArrayLike, ArrayLikeExt};
use tari_storage::lmdb_store::LMDBError;
use tari_utilities::message_format::MessageFormatError;

#[derive(Debug, Error)]
pub enum LMDBVecError {
    MessageFormatError(MessageFormatError),
    LMDBError(LMDBError),
    ChainStorageError(ChainStorageError),
}

pub struct LMDBVec<T> {
    env: Arc<Environment>,
    db: Arc<Database<'static>>,
    _t: PhantomData<T>,
}

impl<T> LMDBVec<T> {
    pub fn new(env: Arc<Environment>, db: Arc<Database<'static>>) -> Self {
        Self {
            env,
            db,
            _t: PhantomData,
        }
    }
}

impl<T> ArrayLike for LMDBVec<T>
where
    T: serde::Serialize,
    for<'t> T: serde::de::DeserializeOwned,
{
    type Error = LMDBVecError;
    type Value = T;

    fn len(&self) -> Result<usize, Self::Error> {
        Ok(lmdb_len(&self.env, &self.db)?)
    }

    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error> {
        let index = self.len()?;
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        {
            lmdb_insert::<usize, T>(&txn, &self.db, &index, &item)?;
        }
        txn.commit()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        Ok(index)
    }

    fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error> {
        Ok(lmdb_get::<usize, T>(&self.env, &self.db, &index)?)
    }

    fn get_or_panic(&self, index: usize) -> Self::Value {
        self.get(index).unwrap().unwrap()
    }
}

impl<T> ArrayLikeExt for LMDBVec<T>
where for<'t> T: serde::de::DeserializeOwned
{
    type Value = T;

    fn truncate(&mut self, len: usize) -> Result<(), MerkleMountainRangeError> {
        let n_elements =
            lmdb_len(&self.env, &self.db).map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        if n_elements > len {
            let txn = WriteTransaction::new(self.env.clone())
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
            {
                for index in len..n_elements {
                    lmdb_delete(&txn, &self.db, &index)
                        .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
                }
            }
            txn.commit()
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        }
        Ok(())
    }

    fn for_each<F>(&self, mut f: F) -> Result<(), MerkleMountainRangeError>
    where F: FnMut(Result<Self::Value, MerkleMountainRangeError>) {
        let n_elements =
            lmdb_len(&self.env, &self.db).map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        for index in 0..n_elements {
            let val = lmdb_get::<usize, T>(&self.env, &self.db, &index)
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
                .ok_or(MerkleMountainRangeError::BackendError("Unexpected error".into()))?;
            f(Ok(val))
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_storage::lmdb_store::{db, LMDBBuilder};
    use tari_test_utils::paths::create_random_database_path;

    #[test]
    fn len_push_get_truncate_for_each() {
        let path = create_random_database_path().to_str().unwrap().to_string();
        let _ = std::fs::create_dir(&path).unwrap_or_default();
        let lmdb_store = LMDBBuilder::new()
            .set_path(&path)
            .set_environment_size(1)
            .set_max_number_of_databases(1)
            .add_database("db", db::CREATE)
            .build()
            .unwrap();
        let mut lmdb_vec = LMDBVec::<i32>::new(lmdb_store.env(), lmdb_store.get_handle("db").unwrap().db().clone());
        let mut mem_vec = vec![100, 200, 300, 400];
        assert_eq!(lmdb_vec.len().unwrap(), 0);

        mem_vec
            .iter()
            .for_each(|val| assert!(lmdb_vec.push(val.clone()).is_ok()));
        assert_eq!(lmdb_vec.len().unwrap(), mem_vec.len());

        mem_vec
            .iter()
            .enumerate()
            .for_each(|(i, val)| assert_eq!(lmdb_vec.get(i).unwrap(), Some(val.clone())));
        assert_eq!(lmdb_vec.get(mem_vec.len()).unwrap(), None);

        mem_vec.truncate(2);
        assert!(lmdb_vec.truncate(2).is_ok());
        assert_eq!(lmdb_vec.len().unwrap(), mem_vec.len());
        lmdb_vec
            .for_each(|val| assert!(mem_vec.contains(&val.unwrap())))
            .unwrap();
    }
}
