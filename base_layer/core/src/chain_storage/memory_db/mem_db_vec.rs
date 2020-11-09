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
    collections::HashMap,
    hash::Hash,
    sync::{Arc, RwLock},
};
use tari_mmr::{error::MerkleMountainRangeError, ArrayLike, ArrayLikeExt};

#[derive(Debug)]
struct MemDbVecStorage<T>
where T: PartialEq + Eq + Hash + Clone
{
    index_offset: i64,
    key_to_item: HashMap<i64, T>,
    item_to_key: HashMap<T, i64>,
}

impl<T> MemDbVecStorage<T>
where T: PartialEq + Eq + Hash + Clone
{
    fn new() -> Self {
        Self {
            index_offset: 0,
            key_to_item: HashMap::<i64, T>::new(),
            item_to_key: HashMap::<T, i64>::new(),
        }
    }

    fn len(&self) -> usize {
        self.key_to_item.len()
    }

    fn is_empty(&self) -> bool {
        self.key_to_item.is_empty()
    }

    fn push(&mut self, item: T) -> usize {
        let index = self.len();
        let key = index_to_key(self.index_offset, index);
        self.key_to_item.insert(key, item.clone());
        self.item_to_key.insert(item, key);
        index
    }

    fn get(&self, index: usize) -> Option<T> {
        let key = index_to_key(self.index_offset, index);
        self.key_to_item.get(&key).map(Clone::clone)
    }

    // fn get_or_panic(&self, index: usize) -> T {
    //     self.get(index).unwrap()
    // }

    fn clear(&mut self) {
        self.key_to_item.clear();
        self.item_to_key.clear();
    }

    fn position(&self, item: &T) -> Option<usize> {
        self.item_to_key
            .get(item)
            .map(|key| key_to_index(self.index_offset, *key))
    }

    fn truncate(&mut self, len: usize) {
        for index in len..self.len() {
            let key = index_to_key(self.index_offset, index);
            if let Some(item) = self.key_to_item.remove(&key) {
                self.item_to_key.remove(&item);
            }
        }
    }

    fn shift(&mut self, n: usize) {
        let num_drain = min(n, self.len());
        for index in 0..num_drain {
            let key = index_to_key(self.index_offset, index);
            if let Some(item) = self.key_to_item.remove(&key) {
                self.item_to_key.remove(&item);
            }
        }
        self.index_offset += num_drain as i64;
    }

    fn push_front(&mut self, item: T) {
        let key = self.index_offset - 1;
        self.index_offset = key;
        self.key_to_item.insert(key, item.clone());
        self.item_to_key.insert(item, key);
    }

    #[cfg(test)]
    fn check_state(&self) {
        assert_eq!(self.key_to_item.len(), self.item_to_key.len());
        for index in 0..self.key_to_item.len() {
            let key = index_to_key(self.index_offset, index);
            let item = self
                .key_to_item
                .get(&key)
                .map(Clone::clone)
                .expect("Missing key to item mapping");
            let stored_key = self
                .item_to_key
                .get(&item)
                .map(Clone::clone)
                .expect("Missing item to key mapping");
            assert_eq!(key, stored_key);
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemDbVec<T>
where T: PartialEq + Eq + Hash + Clone
{
    storage: Arc<RwLock<MemDbVecStorage<T>>>,
}

impl<T> MemDbVec<T>
where T: PartialEq + Eq + Hash + Clone
{
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(MemDbVecStorage::<T>::new())),
        }
    }

    #[cfg(test)]
    pub fn check_state(&self) {
        self.storage.read().expect("Storage lock poisoned").check_state();
    }
}

// Combine the index offset and array index to create a db key.
fn index_to_key(offset: i64, index: usize) -> i64 {
    offset + index as i64
}

// Convert a db key to the array index.
fn key_to_index(offset: i64, key: i64) -> usize {
    let index = key - offset;
    if index >= 0 {
        index as usize
    } else {
        0
    }
}

impl<T> ArrayLike for MemDbVec<T>
where T: PartialEq + Eq + Hash + Clone
{
    type Error = ChainStorageError;
    type Value = T;

    fn len(&self) -> Result<usize, Self::Error> {
        Ok(self
            .storage
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .len())
    }

    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self
            .storage
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .is_empty())
    }

    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error> {
        Ok(self
            .storage
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .push(item))
    }

    fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error> {
        Ok(self
            .storage
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .get(index))
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.storage
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .clear();
        Ok(())
    }

    fn position(&self, item: &Self::Value) -> Result<Option<usize>, Self::Error> {
        Ok(self
            .storage
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .position(item))
    }
}

impl<T> ArrayLikeExt for MemDbVec<T>
where T: PartialEq + Eq + Hash + Clone
{
    type Value = T;

    fn truncate(&mut self, len: usize) -> Result<(), MerkleMountainRangeError> {
        self.storage
            .write()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .truncate(len);
        Ok(())
    }

    fn shift(&mut self, n: usize) -> Result<(), MerkleMountainRangeError> {
        self.storage
            .write()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .shift(n);
        Ok(())
    }

    fn push_front(&mut self, item: Self::Value) -> Result<(), MerkleMountainRangeError> {
        self.storage
            .write()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .push_front(item);
        Ok(())
    }

    fn for_each<F>(&self, mut f: F) -> Result<(), MerkleMountainRangeError>
    where F: FnMut(Result<Self::Value, MerkleMountainRangeError>) {
        let db = self
            .storage
            .read()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?;
        for index in 0..db.len() {
            let val = db
                .get(index)
                .ok_or_else(|| MerkleMountainRangeError::BackendError("Unexpected error".into()))?;
            f(Ok(val))
        }
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
        mem_vec
            .iter()
            .enumerate()
            .for_each(|(i, val)| assert_eq!(db_vec.get(i).unwrap(), Some(val.clone())));

        assert!(mem_vec.push_front(200).is_ok());
        assert!(mem_vec.push_front(100).is_ok());
        assert!(db_vec.push_front(200).is_ok());
        assert!(db_vec.push_front(100).is_ok());
        assert_eq!(db_vec.len().unwrap(), 4);
        mem_vec
            .iter()
            .enumerate()
            .for_each(|(i, val)| assert_eq!(db_vec.get(i).unwrap(), Some(val.clone())));

        for index in 0..db_vec.len().unwrap() {
            let item = db_vec.get(index).unwrap().unwrap();
            assert_eq!(db_vec.position(&item).unwrap(), Some(index));
        }

        assert!(db_vec.clear().is_ok());
        assert_eq!(db_vec.len().unwrap(), 0);

        db_vec.check_state();
    }
}
