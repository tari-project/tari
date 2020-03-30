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

use crate::chain_storage::error::ChainStorageError;
use lmdb_zero::{
    error::{self, LmdbResultExt},
    put,
    ConstAccessor,
    Cursor,
    CursorIter,
    Database,
    Environment,
    Ignore,
    MaybeOwned,
    ReadTransaction,
    WriteTransaction,
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb";

// TODO: Calling `access` for every lmdb operation has some overhead (an atomic read and set). Check if is possible to
// pass an Accessor instead of the WriteTransaction?

pub fn serialize<T>(data: &T) -> Result<Vec<u8>, ChainStorageError>
where T: Serialize {
    let mut buf = Vec::with_capacity(512);
    bincode::serialize_into(&mut buf, data)
        .or_else(|e| {
            error!(target: LOG_TARGET, "Could not serialize lmdb: {:?}", e);
            Err(e)
        })
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    Ok(buf)
}

pub fn deserialize<T>(buf_bytes: &[u8]) -> Result<T, error::Error>
where T: DeserializeOwned {
    bincode::deserialize(buf_bytes)
        .or_else(|e| {
            error!(target: LOG_TARGET, "Could not deserialize lmdb: {:?}", e);
            Err(e)
        })
        .map_err(|e| error::Error::ValRejected(e.to_string()))
}

pub fn lmdb_insert<K, V>(txn: &WriteTransaction, db: &Database, key: &K, val: &V) -> Result<(), ChainStorageError>
where
    K: Serialize,
    V: Serialize,
{
    let key_buf = serialize(key)?;
    let val_buf = serialize(val)?;
    txn.access()
        .put(&db, &key_buf, &val_buf, put::NOOVERWRITE)
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))
}

pub fn lmdb_replace<K, V>(txn: &WriteTransaction, db: &Database, key: &K, val: &V) -> Result<(), ChainStorageError>
where
    K: Serialize,
    V: Serialize,
{
    let key_buf = serialize(key)?;
    let val_buf = serialize(val)?;
    txn.access()
        .put(&db, &key_buf, &val_buf, put::Flags::empty())
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))
}

pub fn lmdb_delete<K>(txn: &WriteTransaction, db: &Database, key: &K) -> Result<(), ChainStorageError>
where K: Serialize {
    let key_buf = serialize(key)?;
    txn.access()
        .del_key(&db, &key_buf)
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))
}

pub fn lmdb_get<K, V>(env: &Environment, db: &Database, key: &K) -> Result<Option<V>, ChainStorageError>
where
    K: Serialize,
    V: DeserializeOwned,
{
    let txn = ReadTransaction::new(env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    let access = txn.access();
    let key_buf = serialize(key)?;
    match access.get(&db, &key_buf).to_opt() {
        Ok(None) => Ok(None),
        Err(e) => Err(ChainStorageError::AccessError(e.to_string())),
        Ok(Some(v)) => match deserialize(v) {
            Ok(val) => Ok(Some(val)),
            Err(e) => Err(ChainStorageError::AccessError(e.to_string())),
        },
    }
}

pub fn lmdb_exists<K>(env: &Environment, db: &Database, key: &K) -> Result<bool, ChainStorageError>
where K: Serialize {
    let txn = ReadTransaction::new(env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    let access = txn.access();
    let key_buf = serialize(key)?;
    let res: error::Result<&Ignore> = access.get(&db, &key_buf);
    let res = res
        .to_opt()
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
        .is_some();
    Ok(res)
}

pub fn lmdb_len(env: &Environment, db: &Database) -> Result<usize, ChainStorageError> {
    let txn = ReadTransaction::new(env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    let stats = txn
        .db_stat(&db)
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    Ok(stats.entries)
}

pub fn lmdb_iter_next<K, V>(c: &mut Cursor, access: &ConstAccessor) -> Result<(K, V), error::Error>
where
    K: DeserializeOwned,
    V: DeserializeOwned,
{
    let (key_bytes, val_bytes) = c.next(access)?;
    let key = deserialize::<K>(key_bytes)?;
    let val = deserialize::<V>(val_bytes)?;
    Ok((key, val))
}

pub fn lmdb_for_each<F, K, V>(env: &Environment, db: &Database, mut f: F) -> Result<(), ChainStorageError>
where
    F: FnMut(Result<(K, V), ChainStorageError>),
    K: DeserializeOwned,
    V: DeserializeOwned,
{
    let txn = ReadTransaction::new(env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    let access = txn.access();
    let cursor = txn
        .cursor(db)
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    let head = |c: &mut Cursor, a: &ConstAccessor| {
        let (key_bytes, val_bytes) = c.first(a)?;
        let key = deserialize::<K>(key_bytes)?;
        let val = deserialize::<V>(val_bytes)?;
        Ok((key, val))
    };
    let cursor = MaybeOwned::Owned(cursor);
    let iter = CursorIter::new(cursor, &access, head, lmdb_iter_next)
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    for p in iter {
        f(p.map_err(|e| ChainStorageError::AccessError(e.to_string())));
    }
    Ok(())
}

pub fn lmdb_clear_db(txn: &WriteTransaction, db: &Database) -> Result<(), ChainStorageError> {
    txn.access()
        .clear_db(&db)
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))
}
