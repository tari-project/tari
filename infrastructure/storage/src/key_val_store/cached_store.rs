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

use std::{
    collections::HashMap,
    hash::Hash,
    sync::{
        RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{IterationResult, KeyValStoreError, KeyValueStore};

pub struct CachedStore<K, V, DS> {
    cache: RwLock<HashMap<K, V>>,
    filled: AtomicBool,
    actual_store: DS,
}

impl<K: Eq + Hash, V, DS: KeyValueStore<K, V>> CachedStore<K, V, DS> {
    pub fn new(inner: DS) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            filled: AtomicBool::new(false),
            actual_store: inner,
        }
    }

    fn ensure_cache_is_filled(&self) -> Result<(), KeyValStoreError> {
        if !self.filled.load(Ordering::Acquire) {
            let mut guard = self.cache.write().map_err(|_| KeyValStoreError::PoisonedAccess)?;
            if !self.filled.load(Ordering::Relaxed) {
                let mut inner_err = None;
                self.actual_store.for_each(|item| match item {
                    Ok((k, v)) => {
                        guard.insert(k, v);
                        IterationResult::Continue
                    },
                    Err(e) => {
                        inner_err = Some(e);
                        IterationResult::Break
                    },
                })?;
                if let Some(e) = inner_err {
                    guard.clear();
                    return Err(e);
                }
                self.filled.store(true, Ordering::Release);
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
        self.actual_store.insert(key.clone(), value.clone())?;
        let mut guard = self.cache.write().map_err(|_| KeyValStoreError::PoisonedAccess)?;
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
        self.actual_store.delete(key)?;
        let mut write_guard = self.cache.write().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        write_guard.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::HashmapDatabase;

    #[test]
    fn cached_store_for_each_respects_break_and_fills_once() {
        struct CountingStore {
            inner: HashmapDatabase<u32, String>,
            for_each_calls: AtomicUsize,
        }

        impl KeyValueStore<u32, String> for CountingStore {
            fn insert(&self, key: u32, value: String) -> Result<(), KeyValStoreError> {
                self.inner.insert(key, value)
            }

            fn get(&self, key: &u32) -> Result<Option<String>, KeyValStoreError> {
                self.inner.get(key)
            }

            fn get_many(&self, keys: &[u32]) -> Result<Vec<String>, KeyValStoreError> {
                self.inner.get_many(keys)
            }

            fn size(&self) -> Result<usize, KeyValStoreError> {
                self.inner.size()
            }

            fn for_each<F>(&self, f: F) -> Result<(), KeyValStoreError>
            where F: FnMut(Result<(u32, String), KeyValStoreError>) -> IterationResult {
                self.for_each_calls.fetch_add(1, Ordering::SeqCst);
                self.inner.for_each(f)
            }

            fn exists(&self, key: &u32) -> Result<bool, KeyValStoreError> {
                self.inner.exists(key)
            }

            fn delete(&self, key: &u32) -> Result<(), KeyValStoreError> {
                self.inner.delete(key)
            }
        }

        let store = CachedStore::new(CountingStore {
            inner: HashmapDatabase::new(),
            for_each_calls: AtomicUsize::new(0),
        });

        // Empty store fills once and does not re-scan on subsequent reads
        assert_eq!(store.get(&1).unwrap(), None);
        assert_eq!(store.get(&2).unwrap(), None);
        assert_eq!(store.actual_store.for_each_calls.load(Ordering::SeqCst), 1);

        store.insert(1, "a".to_string()).unwrap();
        store.insert(2, "b".to_string()).unwrap();
        store.insert(3, "c".to_string()).unwrap();

        let mut visited = 0;
        store
            .for_each(|_| {
                visited += 1;
                IterationResult::Break
            })
            .unwrap();
        assert_eq!(visited, 1);

        // Deleting a non-existent key fails without mutating the cache
        assert!(store.delete(&99).is_err());
        assert_eq!(store.size().unwrap(), 3);
    }
}
