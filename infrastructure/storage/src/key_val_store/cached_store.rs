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

use std::{collections::HashMap, hash::Hash, sync::RwLock};

use crate::{IterationResult, KeyValStoreError, KeyValueStore};

pub struct CachedStore<K, V, DS> {
    cache: RwLock<HashMap<K, V>>,

    actual_store: DS,
}

impl<K: Eq + Hash, V, DS: KeyValueStore<K, V>> CachedStore<K, V, DS> {
    pub fn new(inner: DS) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            actual_store: inner,
        }
    }

    fn ensure_cache_is_filled(&self) -> Result<(), KeyValStoreError> {
        let empty_check_guard = self.cache.read().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        if empty_check_guard.is_empty() {
            // Drop here or we can't get a write lock
            drop(empty_check_guard);
            let mut guard = self.cache.write().map_err(|_| KeyValStoreError::PoisonedAccess)?;
            if guard.is_empty() {
                let mut iter_err = None;
                // fill cache
                self.actual_store.for_each(|item| match item {
                    Ok((k, v)) => {
                        guard.insert(k, v);
                        IterationResult::Continue
                    },
                    Err(e) => {
                        iter_err = Some(e);
                        IterationResult::Break
                    },
                })?;
                if let Some(err) = iter_err {
                    guard.clear();
                    return Err(err);
                }
            }
        }
        Ok(())
    }
}

impl<K: Eq + Hash + Clone, V: Clone, DS> KeyValueStore<K, V> for CachedStore<K, V, DS>
where DS: KeyValueStore<K, V>
{
    fn insert(&self, key: K, value: V) -> Result<(), KeyValStoreError> {
        self.ensure_cache_is_filled()?;
        let mut guard = self.cache.write().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        self.actual_store.insert(key.clone(), value.clone())?;
        guard.insert(key, value);
        Ok(())
    }

    fn get(&self, key: &K) -> Result<Option<V>, KeyValStoreError> {
        self.ensure_cache_is_filled()?;
        let read_lock = self.cache.read().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        Ok(read_lock.get(key).cloned())
    }

    fn get_many(&self, keys: &[K]) -> Result<Vec<V>, KeyValStoreError> {
        let mut result = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(value) = self.get(key)? {
                result.push(value);
            }
        }
        Ok(result)
    }

    fn size(&self) -> Result<usize, KeyValStoreError> {
        self.ensure_cache_is_filled()?;
        let read_guard = self.cache.read().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        Ok(read_guard.len())
    }

    fn for_each<F>(&self, mut f: F) -> Result<(), KeyValStoreError>
    where
        Self: Sized,
        F: FnMut(Result<(K, V), KeyValStoreError>) -> IterationResult,
    {
        self.ensure_cache_is_filled()?;
        let read_guard = self.cache.read().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        let vec = read_guard
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();
        drop(read_guard);
        for (k, v) in vec {
            if matches!(f(Ok((k, v))), IterationResult::Break) {
                break;
            }
        }
        Ok(())
    }

    fn exists(&self, key: &K) -> Result<bool, KeyValStoreError> {
        self.ensure_cache_is_filled()?;
        let read_guard = self.cache.read().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        Ok(read_guard.contains_key(key))
    }

    fn delete(&self, key: &K) -> Result<(), KeyValStoreError> {
        self.ensure_cache_is_filled()?;
        let mut write_guard = self.cache.write().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        self.actual_store.delete(key)?;
        write_guard.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::HashmapDatabase;

    #[test]
    fn cached_store_honors_for_each_break_and_keeps_cache_consistent() {
        let inner = HashmapDatabase::new();
        inner.insert(1u32, "one".to_string()).unwrap();
        inner.insert(2u32, "two".to_string()).unwrap();
        inner.insert(3u32, "three".to_string()).unwrap();

        let store = CachedStore::new(inner);
        assert_eq!(store.size().unwrap(), 3);

        let mut visited = 0usize;
        store
            .for_each(|res| {
                assert!(res.is_ok());
                visited += 1;
                IterationResult::Break
            })
            .unwrap();
        assert_eq!(visited, 1);

        // Deleting a non-existent key fails in HashmapDatabase and keeps existing cache intact
        assert!(store.delete(&99).is_err());
        assert_eq!(store.size().unwrap(), 3);

        store.delete(&2).unwrap();
        assert!(!store.exists(&2).unwrap());
        assert_eq!(store.size().unwrap(), 2);
    }

    struct FailingIterationStore;
    impl KeyValueStore<u32, String> for FailingIterationStore {
        fn insert(&self, _key: u32, _value: String) -> Result<(), KeyValStoreError> {
            Err(KeyValStoreError::DatabaseError("insert failed".to_string()))
        }

        fn get(&self, _key: &u32) -> Result<Option<String>, KeyValStoreError> {
            Ok(None)
        }

        fn get_many(&self, _keys: &[u32]) -> Result<Vec<String>, KeyValStoreError> {
            Ok(Vec::new())
        }

        fn size(&self) -> Result<usize, KeyValStoreError> {
            Ok(0)
        }

        fn for_each<F>(&self, mut f: F) -> Result<(), KeyValStoreError>
        where
            Self: Sized,
            F: FnMut(Result<(u32, String), KeyValStoreError>) -> IterationResult,
        {
            let _ = f(Ok((1, "first".to_string())));
            let _ = f(Err(KeyValStoreError::KeyNotFound));
            Ok(())
        }

        fn exists(&self, _key: &u32) -> Result<bool, KeyValStoreError> {
            Ok(false)
        }

        fn delete(&self, _key: &u32) -> Result<(), KeyValStoreError> {
            Ok(())
        }
    }

    #[test]
    fn cached_store_propagates_for_each_item_error_and_clears_partial_cache() {
        let store = CachedStore::new(FailingIterationStore);
        assert!(matches!(store.get(&1), Err(KeyValStoreError::KeyNotFound)));
        assert!(store.cache.read().unwrap().is_empty());
    }
}
