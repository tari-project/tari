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

use crate::{
    error::WalletStorageError,
    schema::peers,
    storage::database::{DbKey, DbKeyValuePair, DbValue, WalletBackend, WriteOperation},
};
use diesel::{
    prelude::*,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    result::Error as DieselError,
    SqliteConnection,
};
use std::{convert::TryFrom, io, path::Path, time::Duration};
use tari_comms::peer_manager::Peer;
use tari_utilities::ByteArray;

const DATABASE_CONNECTION_TIMEOUT_MS: u64 = 2000;

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
pub struct WalletSqliteDatabase {
    database_connection_pool: Pool<ConnectionManager<SqliteConnection>>,
}
impl WalletSqliteDatabase {
    pub fn new(database_path: String) -> Result<Self, WalletStorageError> {
        let db_exists = Path::new(&database_path).exists();

        let connection = SqliteConnection::establish(&database_path)?;

        connection.execute("PRAGMA foreign_keys = ON")?;
        if !db_exists {
            embed_migrations!("./migrations");
            embedded_migrations::run_with_output(&connection, &mut io::stdout()).map_err(|err| {
                WalletStorageError::DatabaseMigrationError(format!("Database migration failed {}", err))
            })?;
        }
        drop(connection);

        let manager = ConnectionManager::<SqliteConnection>::new(database_path);
        let pool = diesel::r2d2::Pool::builder()
            .connection_timeout(Duration::from_millis(DATABASE_CONNECTION_TIMEOUT_MS))
            .idle_timeout(Some(Duration::from_millis(DATABASE_CONNECTION_TIMEOUT_MS)))
            .build(manager)
            .map_err(|_| WalletStorageError::R2d2Error)?;

        Ok(Self {
            database_connection_pool: pool,
        })
    }
}

impl WalletBackend for WalletSqliteDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| WalletStorageError::R2d2Error)?;

        let result = match key {
            DbKey::Peer(pk) => match PeerSql::find(&pk.to_vec(), &conn) {
                Ok(c) => Some(DbValue::Peer(Box::new(Peer::try_from(c)?))),
                Err(WalletStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::Peers => Some(DbValue::Peers(
                PeerSql::index(&conn)?
                    .iter()
                    .map(|c| Peer::try_from(c.clone()))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        };

        Ok(result)
    }

    fn write(&mut self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| WalletStorageError::R2d2Error)?;

        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::Peer(k, p) => {
                    if let Ok(_) = PeerSql::find(&k.to_vec(), &conn) {
                        return Err(WalletStorageError::DuplicateContact);
                    }

                    PeerSql::try_from(p)?.commit(&conn)?;
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::Peer(k) => match PeerSql::find(&k.to_vec(), &conn) {
                    Ok(p) => {
                        p.clone().delete(&conn)?;
                        return Ok(Some(DbValue::Peer(Box::new(Peer::try_from(p)?))));
                    },
                    Err(WalletStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                },
                DbKey::Peers => return Err(WalletStorageError::OperationNotSupported),
            },
        }

        Ok(None)
    }
}

/// A Sql version of the Peer struct
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "peers"]
struct PeerSql {
    public_key: Vec<u8>,
    peer: String,
}

impl PeerSql {
    /// Write this struct to the database
    pub fn commit(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), WalletStorageError>
    {
        diesel::insert_into(peers::table).values(self.clone()).execute(conn)?;
        Ok(())
    }

    /// Return all peers
    pub fn index(
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<PeerSql>, WalletStorageError> {
        Ok(peers::table.load::<PeerSql>(conn)?)
    }

    /// Find a particular Peer, if it exists
    pub fn find(
        public_key: &Vec<u8>,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<PeerSql, WalletStorageError>
    {
        Ok(peers::table
            .filter(peers::public_key.eq(public_key))
            .first::<PeerSql>(conn)?)
    }

    pub fn delete(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), WalletStorageError>
    {
        let num_deleted = diesel::delete(peers::table.filter(peers::public_key.eq(&self.public_key))).execute(conn)?;

        if num_deleted == 0 {
            return Err(WalletStorageError::ValuesNotFound);
        }

        Ok(())
    }
}

/// Conversion from an Contact to the Sql datatype form
impl TryFrom<PeerSql> for Peer {
    type Error = WalletStorageError;

    fn try_from(p: PeerSql) -> Result<Self, Self::Error> {
        Ok(serde_json::from_str(&p.peer)?)
    }
}

/// Conversion from an Contact to the Sql datatype form
impl TryFrom<Peer> for PeerSql {
    type Error = WalletStorageError;

    fn try_from(p: Peer) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: p.public_key.to_vec(),
            peer: serde_json::to_string(&p)?,
        })
    }
}
