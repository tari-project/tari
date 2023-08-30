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

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    str::from_utf8,
    sync::{Arc, RwLock},
};

use chacha20poly1305::XChaCha20Poly1305;
use chrono::{NaiveDateTime, Utc};
use diesel::{prelude::*, result::Error as DieselError, SqliteConnection};
use log::*;
use tari_common_sqlite::{sqlite_connection_pool::PooledDbConnection, util::diesel_ext::ExpectedRowsExtension};
use tari_common_types::{
    encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    tari_address::TariAddress,
    transaction::{
        TransactionConversionError,
        TransactionDirection,
        TransactionDirectionError,
        TransactionStatus,
        TxId,
    },
    types::{BlockHash, PrivateKey, PublicKey, Signature},
};
use tari_core::transactions::tari_amount::MicroMinotari;
use tari_utilities::{
    hex::{from_hex, Hex},
    ByteArray,
    Hidden,
};
use thiserror::Error;
use tokio::time::Instant;
use zeroize::Zeroize;

use crate::{
    schema::{completed_transactions, inbound_transactions, outbound_transactions},
    storage::sqlite_utilities::wallet_db_connection::WalletDbConnection,
    transaction_service::{
        error::{TransactionKeyError, TransactionStorageError},
        storage::{
            database::{DbKey, DbKeyValuePair, DbValue, TransactionBackend, WriteOperation},
            models::{
                CompletedTransaction,
                InboundTransaction,
                OutboundTransaction,
                TxCancellationReason,
                WalletTransaction,
            },
        },
    },
};
const LOG_TARGET: &str = "wallet::transaction_service::database::wallet";

/// A Sqlite backend for the Transaction Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct TransactionServiceSqliteDatabase {
    database_connection: WalletDbConnection,
    cipher: Arc<RwLock<XChaCha20Poly1305>>,
}

impl TransactionServiceSqliteDatabase {
    pub fn new(database_connection: WalletDbConnection, cipher: XChaCha20Poly1305) -> Self {
        Self {
            database_connection,
            cipher: Arc::new(RwLock::new(cipher)),
        }
    }

    fn insert(&self, kvp: DbKeyValuePair, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        let cipher = acquire_read_lock!(self.cipher);

        match kvp {
            DbKeyValuePair::PendingOutboundTransaction(k, v) => {
                if OutboundTransactionSql::find_by_cancelled(k, false, conn).is_ok() {
                    return Err(TransactionStorageError::DuplicateOutput);
                }
                let o = OutboundTransactionSql::try_from(*v, &cipher)?;
                o.commit(conn)?;
            },
            DbKeyValuePair::PendingInboundTransaction(k, v) => {
                if InboundTransactionSql::find_by_cancelled(k, false, conn).is_ok() {
                    return Err(TransactionStorageError::DuplicateOutput);
                }
                let i = InboundTransactionSql::try_from(*v, &cipher)?;

                i.commit(conn)?;
            },
            DbKeyValuePair::CompletedTransaction(k, v) => {
                if CompletedTransactionSql::find_by_cancelled(k, false, conn).is_ok() {
                    return Err(TransactionStorageError::DuplicateOutput);
                }
                let c = CompletedTransactionSql::try_from(*v, &cipher)?;

                c.commit(conn)?;
            },
        }
        Ok(())
    }

    fn remove(&self, key: DbKey, conn: &mut SqliteConnection) -> Result<Option<DbValue>, TransactionStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        match key {
            DbKey::PendingOutboundTransaction(k) => {
                conn.transaction::<_, _, _>(|conn| match OutboundTransactionSql::find_by_cancelled(k, false, conn) {
                    Ok(v) => {
                        v.delete(conn)?;
                        Ok(Some(DbValue::PendingOutboundTransaction(Box::new(
                            OutboundTransaction::try_from(v, &cipher)?,
                        ))))
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => Err(
                        TransactionStorageError::ValueNotFound(DbKey::PendingOutboundTransaction(k)),
                    ),
                    Err(e) => Err(e),
                })
            },
            DbKey::PendingInboundTransaction(k) => {
                conn.transaction::<_, _, _>(|conn| match InboundTransactionSql::find_by_cancelled(k, false, conn) {
                    Ok(v) => {
                        v.delete(conn)?;
                        Ok(Some(DbValue::PendingInboundTransaction(Box::new(
                            InboundTransaction::try_from(v, &cipher)?,
                        ))))
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => Err(
                        TransactionStorageError::ValueNotFound(DbKey::PendingOutboundTransaction(k)),
                    ),
                    Err(e) => Err(e),
                })
            },
            DbKey::CompletedTransaction(k) => {
                conn.transaction::<_, _, _>(
                    |conn| match CompletedTransactionSql::find_by_cancelled(k, false, conn) {
                        Ok(v) => {
                            v.delete(conn)?;
                            Ok(Some(DbValue::CompletedTransaction(Box::new(
                                CompletedTransaction::try_from(v, &cipher)?,
                            ))))
                        },
                        Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                            Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(k)))
                        },
                        Err(e) => Err(e),
                    },
                )
            },
            DbKey::PendingOutboundTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::PendingInboundTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CompletedTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CancelledPendingOutboundTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CancelledPendingInboundTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CancelledCompletedTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CancelledPendingOutboundTransaction(k) => {
                conn.transaction::<_, _, _>(|conn| match OutboundTransactionSql::find_by_cancelled(k, true, conn) {
                    Ok(v) => {
                        v.delete(conn)?;
                        Ok(Some(DbValue::PendingOutboundTransaction(Box::new(
                            OutboundTransaction::try_from(v, &cipher)?,
                        ))))
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => Err(
                        TransactionStorageError::ValueNotFound(DbKey::CancelledPendingOutboundTransaction(k)),
                    ),
                    Err(e) => Err(e),
                })
            },
            DbKey::CancelledPendingInboundTransaction(k) => {
                conn.transaction::<_, _, _>(|conn| match InboundTransactionSql::find_by_cancelled(k, true, conn) {
                    Ok(v) => {
                        v.delete(conn)?;
                        Ok(Some(DbValue::PendingInboundTransaction(Box::new(
                            InboundTransaction::try_from(v, &cipher)?,
                        ))))
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => Err(
                        TransactionStorageError::ValueNotFound(DbKey::CancelledPendingOutboundTransaction(k)),
                    ),
                    Err(e) => Err(e),
                })
            },
            DbKey::AnyTransaction(_) => Err(TransactionStorageError::OperationNotSupported),
        }
    }
}

impl TransactionBackend for TransactionServiceSqliteDatabase {
    #[allow(clippy::too_many_lines)]
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let result = match key {
            DbKey::PendingOutboundTransaction(t) => {
                match OutboundTransactionSql::find_by_cancelled(*t, false, &mut conn) {
                    Ok(o) => Some(DbValue::PendingOutboundTransaction(Box::new(
                        OutboundTransaction::try_from(o, &cipher)?,
                    ))),
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                    Err(e) => return Err(e),
                }
            },
            DbKey::PendingInboundTransaction(t) => match InboundTransactionSql::find_by_cancelled(*t, false, &mut conn)
            {
                Ok(i) => Some(DbValue::PendingInboundTransaction(Box::new(
                    InboundTransaction::try_from(i, &cipher)?,
                ))),
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::CompletedTransaction(t) => match CompletedTransactionSql::find(*t, &mut conn) {
                Ok(c) => Some(DbValue::CompletedTransaction(Box::new(CompletedTransaction::try_from(
                    c, &cipher,
                )?))),
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::AnyTransaction(t) => {
                match OutboundTransactionSql::find(*t, &mut conn) {
                    Ok(o) => {
                        return Ok(Some(DbValue::WalletTransaction(Box::new(
                            WalletTransaction::PendingOutbound(OutboundTransaction::try_from(o, &cipher)?),
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                };
                match InboundTransactionSql::find(*t, &mut conn) {
                    Ok(i) => {
                        return Ok(Some(DbValue::WalletTransaction(Box::new(
                            WalletTransaction::PendingInbound(InboundTransaction::try_from(i, &cipher)?),
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                };
                match CompletedTransactionSql::find(*t, &mut conn) {
                    Ok(c) => {
                        return Ok(Some(DbValue::WalletTransaction(Box::new(
                            WalletTransaction::Completed(CompletedTransaction::try_from(c, &cipher)?),
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                };

                None
            },
            DbKey::PendingOutboundTransactions => {
                let mut result = HashMap::new();
                for o in OutboundTransactionSql::index_by_cancelled(&mut conn, false)? {
                    result.insert(
                        (o.tx_id as u64).into(),
                        OutboundTransaction::try_from(o.clone(), &cipher)?,
                    );
                }

                Some(DbValue::PendingOutboundTransactions(result))
            },
            DbKey::PendingInboundTransactions => {
                let mut result = HashMap::new();
                for i in InboundTransactionSql::index_by_cancelled(&mut conn, false)? {
                    result.insert(
                        (i.tx_id as u64).into(),
                        InboundTransaction::try_from((i).clone(), &cipher)?,
                    );
                }

                Some(DbValue::PendingInboundTransactions(result))
            },
            DbKey::CompletedTransactions => {
                let mut result = HashMap::new();
                for c in CompletedTransactionSql::index_by_cancelled(&mut conn, false)? {
                    result.insert(
                        (c.tx_id as u64).into(),
                        CompletedTransaction::try_from((c).clone(), &cipher)?,
                    );
                }

                Some(DbValue::CompletedTransactions(result))
            },
            DbKey::CancelledPendingOutboundTransactions => {
                let mut result = HashMap::new();
                for o in OutboundTransactionSql::index_by_cancelled(&mut conn, true)? {
                    result.insert(
                        (o.tx_id as u64).into(),
                        OutboundTransaction::try_from((o).clone(), &cipher)?,
                    );
                }

                Some(DbValue::PendingOutboundTransactions(result))
            },
            DbKey::CancelledPendingInboundTransactions => {
                let mut result = HashMap::new();
                for i in InboundTransactionSql::index_by_cancelled(&mut conn, true)? {
                    result.insert(
                        (i.tx_id as u64).into(),
                        InboundTransaction::try_from(i.clone(), &cipher)?,
                    );
                }

                Some(DbValue::PendingInboundTransactions(result))
            },
            DbKey::CancelledCompletedTransactions => {
                let mut result = HashMap::new();
                for c in CompletedTransactionSql::index_by_cancelled(&mut conn, true)? {
                    result.insert(
                        (c.tx_id as u64).into(),
                        CompletedTransaction::try_from((c).clone(), &cipher)?,
                    );
                }

                Some(DbValue::CompletedTransactions(result))
            },
            DbKey::CancelledPendingOutboundTransaction(t) => {
                match OutboundTransactionSql::find_by_cancelled(*t, true, &mut conn) {
                    Ok(o) => Some(DbValue::PendingOutboundTransaction(Box::new(
                        OutboundTransaction::try_from(o, &cipher)?,
                    ))),
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                    Err(e) => return Err(e),
                }
            },
            DbKey::CancelledPendingInboundTransaction(t) => {
                match InboundTransactionSql::find_by_cancelled(*t, true, &mut conn) {
                    Ok(i) => Some(DbValue::PendingInboundTransaction(Box::new(
                        InboundTransaction::try_from(i, &cipher)?,
                    ))),
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                    Err(e) => return Err(e),
                }
            },
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch '{}': lock {} + db_op {} = {} ms",
                key,
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(result)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = match key {
            DbKey::PendingOutboundTransaction(k) => {
                OutboundTransactionSql::find_by_cancelled(*k, false, &mut conn).is_ok()
            },
            DbKey::PendingInboundTransaction(k) => {
                InboundTransactionSql::find_by_cancelled(*k, false, &mut conn).is_ok()
            },
            DbKey::CompletedTransaction(k) => CompletedTransactionSql::find(*k, &mut conn).is_ok(),
            DbKey::PendingOutboundTransactions => false,
            DbKey::PendingInboundTransactions => false,
            DbKey::CompletedTransactions => false,
            DbKey::CancelledPendingOutboundTransactions => false,
            DbKey::CancelledPendingInboundTransactions => false,
            DbKey::CancelledCompletedTransactions => false,
            DbKey::CancelledPendingOutboundTransaction(k) => {
                OutboundTransactionSql::find_by_cancelled(*k, true, &mut conn).is_ok()
            },
            DbKey::CancelledPendingInboundTransaction(k) => {
                InboundTransactionSql::find_by_cancelled(*k, true, &mut conn).is_ok()
            },
            DbKey::AnyTransaction(k) => {
                CompletedTransactionSql::find(*k, &mut conn).is_ok() ||
                    InboundTransactionSql::find(*k, &mut conn).is_ok() ||
                    OutboundTransactionSql::find(*k, &mut conn).is_ok()
            },
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - contains '{}': lock {} + db_op {} = {} ms",
                key,
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let key_text;

        let result = match op {
            WriteOperation::Insert(kvp) => {
                key_text = "Insert";
                self.insert(kvp, &mut conn).map(|_| None)
            },
            WriteOperation::Remove(key) => {
                key_text = "Remove";
                self.remove(key, &mut conn)
            },
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - write '{}': lock {} + db_op {} = {} ms",
                key_text,
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        result
    }

    fn transaction_exists(&self, tx_id: TxId) -> Result<bool, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = OutboundTransactionSql::find_by_cancelled(tx_id, false, &mut conn).is_ok() ||
            InboundTransactionSql::find_by_cancelled(tx_id, false, &mut conn).is_ok() ||
            CompletedTransactionSql::find_by_cancelled(tx_id, false, &mut conn).is_ok();
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - transaction_exists: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(result)
    }

    fn get_pending_transaction_counterparty_address_by_tx_id(
        &self,
        tx_id: TxId,
    ) -> Result<TariAddress, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        if let Ok(outbound_tx_sql) = OutboundTransactionSql::find_by_cancelled(tx_id, false, &mut conn) {
            let outbound_tx = OutboundTransaction::try_from(outbound_tx_sql, &cipher)?;
            if start.elapsed().as_millis() > 0 {
                trace!(
                    target: LOG_TARGET,
                    "sqlite profile - get_pending_transaction_counterparty_pub_key_by_tx_id: lock {} + db_op {} = {} \
                     ms",
                    acquire_lock.as_millis(),
                    (start.elapsed() - acquire_lock).as_millis(),
                    start.elapsed().as_millis()
                );
            }
            return Ok(outbound_tx.destination_address);
        }
        if let Ok(inbound_tx_sql) = InboundTransactionSql::find_by_cancelled(tx_id, false, &mut conn) {
            let inbound_tx = InboundTransaction::try_from(inbound_tx_sql, &cipher)?;
            if start.elapsed().as_millis() > 0 {
                trace!(
                    target: LOG_TARGET,
                    "sqlite profile - get_pending_transaction_counterparty_pub_key_by_tx_id: lock {} + db_op {} = {} \
                     ms",
                    acquire_lock.as_millis(),
                    (start.elapsed() - acquire_lock).as_millis(),
                    start.elapsed().as_millis()
                );
            }
            return Ok(inbound_tx.source_address);
        }

        Err(TransactionStorageError::ValuesNotFound)
    }

    fn fetch_any_cancelled_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<Option<WalletTransaction>, TransactionStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;
        let cipher = acquire_read_lock!(self.cipher);

        match OutboundTransactionSql::find_by_cancelled(tx_id, true, &mut conn) {
            Ok(o) => {
                return Ok(Some(WalletTransaction::PendingOutbound(OutboundTransaction::try_from(
                    o, &cipher,
                )?)));
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
            Err(e) => return Err(e),
        };
        match InboundTransactionSql::find_by_cancelled(tx_id, true, &mut conn) {
            Ok(i) => {
                return Ok(Some(WalletTransaction::PendingInbound(InboundTransaction::try_from(
                    i, &cipher,
                )?)));
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
            Err(e) => return Err(e),
        };
        match CompletedTransactionSql::find_by_cancelled(tx_id, true, &mut conn) {
            Ok(c) => {
                return Ok(Some(WalletTransaction::Completed(CompletedTransaction::try_from(
                    c, &cipher,
                )?)));
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
            Err(e) => return Err(e),
        };
        Ok(None)
    }

    fn complete_outbound_transaction(
        &self,
        tx_id: TxId,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        if CompletedTransactionSql::find_by_cancelled(tx_id, false, &mut conn).is_ok() {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }

        let completed_tx_sql = CompletedTransactionSql::try_from(completed_transaction, &cipher)?;

        conn.transaction::<_, _, _>(|conn| {
            match OutboundTransactionSql::complete_outbound_transaction(tx_id, conn) {
                Ok(_) => completed_tx_sql.commit(conn)?,
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                    return Err(TransactionStorageError::ValueNotFound(
                        DbKey::PendingOutboundTransaction(tx_id),
                    ))
                },
                Err(e) => return Err(e),
            }

            Ok(())
        })?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - complete_outbound_transaction: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn complete_inbound_transaction(
        &self,
        tx_id: TxId,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        if CompletedTransactionSql::find_by_cancelled(tx_id, false, &mut conn).is_ok() {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }

        let completed_tx_sql = CompletedTransactionSql::try_from(completed_transaction, &cipher)?;

        conn.transaction::<_, _, _>(|conn| {
            match InboundTransactionSql::complete_inbound_transaction(tx_id, conn) {
                Ok(_) => completed_tx_sql.commit(conn)?,
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                    return Err(TransactionStorageError::ValueNotFound(
                        DbKey::PendingInboundTransaction(tx_id),
                    ))
                },
                Err(e) => return Err(e),
            };

            Ok(())
        })?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - complete_inbound_transaction: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn broadcast_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, _, _>(|conn| {
            match CompletedTransactionSql::find_by_cancelled(tx_id, false, conn) {
                Ok(v) => {
                    // Note: This status test that does not error if the status do not match makes it inefficient
                    //       to combine the 'find' and 'update' queries.
                    if TransactionStatus::try_from(v.status)? == TransactionStatus::Completed {
                        v.update(
                            UpdateCompletedTransactionSql {
                                status: Some(TransactionStatus::Broadcast as i32),
                                ..Default::default()
                            },
                            conn,
                        )?;
                    }
                },
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                    return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                        tx_id,
                    )))
                },
                Err(e) => return Err(e),
            }

            Ok(())
        })?;

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - broadcast_completed_transaction: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn reject_completed_transaction(
        &self,
        tx_id: TxId,
        reason: TxCancellationReason,
    ) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        match CompletedTransactionSql::reject_completed_transaction(tx_id, reason, &mut conn) {
            Ok(_) => {},
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                    tx_id,
                )));
            },
            Err(e) => return Err(e),
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - reject_completed_transaction: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn set_pending_transaction_cancellation_status(
        &self,
        tx_id: TxId,
        cancelled: bool,
    ) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        match InboundTransactionSql::find_and_set_cancelled(tx_id, cancelled, &mut conn) {
            Ok(_) => {},
            Err(_) => {
                match OutboundTransactionSql::find_and_set_cancelled(tx_id, cancelled, &mut conn) {
                    Ok(_) => {},
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                        return Err(TransactionStorageError::ValuesNotFound);
                    },
                    Err(e) => return Err(e),
                };
            },
        }

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_pending_transaction_cancellation_status: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn mark_direct_send_success(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        match InboundTransactionSql::mark_direct_send_success(tx_id, &mut conn) {
            Ok(_) => {},
            Err(_) => {
                match OutboundTransactionSql::mark_direct_send_success(tx_id, &mut conn) {
                    Ok(_) => {},
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                        return Err(TransactionStorageError::ValuesNotFound);
                    },
                    Err(e) => return Err(e),
                };
            },
        };

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - mark_direct_send_success: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn cancel_coinbase_transactions_at_block_height(&self, block_height: u64) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        CompletedTransactionSql::reject_coinbases_at_block_height(
            block_height as i64,
            TxCancellationReason::AbandonedCoinbase,
            &mut conn,
        )?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - cancel_coinbase_transaction_at_block_height: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn find_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
        amount: MicroMinotari,
    ) -> Result<Option<CompletedTransaction>, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let coinbase_txs = CompletedTransactionSql::index_coinbase_at_block_height(block_height as i64, &mut conn)?;
        for c in coinbase_txs {
            let completed_tx = CompletedTransaction::try_from(c, &cipher)?;
            if completed_tx.amount == amount {
                return Ok(Some(completed_tx));
            }
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - find_coinbase_transaction_at_block_height: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(None)
    }

    fn increment_send_count(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        if CompletedTransactionSql::increment_send_count(tx_id, &mut conn).is_err() &&
            OutboundTransactionSql::increment_send_count(tx_id, &mut conn).is_err() &&
            InboundTransactionSql::increment_send_count(tx_id, &mut conn).is_err()
        {
            return Err(TransactionStorageError::ValuesNotFound);
        }

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - increment_send_count: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn update_mined_height(
        &self,
        tx_id: TxId,
        mined_height: u64,
        mined_in_block: BlockHash,
        mined_timestamp: u64,
        num_confirmations: u64,
        is_confirmed: bool,
        is_faux: bool,
    ) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let status = if is_confirmed {
            if is_faux {
                TransactionStatus::FauxConfirmed
            } else {
                TransactionStatus::MinedConfirmed
            }
        } else if is_faux {
            TransactionStatus::FauxUnconfirmed
        } else {
            TransactionStatus::MinedUnconfirmed
        };

        match CompletedTransactionSql::update_mined_height(
            tx_id,
            num_confirmations,
            status,
            mined_height,
            mined_in_block,
            mined_timestamp,
            &mut conn,
        ) {
            Ok(_) => {},
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                    tx_id,
                )));
            },
            Err(e) => return Err(e),
        }

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - update_mined_height: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn fetch_last_mined_transaction(&self) -> Result<Option<CompletedTransaction>, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let tx = completed_transactions::table
            // Note: Check 'mined_in_block' as well as 'mined_height' is populated for faux transactions before it is confirmed
            .filter(completed_transactions::mined_in_block.is_not_null())
            .filter(completed_transactions::mined_height.is_not_null())
            .filter(completed_transactions::mined_height.gt(0))
            .order_by(completed_transactions::mined_height.desc())
            .first::<CompletedTransactionSql>(&mut conn)
            .optional()?;
        let result = match tx {
            Some(tx) => Some(CompletedTransaction::try_from(tx, &cipher)?),
            None => None,
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch_last_mined_transaction: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(result)
    }

    // This method returns completed but unconfirmed transactions that were not imported
    fn fetch_unconfirmed_transactions_info(&self) -> Result<Vec<UnconfirmedTransactionInfo>, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut tx_info: Vec<UnconfirmedTransactionInfo> = vec![];
        match UnconfirmedTransactionInfoSql::fetch_unconfirmed_transactions_info(&mut conn) {
            Ok(info) => {
                for item in info {
                    tx_info.push(UnconfirmedTransactionInfo::try_from(item)?);
                }
            },
            Err(e) => return Err(e),
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch_unconfirmed_transactions_info: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(tx_info)
    }

    fn get_transactions_to_be_broadcast(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let txs = completed_transactions::table
            .filter(
                completed_transactions::status
                    .eq(TransactionStatus::Completed as i32)
                    .or(completed_transactions::status.eq(TransactionStatus::Broadcast as i32)),
            )
            .filter(
                completed_transactions::coinbase_block_height
                    .is_null()
                    .or(completed_transactions::coinbase_block_height.eq(0)),
            )
            .filter(completed_transactions::cancelled.is_null())
            .order_by(completed_transactions::tx_id)
            .load::<CompletedTransactionSql>(&mut conn)?;

        let mut result = vec![];
        for tx in txs {
            result.push(CompletedTransaction::try_from(tx, &cipher)?);
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - get_transactions_to_be_broadcast: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(result)
    }

    fn mark_all_transactions_as_unvalidated(&self) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let result = diesel::update(completed_transactions::table)
            .set((
                completed_transactions::cancelled.eq::<Option<i32>>(None),
                completed_transactions::mined_height.eq::<Option<i64>>(None),
                completed_transactions::mined_in_block.eq::<Option<Vec<u8>>>(None),
            ))
            .execute(&mut conn)?;

        trace!(target: LOG_TARGET, "rows updated: {:?}", result);
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_transactions_to_be_revalidated: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn set_transaction_as_unmined(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        match CompletedTransactionSql::set_as_unmined(tx_id, &mut conn) {
            Ok(_) => {},
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                    tx_id,
                )));
            },
            Err(e) => return Err(e),
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_transaction_as_unmined: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn get_pending_inbound_transaction_sender_info(
        &self,
    ) -> Result<Vec<InboundTransactionSenderInfo>, TransactionStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut sender_info: Vec<InboundTransactionSenderInfo> = vec![];
        match InboundTransactionSenderInfoSql::get_pending_inbound_transaction_sender_info(&mut conn) {
            Ok(info) => {
                for item in info {
                    sender_info.push(InboundTransactionSenderInfo::try_from(item)?);
                }
            },
            Err(e) => return Err(e),
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - get_pending_inbound_transaction_sender_info: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(sender_info)
    }

    fn fetch_imported_transactions(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;
        let cipher = acquire_read_lock!(self.cipher);

        CompletedTransactionSql::index_by_status_and_cancelled(TransactionStatus::Imported, false, &mut conn)?
            .into_iter()
            .map(|ct: CompletedTransactionSql| {
                CompletedTransaction::try_from(ct, &cipher).map_err(TransactionStorageError::from)
            })
            .collect::<Result<Vec<CompletedTransaction>, TransactionStorageError>>()
    }

    fn fetch_unconfirmed_faux_transactions(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;
        let cipher = acquire_read_lock!(self.cipher);

        CompletedTransactionSql::index_by_status_and_cancelled(TransactionStatus::FauxUnconfirmed, false, &mut conn)?
            .into_iter()
            .map(|ct: CompletedTransactionSql| {
                CompletedTransaction::try_from(ct, &cipher).map_err(TransactionStorageError::from)
            })
            .collect::<Result<Vec<CompletedTransaction>, TransactionStorageError>>()
    }

    fn fetch_confirmed_faux_transactions_from_height(
        &self,
        height: u64,
    ) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;
        let cipher = acquire_read_lock!(self.cipher);

        CompletedTransactionSql::index_by_status_and_cancelled_from_block_height(
            TransactionStatus::FauxConfirmed,
            false,
            height as i64,
            &mut conn,
        )?
        .into_iter()
        .map(|ct: CompletedTransactionSql| {
            CompletedTransaction::try_from(ct, &cipher).map_err(TransactionStorageError::from)
        })
        .collect::<Result<Vec<CompletedTransaction>, TransactionStorageError>>()
    }

    fn abandon_coinbase_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;
        match CompletedTransactionSql::find_and_abandon_coinbase(tx_id, &mut conn) {
            Ok(_) => {},
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                    tx_id,
                )));
            },
            Err(e) => return Err(e),
        };

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct InboundTransactionSenderInfo {
    pub(crate) tx_id: TxId,
    pub(crate) source_address: TariAddress,
}

impl TryFrom<InboundTransactionSenderInfoSql> for InboundTransactionSenderInfo {
    type Error = TransactionStorageError;

    fn try_from(i: InboundTransactionSenderInfoSql) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: TxId::from(i.tx_id as u64),
            source_address: TariAddress::from_bytes(&i.source_address)
                .map_err(TransactionStorageError::TariAddressError)?,
        })
    }
}

#[derive(Clone, Queryable)]
pub struct InboundTransactionSenderInfoSql {
    pub tx_id: i64,
    pub source_address: Vec<u8>,
}

impl InboundTransactionSenderInfoSql {
    pub fn get_pending_inbound_transaction_sender_info(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<InboundTransactionSenderInfoSql>, TransactionStorageError> {
        let query_result = inbound_transactions::table
            .select((inbound_transactions::tx_id, inbound_transactions::source_address))
            .filter(inbound_transactions::cancelled.eq(i32::from(false)))
            .load::<InboundTransactionSenderInfoSql>(conn)?;
        Ok(query_result)
    }
}

#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[diesel(table_name = inbound_transactions)]
struct InboundTransactionSql {
    tx_id: i64,
    source_address: Vec<u8>,
    amount: i64,
    receiver_protocol: String,
    message: String,
    timestamp: NaiveDateTime,
    cancelled: i32,
    direct_send_success: i32,
    send_count: i32,
    last_send_timestamp: Option<NaiveDateTime>,
}

impl InboundTransactionSql {
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::insert_into(inbound_transactions::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<InboundTransactionSql>, TransactionStorageError> {
        Ok(inbound_transactions::table.load::<InboundTransactionSql>(conn)?)
    }

    pub fn index_by_cancelled(
        conn: &mut SqliteConnection,
        cancelled: bool,
    ) -> Result<Vec<InboundTransactionSql>, TransactionStorageError> {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::cancelled.eq(i32::from(cancelled)))
            .load::<InboundTransactionSql>(conn)?)
    }

    pub fn find(tx_id: TxId, conn: &mut SqliteConnection) -> Result<InboundTransactionSql, TransactionStorageError> {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .first::<InboundTransactionSql>(conn)?)
    }

    pub fn find_by_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &mut SqliteConnection,
    ) -> Result<InboundTransactionSql, TransactionStorageError> {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .filter(inbound_transactions::cancelled.eq(i32::from(cancelled)))
            .first::<InboundTransactionSql>(conn)?)
    }

    pub fn mark_direct_send_success(tx_id: TxId, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::update(
            inbound_transactions::table
                .filter(inbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
                .filter(inbound_transactions::cancelled.eq(i32::from(false))),
        )
        .set(UpdateInboundTransactionSql {
            cancelled: None,
            direct_send_success: Some(1i32),
            receiver_protocol: None,
            send_count: None,
            last_send_timestamp: None,
        })
        .execute(conn)
        .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn complete_inbound_transaction(
        tx_id: TxId,
        conn: &mut SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        diesel::delete(
            inbound_transactions::table
                .filter(inbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
                .filter(inbound_transactions::cancelled.eq(i32::from(false))),
        )
        .execute(conn)
        .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn increment_send_count(tx_id: TxId, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::update(
            inbound_transactions::table
                .filter(inbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
                .filter(inbound_transactions::cancelled.eq(i32::from(false))),
        )
        .set(UpdateInboundTransactionSql {
            cancelled: None,
            direct_send_success: None,
            receiver_protocol: None,
            send_count: Some(
                if let Some(value) = inbound_transactions::table
                    .filter(inbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
                    .filter(inbound_transactions::cancelled.eq(i32::from(false)))
                    .select(inbound_transactions::send_count)
                    .load::<i32>(conn)?
                    .first()
                {
                    value + 1
                } else {
                    return Err(TransactionStorageError::DieselError(DieselError::NotFound));
                },
            ),
            last_send_timestamp: Some(Some(Utc::now().naive_utc())),
        })
        .execute(conn)
        .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn delete(&self, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        let num_deleted =
            diesel::delete(inbound_transactions::table.filter(inbound_transactions::tx_id.eq(&self.tx_id)))
                .execute(conn)?;

        if num_deleted == 0 {
            return Err(TransactionStorageError::ValuesNotFound);
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn update(
        &self,
        update: UpdateInboundTransactionSql,
        conn: &mut SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        let num_updated =
            diesel::update(inbound_transactions::table.filter(inbound_transactions::tx_id.eq(&self.tx_id)))
                .set(update)
                .execute(conn)?;

        if num_updated == 0 {
            return Err(TransactionStorageError::UnexpectedResult(
                "Updating inbound transactions failed. No rows were affected".to_string(),
            ));
        }

        Ok(())
    }

    pub fn find_and_set_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &mut SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        diesel::update(inbound_transactions::table.filter(inbound_transactions::tx_id.eq(tx_id.as_u64() as i64)))
            .set(UpdateInboundTransactionSql {
                cancelled: Some(i32::from(cancelled)),
                direct_send_success: None,
                receiver_protocol: None,
                send_count: None,
                last_send_timestamp: None,
            })
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    #[allow(dead_code)]
    pub fn update_encryption(&self, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateInboundTransactionSql {
                cancelled: None,
                direct_send_success: None,
                receiver_protocol: Some(self.receiver_protocol.clone()),
                send_count: None,
                last_send_timestamp: None,
            },
            conn,
        )
    }

    fn try_from(i: InboundTransaction, cipher: &XChaCha20Poly1305) -> Result<Self, TransactionStorageError> {
        let i = Self {
            tx_id: i.tx_id.as_u64() as i64,
            source_address: i.source_address.to_bytes().to_vec(),
            amount: u64::from(i.amount) as i64,
            receiver_protocol: serde_json::to_string(&i.receiver_protocol)?,
            message: i.message,
            timestamp: i.timestamp,
            cancelled: i32::from(i.cancelled),
            direct_send_success: i32::from(i.direct_send_success),
            send_count: i.send_count as i32,
            last_send_timestamp: i.last_send_timestamp,
        };
        i.encrypt(cipher).map_err(TransactionStorageError::AeadError)
    }
}

impl Encryptable<XChaCha20Poly1305> for InboundTransactionSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        [
            Self::INBOUND_TRANSACTION,
            self.tx_id.to_le_bytes().as_slice(),
            field_name.as_bytes(),
        ]
        .concat()
        .to_vec()
    }

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.receiver_protocol = encrypt_bytes_integral_nonce(
            cipher,
            self.domain("receiver_protocol"),
            Hidden::hide(self.receiver_protocol.as_bytes().to_vec()),
        )?
        .to_hex();

        Ok(self)
    }

    fn decrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        let mut decrypted_protocol = decrypt_bytes_integral_nonce(
            cipher,
            self.domain("receiver_protocol"),
            &from_hex(self.receiver_protocol.as_str()).map_err(|e| e.to_string())?,
        )?;

        self.receiver_protocol = from_utf8(decrypted_protocol.as_slice())
            .map_err(|e| e.to_string())?
            .to_string();

        // zeroize sensitive data
        decrypted_protocol.zeroize();

        Ok(self)
    }
}

impl InboundTransaction {
    fn try_from(i: InboundTransactionSql, cipher: &XChaCha20Poly1305) -> Result<Self, TransactionStorageError> {
        let i = i.decrypt(cipher).map_err(TransactionStorageError::AeadError)?;
        Ok(Self {
            tx_id: (i.tx_id as u64).into(),
            source_address: TariAddress::from_bytes(&i.source_address).map_err(TransactionKeyError::Source)?,
            amount: MicroMinotari::from(i.amount as u64),
            receiver_protocol: serde_json::from_str(&i.receiver_protocol.clone())?,
            status: TransactionStatus::Pending,
            message: i.message,
            timestamp: i.timestamp,
            cancelled: i.cancelled != 0,
            direct_send_success: i.direct_send_success != 0,
            send_count: i.send_count as u32,
            last_send_timestamp: i.last_send_timestamp,
        })
    }
}

#[derive(AsChangeset)]
#[diesel(table_name = inbound_transactions)]
pub struct UpdateInboundTransactionSql {
    cancelled: Option<i32>,
    direct_send_success: Option<i32>,
    receiver_protocol: Option<String>,
    send_count: Option<i32>,
    last_send_timestamp: Option<Option<NaiveDateTime>>,
}

/// A structure to represent a Sql compatible version of the OutboundTransaction struct
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[diesel(table_name = outbound_transactions)]
struct OutboundTransactionSql {
    tx_id: i64,
    destination_address: Vec<u8>,
    amount: i64,
    fee: i64,
    sender_protocol: String,
    message: String,
    timestamp: NaiveDateTime,
    cancelled: i32,
    direct_send_success: i32,
    send_count: i32,
    last_send_timestamp: Option<NaiveDateTime>,
}

impl OutboundTransactionSql {
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::insert_into(outbound_transactions::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<OutboundTransactionSql>, TransactionStorageError> {
        Ok(outbound_transactions::table.load::<OutboundTransactionSql>(conn)?)
    }

    pub fn index_by_cancelled(
        conn: &mut SqliteConnection,
        cancelled: bool,
    ) -> Result<Vec<OutboundTransactionSql>, TransactionStorageError> {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::cancelled.eq(i32::from(cancelled)))
            .load::<OutboundTransactionSql>(conn)?)
    }

    pub fn find(tx_id: TxId, conn: &mut SqliteConnection) -> Result<OutboundTransactionSql, TransactionStorageError> {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .first::<OutboundTransactionSql>(conn)?)
    }

    pub fn find_by_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &mut SqliteConnection,
    ) -> Result<OutboundTransactionSql, TransactionStorageError> {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .filter(outbound_transactions::cancelled.eq(i32::from(cancelled)))
            .first::<OutboundTransactionSql>(conn)?)
    }

    pub fn mark_direct_send_success(tx_id: TxId, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::update(
            outbound_transactions::table
                .filter(outbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
                .filter(outbound_transactions::cancelled.eq(i32::from(false))),
        )
        .set(UpdateOutboundTransactionSql {
            cancelled: None,
            direct_send_success: Some(1i32),
            sender_protocol: None,
            send_count: None,
            last_send_timestamp: None,
        })
        .execute(conn)
        .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn complete_outbound_transaction(
        tx_id: TxId,
        conn: &mut SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        diesel::delete(
            outbound_transactions::table
                .filter(outbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
                .filter(outbound_transactions::cancelled.eq(i32::from(false))),
        )
        .execute(conn)
        .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn increment_send_count(tx_id: TxId, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::update(outbound_transactions::table.filter(outbound_transactions::tx_id.eq(tx_id.as_u64() as i64)))
            .set(UpdateOutboundTransactionSql {
                cancelled: None,
                direct_send_success: None,
                sender_protocol: None,
                send_count: Some(
                    if let Some(value) = outbound_transactions::table
                        .filter(outbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
                        .select(outbound_transactions::send_count)
                        .load::<i32>(conn)?
                        .first()
                    {
                        value + 1
                    } else {
                        return Err(TransactionStorageError::DieselError(DieselError::NotFound));
                    },
                ),
                last_send_timestamp: Some(Some(Utc::now().naive_utc())),
            })
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn delete(&self, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::delete(outbound_transactions::table.filter(outbound_transactions::tx_id.eq(&self.tx_id)))
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn update(
        &self,
        update: UpdateOutboundTransactionSql,
        conn: &mut SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        diesel::update(outbound_transactions::table.filter(outbound_transactions::tx_id.eq(&self.tx_id)))
            .set(update)
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn find_and_set_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &mut SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        diesel::update(outbound_transactions::table.filter(outbound_transactions::tx_id.eq(tx_id.as_u64() as i64)))
            .set(UpdateOutboundTransactionSql {
                cancelled: Some(i32::from(cancelled)),
                direct_send_success: None,
                sender_protocol: None,
                send_count: None,
                last_send_timestamp: None,
            })
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    #[allow(dead_code)]
    pub fn update_encryption(&self, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateOutboundTransactionSql {
                cancelled: None,
                direct_send_success: None,
                sender_protocol: Some(self.sender_protocol.clone()),
                send_count: None,
                last_send_timestamp: None,
            },
            conn,
        )
    }

    fn try_from(o: OutboundTransaction, cipher: &XChaCha20Poly1305) -> Result<Self, TransactionStorageError> {
        let outbound_tx = Self {
            tx_id: o.tx_id.as_u64() as i64,
            destination_address: o.destination_address.to_bytes().to_vec(),
            amount: u64::from(o.amount) as i64,
            fee: u64::from(o.fee) as i64,
            sender_protocol: serde_json::to_string(&o.sender_protocol)?,
            message: o.message,
            timestamp: o.timestamp,
            cancelled: i32::from(o.cancelled),
            direct_send_success: i32::from(o.direct_send_success),
            send_count: o.send_count as i32,
            last_send_timestamp: o.last_send_timestamp,
        };

        outbound_tx.encrypt(cipher).map_err(TransactionStorageError::AeadError)
    }
}

impl Encryptable<XChaCha20Poly1305> for OutboundTransactionSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        [
            Self::OUTBOUND_TRANSACTION,
            self.tx_id.to_le_bytes().as_slice(),
            field_name.as_bytes(),
        ]
        .concat()
        .to_vec()
    }

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.sender_protocol = encrypt_bytes_integral_nonce(
            cipher,
            self.domain("sender_protocol"),
            Hidden::hide(self.sender_protocol.as_bytes().to_vec()),
        )?
        .to_hex();

        Ok(self)
    }

    fn decrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        let mut decrypted_protocol = decrypt_bytes_integral_nonce(
            cipher,
            self.domain("sender_protocol"),
            &from_hex(self.sender_protocol.as_str()).map_err(|e| e.to_string())?,
        )?;

        self.sender_protocol = from_utf8(decrypted_protocol.as_slice())
            .map_err(|e| e.to_string())?
            .to_string();

        // zeroize sensitive data
        decrypted_protocol.zeroize();

        Ok(self)
    }
}

impl OutboundTransaction {
    fn try_from(o: OutboundTransactionSql, cipher: &XChaCha20Poly1305) -> Result<Self, TransactionStorageError> {
        let mut o = o.decrypt(cipher).map_err(TransactionStorageError::AeadError)?;

        let outbound_tx = Self {
            tx_id: (o.tx_id as u64).into(),
            destination_address: TariAddress::from_bytes(&o.destination_address)
                .map_err(TransactionKeyError::Destination)?,
            amount: MicroMinotari::from(o.amount as u64),
            fee: MicroMinotari::from(o.fee as u64),
            sender_protocol: serde_json::from_str(&o.sender_protocol.clone())?,
            status: TransactionStatus::Pending,
            message: o.message,
            timestamp: o.timestamp,
            cancelled: o.cancelled != 0,
            direct_send_success: o.direct_send_success != 0,
            send_count: o.send_count as u32,
            last_send_timestamp: o.last_send_timestamp,
        };

        // zeroize decrypted data
        o.sender_protocol.zeroize();

        Ok(outbound_tx)
    }
}

#[derive(AsChangeset)]
#[diesel(table_name = outbound_transactions)]
pub struct UpdateOutboundTransactionSql {
    cancelled: Option<i32>,
    direct_send_success: Option<i32>,
    sender_protocol: Option<String>,
    send_count: Option<i32>,
    last_send_timestamp: Option<Option<NaiveDateTime>>,
}

/// A structure to represent a Sql compatible version of the CompletedTransaction struct
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[diesel(table_name = completed_transactions)]
pub struct CompletedTransactionSql {
    tx_id: i64,
    source_address: Vec<u8>,
    destination_address: Vec<u8>,
    amount: i64,
    fee: i64,
    transaction_protocol: String,
    status: i32,
    message: String,
    timestamp: NaiveDateTime,
    cancelled: Option<i32>,
    direction: Option<i32>,
    coinbase_block_height: Option<i64>,
    send_count: i32,
    last_send_timestamp: Option<NaiveDateTime>,
    confirmations: Option<i64>,
    mined_height: Option<i64>,
    mined_in_block: Option<Vec<u8>>,
    mined_timestamp: Option<NaiveDateTime>,
    transaction_signature_nonce: Vec<u8>,
    transaction_signature_key: Vec<u8>,
}

impl CompletedTransactionSql {
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::insert_into(completed_transactions::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError> {
        Ok(completed_transactions::table.load::<CompletedTransactionSql>(conn)?)
    }

    pub fn index_by_cancelled(
        conn: &mut SqliteConnection,
        cancelled: bool,
    ) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError> {
        let mut query = completed_transactions::table.into_boxed();

        query = if cancelled {
            query.filter(completed_transactions::cancelled.is_not_null())
        } else {
            query.filter(completed_transactions::cancelled.is_null())
        };

        Ok(query.load::<CompletedTransactionSql>(conn)?)
    }

    pub fn index_by_status_and_cancelled(
        status: TransactionStatus,
        cancelled: bool,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError> {
        let mut query = completed_transactions::table.into_boxed();
        query = if cancelled {
            query.filter(completed_transactions::cancelled.is_not_null())
        } else {
            query.filter(completed_transactions::cancelled.is_null())
        };
        Ok(query
            .filter(completed_transactions::status.eq(status as i32))
            .load::<CompletedTransactionSql>(conn)?)
    }

    pub fn index_by_status_and_cancelled_from_block_height(
        status: TransactionStatus,
        cancelled: bool,
        block_height: i64,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError> {
        let mut query = completed_transactions::table.into_boxed();
        query = if cancelled {
            query.filter(completed_transactions::cancelled.is_not_null())
        } else {
            query.filter(completed_transactions::cancelled.is_null())
        };

        Ok(query
            .filter(completed_transactions::status.eq(status as i32))
            .filter(completed_transactions::mined_height.ge(block_height))
            .load::<CompletedTransactionSql>(conn)?)
    }

    pub fn index_coinbase_at_block_height(
        block_height: i64,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError> {
        Ok(completed_transactions::table
            .filter(completed_transactions::status.eq(TransactionStatus::Coinbase as i32))
            .filter(completed_transactions::coinbase_block_height.eq(block_height))
            .load::<CompletedTransactionSql>(conn)?)
    }

    pub fn find_and_abandon_coinbase(tx_id: TxId, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        let _ = diesel::update(
            completed_transactions::table
                .filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64))
                .filter(completed_transactions::cancelled.is_null())
                .filter(completed_transactions::coinbase_block_height.is_not_null()),
        )
        .set(UpdateCompletedTransactionSql {
            cancelled: Some(Some(TxCancellationReason::AbandonedCoinbase as i32)),
            ..Default::default()
        })
        .execute(conn)
        .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn find(tx_id: TxId, conn: &mut SqliteConnection) -> Result<CompletedTransactionSql, TransactionStorageError> {
        Ok(completed_transactions::table
            .filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .first::<CompletedTransactionSql>(conn)?)
    }

    pub fn find_by_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &mut SqliteConnection,
    ) -> Result<CompletedTransactionSql, TransactionStorageError> {
        let mut query = completed_transactions::table
            .filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .into_boxed();

        query = if cancelled {
            query.filter(completed_transactions::cancelled.is_not_null())
        } else {
            query.filter(completed_transactions::cancelled.is_null())
        };

        Ok(query.first::<CompletedTransactionSql>(conn)?)
    }

    pub fn reject_completed_transaction(
        tx_id: TxId,
        reason: TxCancellationReason,
        conn: &mut SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        diesel::update(
            completed_transactions::table
                .filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64))
                .filter(completed_transactions::cancelled.is_null()),
        )
        .set(UpdateCompletedTransactionSql {
            cancelled: Some(Some(reason as i32)),
            status: Some(TransactionStatus::Rejected as i32),
            ..Default::default()
        })
        .execute(conn)
        .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn increment_send_count(tx_id: TxId, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        // This query uses a sub-query to retrieve an existing value in the table
        diesel::update(completed_transactions::table.filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64)))
            .set(UpdateCompletedTransactionSql {
                send_count: Some(
                    if let Some(value) = completed_transactions::table
                        .filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64))
                        .select(completed_transactions::send_count)
                        .load::<i32>(conn)?
                        .first()
                    {
                        value + 1
                    } else {
                        return Err(TransactionStorageError::DieselError(DieselError::NotFound));
                    },
                ),
                last_send_timestamp: Some(Some(Utc::now().naive_utc())),
                ..Default::default()
            })
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn reject_coinbases_at_block_height(
        block_height: i64,
        reason: TxCancellationReason,
        conn: &mut SqliteConnection,
    ) -> Result<usize, TransactionStorageError> {
        Ok(diesel::update(
            completed_transactions::table
                .filter(completed_transactions::status.eq(TransactionStatus::Coinbase as i32))
                .filter(completed_transactions::coinbase_block_height.eq(block_height)),
        )
        .set(UpdateCompletedTransactionSql {
            cancelled: Some(Some(reason as i32)),
            status: Some(TransactionStatus::Rejected as i32),
            ..Default::default()
        })
        .execute(conn)?)
    }

    pub fn delete(&self, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        let num_deleted =
            diesel::delete(completed_transactions::table.filter(completed_transactions::tx_id.eq(&self.tx_id)))
                .execute(conn)?;

        if num_deleted == 0 {
            return Err(TransactionStorageError::ValuesNotFound);
        }

        Ok(())
    }

    pub fn update(
        &self,
        updated_tx: UpdateCompletedTransactionSql,
        conn: &mut SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        diesel::update(completed_transactions::table.filter(completed_transactions::tx_id.eq(&self.tx_id)))
            .set(updated_tx)
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;
        Ok(())
    }

    pub fn update_mined_height(
        tx_id: TxId,
        num_confirmations: u64,
        status: TransactionStatus,
        mined_height: u64,
        mined_in_block: BlockHash,
        mined_timestamp: u64,
        conn: &mut SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        let timestamp = NaiveDateTime::from_timestamp_opt(mined_timestamp as i64, 0).ok_or_else(|| {
            TransactionStorageError::UnexpectedResult(format!(
                "Could not create timestamp mined_timestamp: {}",
                mined_timestamp
            ))
        })?;
        diesel::update(completed_transactions::table.filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64)))
            .set(UpdateCompletedTransactionSql {
                confirmations: Some(Some(num_confirmations as i64)),
                status: Some(status as i32),
                mined_height: Some(Some(mined_height as i64)),
                mined_in_block: Some(Some(mined_in_block.to_vec())),
                mined_timestamp: Some(timestamp),
                // If the tx is mined, then it can't be cancelled
                cancelled: None,
                ..Default::default()
            })
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn set_as_unmined(tx_id: TxId, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        // This query uses two sub-queries to retrieve existing values in the table
        diesel::update(completed_transactions::table.filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64)))
            .set(UpdateCompletedTransactionSql {
                status: {
                    if let Some(Some(_coinbase_block_height)) = completed_transactions::table
                        .filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64))
                        .select(completed_transactions::coinbase_block_height)
                        .load::<Option<i64>>(conn)?
                        .first()
                    {
                        Some(TransactionStatus::Coinbase as i32)
                    } else if let Some(status) = completed_transactions::table
                        .filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64))
                        .select(completed_transactions::status)
                        .load::<i32>(conn)?
                        .first()
                    {
                        if *status == TransactionStatus::FauxConfirmed as i32 {
                            Some(TransactionStatus::FauxUnconfirmed as i32)
                        } else if *status == TransactionStatus::Broadcast as i32 {
                            Some(TransactionStatus::Broadcast as i32)
                        } else {
                            Some(TransactionStatus::Completed as i32)
                        }
                    } else {
                        return Err(TransactionStorageError::DieselError(DieselError::NotFound));
                    }
                },
                mined_in_block: Some(None),
                mined_height: Some(None),
                confirmations: Some(None),
                // Turns out it should not be cancelled
                cancelled: Some(None),
                ..Default::default()
            })
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        // Ideally the outputs should be marked unmined here as well, but because of the separation of classes,
        // that will be done in the outputs service.

        Ok(())
    }

    #[allow(dead_code)]
    pub fn update_encryption(&self, conn: &mut SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateCompletedTransactionSql {
                transaction_protocol: Some(self.transaction_protocol.clone()),
                ..Default::default()
            },
            conn,
        )?;

        Ok(())
    }

    fn try_from(c: CompletedTransaction, cipher: &XChaCha20Poly1305) -> Result<Self, TransactionStorageError> {
        let output = Self {
            tx_id: c.tx_id.as_u64() as i64,
            source_address: c.source_address.to_bytes().to_vec(),
            destination_address: c.destination_address.to_bytes().to_vec(),
            amount: u64::from(c.amount) as i64,
            fee: u64::from(c.fee) as i64,
            transaction_protocol: serde_json::to_string(&c.transaction)?,
            status: c.status as i32,
            message: c.message,
            timestamp: c.timestamp,
            cancelled: c.cancelled.map(|v| v as i32),
            direction: Some(c.direction as i32),
            coinbase_block_height: c.coinbase_block_height.map(|b| b as i64),
            send_count: c.send_count as i32,
            last_send_timestamp: c.last_send_timestamp,
            confirmations: c.confirmations.map(|ic| ic as i64),
            mined_height: c.mined_height.map(|ic| ic as i64),
            mined_in_block: c.mined_in_block.map(|v| v.to_vec()),
            mined_timestamp: c.mined_timestamp,
            transaction_signature_nonce: c.transaction_signature.get_public_nonce().to_vec(),
            transaction_signature_key: c.transaction_signature.get_signature().to_vec(),
        };

        output.encrypt(cipher).map_err(TransactionStorageError::AeadError)
    }
}

impl Encryptable<XChaCha20Poly1305> for CompletedTransactionSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        [
            Self::COMPLETED_TRANSACTION,
            self.tx_id.to_le_bytes().as_slice(),
            field_name.as_bytes(),
        ]
        .concat()
        .to_vec()
    }

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.transaction_protocol = encrypt_bytes_integral_nonce(
            cipher,
            self.domain("transaction_protocol"),
            Hidden::hide(self.transaction_protocol.as_bytes().to_vec()),
        )?
        .to_hex();

        Ok(self)
    }

    fn decrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        let mut decrypted_protocol = decrypt_bytes_integral_nonce(
            cipher,
            self.domain("transaction_protocol"),
            &from_hex(self.transaction_protocol.as_str()).map_err(|e| e.to_string())?,
        )?;

        self.transaction_protocol = from_utf8(decrypted_protocol.as_slice())
            .map_err(|e| e.to_string())?
            .to_string();

        // zeroize sensitive data
        decrypted_protocol.zeroize();

        Ok(self)
    }
}

#[derive(Debug, Error)]
pub enum CompletedTransactionConversionError {
    #[error("CompletedTransaction conversion failed by wrong direction: {0}")]
    DirectionError(#[from] TransactionDirectionError),
    #[error("CompletedTransaction conversion failed with transaction conversion: {0}")]
    ConversionError(#[from] TransactionConversionError),
    #[error("CompletedTransaction conversion failed with json error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("CompletedTransaction conversion failed with key error: {0}")]
    KeyError(#[from] TransactionKeyError),
    #[error("Aead Error: {0}")]
    AeadError(String),
}

impl CompletedTransaction {
    fn try_from(
        c: CompletedTransactionSql,
        cipher: &XChaCha20Poly1305,
    ) -> Result<Self, CompletedTransactionConversionError> {
        let mut c = c
            .decrypt(cipher)
            .map_err(CompletedTransactionConversionError::AeadError)?;
        let transaction_signature = match PublicKey::from_vec(&c.transaction_signature_nonce) {
            Ok(public_nonce) => match PrivateKey::from_vec(&c.transaction_signature_key) {
                Ok(signature) => Signature::new(public_nonce, signature),
                Err(_) => Signature::default(),
            },
            Err(_) => Signature::default(),
        };
        let mined_in_block = match c.mined_in_block {
            Some(v) => match v.try_into() {
                Ok(v) => Some(v),
                Err(_) => None,
            },
            None => None,
        };

        let output = Self {
            tx_id: (c.tx_id as u64).into(),
            source_address: TariAddress::from_bytes(&c.source_address).map_err(TransactionKeyError::Source)?,
            destination_address: TariAddress::from_bytes(&c.destination_address)
                .map_err(TransactionKeyError::Destination)?,
            amount: MicroMinotari::from(c.amount as u64),
            fee: MicroMinotari::from(c.fee as u64),
            transaction: serde_json::from_str(&c.transaction_protocol.clone())?,
            status: TransactionStatus::try_from(c.status)?,
            message: c.message,
            timestamp: c.timestamp,
            cancelled: c
                .cancelled
                .map(|v| TxCancellationReason::try_from(v as u32).unwrap_or(TxCancellationReason::Unknown)),
            direction: TransactionDirection::try_from(c.direction.unwrap_or(2i32))?,
            coinbase_block_height: c.coinbase_block_height.map(|b| b as u64),
            send_count: c.send_count as u32,
            last_send_timestamp: c.last_send_timestamp,
            transaction_signature,
            confirmations: c.confirmations.map(|ic| ic as u64),
            mined_height: c.mined_height.map(|ic| ic as u64),
            mined_in_block,
            mined_timestamp: c.mined_timestamp,
        };

        // zeroize sensitive data
        c.transaction_protocol.zeroize();

        Ok(output)
    }
}

#[derive(AsChangeset, Default)]
#[diesel(table_name = completed_transactions)]
pub struct UpdateCompletedTransactionSql {
    status: Option<i32>,
    timestamp: Option<NaiveDateTime>,
    cancelled: Option<Option<i32>>,
    direction: Option<i32>,
    transaction_protocol: Option<String>,
    send_count: Option<i32>,
    last_send_timestamp: Option<Option<NaiveDateTime>>,
    confirmations: Option<Option<i64>>,
    mined_height: Option<Option<i64>>,
    mined_in_block: Option<Option<Vec<u8>>>,
    mined_timestamp: Option<NaiveDateTime>,
    transaction_signature_nonce: Option<Vec<u8>>,
    transaction_signature_key: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnconfirmedTransactionInfo {
    pub tx_id: TxId,
    pub signature: Signature,
    pub status: TransactionStatus,
    pub coinbase_block_height: Option<u64>,
}

impl UnconfirmedTransactionInfo {
    pub fn is_coinbase(&self) -> bool {
        if let Some(height) = self.coinbase_block_height {
            height > 0
        } else {
            false
        }
    }
}

impl TryFrom<UnconfirmedTransactionInfoSql> for UnconfirmedTransactionInfo {
    type Error = TransactionStorageError;

    fn try_from(i: UnconfirmedTransactionInfoSql) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: (i.tx_id as u64).into(),
            signature: Signature::new(
                PublicKey::from_vec(&i.transaction_signature_nonce)?,
                PrivateKey::from_vec(&i.transaction_signature_key)?,
            ),
            status: TransactionStatus::try_from(i.status)?,
            coinbase_block_height: i.coinbase_block_height.map(|b| b as u64),
        })
    }
}

#[derive(Clone, Queryable)]
pub struct UnconfirmedTransactionInfoSql {
    pub tx_id: i64,
    pub status: i32,
    pub transaction_signature_nonce: Vec<u8>,
    pub transaction_signature_key: Vec<u8>,
    pub coinbase_block_height: Option<i64>,
}

impl UnconfirmedTransactionInfoSql {
    /// This method returns completed but unconfirmed transactions that were not imported or scanned
    pub fn fetch_unconfirmed_transactions_info(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<UnconfirmedTransactionInfoSql>, TransactionStorageError> {
        let query_result = completed_transactions::table
            .select((
                completed_transactions::tx_id,
                completed_transactions::status,
                completed_transactions::transaction_signature_nonce,
                completed_transactions::transaction_signature_key,
                completed_transactions::coinbase_block_height,
            ))
            .filter(
                completed_transactions::status
                    .ne(TransactionStatus::Imported as i32)
                    .and(completed_transactions::status.ne(TransactionStatus::FauxUnconfirmed as i32))
                    .and(completed_transactions::status.ne(TransactionStatus::FauxConfirmed as i32))
                    .and(
                        completed_transactions::mined_height
                            .is_null()
                            .or(completed_transactions::status.eq(TransactionStatus::MinedUnconfirmed as i32)),
                    ),
            )
            .filter(completed_transactions::cancelled.is_null())
            .order_by(completed_transactions::tx_id)
            .load::<UnconfirmedTransactionInfoSql>(conn)?;
        Ok(query_result)
    }
}

#[cfg(test)]
mod test {
    use std::{default::Default, mem::size_of, time::Duration};

    use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
    use chrono::Utc;
    use diesel::{sql_query, Connection, RunQueryDsl, SqliteConnection};
    use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
    use rand::{rngs::OsRng, RngCore};
    use tari_common::configuration::Network;
    use tari_common_sqlite::sqlite_connection_pool::SqliteConnectionPool;
    use tari_common_types::{
        encryption::Encryptable,
        tari_address::TariAddress,
        transaction::{TransactionDirection, TransactionStatus, TxId},
        types::{PrivateKey, PublicKey, Signature},
    };
    use tari_core::transactions::{
        tari_amount::MicroMinotari,
        test_helpers::{create_test_core_key_manager_with_memory_db, create_wallet_output_with_data, TestParams},
        transaction_components::{OutputFeatures, Transaction},
        transaction_protocol::sender::TransactionSenderMessage,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    };
    use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait};
    use tari_script::{inputs, script};
    use tari_test_utils::random::string;
    use tempfile::tempdir;

    use crate::{
        storage::sqlite_utilities::wallet_db_connection::WalletDbConnection,
        test_utils::create_consensus_constants,
        transaction_service::storage::{
            database::{DbKey, TransactionBackend},
            models::{CompletedTransaction, InboundTransaction, OutboundTransaction, TxCancellationReason},
            sqlite_db::{
                CompletedTransactionSql,
                InboundTransactionSenderInfo,
                InboundTransactionSql,
                OutboundTransactionSql,
                TransactionServiceSqliteDatabase,
                UpdateCompletedTransactionSql,
            },
        },
    };

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn test_crud() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let consensus_constants = create_consensus_constants(0);
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

        let mut conn =
            SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let cipher = XChaCha20Poly1305::new(key_ga);

        conn.run_pending_migrations(MIGRATIONS)
            .map(|v| {
                v.into_iter()
                    .map(|b| {
                        let m = format!("Running migration {}", b);
                        // std::io::stdout()
                        //     .write_all(m.as_ref())
                        //     .expect("Couldn't write migration number to stdout");
                        m
                    })
                    .collect::<Vec<String>>()
            })
            .expect("Migrations failed");

        sql_query("PRAGMA foreign_keys = ON").execute(&mut conn).unwrap();

        let constants = create_consensus_constants(0);
        let mut builder = SenderTransactionProtocol::builder(constants, key_manager.clone());
        let test_params = TestParams::new(&key_manager).await;
        let input = create_wallet_output_with_data(
            script!(Nop),
            OutputFeatures::default(),
            &test_params,
            MicroMinotari::from(100_000),
            &key_manager,
        )
        .await
        .unwrap();
        let amount = MicroMinotari::from(10_000);
        let change = TestParams::new(&key_manager).await;
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari::from(177 / 5))
            .with_message("Yo!".to_string())
            .with_input(input)
            .await
            .unwrap()
            .with_recipient_data(
                script!(Nop),
                OutputFeatures::default(),
                Default::default(),
                MicroMinotari::zero(),
                amount,
            )
            .await
            .unwrap()
            .with_change_data(
                script!(Nop),
                inputs!(change.script_key_pk),
                change.script_key_id,
                change.spend_key_id,
                Default::default(),
            );
        let mut stp = builder.build().await.unwrap();

        let address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let outbound_tx1 = OutboundTransaction {
            tx_id: 1u64.into(),
            destination_address: address,
            amount,
            fee: stp.get_fee_amount().unwrap(),
            sender_protocol: stp.clone(),
            status: TransactionStatus::Pending,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        };
        let address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let outbound_tx2 = OutboundTransactionSql::try_from(
            OutboundTransaction {
                tx_id: 2u64.into(),
                destination_address: address,
                amount,
                fee: stp.get_fee_amount().unwrap(),
                sender_protocol: stp.clone(),
                status: TransactionStatus::Pending,
                message: "Hey!".to_string(),
                timestamp: Utc::now().naive_utc(),
                cancelled: false,
                direct_send_success: false,
                send_count: 0,
                last_send_timestamp: None,
            },
            &cipher,
        )
        .unwrap();

        OutboundTransactionSql::try_from(outbound_tx1.clone(), &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();
        outbound_tx2.commit(&mut conn).unwrap();

        let outbound_txs = OutboundTransactionSql::index_by_cancelled(&mut conn, false).unwrap();
        assert_eq!(outbound_txs.len(), 2);

        let returned_outbound_tx = OutboundTransaction::try_from(
            OutboundTransactionSql::find_by_cancelled(1u64.into(), false, &mut conn).unwrap(),
            &cipher,
        )
        .unwrap();

        assert_eq!(returned_outbound_tx, outbound_tx1);
        assert_eq!(
            OutboundTransactionSql::try_from(returned_outbound_tx, &cipher)
                .unwrap()
                .decrypt(&cipher)
                .unwrap(),
            OutboundTransactionSql::try_from(outbound_tx1.clone(), &cipher)
                .unwrap()
                .decrypt(&cipher)
                .unwrap()
        );

        let output = create_wallet_output_with_data(
            script!(Nop),
            OutputFeatures::default(),
            &test_params,
            MicroMinotari::from(100_000),
            &key_manager,
        )
        .await
        .unwrap();

        let rtp = ReceiverTransactionProtocol::new(
            TransactionSenderMessage::Single(Box::new(stp.build_single_round_message(&key_manager).await.unwrap())),
            output,
            &key_manager,
            &consensus_constants,
        )
        .await;
        let address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let inbound_tx1 = InboundTransaction {
            tx_id: 2u64.into(),
            source_address: address,
            amount,
            receiver_protocol: rtp.clone(),
            status: TransactionStatus::Pending,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        };
        let address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let inbound_tx2 = InboundTransaction {
            tx_id: 3u64.into(),
            source_address: address,
            amount,
            receiver_protocol: rtp,
            status: TransactionStatus::Pending,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        };

        InboundTransactionSql::try_from(inbound_tx1.clone(), &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();
        InboundTransactionSql::try_from(inbound_tx2, &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();

        let inbound_txs = InboundTransactionSql::index_by_cancelled(&mut conn, false).unwrap();
        assert_eq!(inbound_txs.len(), 2);

        let returned_inbound_tx = InboundTransaction::try_from(
            InboundTransactionSql::find_by_cancelled(2u64.into(), false, &mut conn).unwrap(),
            &cipher,
        )
        .unwrap();
        assert_eq!(
            InboundTransactionSql::try_from(returned_inbound_tx, &cipher)
                .unwrap()
                .decrypt(&cipher)
                .unwrap(),
            InboundTransactionSql::try_from(inbound_tx1.clone(), &cipher)
                .unwrap()
                .decrypt(&cipher)
                .unwrap()
        );

        let tx = Transaction::new(
            vec![],
            vec![],
            vec![],
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
        );
        let source_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let destination_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let completed_tx1 = CompletedTransaction {
            tx_id: 2u64.into(),
            source_address,
            destination_address,
            amount,
            fee: MicroMinotari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::MinedUnconfirmed,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: None,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
        };
        let source_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let destination_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let completed_tx2 = CompletedTransaction {
            tx_id: 3u64.into(),
            source_address,
            destination_address,
            amount,
            fee: MicroMinotari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::Broadcast,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: None,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
        };

        CompletedTransactionSql::try_from(completed_tx1.clone(), &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();
        assert!(CompletedTransactionSql::try_from(completed_tx1.clone(), &cipher)
            .unwrap()
            .commit(&mut conn)
            .is_err());

        CompletedTransactionSql::try_from(completed_tx2, &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();

        let completed_txs = CompletedTransactionSql::index_by_cancelled(&mut conn, false).unwrap();
        assert_eq!(completed_txs.len(), 2);

        let returned_completed_tx = CompletedTransaction::try_from(
            CompletedTransactionSql::find_by_cancelled(2u64.into(), false, &mut conn).unwrap(),
            &cipher,
        )
        .unwrap();
        assert_eq!(
            CompletedTransactionSql::try_from(returned_completed_tx, &cipher)
                .unwrap()
                .decrypt(&cipher)
                .unwrap(),
            CompletedTransactionSql::try_from(completed_tx1.clone(), &cipher)
                .unwrap()
                .decrypt(&cipher)
                .unwrap()
        );

        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &mut conn).is_ok());
        InboundTransactionSql::try_from(inbound_tx1.clone(), &cipher)
            .unwrap()
            .delete(&mut conn)
            .unwrap();
        assert!(InboundTransactionSql::try_from(inbound_tx1.clone(), &cipher)
            .unwrap()
            .delete(&mut conn)
            .is_err());
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &mut conn).is_err());

        assert!(OutboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &mut conn).is_ok());
        OutboundTransactionSql::try_from(outbound_tx1.clone(), &cipher)
            .unwrap()
            .delete(&mut conn)
            .unwrap();
        assert!(OutboundTransactionSql::try_from(outbound_tx1.clone(), &cipher)
            .unwrap()
            .delete(&mut conn)
            .is_err());
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, false, &mut conn).is_err());

        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, false, &mut conn).is_ok());
        CompletedTransactionSql::try_from(completed_tx1.clone(), &cipher)
            .unwrap()
            .delete(&mut conn)
            .unwrap();
        assert!(CompletedTransactionSql::try_from(completed_tx1.clone(), &cipher)
            .unwrap()
            .delete(&mut conn)
            .is_err());
        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, false, &mut conn).is_err());

        InboundTransactionSql::try_from(inbound_tx1.clone(), &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();

        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, true, &mut conn).is_err());
        InboundTransactionSql::find_and_set_cancelled(inbound_tx1.tx_id, true, &mut conn).unwrap();
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &mut conn).is_err());
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, true, &mut conn).is_ok());
        InboundTransactionSql::find_and_set_cancelled(inbound_tx1.tx_id, false, &mut conn).unwrap();
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, true, &mut conn).is_err());
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &mut conn).is_ok());
        OutboundTransactionSql::try_from(outbound_tx1.clone(), &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();

        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, true, &mut conn).is_err());
        OutboundTransactionSql::find_and_set_cancelled(outbound_tx1.tx_id, true, &mut conn).unwrap();
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, false, &mut conn).is_err());
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, true, &mut conn).is_ok());
        OutboundTransactionSql::find_and_set_cancelled(outbound_tx1.tx_id, false, &mut conn).unwrap();
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, true, &mut conn).is_err());
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, false, &mut conn).is_ok());

        CompletedTransactionSql::try_from(completed_tx1.clone(), &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();

        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, true, &mut conn).is_err());
        CompletedTransactionSql::try_from(completed_tx1.clone(), &cipher)
            .unwrap()
            .update(
                UpdateCompletedTransactionSql {
                    cancelled: Some(Some(TxCancellationReason::Unknown as i32)),
                    status: Some(TransactionStatus::Rejected as i32),
                    ..Default::default()
                },
                &mut conn,
            )
            .unwrap();
        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, false, &mut conn).is_err());
        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, true, &mut conn).is_ok());

        let source_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let destination_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let coinbase_tx1 = CompletedTransaction {
            tx_id: 101u64.into(),
            source_address,
            destination_address,
            amount,
            fee: MicroMinotari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::Coinbase,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: None,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: Some(2),
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
        };

        let source_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let destination_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let coinbase_tx2 = CompletedTransaction {
            tx_id: 102u64.into(),
            source_address,
            destination_address,
            amount,
            fee: MicroMinotari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::Coinbase,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: None,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: Some(2),
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
        };

        let source_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let destination_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let coinbase_tx3 = CompletedTransaction {
            tx_id: 103u64.into(),
            source_address,
            destination_address,
            amount,
            fee: MicroMinotari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::Coinbase,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: None,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: Some(3),
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
        };

        CompletedTransactionSql::try_from(coinbase_tx1, &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();
        CompletedTransactionSql::try_from(coinbase_tx2, &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();
        CompletedTransactionSql::try_from(coinbase_tx3, &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();

        let coinbase_txs = CompletedTransactionSql::index_coinbase_at_block_height(2, &mut conn).unwrap();

        assert_eq!(coinbase_txs.len(), 2);
        assert!(coinbase_txs.iter().any(|c| c.tx_id == 101));
        assert!(coinbase_txs.iter().any(|c| c.tx_id == 102));
        assert!(!coinbase_txs.iter().any(|c| c.tx_id == 103));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_encryption_crud() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

        let mut conn =
            SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        conn.run_pending_migrations(MIGRATIONS)
            .map(|v| {
                v.into_iter()
                    .map(|b| {
                        let m = format!("Running migration {}", b);
                        // std::io::stdout()
                        //     .write_all(m.as_ref())
                        //     .expect("Couldn't write migration number to stdout");
                        m
                    })
                    .collect::<Vec<String>>()
            })
            .expect("Migrations failed");

        sql_query("PRAGMA foreign_keys = ON").execute(&mut conn).unwrap();

        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let cipher = XChaCha20Poly1305::new(key_ga);

        let source_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let inbound_tx = InboundTransaction {
            tx_id: 1u64.into(),
            source_address,
            amount: MicroMinotari::from(100),
            receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
            status: TransactionStatus::Pending,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        };
        let inbound_tx_sql = InboundTransactionSql::try_from(inbound_tx.clone(), &cipher).unwrap();
        inbound_tx_sql.commit(&mut conn).unwrap();
        let inbound_tx_sql = inbound_tx_sql.encrypt(&cipher).unwrap();
        inbound_tx_sql.update_encryption(&mut conn).unwrap();
        let db_inbound_tx = InboundTransactionSql::find_by_cancelled(1u64.into(), false, &mut conn).unwrap();
        let db_inbound_tx = db_inbound_tx.decrypt(&cipher).unwrap();
        let decrypted_inbound_tx = InboundTransaction::try_from(db_inbound_tx, &cipher).unwrap();
        assert_eq!(inbound_tx, decrypted_inbound_tx);

        let destination_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let outbound_tx = OutboundTransaction {
            tx_id: 2u64.into(),
            destination_address,
            amount: MicroMinotari::from(100),
            fee: MicroMinotari::from(10),
            sender_protocol: SenderTransactionProtocol::new_placeholder(),
            status: TransactionStatus::Pending,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        };

        let outbound_tx_sql = OutboundTransactionSql::try_from(outbound_tx.clone(), &cipher).unwrap();
        outbound_tx_sql.commit(&mut conn).unwrap();
        let outbound_tx_sql = outbound_tx_sql.encrypt(&cipher).unwrap();
        outbound_tx_sql.update_encryption(&mut conn).unwrap();
        let db_outbound_tx = OutboundTransactionSql::find_by_cancelled(2u64.into(), false, &mut conn).unwrap();
        let db_outbound_tx = db_outbound_tx.decrypt(&cipher).unwrap();
        let decrypted_outbound_tx = OutboundTransaction::try_from(db_outbound_tx, &cipher).unwrap();
        assert_eq!(outbound_tx, decrypted_outbound_tx);

        let source_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let destination_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let completed_tx = CompletedTransaction {
            tx_id: 3u64.into(),
            source_address,
            destination_address,
            amount: MicroMinotari::from(100),
            fee: MicroMinotari::from(100),
            transaction: Transaction::new(
                vec![],
                vec![],
                vec![],
                PrivateKey::random(&mut OsRng),
                PrivateKey::random(&mut OsRng),
            ),
            status: TransactionStatus::MinedUnconfirmed,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: None,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: Signature::default(),
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
        };

        let completed_tx_sql = CompletedTransactionSql::try_from(completed_tx.clone(), &cipher).unwrap();
        completed_tx_sql.commit(&mut conn).unwrap();
        let completed_tx_sql = completed_tx_sql.encrypt(&cipher).unwrap();
        completed_tx_sql.update_encryption(&mut conn).unwrap();
        let db_completed_tx = CompletedTransactionSql::find_by_cancelled(3u64.into(), false, &mut conn).unwrap();
        let db_completed_tx = db_completed_tx.decrypt(&cipher).unwrap();
        let decrypted_completed_tx = CompletedTransaction::try_from(db_completed_tx, &cipher).unwrap();
        assert_eq!(completed_tx, decrypted_completed_tx);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_transaction_db_values_must_be_encrypted() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

        let mut pool = SqliteConnectionPool::new(db_path.clone(), 1, true, true, Duration::from_secs(60));
        pool.create_pool()
            .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let cipher = XChaCha20Poly1305::new(key_ga);

        // Note: For this test the connection pool is setup with a pool size of one; the pooled connection must go out
        // of scope to be released once obtained otherwise subsequent calls to obtain a pooled connection will fail .
        {
            let mut conn = pool
                .get_pooled_connection()
                .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

            conn.run_pending_migrations(MIGRATIONS)
                .map(|v| {
                    v.into_iter()
                        .map(|b| {
                            let m = format!("Running migration {}", b);
                            // std::io::stdout()
                            //     .write_all(m.as_ref())
                            //     .expect("Couldn't write migration number to stdout");
                            m
                        })
                        .collect::<Vec<String>>()
                })
                .expect("Migrations failed");

            let source_address = TariAddress::new(
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                Network::LocalNet,
            );
            let inbound_tx = InboundTransaction {
                tx_id: 1u64.into(),
                source_address,
                amount: MicroMinotari::from(100),
                receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
                status: TransactionStatus::Pending,
                message: "Yo!".to_string(),
                timestamp: Utc::now().naive_utc(),
                cancelled: false,
                direct_send_success: false,
                send_count: 0,
                last_send_timestamp: None,
            };
            let inbound_tx_sql = InboundTransactionSql::try_from(inbound_tx, &cipher).unwrap();

            inbound_tx_sql.commit(&mut conn).unwrap();

            let destination_address = TariAddress::new(
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                Network::LocalNet,
            );
            let outbound_tx = OutboundTransaction {
                tx_id: 2u64.into(),
                destination_address,
                amount: MicroMinotari::from(100),
                fee: MicroMinotari::from(10),
                sender_protocol: SenderTransactionProtocol::new_placeholder(),
                status: TransactionStatus::Pending,
                message: "Yo!".to_string(),
                timestamp: Utc::now().naive_utc(),
                cancelled: false,
                direct_send_success: false,
                send_count: 0,
                last_send_timestamp: None,
            };
            let outbound_tx_sql = OutboundTransactionSql::try_from(outbound_tx, &cipher).unwrap();

            outbound_tx_sql.commit(&mut conn).unwrap();

            let source_address = TariAddress::new(
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                Network::LocalNet,
            );
            let destination_address = TariAddress::new(
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                Network::LocalNet,
            );
            let completed_tx = CompletedTransaction {
                tx_id: 3u64.into(),
                source_address,
                destination_address,
                amount: MicroMinotari::from(100),
                fee: MicroMinotari::from(100),
                transaction: Transaction::new(
                    vec![],
                    vec![],
                    vec![],
                    PrivateKey::random(&mut OsRng),
                    PrivateKey::random(&mut OsRng),
                ),
                status: TransactionStatus::MinedUnconfirmed,
                message: "Yo!".to_string(),
                timestamp: Utc::now().naive_utc(),
                cancelled: None,
                direction: TransactionDirection::Unknown,
                coinbase_block_height: None,
                send_count: 0,
                last_send_timestamp: None,
                transaction_signature: Signature::default(),
                confirmations: None,
                mined_height: None,
                mined_in_block: None,
                mined_timestamp: None,
            };
            let completed_tx_sql = CompletedTransactionSql::try_from(completed_tx, &cipher).unwrap();

            completed_tx_sql.commit(&mut conn).unwrap();
        }

        let connection = WalletDbConnection::new(pool, None);

        let db2 = TransactionServiceSqliteDatabase::new(connection.clone(), cipher);

        db2.fetch(&DbKey::PendingInboundTransactions).unwrap();

        assert!(db2.fetch(&DbKey::PendingInboundTransactions).is_ok());
        assert!(db2.fetch(&DbKey::PendingOutboundTransactions).is_ok());
        assert!(db2.fetch(&DbKey::CompletedTransactions).is_ok());

        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let new_cipher = XChaCha20Poly1305::new(key_ga);

        let db3 = TransactionServiceSqliteDatabase::new(connection, new_cipher);
        assert!(db3.fetch(&DbKey::PendingInboundTransactions).is_err());
        assert!(db3.fetch(&DbKey::PendingOutboundTransactions).is_err());
        assert!(db3.fetch(&DbKey::CompletedTransactions).is_err());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_customized_transactional_queries() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
        // Note: For this test the connection pool is setup with a pool size of 2; a pooled connection must go out
        // of scope to be released once obtained otherwise subsequent calls to obtain a pooled connection will fail .
        let mut pool = SqliteConnectionPool::new(db_path.clone(), 2, true, true, Duration::from_secs(60));
        pool.create_pool()
            .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));
        let mut conn = pool
            .get_pooled_connection()
            .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        conn.run_pending_migrations(MIGRATIONS)
            .map(|v| {
                v.into_iter()
                    .map(|b| {
                        let m = format!("Running migration {}", b);
                        // std::io::stdout()
                        //     .write_all(m.as_ref())
                        //     .expect("Couldn't write migration number to stdout");
                        m
                    })
                    .collect::<Vec<String>>()
            })
            .expect("Migrations failed");

        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let cipher = XChaCha20Poly1305::new(key_ga);

        let mut info_list_reference: Vec<InboundTransactionSenderInfo> = vec![];
        for i in 0..1000 {
            let (cancelled, status, coinbase_block_height) = match i % 13 {
                0 => (
                    if i % 3 == 0 {
                        Some(TxCancellationReason::Unknown)
                    } else {
                        None
                    },
                    TransactionStatus::Completed,
                    None,
                ),
                1 => (
                    if i % 5 == 0 {
                        Some(TxCancellationReason::Unknown)
                    } else {
                        None
                    },
                    TransactionStatus::Broadcast,
                    None,
                ),
                2 => (
                    if i % 7 == 0 {
                        Some(TxCancellationReason::Unknown)
                    } else {
                        None
                    },
                    TransactionStatus::Completed,
                    Some(i % 2),
                ),
                3 => (
                    if i % 11 == 0 {
                        Some(TxCancellationReason::Unknown)
                    } else {
                        None
                    },
                    TransactionStatus::Broadcast,
                    Some(i % 2),
                ),
                4 => (None, TransactionStatus::Completed, None),
                5 => (None, TransactionStatus::Broadcast, None),
                6 => (None, TransactionStatus::Pending, None),
                7 => (None, TransactionStatus::Coinbase, None),
                8 => (None, TransactionStatus::MinedUnconfirmed, None),
                9 => (None, TransactionStatus::Imported, None),
                10 => (None, TransactionStatus::MinedConfirmed, None),
                _ => (None, TransactionStatus::Completed, Some(i)),
            };
            let source_address = TariAddress::new(
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                Network::LocalNet,
            );
            let destination_address = TariAddress::new(
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                Network::LocalNet,
            );
            let completed_tx = CompletedTransaction {
                tx_id: TxId::from(i),
                source_address,
                destination_address,
                amount: MicroMinotari::from(100),
                fee: MicroMinotari::from(100),
                transaction: Transaction::new(
                    vec![],
                    vec![],
                    vec![],
                    PrivateKey::random(&mut OsRng),
                    PrivateKey::random(&mut OsRng),
                ),
                status,
                message: "Yo!".to_string(),
                timestamp: Utc::now().naive_utc(),
                cancelled,
                direction: TransactionDirection::Unknown,
                coinbase_block_height,
                send_count: 0,
                last_send_timestamp: None,
                transaction_signature: Signature::default(),
                confirmations: None,
                mined_height: None,
                mined_in_block: None,
                mined_timestamp: None,
            };
            let completed_tx_sql = CompletedTransactionSql::try_from(completed_tx.clone(), &cipher).unwrap();

            completed_tx_sql.commit(&mut conn).unwrap();

            let inbound_tx = InboundTransaction::from(completed_tx);
            let inbound_tx_sql = InboundTransactionSql::try_from(inbound_tx.clone(), &cipher).unwrap();

            inbound_tx_sql.commit(&mut conn).unwrap();

            if cancelled.is_none() {
                info_list_reference.push(InboundTransactionSenderInfo {
                    tx_id: inbound_tx.tx_id,
                    source_address: inbound_tx.source_address,
                })
            }
        }

        let connection = WalletDbConnection::new(pool, None);
        let db1 = TransactionServiceSqliteDatabase::new(connection, cipher);

        let txn_list = db1.get_transactions_to_be_broadcast().unwrap();
        assert_eq!(txn_list.len(), 335);
        for txn in &txn_list {
            assert!(txn.status == TransactionStatus::Completed || txn.status == TransactionStatus::Broadcast);
            assert!(txn.cancelled.is_none());
            assert!(txn.coinbase_block_height.is_none() || txn.coinbase_block_height == Some(0));
        }

        let info_list = db1.get_pending_inbound_transaction_sender_info().unwrap();
        assert_eq!(info_list.len(), 941);
        assert_eq!(info_list, info_list_reference);
    }
}
