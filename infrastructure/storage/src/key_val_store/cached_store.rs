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
    cache: RwLock<Option<HashMap<K, V>>>,
    actual_store: DS,
}

impl<K: Eq + Hash, V, DS: KeyValueStore<K, V>> CachedStore<K, V, DS> {
    pub fn new(inner: DS) -> Self {
        Self {
            cache: RwLock::new(None),
            actual_store: inner,
        }
    }

    fn ensure_cache_is_filled(&self) -> Result<(), KeyValStoreError> {
        let filled_check_guard = self.cache.read().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        if filled_check_guard.is_some() {
            return Ok(());
        }
        // Drop here or we can't get a write lock
        drop(filled_check_guard);
        let mut guard = self.cache.write().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        if guard.is_none() {
            let mut entries = HashMap::new();
            let mut iter_err = None;
            // fill cache
            self.actual_store.for_each(|item| match item {
                Ok((k, v)) => {
                    entries.insert(k, v);
                    IterationResult::Continue
                },
                Err(err) => {
                    iter_err = Some(err);
                    IterationResult::Break
                },
            })?;
            if let Some(err) = iter_err {
                return Err(err);
            }
            *guard = Some(entries);
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
        if let Some(entries) = guard.as_mut() {
            entries.insert(key, value);
        }
        Ok(())
    }

    fn get(&self, key: &K) -> Result<Option<V>, KeyValStoreError> {
        self.ensure_cache_is_filled()?;
        let read_lock = self.cache.read().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        Ok(read_lock.as_ref().and_then(|entries| entries.get(key)).cloned())
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
        Ok(read_guard.as_ref().map_or(0, HashMap::len))
    }

    fn for_each<F>(&self, mut f: F) -> Result<(), KeyValStoreError>
    where
        Self: Sized,
        F: FnMut(Result<(K, V), KeyValStoreError>) -> IterationResult,
    {
        self.ensure_cache_is_filled()?;
        let read_guard = self.cache.read().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        let vec = read_guard
            .as_ref()
            .map(|entries| entries.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>())
            .unwrap_or_default();
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
        Ok(read_guard.as_ref().is_some_and(|entries| entries.contains_key(key)))
    }

    fn delete(&self, key: &K) -> Result<(), KeyValStoreError> {
        self.ensure_cache_is_filled()?;
        let mut write_guard = self.cache.write().map_err(|_| KeyValStoreError::PoisonedAccess)?;
        self.actual_store.delete(key)?;
        if let Some(entries) = write_guard.as_mut() {
            entries.remove(key);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;
    use crate::HashmapDatabase;

    struct TestBackend {
        inner: HashmapDatabase<u64, String>,
        for_each_calls: AtomicUsize,
        fail_for_each_item: AtomicBool,
        fail_for_each_outer: AtomicBool,
        fail_insert: AtomicBool,
        fail_delete: AtomicBool,
    }

    impl TestBackend {
        fn new() -> Self {
            Self {
                inner: HashmapDatabase::new(),
                for_each_calls: AtomicUsize::new(0),
                fail_for_each_item: AtomicBool::new(false),
                fail_for_each_outer: AtomicBool::new(false),
                fail_insert: AtomicBool::new(false),
                fail_delete: AtomicBool::new(false),
            }
        }
    }

    impl KeyValueStore<u64, String> for TestBackend {
        fn insert(&self, key: u64, value: String) -> Result<(), KeyValStoreError> {
            if self.fail_insert.load(Ordering::SeqCst) {
                return Err(KeyValStoreError::DatabaseError("simulated insert failure".to_string()));
            }
            KeyValueStore::insert(&self.inner, key, value)
        }

        fn get(&self, key: &u64) -> Result<Option<String>, KeyValStoreError> {
            KeyValueStore::get(&self.inner, key)
        }

        fn get_many(&self, keys: &[u64]) -> Result<Vec<String>, KeyValStoreError> {
            KeyValueStore::get_many(&self.inner, keys)
        }

        fn size(&self) -> Result<usize, KeyValStoreError> {
            KeyValueStore::size(&self.inner)
        }

        fn for_each<F>(&self, mut f: F) -> Result<(), KeyValStoreError>
        where
            Self: Sized,
            F: FnMut(Result<(u64, String), KeyValStoreError>) -> IterationResult,
        {
            self.for_each_calls.fetch_add(1, Ordering::SeqCst);
            let fail_item = self.fail_for_each_item.load(Ordering::SeqCst);
            let fail_outer = self.fail_for_each_outer.load(Ordering::SeqCst);
            let mut emitted_one = false;
            KeyValueStore::for_each(&self.inner, |item| {
                if (fail_item || fail_outer) && emitted_one {
                    if fail_item {
                        return f(Err(KeyValStoreError::DeserializationError(
                            "corrupted record".to_string(),
                        )));
                    }
                    return IterationResult::Break;
                }
                emitted_one = true;
                f(item)
            })?;
            if fail_outer {
                return Err(KeyValStoreError::DatabaseError("outer iteration failure".to_string()));
            }
            Ok(())
        }

        fn exists(&self, key: &u64) -> Result<bool, KeyValStoreError> {
            KeyValueStore::exists(&self.inner, key)
        }

        fn delete(&self, key: &u64) -> Result<(), KeyValStoreError> {
            if self.fail_delete.load(Ordering::SeqCst) {
                return Err(KeyValStoreError::DatabaseError("simulated delete failure".to_string()));
            }
            KeyValueStore::delete(&self.inner, key)
        }
    }

    #[test]
    fn test_cached_store_for_each_break_and_filter_take() {
        let inner = HashmapDatabase::new();
        inner.insert(1u64, "one".to_string()).unwrap();
        inner.insert(2u64, "two".to_string()).unwrap();
        inner.insert(3u64, "three".to_string()).unwrap();

        let cached = CachedStore::new(inner);

        let mut visited = 0usize;
        cached
            .for_each(|_item| {
                visited += 1;
                IterationResult::Break
            })
            .unwrap();
        assert_eq!(visited, 1);

        let taken_zero = cached.filter_take(0, |_| true).unwrap();
        assert!(taken_zero.is_empty());

        let taken_one = cached.filter_take(1, |_| true).unwrap();
        assert_eq!(taken_one.len(), 1);

        let all = cached.filter(|_| true).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_cached_store_propagates_fill_errors_and_caches_empty_store() {
        let backend = TestBackend::new();
        let cached = CachedStore::new(backend);

        // Empty store should only scan `actual_store` once across repeated reads.
        assert_eq!(cached.size().unwrap(), 0);
        assert!(cached.get(&1).unwrap().is_none());
        assert!(!cached.exists(&1).unwrap());
        assert_eq!(cached.actual_store.for_each_calls.load(Ordering::SeqCst), 1);

        // Item-level error during initial cache fill is propagated and does not leave partial state.
        let backend_with_err = TestBackend::new();
        backend_with_err.inner.insert(1, "one".to_string()).unwrap();
        backend_with_err.inner.insert(2, "two".to_string()).unwrap();
        backend_with_err.fail_for_each_item.store(true, Ordering::SeqCst);

        let cached_err = CachedStore::new(backend_with_err);
        let err = cached_err.size().unwrap_err();
        assert!(matches!(err, KeyValStoreError::DeserializationError(_)));

        // Outer `for_each` error after emitting a record also leaves the cache unfilled rather than partially
        // populated.
        cached_err
            .actual_store
            .fail_for_each_item
            .store(false, Ordering::SeqCst);
        cached_err
            .actual_store
            .fail_for_each_outer
            .store(true, Ordering::SeqCst);
        let err = cached_err.size().unwrap_err();
        assert!(matches!(err, KeyValStoreError::DatabaseError(_)));

        // Once the underlying error is cleared, a subsequent access fills the full cache cleanly.
        cached_err
            .actual_store
            .fail_for_each_outer
            .store(false, Ordering::SeqCst);
        assert_eq!(cached_err.size().unwrap(), 2);
    }

    #[test]
    fn test_cached_store_insert_failure_does_not_pollute_cache() {
        let backend = TestBackend::new();
        backend.inner.insert(1, "one".to_string()).unwrap();
        let cached = CachedStore::new(backend);
        assert_eq!(cached.size().unwrap(), 1);

        cached.actual_store.fail_insert.store(true, Ordering::SeqCst);
        assert!(cached.insert(2, "two".to_string()).is_err());

        // Key 2 must not appear in the in-memory cache after the backend insert failed.
        assert!(!cached.exists(&2).unwrap());
        assert!(cached.get(&2).unwrap().is_none());
        assert_eq!(cached.size().unwrap(), 1);

        // Deleting an existing cached key when backend deletion fails must leave the cached record intact.
        cached.actual_store.fail_delete.store(true, Ordering::SeqCst);
        assert!(cached.delete(&1).is_err());
        assert!(cached.exists(&1).unwrap());
        assert_eq!(cached.get(&1).unwrap().as_deref(), Some("one"));
        assert_eq!(cached.size().unwrap(), 1);
    }
}
