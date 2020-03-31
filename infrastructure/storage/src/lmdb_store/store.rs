//! An ergonomic, multithreaded API for an LMDB datastore

use crate::{
    key_val_store::{error::KeyValStoreError, key_val_store::IterationResult},
    lmdb_store::error::LMDBError,
};
use lmdb_zero::{
    db,
    error::{self, LmdbResultExt},
    open,
    put,
    traits::AsLmdbBytes,
    ConstAccessor,
    Cursor,
    CursorIter,
    Database,
    DatabaseOptions,
    EnvBuilder,
    Environment,
    Ignore,
    MaybeOwned,
    ReadTransaction,
    Stat,
    WriteAccessor,
    WriteTransaction,
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    cmp::max,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

const LOG_TARGET: &str = "lmdb";

/// An atomic pointer to an LMDB database instance
type DatabaseRef = Arc<Database<'static>>;

/// A builder for [LMDBStore](struct.lmdbstore.html)
/// ## Example
///
/// Create a new LMDB database of 500MB in the `db` directory with two named databases: "db1" and "db2"
///
/// ```
/// # use tari_storage::lmdb_store::LMDBBuilder;
/// # use lmdb_zero::db;
/// # use std::env;
/// let mut store = LMDBBuilder::new()
///     .set_path(env::temp_dir())
///     .set_environment_size(500)
///     .set_max_number_of_databases(10)
///     .add_database("db1", db::CREATE)
///     .add_database("db2", db::CREATE)
///     .build()
///     .unwrap();
/// ```
#[derive(Default)]
pub struct LMDBBuilder {
    path: PathBuf,
    db_size_mb: usize,
    max_dbs: usize,
    db_names: HashMap<String, db::Flags>,
}

impl LMDBBuilder {
    /// Create a new LMDBStore builder. Set up the database by calling `set_nnnn` and then create the database
    /// with `build()`. The default values for the database parameters are:
    ///
    /// | Parameter | Default |
    /// |:----------|---------|
    /// | path      | ./store/|
    /// | size      | 64 MB   |
    /// | named DBs | none    |
    pub fn new() -> LMDBBuilder {
        LMDBBuilder {
            path: "./store/".into(),
            db_size_mb: 64,
            db_names: HashMap::new(),
            max_dbs: 8,
        }
    }

    /// Set the directory where the LMDB database exists, or must be created.
    /// Note: The directory must exist already; it is not created for you. If it does not exist, `build()` will
    /// return `LMDBError::InvalidPath`.
    pub fn set_path<P: AsRef<Path>>(mut self, path: P) -> LMDBBuilder {
        self.path = path.as_ref().to_owned();
        self
    }

    /// Sets the size of the environment, in MB.
    /// The actual memory will only be allocated when #build() is called
    pub fn set_environment_size(mut self, size: usize) -> LMDBBuilder {
        self.db_size_mb = size;
        self
    }

    /// Sets the maximum number of databases (tables) in the environment. If this value is less than the number of
    /// DBs that will be created when the environment is built, this value will be ignored.
    pub fn set_max_number_of_databases(mut self, size: usize) -> LMDBBuilder {
        self.max_dbs = size;
        self
    }

    /// Add an additional named database to the LMDB environment.If `add_database` isn't called at least once, only the
    /// `default` database is created.
    pub fn add_database(mut self, name: &str, flags: db::Flags) -> LMDBBuilder {
        // There will always be a 'default' database
        let _ = self.db_names.insert(name.into(), flags);
        self
    }

    /// Create a new LMDBStore instance and open the underlying database environment
    pub fn build(mut self) -> Result<LMDBStore, LMDBError> {
        let max_dbs = max(self.db_names.len(), self.max_dbs) as u32;
        if !self.path.exists() {
            return Err(LMDBError::InvalidPath);
        }
        let path = self
            .path
            .to_str()
            .map(String::from)
            .ok_or_else(|| LMDBError::InvalidPath)?;

        let env = unsafe {
            let mut builder = EnvBuilder::new()?;
            builder.set_mapsize(self.db_size_mb * 1024 * 1024)?;
            builder.set_maxdbs(max_dbs)?;
            // Using open::Flags::NOTLS does not compile!?! NOTLS=0x200000
            let flags = open::Flags::from_bits(0x200_000).expect("LMDB open::Flag is correct");
            builder.open(&path, flags, 0o600)?
        };
        let env = Arc::new(env);
        info!(
            target: LOG_TARGET,
            "({}) LMDB environment created with a capacity of {} MB.", path, self.db_size_mb
        );
        let mut databases: HashMap<String, LMDBDatabase> = HashMap::new();
        if self.db_names.is_empty() {
            self = self.add_database("default", db::CREATE);
        }
        for (name, flags) in self.db_names.iter() {
            let db = Database::open(env.clone(), Some(name), &DatabaseOptions::new(*flags))?;
            let db = LMDBDatabase {
                name: name.to_string(),
                env: env.clone(),
                db: Arc::new(db),
            };
            databases.insert(name.to_string(), db);
            trace!(target: LOG_TARGET, "({}) LMDB database '{}' is ready", path, name);
        }
        Ok(LMDBStore { path, env, databases })
    }
}

/// A Struct for holding state for an LM Database. LMDB is memory mapped, so you can treat the DB as an (essentially)
/// infinitely large memory-backed hashmap. A single environment is stored in one file. The individual databases
/// are key-value tables stored within the file.
///
/// LMDB databases are thread-safe.
///
/// To create an instance of LMDBStore, use [LMDBBuilder](struct.lmdbbuilder.html).
///
/// ## Memory efficiency
///
/// LMDB really only understands raw byte arrays. Complex structures need to be referenced as (what looks like) a
/// single contiguous blob of memory. This presents some trade offs we need to make when `insert`ing and `get`ting
/// data to/from LMDB.
///
/// ### Writing
///
/// For simple types, like `PublickKey([u8; 32])`, it's most efficient to pass a pointer to the memory position; and
/// LMDB will do (at most) a single copy into its memory structures. the lmdb-zero crate assumes this by only
/// requiring the `AsLmdbBytes` trait when `insert`ing data. i.e. `insert` does does take ownership of the key or
/// value; it just wants to be able to read the `[u8]`.
///
/// This poses something of a problem for complex structures. Structs typically don't have a contiguous block of
/// memory backing the instance, and so you either need to impose one (which isn't a great idea-- now you have to write
/// some sort of memory management software), or you eat the cost of doing an intermediate copy into a buffer every
/// time you need to commit a structure to LMDB.
///
/// However, this cost is mitigated if there's any kind of processing that needs to be done in converting `T` to
/// `[u8]` (e.g. if an IP address is stored as a string for some reason, you might want to represent it as `[u8; 4]`)
/// , which probably happens more often than we think, and offers maximum flexibility.
///
/// Furthermore, the "simple" types are typically quite small, so an additional copy is not usually incurring much
/// overhead.
///
/// So this library makes the trade-off of carrying out two copies per write whilst gaining a significant amount of
/// flexibility in the process.
///
/// ### Reading
///
/// When LMDB returns data from a `get` request, it returns a `&[u8]` - you cannot take ownership of this data.
/// Therefore we necessarily need to copy data anyway in order to pull data into the final Struct instance.
/// So the `From<&[u8]> for T` trait implementation will work for reading, and this works fine for both simple and
/// complex data structures.
///
/// `FromLmdbBytes` is not quite what we want because the trait function returns a reference to an object, rather
/// than the object itself.
///
/// An additional consideration is: how was this data serialised? If the writing was a straight memory dump, we
/// don't always have enough information to reconstruct our data object (how long was a string? How many elements
/// were in the array? Was it big- or little-endian ordering of integers?).
///
/// If we have to store this metadata when reading in byte strings, it means it had to be stored too. This is a
/// further roadblock to the "zero-copy" ideal for writing. And since we're now basically serialising and
/// de-serialising, we may as well use a well-known, highly efficient binary format to do so.
///
/// ## Serialisation
///
/// The ideal serialiasation format is the one that does the least "bit-twiddling" between memory and the byte array;
/// as well as being as compact as possible.
///
/// Candidates include: Bincode, MsgPack, and Protobuf / Cap'nProto. Without spending ages on a comparison, I just
/// took the benchmark results from [this project](https://github.com/erickt/rust-serialization-benchmarks):
///
/// ```text
/// test clone                             ... bench:       1,179 ns/iter (+/- 115) = 444 MB/s
///
/// test capnp_deserialize                 ... bench:         277 ns/iter (+/- 27) = 1617 MB/s  **
/// test flatbuffers_deserialize           ... bench:           0 ns/iter (+/- 0) = 472000 MB/s ***
/// test rust_bincode_deserialize          ... bench:       1,533 ns/iter (+/- 228) = 260 MB/s
/// test rmp_serde_deserialize             ... bench:       1,859 ns/iter (+/- 186) = 154 MB/s
/// test rust_protobuf_deserialize         ... bench:         558 ns/iter (+/- 29) = 512 MB/s   *
/// test serde_json_deserialize            ... bench:       2,244 ns/iter (+/- 249) = 269 MB/s
///
/// test capnp_serialize                   ... bench:          28 ns/iter (+/- 5) = 16000 MB/s  **
/// test flatbuffers_serialize             ... bench:           0 ns/iter (+/- 0) = 472000 MB/s ***
/// test rmp_serde_serialize               ... bench:         278 ns/iter (+/- 27) = 1032 MB/s
/// test rust_bincode_serialize            ... bench:         190 ns/iter (+/- 43) = 2105 MB/s  *
/// test rust_protobuf_serialize           ... bench:         468 ns/iter (+/- 18) = 611 MB/s
/// test serde_json_serialize              ... bench:       1,012 ns/iter (+/- 55) = 597 MB/s
/// ```
///
/// Based on these benchmarks, Flatbuffers and Cap'nProto are far and away the quickest. However, looking at the
/// benchmarks more closely, we see that these aren't strictly Orange to Orange comparisons. The flatbuffers and
/// capnproto tests don't actually serialise to and from the general Rust struct (an HTTP request type template), but
/// from specially generated structs based on the schema.
///
/// Strictly speaking, if we're going to serialise arbitrary key-value types, these benchmarks should include the
/// time it takes to populate a flatbuffer / capnproto structure.
///
/// A quick modification of the benchmarks to take this int account this reveals:
///
/// ```text
/// test rust_bincode_deserialize          ... bench:       1,505 ns/iter (+/- 361) = 265 MB/s *
/// test capnp_deserialize                 ... bench:         282 ns/iter (+/- 37) = 1588 MB/s ***
/// test rmp_serde_deserialize             ... bench:       1,800 ns/iter (+/- 144) = 159 MB/s *
///
/// test capnp_serialize                   ... bench:         941 ns/iter (+/- 40) = 476 MB/s  *
/// test rmp_serde_serialize               ... bench:         269 ns/iter (+/- 19) = 1066 MB/s **
/// test rust_bincode_serialize            ... bench:         191 ns/iter (+/- 41) = 1114 MB/s ***
/// ```
///
/// Now bincode emerges as a reasonable contender. Another positive to bincode is that one doesn't have to update and
/// maintain a schema for the data types begin serialized, nor is a separate compilation step required.
///
/// So after all this, we'll use bincode for the time being to handle serialisation to- and from- LMDB
pub struct LMDBStore {
    path: String,
    pub(crate) env: Arc<Environment>,
    pub(crate) databases: HashMap<String, LMDBDatabase>,
}

/// Close all databases and close the environment. You cannot be guaranteed that the dbs will be closed after calling
/// this function because there still may be threads accessing / writing to a database that will block this call.
/// However, in that case `shutdown` returns an error.
impl LMDBStore {
    pub fn flush(&self) -> Result<(), lmdb_zero::error::Error> {
        debug!(target: LOG_TARGET, "Forcing flush of buffers to disk");
        self.env.sync(true)?;
        debug!(target: LOG_TARGET, "Buffers have been flushed");
        Ok(())
    }

    pub fn log_info(&self) {
        match self.env.info() {
            Err(e) => warn!(
                target: LOG_TARGET,
                "Could not retrieve LMDB information for {}. {}",
                self.path,
                e.to_string()
            ),
            Ok(info) => {
                let size_mb = info.mapsize / 1024 / 1024;
                info!(
                    target: LOG_TARGET,
                    "LMDB Environment information ({}). Map Size={} MB. Last page no={}. Last tx id={}",
                    self.path,
                    size_mb,
                    info.last_pgno,
                    info.last_txnid
                )
            },
        }
        match self.env.stat() {
            Err(e) => warn!(
                target: LOG_TARGET,
                "Could not retrieve LMDB statistics for {}. {}",
                self.path,
                e.to_string()
            ),
            Ok(stats) => {
                let page_size = stats.psize / 1024;
                info!(
                    target: LOG_TARGET,
                    "LMDB Environment statistics ({}). Page size={}kB. Tree depth={}. Branch pages={}. Leaf Pages={}, \
                     Overflow pages={}, Entries={}",
                    self.path,
                    page_size,
                    stats.depth,
                    stats.branch_pages,
                    stats.leaf_pages,
                    stats.overflow_pages,
                    stats.entries
                );
            },
        }
    }

    /// Returns a handle to the database given in `db_name`, if it exists, otherwise return None.
    pub fn get_handle(&self, db_name: &str) -> Option<LMDBDatabase> {
        match self.databases.get(db_name) {
            Some(db) => Some(db.clone()),
            None => None,
        }
    }

    pub fn env(&self) -> Arc<Environment> {
        self.env.clone()
    }
}

#[derive(Clone)]
pub struct LMDBDatabase {
    name: String,
    env: Arc<Environment>,
    db: DatabaseRef,
}

impl LMDBDatabase {
    /// Inserts a record into the database. This is an atomic operation. Internally, `insert` creates a new
    /// write transaction, writes the value, and then commits the transaction.
    pub fn insert<K, V>(&self, key: &K, value: &V) -> Result<(), LMDBError>
    where
        K: AsLmdbBytes + ?Sized,
        V: Serialize,
    {
        let env = &(*self.db.env());
        let tx = WriteTransaction::new(env)?;
        {
            let mut accessor = tx.access();
            let buf = LMDBWriteTransaction::convert_value(value, 512)?;
            accessor.put(&*self.db, key, &buf, put::Flags::empty())?;
        }
        tx.commit().map_err(LMDBError::from)
    }

    /// Get a value from the database. This is an atomic operation. A read transaction is created, the value
    /// extracted, copied and converted to V before closing the transaction. A copy is unavoidable because the
    /// extracted byte string is released when the transaction is closed. If you are doing many `gets`, it is more
    /// efficient to use `with_read_transaction`
    pub fn get<K, V>(&self, key: &K) -> Result<Option<V>, LMDBError>
    where
        K: AsLmdbBytes + ?Sized,
        for<'t> V: DeserializeOwned, // read this as, for *any* lifetime, t, we can convert a [u8] to V
    {
        let env = &(*self.db.env());
        let txn = ReadTransaction::new(env)?;
        let accessor = txn.access();
        let val = accessor.get(&self.db, key).to_opt();
        LMDBReadTransaction::convert_value(val)
    }

    /// Return statistics about the database, See [Stat](lmdb_zero/struct.Stat.html) for more details.
    pub fn get_stats(&self) -> Result<Stat, LMDBError> {
        let env = &(*self.db.env());
        ReadTransaction::new(env)
            .and_then(|txn| txn.db_stat(&self.db))
            .map_err(LMDBError::DatabaseError)
    }

    /// Log some pretty printed stats.See [Stat](lmdb_zero/struct.Stat.html) for more details.
    pub fn log_info(&self) {
        match self.get_stats() {
            Err(e) => warn!(
                target: LOG_TARGET,
                "Could not retrieve LMDB statistics for {}. {}",
                self.name,
                e.to_string()
            ),
            Ok(stats) => {
                let page_size = stats.psize / 1024;
                info!(
                    target: LOG_TARGET,
                    "LMDB Database statistics ({}). Page size={}kB. Tree depth={}. Branch pages={}. Leaf Pages={}, \
                     Overflow pages={}, Entries={}",
                    self.name,
                    page_size,
                    stats.depth,
                    stats.branch_pages,
                    stats.leaf_pages,
                    stats.overflow_pages,
                    stats.entries
                );
            },
        }
    }

    /// Returns if the database is empty.
    pub fn is_empty(&self) -> Result<bool, LMDBError> {
        self.get_stats().and_then(|s| Ok(s.entries > 0))
    }

    /// Returns the total number of entries in this database.
    pub fn len(&self) -> Result<usize, LMDBError> {
        self.get_stats().and_then(|s| Ok(s.entries))
    }

    /// Execute function `f` for each value in the database.
    ///
    /// The underlying LMDB library does not permit database cursors to be returned from functions to preserve Rust
    /// memory guarantees, so this is the closest thing to an iterator that you're going to get :/
    ///
    /// `f` is a closure of form `|pair: Result<(K,V), LMDBError>| -> IterationResult`. If `IterationResult::Break` is
    /// returned the closure will not be called again and `for_each` will return. You will usually need to include
    /// type inference to let Rust know which type to deserialise to:
    /// ```nocompile
    ///    let res = db.for_each::<Key, User, _>(|pair| {
    ///        let (key, user) = pair.unwrap();
    ///        //.. do stuff with key and user..
    ///    });
    pub fn for_each<K, V, F>(&self, mut f: F) -> Result<(), LMDBError>
    where
        K: DeserializeOwned,
        V: DeserializeOwned,
        F: FnMut(Result<(K, V), KeyValStoreError>) -> IterationResult,
    {
        let env = self.env.clone();
        let db = self.db.clone();
        let txn = ReadTransaction::new(env).map_err(LMDBError::DatabaseError)?;

        let access = txn.access();
        let cursor = txn.cursor(db).map_err(LMDBError::DatabaseError)?;

        let head = |c: &mut Cursor, a: &ConstAccessor| {
            let (key_bytes, val_bytes) = c.first(a)?;
            ReadOnlyIterator::deserialize::<K, V>(key_bytes, val_bytes)
        };

        let cursor = MaybeOwned::Owned(cursor);
        let iter = CursorIter::new(cursor, &access, head, ReadOnlyIterator::next).map_err(LMDBError::DatabaseError)?;

        for p in iter {
            match f(p.map_err(|e| KeyValStoreError::DatabaseError(e.to_string()))) {
                IterationResult::Break => break,
                IterationResult::Continue => {},
            }
        }

        Ok(())
    }

    /// Checks whether a key exists in this database
    pub fn contains_key<K>(&self, key: &K) -> Result<bool, LMDBError>
    where K: AsLmdbBytes + ?Sized {
        let txn = ReadTransaction::new(&(*self.db.env()))?;
        let accessor = txn.access();
        let res: error::Result<&Ignore> = accessor.get(&self.db, key);
        let res = res.to_opt()?.is_some();
        Ok(res)
    }

    /// Delete a record associated with `key` from the database. If the key is not found,
    pub fn remove<K>(&self, key: &K) -> Result<(), LMDBError>
    where K: AsLmdbBytes + ?Sized {
        let tx = WriteTransaction::new(&(*self.db.env()))?;
        {
            let mut accessor = tx.access();
            accessor.del_key(&self.db, key)?;
        }
        tx.commit().map_err(Into::into)
    }

    /// Create a read-only transaction on the current database and execute the instructions given in the closure. The
    /// transaction is automatically committed when the closure goes out of scope. You may provide the results of the
    /// transaction to the calling scope by populating a `Vec<V>` with the results of `txn.get(k)`. Otherwise, if the
    /// results are not needed, or you did not call `get`, just return `Ok(None)`.
    pub fn with_read_transaction<F, V>(&self, f: F) -> Result<Option<Vec<V>>, LMDBError>
    where
        V: serde::de::DeserializeOwned,
        F: FnOnce(LMDBReadTransaction) -> Result<Option<Vec<V>>, LMDBError>,
    {
        let txn = ReadTransaction::new(self.env.clone())?;
        let access = txn.access();
        let wrapper = LMDBReadTransaction { db: &self.db, access };
        f(wrapper)
    }

    /// Create a transaction with write access on the current table.
    pub fn with_write_transaction<F>(&self, f: F) -> Result<(), LMDBError>
    where F: FnOnce(LMDBWriteTransaction) -> Result<(), LMDBError> {
        let txn = WriteTransaction::new(self.env.clone())?;
        let access = txn.access();
        let wrapper = LMDBWriteTransaction { db: &self.db, access };
        f(wrapper)?;
        txn.commit().map_err(|e| LMDBError::CommitError(e.to_string()))
    }

    pub fn db(&self) -> &DatabaseRef {
        &self.db
    }
}

/// Helper functions for the `for_each` method
struct ReadOnlyIterator {}
impl ReadOnlyIterator {
    fn deserialize<K, V>(key_bytes: &[u8], val_bytes: &[u8]) -> Result<(K, V), error::Error>
    where
        for<'t> K: serde::de::DeserializeOwned,
        for<'t> V: serde::de::DeserializeOwned,
    {
        let key = bincode::deserialize(key_bytes).map_err(|e| error::Error::ValRejected(e.to_string()))?;
        let val = bincode::deserialize(val_bytes).map_err(|e| error::Error::ValRejected(e.to_string()))?;
        Ok((key, val))
    }

    fn next<'r, K, V>(c: &mut Cursor, access: &'r ConstAccessor) -> Result<(K, V), error::Error>
    where
        K: serde::de::DeserializeOwned,
        V: serde::de::DeserializeOwned,
    {
        let (key_bytes, val_bytes) = c.next(access)?;
        ReadOnlyIterator::deserialize(key_bytes, val_bytes)
    }
}

pub struct LMDBReadTransaction<'txn, 'db: 'txn> {
    db: &'db Database<'db>,
    access: ConstAccessor<'txn>,
}

impl<'txn, 'db: 'txn> LMDBReadTransaction<'txn, 'db> {
    /// Get and deserialise a value from the database.
    pub fn get<K, V>(&self, key: &K) -> Result<Option<V>, LMDBError>
    where
        K: AsLmdbBytes + ?Sized,
        for<'t> V: serde::de::DeserializeOwned, // read this as, for *any* lifetime, t, we can convert a [u8] to V
    {
        let val = self.access.get(&self.db, key).to_opt();
        LMDBReadTransaction::convert_value(val)
    }

    /// Checks whether a key exists in this database
    pub fn exists<K>(&self, key: &K) -> Result<bool, LMDBError>
    where K: AsLmdbBytes + ?Sized {
        let res: error::Result<&Ignore> = self.access.get(&self.db, key);
        let res = res.to_opt()?.is_some();
        Ok(res)
    }

    fn convert_value<V>(val: Result<Option<&[u8]>, error::Error>) -> Result<Option<V>, LMDBError>
    where for<'t> V: serde::de::DeserializeOwned /* read this as, for *any* lifetime, t, we can convert a [u8] to V */
    {
        match val {
            Ok(None) => Ok(None),
            Err(e) => Err(LMDBError::GetError(format!("LMDB get error: {}", e.to_string()))),
            Ok(Some(v)) => match bincode::deserialize(v) {
                // The reference to v is about to be dropped, so we must copy the data now
                Ok(val) => Ok(Some(val)),
                Err(e) => Err(LMDBError::GetError(format!("LMDB get error: {}", e))),
            },
        }
    }
}

pub struct LMDBWriteTransaction<'txn, 'db: 'txn> {
    db: &'db Database<'db>,
    access: WriteAccessor<'txn>,
}

impl<'txn, 'db: 'txn> LMDBWriteTransaction<'txn, 'db> {
    pub fn insert<K, V>(&mut self, key: &K, value: &V) -> Result<(), LMDBError>
    where
        K: AsLmdbBytes + ?Sized,
        V: serde::Serialize,
    {
        let buf = LMDBWriteTransaction::convert_value(value, 512)?;
        self.access.put(&self.db, key, &buf, put::Flags::empty())?;
        Ok(())
    }

    /// Checks whether a key exists in this database
    pub fn exists<K>(&self, key: &K) -> Result<bool, LMDBError>
    where K: AsLmdbBytes + ?Sized {
        let res: error::Result<&Ignore> = self.access.get(&self.db, key);
        let res = res.to_opt()?.is_some();
        Ok(res)
    }

    pub fn delete<K>(&mut self, key: &K) -> Result<(), LMDBError>
    where K: AsLmdbBytes + ?Sized {
        self.access.del_key(&self.db, key).map_err(LMDBError::DatabaseError)
    }

    fn convert_value<V>(value: &V, size_estimate: usize) -> Result<Vec<u8>, LMDBError>
    where V: serde::Serialize {
        let mut buf = Vec::with_capacity(size_estimate);
        bincode::serialize_into(&mut buf, value).map_err(|e| LMDBError::SerializationErr(e.to_string()))?;
        Ok(buf)
    }
}

#[cfg(test)]
mod test {
    use crate::lmdb_store::LMDBBuilder;
    use lmdb_zero::db;
    use std::env;

    #[test]
    fn test_lmdb_builder() {
        let mut store = LMDBBuilder::new()
            .set_path(env::temp_dir())
            .set_environment_size(500)
            .set_max_number_of_databases(10)
            .add_database("db1", db::CREATE)
            .add_database("db2", db::CREATE)
            .build()
            .unwrap();
        assert!(&store.databases.len() == &2);
    }
}
