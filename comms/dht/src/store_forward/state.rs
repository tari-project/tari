// Copyright 2019, The Tari Project
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

use crate::proto::store_forward::StoredMessage;
use std::{
    sync::{RwLock, RwLockWriteGuard},
    time::Duration,
};
use ttl_cache::TtlCache;

pub type SignatureBytes = Vec<u8>;

pub struct SafStorage {
    message_cache: RwLock<TtlCache<SignatureBytes, StoredMessage>>,
}

impl SafStorage {
    pub fn new(cache_capacity: usize) -> Self {
        Self {
            message_cache: RwLock::new(TtlCache::new(cache_capacity)),
        }
    }

    pub fn insert(&self, key: SignatureBytes, message: StoredMessage, ttl: Duration) -> Option<StoredMessage> {
        acquire_write_lock!(self.message_cache).insert(key, message, ttl)
    }

    pub fn with_lock<F, T>(&self, f: F) -> T
    where F: FnOnce(RwLockWriteGuard<TtlCache<SignatureBytes, StoredMessage>>) -> T {
        f(acquire_write_lock!(self.message_cache))
    }

    #[cfg(test)]
    pub fn remove(&self, key: &SignatureBytes) -> Option<StoredMessage> {
        acquire_write_lock!(self.message_cache).remove(key)
    }
}
