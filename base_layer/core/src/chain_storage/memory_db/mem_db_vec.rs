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

use crate::chain_storage::error::ChainStorageError;
use std::{
    cmp::min,
    sync::{Arc, RwLock},
};
use tari_mmr::{error::MerkleMountainRangeError, ArrayLike, ArrayLikeExt};

#[derive(Debug, Clone)]
pub struct MemDbVec<T> {
    db: Arc<RwLock<Vec<T>>>,
}

impl<T> MemDbVec<T> {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(Vec::<T>::new())),
        }
    }
}

impl<T: Clone> ArrayLike for MemDbVec<T> {
    type Error = ChainStorageError;
    type Value = T;

    fn len(&self) -> Result<usize, Self::Error> {
        Ok(self
            .db
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .len())
    }

    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error> {
        self.db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .push(item);
        Ok(self.len()? - 1)
    }

    fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error> {
        Ok(self
            .db
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .get(index)
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?)
    }

    fn get_or_panic(&self, index: usize) -> Self::Value {
        self.db.read().unwrap()[index].clone()
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .clear();
        Ok(())
    }
}

impl<T: Clone> ArrayLikeExt for MemDbVec<T> {
    type Value = T;

    fn truncate(&mut self, len: usize) -> Result<(), MerkleMountainRangeError> {
        self.db
            .write()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .truncate(len);
        Ok(())
    }

    fn shift(&mut self, n: usize) -> Result<(), MerkleMountainRangeError> {
        let drain_n = min(
            n,
            self.len()
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?,
        );
        self.db
            .write()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .drain(0..drain_n);
        Ok(())
    }

    fn for_each<F>(&self, f: F) -> Result<(), MerkleMountainRangeError>
    where F: FnMut(Result<Self::Value, MerkleMountainRangeError>) {
        self.db
            .read()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .iter()
            .map(|v| Ok(v.clone()))
            .for_each(f);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn len_push_get_truncate_for_each_shift_clear() {
        let mut db_vec = MemDbVec::<i32>::new();
        let mut mem_vec = vec![100, 200, 300, 400, 500, 600];
        assert_eq!(db_vec.len().unwrap(), 0);

        mem_vec.iter().for_each(|val| assert!(db_vec.push(val.clone()).is_ok()));
        assert_eq!(db_vec.len().unwrap(), mem_vec.len());

        mem_vec
            .iter()
            .enumerate()
            .for_each(|(i, val)| assert_eq!(db_vec.get(i).unwrap(), Some(val.clone())));
        assert_eq!(db_vec.get(mem_vec.len()).unwrap(), None);

        mem_vec.truncate(4);
        assert!(db_vec.truncate(4).is_ok());
        assert_eq!(db_vec.len().unwrap(), mem_vec.len());
        db_vec.for_each(|val| assert!(mem_vec.contains(&val.unwrap()))).unwrap();

        assert!(mem_vec.shift(2).is_ok());
        assert!(db_vec.shift(2).is_ok());
        assert_eq!(db_vec.len().unwrap(), 2);
        assert_eq!(db_vec.get(0).unwrap(), Some(300));
        assert_eq!(db_vec.get(1).unwrap(), Some(400));

        assert!(db_vec.clear().is_ok());
        assert_eq!(db_vec.len().unwrap(), 0);
    }
}
