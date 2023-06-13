// Copyright 2012. The Tari Project
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

use std::cmp::Ordering;

use chrono::NaiveDateTime;
use derivative::Derivative;
use tari_common_types::{
    transaction::TxId,
    types::{BlockHash, Commitment, HashOutput},
};
use tari_core::{
    transaction_key_manager::{BaseLayerKeyManagerInterface, TariKeyId},
    transactions::transaction_components::KeyManagerOutput,
};
use tari_script::{ExecutionStack, TariScript};

use crate::output_manager_service::{
    error::OutputManagerStorageError,
    storage::{OutputSource, OutputStatus},
};

// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DbKeyManagerOutput {
    pub commitment: Commitment,
    pub key_manager_output: KeyManagerOutput,
    pub hash: HashOutput,
    pub status: OutputStatus,
    pub mined_height: Option<u64>,
    pub mined_in_block: Option<BlockHash>,
    pub mined_mmr_position: Option<u64>,
    pub mined_timestamp: Option<NaiveDateTime>,
    pub marked_deleted_at_height: Option<u64>,
    pub marked_deleted_in_block: Option<BlockHash>,
    pub spending_priority: SpendingPriority,
    pub source: OutputSource,
    pub received_in_tx_id: Option<TxId>,
    pub spent_in_tx_id: Option<TxId>,
}

impl DbKeyManagerOutput {
    pub async fn from_key_manager_output<KM: BaseLayerKeyManagerInterface>(
        output: KeyManagerOutput,
        key_manager: &KM,
        spend_priority: Option<SpendingPriority>,
        source: OutputSource,
        received_in_tx_id: Option<TxId>,
        spent_in_tx_id: Option<TxId>,
    ) -> Result<DbKeyManagerOutput, OutputManagerStorageError> {
        Ok(DbKeyManagerOutput {
            hash: output.hash(key_manager).await?,
            commitment: output.commitment(key_manager).await?,
            key_manager_output: output,
            status: OutputStatus::NotStored,
            mined_height: None,
            mined_in_block: None,
            mined_mmr_position: None,
            mined_timestamp: None,
            marked_deleted_at_height: None,
            marked_deleted_in_block: None,
            spending_priority: spend_priority.unwrap_or(SpendingPriority::Normal),
            source,
            received_in_tx_id,
            spent_in_tx_id,
        })
    }
}

impl From<DbKeyManagerOutput> for KeyManagerOutput {
    fn from(value: DbKeyManagerOutput) -> KeyManagerOutput {
        value.key_manager_output
    }
}

impl PartialEq for DbKeyManagerOutput {
    fn eq(&self, other: &DbKeyManagerOutput) -> bool {
        self.key_manager_output.value == other.key_manager_output.value
    }
}

impl PartialOrd<DbKeyManagerOutput> for DbKeyManagerOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.key_manager_output
            .value
            .partial_cmp(&other.key_manager_output.value)
    }
}

impl Ord for DbKeyManagerOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key_manager_output.value.cmp(&other.key_manager_output.value)
    }
}

impl Eq for DbKeyManagerOutput {}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum SpendingPriority {
    Normal,
    HtlcSpendAsap,
    Unknown,
}

impl From<u32> for SpendingPriority {
    fn from(value: u32) -> Self {
        match value {
            0 => SpendingPriority::Normal,
            1 => SpendingPriority::HtlcSpendAsap,
            _ => SpendingPriority::Unknown,
        }
    }
}

impl From<SpendingPriority> for u32 {
    fn from(value: SpendingPriority) -> Self {
        match value {
            SpendingPriority::Normal | SpendingPriority::Unknown => 0,
            SpendingPriority::HtlcSpendAsap => 1,
        }
    }
}

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct KnownOneSidedPaymentScript {
    pub script_hash: Vec<u8>,
    pub script_key_id: TariKeyId,
    pub script: TariScript,
    pub input: ExecutionStack,
    pub script_lock_height: u64,
}

impl PartialEq for KnownOneSidedPaymentScript {
    fn eq(&self, other: &KnownOneSidedPaymentScript) -> bool {
        self.script_hash == other.script_hash
    }
}
