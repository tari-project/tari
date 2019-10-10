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

use crate::{
    key_val_store::{
        key_val_store::{IterationResult, KeyValueStore},
        KeyValStoreError,
    },
    lmdb_store::LMDBDatabase,
};
use lmdb_zero::traits::AsLmdbBytes;
use serde::{de::DeserializeOwned, export::PhantomData, Serialize};
use std::sync::Arc;

/// This is a simple wrapper struct that lifts the generic parameters so that KeyValStore can be implemented on
/// LMDBDatabase. LMDBDatabase doesn't have the generics at the struct level because the LMDBStore can contain many
/// instances of LMDBDatabase each with different K and V types.
pub struct LMDBWrapper<K, V> {
    inner: Arc<LMDBDatabase>,
    _k: PhantomData<K>,
    _v: PhantomData<V>,
}

impl<K, V> LMDBWrapper<K, V> {
    /// Wrap a LMDBDatabase instance so that it implements [KeyValueStore]
    pub fn new(db: Arc<LMDBDatabase>) -> LMDBWrapper<K, V> {
        LMDBWrapper {
            inner: db,
            _k: PhantomData,
            _v: PhantomData,
        }
    }

    /// Get access to the underlying LMDB database
    pub fn inner(&self) -> Arc<LMDBDatabase> {
        Arc::clone(&self.inner)
    }
}

impl<K, V> KeyValueStore<K, V> for LMDBWrapper<K, V>
where
    K: AsLmdbBytes + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    /// Inserts a key-value pair into the key-value database.
    fn insert(&self, key: K, value: V) -> Result<(), KeyValStoreError> {
        self.inner
            .insert::<K, V>(&key, &value)
            .map_err(|e| KeyValStoreError::DatabaseError(e.to_string()))
    }

    /// Get the value corresponding to the provided key from the key-value database.
    fn get(&self, key: &K) -> Result<Option<V>, KeyValStoreError>
    where for<'t> V: serde::de::DeserializeOwned {
        self.inner
            .get::<K, V>(key)
            .map_err(|e| KeyValStoreError::DatabaseError(e.to_string()))
    }

    /// Returns the total number of entries recorded in the key-value database.
    fn size(&self) -> Result<usize, KeyValStoreError> {
        self.inner
            .len()
            .map_err(|e| KeyValStoreError::DatabaseError(e.to_string()))
    }

    /// Iterate over all the stored records and execute the function `f` for each pair in the key-value database.
    fn for_each<F>(&self, f: F) -> Result<(), KeyValStoreError>
    where F: FnMut(Result<(K, V), KeyValStoreError>) -> IterationResult {
        self.inner
            .for_each::<K, V, F>(f)
            .map_err(|e| KeyValStoreError::DatabaseError(e.to_string()))
    }

    /// Checks whether a record exist in the key-value database that corresponds to the provided `key`.
    fn exists(&self, key: &K) -> Result<bool, KeyValStoreError> {
        self.inner
            .contains_key::<K>(key)
            .map_err(|e| KeyValStoreError::DatabaseError(e.to_string()))
    }

    /// Remove the record from the key-value database that corresponds with the provided `key`.
    fn delete(&self, key: &K) -> Result<(), KeyValStoreError> {
        self.inner
            .remove::<K>(key)
            .map_err(|e| KeyValStoreError::DatabaseError(e.to_string()))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::lmdb_store::{LMDBBuilder, LMDBError, LMDBStore};
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    fn get_path(name: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/data");
        path.push(name);
        path.to_str().unwrap().to_string()
    }

    fn init_datastore(name: &str) -> Result<LMDBStore, LMDBError> {
        let path = get_path(name);
        let _ = std::fs::create_dir(&path).unwrap_or_default();
        LMDBBuilder::new()
            .set_path(&path)
            .set_environment_size(10)
            .set_max_number_of_databases(2)
            .add_database(name, lmdb_zero::db::CREATE)
            .build()
    }

    fn clean_up_datastore(name: &str) {
        std::fs::remove_dir_all(get_path(name)).unwrap();
    }

    #[test]
    fn test_lmdb_kvstore() {
        let database_name = "test_lmdb_kvstore"; // Note: every test should have unique database
        let datastore = init_datastore(database_name).unwrap();
        let db = datastore.get_handle(database_name).unwrap();
        let db = LMDBWrapper::new(Arc::new(db));
        #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
        struct Foo {
            value: String,
        }
        let key1 = 1 as u64;
        let key2 = 2 as u64;
        let key3 = 3 as u64;
        let key4 = 4 as u64;
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
        assert!(db.exists(&key1).unwrap());
        assert!(db.exists(&key2).unwrap());
        assert!(db.exists(&key3).unwrap());
        assert!(!db.exists(&key4).unwrap());

        db.delete(&key2).unwrap();
        assert_eq!(db.get(&key1).unwrap().unwrap(), val1);
        assert!(db.get(&key2).unwrap().is_none());
        assert_eq!(db.get(&key3).unwrap().unwrap(), val3);
        assert!(db.get(&key4).unwrap().is_none());
        assert_eq!(db.size().unwrap(), 2);
        assert!(db.exists(&key1).unwrap());
        assert!(!db.exists(&key2).unwrap());
        assert!(db.exists(&key3).unwrap());
        assert!(!db.exists(&key4).unwrap());

        // Only Key1 and Key3 should be in key-value database, but order is not known
        let mut key1_found = false;
        let mut key3_found = false;
        let _res = db.for_each(|pair| {
            let (key, val) = pair.unwrap();
            if key == key1 {
                key1_found = true;
                assert_eq!(val, val1);
            } else if key == key3 {
                key3_found = true;
                assert_eq!(val, val3);
            }
            IterationResult::Continue
        });
        assert!(key1_found);
        assert!(key3_found);

        clean_up_datastore(database_name);
    }
}
