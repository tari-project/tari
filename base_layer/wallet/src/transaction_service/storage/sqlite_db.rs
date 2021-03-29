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
    storage::sqlite_utilities::WalletDbConnection,
    transaction_service::{
        error::TransactionStorageError,
        storage::{
            database::{DbKey, DbKeyValuePair, DbValue, TransactionBackend, WriteOperation},
            models::{
                CompletedTransaction,
                InboundTransaction,
                OutboundTransaction,
                TransactionDirection,
                TransactionStatus,
                WalletTransaction,
            },
        },
    },
    util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
};
use aes_gcm::{self, aead::Error as AeadError, Aes256Gcm};
use chrono::{NaiveDateTime, Utc};
use diesel::{prelude::*, result::Error as DieselError, SqliteConnection};
use log::*;
use std::{
    collections::HashMap,
    convert::TryFrom,
    str::from_utf8,
    sync::{Arc, MutexGuard, RwLock},
};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{tari_amount::MicroTari, types::PublicKey};
use tari_crypto::tari_utilities::{
    hex::{from_hex, Hex},
    ByteArray,
};

const LOG_TARGET: &str = "wallet::transaction_service::database::sqlite_db";

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

    fn insert(&self, kvp: DbKeyValuePair, conn: MutexGuard<SqliteConnection>) -> Result<(), TransactionStorageError> {
        match kvp {
            DbKeyValuePair::PendingOutboundTransaction(k, v) => {
                if OutboundTransactionSql::find_by_cancelled(k, false, &(*conn)).is_ok() {
                    return Err(TransactionStorageError::DuplicateOutput);
                }
                let mut o = OutboundTransactionSql::try_from(*v)?;
                self.encrypt_if_necessary(&mut o)?;
                o.commit(&(*conn))?;
            },
            DbKeyValuePair::PendingInboundTransaction(k, v) => {
                if InboundTransactionSql::find_by_cancelled(k, false, &(*conn)).is_ok() {
                    return Err(TransactionStorageError::DuplicateOutput);
                }
                let mut i = InboundTransactionSql::try_from(*v)?;
                self.encrypt_if_necessary(&mut i)?;

                i.commit(&(*conn))?;
            },
            DbKeyValuePair::CompletedTransaction(k, v) => {
                if CompletedTransactionSql::find_by_cancelled(k, false, &(*conn)).is_ok() {
                    return Err(TransactionStorageError::DuplicateOutput);
                }
                let mut c = CompletedTransactionSql::try_from(*v)?;
                self.encrypt_if_necessary(&mut c)?;

                c.commit(&(*conn))?;
            },
        }
        Ok(())
    }

    fn remove(
        &self,
        key: DbKey,
        conn: MutexGuard<SqliteConnection>,
    ) -> Result<Option<DbValue>, TransactionStorageError>
    {
        match key {
            DbKey::PendingOutboundTransaction(k) => match OutboundTransactionSql::find_by_cancelled(k, false, &(*conn))
            {
                Ok(mut v) => {
                    v.delete(&(*conn))?;
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
            DbKey::PendingInboundTransaction(k) => match InboundTransactionSql::find_by_cancelled(k, false, &(*conn)) {
                Ok(mut v) => {
                    v.delete(&(*conn))?;
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
            DbKey::CompletedTransaction(k) => match CompletedTransactionSql::find_by_cancelled(k, false, &(*conn)) {
                Ok(mut v) => {
                    v.delete(&(*conn))?;
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
                match OutboundTransactionSql::find_by_cancelled(k, true, &(*conn)) {
                    Ok(mut v) => {
                        v.delete(&(*conn))?;
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
                match InboundTransactionSql::find_by_cancelled(k, true, &(*conn)) {
                    Ok(mut v) => {
                        v.delete(&(*conn))?;
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
        let conn = self.database_connection.acquire_lock();

        let result = match key {
            DbKey::PendingOutboundTransaction(t) => {
                match OutboundTransactionSql::find_by_cancelled(*t, false, &(*conn)) {
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
            DbKey::PendingInboundTransaction(t) => {
                match InboundTransactionSql::find_by_cancelled(*t, false, &(*conn)) {
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
            DbKey::CompletedTransaction(t) => match CompletedTransactionSql::find(*t, &(*conn)) {
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
                match OutboundTransactionSql::find(*t, &(*conn)) {
                    Ok(mut o) => {
                        self.decrypt_if_necessary(&mut o)?;

                        return Ok(Some(DbValue::WalletTransaction(Box::new(
                            WalletTransaction::PendingOutbound(OutboundTransaction::try_from(o)?),
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                };
                match InboundTransactionSql::find(*t, &(*conn)) {
                    Ok(mut i) => {
                        self.decrypt_if_necessary(&mut i)?;
                        return Ok(Some(DbValue::WalletTransaction(Box::new(
                            WalletTransaction::PendingInbound(InboundTransaction::try_from(i)?),
                        ))));
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                };
                match CompletedTransactionSql::find(*t, &(*conn)) {
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
                for o in OutboundTransactionSql::index_by_cancelled(&(*conn), false)?.iter_mut() {
                    self.decrypt_if_necessary(o)?;
                    result.insert(o.tx_id as u64, OutboundTransaction::try_from((*o).clone())?);
                }

                Some(DbValue::PendingOutboundTransactions(result))
            },
            DbKey::PendingInboundTransactions => {
                let mut result = HashMap::new();
                for i in InboundTransactionSql::index_by_cancelled(&(*conn), false)?.iter_mut() {
                    self.decrypt_if_necessary(i)?;
                    result.insert(i.tx_id as u64, InboundTransaction::try_from((*i).clone())?);
                }

                Some(DbValue::PendingInboundTransactions(result))
            },
            DbKey::CompletedTransactions => {
                let mut result = HashMap::new();
                for c in CompletedTransactionSql::index_by_cancelled(&(*conn), false)?.iter_mut() {
                    self.decrypt_if_necessary(c)?;
                    result.insert(c.tx_id as u64, CompletedTransaction::try_from((*c).clone())?);
                }

                Some(DbValue::CompletedTransactions(result))
            },
            DbKey::CancelledPendingOutboundTransactions => {
                let mut result = HashMap::new();
                for o in OutboundTransactionSql::index_by_cancelled(&(*conn), true)?.iter_mut() {
                    self.decrypt_if_necessary(o)?;
                    result.insert(o.tx_id as u64, OutboundTransaction::try_from((*o).clone())?);
                }

                Some(DbValue::PendingOutboundTransactions(result))
            },
            DbKey::CancelledPendingInboundTransactions => {
                let mut result = HashMap::new();
                for i in InboundTransactionSql::index_by_cancelled(&(*conn), true)?.iter_mut() {
                    self.decrypt_if_necessary(i)?;
                    result.insert(i.tx_id as u64, InboundTransaction::try_from((*i).clone())?);
                }

                Some(DbValue::PendingInboundTransactions(result))
            },
            DbKey::CancelledCompletedTransactions => {
                let mut result = HashMap::new();
                for c in CompletedTransactionSql::index_by_cancelled(&(*conn), true)?.iter_mut() {
                    self.decrypt_if_necessary(c)?;
                    result.insert(c.tx_id as u64, CompletedTransaction::try_from((*c).clone())?);
                }

                Some(DbValue::CompletedTransactions(result))
            },
            DbKey::CancelledPendingOutboundTransaction(t) => {
                match OutboundTransactionSql::find_by_cancelled(*t, true, &(*conn)) {
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
                match InboundTransactionSql::find_by_cancelled(*t, true, &(*conn)) {
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

        Ok(result)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();

        let result = match key {
            DbKey::PendingOutboundTransaction(k) => {
                OutboundTransactionSql::find_by_cancelled(*k, false, &(*conn)).is_ok()
            },
            DbKey::PendingInboundTransaction(k) => {
                InboundTransactionSql::find_by_cancelled(*k, false, &(*conn)).is_ok()
            },
            DbKey::CompletedTransaction(k) => CompletedTransactionSql::find(*k, &(*conn)).is_ok(),
            DbKey::PendingOutboundTransactions => false,
            DbKey::PendingInboundTransactions => false,
            DbKey::CompletedTransactions => false,
            DbKey::CancelledPendingOutboundTransactions => false,
            DbKey::CancelledPendingInboundTransactions => false,
            DbKey::CancelledCompletedTransactions => false,
            DbKey::CancelledPendingOutboundTransaction(k) => {
                OutboundTransactionSql::find_by_cancelled(*k, true, &(*conn)).is_ok()
            },
            DbKey::CancelledPendingInboundTransaction(k) => {
                InboundTransactionSql::find_by_cancelled(*k, true, &(*conn)).is_ok()
            },
            DbKey::AnyTransaction(k) => {
                CompletedTransactionSql::find(*k, &(*conn)).is_ok() ||
                    InboundTransactionSql::find(*k, &(*conn)).is_ok() ||
                    OutboundTransactionSql::find(*k, &(*conn)).is_ok()
            },
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();

        match op {
            WriteOperation::Insert(kvp) => self.insert(kvp, conn).map(|_| None),
            WriteOperation::Remove(key) => self.remove(key, conn),
        }
    }

    fn transaction_exists(&self, tx_id: u64) -> Result<bool, TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();

        Ok(
            OutboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)).is_ok() ||
                InboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)).is_ok() ||
                CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)).is_ok(),
        )
    }

    fn get_pending_transaction_counterparty_pub_key_by_tx_id(
        &self,
        tx_id: u64,
    ) -> Result<CommsPublicKey, TransactionStorageError>
    {
        let conn = self.database_connection.acquire_lock();

        if let Ok(mut outbound_tx_sql) = OutboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            self.decrypt_if_necessary(&mut outbound_tx_sql)?;
            let outbound_tx = OutboundTransaction::try_from(outbound_tx_sql)?;
            return Ok(outbound_tx.destination_public_key);
        }
        if let Ok(mut inbound_tx_sql) = InboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            self.decrypt_if_necessary(&mut inbound_tx_sql)?;
            let inbound_tx = InboundTransaction::try_from(inbound_tx_sql)?;
            return Ok(inbound_tx.source_public_key);
        }

        Err(TransactionStorageError::ValuesNotFound)
    }

    fn complete_outbound_transaction(
        &self,
        tx_id: u64,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let conn = self.database_connection.acquire_lock();

        if CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)).is_ok() {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }

        match OutboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                let mut completed_tx_sql = CompletedTransactionSql::try_from(completed_transaction)?;
                self.encrypt_if_necessary(&mut completed_tx_sql)?;
                v.delete(&(*conn))?;
                completed_tx_sql.commit(&(*conn))?;
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
        &self,
        tx_id: u64,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let conn = self.database_connection.acquire_lock();

        if CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)).is_ok() {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }

        match InboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                let mut completed_tx_sql = CompletedTransactionSql::try_from(completed_transaction)?;
                self.encrypt_if_necessary(&mut completed_tx_sql)?;
                v.delete(&(*conn))?;
                completed_tx_sql.commit(&(*conn))?;
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

    fn broadcast_completed_transaction(&self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();

        match CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                if TransactionStatus::try_from(v.status)? == TransactionStatus::Completed {
                    v.update(
                        UpdateCompletedTransactionSql::from(UpdateCompletedTransaction {
                            status: Some(TransactionStatus::Broadcast),
                            timestamp: None,
                            cancelled: None,
                            direction: None,
                            send_count: None,
                            last_send_timestamp: None,
                            valid: None,
                        }),
                        &(*conn),
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
        Ok(())
    }

    fn mine_completed_transaction(&self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();

        match CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                v.update(
                    UpdateCompletedTransactionSql::from(UpdateCompletedTransaction {
                        status: Some(TransactionStatus::MinedUnconfirmed),
                        timestamp: None,
                        cancelled: None,
                        direction: None,
                        send_count: None,
                        last_send_timestamp: None,
                        valid: None,
                    }),
                    &(*conn),
                )?;
            },
            Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                    tx_id,
                )))
            },
            Err(e) => return Err(e),
        };
        Ok(())
    }

    fn cancel_completed_transaction(&self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();
        match CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                v.cancel(&(*conn))?;
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

    fn cancel_pending_transaction(&self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();
        match InboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                v.cancel(&(*conn))?;
            },
            Err(_) => {
                match OutboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
                    Ok(v) => {
                        v.cancel(&(*conn))?;
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                        return Err(TransactionStorageError::ValuesNotFound);
                    },
                    Err(e) => return Err(e),
                };
            },
        };
        Ok(())
    }

    fn mark_direct_send_success(&self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();
        match InboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                v.update(
                    UpdateInboundTransactionSql {
                        cancelled: None,
                        direct_send_success: Some(1i32),
                        receiver_protocol: None,
                        send_count: None,
                        last_send_timestamp: None,
                    },
                    &(*conn),
                )?;
            },
            Err(_) => {
                match OutboundTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
                    Ok(v) => {
                        v.update(
                            UpdateOutboundTransactionSql {
                                cancelled: None,
                                direct_send_success: Some(1i32),
                                sender_protocol: None,
                                send_count: None,
                                last_send_timestamp: None,
                            },
                            &(*conn),
                        )?;
                    },
                    Err(TransactionStorageError::DieselError(DieselError::NotFound)) => {
                        return Err(TransactionStorageError::ValuesNotFound);
                    },
                    Err(e) => return Err(e),
                };
            },
        };
        Ok(())
    }

    #[cfg(feature = "test_harness")]
    fn update_completed_transaction_timestamp(
        &self,
        tx_id: u64,
        timestamp: NaiveDateTime,
    ) -> Result<(), TransactionStorageError>
    {
        let conn = self.database_connection.acquire_lock();

        if let Ok(tx) = CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            tx.update(
                UpdateCompletedTransactionSql::from(UpdateCompletedTransaction {
                    status: None,
                    timestamp: Some(timestamp),
                    cancelled: None,
                    direction: None,
                    send_count: None,
                    last_send_timestamp: None,
                    valid: None,
                }),
                &(*conn),
            )?;
        }

        Ok(())
    }

    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), TransactionStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);

        if (*current_cipher).is_some() {
            return Err(TransactionStorageError::AlreadyEncrypted);
        }

        let conn = self.database_connection.acquire_lock();

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

        Ok(())
    }

    fn remove_encryption(&self) -> Result<(), TransactionStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);

        let cipher = if let Some(cipher) = (*current_cipher).clone().take() {
            cipher
        } else {
            return Ok(());
        };
        let conn = self.database_connection.acquire_lock();

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

        Ok(())
    }

    fn cancel_coinbase_transaction_at_block_height(&self, block_height: u64) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();

        let coinbase_txs = CompletedTransactionSql::index_coinbase_at_block_height(block_height as i64, &conn)?;
        for c in coinbase_txs.iter() {
            c.cancel(&conn)?;
        }

        Ok(())
    }

    fn find_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
        amount: MicroTari,
    ) -> Result<Option<CompletedTransaction>, TransactionStorageError>
    {
        let conn = self.database_connection.acquire_lock();

        let mut coinbase_txs = CompletedTransactionSql::index_coinbase_at_block_height(block_height as i64, &conn)?;
        for c in coinbase_txs.iter_mut() {
            self.decrypt_if_necessary(c)?;
            let completed_tx = CompletedTransaction::try_from(c.clone()).map_err(|_| {
                TransactionStorageError::ConversionError("Error converting to CompletedTransaction".to_string())
            })?;

            if completed_tx.amount == amount {
                return Ok(Some(completed_tx));
            }
        }

        Ok(None)
    }

    fn increment_send_count(&self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();

        if let Ok(tx) = CompletedTransactionSql::find(tx_id, &conn) {
            let update = UpdateCompletedTransactionSql {
                status: None,
                timestamp: None,
                cancelled: None,
                direction: None,
                transaction_protocol: None,
                send_count: Some(tx.send_count + 1),
                last_send_timestamp: Some(Some(Utc::now().naive_utc())),
                valid: None,
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

        Ok(())
    }

    fn confirm_broadcast_or_coinbase_transaction(&self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();
        match CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                if v.status == TransactionStatus::MinedUnconfirmed as i32 ||
                    v.status == TransactionStatus::MinedConfirmed as i32 ||
                    v.status == TransactionStatus::Broadcast as i32 ||
                    v.status == TransactionStatus::Coinbase as i32
                {
                    v.confirm(&(*conn))?;
                } else {
                    return Err(TransactionStorageError::TransactionNotMined(tx_id));
                }
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

    fn unconfirm_mined_transaction(&self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();
        match CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                if v.status == TransactionStatus::MinedUnconfirmed as i32 ||
                    v.status == TransactionStatus::MinedConfirmed as i32
                {
                    v.unconfirm(&(*conn))?;
                } else {
                    return Err(TransactionStorageError::TransactionNotMined(tx_id));
                }
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

    fn set_completed_transaction_validity(&self, tx_id: u64, valid: bool) -> Result<(), TransactionStorageError> {
        let conn = self.database_connection.acquire_lock();
        match CompletedTransactionSql::find_by_cancelled(tx_id, false, &(*conn)) {
            Ok(v) => {
                v.set_validity(valid, &(*conn))?;
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
    ) -> Result<Vec<InboundTransactionSql>, TransactionStorageError>
    {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::cancelled.eq(cancelled as i32))
            .load::<InboundTransactionSql>(conn)?)
    }

    pub fn find(tx_id: TxId, conn: &SqliteConnection) -> Result<InboundTransactionSql, TransactionStorageError> {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::tx_id.eq(tx_id as i64))
            .first::<InboundTransactionSql>(conn)?)
    }

    pub fn find_by_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &SqliteConnection,
    ) -> Result<InboundTransactionSql, TransactionStorageError>
    {
        Ok(inbound_transactions::table
            .filter(inbound_transactions::tx_id.eq(tx_id as i64))
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
    ) -> Result<(), TransactionStorageError>
    {
        let num_updated =
            diesel::update(inbound_transactions::table.filter(inbound_transactions::tx_id.eq(&self.tx_id)))
                .set(update)
                .execute(conn)?;

        if num_updated == 0 {
            return Err(TransactionStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(())
    }

    pub fn cancel(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateInboundTransactionSql {
                cancelled: Some(1i32),
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
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        let encrypted_protocol =
            encrypt_bytes_integral_nonce(&cipher, self.receiver_protocol.clone().as_bytes().to_vec())?;
        self.receiver_protocol = encrypted_protocol.to_hex();
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        let decrypted_protocol = decrypt_bytes_integral_nonce(
            &cipher,
            from_hex(self.receiver_protocol.as_str()).map_err(|_| aes_gcm::Error)?,
        )?;
        self.receiver_protocol = from_utf8(decrypted_protocol.as_slice())
            .map_err(|_| aes_gcm::Error)?
            .to_string();
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
            tx_id: i.tx_id as u64,
            source_public_key: PublicKey::from_vec(&i.source_public_key)
                .map_err(|_| TransactionStorageError::ConversionError("Invalid Source Publickey".to_string()))?,
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
    ) -> Result<Vec<OutboundTransactionSql>, TransactionStorageError>
    {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::cancelled.eq(cancelled as i32))
            .load::<OutboundTransactionSql>(conn)?)
    }

    pub fn find(tx_id: TxId, conn: &SqliteConnection) -> Result<OutboundTransactionSql, TransactionStorageError> {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::tx_id.eq(tx_id as i64))
            .first::<OutboundTransactionSql>(conn)?)
    }

    pub fn find_by_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &SqliteConnection,
    ) -> Result<OutboundTransactionSql, TransactionStorageError>
    {
        Ok(outbound_transactions::table
            .filter(outbound_transactions::tx_id.eq(tx_id as i64))
            .filter(outbound_transactions::cancelled.eq(cancelled as i32))
            .first::<OutboundTransactionSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        let num_deleted =
            diesel::delete(outbound_transactions::table.filter(outbound_transactions::tx_id.eq(&self.tx_id)))
                .execute(conn)?;

        if num_deleted == 0 {
            return Err(TransactionStorageError::ValuesNotFound);
        }

        Ok(())
    }

    pub fn update(
        &self,
        update: UpdateOutboundTransactionSql,
        conn: &SqliteConnection,
    ) -> Result<(), TransactionStorageError>
    {
        let num_updated =
            diesel::update(outbound_transactions::table.filter(outbound_transactions::tx_id.eq(&self.tx_id)))
                .set(update)
                .execute(conn)?;

        if num_updated == 0 {
            return Err(TransactionStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(())
    }

    pub fn cancel(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateOutboundTransactionSql {
                cancelled: Some(1i32),
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
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        let encrypted_protocol =
            encrypt_bytes_integral_nonce(&cipher, self.sender_protocol.clone().as_bytes().to_vec())?;
        self.sender_protocol = encrypted_protocol.to_hex();
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        let decrypted_protocol = decrypt_bytes_integral_nonce(
            &cipher,
            from_hex(self.sender_protocol.as_str()).map_err(|_| aes_gcm::Error)?,
        )?;
        self.sender_protocol = from_utf8(decrypted_protocol.as_slice())
            .map_err(|_| aes_gcm::Error)?
            .to_string();
        Ok(())
    }
}

impl TryFrom<OutboundTransaction> for OutboundTransactionSql {
    type Error = TransactionStorageError;

    fn try_from(o: OutboundTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: o.tx_id as i64,
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
            tx_id: o.tx_id as u64,
            destination_public_key: PublicKey::from_vec(&o.destination_public_key)
                .map_err(|_| TransactionStorageError::ConversionError("Invalid destination PublicKey".to_string()))?,
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
    cancelled: i32,
    direction: Option<i32>,
    coinbase_block_height: Option<i64>,
    send_count: i32,
    last_send_timestamp: Option<NaiveDateTime>,
    valid: i32,
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
    ) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError>
    {
        Ok(completed_transactions::table
            .filter(completed_transactions::cancelled.eq(cancelled as i32))
            .load::<CompletedTransactionSql>(conn)?)
    }

    pub fn index_coinbase_at_block_height(
        block_height: i64,
        conn: &SqliteConnection,
    ) -> Result<Vec<CompletedTransactionSql>, TransactionStorageError>
    {
        Ok(completed_transactions::table
            .filter(completed_transactions::status.eq(TransactionStatus::Coinbase as i32))
            .filter(completed_transactions::coinbase_block_height.eq(block_height))
            .load::<CompletedTransactionSql>(conn)?)
    }

    pub fn find(tx_id: TxId, conn: &SqliteConnection) -> Result<CompletedTransactionSql, TransactionStorageError> {
        Ok(completed_transactions::table
            .filter(completed_transactions::tx_id.eq(tx_id as i64))
            .first::<CompletedTransactionSql>(conn)?)
    }

    pub fn find_by_cancelled(
        tx_id: TxId,
        cancelled: bool,
        conn: &SqliteConnection,
    ) -> Result<CompletedTransactionSql, TransactionStorageError>
    {
        Ok(completed_transactions::table
            .filter(completed_transactions::tx_id.eq(tx_id as i64))
            .filter(completed_transactions::cancelled.eq(cancelled as i32))
            .first::<CompletedTransactionSql>(conn)?)
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
    ) -> Result<(), TransactionStorageError>
    {
        let num_updated =
            diesel::update(completed_transactions::table.filter(completed_transactions::tx_id.eq(&self.tx_id)))
                .set(updated_tx)
                .execute(conn)?;

        if num_updated == 0 {
            return Err(TransactionStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(())
    }

    pub fn cancel(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateCompletedTransactionSql {
                status: None,
                timestamp: None,
                cancelled: Some(1i32),
                direction: None,
                transaction_protocol: None,
                send_count: None,
                last_send_timestamp: None,
                valid: None,
            },
            conn,
        )?;

        Ok(())
    }

    pub fn confirm(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateCompletedTransactionSql {
                status: Some(TransactionStatus::MinedConfirmed as i32),
                timestamp: None,
                cancelled: None,
                direction: None,
                transaction_protocol: None,
                send_count: None,
                last_send_timestamp: None,
                valid: None,
            },
            conn,
        )?;

        Ok(())
    }

    pub fn unconfirm(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateCompletedTransactionSql {
                status: Some(TransactionStatus::MinedUnconfirmed as i32),
                timestamp: None,
                cancelled: None,
                direction: None,
                transaction_protocol: None,
                send_count: None,
                last_send_timestamp: None,
                valid: None,
            },
            conn,
        )?;

        Ok(())
    }

    pub fn set_validity(&self, valid: bool, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateCompletedTransactionSql {
                status: None,
                timestamp: None,
                cancelled: None,
                direction: None,
                transaction_protocol: None,
                send_count: None,
                last_send_timestamp: None,
                valid: Some(valid as i32),
            },
            conn,
        )?;

        Ok(())
    }

    pub fn update_encryption(&self, conn: &SqliteConnection) -> Result<(), TransactionStorageError> {
        self.update(
            UpdateCompletedTransactionSql {
                status: None,
                timestamp: None,
                cancelled: None,
                direction: None,
                transaction_protocol: Some(self.transaction_protocol.clone()),
                send_count: None,
                last_send_timestamp: None,
                valid: None,
            },
            conn,
        )?;

        Ok(())
    }
}

impl Encryptable<Aes256Gcm> for CompletedTransactionSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        let encrypted_protocol =
            encrypt_bytes_integral_nonce(&cipher, self.transaction_protocol.clone().as_bytes().to_vec())?;
        self.transaction_protocol = encrypted_protocol.to_hex();
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        let decrypted_protocol = decrypt_bytes_integral_nonce(
            &cipher,
            from_hex(self.transaction_protocol.as_str()).map_err(|_| aes_gcm::Error)?,
        )?;
        self.transaction_protocol = from_utf8(decrypted_protocol.as_slice())
            .map_err(|_| aes_gcm::Error)?
            .to_string();
        Ok(())
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
            cancelled: c.cancelled as i32,
            direction: Some(c.direction as i32),
            coinbase_block_height: c.coinbase_block_height.map(|b| b as i64),
            send_count: c.send_count as i32,
            last_send_timestamp: c.last_send_timestamp,
            valid: c.valid as i32,
        })
    }
}

impl TryFrom<CompletedTransactionSql> for CompletedTransaction {
    type Error = TransactionStorageError;

    fn try_from(c: CompletedTransactionSql) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: c.tx_id as u64,
            source_public_key: PublicKey::from_vec(&c.source_public_key)
                .map_err(|_| TransactionStorageError::ConversionError("Invalid source Publickey".to_string()))?,
            destination_public_key: PublicKey::from_vec(&c.destination_public_key)
                .map_err(|_| TransactionStorageError::ConversionError("Invalid destination PublicKey".to_string()))?,
            amount: MicroTari::from(c.amount as u64),
            fee: MicroTari::from(c.fee as u64),
            transaction: serde_json::from_str(&c.transaction_protocol)?,
            status: TransactionStatus::try_from(c.status)?,
            message: c.message,
            timestamp: c.timestamp,
            cancelled: c.cancelled != 0,
            direction: TransactionDirection::try_from(c.direction.unwrap_or(2i32))?,
            coinbase_block_height: c.coinbase_block_height.map(|b| b as u64),
            send_count: c.send_count as u32,
            last_send_timestamp: c.last_send_timestamp,
            valid: c.valid != 0,
        })
    }
}

/// These are the fields that can be updated for a Completed Transaction
pub struct UpdateCompletedTransaction {
    status: Option<TransactionStatus>,
    timestamp: Option<NaiveDateTime>,
    cancelled: Option<bool>,
    direction: Option<TransactionDirection>,
    send_count: Option<u32>,
    last_send_timestamp: Option<Option<NaiveDateTime>>,
    valid: Option<bool>,
}

#[derive(AsChangeset)]
#[table_name = "completed_transactions"]
pub struct UpdateCompletedTransactionSql {
    status: Option<i32>,
    timestamp: Option<NaiveDateTime>,
    cancelled: Option<i32>,
    direction: Option<i32>,
    transaction_protocol: Option<String>,
    send_count: Option<i32>,
    last_send_timestamp: Option<Option<NaiveDateTime>>,
    valid: Option<i32>,
}

/// Map a Rust friendly UpdateCompletedTransaction to the Sql data type form
impl From<UpdateCompletedTransaction> for UpdateCompletedTransactionSql {
    fn from(u: UpdateCompletedTransaction) -> Self {
        Self {
            status: u.status.map(|s| s as i32),
            timestamp: u.timestamp,
            cancelled: u.cancelled.map(|c| c as i32),
            direction: u.direction.map(|d| d as i32),
            transaction_protocol: None,
            send_count: u.send_count.map(|c| c as i32),
            last_send_timestamp: u.last_send_timestamp,
            valid: u.valid.map(|c| c as i32),
        }
    }
}

#[cfg(test)]
mod test {
    #[cfg(feature = "test_harness")]
    use crate::transaction_service::storage::sqlite_db::UpdateCompletedTransactionSql;
    use crate::{
        storage::sqlite_utilities::WalletDbConnection,
        transaction_service::storage::{
            database::{DbKey, TransactionBackend},
            models::{
                CompletedTransaction,
                InboundTransaction,
                OutboundTransaction,
                TransactionDirection,
                TransactionStatus,
            },
            sqlite_db::{
                CompletedTransactionSql,
                InboundTransactionSql,
                OutboundTransactionSql,
                TransactionServiceSqliteDatabase,
            },
        },
        util::encryption::Encryptable,
    };
    use aes_gcm::{
        aead::{generic_array::GenericArray, NewAead},
        Aes256Gcm,
    };
    use chrono::Utc;
    use diesel::{Connection, SqliteConnection};
    use rand::rngs::OsRng;
    use std::convert::TryFrom;
    use tari_core::transactions::{
        tari_amount::MicroTari,
        transaction::{OutputFeatures, Transaction, UnblindedOutput},
        transaction_protocol::sender::TransactionSenderMessage,
        types::{CryptoFactories, HashDigest, PrivateKey, PublicKey},
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    };
    use tari_crypto::{
        keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
        script::{ExecutionStack, TariScript},
    };
    use tari_test_utils::random::string;
    use tempfile::tempdir;

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

        let mut builder = SenderTransactionProtocol::builder(1);
        let amount = MicroTari::from(10_000);
        let input = UnblindedOutput::new(
            MicroTari::from(100_000),
            PrivateKey::random(&mut OsRng),
            None,
            TariScript::default(),
            ExecutionStack::default(),
            0,
            PrivateKey::default(),
            PublicKey::default(),
        );
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari::from(177))
            .with_offset(PrivateKey::random(&mut OsRng))
            .with_private_nonce(PrivateKey::random(&mut OsRng))
            .with_amount(0, amount)
            .with_message("Yo!".to_string())
            .with_input(input.as_transaction_input(&factories.commitment), input)
            .with_change_secret(PrivateKey::random(&mut OsRng));

        let mut stp = builder.build::<HashDigest>(&factories).unwrap();

        let outbound_tx1 = OutboundTransaction {
            tx_id: 1u64,
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
            tx_id: 2u64,
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
            OutboundTransaction::try_from(OutboundTransactionSql::find_by_cancelled(1u64, false, &conn).unwrap())
                .unwrap();
        assert_eq!(
            OutboundTransactionSql::try_from(returned_outbound_tx).unwrap(),
            OutboundTransactionSql::try_from(outbound_tx1.clone()).unwrap()
        );

        let rtp = ReceiverTransactionProtocol::new(
            TransactionSenderMessage::Single(Box::new(stp.build_single_round_message().unwrap())),
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
            OutputFeatures::default(),
            &factories,
        );

        let inbound_tx1 = InboundTransaction {
            tx_id: 2,
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
            tx_id: 3,
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
            InboundTransaction::try_from(InboundTransactionSql::find_by_cancelled(2u64, false, &conn).unwrap())
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
            tx_id: 2,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::MinedUnconfirmed,
            message: "Yo!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
            valid: true,
        };
        let completed_tx2 = CompletedTransaction {
            tx_id: 3,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::Broadcast,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
            valid: true,
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

        let completed_txs = CompletedTransactionSql::index_by_cancelled(&conn, false).unwrap();
        assert_eq!(completed_txs.len(), 2);

        let returned_completed_tx =
            CompletedTransaction::try_from(CompletedTransactionSql::find_by_cancelled(2u64, false, &conn).unwrap())
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
            .cancel(&conn)
            .unwrap();
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &conn).is_err());
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, true, &conn).is_ok());

        OutboundTransactionSql::try_from(outbound_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();

        assert!(OutboundTransactionSql::find_by_cancelled(outbound_tx1.tx_id, true, &conn).is_err());
        OutboundTransactionSql::try_from(outbound_tx1)
            .unwrap()
            .cancel(&conn)
            .unwrap();
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, false, &conn).is_err());
        assert!(InboundTransactionSql::find_by_cancelled(inbound_tx1.tx_id, true, &conn).is_ok());

        CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .commit(&conn)
            .unwrap();

        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, true, &conn).is_err());
        CompletedTransactionSql::try_from(completed_tx1.clone())
            .unwrap()
            .cancel(&conn)
            .unwrap();
        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, false, &conn).is_err());
        assert!(CompletedTransactionSql::find_by_cancelled(completed_tx1.tx_id, true, &conn).is_ok());

        let coinbase_tx1 = CompletedTransaction {
            tx_id: 101,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::Coinbase,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: Some(2),
            send_count: 0,
            last_send_timestamp: None,
            valid: true,
        };

        let coinbase_tx2 = CompletedTransaction {
            tx_id: 102,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
            transaction: tx.clone(),
            status: TransactionStatus::Coinbase,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: Some(2),
            send_count: 0,
            last_send_timestamp: None,
            valid: true,
        };

        let coinbase_tx3 = CompletedTransaction {
            tx_id: 103,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount,
            fee: MicroTari::from(100),
            transaction: tx,
            status: TransactionStatus::Coinbase,
            message: "Hey!".to_string(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: Some(3),
            send_count: 0,
            last_send_timestamp: None,
            valid: true,
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

        #[cfg(feature = "test_harness")]
        CompletedTransactionSql::find_by_cancelled(completed_tx2.tx_id, false, &conn)
            .unwrap()
            .update(
                UpdateCompletedTransactionSql {
                    status: Some(TransactionStatus::MinedUnconfirmed as i32),
                    timestamp: None,
                    cancelled: None,
                    direction: None,
                    transaction_protocol: None,
                    send_count: None,
                    last_send_timestamp: None,
                    valid: None,
                },
                &conn,
            )
            .unwrap();
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
            tx_id: 1,
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
        let mut db_inbound_tx = InboundTransactionSql::find_by_cancelled(1, false, &conn).unwrap();
        db_inbound_tx.decrypt(&cipher).unwrap();
        let decrypted_inbound_tx = InboundTransaction::try_from(db_inbound_tx).unwrap();
        assert_eq!(inbound_tx, decrypted_inbound_tx);

        let outbound_tx = OutboundTransaction {
            tx_id: 2u64,
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
        let mut db_outbound_tx = OutboundTransactionSql::find_by_cancelled(2, false, &conn).unwrap();
        db_outbound_tx.decrypt(&cipher).unwrap();
        let decrypted_outbound_tx = OutboundTransaction::try_from(db_outbound_tx).unwrap();
        assert_eq!(outbound_tx, decrypted_outbound_tx);

        let completed_tx = CompletedTransaction {
            tx_id: 3,
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
            cancelled: false,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
            valid: true,
        };

        let mut completed_tx_sql = CompletedTransactionSql::try_from(completed_tx.clone()).unwrap();
        completed_tx_sql.commit(&conn).unwrap();
        completed_tx_sql.encrypt(&cipher).unwrap();
        completed_tx_sql.update_encryption(&conn).unwrap();
        let mut db_completed_tx = CompletedTransactionSql::find_by_cancelled(3, false, &conn).unwrap();
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
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        let inbound_tx = InboundTransaction {
            tx_id: 1,
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
            tx_id: 2u64,
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
            tx_id: 3,
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
            cancelled: false,
            direction: TransactionDirection::Unknown,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
            valid: true,
        };
        let completed_tx_sql = CompletedTransactionSql::try_from(completed_tx).unwrap();
        completed_tx_sql.commit(&conn).unwrap();

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

        let connection = WalletDbConnection::new(conn, None);

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
}
