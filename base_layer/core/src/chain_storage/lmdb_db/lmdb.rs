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
    ConstTransaction,
    Cursor,
    CursorIter,
    Database,
    Error,
    MaybeOwned,
    WriteTransaction,
    del,
    error::{self, LmdbResultExt},
    put,
    traits::{AsLmdbBytes, CreateCursor, FromLmdbBytes},
};
use log::*;
use serde::{Serialize, de::DeserializeOwned};
use tari_storage::lmdb_store::BYTES_PER_MB;
use tari_utilities::hex::to_hex;

use crate::chain_storage::{
    OrNotFound,
    error::ChainStorageError,
    lmdb_db::{
        cursors::KeyPrefixCursor,
        helpers::{deserialize, serialize, try_deserialize},
        lmdb_db::TypedDatabaseRef,
    },
};

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb";

pub fn lmdb_insert_typed<K, V>(
    txn: &WriteTransaction<'_>,
    db: &TypedDatabaseRef<K, V>,
    key: &K,
    val: &V,
) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized + Debug,
    V: Serialize + Debug + DeserializeOwned,
{
    lmdb_insert(txn, &db.db, key, val, db.name)
}

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
    V: Serialize + Debug + ?Sized,
{
    let val_buf = serialize(val, None)?;
    match txn.access().put(db, key, &val_buf, put::NOOVERWRITE) {
        Ok(_) => Ok(()),
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
        if let lmdb_zero::Error::Code(code) = &e &&
            *code == lmdb_zero::error::MAP_FULL
        {
            return ChainStorageError::DbResizeRequired(Some(val_buf.len()));
        }
        error!(
            target: LOG_TARGET,
            "Could not insert value into lmdb transaction: {e:?}"
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
    // put::Flags::empty(): This will replace the value if it exists, or insert a new value if it does not.
    let res = txn.access().put(db, key, &val_buf, put::Flags::empty()).map_err(|e| {
        if let lmdb_zero::Error::Code(code) = &e &&
            *code == lmdb_zero::error::MAP_FULL
        {
            return ChainStorageError::DbResizeRequired(Some(val_buf.len()));
        }
        error!(
            target: LOG_TARGET,
            "Could not replace value in lmdb transaction: {e:?}"
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

pub fn lmdb_delete_typed<K, V>(
    txn: &WriteTransaction<'_>,
    db: &TypedDatabaseRef<K, V>,
    key: &K,
) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize + DeserializeOwned,
{
    lmdb_delete(txn, &db.db, key, db.name)
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

pub fn lmdb_delete_if_exists<K>(txn: &WriteTransaction<'_>, db: &Database, key: &K) -> Result<(), ChainStorageError>
where K: AsLmdbBytes + ?Sized {
    txn.access().del_key(db, key).to_opt()?;
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
) -> Result<Vec<(Vec<u8>, V)>, ChainStorageError>
where
    V: DeserializeOwned,
{
    let mut access = txn.access();
    let mut cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {e:?}");
        ChainStorageError::AccessError(e.to_string())
    })?;

    let mut row = match cursor.seek_range_k(&access, key) {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };
    trace!(target: LOG_TARGET, "Key: {}", to_hex(row.0));
    let mut result = vec![];
    while row.0.get(..key.len()).expect("Cannot expect") == key {
        let val = deserialize::<V>(row.1)?;
        result.push((row.0.to_vec(), val));
        cursor.del(&mut access, del::NODUPDATA)?;
        row = match cursor.next(&access).to_opt()? {
            Some(r) => r,
            None => break,
        };
    }
    Ok(result)
}

pub fn lmdb_get_typed<K, V>(
    txn: &ConstTransaction<'_>,
    db: &TypedDatabaseRef<K, V>,
    key: &K,
) -> Result<Option<V>, ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize + DeserializeOwned,
{
    let access = txn.access();
    match access.get(db.db.as_ref(), key).to_opt() {
        Ok(None) => Ok(None),
        Err(e) => {
            error!(target: LOG_TARGET, "Could not get value from lmdb: {e:?}");
            Err(ChainStorageError::AccessError(e.to_string()))
        },
        Ok(Some(v)) => match deserialize(v) {
            Ok(val) => Ok(Some(val)),
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Could not could not deserialize value from lmdb: {e:?}"
                );
                Err(ChainStorageError::AccessError(e.to_string()))
            },
        },
    }
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
            error!(target: LOG_TARGET, "Could not get value from lmdb: {e:?}");
            Err(ChainStorageError::AccessError(e.to_string()))
        },
        Ok(Some(v)) => match deserialize(v) {
            Ok(val) => Ok(Some(val)),
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Could not could not deserialize value from lmdb: {e:?}"
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
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {e:?}");
        ChainStorageError::AccessError(e.to_string())
    })?;
    let mut result = vec![];
    let row = match cursor.seek_k(&access, key) {
        Ok(r) => r,
        Err(e) => {
            if e == Error::Code(error::NOTFOUND) {
                return Ok(result);
            }
            error!(target: LOG_TARGET, "Error in lmdb_get_multiple:{e}");
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

/// Retrieves a value that may have been stored either as a single `V` (the legacy format) or as a
/// `Vec<V>` (the current format that allows multiple values to share one key). Every stored entry is
/// returned; an empty vector means the key was absent.
///
/// The current `Vec<V>` format MUST be attempted first, and this ordering is load-bearing: bincode
/// does not reject trailing bytes, so a real `Vec<V>` would also decode (incorrectly) as a single `V`
/// — only the reverse is impossible. Legacy single entries cannot be misinterpreted as a vector: a
/// single entry's bytes always begin with the entry's own (fixed) byte length, which is far larger
/// than the small element count that prefixes a real vector, so the vector decode runs out of bytes
/// and fails deterministically before any large allocation is attempted. When the `Vec<V>` decode
/// fails the bytes are interpreted as a single legacy `V`.
pub fn lmdb_get_single_or_vec<K, V>(
    txn: &ConstTransaction<'_>,
    db: &Database,
    key: &K,
) -> Result<Vec<V>, ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: DeserializeOwned,
{
    let access = txn.access();
    let bytes: &[u8] = match access.get::<K, [u8]>(db, key).to_opt() {
        Ok(None) => return Ok(Vec::new()),
        Ok(Some(b)) => b,
        Err(e) => {
            error!(target: LOG_TARGET, "Could not get value from lmdb: {e:?}");
            return Err(ChainStorageError::AccessError(e.to_string()));
        },
    };
    // Current format: a vector of entries. This decode is expected to fail for every value still in
    // the legacy format, so it must not log — hence `try_deserialize` rather than `deserialize`.
    if let Ok(entries) = try_deserialize::<Vec<V>>(bytes) {
        return Ok(entries);
    }
    // Legacy format: a single entry stored directly.
    let entry = deserialize::<V>(bytes).map_err(|e| {
        error!(target: LOG_TARGET, "Could not deserialize single-or-vec value from lmdb: {e:?}");
        ChainStorageError::AccessError(e.to_string())
    })?;
    Ok(vec![entry])
}

/// Writes index entries back in their canonical form: a single entry is stored directly (byte-for-byte
/// identical to the legacy single-entry format), multiple entries are stored as a `Vec`, and an empty
/// set deletes the key (tolerating an already-absent key). Keeping the one-entry case in the legacy
/// format means the on-disk layout — and therefore existing databases — are unchanged unless a key
/// genuinely holds more than one index. [`lmdb_get_single_or_vec`] reads either form transparently.
pub fn lmdb_write_index_entries<K, V>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    key: &K,
    entries: &[V],
    table_name: &'static str,
) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize,
{
    match entries {
        [] => match lmdb_delete(txn, db, key, table_name) {
            Ok(()) | Err(ChainStorageError::ValueNotFound { .. }) => Ok(()),
            Err(e) => Err(e),
        },
        [single] => lmdb_replace(txn, db, key, single, None),
        many => lmdb_replace(txn, db, key, &many, None),
    }
}

/// Appends `value` to the entries stored at `key`, rather than overwriting any existing entry.
/// Existing data is read with [`lmdb_get_single_or_vec`] so both the legacy single-entry and the
/// vector formats are handled, and the result is written back via [`lmdb_write_index_entries`]
/// (single entries stay in the legacy format, multiples become a `Vec`). Values already present are
/// not duplicated.
pub fn lmdb_insert_into_vec<K, V>(
    txn: &WriteTransaction<'_>,
    db: &Database,
    key: &K,
    value: V,
    table_name: &'static str,
) -> Result<(), ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize + DeserializeOwned + PartialEq,
{
    let mut entries = lmdb_get_single_or_vec::<K, V>(txn, db, key)?;
    if !entries.contains(&value) {
        entries.push(value);
    }
    lmdb_write_index_entries(txn, db, key, &entries, table_name)
}

/// Retrieves the last value stored in the database
pub fn lmdb_last<V>(txn: &ConstTransaction<'_>, db: &Database) -> Result<Option<V>, ChainStorageError>
where V: DeserializeOwned {
    let mut cursor = txn.cursor(db)?;
    let access = txn.access();
    match cursor.last::<[u8], [u8]>(&access).to_opt() {
        Err(e) => {
            error!(target: LOG_TARGET, "Could not get value from lmdb: {e:?}");
            Err(ChainStorageError::AccessError(e.to_string()))
        },
        Ok(None) => Ok(None),
        Ok(Some((_k, v))) => deserialize(v).map(Some).map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Could not could not deserialize value from lmdb: {e:?}"
            );
            ChainStorageError::AccessError(e.to_string())
        }),
    }
}

pub fn lmdb_exists_typed<K, V>(
    txn: &ConstTransaction<'_>,
    db: &TypedDatabaseRef<K, V>,
    key: &K,
) -> Result<bool, ChainStorageError>
where
    K: AsLmdbBytes + ?Sized,
    V: Serialize + DeserializeOwned,
{
    lmdb_exists(txn, &db.db, key)
}

/// Checks if the key exists in the database
pub fn lmdb_exists<K>(txn: &ConstTransaction<'_>, db: &Database, key: &K) -> Result<bool, ChainStorageError>
where K: AsLmdbBytes + ?Sized {
    let access = txn.access();
    match access.get::<K, [u8]>(db, key).to_opt() {
        Ok(None) => Ok(false),
        Err(e) => {
            error!(target: LOG_TARGET, "Could not read from lmdb: {e:?}");
            Err(ChainStorageError::AccessError(e.to_string()))
        },
        Ok(Some(_)) => Ok(true),
    }
}

/// Returns the amount of entries of the database table
pub fn lmdb_len(txn: &ConstTransaction<'_>, db: &Database) -> Result<usize, ChainStorageError> {
    let stats = txn.db_stat(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not read length from lmdb: {e:?}");
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
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {e:?}");
        ChainStorageError::AccessError(e.to_string())
    })?;

    Ok(KeyPrefixCursor::new(cursor, access, prefix_key))
}

/// Fetches values the key prefix
pub fn lmdb_fetch_matching_after<V>(
    txn: &ConstTransaction<'_>,
    db: &Database,
    key_prefix: &[u8],
) -> Result<Vec<(Vec<u8>, V)>, ChainStorageError>
where
    V: DeserializeOwned,
{
    let mut cursor = lmdb_get_prefix_cursor(txn, db, key_prefix)?;
    let mut result = vec![];
    while let Some((k, val)) = cursor.next()? {
        result.push((k, val));
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
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {e:?}");
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
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {e:?}");
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

#[allow(dead_code)]
pub fn lmdb_all<V>(txn: &ConstTransaction<'_>, db: &Database) -> Result<Vec<(Vec<u8>, V)>, ChainStorageError>
where V: DeserializeOwned {
    let access = txn.access();
    let mut cursor = txn.cursor(db).map_err(|e| {
        error!(target: LOG_TARGET, "Could not get read cursor from lmdb: {e:?}");
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
        let (k, v) = row?;
        result.push((k.to_vec(), deserialize::<V>(v)?));
    }
    Ok(result)
}

/// Fetches the size of all key/values in the given DB. Returns the number of entries, the total size of all the
/// keys and values in bytes.
pub fn fetch_db_entry_sizes(txn: &ConstTransaction<'_>, db: &Database) -> Result<(u64, u64, u64), ChainStorageError> {
    let access = txn.access();
    let mut cursor = txn.cursor(db)?;
    let mut num_entries = 0u64;
    let mut total_key_size = 0u64;
    let mut total_value_size = 0u64;
    while let Some((key, value)) = cursor.next::<[u8], [u8]>(&access).to_opt()? {
        num_entries = num_entries.saturating_add(1);
        total_key_size = total_key_size.saturating_add(key.len() as u64);
        total_value_size = total_value_size.saturating_add(value.len() as u64);
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
    let mut num_deleted = 0usize;
    while let Some((k, v)) = cursor.next::<K, [u8]>(&access).to_opt()? {
        match deserialize(v) {
            Ok(v) => match predicate(k, v) {
                Some(true) => {
                    cursor.del(&mut access, del::Flags::empty())?;
                    num_deleted = num_deleted.saturating_add(1);
                },
                Some(false) => continue,
                None => {
                    break;
                },
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Could not could not deserialize value from lmdb: {e:?}"
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
    let mut num_deleted = 0usize;
    while cursor.next::<[u8], [u8]>(&access).to_opt()?.is_some() {
        cursor.del(&mut access, del::Flags::empty())?;
        num_deleted = num_deleted.saturating_add(1);
    }
    Ok(num_deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain_storage::{lmdb_db::helpers::serialize, tests::temp_db::TempLmdbDatabase};

    // A 64-byte composite-key-like value (mirrors header_hash || output_hash entries).
    fn comp_key(seed: u8) -> Vec<u8> {
        (0..64u8).map(|b| b.wrapping_add(seed)).collect()
    }

    #[test]
    fn reads_legacy_single_entry_as_vec() {
        let db = TempLmdbDatabase::new();
        let value = comp_key(0);
        {
            let txn = db.write_transaction();
            // Store in the legacy single-entry format (a bare `Vec<u8>`).
            lmdb_replace(&txn, db.default_db(), b"k".as_slice(), &value, None).unwrap();
            txn.commit().unwrap();
        }
        let txn = db.read_transaction();
        let entries = lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice()).unwrap();
        assert_eq!(entries, vec![value]);
    }

    #[test]
    fn reads_current_vec_format() {
        let db = TempLmdbDatabase::new();
        let values = vec![comp_key(0), comp_key(1)];
        {
            let txn = db.write_transaction();
            lmdb_replace(&txn, db.default_db(), b"k".as_slice(), &values, None).unwrap();
            txn.commit().unwrap();
        }
        let txn = db.read_transaction();
        let entries = lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice()).unwrap();
        assert_eq!(entries, values);
    }

    #[test]
    fn absent_key_returns_empty_vec() {
        let db = TempLmdbDatabase::new();
        let txn = db.read_transaction();
        let entries = lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"missing".as_slice()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn insert_into_vec_appends_without_overwriting() {
        let db = TempLmdbDatabase::new();
        let (a, b, c) = (comp_key(0), comp_key(1), comp_key(2));
        {
            let txn = db.write_transaction();
            lmdb_insert_into_vec(&txn, db.default_db(), b"k".as_slice(), a.clone(), "t").unwrap();
            lmdb_insert_into_vec(&txn, db.default_db(), b"k".as_slice(), b.clone(), "t").unwrap();
            lmdb_insert_into_vec(&txn, db.default_db(), b"k".as_slice(), c.clone(), "t").unwrap();
            txn.commit().unwrap();
        }
        let txn = db.read_transaction();
        let entries = lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice()).unwrap();
        assert_eq!(entries, vec![a, b, c]);
    }

    #[test]
    fn insert_into_vec_dedups_existing_value() {
        let db = TempLmdbDatabase::new();
        let a = comp_key(0);
        {
            let txn = db.write_transaction();
            lmdb_insert_into_vec(&txn, db.default_db(), b"k".as_slice(), a.clone(), "t").unwrap();
            lmdb_insert_into_vec(&txn, db.default_db(), b"k".as_slice(), a.clone(), "t").unwrap();
            txn.commit().unwrap();
        }
        let txn = db.read_transaction();
        let entries = lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice()).unwrap();
        assert_eq!(entries, vec![a]);
    }

    #[test]
    fn insert_into_vec_upgrades_legacy_single_entry() {
        let db = TempLmdbDatabase::new();
        let (legacy, appended) = (comp_key(0), comp_key(1));
        {
            // Seed a legacy single entry, then append via the vec helper.
            let txn = db.write_transaction();
            lmdb_replace(&txn, db.default_db(), b"k".as_slice(), &legacy, None).unwrap();
            lmdb_insert_into_vec(&txn, db.default_db(), b"k".as_slice(), appended.clone(), "t").unwrap();
            txn.commit().unwrap();
        }
        let txn = db.read_transaction();
        let entries = lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice()).unwrap();
        assert_eq!(entries, vec![legacy, appended]);
    }

    #[test]
    fn legacy_bytes_never_decode_as_vec() {
        // Guards the disambiguation invariant the read helper relies on: a 64-byte legacy entry
        // (outer length prefix = 64) can never be misread as a vector of entries.
        let legacy = serialize(&comp_key(7), None).unwrap();
        assert!(try_deserialize::<Vec<Vec<u8>>>(&legacy).is_err());
        assert_eq!(deserialize::<Vec<u8>>(&legacy).unwrap(), comp_key(7));
    }

    /// Reads the raw stored bytes for `key` (no format interpretation).
    fn raw_bytes(db: &TempLmdbDatabase, key: &[u8]) -> Vec<u8> {
        let txn = db.read_transaction();
        let access = txn.access();
        access.get::<[u8], [u8]>(db.default_db(), key).unwrap().to_vec()
    }

    #[test]
    fn single_entry_is_stored_in_legacy_format_multiple_as_vec() {
        let db = TempLmdbDatabase::new();
        let (a, b) = (comp_key(0), comp_key(1));
        {
            let txn = db.write_transaction();
            lmdb_insert_into_vec(&txn, db.default_db(), b"k".as_slice(), a.clone(), "t").unwrap();
            txn.commit().unwrap();
        }
        // One entry must be byte-for-byte identical to the legacy single-entry encoding, so existing
        // databases are not rewritten for the common one-index case.
        assert_eq!(raw_bytes(&db, b"k"), serialize(&a, None).unwrap());
        {
            let txn = db.write_transaction();
            lmdb_insert_into_vec(&txn, db.default_db(), b"k".as_slice(), b.clone(), "t").unwrap();
            txn.commit().unwrap();
        }
        // Two entries switch to the vector encoding.
        assert_eq!(raw_bytes(&db, b"k"), serialize(&vec![a, b], None).unwrap());
    }

    #[test]
    fn write_index_entries_canonicalises_and_deletes() {
        let db = TempLmdbDatabase::new();
        let (a, b) = (comp_key(0), comp_key(1));
        // Two stays a vec; two -> one downgrades to legacy; one -> zero deletes the key.
        {
            let txn = db.write_transaction();
            lmdb_write_index_entries(&txn, db.default_db(), b"k".as_slice(), &[a.clone(), b.clone()], "t").unwrap();
            txn.commit().unwrap();
        }
        assert_eq!(raw_bytes(&db, b"k"), serialize(&vec![a.clone(), b], None).unwrap());
        {
            let txn = db.write_transaction();
            lmdb_write_index_entries(&txn, db.default_db(), b"k".as_slice(), std::slice::from_ref(&a), "t").unwrap();
            txn.commit().unwrap();
        }
        assert_eq!(raw_bytes(&db, b"k"), serialize(&a, None).unwrap());
        {
            let txn = db.write_transaction();
            lmdb_write_index_entries::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice(), &[], "t").unwrap();
            txn.commit().unwrap();
        }
        let txn = db.read_transaction();
        assert!(
            lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn write_index_entries_tolerates_deleting_absent_key() {
        let db = TempLmdbDatabase::new();
        let txn = db.write_transaction();
        // Deleting (empty entries) a key that was never written must not error.
        lmdb_write_index_entries::<_, Vec<u8>>(&txn, db.default_db(), b"missing".as_slice(), &[], "t").unwrap();
    }

    // A 64-byte composite key: header_hash[0..32] || output_hash[32..64].
    fn entry_for_header(header: u8, output: u8) -> Vec<u8> {
        let mut v = vec![header; 32];
        v.extend_from_slice(&[output; 32]);
        v
    }

    /// Mirrors `remove_index_entry_for_header`: retain entries whose 32-byte header prefix differs,
    /// then write canonically. Verifies removal targets only the matching entry and the key is deleted
    /// only when the last entry is gone — and that removing an absent header is a no-op.
    #[test]
    fn removing_by_header_prefix_removes_only_match_and_deletes_when_empty() {
        let db = TempLmdbDatabase::new();
        let (a, b) = (entry_for_header(1, 9), entry_for_header(2, 9));
        let remove_header = |header: u8| {
            let txn = db.write_transaction();
            let mut entries = lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice()).unwrap();
            entries.retain(|e| e.get(0..32) != Some(&[header; 32][..]));
            lmdb_write_index_entries(&txn, db.default_db(), b"k".as_slice(), &entries, "t").unwrap();
            txn.commit().unwrap();
        };
        {
            let txn = db.write_transaction();
            lmdb_write_index_entries(&txn, db.default_db(), b"k".as_slice(), &[a.clone(), b.clone()], "t").unwrap();
            txn.commit().unwrap();
        }
        // Removing a header that is not present leaves both entries untouched.
        remove_header(7);
        {
            let txn = db.read_transaction();
            assert_eq!(
                lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice()).unwrap(),
                vec![a.clone(), b.clone()]
            );
        }
        // Removing header 1 leaves only entry b (now stored in the legacy single format).
        remove_header(1);
        assert_eq!(raw_bytes(&db, b"k"), serialize(&b, None).unwrap());
        // Removing header 2 empties the vector and deletes the key.
        remove_header(2);
        {
            let txn = db.read_transaction();
            assert!(
                lmdb_get_single_or_vec::<_, Vec<u8>>(&txn, db.default_db(), b"k".as_slice())
                    .unwrap()
                    .is_empty()
            );
        }
        // Removing again on the now-absent key must not error.
        remove_header(2);
    }
}
