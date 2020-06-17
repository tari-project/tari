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
    schema::{peers, wallet_settings},
    storage::database::{DbKey, DbKeyValuePair, DbValue, WalletBackend, WriteOperation},
};
use diesel::{prelude::*, result::Error as DieselError, SqliteConnection};

use std::{
    convert::TryFrom,
    sync::{Arc, Mutex},
};
use tari_comms::{peer_manager::Peer, types::CommsSecretKey};
use tari_crypto::tari_utilities::{hex::Hex, ByteArray};

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct WalletSqliteDatabase {
    database_connection: Arc<Mutex<SqliteConnection>>,
}
impl WalletSqliteDatabase {
    pub fn new(database_connection: Arc<Mutex<SqliteConnection>>) -> Self {
        Self { database_connection }
    }
}

impl WalletBackend for WalletSqliteDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError> {
        let conn = acquire_lock!(self.database_connection);

        let result = match key {
            DbKey::Peer(pk) => match PeerSql::find(&pk.to_vec(), &(*conn)) {
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
            DbKey::CommsSecretKey => {
                if let Some(key_str) = WalletSettingSql::get(format!("{}", key), &conn)? {
                    Some(DbValue::CommsSecretKey(CommsSecretKey::from_hex(key_str.as_str())?))
                } else {
                    None
                }
            },
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError> {
        let conn = acquire_lock!(self.database_connection);

        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::Peer(k, p) => {
                    if PeerSql::find(&k.to_vec(), &(*conn)).is_ok() {
                        return Err(WalletStorageError::DuplicateContact);
                    }
                    PeerSql::try_from(p)?.commit(&conn)?;
                },
                DbKeyValuePair::CommsSecretKey(sk) => {
                    WalletSettingSql::new(format!("{}", DbKey::CommsSecretKey), sk.to_hex()).set(&conn)?
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::Peer(k) => match PeerSql::find(&k.to_vec(), &(*conn)) {
                    Ok(p) => {
                        p.delete(&conn)?;
                        return Ok(Some(DbValue::Peer(Box::new(Peer::try_from(p)?))));
                    },
                    Err(WalletStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                },
                DbKey::Peers => return Err(WalletStorageError::OperationNotSupported),
                DbKey::CommsSecretKey => return Err(WalletStorageError::OperationNotSupported),
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
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::insert_into(peers::table).values(self.clone()).execute(conn)?;
        Ok(())
    }

    /// Return all peers
    pub fn index(conn: &SqliteConnection) -> Result<Vec<PeerSql>, WalletStorageError> {
        Ok(peers::table.load::<PeerSql>(conn)?)
    }

    /// Find a particular Peer, if it exists
    pub fn find(public_key: &[u8], conn: &SqliteConnection) -> Result<PeerSql, WalletStorageError> {
        Ok(peers::table
            .filter(peers::public_key.eq(public_key))
            .first::<PeerSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
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

/// A Sql version of the wallet setting key-value table
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "wallet_settings"]
struct WalletSettingSql {
    key: String,
    value: String,
}

impl WalletSettingSql {
    pub fn new(key: String, value: String) -> Self {
        Self { key, value }
    }

    pub fn set(&self, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::replace_into(wallet_settings::table)
            .values(self)
            .execute(conn)?;

        Ok(())
    }

    pub fn get(key: String, conn: &SqliteConnection) -> Result<Option<String>, WalletStorageError> {
        wallet_settings::table
            .filter(wallet_settings::key.eq(key))
            .first::<WalletSettingSql>(conn)
            .map(|v: WalletSettingSql| Some(v.value))
            .or_else(|err| match err {
                diesel::result::Error::NotFound => Ok(None),
                err => Err(err.into()),
            })
    }
}
