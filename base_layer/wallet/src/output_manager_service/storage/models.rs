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

use std::{cmp::Ordering, convert::TryFrom};

use chrono::NaiveDateTime;
use derivative::Derivative;
use tari_common_types::{
    transaction::TxId,
    types::{BlockHash, Commitment, HashOutput},
};
use tari_core::transactions::{
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    transaction_components::WalletOutput,
};
use tari_script::{ExecutionStack, TariScript};

use crate::output_manager_service::{
    error::OutputManagerStorageError,
    storage::{OutputSource, OutputStatus},
};

// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DbWalletOutput {
    pub commitment: Commitment,
    pub wallet_output: WalletOutput,
    pub hash: HashOutput,
    pub status: OutputStatus,
    pub mined_height: Option<u64>,
    pub mined_in_block: Option<BlockHash>,
    pub mined_timestamp: Option<NaiveDateTime>,
    pub marked_deleted_at_height: Option<u64>,
    pub marked_deleted_in_block: Option<BlockHash>,
    pub spending_priority: SpendingPriority,
    pub source: OutputSource,
    pub received_in_tx_id: Option<TxId>,
    pub spent_in_tx_id: Option<TxId>,
}

impl DbWalletOutput {
    pub async fn from_wallet_output<KM: TransactionKeyManagerInterface>(
        output: WalletOutput,
        key_manager: &KM,
        spend_priority: Option<SpendingPriority>,
        source: OutputSource,
        received_in_tx_id: Option<TxId>,
        spent_in_tx_id: Option<TxId>,
    ) -> Result<DbWalletOutput, OutputManagerStorageError> {
        let tx_output = output.to_transaction_output(key_manager).await?;
        Ok(DbWalletOutput {
            hash: tx_output.hash(),
            commitment: tx_output.commitment,
            wallet_output: output,
            status: OutputStatus::NotStored,
            mined_height: None,
            mined_in_block: None,
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

impl From<DbWalletOutput> for WalletOutput {
    fn from(value: DbWalletOutput) -> WalletOutput {
        value.wallet_output
    }
}

impl PartialEq for DbWalletOutput {
    fn eq(&self, other: &DbWalletOutput) -> bool {
        self.wallet_output.value == other.wallet_output.value
    }
}

impl PartialOrd<DbWalletOutput> for DbWalletOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DbWalletOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.wallet_output.value.cmp(&other.wallet_output.value)
    }
}

impl Eq for DbWalletOutput {}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SpendingPriority {
    Normal,
    HtlcSpendAsap,
}

impl TryFrom<u32> for SpendingPriority {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SpendingPriority::Normal),
            1 => Ok(SpendingPriority::HtlcSpendAsap),
            _ => Err(format!("Invalid spending priority value: {}", value)),
        }
    }
}

impl From<SpendingPriority> for i32 {
    fn from(value: SpendingPriority) -> Self {
        match value {
            SpendingPriority::Normal => 0,
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
