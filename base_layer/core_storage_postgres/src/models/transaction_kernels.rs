use chrono::{NaiveDateTime, Utc};
use crate::error::PostgresChainStorageError;
use diesel::prelude::*;
use tari_core::transactions::types::HashOutput;
use crate::schema::*;
use tari_core::transactions::transaction;
use tari_crypto::tari_utilities::Hashable;
use tari_crypto::tari_utilities::hex::Hex;
use tari_crypto::tari_utilities::ByteArray;

#[derive(Queryable, Identifiable, Insertable)]
#[table_name="transaction_kernels"]
#[primary_key(hash)]
pub struct TransactionKernel {
    pub hash: String,
    pub features: i32,
    pub fee: i64,
    pub lock_height: i64,
    pub meta_info: Option<String>,
    pub linked_kernel: Option<String>,
    pub excess: String,
    pub excess_sig_nonce: Vec<u8>,
    pub excess_sig_sig: Vec<u8>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl TransactionKernel {
    pub fn insert(hash: HashOutput, kernel: transaction::TransactionKernel, conn: &PgConnection) -> Result<(),
        PostgresChainStorageError> {

        let row: TransactionKernel = kernel.into();
        if row.hash != hash.to_hex() {
            return Err(
                PostgresChainStorageError::InsertError(format!("Found two different hashes when saving kernel:{} != {}", &row.hash, &hash.to_hex())));
        }

        diesel::insert_into(transaction_kernels::table).values(row)
            .execute(conn).map_err(|err| PostgresChainStorageError::InsertError(
            format!("Could not insert transaction kernel '{}':{}", hash.to_hex(), err)
        ))?;

        Ok(())
    }
}

impl From<transaction::TransactionKernel> for TransactionKernel {
    fn from(value: transaction::TransactionKernel) -> Self {
        Self {
            hash: value.hash().to_hex(),
            features: value.features.bits() as i32,
            fee: value.fee.0 as i64,
            lock_height: value.lock_height as i64,
            meta_info: value.meta_info.map(|mi| mi.to_hex()),
            linked_kernel: value.linked_kernel.map(|lk| lk.to_hex()),
            excess: value.excess.to_hex(),
            excess_sig_nonce: value.excess_sig.get_public_nonce().to_vec(),
            excess_sig_sig: value.excess_sig.get_signature().to_vec(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc()
        }
    }
}