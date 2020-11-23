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
    del,
    error::{self, LmdbResultExt},
    put,
    traits::{AsLmdbBytes, FromLmdbBytes},
    ConstTransaction,
    Cursor,
    CursorIter,
    Database,
    Error,
    MaybeOwned,
    WriteTransaction,
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb";

// TODO: Calling `access` for every lmdb operation has some overhead (an atomic read and set). Check if is possible to
// pass an Accessor instead of the WriteTransaction?

pub fn serialize<T>(data: &T) -> Result<Vec<u8>, ChainStorageError>
where T: Serialize {
    let size = bincode::serialized_size(&data).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    let mut buf = Vec::with_capacity(size as usize);
    bincode::serialize_into(&mut buf, data).map_err(|e| {
        error!(target: LOG_TARGET, "Could not serialize lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;
    Ok(buf)
}

pub fn deserialize<T>(buf_bytes: &[u8]) -> Result<T, error::Error>
where T: DeserializeOwned {
    bincode::deserialize(buf_bytes)
        .map_err(|e| {
            error!(target: LOG_TARGET, "Could not deserialize lmdb: {:?}", e);
            e
        })
        .map_err(|e| error::Error::ValRejected(e.to_string()))
}

pub fn lmdb_insert<K, V>(txn: &WriteTransaction<'_>, db: &Database, key: &K, val: &V) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize,
{
    let val_buf = serialize(val)?;
    txn.access().put(&db, key, &val_buf, put::NOOVERWRITE).map_err(|e| {
        error!(
            target: LOG_TARGET,
            "Could not add insert value into lmdb transaction: {:?}", e
        );
        ChainStorageError::AccessError(e.to_string())
    })
}

pub fn lmdb_insert_dup<K, V>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    key: &K,
    val: &V,
) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize,
{
    let val_buf = serialize(val)?;
    txn.access().put(&db, key, &val_buf, put::Flags::empty()).map_err(|e| {
        error!(
            target: LOG_TARGET,
            "Could not add insert value into lmdb transaction: {:?}", e
        );
        ChainStorageError::AccessError(e.to_string())
    })
}

pub fn lmdb_replace<K, V>(txn: &WriteTransaction<'_>, db: &Database, key: &K, val: &V) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize,
{
    let val_buf = serialize(val)?;
    txn.access().put(&db, key, &val_buf, put::Flags::empty()).map_err(|e| {
        error!(
            target: LOG_TARGET,
            "Could not add replace value into lmdb transaction: {:?}", e
        );
        ChainStorageError::AccessError(e.to_string())
    })
}

/// Deletes the given key. An error is returned if the key does not exist
pub fn lmdb_delete<K>(txn: &WriteTransaction<'_>, db: &Database, key: &K) -> Result<(), ChainStorageError>
where K: AsLmdbBytes + ?Sized {
    txn.access().del_key(&db, key)?;
    Ok(())
}

pub fn lmdb_delete_key_value<K, V>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    key: &K,
    value: &V,
) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize,
{
    txn.access().del_item(&db, key, &serialize(value)?)?;
    Ok(())
}

pub fn lmdb_delete_keys_starting_with<V>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    key: &str,
) -> Result<Vec<V>, ChainStorageError>
where
    V: DeserializeOwned,
{
    let mut access = txn.access();
    let mut cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;

    debug!(target: LOG_TARGET, "Deleting rows matching pattern: {}", key);

    let mut row = match cursor.seek_range_k(&access, key) {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };
    trace!(target: LOG_TARGET, "Key: {}", row.0);
    let mut result = vec![];
    while row.0.starts_with(key) {
        let val = deserialize::<V>(row.1)?;
        result.push(val);
        cursor.del(&mut access, del::NODUPDATA)?;
        row = match cursor.next(&access) {
            Ok(r) => r,
            Err(_) => break,
        }
    }
    Ok(result)
}

pub fn lmdb_get<K, V>(txn: &ConstTransaction<'_>, db: &Database, key: &K) -> Result<Option<V>, ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: DeserializeOwned,
{
    let access = txn.access();
    match access.get(&db, key).to_opt() {
        Ok(None) => Ok(None),
        Err(e) => {
            error!(target: LOG_TARGET, "Could not get value from lmdb: {:?}", e);
            Err(ChainStorageError::AccessError(e.to_string()))
        },
        Ok(Some(v)) => match deserialize(v) {
            Ok(val) => Ok(Some(val)),
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Could not could not deserialize value from lmdb: {:?}", e
                );
                Err(ChainStorageError::AccessError(e.to_string()))
            },
        },
    }
}

pub fn lmdb_get_multiple<K, V>(txn: &ConstTransaction<'_>, db: &Database, key: &K) -> Result<Vec<V>, ChainStorageError>
where
    K: AsLmdbBytes + FromLmdbBytes + ?Sized,
    V: DeserializeOwned,
{
    let access = txn.access();
    let mut cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;
    let mut result = vec![];
    let row = match cursor.seek_k(&access, key) {
        Ok(r) => r,
        Err(e) => {
            if e == Error::Code(error::NOTFOUND) {
                return Ok(result);
            }
            error!(target: LOG_TARGET, "Error in lmdb_get_multiple:{}", e.to_string());
            // No matches
            return Err(e.into());
        },
    };
    result.push(deserialize(row)?);
    while let Ok((_, row)) = cursor.next_dup::<K, [u8]>(&access) {
        result.push(deserialize(row)?);
    }
    Ok(result)
}

pub fn lmdb_last<V>(txn: &ConstTransaction<'_>, db: &Database) -> Result<Option<V>, ChainStorageError>
where V: DeserializeOwned {
    let mut cursor = txn.cursor(db)?;
    let access = txn.access();
    match cursor.last::<[u8], [u8]>(&access).to_opt() {
        Err(e) => {
            error!(target: LOG_TARGET, "Could not get value from lmdb: {:?}", e);
            Err(ChainStorageError::AccessError(e.to_string()))
        },
        Ok(None) => Ok(None),
        Ok(Some((_k, v))) => deserialize(v).map(Some).map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Could not could not deserialize value from lmdb: {:?}", e
            );
            ChainStorageError::AccessError(e.to_string())
        }),
    }
}

pub fn lmdb_exists<K>(txn: &ConstTransaction<'_>, db: &Database, key: &K) -> Result<bool, ChainStorageError>
where K: AsLmdbBytes + ?Sized {
    let access = txn.access();
    match access.get::<K, [u8]>(&db, key).to_opt() {
        Ok(None) => Ok(false),
        Err(e) => {
            error!(target: LOG_TARGET, "Could not read from lmdb: {:?}", e);
            Err(ChainStorageError::AccessError(e.to_string()))
        },
        Ok(Some(_)) => Ok(true),
    }
}

pub fn lmdb_len(txn: &ConstTransaction<'_>, db: &Database) -> Result<usize, ChainStorageError> {
    let stats = txn.db_stat(&db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not read length from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;
    Ok(stats.entries)
}

pub fn lmdb_fetch_keys_starting_with<V>(
    key: &str,
    txn: &ConstTransaction<'_>,
    db: &Database,
) -> Result<Vec<V>, ChainStorageError>
where
    V: DeserializeOwned,
{
    let access = txn.access();
    let mut cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;

    trace!(target: LOG_TARGET, "Getting rows matching pattern: {}", key);

    let mut row = match cursor.seek_range_k(&access, key) {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };
    trace!(target: LOG_TARGET, "Key: {}", row.0);
    let mut result = vec![];
    while row.0.starts_with(key) {
        let val = deserialize::<V>(row.1)?;
        result.push(val);
        row = match cursor.next(&access) {
            Ok(r) => r,
            Err(_) => break,
        }
    }
    Ok(result)
}

pub fn lmdb_list_keys(txn: &ConstTransaction<'_>, db: &Database) -> Result<Vec<Vec<u8>>, ChainStorageError> {
    let access = txn.access();
    let mut cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;
    let iter = CursorIter::new(
        MaybeOwned::Borrowed(&mut cursor),
        &access,
        |c, a| c.first(a),
        Cursor::next::<[u8], [u8]>,
    )?;

    let mut result = vec![];
    for row in iter {
        result.push(Vec::from(row?.0));
    }
    Ok(result)
}

pub fn lmdb_filter_map_values<F, V, R>(
    txn: &ConstTransaction<'_>,
    db: &Database,
    f: F,
) -> Result<Vec<R>, ChainStorageError>
where
    F: Fn(V) -> Result<Option<R>, ChainStorageError>,
    V: DeserializeOwned,
{
    let access = txn.access();
    let mut cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;
    let iter = CursorIter::new(
        MaybeOwned::Borrowed(&mut cursor),
        &access,
        |c, a| c.first(a),
        Cursor::next::<[u8], [u8]>,
    )?;

    let mut result = vec![];
    for row in iter {
        // result.push(Vec::from(row?.0));
        let val = deserialize::<V>(row?.1)?;
        if let Some(r) = f(val)? {
            result.push(r);
        }
    }
    Ok(result)
}
