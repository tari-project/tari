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

use crate::key_val_store::KeyValStoreError;

/// Used to indicate whether an iteration should continue or break (i.e not called again)
pub enum IterationResult {
    /// Continue the iteration
    Continue,
    /// Stop the iteration (i.e break)
    Break,
}

impl Default for IterationResult {
    fn default() -> Self {
        IterationResult::Continue
    }
}

/// General CRUD behaviour of Key-value store implementations.
pub trait KeyValueStore<K, V> {
    /// Inserts a key-value pair into the key-value database.
    fn insert(&self, key: K, value: V) -> Result<(), KeyValStoreError>;

    /// Get the value corresponding to the provided key from the key-value database.
    fn get(&self, key: &K) -> Result<Option<V>, KeyValStoreError>;

    /// Returns the total number of entries recorded in the key-value database.
    fn size(&self) -> Result<usize, KeyValStoreError>;

    /// Execute function `f` for each value in the database.
    ///
    /// `f` is a closure of form `|pair: Result<(K,V), KeyValStoreError>| -> IterationResult`.
    /// If `IterationResult::Break` is returned the closure will not be called again and
    /// `for_each` will return. You will usually need to include type inference to let
    /// Rust know which type to deserialise to:
    /// ```nocompile
    ///    let res = db.for_each::<Key, Val, _>(|pair| {
    ///        let (key, val) = pair.unwrap();
    ///        //.. do stuff with key and val..
    ///    });
    fn for_each<F>(&self, f: F) -> Result<(), KeyValStoreError>
    where
        Self: Sized,
        F: FnMut(Result<(K, V), KeyValStoreError>) -> IterationResult;

    /// Checks whether the provided `key` exists in the key-value database.
    fn exists(&self, key: &K) -> Result<bool, KeyValStoreError>;

    /// Delete a key-pair record associated with the provided `key` from the key-pair database.
    fn delete(&self, key: &K) -> Result<(), KeyValStoreError>;

    /// Execute function `f` for each value in the database. Any errors are filtered out.
    /// This is useful for any caller which could not do any better with an error
    /// than filtering it out.
    ///
    /// `f` is a closure of form `|pair: (K,V)| -> ()`. You will usually need to include type
    /// inference to let Rust know which type to deserialise to:
    /// ```nocompile
    ///    let res = db.for_each_ok::<Key, Val, _>(|(key, val)| {
    ///        //.. do stuff with key and val..
    ///    });
    fn for_each_ok<F>(&self, mut f: F) -> Result<(), KeyValStoreError>
    where
        Self: Sized,
        F: FnMut((K, V)) -> IterationResult,
    {
        self.for_each(|result| match result {
            Ok(pair) => f(pair),
            Err(_) => IterationResult::Continue,
        })
    }

    /// Return a `Vec<(K, V)>` filtered by the predicate.
    ///
    /// Bare in mind that this is not an `Iterator` and filter will fetch data eagerly.
    fn filter<F>(&self, predicate: F) -> Result<Vec<(K, V)>, KeyValStoreError>
    where
        Self: Sized,
        F: FnMut(&(K, V)) -> bool,
    {
        self.filter_take(self.size()?, predicate)
    }

    /// Return a `Vec<(K, V)>` filtered by the predicate. At most `n` pairs are returned.
    fn filter_take<F>(&self, n: usize, mut predicate: F) -> Result<Vec<(K, V)>, KeyValStoreError>
    where
        Self: Sized,
        F: FnMut(&(K, V)) -> bool,
    {
        let mut values = Vec::with_capacity(n);
        self.for_each_ok(|pair| {
            if predicate(&pair) {
                values.push(pair);
            }
            if values.len() == n {
                return IterationResult::Break;
            }
            IterationResult::Continue
        })?;

        Ok(values)
    }
}
