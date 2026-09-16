# Tari Storage

This crate is part of the [Tari Cryptocurrency](https://tari.com) project.

An abstraction layer for persistent key-value storage. The Tari domain layer classes should only make use of these
traits and objects and let the underlying implementations handle the details.

##  DataStore

Provides a general CRUD behaviour of Key-Value Store implementations. `Datastore` is agnostic of the underlying
implementation.

## LMDB

Currently, Tari supports LMDB for local disk persistence.

Use `LMDBBuilder` to open/create a new database.

```rust,ignore
# use tari_storage::lmdb::LMDBBuilder;
let mut store = LMDBBuilder::new()
    .set_path("/tmp/")
    .set_mapsize(500)
    .add_database("db1")
    .add_database("db2")
    .build()
    .unwrap();
```