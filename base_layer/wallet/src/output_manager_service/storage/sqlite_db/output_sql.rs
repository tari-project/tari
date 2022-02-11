//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::convert::{TryFrom, TryInto};

use aes_gcm::Aes256Gcm;
use diesel::{prelude::*, sql_query, SqliteConnection};
use log::*;
use tari_common_types::{
    transaction::TxId,
    types::{ComSignature, Commitment, PrivateKey, PublicKey},
};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{OutputFeatures, OutputFlags, UnblindedOutput},
        CryptoFactories,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    script::{ExecutionStack, TariScript},
    tari_utilities::ByteArray,
};
use tari_utilities::hash::Hashable;

use crate::{
    output_manager_service::{
        error::OutputManagerStorageError,
        service::{Balance, UTXOSelectionStrategy},
        storage::{
            models::DbUnblindedOutput,
            sqlite_db::{UpdateOutput, UpdateOutputSql},
            OutputStatus,
        },
    },
    schema::outputs,
    util::{
        diesel_ext::ExpectedRowsExtension,
        encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    },
};

const LOG_TARGET: &str = "wallet::output_manager_service::database::wallet";

#[derive(Clone, Debug, Queryable, Identifiable, PartialEq, QueryableByName)]
#[table_name = "outputs"]
pub struct OutputSql {
    pub id: i32, // Auto inc primary key
    pub commitment: Option<Vec<u8>>,
    pub spending_key: Vec<u8>,
    pub value: i64,
    pub flags: i32,
    pub maturity: i64,
    pub status: i32,
    pub hash: Option<Vec<u8>>,
    pub script: Vec<u8>,
    pub input_data: Vec<u8>,
    pub script_private_key: Vec<u8>,
    pub sender_offset_public_key: Vec<u8>,
    pub metadata_signature_nonce: Vec<u8>,
    pub metadata_signature_u_key: Vec<u8>,
    pub metadata_signature_v_key: Vec<u8>,
    pub mined_height: Option<i64>,
    pub mined_in_block: Option<Vec<u8>>,
    pub mined_mmr_position: Option<i64>,
    pub marked_deleted_at_height: Option<i64>,
    pub marked_deleted_in_block: Option<Vec<u8>>,
    pub received_in_tx_id: Option<i64>,
    pub spent_in_tx_id: Option<i64>,
    pub coinbase_block_height: Option<i64>,
    pub metadata: Option<Vec<u8>>,
    pub features_parent_public_key: Option<Vec<u8>>,
    pub features_unique_id: Option<Vec<u8>>,
    pub script_lock_height: i64,
    pub spending_priority: i32,
    pub features_json: String,
    pub covenant: Vec<u8>,
}

impl OutputSql {
    /// Return all outputs
    pub fn index(conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table.load::<OutputSql>(conn)?)
    }

    /// Return all outputs with a given status
    pub fn index_status(
        status: OutputStatus,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table.filter(outputs::status.eq(status as i32)).load(conn)?)
    }

    /// Retrieves UTXOs than can be spent, sorted by priority, then value from smallest to largest.
    pub fn fetch_unspent_outputs_for_spending(
        mut strategy: UTXOSelectionStrategy,
        amount: u64,
        tip_height: i64,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        if strategy == UTXOSelectionStrategy::Default {
            // lets get the max value for all utxos
            let max: Vec<i64> = outputs::table
                .filter(outputs::status.eq(OutputStatus::Unspent as i32))
                .filter(outputs::script_lock_height.le(tip_height))
                .filter(outputs::maturity.le(tip_height))
                .filter(outputs::features_unique_id.is_null())
                .filter(outputs::features_parent_public_key.is_null())
                .order(outputs::value.desc())
                .select(outputs::value)
                .limit(1)
                .load(conn)?;
            if max.is_empty() {
                strategy = UTXOSelectionStrategy::Smallest
            } else if amount > max[0] as u64 {
                strategy = UTXOSelectionStrategy::Largest
            } else {
                strategy = UTXOSelectionStrategy::MaturityThenSmallest
            }
        }

        let mut query = outputs::table
            .into_boxed()
            .filter(outputs::status.eq(OutputStatus::Unspent as i32))
            .filter(outputs::script_lock_height.le(tip_height))
            .filter(outputs::maturity.le(tip_height))
            .filter(outputs::features_unique_id.is_null())
            .filter(outputs::features_parent_public_key.is_null())
            .order_by(outputs::spending_priority.desc());
        match strategy {
            UTXOSelectionStrategy::Smallest => {
                query = query.then_order_by(outputs::value.asc());
            },
            UTXOSelectionStrategy::MaturityThenSmallest => {
                query = query
                    .then_order_by(outputs::maturity.asc())
                    .then_order_by(outputs::value.asc());
            },
            UTXOSelectionStrategy::Largest => {
                query = query.then_order_by(outputs::value.desc());
            },
            UTXOSelectionStrategy::Default => {},
        };
        Ok(query.load(conn)?)
    }

    /// Return all unspent outputs that have a maturity above the provided chain tip
    pub fn index_time_locked(tip: u64, conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::status.eq(OutputStatus::Unspent as i32))
            .filter(outputs::maturity.gt(tip as i64))
            .load(conn)?)
    }

    pub fn index_unconfirmed(conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(
                outputs::status
                    .eq(OutputStatus::UnspentMinedUnconfirmed as i32)
                    .or(outputs::mined_in_block.is_null()),
            )
            .order(outputs::id.asc())
            .load(conn)?)
    }

    pub fn index_by_feature_flags(
        flags: OutputFlags,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        let res = diesel::sql_query("SELECT * FROM outputs where flags & $1 = $1 ORDER BY id;")
            .bind::<diesel::sql_types::Integer, _>(flags.bits() as i32)
            .load(conn)?;
        Ok(res)
    }

    pub fn index_marked_deleted_in_block_is_null(
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            // Return outputs not marked as deleted or confirmed
            .filter(outputs::marked_deleted_in_block.is_null().or(outputs::status.eq(OutputStatus::SpentMinedUnconfirmed as i32)))
            // Only return mined
            .filter(outputs::mined_in_block.is_not_null())
            .order(outputs::id.asc())
            .load(conn)?)
    }

    pub fn first_by_mined_height_desc(conn: &SqliteConnection) -> Result<Option<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::mined_height.is_not_null())
            .order(outputs::mined_height.desc())
            .first(conn)
            .optional()?)
    }

    pub fn first_by_marked_deleted_height_desc(
        conn: &SqliteConnection,
    ) -> Result<Option<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::marked_deleted_at_height.is_not_null())
            .order(outputs::marked_deleted_at_height.desc())
            .first(conn)
            .optional()?)
    }

    /// Find a particular Output, if it exists
    pub fn find(spending_key: &[u8], conn: &SqliteConnection) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::spending_key.eq(spending_key))
            .first::<OutputSql>(conn)?)
    }

    pub fn find_by_tx_id(tx_id: TxId, conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(
                outputs::received_in_tx_id
                    .eq(i64::from(tx_id))
                    .or(outputs::spent_in_tx_id.eq(i64::from(tx_id))),
            )
            .load(conn)?)
    }

    /// Return the available, time locked, pending incoming and pending outgoing balance
    pub fn get_balance(
        current_tip_for_time_lock_calculation: Option<u64>,
        conn: &SqliteConnection,
    ) -> Result<Balance, OutputManagerStorageError> {
        #[derive(QueryableByName, Clone)]
        struct BalanceQueryResult {
            #[sql_type = "diesel::sql_types::BigInt"]
            amount: i64,
            #[sql_type = "diesel::sql_types::Text"]
            category: String,
        }
        let balance_query_result = if let Some(current_tip) = current_tip_for_time_lock_calculation {
            let balance_query = sql_query(
                "SELECT coalesce(sum(value), 0) as amount, 'available_balance' as category \
                 FROM outputs WHERE status = ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'time_locked_balance' as category \
                 FROM outputs WHERE status = ? AND maturity > ? OR script_lock_height > ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'pending_incoming_balance' as category \
                 FROM outputs WHERE status = ? OR status = ? OR status = ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'pending_outgoing_balance' as category \
                 FROM outputs WHERE status = ? OR status = ? OR status = ?",
            )
                // available_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::Unspent as i32)
                // time_locked_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::Unspent as i32)
                .bind::<diesel::sql_types::BigInt, _>(current_tip as i64)
                .bind::<diesel::sql_types::BigInt, _>(current_tip as i64)
                // pending_incoming_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::EncumberedToBeReceived as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::ShortTermEncumberedToBeReceived as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::UnspentMinedUnconfirmed as i32)
                // pending_outgoing_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::EncumberedToBeSpent as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::ShortTermEncumberedToBeSpent as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::SpentMinedUnconfirmed as i32);
            balance_query.load::<BalanceQueryResult>(conn)?
        } else {
            let balance_query = sql_query(
                "SELECT coalesce(sum(value), 0) as amount, 'available_balance' as category \
                 FROM outputs WHERE status = ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'pending_incoming_balance' as category \
                 FROM outputs WHERE status = ? OR status = ? OR status = ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'pending_outgoing_balance' as category \
                 FROM outputs WHERE status = ? OR status = ? OR status = ?",
            )
                // available_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::Unspent as i32)
                // pending_incoming_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::EncumberedToBeReceived as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::ShortTermEncumberedToBeReceived as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::UnspentMinedUnconfirmed as i32)
                // pending_outgoing_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::EncumberedToBeSpent as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::ShortTermEncumberedToBeSpent as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::SpentMinedUnconfirmed as i32);
            balance_query.load::<BalanceQueryResult>(conn)?
        };
        let mut available_balance = None;
        let mut time_locked_balance = Some(None);
        let mut pending_incoming_balance = None;
        let mut pending_outgoing_balance = None;
        for balance in balance_query_result {
            match balance.category.as_str() {
                "available_balance" => available_balance = Some(MicroTari::from(balance.amount as u64)),
                "time_locked_balance" => time_locked_balance = Some(Some(MicroTari::from(balance.amount as u64))),
                "pending_incoming_balance" => pending_incoming_balance = Some(MicroTari::from(balance.amount as u64)),
                "pending_outgoing_balance" => pending_outgoing_balance = Some(MicroTari::from(balance.amount as u64)),
                _ => {
                    return Err(OutputManagerStorageError::UnexpectedResult(
                        "Unexpected category in balance query".to_string(),
                    ))
                },
            }
        }

        Ok(Balance {
            available_balance: available_balance.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult("Available balance could not be calculated".to_string())
            })?,
            time_locked_balance: time_locked_balance.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult("Time locked balance could not be calculated".to_string())
            })?,
            pending_incoming_balance: pending_incoming_balance.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult(
                    "Pending incoming balance could not be calculated".to_string(),
                )
            })?,
            pending_outgoing_balance: pending_outgoing_balance.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult(
                    "Pending outgoing balance could not be calculated".to_string(),
                )
            })?,
        })
    }

    pub fn find_by_commitment(
        commitment: &[u8],
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::commitment.eq(commitment))
            .first::<OutputSql>(conn)?)
    }

    pub fn find_by_commitment_and_cancelled(
        commitment: &[u8],
        cancelled: bool,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        let cancelled_flag = OutputStatus::CancelledInbound as i32;

        let mut request = outputs::table.filter(outputs::commitment.eq(commitment)).into_boxed();
        if cancelled {
            request = request.filter(outputs::status.eq(cancelled_flag))
        } else {
            request = request.filter(outputs::status.ne(cancelled_flag))
        };

        Ok(request.first::<OutputSql>(conn)?)
    }

    pub fn find_by_tx_id_and_status(
        tx_id: TxId,
        status: OutputStatus,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(
                outputs::received_in_tx_id
                    .eq(Some(tx_id.as_u64() as i64))
                    .or(outputs::spent_in_tx_id.eq(Some(tx_id.as_u64() as i64))),
            )
            .filter(outputs::status.eq(status as i32))
            .load(conn)?)
    }

    /// Find outputs via tx_id that are encumbered. Any outputs that are encumbered cannot be marked as spent.
    pub fn find_by_tx_id_and_encumbered(
        tx_id: TxId,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(
                outputs::received_in_tx_id
                    .eq(Some(tx_id.as_u64() as i64))
                    .or(outputs::spent_in_tx_id.eq(Some(tx_id.as_u64() as i64))),
            )
            .filter(
                outputs::status
                    .eq(OutputStatus::EncumberedToBeReceived as i32)
                    .or(outputs::status.eq(OutputStatus::EncumberedToBeSpent as i32))
                    .or(outputs::status.eq(OutputStatus::ShortTermEncumberedToBeReceived as i32))
                    .or(outputs::status.eq(OutputStatus::ShortTermEncumberedToBeSpent as i32)),
            )
            .load(conn)?)
    }

    /// Find a particular Output, if it exists and is in the specified Spent state
    pub fn find_status(
        spending_key: &[u8],
        status: OutputStatus,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::status.eq(status as i32))
            .filter(outputs::spending_key.eq(spending_key))
            .first::<OutputSql>(conn)?)
    }

    /// Find a particular Output, if it exists and is in the specified Spent state
    pub fn find_by_hash(
        hash: &[u8],
        status: OutputStatus,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::status.eq(status as i32))
            .filter(outputs::hash.eq(Some(hash)))
            .first::<OutputSql>(conn)?)
    }

    /// Find a particular Output, if it exists and is in the specified Spent state
    pub fn find_pending_coinbase_at_block_height(
        block_height: u64,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::status.ne(OutputStatus::Unspent as i32))
            .filter(outputs::coinbase_block_height.eq(block_height as i64))
            .first::<OutputSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
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
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        diesel::update(outputs::table.filter(outputs::id.eq(&self.id)))
            .set(UpdateOutputSql::from(updated_output))
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        OutputSql::find(&self.spending_key, conn)
    }

    /// Update the changed fields of this record after encryption/decryption is performed
    pub fn update_encryption(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        let _ = self.update(
            UpdateOutput {
                spending_key: Some(self.spending_key.clone()),
                script_private_key: Some(self.script_private_key.clone()),
                ..Default::default()
            },
            conn,
        )?;
        Ok(())
    }
}

/// Conversion from an DbUnblindedOutput to the Sql datatype form
impl TryFrom<OutputSql> for DbUnblindedOutput {
    type Error = OutputManagerStorageError;

    fn try_from(o: OutputSql) -> Result<Self, Self::Error> {
        let mut features: OutputFeatures =
            serde_json::from_str(o.features_json.as_str()).map_err(|s| OutputManagerStorageError::ConversionError {
                reason: format!("Could not convert json into OutputFeatures:{}", s),
            })?;

        features.flags = OutputFlags::from_bits(o.flags as u8).ok_or(OutputManagerStorageError::ConversionError {
            reason: "Flags could not be converted from bits".to_string(),
        })?;
        features.maturity = o.maturity as u64;
        features.metadata = o.metadata.unwrap_or_default();
        features.unique_id = o.features_unique_id.clone();
        features.parent_public_key = o
            .features_parent_public_key
            .map(|p| PublicKey::from_bytes(&p))
            .transpose()?;

        let unblinded_output = UnblindedOutput::new_current_version(
            MicroTari::from(o.value as u64),
            PrivateKey::from_vec(&o.spending_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PrivateKey from stored bytes, They might be encrypted"
                );
                OutputManagerStorageError::ConversionError {
                    reason: "PrivateKey could not be converted from bytes".to_string(),
                }
            })?,
            features,
            TariScript::from_bytes(o.script.as_slice())?,
            ExecutionStack::from_bytes(o.input_data.as_slice())?,
            PrivateKey::from_vec(&o.script_private_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PrivateKey from stored bytes, They might be encrypted"
                );
                OutputManagerStorageError::ConversionError {
                    reason: "PrivateKey could not be converted from bytes".to_string(),
                }
            })?,
            PublicKey::from_vec(&o.sender_offset_public_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PublicKey from stored bytes, They might be encrypted"
                );
                OutputManagerStorageError::ConversionError {
                    reason: "PrivateKey could not be converted from bytes".to_string(),
                }
            })?,
            ComSignature::new(
                Commitment::from_vec(&o.metadata_signature_nonce).map_err(|_| {
                    error!(
                        target: LOG_TARGET,
                        "Could not create PublicKey from stored bytes, They might be encrypted"
                    );
                    OutputManagerStorageError::ConversionError {
                        reason: "PrivateKey could not be converted from bytes".to_string(),
                    }
                })?,
                PrivateKey::from_vec(&o.metadata_signature_u_key).map_err(|_| {
                    error!(
                        target: LOG_TARGET,
                        "Could not create PrivateKey from stored bytes, They might be encrypted"
                    );
                    OutputManagerStorageError::ConversionError {
                        reason: "PrivateKey could not be converted from bytes".to_string(),
                    }
                })?,
                PrivateKey::from_vec(&o.metadata_signature_v_key).map_err(|_| {
                    error!(
                        target: LOG_TARGET,
                        "Could not create PrivateKey from stored bytes, They might be encrypted"
                    );
                    OutputManagerStorageError::ConversionError {
                        reason: "PrivateKey could not be converted from bytes".to_string(),
                    }
                })?,
            ),
            o.script_lock_height as u64,
            Covenant::from_bytes(&o.covenant).map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Could not create Covenant from stored bytes ({}), They might be encrypted", e
                );
                OutputManagerStorageError::ConversionError {
                    reason: "Covenant could not be converted from bytes".to_string(),
                }
            })?,
        );

        let hash = match o.hash {
            None => {
                let factories = CryptoFactories::default();
                unblinded_output.as_transaction_output(&factories)?.hash()
            },
            Some(v) => v,
        };
        let commitment = match o.commitment {
            None => {
                let factories = CryptoFactories::default();
                factories
                    .commitment
                    .commit(&unblinded_output.spending_key, &unblinded_output.value.into())
            },
            Some(c) => Commitment::from_vec(&c)?,
        };
        let spending_priority = (o.spending_priority as u32).into();
        Ok(Self {
            commitment,
            unblinded_output,
            hash,
            status: o.status.try_into()?,
            mined_height: o.mined_height.map(|mh| mh as u64),
            mined_in_block: o.mined_in_block,
            mined_mmr_position: o.mined_mmr_position.map(|mp| mp as u64),
            marked_deleted_at_height: o.marked_deleted_at_height.map(|d| d as u64),
            marked_deleted_in_block: o.marked_deleted_in_block,
            spending_priority,
        })
    }
}

impl Encryptable<Aes256Gcm> for OutputSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        self.spending_key = encrypt_bytes_integral_nonce(cipher, self.spending_key.clone())?;
        self.script_private_key = encrypt_bytes_integral_nonce(cipher, self.script_private_key.clone())?;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        self.spending_key = decrypt_bytes_integral_nonce(cipher, self.spending_key.clone())?;
        self.script_private_key = decrypt_bytes_integral_nonce(cipher, self.script_private_key.clone())?;
        Ok(())
    }
}

// impl PartialEq<NewOutputSql> for OutputSql {
//     fn eq(&self, other: &NewOutputSql) -> bool {
//         &NewOutputSql::from(self.clone()) == other
//     }
// }
