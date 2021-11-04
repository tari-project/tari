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
use crate::{
    output_manager_service::{
        error::OutputManagerStorageError,
        service::Balance,
        storage::{
            database::{DbKey, DbKeyValuePair, DbValue, KeyManagerState, OutputManagerBackend, WriteOperation},
            models::{DbUnblindedOutput, KnownOneSidedPaymentScript, OutputStatus},
            sqlite_db::OutputSql,
        },
    },
    schema::{key_manager_states, known_one_sided_payment_scripts, outputs},
    storage::sqlite_utilities::WalletDbConnection,
    util::{
        diesel_ext::ExpectedRowsExtension,
        encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    },
};
use aes_gcm::Aes256Gcm;
use chrono::{NaiveDateTime, Utc};
use diesel::{prelude::*, result::Error as DieselError, sql_query, SqliteConnection};
use log::*;
use std::{
    convert::{TryFrom, TryInto},
    str::from_utf8,
    sync::{Arc, RwLock},
};
use tari_common_types::{
    transaction::TxId,
    types::{ComSignature, Commitment, PrivateKey, PublicKey},
};
use tari_core::{
    tari_utilities::hash::Hashable,
    transactions::{
        tari_amount::MicroTari,
        transaction::{OutputFeatures, OutputFlags, TransactionOutput, UnblindedOutput},
        CryptoFactories,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    script::{ExecutionStack, TariScript},
    tari_utilities::{
        hex::{from_hex, Hex},
        ByteArray,
    },
};
use tari_key_manager::cipher_seed::CipherSeed;
use tokio::time::Instant;

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
    hash: Option<Vec<u8>>,
    script: Vec<u8>,
    input_data: Vec<u8>,
    script_private_key: Vec<u8>,
    sender_offset_public_key: Vec<u8>,
    metadata_signature_nonce: Vec<u8>,
    metadata_signature_u_key: Vec<u8>,
    metadata_signature_v_key: Vec<u8>,
    received_in_tx_id: Option<i64>,
    coinbase_block_height: Option<i64>,
}

impl NewOutputSql {
    pub fn new(
        output: DbUnblindedOutput,
        status: OutputStatus,
        received_in_tx_id: Option<TxId>,
        coinbase_block_height: Option<u64>,
    ) -> Result<Self, OutputManagerStorageError> {
        Ok(Self {
            commitment: Some(output.commitment.to_vec()),
            spending_key: output.unblinded_output.spending_key.to_vec(),
            value: (u64::from(output.unblinded_output.value)) as i64,
            flags: output.unblinded_output.features.flags.bits() as i32,
            maturity: output.unblinded_output.features.maturity as i64,
            status: status as i32,
            received_in_tx_id: received_in_tx_id.map(|i| i as i64),
            hash: Some(output.hash),
            script: output.unblinded_output.script.as_bytes(),
            input_data: output.unblinded_output.input_data.as_bytes(),
            script_private_key: output.unblinded_output.script_private_key.to_vec(),
            sender_offset_public_key: output.unblinded_output.sender_offset_public_key.to_vec(),
            metadata_signature_nonce: output.unblinded_output.metadata_signature.public_nonce().to_vec(),
            metadata_signature_u_key: output.unblinded_output.metadata_signature.u().to_vec(),
            metadata_signature_v_key: output.unblinded_output.metadata_signature.v().to_vec(),
            coinbase_block_height: coinbase_block_height.map(|bh| bh as i64),
        })
    }

    /// Write this struct to the database
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(outputs::table).values(self.clone()).execute(conn)?;
        Ok(())
    }
}

impl Encryptable<Aes256Gcm> for NewOutputSql {
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

impl From<OutputSql> for NewOutputSql {
    fn from(o: OutputSql) -> Self {
        Self {
            commitment: o.commitment,
            spending_key: o.spending_key,
            value: o.value,
            flags: o.flags,
            maturity: o.maturity,
            status: o.status,
            hash: o.hash,
            script: o.script,
            input_data: o.input_data,
            script_private_key: o.script_private_key,
            sender_offset_public_key: o.sender_offset_public_key,
            metadata_signature_nonce: o.metadata_signature_nonce,
            metadata_signature_u_key: o.metadata_signature_u_key,
            metadata_signature_v_key: o.metadata_signature_v_key,
            received_in_tx_id: o.received_in_tx_id,
            coinbase_block_height: o.coinbase_block_height,
        }
    }
}
