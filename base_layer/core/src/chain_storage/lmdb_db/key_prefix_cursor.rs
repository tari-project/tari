//  Copyright 2021, The Tari Project
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

use std::marker::PhantomData;

use lmdb_zero::{ConstAccessor, Cursor, LmdbResultExt};
use serde::de::DeserializeOwned;

use crate::chain_storage::{lmdb_db::helpers::deserialize, ChainStorageError};

pub struct KeyPrefixCursor<'a, V> {
    cursor: Cursor<'a, 'a>,
    value_type: PhantomData<V>,
    prefix_key: &'a [u8],
    access: ConstAccessor<'a>,
    has_seeked: bool,
}

impl<'a, V> KeyPrefixCursor<'a, V>
where V: DeserializeOwned
{
    pub(super) fn new(cursor: Cursor<'a, 'a>, access: ConstAccessor<'a>, prefix_key: &'a [u8]) -> Self {
        Self {
            cursor,
            access,
            prefix_key,
            value_type: PhantomData,
            has_seeked: false,
        }
    }

    /// Returns the item on or after the key prefix, progressing forwards until the key prefix no longer matches
    pub fn next(&mut self) -> Result<Option<(Vec<u8>, V)>, ChainStorageError> {
        if !self.has_seeked {
            if let Some((k, val)) = self.seek_gte(self.prefix_key)? {
                return Ok(Some((k, val)));
            }
        }

        match self.cursor.next(&self.access).to_opt()? {
            Some((k, v)) => Self::deserialize_if_matches(self.prefix_key, k, v),
            None => Ok(None),
        }
    }

    /// Returns the item on or before the given seek key, progressing backwards until the key prefix no longer matches
    #[allow(dead_code)]
    pub fn prev(&mut self) -> Result<Option<(Vec<u8>, V)>, ChainStorageError> {
        if !self.has_seeked {
            let prefix_key = self.prefix_key;
            if let Some((k, val)) = self.seek_gte(prefix_key)? {
                // seek_range_k returns the greater key, so we only want to return the current value that was seeked to
                // if it exactly matches the prefix_key
                if k == prefix_key {
                    return Ok(Some((k, val)));
                }
            }
        }

        match self.cursor.prev(&self.access).to_opt()? {
            Some((k, v)) => Self::deserialize_if_matches(self.prefix_key, k, v),
            None => Ok(None),
        }
    }

    // This function could be used later in cases where multiple seeks are required.
    #[cfg(test)]
    pub fn reset_to(&mut self, prefix_key: &'a [u8]) {
        self.has_seeked = false;
        self.prefix_key = prefix_key;
    }

    fn seek_gte(&mut self, key: &[u8]) -> Result<Option<(Vec<u8>, V)>, ChainStorageError> {
        self.has_seeked = true;
        let seek_result = self.cursor.seek_range_k(&self.access, key).to_opt()?;
        let (k, v) = match seek_result {
            Some(r) => r,
            None => return Ok(None),
        };
        Self::deserialize_if_matches(key, k, v)
    }

    fn deserialize_if_matches(
        key_prefix: &[u8],
        k: &[u8],
        v: &[u8],
    ) -> Result<Option<(Vec<u8>, V)>, ChainStorageError> {
        let prefix_len = key_prefix.len();
        if k.len() < prefix_len || k[..prefix_len] != *key_prefix {
            return Ok(None);
        }
        let val = deserialize::<V>(v)?;
        Ok(Some((k.to_vec(), val)))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use lmdb_zero::{db, ReadTransaction, WriteTransaction};
    use tari_storage::lmdb_store::{LMDBBuilder, LMDBConfig};
    use tari_test_utils::paths::create_temporary_data_path;

    use crate::chain_storage::lmdb_db::lmdb::{lmdb_get_prefix_cursor, lmdb_insert};

    #[test]
    fn test_lmdb_get_prefix_cursor() {
        let temp_path = create_temporary_data_path();

        let lmdb_store = LMDBBuilder::new()
            .set_path(&temp_path)
            .set_env_config(LMDBConfig::default())
            .set_max_number_of_databases(1)
            .add_database("test", db::CREATE)
            .build()
            .unwrap();

        let db = lmdb_store.get_handle("test").unwrap();
        {
            let txn = WriteTransaction::new(lmdb_store.env()).unwrap();
            lmdb_insert(&txn, &db.db(), &[0xffu8, 0, 0, 0], &1u64, "test").unwrap();
            lmdb_insert(&txn, &db.db(), &[0x2bu8, 0, 0, 1], &2u64, "test").unwrap();
            lmdb_insert(&txn, &db.db(), &[0x2bu8, 0, 1, 1], &3u64, "test").unwrap();
            lmdb_insert(&txn, &db.db(), &[0x2bu8, 1, 1, 0], &4u64, "test").unwrap();
            lmdb_insert(&txn, &db.db(), &[0x2bu8, 1, 1, 1], &5u64, "test").unwrap();
            lmdb_insert(&txn, &db.db(), &[0x00u8, 1, 1, 1], &5u64, "test").unwrap();
            txn.commit().unwrap();
        }

        {
            let txn = ReadTransaction::new(lmdb_store.env()).unwrap();
            let db = db.db();
            let mut cursor = lmdb_get_prefix_cursor::<u64>(&txn, &db, &[0x2b]).unwrap();
            let kv = cursor.next().unwrap().unwrap();
            assert_eq!(kv, (vec![0x2b, 0, 0, 1], 2));
            let kv = cursor.next().unwrap().unwrap();
            assert_eq!(kv, (vec![0x2b, 0, 1, 1], 3));
            let kv = cursor.next().unwrap().unwrap();
            assert_eq!(kv, (vec![0x2b, 1, 1, 0], 4));
            let kv = cursor.next().unwrap().unwrap();
            assert_eq!(kv, (vec![0x2b, 1, 1, 1], 5));
            assert_eq!(cursor.next().unwrap(), None);

            cursor.reset_to(&[0x2b, 1, 1]);
            let kv = cursor.next().unwrap().unwrap();
            assert_eq!(kv, (vec![0x2b, 1, 1, 0], 4));
            let kv = cursor.next().unwrap().unwrap();
            assert_eq!(kv, (vec![0x2b, 1, 1, 1], 5));
            assert_eq!(cursor.next().unwrap(), None);

            cursor.reset_to(&[0x11]);
            assert_eq!(cursor.next().unwrap(), None);
        }

        fs::remove_dir_all(&temp_path).expect("Could not delete temporary file");
    }
}
