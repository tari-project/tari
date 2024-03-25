// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! An ergonomic, multithreaded API for an LMDB datastore

use std::{
    cmp::max,
    collections::HashMap,
    convert::TryInto,
    path::{Path, PathBuf},
    sync::Arc,
};

use lmdb_zero::{
    db,
    error,
    error::LmdbResultExt,
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
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    key_val_store::{error::KeyValStoreError, key_val_store::IterationResult},
    lmdb_store::error::LMDBError,
};

const LOG_TARGET: &str = "lmdb";
const BYTES_PER_MB: usize = 1024 * 1024;

/// An atomic pointer to an LMDB database instance
pub type DatabaseRef = Arc<Database<'static>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LMDBConfig {
    init_size_bytes: usize,
    grow_size_bytes: usize,
    resize_threshold_bytes: usize,
}

impl LMDBConfig {
    /// Specify LMDB config in bytes.
    pub fn new(init_size_bytes: usize, grow_size_bytes: usize, resize_threshold_bytes: usize) -> Self {
        Self {
            init_size_bytes,
            grow_size_bytes,
            resize_threshold_bytes,
        }
    }

    /// Specify LMDB config in megabytes.
    pub fn new_from_mb(init_size_mb: usize, grow_size_mb: usize, resize_threshold_mb: usize) -> Self {
        Self {
            init_size_bytes: init_size_mb * BYTES_PER_MB,
            grow_size_bytes: grow_size_mb * BYTES_PER_MB,
            resize_threshold_bytes: resize_threshold_mb * BYTES_PER_MB,
        }
    }

    /// Get the initial size of the LMDB environment in bytes.
    pub fn init_size_bytes(&self) -> usize {
        self.init_size_bytes
    }

    /// Get the grow size of the LMDB environment in bytes. The LMDB environment will be resized by this amount when
    /// `resize_threshold_bytes` are left.
    pub fn grow_size_bytes(&self) -> usize {
        self.grow_size_bytes
    }

    /// Get the resize threshold in bytes. The LMDB environment will be resized when this much free space is left.
    pub fn resize_threshold_bytes(&self) -> usize {
        self.resize_threshold_bytes
    }
}

impl Default for LMDBConfig {
    fn default() -> Self {
        Self::new_from_mb(16, 16, 8)
    }
}

/// A builder for [LMDBStore](struct.lmdbstore.html)
/// ## Example
///
/// Create a new LMDB database of 64MB in the `db` directory with two named databases: "db1" and "db2"
///
/// ```
/// # use tari_storage::lmdb_store::{LMDBBuilder, LMDBConfig};
/// # use lmdb_zero::db;
/// # use std::env;
/// let mut store = LMDBBuilder::new()
///     .set_path(env::temp_dir())
///     .set_env_config(LMDBConfig::default())
///     .set_max_number_of_databases(10)
///     .add_database("db1", db::CREATE)
///     .add_database("db2", db::CREATE)
///     .build()
///     .unwrap();
/// ```
pub struct LMDBBuilder {
    path: PathBuf,
    env_flags: open::Flags,
    max_dbs: usize,
    db_names: HashMap<String, db::Flags>,
    env_config: LMDBConfig,
}

impl LMDBBuilder {
    /// Create a new LMDBStore builder. Set up the database by calling `set_nnnn` and then create the database
    /// with `build()`. The default values for the database parameters are:
    ///
    /// | Parameter | Default |
    /// |:----------|---------|
    /// | path      | ./store/|
    /// | named DBs | none    |
    pub fn new() -> LMDBBuilder {
        Default::default()
    }

    /// Set the directory where the LMDB database exists, or must be created.
    /// Note: The directory must exist already; it is not created for you. If it does not exist, `build()` will
    /// return `LMDBError::InvalidPath`.
    pub fn set_path<P: AsRef<Path>>(mut self, path: P) -> LMDBBuilder {
        self.path = path.as_ref().to_owned();
        self
    }

    /// Set environment flags
    pub fn set_env_flags(mut self, flags: open::Flags) -> LMDBBuilder {
        self.env_flags = flags;
        self
    }

    /// Sets the parameters of the LMDB environment.
    /// The actual memory will only be allocated when #build() is called
    pub fn set_env_config(mut self, config: LMDBConfig) -> LMDBBuilder {
        self.env_config = config;
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
        let max_dbs = max(self.db_names.len(), self.max_dbs).try_into().unwrap();
        if !self.path.exists() {
            return Err(LMDBError::InvalidPath);
        }
        let path = self.path.to_str().map(String::from).ok_or(LMDBError::InvalidPath)?;

        let env = unsafe {
            let mut builder = EnvBuilder::new()?;
            builder.set_mapsize(self.env_config.init_size_bytes)?;
            builder.set_maxdbs(max_dbs)?;
            // Always include NOTLS flag since we know that we're using this with tokio
            let flags = self.env_flags | open::NOTLS;
            let env = builder.open(&path, flags, 0o600)?;
            // SAFETY: no transactions can be open at this point
            LMDBStore::resize_if_required(&env, &self.env_config)?;
            Arc::new(env)
        };

        debug!(
            target: LOG_TARGET,
            "({}) LMDB environment created with a capacity of {} MB, {} MB remaining.",
            path,
            env.info()?.mapsize / BYTES_PER_MB,
            (env.info()?.mapsize - env.stat()?.psize as usize * env.info()?.last_pgno) / BYTES_PER_MB,
        );

        let mut databases: HashMap<String, LMDBDatabase> = HashMap::new();
        if self.db_names.is_empty() {
            self = self.add_database("default", db::CREATE);
        }
        for (name, flags) in &self.db_names {
            let db = Database::open(env.clone(), Some(name), &DatabaseOptions::new(*flags))?;
            let db = LMDBDatabase {
                name: name.to_string(),
                env_config: self.env_config.clone(),
                env: env.clone(),
                db: Arc::new(db),
            };
            databases.insert(name.to_string(), db);
            trace!(target: LOG_TARGET, "({}) LMDB database '{}' is ready", path, name);
        }
        Ok(LMDBStore {
            path,
            env_config: self.env_config,
            env,
            databases,
        })
    }
}

impl Default for LMDBBuilder {
    fn default() -> Self {
        Self {
            path: "./store/".into(),
            env_flags: open::Flags::empty(),
            db_names: HashMap::new(),
            max_dbs: 8,
            env_config: LMDBConfig::default(),
        }
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
/// The ideal serialisation format is the one that does the least "bit-twiddling" between memory and the byte array;
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
    env_config: LMDBConfig,
    env: Arc<Environment>,
    databases: HashMap<String, LMDBDatabase>,
}

impl LMDBStore {
    /// Close all databases and close the environment. You cannot be guaranteed that the dbs will be closed after
    /// calling this function because there still may be threads accessing / writing to a database that will block
    /// this call. However, in that case `shutdown` returns an error.
    pub fn flush(&self) -> Result<(), lmdb_zero::error::Error> {
        trace!(target: LOG_TARGET, "Forcing flush of buffers to disk");
        self.env.sync(true)?;
        debug!(target: LOG_TARGET, "LMDB Buffers have been flushed");
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
                let size_mb = info.mapsize / BYTES_PER_MB;
                debug!(
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
                debug!(
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
        self.databases.get(db_name).cloned()
    }

    pub fn env_config(&self) -> LMDBConfig {
        self.env_config.clone()
    }

    pub fn env(&self) -> Arc<Environment> {
        self.env.clone()
    }

    /// Resize the LMDB environment if remaining mapsize is less than the configured resize threshold.
    ///
    /// # Safety
    /// This may only be called if no write transactions are active in the current process. Note that the library does
    /// not check for this condition, the caller must ensure it explicitly.
    ///
    /// <http://www.lmdb.tech/doc/group__mdb.html#gaa2506ec8dab3d969b0e609cd82e619e5>
    pub unsafe fn resize_if_required(env: &Environment, config: &LMDBConfig) -> Result<(), LMDBError> {
        let env_info = env.info()?;
        let stat = env.stat()?;
        let size_used_bytes = stat.psize as usize * env_info.last_pgno;
        let size_left_bytes = env_info.mapsize - size_used_bytes;

        if size_left_bytes <= config.resize_threshold_bytes {
            debug!(
                target: LOG_TARGET,
                "Resize required: Used bytes: {}, Remaining bytes: {}", size_used_bytes, size_left_bytes
            );
            Self::resize(env, config, None)?;
        }
        Ok(())
    }

    /// Grows the LMDB environment by the configured amount
    ///
    /// # Safety
    /// This may only be called if no write transactions are active in the current process. Note that the library does
    /// not check for this condition, the caller must ensure it explicitly.
    ///
    /// <http://www.lmdb.tech/doc/group__mdb.html#gaa2506ec8dab3d969b0e609cd82e619e5>
    pub unsafe fn resize(env: &Environment, config: &LMDBConfig, shortfall: Option<usize>) -> Result<(), LMDBError> {
        let env_info = env.info()?;
        let current_mapsize = env_info.mapsize;
        env.set_mapsize(current_mapsize + config.grow_size_bytes + shortfall.unwrap_or_default())?;
        let env_info = env.info()?;
        let new_mapsize = env_info.mapsize;
        debug!(
            target: LOG_TARGET,
            "({}) LMDB MB, mapsize was grown from {:?} MB to {:?} MB, increased by {:?} MB",
            env.path()?.to_str()?,
            current_mapsize / BYTES_PER_MB,
            new_mapsize / BYTES_PER_MB,
            (config.grow_size_bytes + shortfall.unwrap_or_default()) / BYTES_PER_MB,
        );

        Ok(())
    }
}

#[derive(Clone)]
pub struct LMDBDatabase {
    name: String,
    env_config: LMDBConfig,
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
        const MAX_RESIZES: usize = 5;
        let value = LMDBWriteTransaction::convert_value(value)?;
        for _ in 0..MAX_RESIZES {
            match self.write(key, &value) {
                Ok(txn) => return Ok(txn),
                Err(error::Error::Code(error::MAP_FULL)) => {
                    info!(
                        target: LOG_TARGET,
                        "Failed to obtain write transaction because the database needs to be resized"
                    );
                    unsafe {
                        LMDBStore::resize(&self.env, &self.env_config, Some(value.len()))?;
                    }
                },
                Err(e) => return Err(e.into()),
            }
        }

        // Failed to resize
        Err(error::Error::Code(error::MAP_FULL).into())
    }

    #[allow(clippy::ptr_arg)]
    fn write<K>(&self, key: &K, value: &Vec<u8>) -> Result<(), lmdb_zero::Error>
    where K: AsLmdbBytes + ?Sized {
        let env = self.db.env();
        let tx = WriteTransaction::new(env)?;
        {
            let mut accessor = tx.access();
            accessor.put(&self.db, key, value, put::Flags::empty())?;
        }
        tx.commit()?;
        Ok(())
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
        let env = self.db.env();
        let txn = ReadTransaction::new(env)?;
        let accessor = txn.access();
        let val = accessor.get(&self.db, key).to_opt();
        LMDBReadTransaction::convert_value(val)
    }

    /// Return statistics about the database, See [Stat](lmdb_zero/struct.Stat.html) for more details.
    pub fn get_stats(&self) -> Result<Stat, LMDBError> {
        let env = self.db.env();
        Ok(ReadTransaction::new(env).and_then(|txn| txn.db_stat(&self.db))?)
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
                debug!(
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
        self.get_stats().map(|s| s.entries > 0)
    }

    /// Returns the total number of entries in this database.
    pub fn len(&self) -> Result<usize, LMDBError> {
        self.get_stats().map(|s| s.entries)
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
        let txn = ReadTransaction::new(env)?;

        let access = txn.access();
        let cursor = txn.cursor(db)?;

        let head = |c: &mut Cursor, a: &ConstAccessor| {
            let (key_bytes, val_bytes) = c.first(a)?;
            ReadOnlyIterator::deserialize::<K, V>(key_bytes, val_bytes)
        };

        let cursor = MaybeOwned::Owned(cursor);
        let iter = CursorIter::new(cursor, &access, head, ReadOnlyIterator::next)?;

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
        let txn = ReadTransaction::new(self.db.env())?;
        let accessor = txn.access();
        let res: error::Result<&Ignore> = accessor.get(&self.db, key);
        let res = res.to_opt()?.is_some();
        Ok(res)
    }

    /// Delete a record associated with `key` from the database. If the key is not found,
    pub fn remove<K>(&self, key: &K) -> Result<(), LMDBError>
    where K: AsLmdbBytes + ?Sized {
        let tx = WriteTransaction::new(self.db.env())?;
        {
            let mut accessor = tx.access();
            accessor.del_key(&self.db, key)?;
        }
        tx.commit().map_err(Into::into)
    }

    /// Create a read-only transaction on the current database and execute the instructions given in the closure. The
    /// transaction is automatically committed when the closure goes out of scope.
    pub fn with_read_transaction<F, R>(&self, f: F) -> Result<R, LMDBError>
    where F: FnOnce(LMDBReadTransaction) -> R {
        let txn = ReadTransaction::new(self.env.clone())?;
        let access = txn.access();
        let wrapper = LMDBReadTransaction { db: &self.db, access };
        Ok(f(wrapper))
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

    /// Returns an owned atomic reference to the database
    pub fn db(&self) -> DatabaseRef {
        self.db.clone()
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

    fn next<K, V>(c: &mut Cursor, access: &ConstAccessor) -> Result<(K, V), error::Error>
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
        let val = self.access.get(self.db, key).to_opt();
        LMDBReadTransaction::convert_value(val)
    }

    /// Checks whether a key exists in this database
    pub fn exists<K>(&self, key: &K) -> Result<bool, LMDBError>
    where K: AsLmdbBytes + ?Sized {
        let res: error::Result<&Ignore> = self.access.get(self.db, key);
        let res = res.to_opt()?.is_some();
        Ok(res)
    }

    fn convert_value<V>(val: Result<Option<&[u8]>, error::Error>) -> Result<Option<V>, LMDBError>
    where for<'t> V: serde::de::DeserializeOwned /* read this as, for *any* lifetime, t, we can convert a [u8] to V */
    {
        match val {
            Ok(None) => Ok(None),
            Err(e) => Err(LMDBError::GetError(format!("LMDB get error: {}", e))),
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
        let buf = Self::convert_value(value)?;
        self.access.put(self.db, key, &buf, put::Flags::empty())?;
        Ok(())
    }

    /// Checks whether a key exists in this database
    pub fn exists<K>(&self, key: &K) -> Result<bool, LMDBError>
    where K: AsLmdbBytes + ?Sized {
        let res: error::Result<&Ignore> = self.access.get(self.db, key);
        let res = res.to_opt()?.is_some();
        Ok(res)
    }

    pub fn delete<K>(&mut self, key: &K) -> Result<(), LMDBError>
    where K: AsLmdbBytes + ?Sized {
        Ok(self.access.del_key(self.db, key)?)
    }

    fn convert_value<V>(value: &V) -> Result<Vec<u8>, LMDBError>
    where V: serde::Serialize {
        let size = bincode::serialized_size(value).map_err(|e| LMDBError::SerializationErr(e.to_string()))?;
        let mut buf = Vec::with_capacity(size.try_into().unwrap());
        bincode::serialize_into(&mut buf, value).map_err(|e| LMDBError::SerializationErr(e.to_string()))?;
        Ok(buf)
    }
}

#[cfg(test)]
mod test {
    use std::env;

    use lmdb_zero::db;

    use crate::lmdb_store::{LMDBBuilder, LMDBConfig};

    #[test]
    fn test_lmdb_builder() {
        let store = LMDBBuilder::new()
            .set_path(env::temp_dir())
            .set_env_config(LMDBConfig::default())
            .set_max_number_of_databases(10)
            .add_database("db1", db::CREATE)
            .add_database("db2", db::CREATE)
            .build()
            .unwrap();
        assert_eq!(store.databases.len(), 2);
    }
}
