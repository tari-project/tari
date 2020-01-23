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
    output_manager_service::{
        error::OutputManagerStorageError,
        storage::database::{
            DbKey,
            DbKeyValuePair,
            DbValue,
            KeyManagerState,
            OutputManagerBackend,
            PendingTransactionOutputs,
            WriteOperation,
        },
        TxId,
    },
    schema::{key_manager_states, outputs, pending_transaction_outputs},
};
use chrono::{Duration as ChronoDuration, NaiveDateTime, Utc};
use diesel::{
    prelude::*,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    result::Error as DieselError,
    SqliteConnection,
};
use std::{collections::HashMap, convert::TryFrom, io, path::Path, time::Duration};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{OutputFeatures, OutputFlags, UnblindedOutput},
    types::PrivateKey,
};
use tari_utilities::ByteArray;

const DATABASE_CONNECTION_TIMEOUT_MS: u64 = 2000;

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct OutputManagerSqliteDatabase {
    database_connection_pool: Pool<ConnectionManager<SqliteConnection>>,
}
impl OutputManagerSqliteDatabase {
    pub fn new(database_path: String) -> Result<Self, OutputManagerStorageError> {
        let db_exists = Path::new(&database_path).exists();

        let connection = SqliteConnection::establish(&database_path)?;

        connection.execute("PRAGMA foreign_keys = ON")?;
        if !db_exists {
            embed_migrations!("./migrations");
            embedded_migrations::run_with_output(&connection, &mut io::stdout()).map_err(|err| {
                OutputManagerStorageError::DatabaseMigrationError(format!("Database migration failed {}", err))
            })?;
        }
        drop(connection);

        let manager = ConnectionManager::<SqliteConnection>::new(database_path);
        let pool = diesel::r2d2::Pool::builder()
            .connection_timeout(Duration::from_millis(DATABASE_CONNECTION_TIMEOUT_MS))
            .idle_timeout(Some(Duration::from_millis(DATABASE_CONNECTION_TIMEOUT_MS)))
            .build(manager)
            .map_err(|_| OutputManagerStorageError::R2d2Error)?;

        Ok(Self {
            database_connection_pool: pool,
        })
    }
}
impl OutputManagerBackend for OutputManagerSqliteDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| OutputManagerStorageError::R2d2Error)?;

        let result = match key {
            DbKey::SpentOutput(k) => match OutputSql::find_spent(&k.to_vec(), true, &conn) {
                Ok(o) => Some(DbValue::SpentOutput(Box::new(UnblindedOutput::try_from(o)?))),
                Err(e) => {
                    match e {
                        OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                        e => return Err(e),
                    };
                    None
                },
            },
            DbKey::UnspentOutput(k) => match OutputSql::find_spent(&k.to_vec(), false, &conn) {
                Ok(o) => Some(DbValue::UnspentOutput(Box::new(UnblindedOutput::try_from(o)?))),
                Err(e) => {
                    match e {
                        OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                        e => return Err(e),
                    };
                    None
                },
            },
            DbKey::PendingTransactionOutputs(tx_id) => match PendingTransactionOutputSql::find(tx_id, &conn) {
                Ok(p) => {
                    let outputs = OutputSql::find_by_tx_id_and_encumbered(tx_id, &conn)?;
                    Some(DbValue::PendingTransactionOutputs(Box::new(
                        pending_transaction_outputs_from_sql_outputs(&(p.tx_id.clone() as u64), &p.timestamp, outputs)?,
                    )))
                },
                Err(e) => {
                    match e {
                        OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                        e => return Err(e),
                    };
                    None
                },
            },
            DbKey::UnspentOutputs => Some(DbValue::UnspentOutputs(
                OutputSql::index_spent(false, &conn)?
                    .iter()
                    .map(|o| UnblindedOutput::try_from(o.clone()))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            DbKey::SpentOutputs => Some(DbValue::SpentOutputs(
                OutputSql::index_spent(true, &conn)?
                    .iter()
                    .map(|o| UnblindedOutput::try_from(o.clone()))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            DbKey::AllPendingTransactionOutputs => {
                let pending_sql_txs = PendingTransactionOutputSql::index(&conn)?;
                let mut pending_txs = HashMap::new();
                for p_tx in pending_sql_txs {
                    let outputs = OutputSql::find_by_tx_id_and_encumbered(&(p_tx.tx_id.clone() as u64), &conn)?;
                    pending_txs.insert(
                        p_tx.tx_id.clone() as u64,
                        pending_transaction_outputs_from_sql_outputs(
                            &(p_tx.tx_id.clone() as u64),
                            &p_tx.timestamp,
                            outputs,
                        )?,
                    );
                }
                Some(DbValue::AllPendingTransactionOutputs(pending_txs))
            },
            DbKey::KeyManagerState => match KeyManagerStateSql::get_state(&conn).ok() {
                None => None,
                Some(km) => Some(DbValue::KeyManagerState(KeyManagerState::try_from(km)?)),
            },
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| OutputManagerStorageError::R2d2Error)?;

        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::SpentOutput(k, o) => {
                    if let Ok(_) = OutputSql::find(&k.to_vec(), &conn) {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    OutputSql::new(*o, true, false, false, None).commit(&conn)?
                },
                DbKeyValuePair::UnspentOutput(k, o) => {
                    if let Ok(_) = OutputSql::find(&k.to_vec(), &conn) {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    OutputSql::new(*o, false, false, false, None).commit(&conn)?
                },
                DbKeyValuePair::PendingTransactionOutputs(tx_id, p) => {
                    if let Ok(_) = PendingTransactionOutputSql::find(&tx_id, &conn) {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    PendingTransactionOutputSql::new(p.tx_id.clone(), p.timestamp.clone()).commit(&conn)?;
                    for o in p.outputs_to_be_spent {
                        OutputSql::new(o.clone(), false, false, true, Some(p.tx_id.clone())).commit(&conn)?;
                    }
                    for o in p.outputs_to_be_received {
                        OutputSql::new(o.clone(), false, true, true, Some(p.tx_id.clone())).commit(&conn)?;
                    }
                },
                DbKeyValuePair::KeyManagerState(km) => KeyManagerStateSql::set_state(km, &conn)?,
            },
            WriteOperation::Remove(k) => match k {
                DbKey::SpentOutput(s) => match OutputSql::find_spent(&s.to_vec(), true, &conn) {
                    Ok(o) => {
                        o.clone().delete(&conn)?;
                        return Ok(Some(DbValue::SpentOutput(Box::new(UnblindedOutput::try_from(o)?))));
                    },
                    Err(e) => {
                        match e {
                            OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                            e => return Err(e),
                        };
                    },
                },
                DbKey::UnspentOutput(k) => match OutputSql::find_spent(&k.to_vec(), false, &conn) {
                    Ok(o) => {
                        o.clone().delete(&conn)?;
                        return Ok(Some(DbValue::SpentOutput(Box::new(UnblindedOutput::try_from(o)?))));
                    },
                    Err(e) => {
                        match e {
                            OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                            e => return Err(e),
                        };
                    },
                },
                DbKey::PendingTransactionOutputs(tx_id) => match PendingTransactionOutputSql::find(&tx_id, &conn) {
                    Ok(p) => {
                        let outputs = OutputSql::find_by_tx_id_and_encumbered(&(p.tx_id as u64), &conn)?;
                        p.clone().delete(&conn)?;
                        return Ok(Some(DbValue::PendingTransactionOutputs(Box::new(
                            pending_transaction_outputs_from_sql_outputs(
                                &(p.tx_id.clone() as u64),
                                &p.timestamp,
                                outputs,
                            )?,
                        ))));
                    },
                    Err(e) => {
                        match e {
                            OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                            e => return Err(e),
                        };
                    },
                },
                DbKey::UnspentOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::SpentOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::AllPendingTransactionOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::KeyManagerState => return Err(OutputManagerStorageError::OperationNotSupported),
            },
        }

        Ok(None)
    }

    fn confirm_transaction(&self, tx_id: u64) -> Result<(), OutputManagerStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| OutputManagerStorageError::R2d2Error)?;

        match PendingTransactionOutputSql::find(&tx_id, &conn) {
            Ok(p) => {
                let outputs = OutputSql::find_by_tx_id_and_encumbered(&tx_id, &conn)?;

                for o in outputs {
                    if o.to_be_received == 1i32 {
                        o.update(
                            UpdateOutput {
                                spent: None,
                                received: None,
                                encumbered: Some(false),
                                tx_id: None,
                            },
                            &conn,
                        )?;
                    } else if o.to_be_received == 0i32 {
                        o.update(
                            UpdateOutput {
                                spent: Some(true),
                                received: None,
                                encumbered: Some(false),
                                tx_id: None,
                            },
                            &conn,
                        )?;
                    }
                }

                p.delete(&conn)?;
            },
            Err(e) => {
                match e {
                    OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                    e => return Err(e),
                };
            },
        }

        Ok(())
    }

    fn encumber_outputs(
        &self,
        tx_id: u64,
        outputs_to_send: &Vec<UnblindedOutput>,
        change_output: Option<UnblindedOutput>,
    ) -> Result<(), OutputManagerStorageError>
    {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| OutputManagerStorageError::R2d2Error)?;

        let mut outputs_to_be_spent = Vec::new();
        for i in outputs_to_send {
            let output = OutputSql::find(&i.spending_key.to_vec(), &conn)?;
            if output.spent == 1 {
                return Err(OutputManagerStorageError::OutputAlreadySpent);
            }
            outputs_to_be_spent.push(output);
        }

        PendingTransactionOutputSql::new(tx_id.clone(), Utc::now().naive_utc()).commit(&conn)?;

        for o in outputs_to_be_spent {
            o.update(
                UpdateOutput {
                    spent: None,
                    received: None,
                    encumbered: Some(true),
                    tx_id: Some(tx_id.clone()),
                },
                &conn,
            )?;
        }

        if let Some(co) = change_output {
            OutputSql::new(co, false, true, true, Some(tx_id.clone())).commit(&conn)?;
        }

        Ok(())
    }

    fn cancel_pending_transaction(&self, tx_id: u64) -> Result<(), OutputManagerStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| OutputManagerStorageError::R2d2Error)?;

        match PendingTransactionOutputSql::find(&tx_id, &conn) {
            Ok(p) => {
                let outputs = OutputSql::find_by_tx_id_and_encumbered(&tx_id, &conn)?;

                for o in outputs {
                    if o.to_be_received == 1i32 {
                        o.delete(&conn)?;
                    } else if o.to_be_received == 0i32 {
                        o.update(
                            UpdateOutput {
                                spent: None,
                                received: None,
                                encumbered: Some(false),
                                tx_id: None,
                            },
                            &conn,
                        )?;
                        o.update_null(NullOutputSql { tx_id: None }, &conn)?;
                    }
                }

                p.delete(&conn)?;
            },
            Err(e) => {
                match e {
                    OutputManagerStorageError::DieselError(DieselError::NotFound) => {
                        return Err(OutputManagerStorageError::ValueNotFound(
                            DbKey::PendingTransactionOutputs(tx_id.clone()),
                        ))
                    },
                    e => return Err(e),
                };
            },
        }

        Ok(())
    }

    fn timeout_pending_transactions(&self, period: Duration) -> Result<(), OutputManagerStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| OutputManagerStorageError::R2d2Error)?;

        let older_pending_txs = PendingTransactionOutputSql::index_older(
            Utc::now().naive_utc() - ChronoDuration::from_std(period)?,
            &conn,
        )?;
        drop(conn);
        for ptx in older_pending_txs {
            self.cancel_pending_transaction(ptx.tx_id.clone() as u64)?;
        }
        Ok(())
    }

    fn increment_key_index(&self) -> Result<(), OutputManagerStorageError> {
        let conn = self
            .database_connection_pool
            .clone()
            .get()
            .map_err(|_| OutputManagerStorageError::R2d2Error)?;

        KeyManagerStateSql::increment_index(&conn)?;

        Ok(())
    }
}

/// A utility function to construct a PendingTransactionOutputs structure for a TxId, set of Outputs and a Timestamp
fn pending_transaction_outputs_from_sql_outputs(
    tx_id: &TxId,
    timestamp: &NaiveDateTime,
    outputs: Vec<OutputSql>,
) -> Result<PendingTransactionOutputs, OutputManagerStorageError>
{
    let mut outputs_to_be_spent = Vec::new();
    let mut outputs_to_be_received = Vec::new();
    for o in outputs {
        if o.to_be_received == 1i32 {
            outputs_to_be_received.push(UnblindedOutput::try_from(o.clone())?);
        } else if o.to_be_received == 0i32 {
            outputs_to_be_spent.push(UnblindedOutput::try_from(o.clone())?);
        }
    }

    Ok(PendingTransactionOutputs {
        tx_id: tx_id.clone() as u64,
        outputs_to_be_spent,
        outputs_to_be_received,
        timestamp: timestamp.clone(),
    })
}

/// This struct represents an Output in the Sql database. A distinct struct is required to define the Sql friendly
/// equivalent datatypes for the members.
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "outputs"]
struct OutputSql {
    spending_key: Vec<u8>,
    value: i64,
    flags: i32,
    maturity: i64,
    spent: i32,
    to_be_received: i32,
    encumbered: i32,
    tx_id: Option<i64>,
}

impl OutputSql {
    pub fn new(
        output: UnblindedOutput,
        spent: bool,
        to_be_received: bool,
        encumbered: bool,
        tx_id: Option<TxId>,
    ) -> Self
    {
        Self {
            spending_key: output.spending_key.to_vec(),
            value: (u64::from(output.value)) as i64,
            flags: output.features.flags.bits() as i32,
            maturity: output.features.maturity as i64,
            spent: spent as i32,
            to_be_received: to_be_received as i32,
            encumbered: encumbered as i32,
            tx_id: tx_id.map(|i| i as i64),
        }
    }

    /// Write this struct to the database
    pub fn commit(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), OutputManagerStorageError>
    {
        diesel::insert_into(outputs::table).values(self.clone()).execute(conn)?;
        Ok(())
    }

    /// Return all unencumbered outputs
    #[cfg(test)]
    pub fn index(
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::encumbered.eq(false as i32))
            .load::<OutputSql>(conn)?)
    }

    /// Return all unencumbered outputs with the specified spent status
    pub fn index_spent(
        spent: bool,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError>
    {
        Ok(outputs::table
            .filter(outputs::encumbered.eq(false as i32))
            .filter(outputs::spent.eq(spent as i32))
            .load(conn)?)
    }

    /// Find a particular Output, if it exists
    pub fn find(
        spending_key: &Vec<u8>,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<OutputSql, OutputManagerStorageError>
    {
        Ok(outputs::table
            .filter(outputs::spending_key.eq(spending_key))
            .first::<OutputSql>(conn)?)
    }

    /// Find outputs via tx_id that are encumbered. Any outputs that are encumbered cannot be marked as spent.
    pub fn find_by_tx_id_and_encumbered(
        tx_id: &TxId,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError>
    {
        Ok(outputs::table
            .filter(outputs::tx_id.eq(Some(*tx_id as i64)))
            .filter(outputs::encumbered.eq(true as i32))
            .load(conn)?)
    }

    /// Find a particular Output, if it exists and is in the specified Spent state
    pub fn find_spent(
        spending_key: &Vec<u8>,
        spent: bool,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<OutputSql, OutputManagerStorageError>
    {
        Ok(outputs::table
            .filter(outputs::spent.eq(spent as i32))
            .filter(outputs::spending_key.eq(spending_key))
            .first::<OutputSql>(conn)?)
    }

    pub fn delete(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), OutputManagerStorageError>
    {
        let num_deleted =
            diesel::delete(outputs::table.filter(outputs::spending_key.eq(&self.spending_key))).execute(conn)?;

        if num_deleted == 0 {
            return Err(OutputManagerStorageError::ValuesNotFound);
        }

        Ok(())
    }

    pub fn update(
        &self,
        updated_output: UpdateOutput,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<OutputSql, OutputManagerStorageError>
    {
        let num_updated = diesel::update(outputs::table.filter(outputs::spending_key.eq(&self.spending_key)))
            .set(UpdateOutputSql::from(updated_output))
            .execute(conn)?;

        if num_updated == 0 {
            return Err(OutputManagerStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(OutputSql::find(&self.spending_key, conn)?)
    }

    /// This function is used to update an existing record to set fields to null
    pub fn update_null(
        &self,
        updated_null: NullOutputSql,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<OutputSql, OutputManagerStorageError>
    {
        let num_updated = diesel::update(outputs::table.filter(outputs::spending_key.eq(&self.spending_key)))
            .set(updated_null)
            .execute(conn)?;

        if num_updated == 0 {
            return Err(OutputManagerStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(OutputSql::find(&self.spending_key, conn)?)
    }
}

/// Conversion from an UnblindedOutput to the Sql datatype form
impl TryFrom<OutputSql> for UnblindedOutput {
    type Error = OutputManagerStorageError;

    fn try_from(o: OutputSql) -> Result<Self, Self::Error> {
        Ok(Self {
            value: MicroTari::from(o.value as u64),
            spending_key: PrivateKey::from_vec(&o.spending_key)
                .map_err(|_| OutputManagerStorageError::ConversionError)?,
            features: OutputFeatures {
                flags: OutputFlags::from_bits(o.flags as u8).ok_or(OutputManagerStorageError::ConversionError)?,
                maturity: o.maturity as u64,
            },
        })
    }
}

/// These are the fields that can be updated for an Output
pub struct UpdateOutput {
    spent: Option<bool>,
    received: Option<bool>,
    encumbered: Option<bool>,
    tx_id: Option<TxId>,
}

#[derive(AsChangeset)]
#[table_name = "outputs"]
pub struct UpdateOutputSql {
    spent: Option<i32>,
    to_be_received: Option<i32>,
    encumbered: Option<i32>,
    tx_id: Option<i64>,
}

#[derive(AsChangeset)]
#[table_name = "outputs"]
#[changeset_options(treat_none_as_null = "true")]
/// This struct is used to set the contained field to null
pub struct NullOutputSql {
    tx_id: Option<i64>,
}

/// Map a Rust friendly UpdateOutput to the Sql data type form
impl From<UpdateOutput> for UpdateOutputSql {
    fn from(u: UpdateOutput) -> Self {
        Self {
            spent: u.spent.map(|s| s as i32),
            to_be_received: u.received.map(|r| r as i32),
            encumbered: u.encumbered.map(|e| e as i32),
            tx_id: u.tx_id.map(|t| t as i64),
        }
    }
}

/// This struct represents a PendingTransactionOutputs  in the Sql database. A distinct struct is required to define the
/// Sql friendly equivalent datatypes for the members.
#[derive(Clone, Queryable, Insertable)]
#[table_name = "pending_transaction_outputs"]
struct PendingTransactionOutputSql {
    tx_id: i64,
    timestamp: NaiveDateTime,
}
impl PendingTransactionOutputSql {
    pub fn new(tx_id: TxId, timestamp: NaiveDateTime) -> Self {
        Self {
            tx_id: tx_id as i64,
            timestamp,
        }
    }

    pub fn commit(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), OutputManagerStorageError>
    {
        diesel::insert_into(pending_transaction_outputs::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn find(
        tx_id: &TxId,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<PendingTransactionOutputSql, OutputManagerStorageError>
    {
        Ok(pending_transaction_outputs::table
            .filter(pending_transaction_outputs::tx_id.eq(tx_id.clone() as i64))
            .first::<PendingTransactionOutputSql>(conn)?)
    }

    pub fn index(
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<PendingTransactionOutputSql>, OutputManagerStorageError> {
        Ok(pending_transaction_outputs::table.load::<PendingTransactionOutputSql>(conn)?)
    }

    pub fn index_older(
        timestamp: NaiveDateTime,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<PendingTransactionOutputSql>, OutputManagerStorageError>
    {
        Ok(pending_transaction_outputs::table
            .filter(pending_transaction_outputs::timestamp.lt(timestamp))
            .load::<PendingTransactionOutputSql>(conn)?)
    }

    pub fn delete(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), OutputManagerStorageError>
    {
        let num_deleted = diesel::delete(
            pending_transaction_outputs::table.filter(pending_transaction_outputs::tx_id.eq(&self.tx_id)),
        )
        .execute(conn)?;

        if num_deleted == 0 {
            return Err(OutputManagerStorageError::ValuesNotFound);
        }

        let outputs = OutputSql::find_by_tx_id_and_encumbered(&(self.tx_id as u64), &conn)?;
        for o in outputs {
            o.delete(&conn)?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Queryable, Insertable)]
#[table_name = "key_manager_states"]
struct KeyManagerStateSql {
    id: Option<i64>,
    master_seed: Vec<u8>,
    branch_seed: String,
    primary_key_index: i64,
    timestamp: NaiveDateTime,
}

impl From<KeyManagerState> for KeyManagerStateSql {
    fn from(km: KeyManagerState) -> Self {
        Self {
            id: None,
            master_seed: km.master_seed.to_vec(),
            branch_seed: km.branch_seed,
            primary_key_index: km.primary_key_index as i64,
            timestamp: Utc::now().naive_utc(),
        }
    }
}

impl TryFrom<KeyManagerStateSql> for KeyManagerState {
    type Error = OutputManagerStorageError;

    fn try_from(km: KeyManagerStateSql) -> Result<Self, Self::Error> {
        Ok(Self {
            master_seed: PrivateKey::from_vec(&km.master_seed)
                .map_err(|_| OutputManagerStorageError::ConversionError)?,
            branch_seed: km.branch_seed,
            primary_key_index: km.primary_key_index as usize,
        })
    }
}

impl KeyManagerStateSql {
    fn commit(
        &self,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), OutputManagerStorageError>
    {
        diesel::insert_into(key_manager_states::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn get_state(
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<KeyManagerStateSql, OutputManagerStorageError> {
        Ok(key_manager_states::table
            .first::<KeyManagerStateSql>(conn)
            .map_err(|_| OutputManagerStorageError::KeyManagerNotInitialized)?)
    }

    pub fn set_state(
        key_manager_state: KeyManagerState,
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), OutputManagerStorageError>
    {
        match KeyManagerStateSql::get_state(conn) {
            Ok(km) => {
                let update = KeyManagerStateUpdate {
                    master_seed: Some(key_manager_state.master_seed),
                    branch_seed: Some(key_manager_state.branch_seed),
                    primary_key_index: Some(key_manager_state.primary_key_index),
                };

                let num_updated = diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(KeyManagerStateUpdateSql::from(update))
                    .execute(conn)?;
                if num_updated == 0 {
                    return Err(OutputManagerStorageError::UnexpectedResult(
                        "Database update error".to_string(),
                    ));
                }
            },
            Err(_) => KeyManagerStateSql::from(key_manager_state).commit(conn)?,
        }
        Ok(())
    }

    pub fn increment_index(
        conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<usize, OutputManagerStorageError> {
        Ok(match KeyManagerStateSql::get_state(conn) {
            Ok(km) => {
                let current_index = (km.primary_key_index + 1) as usize;
                let update = KeyManagerStateUpdate {
                    master_seed: None,
                    branch_seed: None,
                    primary_key_index: Some(current_index),
                };
                let num_updated = diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(KeyManagerStateUpdateSql::from(update))
                    .execute(conn)?;
                if num_updated == 0 {
                    return Err(OutputManagerStorageError::UnexpectedResult(
                        "Database update error".to_string(),
                    ));
                }
                current_index
            },
            Err(_) => return Err(OutputManagerStorageError::KeyManagerNotInitialized),
        })
    }
}

struct KeyManagerStateUpdate {
    master_seed: Option<PrivateKey>,
    branch_seed: Option<String>,
    primary_key_index: Option<usize>,
}

#[derive(AsChangeset)]
#[table_name = "key_manager_states"]
struct KeyManagerStateUpdateSql {
    master_seed: Option<Vec<u8>>,
    branch_seed: Option<String>,
    primary_key_index: Option<i64>,
}

impl From<KeyManagerStateUpdate> for KeyManagerStateUpdateSql {
    fn from(km: KeyManagerStateUpdate) -> Self {
        Self {
            master_seed: km.master_seed.map(|ms| ms.to_vec()),
            branch_seed: km.branch_seed,
            primary_key_index: km.primary_key_index.map(|i| i as i64),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::output_manager_service::storage::{
        database::KeyManagerState,
        sqlite_db::{KeyManagerStateSql, OutputSql, PendingTransactionOutputSql, UpdateOutput},
    };
    use chrono::{Duration as ChronoDuration, Utc};
    use diesel::{r2d2::ConnectionManager, Connection, SqliteConnection};
    use rand::{distributions::Alphanumeric, CryptoRng, OsRng, Rng, RngCore};
    use std::{convert::TryFrom, iter, time::Duration};
    use tari_core::transactions::{
        tari_amount::MicroTari,
        transaction::{OutputFeatures, TransactionInput, UnblindedOutput},
        types::{CommitmentFactory, PrivateKey},
    };
    use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey};
    use tempdir::TempDir;

    pub fn random_string(len: usize) -> String {
        let mut rng = OsRng::new().unwrap();
        iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
    }

    pub fn make_input<R: Rng + CryptoRng>(rng: &mut R, val: MicroTari) -> (TransactionInput, UnblindedOutput) {
        let key = PrivateKey::random(rng);
        let factory = CommitmentFactory::default();
        let commitment = factory.commit_value(&key, val.into());
        let input = TransactionInput::new(OutputFeatures::default(), commitment);
        (input, UnblindedOutput::new(val, key, None))
    }

    #[test]
    fn test_crud() {
        let mut rng = rand::OsRng::new().unwrap();

        let db_name = format!("{}.sqlite3", random_string(8).as_str());
        let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        let manager = ConnectionManager::<SqliteConnection>::new(db_path);
        let pool = diesel::r2d2::Pool::builder().max_size(1).build(manager).unwrap();

        let conn = pool.get().unwrap();
        conn.execute("PRAGMA foreign_keys = ON").unwrap();

        let mut outputs = Vec::new();
        let mut outputs_spent = Vec::new();
        let mut outputs_unspent = Vec::new();

        for _i in 0..2 {
            let (_, uo) = make_input(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
            let o = OutputSql::new(uo, false, false, false, None);
            outputs.push(o.clone());
            outputs_unspent.push(o.clone());
            o.commit(&conn).unwrap();
        }

        for _i in 0..3 {
            let (_, uo) = make_input(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
            let o = OutputSql::new(uo, true, false, false, None);
            outputs.push(o.clone());
            outputs_spent.push(o.clone());
            o.commit(&conn).unwrap();
        }

        assert_eq!(OutputSql::index(&conn).unwrap(), outputs);
        assert_eq!(OutputSql::index_spent(false, &conn).unwrap(), outputs_unspent);
        assert_eq!(OutputSql::index_spent(true, &conn).unwrap(), outputs_spent);

        assert_eq!(
            OutputSql::find(&outputs[0].spending_key, &conn).unwrap().spending_key,
            outputs[0].spending_key
        );

        assert_eq!(
            OutputSql::find_spent(&outputs[0].spending_key, false, &conn)
                .unwrap()
                .spending_key,
            outputs[0].spending_key
        );

        assert!(OutputSql::find_spent(&outputs[0].spending_key, true, &conn).is_err());

        let _ = OutputSql::find(&outputs[4].spending_key, &conn).unwrap().delete(&conn);

        assert_eq!(OutputSql::index(&conn).unwrap().len(), 4);

        let tx_id = 44u64;

        assert!(OutputSql::find(&outputs[0].spending_key, &conn)
            .unwrap()
            .update(
                UpdateOutput {
                    spent: None,
                    received: None,
                    encumbered: None,
                    tx_id: Some(tx_id),
                },
                &conn,
            )
            .is_err());

        PendingTransactionOutputSql::new(tx_id, Utc::now().naive_utc())
            .commit(&conn)
            .unwrap();

        PendingTransactionOutputSql::new(11u64, Utc::now().naive_utc())
            .commit(&conn)
            .unwrap();

        let pt = PendingTransactionOutputSql::find(&tx_id, &conn).unwrap();

        assert_eq!(pt.tx_id as u64, tx_id);

        let pts = PendingTransactionOutputSql::index(&conn).unwrap();

        assert_eq!(pts.len(), 2);

        let _updated1 = OutputSql::find(&outputs[0].spending_key, &conn)
            .unwrap()
            .update(
                UpdateOutput {
                    spent: Some(false),
                    received: None,
                    encumbered: None,
                    tx_id: Some(44u64),
                },
                &conn,
            )
            .unwrap();

        let _updated2 = OutputSql::find(&outputs[1].spending_key, &conn)
            .unwrap()
            .update(
                UpdateOutput {
                    spent: Some(false),
                    received: None,
                    encumbered: Some(true),
                    tx_id: Some(44u64),
                },
                &conn,
            )
            .unwrap();

        let result = OutputSql::find_by_tx_id_and_encumbered(&44u64, &conn).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spending_key, outputs[1].spending_key);

        PendingTransactionOutputSql::new(
            12u64,
            Utc::now().naive_utc() - ChronoDuration::from_std(Duration::from_millis(600_000)).unwrap(),
        )
        .commit(&conn)
        .unwrap();

        let pending_older1 = PendingTransactionOutputSql::index_older(Utc::now().naive_utc(), &conn).unwrap();
        assert_eq!(pending_older1.len(), 3);

        let pending_older2 = PendingTransactionOutputSql::index_older(
            Utc::now().naive_utc() - ChronoDuration::from_std(Duration::from_millis(200_000)).unwrap(),
            &conn,
        )
        .unwrap();
        assert_eq!(pending_older2.len(), 1);
    }

    #[test]
    fn test_key_manager_crud() {
        let mut rng = rand::OsRng::new().unwrap();

        let db_name = format!("{}.sqlite3", random_string(8).as_str());
        let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        let manager = ConnectionManager::<SqliteConnection>::new(db_path);
        let pool = diesel::r2d2::Pool::builder().max_size(1).build(manager).unwrap();

        let conn = pool.get().unwrap();
        conn.execute("PRAGMA foreign_keys = ON").unwrap();

        assert!(KeyManagerStateSql::get_state(&conn).is_err());

        let state1 = KeyManagerState {
            master_seed: PrivateKey::random(&mut rng),
            branch_seed: random_string(8),
            primary_key_index: 0,
        };

        KeyManagerStateSql::set_state(state1.clone(), &conn).unwrap();

        let state1_read = KeyManagerStateSql::get_state(&conn).unwrap();

        assert_eq!(state1, KeyManagerState::try_from(state1_read).unwrap());

        let state2 = KeyManagerState {
            master_seed: PrivateKey::random(&mut rng),
            branch_seed: random_string(8),
            primary_key_index: 0,
        };

        KeyManagerStateSql::set_state(state2.clone(), &conn).unwrap();

        let state2_read = KeyManagerStateSql::get_state(&conn).unwrap();

        assert_eq!(state2, KeyManagerState::try_from(state2_read).unwrap());

        KeyManagerStateSql::increment_index(&conn).unwrap();
        KeyManagerStateSql::increment_index(&conn).unwrap();

        let state3_read = KeyManagerStateSql::get_state(&conn).unwrap();

        assert_eq!(state3_read.primary_key_index, 2);
    }
}
