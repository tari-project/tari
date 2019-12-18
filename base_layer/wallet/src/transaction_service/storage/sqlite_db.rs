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
    output_manager_service::TxId,
    schema::{completed_transactions, inbound_transactions, outbound_transactions},
    transaction_service::{
        error::TransactionStorageError,
        storage::database::{
            CompletedTransaction,
            DbKey,
            DbKeyValuePair,
            DbValue,
            InboundTransaction,
            OutboundTransaction,
            TransactionBackend,
            TransactionStatus,
            WriteOperation,
        },
    },
};
use chrono::NaiveDateTime;
use diesel::{
    prelude::*,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    result::Error as DieselError,
    SqliteConnection,
};
use std::{collections::HashMap, convert::TryFrom, io, path::Path, time::Duration};
use tari_transactions::{tari_amount::MicroTari, types::PublicKey};
use tari_utilities::ByteArray;

const DATABASE_CONNECTION_TIMEOUT_MS: u64 = 2000;

/// A Sqlite backend for the Transaction Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct TransactionServiceSqliteDatabase {
    database_connection_pool: Pool<ConnectionManager<SqliteConnection>>,
}
impl TransactionServiceSqliteDatabase {
    pub fn new(database_path: String) -> Result<Self, TransactionStorageError> {
        let db_exists = Path::new(&database_path).exists();

        let connection = SqliteConnection::establish(&database_path)?;

        connection.execute("PRAGMA foreign_keys = ON")?;
        if !db_exists {
            embed_migrations!("./migrations");
            embedded_migrations::run_with_output(&connection, &mut io::stdout()).map_err(|err| {
                TransactionStorageError::DatabaseMigrationError(format!("Database migration failed {}", err))
            })?;
        }
        drop(connection);

        let manager = ConnectionManager::<SqliteConnection>::new(database_path);
        let pool = diesel::r2d2::Pool::builder()
            .connection_timeout(Duration::from_millis(DATABASE_CONNECTION_TIMEOUT_MS))
            .idle_timeout(Some(Duration::from_millis(DATABASE_CONNECTION_TIMEOUT_MS)))
            .build(manager)
            .map_err(|_| TransactionStorageError::R2d2Error)?;

        Ok(Self {
            database_connection_pool: pool,
        })
    }
}

impl TransactionBackend for TransactionServiceSqliteDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, TransactionStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TransactionStorageError::R2d2Error)?;

        let result = match key {
            DbKey::PendingOutboundTransaction(t) => match OutboundTransactionSql::find(t, &conn) {
                Ok(o) => Some(DbValue::PendingOutboundTransaction(Box::new(
                    OutboundTransaction::try_from(o)?,
                ))),
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::PendingInboundTransaction(t) => match InboundTransactionSql::find(t, &conn) {
                Ok(o) => Some(DbValue::PendingInboundTransaction(Box::new(
                    InboundTransaction::try_from(o)?,
                ))),
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::CompletedTransaction(t) => match CompletedTransactionSql::find(t, &conn) {
                Ok(o) => Some(DbValue::CompletedTransaction(Box::new(CompletedTransaction::try_from(
                    o,
                )?))),
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::PendingOutboundTransactions => Some(DbValue::PendingOutboundTransactions(
                OutboundTransactionSql::index(&conn)?
                    .iter()
                    .fold(HashMap::new(), |mut acc, x| {
                        if let Ok(v) = OutboundTransaction::try_from((*x).clone()) {
                            acc.insert(x.tx_id as u64, v);
                        }
                        acc
                    }),
            )),
            DbKey::PendingInboundTransactions => Some(DbValue::PendingInboundTransactions(
                InboundTransactionSql::index(&conn)?
                    .iter()
                    .fold(HashMap::new(), |mut acc, x| {
                        if let Ok(v) = InboundTransaction::try_from((*x).clone()) {
                            acc.insert(x.tx_id as u64, v);
                        }
                        acc
                    }),
            )),
            DbKey::CompletedTransactions => Some(DbValue::CompletedTransactions(
                CompletedTransactionSql::index(&conn)?
                    .iter()
                    .fold(HashMap::new(), |mut acc, x| {
                        if let Ok(v) = CompletedTransaction::try_from((*x).clone()) {
                            acc.insert(x.tx_id as u64, v);
                        }
                        acc
                    }),
            )),
        };

        Ok(result)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, TransactionStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TransactionStorageError::R2d2Error)?;

        let result = match key {
            DbKey::PendingOutboundTransaction(k) => OutboundTransactionSql::find(k, &conn).is_ok(),
            DbKey::PendingInboundTransaction(k) => InboundTransactionSql::find(k, &conn).is_ok(),
            DbKey::CompletedTransaction(k) => CompletedTransactionSql::find(k, &conn).is_ok(),
            DbKey::PendingOutboundTransactions => false,
            DbKey::PendingInboundTransactions => false,
            DbKey::CompletedTransactions => false,
        };

        Ok(result)
    }

    fn write(&mut self, op: WriteOperation) -> Result<Option<DbValue>, TransactionStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TransactionStorageError::R2d2Error)?;

        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::PendingOutboundTransaction(k, v) => {
                    if let Ok(_) = OutboundTransactionSql::find(&k, &conn) {
                        return Err(TransactionStorageError::DuplicateOutput);
                    }
                    OutboundTransactionSql::try_from(*v)?.commit(&conn)?;
                },
                DbKeyValuePair::PendingInboundTransaction(k, v) => {
                    if let Ok(_) = InboundTransactionSql::find(&k, &conn) {
                        return Err(TransactionStorageError::DuplicateOutput);
                    }
                    InboundTransactionSql::try_from(*v)?.commit(&conn)?;
                },
                DbKeyValuePair::CompletedTransaction(k, v) => {
                    if let Ok(_) = CompletedTransactionSql::find(&k, &conn) {
                        return Err(TransactionStorageError::DuplicateOutput);
                    }
                    CompletedTransactionSql::try_from(*v)?.commit(&conn)?;
                },
            },
            WriteOperation::Remove(kvp) => match kvp {
                DbKey::PendingOutboundTransaction(k) => match OutboundTransactionSql::find(&k, &conn) {
                    Ok(v) => {
                        v.delete(&conn)?;
                        return Ok(Some(DbValue::PendingOutboundTransaction(Box::new(
                            OutboundTransaction::try_from(v)?,
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                        return Err(TransactionStorageError::ValueNotFound(
                            DbKey::PendingOutboundTransaction(k),
                        ))
                    },
                    Err(e) => return Err(e),
                },
                DbKey::PendingInboundTransaction(k) => match InboundTransactionSql::find(&k, &conn) {
                    Ok(v) => {
                        v.delete(&conn)?;
                        return Ok(Some(DbValue::PendingInboundTransaction(Box::new(
                            InboundTransaction::try_from(v)?,
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                        return Err(TransactionStorageError::ValueNotFound(
                            DbKey::PendingOutboundTransaction(k),
                        ))
                    },
                    Err(e) => return Err(e),
                },
                DbKey::CompletedTransaction(k) => match CompletedTransactionSql::find(&k, &conn) {
                    Ok(v) => {
                        v.delete(&conn)?;
                        return Ok(Some(DbValue::CompletedTransaction(Box::new(
                            CompletedTransaction::try_from(v)?,
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                        return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(k)))
                    },
                    Err(e) => return Err(e),
                },
                DbKey::PendingOutboundTransactions => return Err(TransactionStorageError::OperationNotSupported),
                DbKey::PendingInboundTransactions => return Err(TransactionStorageError::OperationNotSupported),
                DbKey::CompletedTransactions => return Err(TransactionStorageError::OperationNotSupported),
            },
        }
        Ok(None)
    }

    fn complete_outbound_transaction(
        &mut self,
        tx_id: u64,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TransactionStorageError::R2d2Error)?;

        if CompletedTransactionSql::find(&tx_id, &conn).is_ok() {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }

        match OutboundTransactionSql::find(&tx_id, &conn) {
            Ok(v) => {
                let completed_tx_sql = CompletedTransactionSql::try_from(completed_transaction)?;
                v.delete(&conn)?;
                completed_tx_sql.commit(&conn)?;
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(
                    DbKey::PendingOutboundTransaction(tx_id),
                ))
            },
            Err(e) => return Err(e),
        };
        Ok(())
    }

    fn complete_inbound_transaction(
        &mut self,
        tx_id: u64,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TransactionStorageError::R2d2Error)?;

        if CompletedTransactionSql::find(&tx_id, &conn).is_ok() {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }

        match InboundTransactionSql::find(&tx_id, &conn) {
            Ok(v) => {
                let completed_tx_sql = CompletedTransactionSql::try_from(completed_transaction)?;
                v.delete(&conn)?;
                completed_tx_sql.commit(&conn)?;
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(
                    DbKey::PendingInboundTransaction(tx_id),
                ))
            },
            Err(e) => return Err(e),
        };
        Ok(())
    }

    #[cfg(feature = "test_harness")]
    fn broadcast_completed_transaction(&mut self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TransactionStorageError::R2d2Error)?;

        match CompletedTransactionSql::find(&tx_id, &conn) {
            Ok(v) => {
                let _ = v.update(
                    UpdateCompletedTransaction {
                        status: Some(TransactionStatus::Broadcast),
                    },
                    &conn,
                )?;
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(
                    DbKey::PendingInboundTransaction(tx_id),
                ))
            },
            Err(e) => return Err(e),
        };
        Ok(())
    }

    #[cfg(feature = "test_harness")]
    fn mine_completed_transaction(&mut self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| TransactionStorageError::R2d2Error)?;

        match CompletedTransactionSql::find(&tx_id, &conn) {
            Ok(v) => {
                let _ = v.update(
                    UpdateCompletedTransaction {
                        status: Some(TransactionStatus::Mined),
                    },
                    &conn,
                )?;
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(
                    DbKey::PendingInboundTransaction(tx_id),
                ))
            },
            Err(e) => return Err(e),
        };
        Ok(())
    }
}

#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "inbound_transactions"]
struct InboundTransactionSql {
    tx_id: i64,
    source_public_key: Vec<u8>,
    amount: i64,
    receiver_protocol: String,
    message: String,
    timestamp: NaiveDateTime,
}

impl InboundTransactionSql {
    pub fn commit(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), TransactionStorageError>
    {
        diesel::insert_into(inbound_transactions::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn index(
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<InboundTransactionSql>, TransactionStorageError> {
        Ok(inbound_transactions::table.load::<InboundTransactionSql>(conn)?)
    }

    pub fn find(
        tx_id: &TxId,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<InboundTransactionSql, TransactionStorageError>
    {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::tx_id.eq(*tx_id as i64))
            .first::<InboundTransactionSql>(conn)?)
    }

    pub fn delete(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), TransactionStorageError>
    {
        let num_deleted =
            diesel::delete(inbound_transactions::table.filter(inbound_transactions::tx_id.eq(&self.tx_id)))
                .execute(conn)?;

        if num_deleted == 0 {
            return Err(TransactionStorageError::ValuesNotFound);
        }

        Ok(())
    }
}

impl TryFrom<InboundTransaction> for InboundTransactionSql {
    type Error = TransactionStorageError;

    fn try_from(i: InboundTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: i.tx_id as i64,
            source_public_key: i.source_public_key.to_vec(),
            amount: u64::from(i.amount) as i64,
            receiver_protocol: serde_json::to_string(&i.receiver_protocol)?,
            message: i.message,
            timestamp: i.timestamp,
        })
    }
}

impl TryFrom<InboundTransactionSql> for InboundTransaction {
    type Error = TransactionStorageError;

    fn try_from(i: InboundTransactionSql) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: i.tx_id as u64,
            source_public_key: PublicKey::from_vec(&i.source_public_key)
                .map_err(|_| TransactionStorageError::ConversionError)?,
            amount: MicroTari::from(i.amount as u64),
            receiver_protocol: serde_json::from_str(&i.receiver_protocol)?,
            message: i.message,
            timestamp: i.timestamp,
        })
    }
}

/// A structure to represent a Sql compatible version of the OutboundTransaction struct
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "outbound_transactions"]
struct OutboundTransactionSql {
    tx_id: i64,
    destination_public_key: Vec<u8>,
    amount: i64,
    fee: i64,
    sender_protocol: String,
    message: String,
    timestamp: NaiveDateTime,
}

impl OutboundTransactionSql {
    pub fn commit(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), TransactionStorageError>
    {
        diesel::insert_into(outbound_transactions::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn index(
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<OutboundTransactionSql>, TransactionStorageError> {
        Ok(outbound_transactions::table.load::<OutboundTransactionSql>(conn)?)
    }

    pub fn find(
        tx_id: &TxId,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<OutboundTransactionSql, TransactionStorageError>
    {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::tx_id.eq(*tx_id as i64))
            .first::<OutboundTransactionSql>(conn)?)
    }

    pub fn delete(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), TransactionStorageError>
    {
        let num_deleted =
            diesel::delete(outbound_transactions::table.filter(outbound_transactions::tx_id.eq(&self.tx_id)))
                .execute(conn)?;

        if num_deleted == 0 {
            return Err(TransactionStorageError::ValuesNotFound);
        }

        Ok(())
    }
}

impl TryFrom<OutboundTransaction> for OutboundTransactionSql {
    type Error = TransactionStorageError;

    fn try_from(i: OutboundTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: i.tx_id as i64,
            destination_public_key: i.destination_public_key.to_vec(),
            amount: u64::from(i.amount) as i64,
            fee: u64::from(i.fee) as i64,
            sender_protocol: serde_json::to_string(&i.sender_protocol)?,
            message: i.message,
            timestamp: i.timestamp,
        })
    }
}

impl TryFrom<OutboundTransactionSql> for OutboundTransaction {
    type Error = TransactionStorageError;

    fn try_from(i: OutboundTransactionSql) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: i.tx_id as u64,
            destination_public_key: PublicKey::from_vec(&i.destination_public_key)
                .map_err(|_| TransactionStorageError::ConversionError)?,
            amount: MicroTari::from(i.amount as u64),
            fee: MicroTari::from(i.fee as u64),
            sender_protocol: serde_json::from_str(&i.sender_protocol)?,
            message: i.message,
            timestamp: i.timestamp,
        })
    }
}

/// A structure to represent a Sql compatible version of the CompletedTransaction struct
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "completed_transactions"]
struct CompletedTransactionSql {
    tx_id: i64,
    source_public_key: Vec<u8>,
    destination_public_key: Vec<u8>,
    amount: i64,
    fee: i64,
    transaction_protocol: String,
    status: i32,
    message: String,
    timestamp: NaiveDateTime,
}

impl CompletedTransactionSql {
    pub fn commit(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), TransactionStorageError>
    {
        diesel::insert_into(completed_transactions::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn index(
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError> {
        Ok(completed_transactions::table.load::<CompletedTransactionSql>(conn)?)
    }

    pub fn find(
        tx_id: &TxId,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<CompletedTransactionSql, TransactionStorageError>
    {
        Ok(completed_transactions::table
            .filter(completed_transactions::tx_id.eq(*tx_id as i64))
            .first::<CompletedTransactionSql>(conn)?)
    }

    pub fn delete(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), TransactionStorageError>
    {
        let num_deleted =
            diesel::delete(completed_transactions::table.filter(completed_transactions::tx_id.eq(&self.tx_id)))
                .execute(conn)?;

        if num_deleted == 0 {
            return Err(TransactionStorageError::ValuesNotFound);
        }

        Ok(())
    }

    #[cfg(feature = "test_harness")]
    pub fn update(
        &self,
        updated_tx: UpdateCompletedTransaction,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<CompletedTransactionSql, TransactionStorageError>
    {
        let num_updated =
            diesel::update(completed_transactions::table.filter(completed_transactions::tx_id.eq(&self.tx_id)))
                .set(UpdateCompletedTransactionSql::from(updated_tx))
                .execute(conn)?;

        if num_updated == 0 {
            return Err(TransactionStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(CompletedTransactionSql::find(&(self.tx_id as u64), conn)?)
    }
}

impl TryFrom<CompletedTransaction> for CompletedTransactionSql {
    type Error = TransactionStorageError;

    fn try_from(c: CompletedTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: c.tx_id as i64,
            source_public_key: c.source_public_key.to_vec(),
            destination_public_key: c.destination_public_key.to_vec(),
            amount: u64::from(c.amount) as i64,
            fee: u64::from(c.fee) as i64,
            transaction_protocol: serde_json::to_string(&c.transaction)?,
            status: c.status as i32,
            message: c.message,
            timestamp: c.timestamp,
        })
    }
}

impl TryFrom<CompletedTransactionSql> for CompletedTransaction {
    type Error = TransactionStorageError;

    fn try_from(c: CompletedTransactionSql) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: c.tx_id as u64,
            source_public_key: PublicKey::from_vec(&c.source_public_key)
                .map_err(|_| TransactionStorageError::ConversionError)?,
            destination_public_key: PublicKey::from_vec(&c.destination_public_key)
                .map_err(|_| TransactionStorageError::ConversionError)?,
            amount: MicroTari::from(c.amount as u64),
            fee: MicroTari::from(c.fee as u64),
            transaction: serde_json::from_str(&c.transaction_protocol)?,
            status: match c.status {
                0 => TransactionStatus::Completed,
                1 => TransactionStatus::Broadcast,
                2 => TransactionStatus::Mined,
                _ => return Err(TransactionStorageError::ConversionError),
            },
            message: c.message,
            timestamp: c.timestamp,
        })
    }
}

/// These are the fields that can be updated for a Completed Transaction
pub struct UpdateCompletedTransaction {
    status: Option<TransactionStatus>,
}

#[derive(AsChangeset)]
#[table_name = "completed_transactions"]
pub struct UpdateCompletedTransactionSql {
    status: Option<i32>,
}

/// Map a Rust friendly UpdateCompletedTransaction to the Sql data type form
impl From<UpdateCompletedTransaction> for UpdateCompletedTransactionSql {
    fn from(u: UpdateCompletedTransaction) -> Self {
        Self {
            status: u.status.map(|s| s as i32),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::transaction_service::storage::{
        database::{CompletedTransaction, InboundTransaction, OutboundTransaction, TransactionStatus},
        sqlite_db::{
            CompletedTransactionSql,
            InboundTransactionSql,
            OutboundTransactionSql,
            UpdateCompletedTransaction,
        },
    };
    use chrono::Utc;
    use diesel::{r2d2::ConnectionManager, Connection, SqliteConnection};
    use std::convert::TryFrom;
    use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait};
    use tari_test_utils::random::string;
    use tari_transactions::{
        tari_amount::MicroTari,
        transaction::{OutputFeatures, Transaction, UnblindedOutput},
        transaction_protocol::sender::TransactionSenderMessage,
        types::{CryptoFactories, HashDigest, PrivateKey, PublicKey},
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    };
    use tempdir::TempDir;

    #[test]
    fn test_crud() {
        let mut rng = rand::OsRng::new().unwrap();
        let factories = CryptoFactories::default();
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_folder = TempDir::new(string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        let manager = ConnectionManager::<SqliteConnection>::new(db_path);
        let pool = diesel::r2d2::Pool::builder().max_size(1).build(manager).unwrap();

        let conn = pool.get().unwrap();
        conn.execute("PRAGMA foreign_keys = ON").unwrap();

        let mut builder = SenderTransactionProtocol::builder(1);
        let amount = MicroTari::from(10_000);
        let input = UnblindedOutput::new(MicroTari::from(100_000), PrivateKey::random(&mut rng), None);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari::from(177))
            .with_offset(PrivateKey::random(&mut rng))
            .with_private_nonce(PrivateKey::random(&mut rng))
            .with_amount(0, amount)
            .with_message("Yo!".to_string())
            .with_input(
                input.as_transaction_input(&factories.commitment, OutputFeatures::default()),
                input.clone(),
            )
            .with_change_secret(PrivateKey::random(&mut rng));

        let stp = builder.build::<HashDigest>(&factories).unwrap();

        let outbound_tx1 = OutboundTransaction {
            tx_id: 1u64,
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
            amount,
            fee: stp.clone().get_fee_amount().unwrap(),
            sender_protocol: stp.clone(),
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
        };

        let outbound_tx2 = OutboundTransactionSql::try_from(OutboundTransaction {
            tx_id: 2u64,
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
            amount,
            fee: stp.clone().get_fee_amount().unwrap(),
            sender_protocol: stp.clone(),
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
        })
        .unwrap();

        OutboundTransactionSql::from(OutboundTransactionSql::try_from(outbound_tx1.clone()).unwrap())
            .commit(&conn)
            .unwrap();
        OutboundTransactionSql::from(outbound_tx2.clone())
            .commit(&conn)
            .unwrap();

        let outbound_txs = OutboundTransactionSql::index(&conn).unwrap();
        assert_eq!(outbound_txs.len(), 2);

        let returned_outbound_tx =
            OutboundTransaction::try_from(OutboundTransactionSql::find(&1u64, &conn).unwrap()).unwrap();
        assert_eq!(
            OutboundTransactionSql::try_from(returned_outbound_tx).unwrap(),
            OutboundTransactionSql::try_from(outbound_tx1.clone()).unwrap()
        );

        let rtp = ReceiverTransactionProtocol::new(
            TransactionSenderMessage::Single(Box::new(stp.clone().build_single_round_message().unwrap())),
            PrivateKey::random(&mut rng),
            PrivateKey::random(&mut rng),
            OutputFeatures::default(),
            &factories,
        );

        let inbound_tx1 = InboundTransaction {
            tx_id: 2,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
            amount,
            receiver_protocol: rtp.clone(),
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
        };
        let inbound_tx2 = InboundTransaction {
            tx_id: 3,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
            amount,
            receiver_protocol: rtp.clone(),
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
        };

        InboundTransactionSql::try_from(inbound_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();
        InboundTransactionSql::try_from(inbound_tx2)
            .unwrap()
            .commit(&conn)
            .unwrap();

        let inbound_txs = InboundTransactionSql::index(&conn).unwrap();
        assert_eq!(inbound_txs.len(), 2);

        let returned_inbound_tx =
            InboundTransaction::try_from(InboundTransactionSql::find(&2u64, &conn).unwrap()).unwrap();
        assert_eq!(
            InboundTransactionSql::try_from(returned_inbound_tx).unwrap(),
            InboundTransactionSql::try_from(inbound_tx1.clone()).unwrap()
        );

        let tx = Transaction::new(vec![], vec![], vec![], PrivateKey::random(&mut rng));

        let completed_tx1 = CompletedTransaction {
            tx_id: 2,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
            amount,
            fee: MicroTari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::Mined,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
        };
        let completed_tx2 = CompletedTransaction {
            tx_id: 3,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
            amount,
            fee: MicroTari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::Broadcast,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
        };

        CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();
        assert!(CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .commit(&conn)
            .is_err());

        CompletedTransactionSql::try_from(completed_tx2.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();

        let completed_txs = CompletedTransactionSql::index(&conn).unwrap();
        assert_eq!(completed_txs.len(), 2);

        let returned_completed_tx =
            CompletedTransaction::try_from(CompletedTransactionSql::find(&2u64, &conn).unwrap()).unwrap();
        assert_eq!(
            CompletedTransactionSql::try_from(returned_completed_tx).unwrap(),
            CompletedTransactionSql::try_from(completed_tx1.clone()).unwrap()
        );

        assert!(InboundTransactionSql::find(&inbound_tx1.tx_id, &conn).is_ok());
        InboundTransactionSql::try_from(inbound_tx1.clone())
            .unwrap()
            .delete(&conn)
            .unwrap();
        assert!(InboundTransactionSql::try_from(inbound_tx1.clone())
            .unwrap()
            .delete(&conn)
            .is_err());
        assert!(InboundTransactionSql::find(&inbound_tx1.tx_id, &conn).is_err());

        assert!(OutboundTransactionSql::find(&inbound_tx1.tx_id, &conn).is_ok());
        OutboundTransactionSql::try_from(outbound_tx1.clone())
            .unwrap()
            .delete(&conn)
            .unwrap();
        assert!(OutboundTransactionSql::try_from(outbound_tx1.clone())
            .unwrap()
            .delete(&conn)
            .is_err());
        assert!(OutboundTransactionSql::find(&outbound_tx1.tx_id, &conn).is_err());

        assert!(CompletedTransactionSql::find(&completed_tx1.tx_id, &conn).is_ok());
        CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .delete(&conn)
            .unwrap();
        assert!(CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .delete(&conn)
            .is_err());
        assert!(CompletedTransactionSql::find(&completed_tx1.tx_id, &conn).is_err());

        #[cfg(feature = "test_harness")]
        let updated_tx = CompletedTransactionSql::find(&completed_tx2.tx_id, &conn)
            .unwrap()
            .update(
                UpdateCompletedTransaction {
                    status: Some(TransactionStatus::Mined),
                },
                &conn,
            )
            .unwrap();
        #[cfg(feature = "test_harness")]
        assert_eq!(updated_tx.status, 2);
    }
}
