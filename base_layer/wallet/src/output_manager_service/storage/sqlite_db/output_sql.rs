use diesel::{prelude::*,  SqliteConnection};
use crate::{
    output_manager_service::{
        error::OutputManagerStorageError,
    },
    schema::{  outputs, },
    util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
};
use crate::output_manager_service::storage::OutputStatus;
use crate::schema::outputs::columns;
use tari_core::transactions::transaction::OutputFlags;
use crate::output_manager_service::storage::sqlite_db::{NullOutputSql, UpdateOutput, UpdateOutputSql};
use aes_gcm::Aes256Gcm;
use tari_core::transactions::transaction_protocol::TxId;
use crate::output_manager_service::storage::sqlite_db::AeadError;

#[derive(Clone, Debug, Queryable, Identifiable, PartialEq)]
#[table_name = "outputs"]
pub struct OutputSql {
    pub id: i32,
    pub commitment: Option<Vec<u8>>,
    pub spending_key: Vec<u8>,
    pub value: i64,
    pub flags: i32,
    pub maturity: i64,
    pub status: i32,
    pub tx_id: Option<i64>,
    pub hash: Option<Vec<u8>>,
    pub script: Vec<u8>,
    pub input_data: Vec<u8>,
    pub height: i64,
    pub script_private_key: Vec<u8>,
    pub script_offset_public_key: Vec<u8>,
    pub unique_id: Option<Vec<u8>>,
    pub metadata: Option<Vec<u8>>,
    pub features_asset_public_key: Option<Vec<u8>>,
    pub parent_public_key: Option<Vec<u8>>,
    pub features_mint_asset_public_key: Option<Vec<u8>>,
    pub features_mint_asset_owner_commitment: Option<Vec<u8>>

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
        flags: OutputFlags, conn : &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table.filter(columns::flags.eq(flags.bits() as i32)).load(conn)?)
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

    pub fn find_by_commitment_and_block_height(
        commitment: &[u8],
        height: u64,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(columns::commitment.eq(commitment))
            .filter(columns::height.eq(height as i64))
            .first::<OutputSql>(conn)?)
    }

    /// Find outputs via tx_id
    pub fn find_by_tx_id(tx_id: TxId, conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(columns::tx_id.eq(Some(tx_id.as_u64() as i64)))
            .load(conn)?)
    }

    /// Find outputs via tx_id that are encumbered. Any outputs that are encumbered cannot be marked as spent.
    pub fn find_by_tx_id_and_encumbered(
        tx_id: TxId,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(columns::tx_id.eq(Some(tx_id.as_u64() as i64)))
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
                .. Default::default()
            },
            conn,
        )?;
        Ok(())
    }
}

impl Encryptable<Aes256Gcm> for OutputSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = encrypt_bytes_integral_nonce(&cipher, self.spending_key.clone())?;
        self.script_private_key = encrypt_bytes_integral_nonce(&cipher, self.script_private_key.clone())?;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = decrypt_bytes_integral_nonce(&cipher, self.spending_key.clone())?;
        self.script_private_key = decrypt_bytes_integral_nonce(&cipher, self.script_private_key.clone())?;
        Ok(())
    }
}

// impl PartialEq<NewOutputSql> for OutputSql {
//     fn eq(&self, other: &NewOutputSql) -> bool {
//         &NewOutputSql::from(self.clone()) == other
//     }
// }
