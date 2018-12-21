//! An abstraction layer for persistent key-value storage. The Tari domain layer classes should only make use of
//! these traits and objects and let the underlying implementations handle the details.

use bincode::{deserialize, serialize, ErrorKind};
use derive_error::Error;
use serde::{de::DeserializeOwned, Serialize};
use std::error::Error;

#[derive(Debug, Error)]
pub enum DatastoreError {
    /// An error occurred with the underlying data store implementation
    #[error(embedded_msg, no_from, non_std)]
    InternalError(String),
    /// An error occurred during serialization
    #[error(no_from, non_std)]
    SerializationErr(String),
    /// An error occurred during deserialization
    #[error(no_from, non_std)]
    DeserializationErr(String),
    /// Occurs when trying to perform an action that requires us to be in a live transaction
    TransactionNotLiveError,
    /// A transaction or query was attempted while no database was open.
    DatabaseNotOpen,
    /// A database with the requested name does not exist
    UnknownDatabase,
    /// An error occurred during a put query
    #[error(embedded_msg, no_from, non_std)]
    PutError(String),
    /// An error occurred during a get query
    #[error(embedded_msg, no_from, non_std)]
    GetError(String),
}

impl From<bincode::Error> for DatastoreError {
    fn from(e: Box<ErrorKind>) -> Self {
        let msg = format!("Datastore conversion error: {}", e.description());
        DatastoreError::DeserializationErr(msg)
    }
}

/// General CRUD behaviour of KVStore implementations. Datastore is agnostic of the underlying implementation, but
/// does assume that key-value pairs are stored using byte arrays (`&[u8]`). You can use `get_raw` or `put_raw` to
/// read and write binary data directly, or use `#[derive(Serialize, Deserialize, PartialEq, Debug)]` on a trait to
/// generate code for automatically de/serializing your data structures to byte strings using `bincode`.
pub trait DataStore {
    /// Connect to the logical database with `name`. If the Datastore does not support multiple logical databases,
    /// this function has no effect
    fn connect(&mut self, name: &str) -> Result<(), DatastoreError>;

    /// Get the raw value at the given key, or None if the key does not exist
    fn get_raw(&self, key: &[u8]) -> Result<Option<Vec<u8>>, DatastoreError>;

    /// Retrieve a value from the store, deserialize it using bincode and return the value or None if the key does not
    /// exist
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, DatastoreError> {
        let key = key.as_bytes();
        let result = self.get_raw(key)?;
        match result {
            None => Ok(None),
            Some(val) => Ok(Some(deserialize(&val[..])?)),
        }
    }

    /// Check whether the given key exists in the database
    fn exists(&self, key: &[u8]) -> Result<bool, DatastoreError>;

    /// Save a value at the given key. Existing values are overwritten
    fn put_raw(&mut self, key: &[u8], value: Vec<u8>) -> Result<(), DatastoreError>;

    /// Serialize a value using Bincode and then save it value at the given key. Existing values are overwritten
    fn put<T: Serialize>(&mut self, key: &str, value: &T) -> Result<(), DatastoreError> {
        let key = key.as_bytes();
        let val = serialize(value)?;
        self.put_raw(key, val)
    }
}

/// BatchWrite is implemented on Datastores if it supports batch writes, or transactions, to efficiently write
/// multiple puts to the Datastore.
pub trait BatchWrite {
    type Store: DataStore;
    type Batcher: BatchWrite + Sized;

    fn new(store: &Self::Store) -> Result<Self::Batcher, DatastoreError>;

    /// Save a value at the given key. Existing values are overwritten
    fn put_raw(&mut self, key: &[u8], value: Vec<u8>) -> Result<(), DatastoreError>;

    /// Serialize a value and then save it value at the given key. Existing values are overwritten
    fn put<T: Serialize>(&mut self, key: &str, value: &T) -> Result<(), DatastoreError> {
        let key = key.as_bytes();
        let val = serialize(value)?;
        self.put_raw(key, val)
    }

    /// Commit all puts in the batch write to the database
    fn commit(self) -> Result<(), DatastoreError>;

    /// Discard all puts that have been made in this batch
    fn abort(self) -> Result<(), DatastoreError>;
}
