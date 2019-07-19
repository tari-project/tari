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

use crate::key_val_store::{error::KeyValStoreError, key_val_store::KeyValStore};
use lmdb_zero::traits::AsLmdbBytes;
use std::{collections::HashMap, sync::RwLock};

///  The HMapDatabase mimics the behaviour of LMDBDatabase without keeping a persistent copy of the key-value records.
/// It allows key-value pairs to be inserted, retrieved and removed in a thread-safe manner.
#[derive(Default)]
pub struct HMapDatabase {
    db: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
}

impl HMapDatabase {
    /// Creates a new empty HMapDatabase with the specified name
    pub fn new() -> Self {
        Self {
            db: RwLock::new(HashMap::new()),
        }
    }

    /// Inserts a key-value record into the database. Internally, `insert` serializes the key and value using bincode
    /// and adds the pair into HashMap guarded with a RwLock.
    pub fn insert<K, V>(&self, key: &K, value: &V) -> Result<(), KeyValStoreError>
    where
        K: AsLmdbBytes + ?Sized,
        V: serde::Serialize,
    {
        let mut value_buf = Vec::with_capacity(512);
        bincode::serialize_into(&mut value_buf, value)
            .map_err(|e| KeyValStoreError::SerializationError(e.to_string()))?;

        self.db
            .write()
            .map_err(|_| KeyValStoreError::PoisonedAccess)?
            .insert(key.as_lmdb_bytes().to_vec(), value_buf);
        Ok(())
    }

    /// Get a value from the key-value database. The retrieved value is deserialized from bincode into `V`
    pub fn get<K, V>(&self, key: &K) -> Result<Option<V>, KeyValStoreError>
    where
        K: AsLmdbBytes + ?Sized,
        for<'t> V: serde::de::DeserializeOwned, // read this as, for *any* lifetime, t, we can convert a [u8] to V
    {
        match self
            .db
            .read()
            .map_err(|_| KeyValStoreError::PoisonedAccess)?
            .get(&key.as_lmdb_bytes().to_vec())
        {
            Some(val_buf) => match bincode::deserialize(val_buf) {
                Ok(val) => Ok(Some(val)),
                Err(_) => Ok(None),
            },
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
    pub fn for_each<K, V, F>(&self, mut f: F) -> Result<(), KeyValStoreError>
    where
        K: serde::de::DeserializeOwned,
        V: serde::de::DeserializeOwned,
        F: FnMut(Result<(K, V), KeyValStoreError>),
    {
        for (key_buf, val_buf) in self
            .db
            .read()
            .map_err(|_| KeyValStoreError::PoisonedAccess)?
            .clone()
            .into_iter()
        {
            let key =
                bincode::deserialize(&key_buf).map_err(|e| KeyValStoreError::DeserializationError(e.to_string()))?;
            let val =
                bincode::deserialize(&val_buf).map_err(|e| KeyValStoreError::DeserializationError(e.to_string()))?;

            f(Ok((key, val)));
        }

        Ok(())
    }

    /// Checks whether a record exist in the key-value database that corresponds to the provided `key`.
    pub fn contains_key<K>(&self, key: &K) -> Result<bool, KeyValStoreError>
    where K: AsLmdbBytes + ?Sized {
        Ok(self
            .db
            .read()
            .map_err(|_| KeyValStoreError::PoisonedAccess)?
            .contains_key(&key.as_lmdb_bytes().to_vec()))
    }

    /// Remove the record from the key-value database that corresponds with the provided `key`.
    pub fn remove<K>(&self, key: &K) -> Result<(), KeyValStoreError>
    where K: AsLmdbBytes + ?Sized {
        match self
            .db
            .write()
            .map_err(|_| KeyValStoreError::PoisonedAccess)?
            .remove(&key.as_lmdb_bytes().to_vec())
        {
            Some(_) => Ok(()),
            None => Err(KeyValStoreError::KeyNotFound),
        }
    }
}

impl KeyValStore for HMapDatabase {
    /// Inserts a key-value pair into the key-value database.
    fn insert_pair<K, V>(&self, key: &K, value: &V) -> Result<(), KeyValStoreError>
    where
        K: AsLmdbBytes + ?Sized,
        V: serde::Serialize,
    {
        self.insert::<K, V>(key, value)
    }

    /// Get the value corresponding to the provided key from the key-value database.
    fn get_value<K, V>(&self, key: &K) -> Result<Option<V>, KeyValStoreError>
    where
        K: AsLmdbBytes + ?Sized,
        for<'t> V: serde::de::DeserializeOwned,
    {
        self.get::<K, V>(key)
    }

    /// Returns the total number of entries recorded in the key-value database.
    fn size(&self) -> Result<usize, KeyValStoreError> {
        self.len()
    }

    /// Iterate over all the stored records and execute the function `f` for each pair in the key-value database.
    fn for_each<K, V, F>(&self, f: F) -> Result<(), KeyValStoreError>
    where
        K: serde::de::DeserializeOwned,
        V: serde::de::DeserializeOwned,
        F: FnMut(Result<(K, V), KeyValStoreError>),
    {
        self.for_each::<K, V, F>(f)
    }

    /// Checks whether a record exist in the key-value database that corresponds to the provided `key`.
    fn exists<K>(&self, key: &K) -> Result<bool, KeyValStoreError>
    where K: AsLmdbBytes + ?Sized {
        self.contains_key::<K>(key)
    }

    /// Remove the record from the key-value database that corresponds with the provided `key`.
    fn delete<K>(&self, key: &K) -> Result<(), KeyValStoreError>
    where K: AsLmdbBytes + ?Sized {
        self.remove::<K>(key)
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
        struct R {
            value: String,
        }
        let key1 = 1 as u64;
        let key2 = 2 as u64;
        let key3 = 3 as u64;
        let key4 = 4 as u64;
        let val1 = R {
            value: "one".to_string(),
        };
        let val2 = R {
            value: "two".to_string(),
        };
        let val3 = R {
            value: "three".to_string(),
        };
        db.insert_pair(&key1, &val1).unwrap();
        db.insert_pair(&key2, &val2).unwrap();
        db.insert_pair(&key3, &val3).unwrap();

        assert_eq!(db.get_value::<u64, R>(&key1).unwrap().unwrap(), val1);
        assert_eq!(db.get_value::<u64, R>(&key2).unwrap().unwrap(), val2);
        assert_eq!(db.get_value::<u64, R>(&key3).unwrap().unwrap(), val3);
        assert!(db.get_value::<u64, R>(&key4).unwrap().is_none());
        assert_eq!(db.size().unwrap(), 3);
        assert!(db.exists(&key1).unwrap());
        assert!(db.exists(&key2).unwrap());
        assert!(db.exists(&key3).unwrap());
        assert!(!db.exists(&key4).unwrap());

        db.remove(&key2).unwrap();
        assert_eq!(db.get_value::<u64, R>(&key1).unwrap().unwrap(), val1);
        assert!(db.get_value::<u64, R>(&key2).unwrap().is_none());
        assert_eq!(db.get_value::<u64, R>(&key3).unwrap().unwrap(), val3);
        assert!(db.get_value::<u64, R>(&key4).unwrap().is_none());
        assert_eq!(db.size().unwrap(), 2);
        assert!(db.exists(&key1).unwrap());
        assert!(!db.exists(&key2).unwrap());
        assert!(db.exists(&key3).unwrap());
        assert!(!db.exists(&key4).unwrap());

        // Only Key1 and Key3 should be in key-value database, but order is not known
        let mut key1_found = false;
        let mut key3_found = false;
        let _res = db.for_each::<u64, R, _>(|pair| {
            let (key, val) = pair.unwrap();
            if key == key1 {
                key1_found = true;
                assert_eq!(val, val1);
            } else if key == key3 {
                key3_found = true;
                assert_eq!(val, val3);
            }
        });
        assert!(key1_found);
        assert!(key3_found);
    }
}
