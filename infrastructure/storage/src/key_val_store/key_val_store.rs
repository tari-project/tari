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
use lmdb_zero::traits::AsLmdbBytes;

/// General CRUD behaviour of Key-value store implementations.
pub trait KeyValStore {
    /// Inserts a key-value pair into the key-value database.
    fn insert_pair<K, V>(&self, key: &K, value: &V) -> Result<(), KeyValStoreError>
    where
        K: AsLmdbBytes + ?Sized,
        V: serde::Serialize;

    /// Get the value corresponding to the provided key from the key-value database.
    fn get_value<K, V>(&self, key: &K) -> Result<Option<V>, KeyValStoreError>
    where
        K: AsLmdbBytes + ?Sized,
        for<'t> V: serde::de::DeserializeOwned;

    /// Returns the total number of entries recorded in the key-value database.
    fn size(&self) -> Result<usize, KeyValStoreError>;

    /// Execute function `f` for each value in the database.
    ///
    /// `f` is a closure of form `|pair: Result<(K,V), KeyValStoreError>| -> ()`. You will usually need to include type
    /// inference to let Rust know which type to deserialise to:
    /// ```nocompile
    ///    let res = db.for_each::<Key, Val, _>(|pair| {
    ///        let (key, val) = pair.unwrap();
    ///        //.. do stuff with key and val..
    ///    });
    fn for_each<K, V, F>(&self, f: F) -> Result<(), KeyValStoreError>
    where
        K: serde::de::DeserializeOwned,
        V: serde::de::DeserializeOwned,
        F: FnMut(Result<(K, V), KeyValStoreError>);

    /// Checks whether the provided `key` exists in the key-value database.
    fn exists<K>(&self, key: &K) -> Result<bool, KeyValStoreError>
    where K: AsLmdbBytes + ?Sized;

    /// Delete a key-pair record associated with the provided `key` from the key-pair database.
    fn delete<K>(&self, key: &K) -> Result<(), KeyValStoreError>
    where K: AsLmdbBytes + ?Sized;
}
