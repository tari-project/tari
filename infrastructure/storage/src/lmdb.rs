//! An implementation of [KVStore](trait.KVStore.html) using [LMDB](http://www.lmdb.tech)

use crate::keyvalue_store::{BatchWrite, DataStore, DatastoreError};
use lmdb_zero as lmdb;
use lmdb_zero::error::LmdbResultExt;
use std::{collections::HashMap, sync::Arc};

/// A builder for [LMDBStore](struct.lmdbstore.html)
/// ## Example
///
/// Create a new LMDB database of 500MB in the `db` directory with two named databases: "db1" and "db2"
///
/// ```
/// # use tari_storage::lmdb::LMDBBuilder;
/// let mut store = LMDBBuilder::new()
///     .set_path("/tmp/")
///     .set_mapsize(500)
///     .add_database("db1")
///     .add_database("db2")
///     .build()
///     .unwrap();
/// ```
pub struct LMDBBuilder {
    path: String,
    db_size_mb: usize,
    db_names: Vec<String>,
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
            db_names: Vec::new(),
        }
    }

    /// Set the directory where the LMDB database exists, or must be created.
    /// Note: The directory must exist already; it is not created for you. If it does not exist, `build()` will
    /// return `DataStoreError::InternalError`.
    /// the `path` must have a trailing slash
    pub fn set_path(mut self, path: &str) -> LMDBBuilder {
        self.path = path.into();
        self
    }

    /// Sets the size of the database, in MB.
    /// The actual memory will only be allocated when #build() is called
    pub fn set_mapsize(mut self, size: usize) -> LMDBBuilder {
        self.db_size_mb = size;
        self
    }

    /// Add an additional named database to the LMDB environment.If `add_database` isn't called at least once, only the
    /// `default` database is created.
    pub fn add_database(mut self, name: &str) -> LMDBBuilder {
        // There will always be a 'default' database
        if name != "default" {
            self.db_names.push(name.into());
        }
        self
    }

    /// Create a new LMDBStore instance and open the underlying database environment
    pub fn build(self) -> Result<LMDBStore, DatastoreError> {
        let env = unsafe {
            let mut builder = lmdb::EnvBuilder::new()?;
            builder.set_mapsize(self.db_size_mb * 1024 * 1024)?;
            builder.set_maxdbs(self.db_names.len() as u32 + 1)?;
            builder.open(&self.path, lmdb::open::Flags::empty(), 0o600)?
        };
        let env = Arc::new(env);
        let mut databases: HashMap<String, Arc<lmdb::Database<'static>>> = HashMap::new();
        let opt = lmdb::DatabaseOptions::new(lmdb::db::CREATE);
        // Add the default db
        let default = Arc::new(lmdb::Database::open(env.clone(), None, &opt)?);
        let curr_db = default.clone();
        databases.insert("default".to_string(), default);
        for name in &self.db_names {
            let db = Arc::new(lmdb::Database::open(env.clone(), Some(name), &opt)?);
            databases.insert(name.to_string(), db);
        }
        Ok(LMDBStore {
            env,
            databases,
            curr_db,
        })
    }
}

/// A Struct for holding state for the LMDB implementation of DataStore and BatchWrite. To create an instance of
/// LMDBStore, use [LMDBBuilder](struct.lmdbbuilder.html).
pub struct LMDBStore {
    pub(crate) env: Arc<lmdb::Environment>,
    pub(crate) databases: HashMap<String, Arc<lmdb::Database<'static>>>,
    pub(crate) curr_db: Arc<lmdb::Database<'static>>,
}

/// Consume self to remove all databases from scope
impl LMDBStore {
    fn delete_db_from_scope(self) -> Result<(), lmdb_zero::error::Error> {
        self.curr_db.env().sync(true)
    }
}

impl DataStore for LMDBStore {
    fn connect(&mut self, name: &str) -> Result<(), DatastoreError> {
        match self.databases.get(name) {
            Some(db) => {
                self.curr_db = db.clone();
                Ok(())
            },
            None => Err(DatastoreError::UnknownDatabase),
        }
    }

    fn get_raw(&self, key: &[u8]) -> Result<Option<Vec<u8>>, DatastoreError> {
        let txn = lmdb::ReadTransaction::new(self.env.clone())?;
        let accessor = txn.access();
        match accessor.get::<[u8], [u8]>(&self.curr_db, key).to_opt() {
            Ok(None) => Ok(None),
            Ok(Some(v)) => Ok(Some(v.to_vec())),
            Err(e) => Err(DatastoreError::GetError(format!("LMDB get error: {}", e.to_string()))),
        }
    }

    fn exists(&self, key: &[u8]) -> Result<bool, DatastoreError> {
        let txn = lmdb::ReadTransaction::new(self.env.clone())?;
        let accessor = txn.access();
        let res: lmdb::error::Result<&lmdb::Ignore> = accessor.get(&self.curr_db, key);
        Ok(res.to_opt()?.is_some())
    }

    fn put_raw(&mut self, key: &[u8], value: Vec<u8>) -> Result<(), DatastoreError> {
        let tx = lmdb::WriteTransaction::new(self.env.clone())?;
        {
            let mut accessor = tx.access();
            accessor.put(&self.curr_db, key, &value, lmdb::put::Flags::empty())?;
        }
        tx.commit().map_err(|e| e.into())
    }
}

struct LMDBBatch<'a> {
    db: Arc<lmdb::Database<'static>>,
    tx: lmdb::WriteTransaction<'a>,
}

impl<'a> BatchWrite for LMDBBatch<'a> {
    type Batcher = LMDBBatch<'a>;
    type Store = LMDBStore;

    fn new(store: &LMDBStore) -> Result<LMDBBatch<'a>, DatastoreError> {
        Ok(LMDBBatch {
            db: store.curr_db.clone(),
            tx: lmdb::WriteTransaction::new(store.env.clone())?,
        })
    }

    fn put_raw(&mut self, key: &[u8], value: Vec<u8>) -> Result<(), DatastoreError> {
        {
            let mut accessor = self.tx.access();
            accessor.put(&self.db, key, &value, lmdb::put::Flags::empty())?;
        }
        Ok(())
    }

    fn commit(self) -> Result<(), DatastoreError> {
        self.tx.commit().map_err(|e| e.into())
    }

    fn abort(self) -> Result<(), DatastoreError> {
        Ok(())
    }
}

impl From<lmdb::error::Error> for DatastoreError {
    fn from(err: lmdb::error::Error) -> Self {
        let err_msg = format!("LMDB Error: {}", err.to_string());
        DatastoreError::InternalError(err_msg)
    }
}

#[cfg(test)]
mod test {
    use super::{LMDBBuilder, LMDBStore};
    use crate::{
        keyvalue_store::{BatchWrite, DataStore, DatastoreError},
        lmdb::LMDBBatch,
    };
    use bincode::{deserialize, serialize};
    use rand::{OsRng, RngCore};
    use serde_derive::{Deserialize, Serialize};
    use std::{fs, str};
    extern crate sys_info;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Entity {
        x: f32,
        y: f32,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct World(Vec<Entity>);

    fn to_bytes(i: u32) -> Vec<u8> {
        i.to_le_bytes().to_vec()
    }

    fn from_bytes(v: &[u8]) -> u32 {
        u32::from_le_bytes([v[0], v[1], v[2], v[3]])
    }

    fn make_vector(len: usize) -> Vec<u8> {
        let mut vec = vec![0; len];
        let mut rng = OsRng::new().unwrap();
        rng.fill_bytes(&mut vec);
        vec
    }

    #[test]
    fn path_must_exist() {
        let test_dir = "./tests/not_here/";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        let msg = match sys_info::os_type() {
            Ok(ref msg) if msg.to_uppercase() == "WINDOWS" => "LMDB Error: The system cannot find the path specified.\r\n",
            Ok(ref msg) if msg.to_uppercase() == "LINUX" || msg == "DARWIN" => "LMDB Error: No such file or directory",
            _ => ":(",
        };
        let builder = LMDBBuilder::new();
        match builder.set_mapsize(1).set_path(test_dir).build() {
            Err(DatastoreError::InternalError(s)) => assert_eq!(s, msg),
            _ => panic!(),
        }
    }

    #[test]
    fn batch_writes() {
        let test_dir = "./tests/test_tx/";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        assert!(fs::create_dir(test_dir).is_ok());
        let builder = LMDBBuilder::new();
        let store = builder.set_mapsize(5).set_path(test_dir).build().unwrap();
        let mut batch = LMDBBatch::new(&store).unwrap();
        batch.put_raw(b"a", b"apple".to_vec()).unwrap();
        batch.put_raw(b"b", b"banana".to_vec()).unwrap();
        batch.put_raw(b"c", b"carrot".to_vec()).unwrap();
        batch.commit().unwrap();
        let banana = store.get_raw(b"b").unwrap().unwrap();
        assert_eq!(&banana, b"banana");
        // Clean up
        assert!(store.delete_db_from_scope().is_ok());
        let _no_val = fs::remove_dir_all(test_dir);
        if std::fs::metadata(test_dir).is_ok() {
            println!("Database file handles not released, still open in {:?}!", test_dir);
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
    }

    #[test]
    fn writes_to_default_db() {
        let test_dir = "./tests/test_default";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        assert!(fs::create_dir(test_dir).is_ok());

        let mut store = LMDBBuilder::new().set_path(test_dir).build().unwrap();
        store.connect("default").unwrap();
        // Write some values
        store.put_raw(b"England", b"rose".to_vec()).unwrap();
        store.put_raw(b"SouthAfrica", b"protea".to_vec()).unwrap();
        store.put_raw(b"Scotland", b"thistle".to_vec()).unwrap();
        // And read them back
        let val = store.get_raw(b"Scotland").unwrap().unwrap();
        assert_eq!(str::from_utf8(&val).unwrap(), "thistle");
        let val = store.get_raw(b"England").unwrap().unwrap();
        assert_eq!(str::from_utf8(&val).unwrap(), "rose");
        // Clean up
        assert!(store.delete_db_from_scope().is_ok());
        let _no_val = fs::remove_dir_all(test_dir);
        if std::fs::metadata(test_dir).is_ok() {
            println!("Database file handles not released, still open in {:?}!", test_dir);
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
    }

    #[test]
    fn aborts_write() {
        let test_dir = "./tests/test_abort";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        assert!(fs::create_dir(test_dir).is_ok());
        let mut store = LMDBBuilder::new().set_path(test_dir).build().unwrap();
        store.connect("default").unwrap();
        // Write some values
        let mut batch = LMDBBatch::new(&store).unwrap();
        batch.put_raw(b"England", b"rose".to_vec()).unwrap();
        batch.put_raw(b"SouthAfrica", b"protea".to_vec()).unwrap();
        batch.put_raw(b"Scotland", b"thistle".to_vec()).unwrap();
        batch.abort().unwrap();
        // And check nothing was written
        let check = |k: &[u8], store: &LMDBStore| {
            let val = store.get_raw(k).unwrap();
            assert!(val.is_none());
        };
        check(b"Scotland", &store);
        check(b"SouthAfrica", &store);
        check(b"England", &store);
        // Clean up
        assert!(store.delete_db_from_scope().is_ok());
        let _no_val = fs::remove_dir_all(test_dir);
        if std::fs::metadata(test_dir).is_ok() {
            println!("Database file handles not released, still open in {:?}!", test_dir);
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
    }

    /// Set the DB size to 1MB and write more than a MB to it
    #[test]
    fn overflow_db() {
        let test_dir = "./tests/test_overflow";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        assert!(fs::create_dir(test_dir).is_ok());
        let builder = LMDBBuilder::new();
        let mut store = builder
            .set_path(test_dir)
            // Set the max DB size to 1MB
            .set_mapsize(1)
            .build()
            .unwrap();
        assert!(store.connect("default").is_ok());
        // Write 500,000 bytes
        store.put_raw(b"key", make_vector(500_000)).unwrap();
        // Try write another 600,000 bytes and watch it fail
        match store.put_raw(b"key2", make_vector(600_000)).unwrap_err() {
            DatastoreError::InternalError(s) => {
                assert_eq!(s, "LMDB Error: MDB_MAP_FULL: Environment mapsize limit reached");
            },
            err => {
                println!("{:?}", err);
                assert!(fs::remove_dir_all(test_dir).is_ok());
                panic!()
            },
        }
        // Clean up
        assert!(store.delete_db_from_scope().is_ok());
        let _no_val = fs::remove_dir_all(test_dir);
        if std::fs::metadata(test_dir).is_ok() {
            println!("Database file handles not released, still open in {:?}!", test_dir);
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
    }

    #[test]
    fn read_and_write_10k_values() {
        let test_dir = "./tests/test_10k";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        assert!(fs::create_dir(test_dir).is_ok());
        let builder = LMDBBuilder::new();
        let mut store = builder.set_path(test_dir).add_database("test").build().unwrap();
        assert!(store.connect("test").is_ok());
        // Write 100,000 integers to the DB with val = 2*key
        let mut batch = LMDBBatch::new(&store).unwrap();
        for i in 0u32..10_000 {
            batch.put_raw(&to_bytes(i), to_bytes(2 * i)).unwrap();
        }
        batch.commit().unwrap();
        // And read them back
        for i in 0u32..10_000 {
            let val = store.get_raw(&to_bytes(i)).unwrap().unwrap();
            assert_eq!(from_bytes(&val), i * 2);
        }
        // Clean up
        assert!(store.delete_db_from_scope().is_ok());
        let _no_val = fs::remove_dir_all(test_dir);
        if std::fs::metadata(test_dir).is_ok() {
            println!("Database file handles not released, still open in {:?}!", test_dir);
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
    }

    #[test]
    fn test_exist_on_different_databases() {
        let test_dir = "./tests/test_exist";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        assert!(fs::create_dir(test_dir).is_ok());
        let mut store = LMDBBuilder::new()
            .set_path(test_dir)
            .add_database("db1")
            .add_database("db2")
            .build()
            .unwrap();
        store.connect("db1").unwrap();
        // Write some values
        store.put_raw(b"db1-a", b"val1".to_vec()).unwrap();
        store.put_raw(b"db1-b", b"val2".to_vec()).unwrap();
        store.put_raw(b"common", b"db1".to_vec()).unwrap();
        // Change databases
        store.connect("db2").unwrap();
        store.put_raw(b"db2-a", b"val3".to_vec()).unwrap();
        store.put_raw(b"db2-b", b"val4".to_vec()).unwrap();
        store.put_raw(b"common", b"db2".to_vec()).unwrap();
        // Check existence and non-existence of keys
        assert!(!store.exists(b"db1-a").unwrap());
        assert!(!store.exists(b"db1-b").unwrap());
        assert!(store.exists(b"db2-a").unwrap());
        assert!(store.exists(b"db2-b").unwrap());
        assert!(store.exists(b"common").unwrap());
        // Change back to db1
        store.connect("db1").unwrap();
        // Check existence and non-existence of keys
        assert!(store.exists(b"db1-a").unwrap());
        assert!(store.exists(b"db1-b").unwrap());
        assert!(!store.exists(b"db2-a").unwrap());
        assert!(!store.exists(b"db2-b").unwrap());
        assert!(store.exists(b"common").unwrap());
        // Finally check the value of 'common'
        let val = store.get_raw(b"common").unwrap().unwrap();
        assert_eq!(&val, b"db1");
        // Clean up
        assert!(store.delete_db_from_scope().is_ok());
        let _no_val = fs::remove_dir_all(test_dir);
        if std::fs::metadata(test_dir).is_ok() {
            println!("Database file handles not released, still open in {:?}!", test_dir);
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
    }

    #[test]
    fn write_structs() {
        let test_dir = "./tests/test_struct";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        assert!(fs::create_dir(test_dir).is_ok());
        let builder = LMDBBuilder::new();
        let mut store = builder.set_path(test_dir).build().unwrap();
        let world = World(vec![Entity { x: 0.0, y: 4.0 }, Entity { x: 10.0, y: 20.5 }]);
        let encoded: Vec<u8> = serialize(&world).unwrap();
        // 8 bytes for the length of the vector, 4 bytes per float.
        assert_eq!(encoded.len(), 8 + 4 * 4);
        store.put_raw(b"world", encoded).unwrap();
        // Write using `put`
        let world_2 = World(vec![Entity { x: 100.0, y: -123.45 }, Entity { x: 42.0, y: -42.0 }]);
        store.put("brave new world", &world_2).unwrap();
        // Get world back using get_raw
        let val = store.get_raw(b"world").unwrap().unwrap();
        let decoded: World = deserialize(&val[..]).unwrap();
        assert_eq!(world, decoded);
        // Get world2 back using get_raw
        let val = store.get_raw(b"brave new world").unwrap().unwrap();
        let decoded: World = deserialize(&val[..]).unwrap();
        assert_eq!(world_2, decoded);
        // Get world_2 back using get
        let val = store.get("brave new world").unwrap().unwrap();
        assert_eq!(world_2, val);
        // And check that get returns None
        let val = store.get::<World>("not here").unwrap();
        assert!(val.is_none());
        // Clean up
        assert!(store.delete_db_from_scope().is_ok());
        let _no_val = fs::remove_dir_all(test_dir);
        if std::fs::metadata(test_dir).is_ok() {
            println!("Database file handles not released, still open in {:?}!", test_dir);
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
    }
}
