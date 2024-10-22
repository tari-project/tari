use borsh::BorshSerialize;
//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that
// the  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES,  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL,  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY,  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.
use derivative::Derivative;
use diesel::prelude::*;
use tari_common_types::transaction::TxId;
use tari_utilities::ByteArray;

use crate::{
    output_manager_service::{
        error::OutputManagerStorageError,
        storage::{models::DbWalletOutput, OutputStatus},
    },
    schema::outputs,
};

/// This struct represents an Output in the Sql database. A distinct struct is required to define the Sql friendly
/// equivalent datatypes for the members.
#[derive(Clone, Derivative, Insertable, PartialEq)]
#[derivative(Debug)]
#[diesel(table_name = outputs)]
pub struct NewOutputSql {
    pub commitment: Vec<u8>,
    pub spending_key: String,
    pub rangeproof: Option<Vec<u8>>,
    pub value: i64,
    pub output_type: i32,
    pub maturity: i64,
    pub status: i32,
    pub hash: Vec<u8>,
    pub script: Vec<u8>,
    pub input_data: Vec<u8>,
    pub script_private_key: String,
    pub coinbase_extra: Option<Vec<u8>>,
    pub sender_offset_public_key: Vec<u8>,
    pub metadata_signature_ephemeral_commitment: Vec<u8>,
    pub metadata_signature_ephemeral_pubkey: Vec<u8>,
    pub metadata_signature_u_a: Vec<u8>,
    pub metadata_signature_u_x: Vec<u8>,
    pub metadata_signature_u_y: Vec<u8>,
    pub received_in_tx_id: Option<i64>,
    pub features_json: String,
    pub covenant: Vec<u8>,
    pub encrypted_data: Vec<u8>,
    pub minimum_value_promise: i64,
    pub source: i32,
    pub spending_priority: i32,
}

impl NewOutputSql {
    #[allow(clippy::cast_possible_wrap)]
    pub fn new(
        output: DbWalletOutput,
        status: Option<OutputStatus>,
        received_in_tx_id: Option<TxId>,
    ) -> Result<Self, OutputManagerStorageError> {
        let mut covenant = Vec::new();
        BorshSerialize::serialize(&output.wallet_output.covenant, &mut covenant)?;

        let output = Self {
            commitment: output.commitment.to_vec(),
            spending_key: output.wallet_output.spending_key_id.to_string(),
            rangeproof: output.wallet_output.range_proof.map(|proof| proof.to_vec()),
            value: output.wallet_output.value.as_u64() as i64,
            output_type: i32::from(output.wallet_output.features.output_type.as_byte()),
            maturity: output.wallet_output.features.maturity as i64,
            status: status.unwrap_or(output.status) as i32,
            received_in_tx_id: received_in_tx_id.map(|i| i.as_u64() as i64),
            hash: output.hash.to_vec(),
            script: output.wallet_output.script.to_bytes(),
            input_data: output.wallet_output.input_data.to_bytes(),
            script_private_key: output.wallet_output.script_key_id.to_string(),
            coinbase_extra: Some(output.wallet_output.features.coinbase_extra.to_vec().clone()),
            sender_offset_public_key: output.wallet_output.sender_offset_public_key.to_vec(),
            metadata_signature_ephemeral_commitment: output
                .wallet_output
                .metadata_signature
                .ephemeral_commitment()
                .to_vec(),
            metadata_signature_ephemeral_pubkey: output.wallet_output.metadata_signature.ephemeral_pubkey().to_vec(),
            metadata_signature_u_a: output.wallet_output.metadata_signature.u_a().to_vec(),
            metadata_signature_u_x: output.wallet_output.metadata_signature.u_x().to_vec(),
            metadata_signature_u_y: output.wallet_output.metadata_signature.u_y().to_vec(),
            features_json: serde_json::to_string(&output.wallet_output.features).map_err(|s| {
                OutputManagerStorageError::ConversionError {
                    reason: format!("Could not parse features from JSON:{}", s),
                }
            })?,
            covenant,
            encrypted_data: output.wallet_output.encrypted_data.to_byte_vec(),
            minimum_value_promise: output.wallet_output.minimum_value_promise.as_u64() as i64,
            source: output.source as i32,
            spending_priority: output.spending_priority.into(),
        };

        Ok(output)
    }

    /// Write this struct to the database
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(outputs::table).values(self.clone()).execute(conn)?;
        Ok(())
    }
}
