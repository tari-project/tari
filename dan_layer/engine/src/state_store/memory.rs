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
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use anyhow::anyhow;

use crate::state_store::{AtomicDb, StateReader, StateStoreError, StateWriter};

type InnerKvMap = HashMap<Vec<u8>, Vec<u8>>;

#[derive(Debug, Clone, Default)]
pub struct MemoryStateStore {
    state: Arc<RwLock<InnerKvMap>>,
}

pub struct MemoryTransaction<T> {
    pending: InnerKvMap,
    guard: T,
}

impl<'a> AtomicDb<'a> for MemoryStateStore {
    type Error = anyhow::Error;
    type ReadAccess = MemoryTransaction<RwLockReadGuard<'a, InnerKvMap>>;
    type WriteAccess = MemoryTransaction<RwLockWriteGuard<'a, InnerKvMap>>;

    fn read_access(&'a self) -> Result<Self::ReadAccess, Self::Error> {
        let guard = self.state.read().map_err(|_| anyhow!("Failed to read state"))?;

        Ok(MemoryTransaction {
            pending: HashMap::default(),
            guard,
        })
    }

    fn write_access(&'a self) -> Result<Self::WriteAccess, Self::Error> {
        let guard = self.state.write().map_err(|_| anyhow!("Failed to write state"))?;

        Ok(MemoryTransaction {
            pending: HashMap::default(),
            guard,
        })
    }

    fn commit(&self, mut tx: Self::WriteAccess) -> Result<(), Self::Error> {
        tx.guard.extend(tx.pending.into_iter());
        Ok(())
    }
}

impl<'a> StateReader for MemoryTransaction<RwLockReadGuard<'a, InnerKvMap>> {
    fn get_state_raw(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StateStoreError> {
        Ok(self.pending.get(key).cloned().or_else(|| self.guard.get(key).cloned()))
    }

    fn exists(&self, key: &[u8]) -> Result<bool, StateStoreError> {
        Ok(self.pending.contains_key(key) || self.guard.contains_key(key))
    }
}

impl<'a> StateReader for MemoryTransaction<RwLockWriteGuard<'a, InnerKvMap>> {
    fn get_state_raw(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StateStoreError> {
        Ok(self.pending.get(key).cloned().or_else(|| self.guard.get(key).cloned()))
    }

    fn exists(&self, key: &[u8]) -> Result<bool, StateStoreError> {
        Ok(self.pending.contains_key(key) || self.guard.contains_key(key))
    }
}

impl<'a> StateWriter for MemoryTransaction<RwLockWriteGuard<'a, InnerKvMap>> {
    fn set_state_raw(&mut self, key: &[u8], value: Vec<u8>) -> Result<(), StateStoreError> {
        self.pending.insert(key.to_vec(), value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tari_template_abi::{Decode, Encode};

    use super::*;

    #[test]
    fn read_write() {
        let store = MemoryStateStore::default();
        let mut access = store.write_access().unwrap();
        access.set_state_raw(b"abc", vec![1, 2, 3]).unwrap();
        let res = access.get_state_raw(b"abc").unwrap();
        assert_eq!(res, Some(vec![1, 2, 3]));
        let res = access.get_state_raw(b"def").unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn read_write_rollback_commit() {
        #[derive(Debug, Encode, Decode, PartialEq, Eq, Clone)]
        struct UserData {
            name: String,
            age: u8,
        }

        let user_data = UserData {
            name: "Foo".to_string(),
            age: 99,
        };

        let store = MemoryStateStore::default();
        {
            let mut access = store.write_access().unwrap();
            access.set_state(b"abc", user_data.clone()).unwrap();
            let res = access.get_state(b"abc").unwrap();
            assert_eq!(res, Some(user_data.clone()));
            let res = access.get_state::<_, UserData>(b"def").unwrap();
            assert_eq!(res, None);
            // Drop without commit rolls back
        }

        {
            let access = store.read_access().unwrap();
            let res = access.get_state::<_, UserData>(b"abc").unwrap();
            assert_eq!(res, None);
        }

        {
            let mut access = store.write_access().unwrap();
            access.set_state(b"abc", user_data.clone()).unwrap();
            store.commit(access).unwrap();
        }

        let access = store.read_access().unwrap();
        let res = access.get_state(b"abc").unwrap();
        assert_eq!(res, Some(user_data));
    }
}
