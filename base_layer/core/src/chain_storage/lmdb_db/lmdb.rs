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

use std::{fmt::Debug, time::Instant};

use lmdb_zero::{
    del,
    error::{self, LmdbResultExt},
    put,
    traits::{AsLmdbBytes, CreateCursor, FromLmdbBytes},
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
use tari_storage::lmdb_store::BYTES_PER_MB;
use tari_utilities::hex::to_hex;

use crate::chain_storage::{
    error::ChainStorageError,
    lmdb_db::{
        cursors::KeyPrefixCursor,
        helpers::{deserialize, serialize},
    },
    OrNotFound,
};

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb";

/// Makes an insertion into the lmdb table, will error if the key already exists
pub fn lmdb_insert<K, V>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    key: &K,
    val: &V,
    table_name: &'static str,
) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized + Debug,
    V: Serialize + Debug,
{
    let val_buf = serialize(val, None)?;
    match txn.access().put(db, key, &val_buf, put::NOOVERWRITE) {
        Ok(_) => {
            trace!(
                target: LOG_TARGET, "Inserted {} bytes with key '{}' into '{}'",
                val_buf.len(), to_hex(key.as_lmdb_bytes()), table_name
            );
            Ok(())
        },
        err @ Err(lmdb_zero::Error::Code(lmdb_zero::error::KEYEXIST)) => {
            error!(
                target: LOG_TARGET, "Could not insert {} bytes with key '{}' into '{}' ({:?})",
                val_buf.len(), to_hex(key.as_lmdb_bytes()), table_name, err
            );
            Err(ChainStorageError::KeyExists {
                table_name,
                key: to_hex(key.as_lmdb_bytes()),
            })
        },
        err @ Err(lmdb_zero::Error::Code(lmdb_zero::error::MAP_FULL)) => {
            info!(
                target: LOG_TARGET, "Could not insert {} bytes with key '{}' into '{}' ({:?})",
                val_buf.len(), to_hex(key.as_lmdb_bytes()), table_name, err
            );
            Err(ChainStorageError::DbResizeRequired(Some(val_buf.len())))
        },
        Err(e) => {
            error!(
                target: LOG_TARGET, "Could not insert {} bytes with key '{}' into '{}' ({:?})",
                val_buf.len(), to_hex(key.as_lmdb_bytes()), table_name, e
            );
            Err(ChainStorageError::InsertError {
                table: table_name,
                error: e.to_string(),
            })
        },
    }
}

/// Note that calling this on a table that does not allow duplicates will replace it
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
    let val_buf = serialize(val, None)?;
    txn.access().put(db, key, &val_buf, put::Flags::empty()).map_err(|e| {
        if let lmdb_zero::Error::Code(code) = &e {
            if *code == lmdb_zero::error::MAP_FULL {
                return ChainStorageError::DbResizeRequired(Some(val_buf.len()));
            }
        }
        error!(
            target: LOG_TARGET,
            "Could not insert value into lmdb transaction: {:?}", e
        );
        ChainStorageError::AccessError(e.to_string())
    })
}

/// Inserts or replaces the item at the given key. If the key does not exist, a new entry is created
pub fn lmdb_replace<K, V>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    key: &K,
    val: &V,
    size_hint: Option<usize>,
) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize,
{
    let val_buf = serialize(val, size_hint)?;
    let start = Instant::now();
    let res = txn.access().put(db, key, &val_buf, put::Flags::empty()).map_err(|e| {
        if let lmdb_zero::Error::Code(code) = &e {
            if *code == lmdb_zero::error::MAP_FULL {
                return ChainStorageError::DbResizeRequired(Some(val_buf.len()));
            }
        }
        error!(
            target: LOG_TARGET,
            "Could not replace value in lmdb transaction: {:?}", e
        );
        ChainStorageError::AccessError(e.to_string())
    });
    if val_buf.len() >= BYTES_PER_MB {
        let write_time = start.elapsed();
        trace!(
            "lmdb_replace - {} MB, lmdb write in {:.2?}",
            val_buf.len() / BYTES_PER_MB,
            write_time
        );
    }
    res
}

/// Deletes the given key. An error is returned if the key does not exist
pub fn lmdb_delete<K>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    key: &K,
    table_name: &'static str,
) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
{
    txn.access()
        .del_key(db, key)
        .or_not_found(table_name, "<unknown>", to_hex(key.as_lmdb_bytes()))?;
    Ok(())
}

/// Deletes the given key value pair. An error is returned if the key and value does not exist
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
    txn.access().del_item(db, key, &serialize(value, None)?)?;
    Ok(())
}

/// Deletes all keys matching the key
pub fn lmdb_delete_keys_starting_with<V>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    key: &[u8],
) -> Result<Vec<V>, ChainStorageError>
where
    V: DeserializeOwned,
{
    let mut access = txn.access();
    let mut cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;

    let mut row = match cursor.seek_range_k(&access, key) {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };
    trace!(target: LOG_TARGET, "Key: {}", to_hex(row.0));
    let mut result = vec![];
    while row.0[..key.len()] == *key {
        let val = deserialize::<V>(row.1)?;
        result.push(val);
        cursor.del(&mut access, del::NODUPDATA)?;
        row = match cursor.next(&access).to_opt()? {
            Some(r) => r,
            None => break,
        };
    }
    Ok(result)
}

/// retrieves the given key value pair
pub fn lmdb_get<K, V>(txn: &ConstTransaction<'_>, db: &Database, key: &K) -> Result<Option<V>, ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: DeserializeOwned,
{
    let access = txn.access();
    match access.get(db, key).to_opt() {
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

/// retrieves the multiple values matching the key
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

/// Retrieves the last value stored in the database
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

/// Checks if the key exists in the database
pub fn lmdb_exists<K>(txn: &ConstTransaction<'_>, db: &Database, key: &K) -> Result<bool, ChainStorageError>
where K: AsLmdbBytes + ?Sized {
    let access = txn.access();
    match access.get::<K, [u8]>(db, key).to_opt() {
        Ok(None) => Ok(false),
        Err(e) => {
            error!(target: LOG_TARGET, "Could not read from lmdb: {:?}", e);
            Err(ChainStorageError::AccessError(e.to_string()))
        },
        Ok(Some(_)) => Ok(true),
    }
}

/// Returns the amount of entries of the database table
pub fn lmdb_len(txn: &ConstTransaction<'_>, db: &Database) -> Result<usize, ChainStorageError> {
    let stats = txn.db_stat(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not read length from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;
    Ok(stats.entries)
}

/// Return a cursor that iterates, either backwards or forwards through keys matching the given prefix
pub fn lmdb_get_prefix_cursor<'a, V>(
    txn: &'a ConstTransaction<'a>,
    db: &'a Database,
    prefix_key: &'a [u8],
) -> Result<KeyPrefixCursor<'a, V>, ChainStorageError>
where
    V: DeserializeOwned,
{
    let access = txn.access();

    let cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;

    Ok(KeyPrefixCursor::new(cursor, access, prefix_key))
}

/// Fetches values the key prefix
pub fn lmdb_fetch_matching_after<V>(
    txn: &ConstTransaction<'_>,
    db: &Database,
    key_prefix: &[u8],
) -> Result<Vec<V>, ChainStorageError>
where
    V: DeserializeOwned,
{
    let mut cursor = lmdb_get_prefix_cursor(txn, db, key_prefix)?;
    let mut result = vec![];
    while let Some((_, val)) = cursor.next()? {
        result.push(val);
    }
    Ok(result)
}

/// Fetches first value the key prefix
pub fn lmdb_first_after<K, V>(
    txn: &ConstTransaction<'_>,
    db: &Database,
    key: &K,
) -> Result<Option<V>, ChainStorageError>
where
    K: AsLmdbBytes + FromLmdbBytes + ?Sized,
    V: DeserializeOwned,
{
    let access = txn.access();
    let mut cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {:?}", e);
        ChainStorageError::AccessError(e.to_string())
    })?;

    match cursor.seek_range_k(&access, key) {
        Ok((_, v)) => {
            let val = deserialize::<V>(v)?;
            Ok(Some(val))
        },
        Err(_) => Ok(None),
    }
}

/// Filter the values matching the fn
pub fn lmdb_filter_map_values<F, V, R>(
    txn: &ConstTransaction<'_>,
    db: &Database,
    f: F,
) -> Result<Vec<R>, ChainStorageError>
where
    F: Fn(V) -> Option<R>,
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
        if let Some(r) = f(val) {
            result.push(r);
        }
    }
    Ok(result)
}

/// Fetches the size of all key/values in the given DB. Returns the number of entries, the total size of all the
/// keys and values in bytes.
pub fn fetch_db_entry_sizes(txn: &ConstTransaction<'_>, db: &Database) -> Result<(u64, u64, u64), ChainStorageError> {
    let access = txn.access();
    let mut cursor = txn.cursor(db)?;
    let mut num_entries = 0;
    let mut total_key_size = 0;
    let mut total_value_size = 0;
    while let Some((key, value)) = cursor.next::<[u8], [u8]>(&access).to_opt()? {
        num_entries += 1;
        total_key_size += key.len() as u64;
        total_value_size += value.len() as u64;
    }
    Ok((num_entries, total_key_size, total_value_size))
}

/// deletes entries using the filter Fn
pub fn lmdb_delete_each_where<K, V, F>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    mut predicate: F,
) -> Result<usize, ChainStorageError>
where
    K: FromLmdbBytes + ?Sized,
    V: DeserializeOwned,
    F: FnMut(&K, V) -> Option<bool>,
{
    let mut cursor = txn.cursor(db)?;
    let mut access = txn.access();
    let mut num_deleted = 0;
    while let Some((k, v)) = cursor.next::<K, [u8]>(&access).to_opt()? {
        match deserialize(v) {
            Ok(v) => match predicate(k, v) {
                Some(true) => {
                    cursor.del(&mut access, del::Flags::empty())?;
                    num_deleted += 1;
                },
                Some(false) => continue,
                None => {
                    break;
                },
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Could not could not deserialize value from lmdb: {:?}", e
                );
                return Err(ChainStorageError::AccessError(e.to_string()));
            },
        }
    }
    Ok(num_deleted)
}

/// Deletes the entire database
pub fn lmdb_clear(txn: &WriteTransaction<'_>, db: &Database) -> Result<usize, ChainStorageError> {
    let mut cursor = txn.cursor(db)?;
    let mut access = txn.access();
    let mut num_deleted = 0;
    while cursor.next::<[u8], [u8]>(&access).to_opt()?.is_some() {
        cursor.del(&mut access, del::Flags::empty())?;
        num_deleted += 1;
    }
    Ok(num_deleted)
}
