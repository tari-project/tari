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

use aes_gcm::{self, Aes256Gcm};
use chrono::{NaiveDateTime, Utc};
use diesel::{prelude::*, result::Error as DieselError, SqliteConnection};
use log::*;
use tari_common_types::{
    transaction::{
        TransactionConversionError,
        TransactionDirection,
        TransactionDirectionError,
        TransactionStatus,
        TxId,
    },
    types::{BlockHash, PrivateKey, PublicKey, Signature},
};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::tari_amount::MicroTari;
use tari_crypto::tari_utilities::{
    hex::{from_hex, Hex},
    ByteArray,
};
use thiserror::Error;
use tokio::time::Instant;

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
    util::{
        diesel_ext::ExpectedRowsExtension,
        encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::database::wallet";

/// A Sqlite backend for the Transaction Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct TransactionServiceSqliteDatabase {
    database_connection: WalletDbConnection,
    cipher: Arc<RwLock<Option<Aes256Gcm>>>,
}

impl TransactionServiceSqliteDatabase {
    pub fn new(database_connection: WalletDbConnection, cipher: Option<Aes256Gcm>) -> Self {
        Self {
            database_connection,
            cipher: Arc::new(RwLock::new(cipher)),
        }
    }

    fn insert(&self, kvp: DbKeyValuePair, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        match kvp {
            DbKeyValuePair::PendingOutboundTransaction(k, v) => {
                if OutboundTransactionSql::find_by_cancelled(k, false, conn).is_ok() {
                    return Err(TransactionStorageError::DuplicateOutput);
                }
                let mut o = OutboundTransactionSql::try_from(*v)?;
                self.encrypt_if_necessary(&mut o)?;
                o.commit(conn)?;
            },
            DbKeyValuePair::PendingInboundTransaction(k, v) => {
                if InboundTransactionSql::find_by_cancelled(k, false, conn).is_ok() {
                    return Err(TransactionStorageError::DuplicateOutput);
                }
                let mut i = InboundTransactionSql::try_from(*v)?;
                self.encrypt_if_necessary(&mut i)?;

                i.commit(conn)?;
            },
            DbKeyValuePair::CompletedTransaction(k, v) => {
                if CompletedTransactionSql::find_by_cancelled(k, false, conn).is_ok() {
                    return Err(TransactionStorageError::DuplicateOutput);
                }
                let mut c = CompletedTransactionSql::try_from(*v)?;
                self.encrypt_if_necessary(&mut c)?;

                c.commit(conn)?;
            },
        }
        Ok(())
    }

    fn remove(&self, key: DbKey, conn: &SqliteConnection) -> Result<Option<DbValue>, TransactionStorageError> {
        match key {
            DbKey::PendingOutboundTransaction(k) => match OutboundTransactionSql::find_by_cancelled(k, false, conn) {
                Ok(mut v) => {
                    v.delete(conn)?;
                    self.decrypt_if_necessary(&mut v)?;
                    Ok(Some(DbValue::PendingOutboundTransaction(Box::new(
                        OutboundTransaction::try_from(v)?,
                    ))))
                },
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => Err(
                    TransactionStorageError::ValueNotFound(DbKey::PendingOutboundTransaction(k)),
                ),
                Err(e) => Err(e),
            },
            DbKey::PendingInboundTransaction(k) => match InboundTransactionSql::find_by_cancelled(k, false, conn) {
                Ok(mut v) => {
                    v.delete(conn)?;
                    self.decrypt_if_necessary(&mut v)?;
                    Ok(Some(DbValue::PendingInboundTransaction(Box::new(
                        InboundTransaction::try_from(v)?,
                    ))))
                },
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => Err(
                    TransactionStorageError::ValueNotFound(DbKey::PendingOutboundTransaction(k)),
                ),
                Err(e) => Err(e),
            },
            DbKey::CompletedTransaction(k) => match CompletedTransactionSql::find_by_cancelled(k, false, conn) {
                Ok(mut v) => {
                    v.delete(conn)?;
                    self.decrypt_if_necessary(&mut v)?;
                    Ok(Some(DbValue::CompletedTransaction(Box::new(
                        CompletedTransaction::try_from(v)?,
                    ))))
                },
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                    Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(k)))
                },
                Err(e) => Err(e),
            },
            DbKey::PendingOutboundTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::PendingInboundTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CompletedTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CancelledPendingOutboundTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CancelledPendingInboundTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CancelledCompletedTransactions => Err(TransactionStorageError::OperationNotSupported),
            DbKey::CancelledPendingOutboundTransaction(k) => {
                match OutboundTransactionSql::find_by_cancelled(k, true, conn) {
                    Ok(mut v) => {
                        v.delete(conn)?;
                        self.decrypt_if_necessary(&mut v)?;
                        Ok(Some(DbValue::PendingOutboundTransaction(Box::new(
                            OutboundTransaction::try_from(v)?,
                        ))))
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => Err(
                        TransactionStorageError::ValueNotFound(DbKey::CancelledPendingOutboundTransaction(k)),
                    ),
                    Err(e) => Err(e),
                }
            },
            DbKey::CancelledPendingInboundTransaction(k) => {
                match InboundTransactionSql::find_by_cancelled(k, true, conn) {
                    Ok(mut v) => {
                        v.delete(conn)?;
                        self.decrypt_if_necessary(&mut v)?;
                        Ok(Some(DbValue::PendingInboundTransaction(Box::new(
                            InboundTransaction::try_from(v)?,
                        ))))
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => Err(
                        TransactionStorageError::ValueNotFound(DbKey::CancelledPendingOutboundTransaction(k)),
                    ),
                    Err(e) => Err(e),
                }
            },
            DbKey::AnyTransaction(_) => Err(TransactionStorageError::OperationNotSupported),
        }
    }

    fn decrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), TransactionStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.decrypt(cipher)
                .map_err(|_| TransactionStorageError::AeadError("Decryption Error".to_string()))?;
        }
        Ok(())
    }

    fn encrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), TransactionStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.encrypt(cipher)
                .map_err(|_| TransactionStorageError::AeadError("Encryption Error".to_string()))?;
        }
        Ok(())
    }
}

impl TransactionBackend for TransactionServiceSqliteDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, TransactionStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = match key {
            DbKey::PendingOutboundTransaction(t) => match OutboundTransactionSql::find_by_cancelled(*t, false, &conn) {
                Ok(mut o) => {
                    self.decrypt_if_necessary(&mut o)?;

                    Some(DbValue::PendingOutboundTransaction(Box::new(
                        OutboundTransaction::try_from(o)?,
                    )))
                },
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::PendingInboundTransaction(t) => match InboundTransactionSql::find_by_cancelled(*t, false, &conn) {
                Ok(mut i) => {
                    self.decrypt_if_necessary(&mut i)?;
                    Some(DbValue::PendingInboundTransaction(Box::new(
                        InboundTransaction::try_from(i)?,
                    )))
                },
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::CompletedTransaction(t) => match CompletedTransactionSql::find(*t, &conn) {
                Ok(mut c) => {
                    self.decrypt_if_necessary(&mut c)?;
                    Some(DbValue::CompletedTransaction(Box::new(CompletedTransaction::try_from(
                        c,
                    )?)))
                },
                Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::AnyTransaction(t) => {
                match OutboundTransactionSql::find(*t, &conn) {
                    Ok(mut o) => {
                        self.decrypt_if_necessary(&mut o)?;

                        return Ok(Some(DbValue::WalletTransaction(Box::new(
                            WalletTransaction::PendingOutbound(OutboundTransaction::try_from(o)?),
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                };
                match InboundTransactionSql::find(*t, &conn) {
                    Ok(mut i) => {
                        self.decrypt_if_necessary(&mut i)?;
                        return Ok(Some(DbValue::WalletTransaction(Box::new(
                            WalletTransaction::PendingInbound(InboundTransaction::try_from(i)?),
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                };
                match CompletedTransactionSql::find(*t, &conn) {
                    Ok(mut c) => {
                        self.decrypt_if_necessary(&mut c)?;
                        return Ok(Some(DbValue::WalletTransaction(Box::new(
                            WalletTransaction::Completed(CompletedTransaction::try_from(c)?),
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                };

                None
            },
            DbKey::PendingOutboundTransactions => {
                let mut result = HashMap::new();
                for o in OutboundTransactionSql::index_by_cancelled(&conn, false)?.iter_mut() {
                    self.decrypt_if_necessary(o)?;
                    result.insert((o.tx_id as u64).into(), OutboundTransaction::try_from((*o).clone())?);
                }

                Some(DbValue::PendingOutboundTransactions(result))
            },
            DbKey::PendingInboundTransactions => {
                let mut result = HashMap::new();
                for i in InboundTransactionSql::index_by_cancelled(&conn, false)?.iter_mut() {
                    self.decrypt_if_necessary(i)?;
                    result.insert((i.tx_id as u64).into(), InboundTransaction::try_from((*i).clone())?);
                }

                Some(DbValue::PendingInboundTransactions(result))
            },
            DbKey::CompletedTransactions => {
                let mut result = HashMap::new();
                for c in CompletedTransactionSql::index_by_cancelled(&conn, false)?.iter_mut() {
                    self.decrypt_if_necessary(c)?;
                    result.insert((c.tx_id as u64).into(), CompletedTransaction::try_from((*c).clone())?);
                }

                Some(DbValue::CompletedTransactions(result))
            },
            DbKey::CancelledPendingOutboundTransactions => {
                let mut result = HashMap::new();
                for o in OutboundTransactionSql::index_by_cancelled(&conn, true)?.iter_mut() {
                    self.decrypt_if_necessary(o)?;
                    result.insert((o.tx_id as u64).into(), OutboundTransaction::try_from((*o).clone())?);
                }

                Some(DbValue::PendingOutboundTransactions(result))
            },
            DbKey::CancelledPendingInboundTransactions => {
                let mut result = HashMap::new();
                for i in InboundTransactionSql::index_by_cancelled(&conn, true)?.iter_mut() {
                    self.decrypt_if_necessary(i)?;
                    result.insert((i.tx_id as u64).into(), InboundTransaction::try_from((*i).clone())?);
                }

                Some(DbValue::PendingInboundTransactions(result))
            },
            DbKey::CancelledCompletedTransactions => {
                let mut result = HashMap::new();
                for c in CompletedTransactionSql::index_by_cancelled(&conn, true)?.iter_mut() {
                    self.decrypt_if_necessary(c)?;
                    result.insert((c.tx_id as u64).into(), CompletedTransaction::try_from((*c).clone())?);
                }

                Some(DbValue::CompletedTransactions(result))
            },
            DbKey::CancelledPendingOutboundTransaction(t) => {
                match OutboundTransactionSql::find_by_cancelled(*t, true, &conn) {
                    Ok(mut o) => {
                        self.decrypt_if_necessary(&mut o)?;

                        Some(DbValue::PendingOutboundTransaction(Box::new(
                            OutboundTransaction::try_from(o)?,
                        )))
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => None,
                    Err(e) => return Err(e),
                }
            },
            DbKey::CancelledPendingInboundTransaction(t) => {
                match InboundTransactionSql::find_by_cancelled(*t, true, &conn) {
                    Ok(mut i) => {
                        self.decrypt_if_necessary(&mut i)?;
                        Some(DbValue::PendingInboundTransaction(Box::new(
                            InboundTransaction::try_from(i)?,
                        )))
                    },
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = match key {
            DbKey::PendingOutboundTransaction(k) => OutboundTransactionSql::find_by_cancelled(*k, false, &conn).is_ok(),
            DbKey::PendingInboundTransaction(k) => InboundTransactionSql::find_by_cancelled(*k, false, &conn).is_ok(),
            DbKey::CompletedTransaction(k) => CompletedTransactionSql::find(*k, &conn).is_ok(),
            DbKey::PendingOutboundTransactions => false,
            DbKey::PendingInboundTransactions => false,
            DbKey::CompletedTransactions => false,
            DbKey::CancelledPendingOutboundTransactions => false,
            DbKey::CancelledPendingInboundTransactions => false,
            DbKey::CancelledCompletedTransactions => false,
            DbKey::CancelledPendingOutboundTransaction(k) => {
                OutboundTransactionSql::find_by_cancelled(*k, true, &conn).is_ok()
            },
            DbKey::CancelledPendingInboundTransaction(k) => {
                InboundTransactionSql::find_by_cancelled(*k, true, &conn).is_ok()
            },
            DbKey::AnyTransaction(k) => {
                CompletedTransactionSql::find(*k, &conn).is_ok() ||
                    InboundTransactionSql::find(*k, &conn).is_ok() ||
                    OutboundTransactionSql::find(*k, &conn).is_ok()
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let key_text;

        let result = match op {
            WriteOperation::Insert(kvp) => {
                key_text = "Insert";
                self.insert(kvp, &conn).map(|_| None)
            },
            WriteOperation::Remove(key) => {
                key_text = "Remove";
                self.remove(key, &conn)
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = OutboundTransactionSql::find_by_cancelled(tx_id, false, &conn).is_ok() ||
            InboundTransactionSql::find_by_cancelled(tx_id, false, &conn).is_ok() ||
            CompletedTransactionSql::find_by_cancelled(tx_id, false, &conn).is_ok();
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

    fn get_pending_transaction_counterparty_pub_key_by_tx_id(
        &self,
        tx_id: TxId,
    ) -> Result<CommsPublicKey, TransactionStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        if let Ok(mut outbound_tx_sql) = OutboundTransactionSql::find_by_cancelled(tx_id, false, &conn) {
            self.decrypt_if_necessary(&mut outbound_tx_sql)?;
            let outbound_tx = OutboundTransaction::try_from(outbound_tx_sql)?;
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
            return Ok(outbound_tx.destination_public_key);
        }
        if let Ok(mut inbound_tx_sql) = InboundTransactionSql::find_by_cancelled(tx_id, false, &conn) {
            self.decrypt_if_necessary(&mut inbound_tx_sql)?;
            let inbound_tx = InboundTransaction::try_from(inbound_tx_sql)?;
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
            return Ok(inbound_tx.source_public_key);
        }

        Err(TransactionStorageError::ValuesNotFound)
    }

    fn fetch_any_cancelled_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<Option<WalletTransaction>, TransactionStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;

        match OutboundTransactionSql::find_by_cancelled(tx_id, true, &conn) {
            Ok(mut o) => {
                self.decrypt_if_necessary(&mut o)?;

                return Ok(Some(WalletTransaction::PendingOutbound(OutboundTransaction::try_from(
                    o,
                )?)));
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
            Err(e) => return Err(e),
        };
        match InboundTransactionSql::find_by_cancelled(tx_id, true, &conn) {
            Ok(mut i) => {
                self.decrypt_if_necessary(&mut i)?;
                return Ok(Some(WalletTransaction::PendingInbound(InboundTransaction::try_from(
                    i,
                )?)));
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
            Err(e) => return Err(e),
        };
        match CompletedTransactionSql::find_by_cancelled(tx_id, true, &conn) {
            Ok(mut c) => {
                self.decrypt_if_necessary(&mut c)?;
                return Ok(Some(WalletTransaction::Completed(CompletedTransaction::try_from(c)?)));
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        if CompletedTransactionSql::find_by_cancelled(tx_id, false, &conn).is_ok() {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }

        match OutboundTransactionSql::find_by_cancelled(tx_id, false, &conn) {
            Ok(v) => {
                let mut completed_tx_sql = CompletedTransactionSql::try_from(completed_transaction)?;
                self.encrypt_if_necessary(&mut completed_tx_sql)?;
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        if CompletedTransactionSql::find_by_cancelled(tx_id, false, &conn).is_ok() {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }

        match InboundTransactionSql::find_by_cancelled(tx_id, false, &conn) {
            Ok(v) => {
                let mut completed_tx_sql = CompletedTransactionSql::try_from(completed_transaction)?;
                self.encrypt_if_necessary(&mut completed_tx_sql)?;
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        match CompletedTransactionSql::find_by_cancelled(tx_id, false, &conn) {
            Ok(v) => {
                if TransactionStatus::try_from(v.status)? == TransactionStatus::Completed {
                    v.update(
                        UpdateCompletedTransactionSql {
                            status: Some(TransactionStatus::Broadcast as i32),
                            ..Default::default()
                        },
                        &conn,
                    )?;
                }
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                    tx_id,
                )))
            },
            Err(e) => return Err(e),
        };
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        match CompletedTransactionSql::find_by_cancelled(tx_id, false, &conn) {
            Ok(v) => {
                v.reject(reason, &conn)?;
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                    tx_id,
                )));
            },
            Err(e) => return Err(e),
        };
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        match InboundTransactionSql::find(tx_id, &conn) {
            Ok(v) => {
                v.set_cancelled(cancelled, &conn)?;
            },
            Err(_) => {
                match OutboundTransactionSql::find(tx_id, &conn) {
                    Ok(v) => {
                        v.set_cancelled(cancelled, &conn)?;
                    },
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        match InboundTransactionSql::find_by_cancelled(tx_id, false, &conn) {
            Ok(v) => {
                v.update(
                    UpdateInboundTransactionSql {
                        cancelled: None,
                        direct_send_success: Some(1i32),
                        receiver_protocol: None,
                        send_count: None,
                        last_send_timestamp: None,
                    },
                    &conn,
                )?;
            },
            Err(_) => {
                match OutboundTransactionSql::find_by_cancelled(tx_id, false, &conn) {
                    Ok(v) => {
                        v.update(
                            UpdateOutboundTransactionSql {
                                cancelled: None,
                                direct_send_success: Some(1i32),
                                sender_protocol: None,
                                send_count: None,
                                last_send_timestamp: None,
                            },
                            &conn,
                        )?;
                    },
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

    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), TransactionStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);

        if (*current_cipher).is_some() {
            return Err(TransactionStorageError::AlreadyEncrypted);
        }

        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let mut inbound_txs = InboundTransactionSql::index(&conn)?;
        // If the db is already encrypted then the very first output we try to encrypt will fail.
        for tx in inbound_txs.iter_mut() {
            // Test if this transaction is encrypted or not to avoid a double encryption.
            let _ = InboundTransaction::try_from(tx.clone()).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not convert Inbound Transaction from database version, it might already be encrypted"
                );
                TransactionStorageError::AlreadyEncrypted
            })?;
            tx.encrypt(&cipher)
                .map_err(|_| TransactionStorageError::AeadError("Encryption Error".to_string()))?;
            tx.update_encryption(&conn)?;
        }

        let mut outbound_txs = OutboundTransactionSql::index(&conn)?;
        // If the db is already encrypted then the very first output we try to encrypt will fail.
        for tx in outbound_txs.iter_mut() {
            // Test if this transaction is encrypted or not to avoid a double encryption.
            let _ = OutboundTransaction::try_from(tx.clone()).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not convert Inbound Transaction from database version, it might already be encrypted"
                );
                TransactionStorageError::AlreadyEncrypted
            })?;
            tx.encrypt(&cipher)
                .map_err(|_| TransactionStorageError::AeadError("Encryption Error".to_string()))?;
            tx.update_encryption(&conn)?;
        }

        let mut completed_txs = CompletedTransactionSql::index(&conn)?;
        // If the db is already encrypted then the very first output we try to encrypt will fail.
        for tx in completed_txs.iter_mut() {
            // Test if this transaction is encrypted or not to avoid a double encryption.
            let _ = CompletedTransaction::try_from(tx.clone()).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not convert Inbound Transaction from database version, it might already be encrypted"
                );
                TransactionStorageError::AlreadyEncrypted
            })?;
            tx.encrypt(&cipher)
                .map_err(|_| TransactionStorageError::AeadError("Encryption Error".to_string()))?;
            tx.update_encryption(&conn)?;
        }

        (*current_cipher) = Some(cipher);
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - apply_encryption: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn remove_encryption(&self) -> Result<(), TransactionStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);

        let cipher = if let Some(cipher) = (*current_cipher).clone().take() {
            cipher
        } else {
            return Ok(());
        };
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let mut inbound_txs = InboundTransactionSql::index(&conn)?;

        for tx in inbound_txs.iter_mut() {
            tx.decrypt(&cipher)
                .map_err(|_| TransactionStorageError::AeadError("Decryption Error".to_string()))?;
            tx.update_encryption(&conn)?;
        }

        let mut outbound_txs = OutboundTransactionSql::index(&conn)?;

        for tx in outbound_txs.iter_mut() {
            tx.decrypt(&cipher)
                .map_err(|_| TransactionStorageError::AeadError("Decryption Error".to_string()))?;
            tx.update_encryption(&conn)?;
        }

        let mut completed_txs = CompletedTransactionSql::index(&conn)?;
        for tx in completed_txs.iter_mut() {
            tx.decrypt(&cipher)
                .map_err(|_| TransactionStorageError::AeadError("Decryption Error".to_string()))?;
            tx.update_encryption(&conn)?;
        }

        // Now that all the decryption has been completed we can safely remove the cipher fully
        let _ = (*current_cipher).take();
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - remove_encryption: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn cancel_coinbase_transaction_at_block_height(&self, block_height: u64) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let coinbase_txs = CompletedTransactionSql::index_coinbase_at_block_height(block_height as i64, &conn)?;
        for c in coinbase_txs.iter() {
            c.reject(TxCancellationReason::AbandonedCoinbase, &conn)?;
        }
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
        amount: MicroTari,
    ) -> Result<Option<CompletedTransaction>, TransactionStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let mut coinbase_txs = CompletedTransactionSql::index_coinbase_at_block_height(block_height as i64, &conn)?;
        for c in coinbase_txs.iter_mut() {
            self.decrypt_if_necessary(c)?;
            let completed_tx = CompletedTransaction::try_from(c.clone())?;
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        if let Ok(tx) = CompletedTransactionSql::find(tx_id, &conn) {
            let update = UpdateCompletedTransactionSql {
                send_count: Some(tx.send_count + 1),
                last_send_timestamp: Some(Some(Utc::now().naive_utc())),
                ..Default::default()
            };
            tx.update(update, &conn)?;
        } else if let Ok(tx) = OutboundTransactionSql::find(tx_id, &conn) {
            let update = UpdateOutboundTransactionSql {
                cancelled: None,
                direct_send_success: None,
                sender_protocol: None,
                send_count: Some(tx.send_count + 1),
                last_send_timestamp: Some(Some(Utc::now().naive_utc())),
            };
            tx.update(update, &conn)?;
        } else if let Ok(tx) = InboundTransactionSql::find_by_cancelled(tx_id, false, &conn) {
            let update = UpdateInboundTransactionSql {
                cancelled: None,
                direct_send_success: None,
                receiver_protocol: None,
                send_count: Some(tx.send_count + 1),
                last_send_timestamp: Some(Some(Utc::now().naive_utc())),
            };
            tx.update(update, &conn)?;
        } else {
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
        num_confirmations: u64,
        is_confirmed: bool,
        is_faux: bool,
    ) -> Result<(), TransactionStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        match CompletedTransactionSql::find(tx_id, &conn) {
            Ok(v) => {
                v.update_mined_height(
                    mined_height,
                    mined_in_block,
                    num_confirmations,
                    is_confirmed,
                    &conn,
                    is_faux,
                )?;
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                    tx_id,
                )));
            },
            Err(e) => return Err(e),
        };
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let tx = completed_transactions::table
            // Note: Check 'mined_in_block' as well as 'mined_height' is populated for faux transactions before it is confirmed
            .filter(completed_transactions::mined_in_block.is_not_null())
            .filter(completed_transactions::mined_height.is_not_null())
            .filter(completed_transactions::mined_height.gt(0))
            .order_by(completed_transactions::mined_height.desc())
            .first::<CompletedTransactionSql>(&*conn)
            .optional()?;
        let result = match tx {
            Some(mut tx) => {
                self.decrypt_if_necessary(&mut tx)?;
                Some(tx.try_into()?)
            },
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut tx_info: Vec<UnconfirmedTransactionInfo> = vec![];
        match UnconfirmedTransactionInfoSql::fetch_unconfirmed_transactions_info(&*conn) {
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
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
            .load::<CompletedTransactionSql>(&*conn)?;

        let mut result = vec![];
        for mut tx in txs {
            self.decrypt_if_necessary(&mut tx)?;
            result.push(tx.try_into()?);
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let result = diesel::update(completed_transactions::table.filter(completed_transactions::cancelled.is_null()))
            .set((
                completed_transactions::mined_height.eq::<Option<i64>>(None),
                completed_transactions::mined_in_block.eq::<Option<Vec<u8>>>(None),
            ))
            .execute(&conn)?;

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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        match CompletedTransactionSql::find(tx_id, &conn) {
            Ok(v) => {
                v.set_as_unmined(&conn)?;
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                    tx_id,
                )));
            },
            Err(e) => return Err(e),
        };
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
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut sender_info: Vec<InboundTransactionSenderInfo> = vec![];
        match InboundTransactionSenderInfoSql::get_pending_inbound_transaction_sender_info(&conn) {
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
        let conn = self.database_connection.get_pooled_connection()?;
        CompletedTransactionSql::index_by_status_and_cancelled(TransactionStatus::Imported, false, &conn)?
            .into_iter()
            .map(|mut ct: CompletedTransactionSql| {
                if let Err(e) = self.decrypt_if_necessary(&mut ct) {
                    return Err(e);
                }
                CompletedTransaction::try_from(ct).map_err(TransactionStorageError::from)
            })
            .collect::<Result<Vec<CompletedTransaction>, TransactionStorageError>>()
    }

    fn fetch_unconfirmed_faux_transactions(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        CompletedTransactionSql::index_by_status_and_cancelled(TransactionStatus::FauxUnconfirmed, false, &conn)?
            .into_iter()
            .map(|mut ct: CompletedTransactionSql| {
                if let Err(e) = self.decrypt_if_necessary(&mut ct) {
                    return Err(e);
                }
                CompletedTransaction::try_from(ct).map_err(TransactionStorageError::from)
            })
            .collect::<Result<Vec<CompletedTransaction>, TransactionStorageError>>()
    }

    fn fetch_confirmed_faux_transactions_from_height(
        &self,
        height: u64,
    ) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        CompletedTransactionSql::index_by_status_and_cancelled_from_block_height(
            TransactionStatus::FauxConfirmed,
            false,
            height as i64,
            &conn,
        )?
        .into_iter()
        .map(|mut ct: CompletedTransactionSql| {
            if let Err(e) = self.decrypt_if_necessary(&mut ct) {
                return Err(e);
            }
            CompletedTransaction::try_from(ct).map_err(TransactionStorageError::from)
        })
        .collect::<Result<Vec<CompletedTransaction>, TransactionStorageError>>()
    }

    fn abandon_coinbase_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        match CompletedTransactionSql::find_by_cancelled(tx_id, false, &conn) {
            Ok(tx) => {
                tx.abandon_coinbase(&conn)?;
            },
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
    pub(crate) source_public_key: CommsPublicKey,
}

impl TryFrom<InboundTransactionSenderInfoSql> for InboundTransactionSenderInfo {
    type Error = TransactionStorageError;

    fn try_from(i: InboundTransactionSenderInfoSql) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: TxId::from(i.tx_id as u64),
            source_public_key: CommsPublicKey::from_bytes(&*i.source_public_key)
                .map_err(TransactionStorageError::ByteArrayError)?,
        })
    }
}

#[derive(Clone, Queryable)]
pub struct InboundTransactionSenderInfoSql {
    pub tx_id: i64,
    pub source_public_key: Vec<u8>,
}

impl InboundTransactionSenderInfoSql {
    pub fn get_pending_inbound_transaction_sender_info(
        conn: &SqliteConnection,
    ) -> Result<Vec<InboundTransactionSenderInfoSql>, TransactionStorageError> {
        let query_result = inbound_transactions::table
            .select((inbound_transactions::tx_id, inbound_transactions::source_public_key))
            .filter(inbound_transactions::cancelled.eq(false as i32))
            .load::<InboundTransactionSenderInfoSql>(conn)?;
        Ok(query_result)
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
    cancelled: i32,
    direct_send_success: i32,
    send_count: i32,
    last_send_timestamp: Option<NaiveDateTime>,
}

impl InboundTransactionSql {
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::insert_into(inbound_transactions::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn index(conn: &SqliteConnection) -> Result<Vec<InboundTransactionSql>, TransactionStorageError> {
        Ok(inbound_transactions::table.load::<InboundTransactionSql>(conn)?)
    }

    pub fn index_by_cancelled(
        conn: &SqliteConnection,
        cancelled: bool,
    ) -> Result<Vec<InboundTransactionSql>, TransactionStorageError> {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::cancelled.eq(cancelled as i32))
            .load::<InboundTransactionSql>(conn)?)
    }

    pub fn find(tx_id: TxId, conn: &SqliteConnection) -> Result<InboundTransactionSql, TransactionStorageError> {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .first::<InboundTransactionSql>(conn)?)
    }

    pub fn find_by_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &SqliteConnection,
    ) -> Result<InboundTransactionSql, TransactionStorageError> {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .filter(inbound_transactions::cancelled.eq(cancelled as i32))
            .first::<InboundTransactionSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        let num_deleted =
            diesel::delete(inbound_transactions::table.filter(inbound_transactions::tx_id.eq(&self.tx_id)))
                .execute(conn)?;

        if num_deleted == 0 {
            return Err(TransactionStorageError::ValuesNotFound);
        }

        Ok(())
    }

    pub fn update(
        &self,
        update: UpdateInboundTransactionSql,
        conn: &SqliteConnection,
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

    pub fn set_cancelled(&self, cancelled: bool, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateInboundTransactionSql {
                cancelled: Some(cancelled as i32),
                direct_send_success: None,
                receiver_protocol: None,
                send_count: None,
                last_send_timestamp: None,
            },
            conn,
        )
    }

    pub fn update_encryption(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
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
}

impl Encryptable<Aes256Gcm> for InboundTransactionSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        let encrypted_protocol = encrypt_bytes_integral_nonce(cipher, self.receiver_protocol.as_bytes().to_vec())?;
        self.receiver_protocol = encrypted_protocol.to_hex();
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        let decrypted_protocol = decrypt_bytes_integral_nonce(
            cipher,
            from_hex(self.receiver_protocol.as_str()).map_err(|e| e.to_string())?,
        )?;
        self.receiver_protocol = from_utf8(decrypted_protocol.as_slice())
            .map_err(|e| e.to_string())?
            .to_string();
        Ok(())
    }
}

impl TryFrom<InboundTransaction> for InboundTransactionSql {
    type Error = TransactionStorageError;

    fn try_from(i: InboundTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: i.tx_id.as_u64() as i64,
            source_public_key: i.source_public_key.to_vec(),
            amount: u64::from(i.amount) as i64,
            receiver_protocol: serde_json::to_string(&i.receiver_protocol)?,
            message: i.message,
            timestamp: i.timestamp,
            cancelled: i.cancelled as i32,
            direct_send_success: i.direct_send_success as i32,
            send_count: i.send_count as i32,
            last_send_timestamp: i.last_send_timestamp,
        })
    }
}

impl TryFrom<InboundTransactionSql> for InboundTransaction {
    type Error = TransactionStorageError;

    fn try_from(i: InboundTransactionSql) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: (i.tx_id as u64).into(),
            source_public_key: PublicKey::from_vec(&i.source_public_key).map_err(TransactionKeyError::Source)?,
            amount: MicroTari::from(i.amount as u64),
            receiver_protocol: serde_json::from_str(&i.receiver_protocol)?,
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
#[table_name = "inbound_transactions"]
pub struct UpdateInboundTransactionSql {
    cancelled: Option<i32>,
    direct_send_success: Option<i32>,
    receiver_protocol: Option<String>,
    send_count: Option<i32>,
    last_send_timestamp: Option<Option<NaiveDateTime>>,
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
    cancelled: i32,
    direct_send_success: i32,
    send_count: i32,
    last_send_timestamp: Option<NaiveDateTime>,
}

impl OutboundTransactionSql {
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::insert_into(outbound_transactions::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn index(conn: &SqliteConnection) -> Result<Vec<OutboundTransactionSql>, TransactionStorageError> {
        Ok(outbound_transactions::table.load::<OutboundTransactionSql>(conn)?)
    }

    pub fn index_by_cancelled(
        conn: &SqliteConnection,
        cancelled: bool,
    ) -> Result<Vec<OutboundTransactionSql>, TransactionStorageError> {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::cancelled.eq(cancelled as i32))
            .load::<OutboundTransactionSql>(conn)?)
    }

    pub fn find(tx_id: TxId, conn: &SqliteConnection) -> Result<OutboundTransactionSql, TransactionStorageError> {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .first::<OutboundTransactionSql>(conn)?)
    }

    pub fn find_by_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &SqliteConnection,
    ) -> Result<OutboundTransactionSql, TransactionStorageError> {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .filter(outbound_transactions::cancelled.eq(cancelled as i32))
            .first::<OutboundTransactionSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::delete(outbound_transactions::table.filter(outbound_transactions::tx_id.eq(&self.tx_id)))
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;
        Ok(())
    }

    pub fn update(
        &self,
        update: UpdateOutboundTransactionSql,
        conn: &SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        diesel::update(outbound_transactions::table.filter(outbound_transactions::tx_id.eq(&self.tx_id)))
            .set(update)
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    pub fn set_cancelled(&self, cancelled: bool, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateOutboundTransactionSql {
                cancelled: Some(cancelled as i32),
                direct_send_success: None,
                sender_protocol: None,
                send_count: None,
                last_send_timestamp: None,
            },
            conn,
        )
    }

    pub fn update_encryption(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
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
}

impl Encryptable<Aes256Gcm> for OutboundTransactionSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        let encrypted_protocol = encrypt_bytes_integral_nonce(cipher, self.sender_protocol.as_bytes().to_vec())?;
        self.sender_protocol = encrypted_protocol.to_hex();
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        let decrypted_protocol = decrypt_bytes_integral_nonce(
            cipher,
            from_hex(self.sender_protocol.as_str()).map_err(|e| e.to_string())?,
        )?;
        self.sender_protocol = from_utf8(decrypted_protocol.as_slice())
            .map_err(|e| e.to_string())?
            .to_string();
        Ok(())
    }
}

impl TryFrom<OutboundTransaction> for OutboundTransactionSql {
    type Error = TransactionStorageError;

    fn try_from(o: OutboundTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: o.tx_id.as_u64() as i64,
            destination_public_key: o.destination_public_key.to_vec(),
            amount: u64::from(o.amount) as i64,
            fee: u64::from(o.fee) as i64,
            sender_protocol: serde_json::to_string(&o.sender_protocol)?,
            message: o.message,
            timestamp: o.timestamp,
            cancelled: o.cancelled as i32,
            direct_send_success: o.direct_send_success as i32,
            send_count: o.send_count as i32,
            last_send_timestamp: o.last_send_timestamp,
        })
    }
}

impl TryFrom<OutboundTransactionSql> for OutboundTransaction {
    type Error = TransactionStorageError;

    fn try_from(o: OutboundTransactionSql) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: (o.tx_id as u64).into(),
            destination_public_key: PublicKey::from_vec(&o.destination_public_key)
                .map_err(TransactionKeyError::Destination)?,
            amount: MicroTari::from(o.amount as u64),
            fee: MicroTari::from(o.fee as u64),
            sender_protocol: serde_json::from_str(&o.sender_protocol)?,
            status: TransactionStatus::Pending,
            message: o.message,
            timestamp: o.timestamp,
            cancelled: o.cancelled != 0,
            direct_send_success: o.direct_send_success != 0,
            send_count: o.send_count as u32,
            last_send_timestamp: o.last_send_timestamp,
        })
    }
}

#[derive(AsChangeset)]
#[table_name = "outbound_transactions"]
pub struct UpdateOutboundTransactionSql {
    cancelled: Option<i32>,
    direct_send_success: Option<i32>,
    sender_protocol: Option<String>,
    send_count: Option<i32>,
    last_send_timestamp: Option<Option<NaiveDateTime>>,
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
    cancelled: Option<i32>,
    direction: Option<i32>,
    coinbase_block_height: Option<i64>,
    send_count: i32,
    last_send_timestamp: Option<NaiveDateTime>,
    confirmations: Option<i64>,
    mined_height: Option<i64>,
    mined_in_block: Option<Vec<u8>>,
    transaction_signature_nonce: Vec<u8>,
    transaction_signature_key: Vec<u8>,
}

impl CompletedTransactionSql {
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        diesel::insert_into(completed_transactions::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn index(conn: &SqliteConnection) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError> {
        Ok(completed_transactions::table.load::<CompletedTransactionSql>(conn)?)
    }

    pub fn index_by_cancelled(
        conn: &SqliteConnection,
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
        conn: &SqliteConnection,
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
        conn: &SqliteConnection,
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
        conn: &SqliteConnection,
    ) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError> {
        Ok(completed_transactions::table
            .filter(completed_transactions::status.eq(TransactionStatus::Coinbase as i32))
            .filter(completed_transactions::coinbase_block_height.eq(block_height))
            .load::<CompletedTransactionSql>(conn)?)
    }

    pub fn find(tx_id: TxId, conn: &SqliteConnection) -> Result<CompletedTransactionSql, TransactionStorageError> {
        Ok(completed_transactions::table
            .filter(completed_transactions::tx_id.eq(tx_id.as_u64() as i64))
            .first::<CompletedTransactionSql>(conn)?)
    }

    pub fn find_by_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &SqliteConnection,
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

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
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
        conn: &SqliteConnection,
    ) -> Result<(), TransactionStorageError> {
        diesel::update(completed_transactions::table.filter(completed_transactions::tx_id.eq(&self.tx_id)))
            .set(updated_tx)
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;
        Ok(())
    }

    pub fn reject(&self, reason: TxCancellationReason, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateCompletedTransactionSql {
                cancelled: Some(Some(reason as i32)),
                status: Some(TransactionStatus::Rejected as i32),
                ..Default::default()
            },
            conn,
        )?;

        Ok(())
    }

    pub fn abandon_coinbase(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        if self.coinbase_block_height.is_none() {
            return Err(TransactionStorageError::NotCoinbase);
        }

        self.update(
            UpdateCompletedTransactionSql {
                cancelled: Some(Some(TxCancellationReason::AbandonedCoinbase as i32)),
                ..Default::default()
            },
            conn,
        )?;

        Ok(())
    }

    pub fn set_as_unmined(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        let status = if self.coinbase_block_height.is_some() {
            Some(TransactionStatus::Coinbase as i32)
        } else if self.status == TransactionStatus::FauxConfirmed as i32 {
            Some(TransactionStatus::FauxUnconfirmed as i32)
        } else if self.status == TransactionStatus::Broadcast as i32 {
            Some(TransactionStatus::Broadcast as i32)
        } else {
            Some(TransactionStatus::Completed as i32)
        };

        self.update(
            UpdateCompletedTransactionSql {
                status,
                mined_in_block: Some(None),
                mined_height: Some(None),
                confirmations: Some(None),
                // Turns out it should not be cancelled
                cancelled: Some(None),
                ..Default::default()
            },
            conn,
        )?;

        // Ideally the outputs should be marked unmined here as well, but because of the separation of classes,
        // that will be done in the outputs service.

        Ok(())
    }

    pub fn update_encryption(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateCompletedTransactionSql {
                transaction_protocol: Some(self.transaction_protocol.clone()),
                ..Default::default()
            },
            conn,
        )?;

        Ok(())
    }

    pub fn update_mined_height(
        &self,
        mined_height: u64,
        mined_in_block: BlockHash,
        num_confirmations: u64,
        is_confirmed: bool,
        conn: &SqliteConnection,
        is_faux: bool,
    ) -> Result<(), TransactionStorageError> {
        let status = if is_confirmed {
            if is_faux {
                TransactionStatus::FauxConfirmed as i32
            } else {
                TransactionStatus::MinedConfirmed as i32
            }
        } else if is_faux {
            TransactionStatus::FauxUnconfirmed as i32
        } else {
            TransactionStatus::MinedUnconfirmed as i32
        };

        self.update(
            UpdateCompletedTransactionSql {
                confirmations: Some(Some(num_confirmations as i64)),
                status: Some(status),
                mined_height: Some(Some(mined_height as i64)),
                mined_in_block: Some(Some(mined_in_block)),
                // If the tx is mined, then it can't be cancelled
                cancelled: None,
                ..Default::default()
            },
            conn,
        )?;

        Ok(())
    }
}

impl Encryptable<Aes256Gcm> for CompletedTransactionSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        let encrypted_protocol = encrypt_bytes_integral_nonce(cipher, self.transaction_protocol.as_bytes().to_vec())?;
        self.transaction_protocol = encrypted_protocol.to_hex();
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        let decrypted_protocol = decrypt_bytes_integral_nonce(
            cipher,
            from_hex(self.transaction_protocol.as_str()).map_err(|e| e.to_string())?,
        )?;
        self.transaction_protocol = from_utf8(decrypted_protocol.as_slice())
            .map_err(|e| e.to_string())?
            .to_string();
        Ok(())
    }
}

impl TryFrom<CompletedTransaction> for CompletedTransactionSql {
    type Error = TransactionStorageError;

    fn try_from(c: CompletedTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: c.tx_id.as_u64() as i64,
            source_public_key: c.source_public_key.to_vec(),
            destination_public_key: c.destination_public_key.to_vec(),
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
            mined_in_block: c.mined_in_block,
            transaction_signature_nonce: c.transaction_signature.get_public_nonce().to_vec(),
            transaction_signature_key: c.transaction_signature.get_signature().to_vec(),
        })
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
}

impl TryFrom<CompletedTransactionSql> for CompletedTransaction {
    type Error = CompletedTransactionConversionError;

    fn try_from(c: CompletedTransactionSql) -> Result<Self, Self::Error> {
        let transaction_signature = match PublicKey::from_vec(&c.transaction_signature_nonce) {
            Ok(public_nonce) => match PrivateKey::from_vec(&c.transaction_signature_key) {
                Ok(signature) => Signature::new(public_nonce, signature),
                Err(_) => Signature::default(),
            },
            Err(_) => Signature::default(),
        };
        Ok(Self {
            tx_id: (c.tx_id as u64).into(),
            source_public_key: PublicKey::from_vec(&c.source_public_key).map_err(TransactionKeyError::Source)?,
            destination_public_key: PublicKey::from_vec(&c.destination_public_key)
                .map_err(TransactionKeyError::Destination)?,
            amount: MicroTari::from(c.amount as u64),
            fee: MicroTari::from(c.fee as u64),
            transaction: serde_json::from_str(&c.transaction_protocol)?,
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
            mined_in_block: c.mined_in_block,
        })
    }
}

#[derive(AsChangeset, Default)]
#[table_name = "completed_transactions"]
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
        conn: &SqliteConnection,
    ) -> Result<Vec<UnconfirmedTransactionInfoSql>, TransactionStorageError> {
        // TODO: Should we not return cancelled transactions as well and handle it upstream? It could be mined. #LOGGED
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
            .load::<UnconfirmedTransactionInfoSql>(&*conn)?;
        Ok(query_result)
    }
}

#[cfg(test)]
mod test {
    use std::{convert::TryFrom, time::Duration};

    use aes_gcm::{
        aead::{generic_array::GenericArray, NewAead},
        Aes256Gcm,
    };
    use chrono::Utc;
    use diesel::{Connection, SqliteConnection};
    use rand::rngs::OsRng;
    use tari_common_sqlite::sqlite_connection_pool::SqliteConnectionPool;
    use tari_common_types::{
        transaction::{TransactionDirection, TransactionStatus, TxId},
        types::{HashDigest, PrivateKey, PublicKey, Signature},
    };
    use tari_core::{
        covenants::Covenant,
        transactions::{
            tari_amount::MicroTari,
            test_helpers::{create_unblinded_output, TestParams},
            transaction_components::{OutputFeatures, Transaction},
            transaction_protocol::sender::TransactionSenderMessage,
            CryptoFactories,
            ReceiverTransactionProtocol,
            SenderTransactionProtocol,
        },
    };
    use tari_crypto::{
        keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
        script,
        script::{ExecutionStack, TariScript},
    };
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
            },
        },
        util::encryption::Encryptable,
    };

    #[test]
    fn test_crud() {
        let factories = CryptoFactories::default();
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        conn.execute("PRAGMA foreign_keys = ON").unwrap();

        let constants = create_consensus_constants(0);
        let mut builder = SenderTransactionProtocol::builder(1, constants);
        let test_params = TestParams::new();
        let input = create_unblinded_output(
            TariScript::default(),
            OutputFeatures::default(),
            test_params,
            MicroTari::from(100_000),
        );
        let amount = MicroTari::from(10_000);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari::from(177 / 5))
            .with_offset(PrivateKey::random(&mut OsRng))
            .with_private_nonce(PrivateKey::random(&mut OsRng))
            .with_amount(0, amount)
            .with_message("Yo!".to_string())
            .with_input(
                input
                    .as_transaction_input(&factories.commitment)
                    .expect("Should be able to make transaction input"),
                input,
            )
            .with_change_secret(PrivateKey::random(&mut OsRng))
            .with_recipient_data(
                0,
                script!(Nop),
                PrivateKey::random(&mut OsRng),
                Default::default(),
                PrivateKey::random(&mut OsRng),
                Covenant::default(),
            )
            .with_change_script(script!(Nop), ExecutionStack::default(), PrivateKey::random(&mut OsRng));

        let mut stp = builder.build::<HashDigest>(&factories, None, u64::MAX).unwrap();

        let outbound_tx1 = OutboundTransaction {
            tx_id: 1u64.into(),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
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

        let outbound_tx2 = OutboundTransactionSql::try_from(OutboundTransaction {
            tx_id: 2u64.into(),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
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
        })
        .unwrap();

        OutboundTransactionSql::try_from(outbound_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();
        outbound_tx2.commit(&conn).unwrap();

        let outbound_txs = OutboundTransactionSql::index_by_cancelled(&conn, false).unwrap();
        assert_eq!(outbound_txs.len(), 2);

        let returned_outbound_tx =
            OutboundTransaction::try_from(OutboundTransactionSql::find_by_cancelled(1.into(), false, &conn).unwrap())
                .unwrap();
        assert_eq!(
            OutboundTransactionSql::try_from(returned_outbound_tx).unwrap(),
            OutboundTransactionSql::try_from(outbound_tx1.clone()).unwrap()
        );

        let rtp = ReceiverTransactionProtocol::new(
            TransactionSenderMessage::Single(Box::new(stp.build_single_round_message().unwrap())),
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
            &factories,
        );

        let inbound_tx1 = InboundTransaction {
            tx_id: 2u64.into(),
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
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
        let inbound_tx2 = InboundTransaction {
            tx_id: 3u64.into(),
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
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

        InboundTransactionSql::try_from(inbound_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();
        InboundTransactionSql::try_from(inbound_tx2)
            .unwrap()
            .commit(&conn)
            .unwrap();

        let inbound_txs = InboundTransactionSql::index_by_cancelled(&conn, false).unwrap();
        assert_eq!(inbound_txs.len(), 2);

        let returned_inbound_tx =
            InboundTransaction::try_from(InboundTransactionSql::find_by_cancelled(2.into(), false, &conn).unwrap())
                .unwrap();
        assert_eq!(
            InboundTransactionSql::try_from(returned_inbound_tx).unwrap(),
            InboundTransactionSql::try_from(inbound_tx1.clone()).unwrap()
        );

        let tx = Transaction::new(
            vec![],
            vec![],
            vec![],
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
        );

        let completed_tx1 = CompletedTransaction {
            tx_id: 2u64.into(),
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
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
        };
        let completed_tx2 = CompletedTransaction {
            tx_id: 3u64.into(),
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
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
        };

        CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();
        assert!(CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .commit(&conn)
            .is_err());

        CompletedTransactionSql::try_from(completed_tx2)
            .unwrap()
            .commit(&conn)
            .unwrap();

        let completed_txs = CompletedTransactionSql::index_by_cancelled(&conn, false).unwrap();
        assert_eq!(completed_txs.len(), 2);

        let returned_completed_tx =
            CompletedTransaction::try_from(CompletedTransactionSql::find_by_cancelled(2.into(), false, &conn).unwrap())
                .unwrap();
        assert_eq!(
            CompletedTransactionSql::try_from(returned_completed_tx).unwrap(),
            CompletedTransactionSql::try_from(completed_tx1.clone()).unwrap()
        );

        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &conn).is_ok());
        InboundTransactionSql::try_from(inbound_tx1.clone())
            .unwrap()
            .delete(&conn)
            .unwrap();
        assert!(InboundTransactionSql::try_from(inbound_tx1.clone())
            .unwrap()
            .delete(&conn)
            .is_err());
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &conn).is_err());

        assert!(OutboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &conn).is_ok());
        OutboundTransactionSql::try_from(outbound_tx1.clone())
            .unwrap()
            .delete(&conn)
            .unwrap();
        assert!(OutboundTransactionSql::try_from(outbound_tx1.clone())
            .unwrap()
            .delete(&conn)
            .is_err());
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, false, &conn).is_err());

        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, false, &conn).is_ok());
        CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .delete(&conn)
            .unwrap();
        assert!(CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .delete(&conn)
            .is_err());
        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, false, &conn).is_err());

        InboundTransactionSql::try_from(inbound_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();

        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, true, &conn).is_err());
        InboundTransactionSql::try_from(inbound_tx1.clone())
            .unwrap()
            .set_cancelled(true, &conn)
            .unwrap();
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &conn).is_err());
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, true, &conn).is_ok());
        InboundTransactionSql::try_from(inbound_tx1.clone())
            .unwrap()
            .set_cancelled(false, &conn)
            .unwrap();
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, true, &conn).is_err());
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &conn).is_ok());
        OutboundTransactionSql::try_from(outbound_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();

        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, true, &conn).is_err());
        OutboundTransactionSql::try_from(outbound_tx1.clone())
            .unwrap()
            .set_cancelled(true, &conn)
            .unwrap();
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, false, &conn).is_err());
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, true, &conn).is_ok());
        OutboundTransactionSql::try_from(outbound_tx1.clone())
            .unwrap()
            .set_cancelled(false, &conn)
            .unwrap();
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, true, &conn).is_err());
        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, false, &conn).is_ok());

        CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();

        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, true, &conn).is_err());
        CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .reject(TxCancellationReason::Unknown, &conn)
            .unwrap();
        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, false, &conn).is_err());
        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, true, &conn).is_ok());

        let coinbase_tx1 = CompletedTransaction {
            tx_id: 101u64.into(),
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
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
        };

        let coinbase_tx2 = CompletedTransaction {
            tx_id: 102u64.into(),
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
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
        };

        let coinbase_tx3 = CompletedTransaction {
            tx_id: 103u64.into(),
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
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
        };

        CompletedTransactionSql::try_from(coinbase_tx1)
            .unwrap()
            .commit(&conn)
            .unwrap();
        CompletedTransactionSql::try_from(coinbase_tx2)
            .unwrap()
            .commit(&conn)
            .unwrap();
        CompletedTransactionSql::try_from(coinbase_tx3)
            .unwrap()
            .commit(&conn)
            .unwrap();

        let coinbase_txs = CompletedTransactionSql::index_coinbase_at_block_height(2, &conn).unwrap();

        assert_eq!(coinbase_txs.len(), 2);
        assert!(coinbase_txs.iter().any(|c| c.tx_id == 101));
        assert!(coinbase_txs.iter().any(|c| c.tx_id == 102));
        assert!(!coinbase_txs.iter().any(|c| c.tx_id == 103));
    }

    #[test]
    fn test_encryption_crud() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        conn.execute("PRAGMA foreign_keys = ON").unwrap();

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

        let inbound_tx = InboundTransaction {
            tx_id: 1u64.into(),
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount: MicroTari::from(100),
            receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
            status: TransactionStatus::Pending,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        };
        let mut inbound_tx_sql = InboundTransactionSql::try_from(inbound_tx.clone()).unwrap();
        inbound_tx_sql.commit(&conn).unwrap();
        inbound_tx_sql.encrypt(&cipher).unwrap();
        inbound_tx_sql.update_encryption(&conn).unwrap();
        let mut db_inbound_tx = InboundTransactionSql::find_by_cancelled(1.into(), false, &conn).unwrap();
        db_inbound_tx.decrypt(&cipher).unwrap();
        let decrypted_inbound_tx = InboundTransaction::try_from(db_inbound_tx).unwrap();
        assert_eq!(inbound_tx, decrypted_inbound_tx);

        let outbound_tx = OutboundTransaction {
            tx_id: 2u64.into(),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount: MicroTari::from(100),
            fee: MicroTari::from(10),
            sender_protocol: SenderTransactionProtocol::new_placeholder(),
            status: TransactionStatus::Pending,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        };

        let mut outbound_tx_sql = OutboundTransactionSql::try_from(outbound_tx.clone()).unwrap();
        outbound_tx_sql.commit(&conn).unwrap();
        outbound_tx_sql.encrypt(&cipher).unwrap();
        outbound_tx_sql.update_encryption(&conn).unwrap();
        let mut db_outbound_tx = OutboundTransactionSql::find_by_cancelled(2.into(), false, &conn).unwrap();
        db_outbound_tx.decrypt(&cipher).unwrap();
        let decrypted_outbound_tx = OutboundTransaction::try_from(db_outbound_tx).unwrap();
        assert_eq!(outbound_tx, decrypted_outbound_tx);

        let completed_tx = CompletedTransaction {
            tx_id: 3u64.into(),
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount: MicroTari::from(100),
            fee: MicroTari::from(100),
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
        };

        let mut completed_tx_sql = CompletedTransactionSql::try_from(completed_tx.clone()).unwrap();
        completed_tx_sql.commit(&conn).unwrap();
        completed_tx_sql.encrypt(&cipher).unwrap();
        completed_tx_sql.update_encryption(&conn).unwrap();
        let mut db_completed_tx = CompletedTransactionSql::find_by_cancelled(3.into(), false, &conn).unwrap();
        db_completed_tx.decrypt(&cipher).unwrap();
        let decrypted_completed_tx = CompletedTransaction::try_from(db_completed_tx).unwrap();
        assert_eq!(completed_tx, decrypted_completed_tx);
    }

    #[test]
    fn test_apply_remove_encryption() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let mut pool = SqliteConnectionPool::new(db_path.clone(), 1, true, true, Duration::from_secs(60));
        pool.create_pool()
            .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));
        // Note: For this test the connection pool is setup with a pool size of one; the pooled connection must go out
        // of scope to be released once obtained otherwise subsequent calls to obtain a pooled connection will fail .
        {
            let conn = pool
                .get_pooled_connection()
                .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

            embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

            let inbound_tx = InboundTransaction {
                tx_id: 1u64.into(),
                source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                amount: MicroTari::from(100),
                receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
                status: TransactionStatus::Pending,
                message: "Yo!".to_string(),
                timestamp: Utc::now().naive_utc(),
                cancelled: false,
                direct_send_success: false,
                send_count: 0,
                last_send_timestamp: None,
            };
            let inbound_tx_sql = InboundTransactionSql::try_from(inbound_tx).unwrap();
            inbound_tx_sql.commit(&conn).unwrap();

            let outbound_tx = OutboundTransaction {
                tx_id: 2u64.into(),
                destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                amount: MicroTari::from(100),
                fee: MicroTari::from(10),
                sender_protocol: SenderTransactionProtocol::new_placeholder(),
                status: TransactionStatus::Pending,
                message: "Yo!".to_string(),
                timestamp: Utc::now().naive_utc(),
                cancelled: false,
                direct_send_success: false,
                send_count: 0,
                last_send_timestamp: None,
            };
            let outbound_tx_sql = OutboundTransactionSql::try_from(outbound_tx).unwrap();
            outbound_tx_sql.commit(&conn).unwrap();

            let completed_tx = CompletedTransaction {
                tx_id: 3u64.into(),
                source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                amount: MicroTari::from(100),
                fee: MicroTari::from(100),
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
            };
            let completed_tx_sql = CompletedTransactionSql::try_from(completed_tx).unwrap();
            completed_tx_sql.commit(&conn).unwrap();
        }

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

        let connection = WalletDbConnection::new(pool, None);

        let db1 = TransactionServiceSqliteDatabase::new(connection.clone(), Some(cipher.clone()));
        assert!(db1.apply_encryption(cipher.clone()).is_err());

        let db2 = TransactionServiceSqliteDatabase::new(connection.clone(), None);
        assert!(db2.remove_encryption().is_ok());
        db2.apply_encryption(cipher).unwrap();
        assert!(db2.fetch(&DbKey::PendingInboundTransactions).is_ok());
        assert!(db2.fetch(&DbKey::PendingOutboundTransactions).is_ok());
        assert!(db2.fetch(&DbKey::CompletedTransactions).is_ok());

        let db3 = TransactionServiceSqliteDatabase::new(connection, None);
        assert!(db3.fetch(&DbKey::PendingInboundTransactions).is_err());
        assert!(db3.fetch(&DbKey::PendingOutboundTransactions).is_err());
        assert!(db3.fetch(&DbKey::CompletedTransactions).is_err());

        db2.remove_encryption().unwrap();

        assert!(db3.fetch(&DbKey::PendingInboundTransactions).is_ok());
        assert!(db3.fetch(&DbKey::PendingOutboundTransactions).is_ok());
        assert!(db3.fetch(&DbKey::CompletedTransactions).is_ok());
    }

    #[test]
    fn test_customized_transactional_queries() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        // Note: For this test the connection pool is setup with a pool size of 2; a pooled connection must go out
        // of scope to be released once obtained otherwise subsequent calls to obtain a pooled connection will fail .
        let mut pool = SqliteConnectionPool::new(db_path.clone(), 2, true, true, Duration::from_secs(60));
        pool.create_pool()
            .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));
        let conn = pool
            .get_pooled_connection()
            .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

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
            let completed_tx = CompletedTransaction {
                tx_id: TxId::from(i),
                source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                amount: MicroTari::from(100),
                fee: MicroTari::from(100),
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
            };
            let completed_tx_sql = CompletedTransactionSql::try_from(completed_tx.clone()).unwrap();
            completed_tx_sql.commit(&conn).unwrap();

            let inbound_tx = InboundTransaction::from(completed_tx);
            let inbound_tx_sql = InboundTransactionSql::try_from(inbound_tx.clone()).unwrap();
            inbound_tx_sql.commit(&conn).unwrap();

            if cancelled.is_none() {
                info_list_reference.push(InboundTransactionSenderInfo {
                    tx_id: inbound_tx.tx_id,
                    source_public_key: inbound_tx.source_public_key,
                })
            }
        }

        let connection = WalletDbConnection::new(pool, None);
        let db1 = TransactionServiceSqliteDatabase::new(connection, None);

        let txn_list = db1.get_transactions_to_be_broadcast().unwrap();
        assert_eq!(txn_list.len(), 335);
        for txn in &txn_list {
            assert!(txn.status == TransactionStatus::Completed || txn.status == TransactionStatus::Broadcast);
            assert!(txn.cancelled.is_none());
            assert!(txn.coinbase_block_height == None || txn.coinbase_block_height == Some(0));
        }

        let info_list = db1.get_pending_inbound_transaction_sender_info().unwrap();
        assert_eq!(info_list.len(), 941);
        assert_eq!(info_list, info_list_reference);
    }
}
