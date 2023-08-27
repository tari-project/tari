//  Copyright 2022. The Taiji Project
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
            // Drop here or we can't get a read lock
            drop(empty_check_guard);
            let mut guard = self.cache.write().map_err(|_| KeyValStoreError::PoisonedAccess)?;
            // fill cache
            self.actual_store.for_each(|item| match item {
                Ok((k, v)) => {
                    guard.insert(k, v);
                    IterationResult::Continue
                },
                Err(_) => IterationResult::Break,
            })?;
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
        guard.insert(key.clone(), value.clone());
        drop(guard);
        self.actual_store.insert(key, value)?;
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
            f(Ok((k, v)));
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
        write_guard.remove(key);
        drop(write_guard);
        self.actual_store.delete(key)
    }
}
