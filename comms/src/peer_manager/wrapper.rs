//  Copyright 2020, The Tari Project
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

use crate::peer_manager::{migrations::MIGRATION_VERSION_KEY, Peer, PeerId};
use tari_storage::{IterationResult, KeyValStoreError, KeyValueStore};

// TODO: Hack to get around current peer database design. Once PeerManager uses a PeerDatabase abstraction and the LMDB
//       implementation has access to multiple databases we can remove this wrapper.

pub struct KeyValueWrapper<T> {
    inner: T,
}

impl<T> KeyValueWrapper<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T> KeyValueStore<PeerId, Peer> for KeyValueWrapper<T>
where T: KeyValueStore<PeerId, Peer>
{
    fn insert(&self, key: u64, value: Peer) -> Result<(), KeyValStoreError> {
        if key == MIGRATION_VERSION_KEY {
            panic!("MIGRATION_VERSION_KEY used in `KeyValueWrapper::insert`. MIGRATION_VERSION_KEY is a reserved key");
        }
        self.inner.insert(key, value)
    }

    fn get(&self, key: &u64) -> Result<Option<Peer>, KeyValStoreError> {
        if key == &MIGRATION_VERSION_KEY {
            return Ok(None);
        }
        self.inner.get(key)
    }

    fn size(&self) -> Result<usize, KeyValStoreError> {
        self.inner.size().map(|s| s.saturating_sub(1))
    }

    fn for_each<F>(&self, f: F) -> Result<(), KeyValStoreError>
    where
        Self: Sized,
        F: FnMut(Result<(PeerId, Peer), KeyValStoreError>) -> IterationResult,
    {
        let result = self.inner.for_each(f);
        if result.is_err() && self.size()? == 0 {
            // for_each erroneously returns an error if the first key is invalid instead of passing it to the for
            // each closure
            return Ok(());
        }

        result
    }

    fn exists(&self, key: &u64) -> Result<bool, KeyValStoreError> {
        if key == &MIGRATION_VERSION_KEY {
            return Ok(false);
        }
        self.inner.exists(key)
    }

    fn delete(&self, key: &u64) -> Result<(), KeyValStoreError> {
        if key == &MIGRATION_VERSION_KEY {
            panic!("MIGRATION_VERSION_KEY used in `KeyValueWrapper::delete`. MIGRATION_VERSION_KEY is a reserved key");
        }
        self.inner.delete(key)
    }
}
