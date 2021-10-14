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

use tari_core::{crypto::tari_utilities::ByteArray, transactions::transaction_protocol::TxId};

use crate::{
    output_manager_service::{
        error::OutputManagerStorageError,
        storage::{models::DbUnblindedOutput, OutputStatus},
    },
    schema::outputs,
    util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
};

/// This struct represents an Output in the Sql database. A distinct struct is required to define the Sql friendly
/// equivalent datatypes for the members.
#[derive(Clone, Debug, Insertable, PartialEq)]
#[table_name = "outputs"]
pub struct NewOutputSql {
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
    pub metadata: Option<Vec<u8>>,
    pub features_asset_public_key: Option<Vec<u8>>,
    pub features_mint_asset_public_key: Option<Vec<u8>>,
    pub features_mint_asset_owner_commitment: Option<Vec<u8>>,
    pub features_parent_public_key: Option<Vec<u8>>,
    pub features_unique_id: Option<Vec<u8>>,
    pub sender_offset_public_key: Vec<u8>,
    pub metadata_signature_nonce: Vec<u8>,
    pub metadata_signature_u_key: Vec<u8>,
    pub metadata_signature_v_key: Vec<u8>,
    pub received_in_tx_id: Option<i64>,
    pub coinbase_block_height: Option<i64>,
}

impl NewOutputSql {
    pub fn new(
        output: DbUnblindedOutput,
        status: OutputStatus,
        received_in_tx_id: Option<TxId>,
        coinbase_block_height: Option<u64>,
    ) -> Self {
        Self {
            commitment: Some(output.commitment.to_vec()),
            spending_key: output.unblinded_output.spending_key.to_vec(),
            value: (u64::from(output.unblinded_output.value)) as i64,
            flags: output.unblinded_output.features.flags.bits() as i32,
            maturity: output.unblinded_output.features.maturity as i64,
            status: status as i32,
            received_in_tx_id: received_in_tx_id.map(i64::from),
            hash: Some(output.hash),
            script: output.unblinded_output.script.as_bytes(),
            input_data: output.unblinded_output.input_data.as_bytes(),
            script_private_key: output.unblinded_output.script_private_key.to_vec(),
            metadata: Some(output.unblinded_output.features.metadata),
            features_asset_public_key: output.unblinded_output.features.asset.map(|a| a.public_key.to_vec()),
            features_mint_asset_public_key: output
                .unblinded_output
                .features
                .mint_non_fungible
                .clone()
                .map(|a| a.asset_public_key.to_vec()),
            features_mint_asset_owner_commitment: output
                .unblinded_output
                .features
                .mint_non_fungible
                .map(|a| a.asset_owner_commitment.to_vec()),
            features_parent_public_key: output.unblinded_output.features.parent_public_key.map(|a| a.to_vec()),
            features_unique_id: output.unblinded_output.features.unique_id,
            sender_offset_public_key: output.unblinded_output.sender_offset_public_key.to_vec(),
            metadata_signature_nonce: output.unblinded_output.metadata_signature.public_nonce().to_vec(),
            metadata_signature_u_key: output.unblinded_output.metadata_signature.u().to_vec(),
            metadata_signature_v_key: output.unblinded_output.metadata_signature.v().to_vec(),
            coinbase_block_height: coinbase_block_height.map(|bh| bh as i64),
        }
    }

    /// Write this struct to the database
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(outputs::table).values(self.clone()).execute(conn)?;
        Ok(())
    }

    // /// Return all outputs with a given status
    // pub fn index_status(
    //     status: OutputStatus,
    //     conn: &SqliteConnection,
    // ) -> Result<Vec<NewOutputSql>, OutputManagerStorageError> {
    //     Ok(outputs::table.filter(columns::status.eq(status as i32)).load(conn)?)
    // }
}

impl Encryptable<Aes256Gcm> for NewOutputSql {
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
