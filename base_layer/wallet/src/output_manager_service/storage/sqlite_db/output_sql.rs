use crate::{
    output_manager_service::{
        error::OutputManagerStorageError,
        service::Balance,
        storage::{
            sqlite_db::{AeadError, NullOutputSql, UpdateOutput, UpdateOutputSql},
            OutputStatus,
        },
    },
    schema::{outputs, outputs::columns},
    util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
};
use aes_gcm::Aes256Gcm;
use diesel::{prelude::*, sql_query, SqliteConnection};
use tari_common_types::transaction::TxId;
use tari_core::transactions::{tari_amount::MicroTari, transaction::OutputFlags};

#[derive(Clone, Debug, Queryable, QueryableByName, Identifiable, PartialEq)]
#[table_name = "outputs"]
pub struct OutputSql {
    pub id: i32,
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
    pub features_asset_public_key: Option<Vec<u8>>,
    pub features_mint_asset_public_key: Option<Vec<u8>>,
    pub features_mint_asset_owner_commitment: Option<Vec<u8>>,
    pub features_sidechain_checkpoint_merkle_root: Option<Vec<u8>>,
    pub features_parent_public_key: Option<Vec<u8>>,
    pub features_unique_id: Option<Vec<u8>>,
    pub features_asset_template_ids_implemented: Option<String>,
    pub features_sidechain_committee: Option<String>,
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
        Ok(outputs::table.filter(columns::status.eq(status as i32)).load(conn)?)
    }

    /// Return all unspent outputs that have a maturity above the provided chain tip
    pub fn index_time_locked(tip: u64, conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(columns::status.eq(OutputStatus::Unspent as i32))
            .filter(columns::maturity.gt(tip as i64))
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

    /// Find a particular Output, if it exists
    pub fn find(spending_key: &[u8], conn: &SqliteConnection) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(columns::spending_key.eq(spending_key))
            .first::<OutputSql>(conn)?)
    }

    pub fn find_by_commitment(
        commitment: &[u8],
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        let cancelled = OutputStatus::CancelledInbound as i32;
        Ok(outputs::table
            .filter(columns::status.ne(cancelled))
            .filter(columns::commitment.eq(commitment))
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
        let tx_id = Some(tx_id.as_u64() as i64);
        let received = outputs::received_in_tx_id.eq(tx_id);
        let spent = outputs::spent_in_tx_id.eq(tx_id);
        Ok(outputs::table
            .filter(received.or(spent))
            .filter(outputs::status.eq(status as i32))
            .load(conn)?)
    }

    /// Find outputs via tx_id
    pub fn find_by_tx_id(tx_id: TxId, conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        let tx_id = Some(tx_id.as_u64() as i64);
        let received = outputs::received_in_tx_id.eq(tx_id);
        let spent = outputs::spent_in_tx_id.eq(tx_id);
        Ok(outputs::table.filter(received.or(spent)).load(conn)?)
    }

    /// Find outputs via tx_id that are encumbered. Any outputs that are encumbered cannot be marked as spent.
    pub fn find_by_tx_id_and_encumbered(
        tx_id: TxId,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        let tx_id = Some(tx_id.as_u64() as i64);
        let received = outputs::received_in_tx_id.eq(tx_id);
        let spent = outputs::spent_in_tx_id.eq(tx_id);
        Ok(outputs::table
            .filter(received.or(spent))
            .filter(
                columns::status
                    .eq(OutputStatus::EncumberedToBeReceived as i32)
                    .or(columns::status.eq(OutputStatus::EncumberedToBeSpent as i32)),
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
            .filter(columns::status.eq(status as i32))
            .filter(columns::spending_key.eq(spending_key))
            .first::<OutputSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        let num_deleted =
            diesel::delete(outputs::table.filter(columns::spending_key.eq(&self.spending_key))).execute(conn)?;

        if num_deleted == 0 {
            return Err(OutputManagerStorageError::ValuesNotFound);
        }

        Ok(())
    }

    // TODO: This method needs to be checked for concurrency
    pub fn update(
        &self,
        updated_output: UpdateOutput,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        let num_updated = diesel::update(outputs::table.filter(columns::id.eq(&self.id)))
            .set(UpdateOutputSql::from(updated_output))
            .execute(conn)?;

        if num_updated == 0 {
            return Err(OutputManagerStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        OutputSql::find(&self.spending_key, conn)
    }

    /// This function is used to update an existing record to set fields to null
    pub fn update_null(
        &self,
        updated_null: NullOutputSql,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        let num_updated = diesel::update(outputs::table.filter(columns::spending_key.eq(&self.spending_key)))
            .set(updated_null)
            .execute(conn)?;

        if num_updated == 0 {
            return Err(OutputManagerStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

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

    pub fn index_marked_deleted_in_block_is_null(
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
                    .filter(outputs::marked_deleted_in_block.is_null())
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
                     FROM outputs WHERE status = ? AND maturity > ? \
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
        for balance in balance_query_result.clone() {
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
}

impl Encryptable<Aes256Gcm> for OutputSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = encrypt_bytes_integral_nonce(cipher, self.spending_key.clone())?;
        self.script_private_key = encrypt_bytes_integral_nonce(cipher, self.script_private_key.clone())?;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
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
