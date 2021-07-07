// Copyright 2021. The Tari Project
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

use aes_gcm::{Aes256Gcm, Error as AeadError};
use diesel::{RunQueryDsl, SqliteConnection};

use tari_core::crypto::tari_utilities::ByteArray;
use tari_core::transactions::transaction_protocol::TxId;

use crate::output_manager_service::error::OutputManagerStorageError;
use crate::output_manager_service::storage::models::DbUnblindedOutput;
use crate::output_manager_service::storage::OutputStatus;
use crate::schema::outputs;
use crate::util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable};

/// This struct represents an Output in the Sql database. A distinct struct is required to define the Sql friendly
/// equivalent datatypes for the members.
#[derive(Clone, Debug, Insertable, PartialEq)]
#[table_name = "outputs"]
pub struct NewOutputSql {
    commitment: Option<Vec<u8>>,
    spending_key: Vec<u8>,
    value: i64,
    flags: i32,
    maturity: i64,
    status: i32,
    tx_id: Option<i64>,
    hash: Option<Vec<u8>>,
    script: Vec<u8>,
    input_data: Vec<u8>,
    script_private_key: Vec<u8>,
    metadata: Option<Vec<u8>>,
    features_asset_public_key: Option<Vec<u8>>,
    unique_id: Option<Vec<u8>>,
    parent_public_key: Option<Vec<u8>>
}

impl NewOutputSql {
    pub fn new(output: DbUnblindedOutput, status: OutputStatus, tx_id: Option<TxId>) -> Self {
        Self {
            commitment: Some(output.commitment.to_vec()),
            spending_key: output.unblinded_output.spending_key.to_vec(),
            value: (u64::from(output.unblinded_output.value)) as i64,
            flags: output.unblinded_output.features.flags.bits() as i32,
            maturity: output.unblinded_output.features.maturity as i64,
            status: status as i32,
            tx_id: tx_id.map(|i| i.as_u64() as i64),
            hash: Some(output.hash),
            script: output.unblinded_output.script.as_bytes(),
            input_data: output.unblinded_output.input_data.as_bytes(),
            height: output.unblinded_output.height as i64,
            script_private_key: output.unblinded_output.script_private_key.to_vec(),
            script_offset_public_key: output.unblinded_output.script_offset_public_key.to_vec(),
            metadata: Some(output.unblinded_output.features.metadata),
            features_asset_public_key: output.unblinded_output.features.asset.map(|a| a.public_key.to_vec()),
            unique_id: output.unblinded_output.unique_id,
            parent_public_key: output.unblinded_output.parent_public_key.map(|a| a.to_vec())
        }
    }

    /// Write this struct to the database
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(outputs::table).values(self.clone()).execute(conn)?;
        Ok(())
    }
}

impl Encryptable<Aes256Gcm> for NewOutputSql {
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

