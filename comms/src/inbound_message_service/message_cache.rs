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

use crate::consts::{IMS_MSG_CACHE_STORAGE_CAPACITY, IMS_MSG_CACHE_TTL};
use derive_error::Error;
use std::{hash::Hash, sync::RwLock, time::Duration};
use ttl_cache::TtlCache;

#[derive(Debug, Error)]
pub enum MessageCacheError {
    /// A duplicate entry existed in the message cache
    DuplicateEntry,
}

#[derive(Clone, Copy)]
pub struct MessageCacheConfig {
    /// The maximum number of messages that can be tracked using the MessageCache
    storage_capacity: usize,
    /// The Time-to-live for each stored message
    msg_ttl: Duration,
}

impl Default for MessageCacheConfig {
    fn default() -> Self {
        MessageCacheConfig {
            storage_capacity: IMS_MSG_CACHE_STORAGE_CAPACITY,
            msg_ttl: IMS_MSG_CACHE_TTL,
        }
    }
}

/// The MessageCache is used to track handled messages to ensure that processing resources are not wasted on
/// duplicate messages and that these duplicate messages are not sent to services or propagate through the network.
pub struct MessageCache<K: Eq + Hash> {
    config: MessageCacheConfig,
    cache: RwLock<TtlCache<K, ()>>,
}

impl<K> MessageCache<K>
where K: Eq + Hash
{
    /// Create a new MessageCache with the specified configuration
    pub fn new(config: MessageCacheConfig) -> Self {
        Self {
            config,
            cache: RwLock::new(TtlCache::new(config.storage_capacity)),
        }
    }

    /// Insert a new message into the MessageCache with a time-to-live starting at the insertion time. It will
    /// return a DuplicateEntry Error if the message has already been added into the cache.
    pub fn insert(&mut self, msg: K) -> Result<(), MessageCacheError> {
        let mut cache_lock = acquire_write_lock!(self.cache);

        match cache_lock.insert(msg, (), self.config.msg_ttl) {
            Some(_) => Err(MessageCacheError::DuplicateEntry),
            None => Ok(()),
        }
    }

    /// Check if the message is available in the MessageCache
    pub fn contains(&self, msg: &K) -> bool {
        let cache_lock = acquire_read_lock!(self.cache);
        cache_lock.contains_key(msg)
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use futures::executor::block_on;
    use std::{thread, time::Duration};

    #[test]
    fn test_msg_rlu_and_ttl() {
        block_on(async {
            let mut msg_cache: MessageCache<String> = MessageCache::new(MessageCacheConfig {
                storage_capacity: 3,
                msg_ttl: Duration::from_millis(100),
            });
            let msg1 = "msg1".to_string();
            let msg2 = "msg2".to_string();
            let msg3 = "msg3".to_string();
            let msg4 = "msg4".to_string();

            msg_cache.insert(msg1.clone()).unwrap();
            assert!(msg_cache.contains(&msg1));
            assert!(!msg_cache.contains(&msg2));

            msg_cache.insert(msg2.clone()).unwrap();
            assert!(msg_cache.contains(&msg1));
            assert!(msg_cache.contains(&msg2));
            assert!(!msg_cache.contains(&msg3));

            msg_cache.insert(msg3.clone()).unwrap();
            assert!(msg_cache.contains(&msg1));
            assert!(msg_cache.contains(&msg2));
            assert!(msg_cache.contains(&msg3));
            assert!(!msg_cache.contains(&msg4));

            thread::sleep(Duration::from_millis(50));
            msg_cache.insert(msg4.clone()).unwrap();

            // Due to storage limits, msg1 was removed when msg4 was added
            assert!(!msg_cache.contains(&msg1));
            assert!(msg_cache.contains(&msg2));
            assert!(msg_cache.contains(&msg3));
            assert!(msg_cache.contains(&msg4));

            // msg2 and msg3 would have reached their ttl thresholds
            thread::sleep(Duration::from_millis(51));
            assert!(!msg_cache.contains(&msg2));
            assert!(!msg_cache.contains(&msg3));
            assert!(msg_cache.contains(&msg4));
        })
    }
}
