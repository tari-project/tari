//  Copyright 2019 The Tari Project
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

use crate::key_val_store::{error::KeyValStoreError, key_val_store::KeyValueStore};
use std::{collections::HashMap, hash::Hash, sync::RwLock};

///  The HMapDatabase mimics the behaviour of LMDBDatabase without keeping a persistent copy of the key-value records.
/// It allows key-value pairs to be inserted, retrieved and removed in a thread-safe manner.
#[derive(Default)]
pub struct HMapDatabase<K: Eq + Hash, V> {
    db: RwLock<HashMap<K, V>>,
}

impl<K: Clone + Eq + Hash, V: Clone> HMapDatabase<K, V> {
    /// Creates a new empty HMapDatabase with the specified name
    pub fn new() -> Self {
        Self {
            db: RwLock::new(HashMap::new()),
        }
    }

    /// Inserts a key-value record into the database. Internally, `insert` serializes the key and value using bincode
    /// and adds the pair into HashMap guarded with a RwLock.
    pub fn insert(&self, key: K, value: V) -> Result<(), KeyValStoreError> {
        self.db
            .write()
            .map_err(|_| KeyValStoreError::PoisonedAccess)?
            .insert(key, value);
        Ok(())
    }

    /// Get a value from the key-value database. The retrieved value is deserialized from bincode into `V`
    pub fn get(&self, key: &K) -> Result<Option<V>, KeyValStoreError> {
        match self.db.read().map_err(|_| KeyValStoreError::PoisonedAccess)?.get(key) {
            Some(val) => Ok(Some(val.clone())),
            None => Ok(None),
        }
    }

    /// Returns if the  key-value database is empty
    pub fn is_empty(&self) -> Result<bool, KeyValStoreError> {
        Ok(self.db.read().map_err(|_| KeyValStoreError::PoisonedAccess)?.is_empty())
    }

    /// Returns the total number of entries recorded in the key-value database.
    pub fn len(&self) -> Result<usize, KeyValStoreError> {
        Ok(self.db.read().map_err(|_| KeyValStoreError::PoisonedAccess)?.len())
    }

    /// Iterate over all the stored records and execute the function `f` for each pair in the key-value database.
    pub fn for_each<F>(&self, mut f: F) -> Result<(), KeyValStoreError>
    where F: FnMut(Result<(K, V), KeyValStoreError>) {
        for (key, val) in self.db.read().map_err(|_| KeyValStoreError::PoisonedAccess)?.iter() {
            f(Ok((key.clone(), val.clone())));
        }
        Ok(())
    }

    /// Checks whether a record exist in the key-value database that corresponds to the provided `key`.
    pub fn contains_key(&self, key: &K) -> Result<bool, KeyValStoreError> {
        Ok(self
            .db
            .read()
            .map_err(|_| KeyValStoreError::PoisonedAccess)?
            .contains_key(key))
    }

    /// Remove the record from the key-value database that corresponds with the provided `key`.
    pub fn remove(&self, key: &K) -> Result<(), KeyValStoreError> {
        match self
            .db
            .write()
            .map_err(|_| KeyValStoreError::PoisonedAccess)?
            .remove(key)
        {
            Some(_) => Ok(()),
            None => Err(KeyValStoreError::KeyNotFound),
        }
    }
}

impl<K: Clone + Eq + Hash, V: Clone> KeyValueStore<K, V> for HMapDatabase<K, V> {
    /// Inserts a key-value pair into the key-value database.
    fn insert(&self, key: K, value: V) -> Result<(), KeyValStoreError> {
        self.insert(key, value)
    }

    /// Get the value corresponding to the provided key from the key-value database.
    fn get(&self, key: &K) -> Result<Option<V>, KeyValStoreError> {
        self.get(key)
    }

    /// Returns the total number of entries recorded in the key-value database.
    fn size(&self) -> Result<usize, KeyValStoreError> {
        self.len()
    }

    /// Iterate over all the stored records and execute the function `f` for each pair in the key-value database.
    fn for_each<F>(&self, f: F) -> Result<(), KeyValStoreError>
    where F: FnMut(Result<(K, V), KeyValStoreError>) {
        self.for_each(f)
    }

    /// Checks whether a record exist in the key-value database that corresponds to the provided `key`.
    fn exists(&self, key: &K) -> Result<bool, KeyValStoreError> {
        self.contains_key(key)
    }

    /// Remove the record from the key-value database that corresponds with the provided `key`.
    fn delete(&self, key: &K) -> Result<(), KeyValStoreError> {
        self.remove(key)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[test]
    fn test_hmap_kvstore() {
        let db = HMapDatabase::new();

        #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
        struct Foo {
            value: String,
        }

        let val1 = Foo {
            value: "one".to_string(),
        };
        let val2 = Foo {
            value: "two".to_string(),
        };
        let val3 = Foo {
            value: "three".to_string(),
        };

        db.insert(1, val1.clone()).unwrap();
        db.insert(2, val2.clone()).unwrap();
        db.insert(3, val3.clone()).unwrap();

        assert_eq!(db.get(&1).unwrap().unwrap(), val1);
        assert_eq!(db.get(&2).unwrap().unwrap(), val2);
        assert_eq!(db.get(&3).unwrap().unwrap(), val3);
        assert!(db.get(&4).unwrap().is_none());
        assert_eq!(db.size().unwrap(), 3);
        assert!(db.exists(&1).unwrap());
        assert!(db.exists(&2).unwrap());
        assert!(db.exists(&3).unwrap());
        assert!(!db.exists(&4).unwrap());

        db.remove(&2).unwrap();
        assert_eq!(db.get(&1).unwrap().unwrap(), val1);
        assert!(db.get(&2).unwrap().is_none());
        assert_eq!(db.get(&3).unwrap().unwrap(), val3);
        assert!(db.get(&4).unwrap().is_none());
        assert_eq!(db.size().unwrap(), 2);
        assert!(db.exists(&1).unwrap());
        assert!(!db.exists(&2).unwrap());
        assert!(db.exists(&3).unwrap());
        assert!(!db.exists(&4).unwrap());

        // Only Key1 and Key3 should be in key-value database, but order is not known
        let mut key1_found = false;
        let mut key3_found = false;
        let _res = db.for_each(|pair| {
            let (key, val) = pair.unwrap();
            if key == 1 {
                key1_found = true;
                assert_eq!(val, val1);
            } else if key == 3 {
                key3_found = true;
                assert_eq!(val, val3);
            }
        });
        assert!(key1_found);
        assert!(key3_found);
    }
}
