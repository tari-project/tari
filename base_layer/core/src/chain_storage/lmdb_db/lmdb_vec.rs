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
    lmdb_db::lmdb::{lmdb_clear_db, lmdb_delete, lmdb_get, lmdb_insert, lmdb_len, lmdb_replace},
};
use derive_error::Error;
use lmdb_zero::{Database, Environment, WriteTransaction};
use log::*;
use std::{cmp::min, marker::PhantomData, sync::Arc};
use tari_crypto::tari_utilities::message_format::MessageFormatError;
use tari_mmr::{error::MerkleMountainRangeError, ArrayLike, ArrayLikeExt};
use tari_storage::lmdb_store::LMDBError;

const INDEX_OFFSET_DB_KEY: i64 = i64::min_value();
pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_vec";

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

impl<T> LMDBVec<T>
where
    T: serde::Serialize,
    for<'t> T: serde::de::DeserializeOwned,
{
    // Fetches the stored index offset of the first stored element in the db. When the index offset cannot be retrieved
    // then a zero offset is recorded.
    fn fetch_index_offset(&self) -> Result<i64, ChainStorageError> {
        Ok(
            match lmdb_get::<i64, i64>(&self.env, &self.db, &INDEX_OFFSET_DB_KEY)
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            {
                Some(offset) => offset,
                None => {
                    let offset = 0;
                    self.set_index_offset(offset)?;
                    offset
                },
            },
        )
    }

    // Store the provided offset as the new index offset.
    fn set_index_offset(&self, offset: i64) -> Result<(), ChainStorageError> {
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        {
            lmdb_replace::<i64, i64>(&txn, &self.db, &INDEX_OFFSET_DB_KEY, &offset)?;
        }
        txn.commit().map_err(|e| {
            error!(target: LOG_TARGET, "Lmdb commit failed with: {:?}", e);
            ChainStorageError::AccessError(e.to_string())
        })
    }

    // Uses the stored index offset to calculate the new db key for the provided index.
    fn fetch_key(&self, index: usize) -> Result<i64, LMDBVecError> {
        Ok(index_to_key(self.fetch_index_offset()?, index))
    }
}

// Combine the index offset and array index to create a db key.
fn index_to_key(offset: i64, index: usize) -> i64 {
    offset + index as i64
}

impl<T> ArrayLike for LMDBVec<T>
where
    T: serde::Serialize + PartialEq,
    for<'t> T: serde::de::DeserializeOwned,
{
    type Error = LMDBVecError;
    type Value = T;

    fn len(&self) -> Result<usize, Self::Error> {
        Ok(lmdb_len(&self.env, &self.db)?.saturating_sub(1)) // Exclude the index offset
    }

    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self.len()? == 0)
    }

    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error> {
        let len = self.len()?;
        let new_key = self.fetch_key(len)?;
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        {
            lmdb_insert::<i64, T>(&txn, &self.db, &new_key, &item)?;
        }
        txn.commit().map_err(|e| {
            error!(target: LOG_TARGET, "Lmdb commit failed with: {:?}", e);
            ChainStorageError::AccessError(e.to_string())
        })?;
        Ok(len)
    }

    fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error> {
        let key = self.fetch_key(index)?;
        Ok(lmdb_get::<i64, T>(&self.env, &self.db, &key)?)
    }

    fn get_or_panic(&self, index: usize) -> Self::Value {
        self.get(index).unwrap().unwrap()
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        {
            lmdb_clear_db(&txn, &self.db)?;
        }
        txn.commit().map_err(|e| {
            error!(target: LOG_TARGET, "Lmdb commit failed with: {:?}", e);
            ChainStorageError::AccessError(e.to_string())
        })?;
        Ok(())
    }

    fn position(&self, item: &Self::Value) -> Result<Option<usize>, Self::Error> {
        let num_elements = self.len()?;
        let index_offset = self.fetch_index_offset()?;
        for index in 0..num_elements {
            let key = index_to_key(index_offset, index);
            if let Some(stored_item) = lmdb_get::<i64, T>(&self.env, &self.db, &key)
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            {
                if stored_item == *item {
                    return Ok(Some(index));
                }
            }
        }
        Ok(None)
    }
}

impl<T> ArrayLikeExt for LMDBVec<T>
where
    T: serde::Serialize + PartialEq,
    for<'t> T: serde::de::DeserializeOwned,
{
    type Value = T;

    fn truncate(&mut self, len: usize) -> Result<(), MerkleMountainRangeError> {
        let num_elements = self
            .len()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        if num_elements > len {
            let index_offset = self
                .fetch_index_offset()
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
            let txn = WriteTransaction::new(self.env.clone())
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
            {
                for index in len..num_elements {
                    let key = index_to_key(index_offset, index);
                    lmdb_delete(&txn, &self.db, &key)
                        .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
                }
            }
            txn.commit().map_err(|e| {
                error!(target: LOG_TARGET, "Lmdb commit failed with: {:?}", e);
                MerkleMountainRangeError::BackendError(e.to_string())
            })?;
        }
        Ok(())
    }

    fn shift(&mut self, n: usize) -> Result<(), MerkleMountainRangeError> {
        let num_elements = self
            .len()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        let num_drain = min(n, num_elements);
        let index_offset = self
            .fetch_index_offset()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        let txn = WriteTransaction::new(self.env.clone())
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        {
            for index in 0..num_drain {
                let key = index_to_key(index_offset, index);
                lmdb_delete(&txn, &self.db, &key).map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
            }
            // Update the stored index offset
            let updated_index_offset = index_offset + num_drain as i64;
            lmdb_replace::<i64, i64>(&txn, &self.db, &INDEX_OFFSET_DB_KEY, &updated_index_offset)
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        }
        txn.commit().map_err(|e| {
            error!(target: LOG_TARGET, "Lmdb commit failed with: {:?}", e);
            MerkleMountainRangeError::BackendError(e.to_string())
        })
    }

    fn push_front(&mut self, item: Self::Value) -> Result<(), MerkleMountainRangeError> {
        let index_offset = self
            .fetch_index_offset()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        let key = index_offset - 1;
        let txn = WriteTransaction::new(self.env.clone())
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        {
            lmdb_insert::<i64, T>(&txn, &self.db, &key, &item)
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
            lmdb_replace::<i64, i64>(&txn, &self.db, &INDEX_OFFSET_DB_KEY, &key)
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        }
        txn.commit().map_err(|e| {
            error!(target: LOG_TARGET, "Lmdb commit failed with: {:?}", e);
            MerkleMountainRangeError::BackendError(e.to_string())
        })?;
        Ok(())
    }

    fn for_each<F>(&self, mut f: F) -> Result<(), MerkleMountainRangeError>
    where F: FnMut(Result<Self::Value, MerkleMountainRangeError>) {
        let num_elements = self
            .len()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        let index_offset = self
            .fetch_index_offset()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        for index in 0..num_elements {
            let key = index_to_key(index_offset, index);
            let val = lmdb_get::<i64, T>(&self.env, &self.db, &key)
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
                .ok_or_else(|| MerkleMountainRangeError::BackendError("Unexpected error".into()))?;
            f(Ok(val))
        }
        Ok(())
    }
}

impl<T> Clone for LMDBVec<T>
where
    T: serde::Serialize,
    for<'t> T: serde::de::DeserializeOwned,
{
    fn clone(&self) -> Self {
        LMDBVec::new(self.env.clone(), self.db.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_storage::lmdb_store::{db, LMDBBuilder};
    use tari_test_utils::paths::create_temporary_data_path;

    #[test]
    fn len_push_get_truncate_for_each_shift_clear() {
        let path = create_temporary_data_path().to_str().unwrap().to_string();
        let _ = std::fs::create_dir(&path).unwrap_or_default();
        let lmdb_store = LMDBBuilder::new()
            .set_path(&path)
            .set_environment_size(1)
            .set_max_number_of_databases(1)
            .add_database("db", db::CREATE)
            .build()
            .unwrap();
        let mut lmdb_vec = LMDBVec::<i32>::new(lmdb_store.env(), lmdb_store.get_handle("db").unwrap().db());
        let mut mem_vec = vec![100, 200, 300, 400, 500, 600];
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

        mem_vec.truncate(4);
        assert!(lmdb_vec.truncate(4).is_ok());
        assert_eq!(lmdb_vec.len().unwrap(), mem_vec.len());
        lmdb_vec
            .for_each(|val| assert!(mem_vec.contains(&val.unwrap())))
            .unwrap();

        assert!(mem_vec.shift(2).is_ok());
        assert!(lmdb_vec.shift(2).is_ok());
        assert_eq!(lmdb_vec.len().unwrap(), 2);
        mem_vec
            .iter()
            .enumerate()
            .for_each(|(i, val)| assert_eq!(lmdb_vec.get(i).unwrap(), Some(val.clone())));

        assert!(mem_vec.push_front(200).is_ok());
        assert!(mem_vec.push_front(100).is_ok());
        assert!(lmdb_vec.push_front(200).is_ok());
        assert!(lmdb_vec.push_front(100).is_ok());
        assert_eq!(lmdb_vec.len().unwrap(), 4);
        mem_vec
            .iter()
            .enumerate()
            .for_each(|(i, val)| assert_eq!(lmdb_vec.get(i).unwrap(), Some(val.clone())));

        for index in 0..lmdb_vec.len().unwrap() {
            let item = lmdb_vec.get(index).unwrap().unwrap();
            assert_eq!(lmdb_vec.position(&item).unwrap(), Some(index));
        }

        assert!(lmdb_vec.clear().is_ok());
        assert_eq!(lmdb_vec.len().unwrap(), 0);
    }
}
