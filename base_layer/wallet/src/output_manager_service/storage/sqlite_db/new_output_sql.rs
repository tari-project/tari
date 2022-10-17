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
use chacha20poly1305::XChaCha20Poly1305;
use derivative::Derivative;
use diesel::{prelude::*, SqliteConnection};
use tari_common_types::transaction::TxId;
use tari_utilities::ByteArray;

use crate::{
    output_manager_service::{
        error::OutputManagerStorageError,
        storage::{models::DbUnblindedOutput, sqlite_db::OutputSql, OutputStatus},
    },
    schema::outputs,
    util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
};

/// This struct represents an Output in the Sql database. A distinct struct is required to define the Sql friendly
/// equivalent datatypes for the members.
#[derive(Clone, Derivative, Insertable, PartialEq)]
#[derivative(Debug)]
#[table_name = "outputs"]
pub struct NewOutputSql {
    pub commitment: Option<Vec<u8>>,
    #[derivative(Debug = "ignore")]
    pub spending_key: Vec<u8>,
    pub value: i64,
    pub output_type: i32,
    pub maturity: i64,
    pub status: i32,
    pub hash: Option<Vec<u8>>,
    pub script: Vec<u8>,
    pub input_data: Vec<u8>,
    #[derivative(Debug = "ignore")]
    pub script_private_key: Vec<u8>,
    pub metadata: Option<Vec<u8>>,
    pub sender_offset_public_key: Vec<u8>,
    pub metadata_signature_nonce: Vec<u8>,
    pub metadata_signature_u_key: Vec<u8>,
    pub metadata_signature_v_key: Vec<u8>,
    pub received_in_tx_id: Option<i64>,
    pub coinbase_block_height: Option<i64>,
    pub features_json: String,
    pub covenant: Vec<u8>,
    pub encrypted_value: Vec<u8>,
    pub minimum_value_promise: i64,
    pub source: i32,
}

impl NewOutputSql {
    #[allow(clippy::cast_possible_wrap)]
    pub fn new(
        output: DbUnblindedOutput,
        status: OutputStatus,
        received_in_tx_id: Option<TxId>,
        coinbase_block_height: Option<u64>,
    ) -> Result<Self, OutputManagerStorageError> {
        Ok(Self {
            commitment: Some(output.commitment.to_vec()),
            spending_key: output.unblinded_output.spending_key.to_vec(),
            value: output.unblinded_output.value.as_u64() as i64,
            output_type: i32::from(output.unblinded_output.features.output_type.as_byte()),
            maturity: output.unblinded_output.features.maturity as i64,
            status: status as i32,
            received_in_tx_id: received_in_tx_id.map(|i| i.as_u64() as i64),
            hash: Some(output.hash.to_vec()),
            script: output.unblinded_output.script.to_bytes(),
            input_data: output.unblinded_output.input_data.to_bytes(),
            script_private_key: output.unblinded_output.script_private_key.to_vec(),
            metadata: Some(output.unblinded_output.features.metadata.clone()),
            sender_offset_public_key: output.unblinded_output.sender_offset_public_key.to_vec(),
            metadata_signature_nonce: output.unblinded_output.metadata_signature.public_nonce().to_vec(),
            metadata_signature_u_key: output.unblinded_output.metadata_signature.u().to_vec(),
            metadata_signature_v_key: output.unblinded_output.metadata_signature.v().to_vec(),
            coinbase_block_height: coinbase_block_height.map(|bh| bh as i64),
            features_json: serde_json::to_string(&output.unblinded_output.features).map_err(|s| {
                OutputManagerStorageError::ConversionError {
                    reason: format!("Could not parse features from JSON:{}", s),
                }
            })?,
            covenant: output.unblinded_output.covenant.to_bytes(),
            encrypted_value: output.unblinded_output.encrypted_value.to_vec(),
            minimum_value_promise: output.unblinded_output.minimum_value_promise.as_u64() as i64,
            source: output.source as i32,
        })
    }

    /// Write this struct to the database
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(outputs::table).values(self.clone()).execute(conn)?;
        Ok(())
    }
}

impl Encryptable<XChaCha20Poly1305> for NewOutputSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        // WARNING: using `OUTPUT` for both NewOutputSql and OutputSql due to later transition without re-encryption
        [Self::OUTPUT, self.script.as_slice(), field_name.as_bytes()]
            .concat()
            .to_vec()
    }

    fn encrypt(&mut self, cipher: &XChaCha20Poly1305) -> Result<(), String> {
        self.spending_key =
            encrypt_bytes_integral_nonce(cipher, self.domain("spending_key"), self.spending_key.clone())?;

        self.script_private_key = encrypt_bytes_integral_nonce(
            cipher,
            self.domain("script_private_key"),
            self.script_private_key.clone(),
        )?;

        Ok(())
    }

    fn decrypt(&mut self, cipher: &XChaCha20Poly1305) -> Result<(), String> {
        self.spending_key =
            decrypt_bytes_integral_nonce(cipher, self.domain("spending_key"), self.spending_key.clone())?;

        self.script_private_key = decrypt_bytes_integral_nonce(
            cipher,
            self.domain("script_private_key"),
            self.script_private_key.clone(),
        )?;

        Ok(())
    }
}

impl From<OutputSql> for NewOutputSql {
    fn from(o: OutputSql) -> Self {
        Self {
            commitment: o.commitment,
            spending_key: o.spending_key,
            value: o.value,
            output_type: o.output_type,
            maturity: o.maturity,
            status: o.status,
            hash: o.hash,
            script: o.script,
            input_data: o.input_data,
            script_private_key: o.script_private_key,
            metadata: o.metadata,
            sender_offset_public_key: o.sender_offset_public_key,
            metadata_signature_nonce: o.metadata_signature_nonce,
            metadata_signature_u_key: o.metadata_signature_u_key,
            metadata_signature_v_key: o.metadata_signature_v_key,
            received_in_tx_id: o.received_in_tx_id,
            coinbase_block_height: o.coinbase_block_height,
            features_json: o.features_json,
            covenant: o.covenant,
            encrypted_value: o.encrypted_value,
            minimum_value_promise: o.minimum_value_promise,
            source: 0,
        }
    }
}
