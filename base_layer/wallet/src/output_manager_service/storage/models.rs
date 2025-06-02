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

use blake2::Blake2b;
use chrono::{DateTime, Utc};
use derivative::Derivative;
use digest::consts::U32;
use tari_common_types::{
    transaction::TxId,
    types::{BlockHash, CompressedCommitment, HashOutput},
};
use tari_core::transactions::{
    transaction_components::{encrypted_data::PaymentId, WalletOutput},
    transaction_key_manager::{TariKeyId, TransactionKeyManagerInterface},
};
use tari_crypto::hashing::DomainSeparatedHasher;
use tari_hashing::PaymentReferenceHashDomain;
use tari_script::{ExecutionStack, TariScript};
use tari_utilities::ByteArray;

use crate::output_manager_service::{
    error::OutputManagerStorageError,
    payment_reference::{PaymentDetails, PaymentDirection, PayRefStatus},
    storage::{OutputSource, OutputStatus},
};

// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DbWalletOutput {
    pub commitment: CompressedCommitment,
    pub wallet_output: WalletOutput,
    pub hash: HashOutput,
    pub status: OutputStatus,
    pub mined_height: Option<u64>,
    pub mined_in_block: Option<BlockHash>,
    pub mined_timestamp: Option<DateTime<Utc>>,
    pub marked_deleted_at_height: Option<u64>,
    pub marked_deleted_in_block: Option<BlockHash>,
    pub spending_priority: SpendingPriority,
    pub source: OutputSource,
    pub received_in_tx_id: Option<TxId>,
    pub spent_in_tx_id: Option<TxId>,
    pub payment_id: PaymentId,
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
        let payment_id = output.payment_id.clone();
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
            payment_id,
        })
    }

    /// Generate a Payment Reference (PayRef) for this output if it has been mined
    /// PayRef = Blake2b_256(block_hash || commitment)
    pub fn generate_payment_reference(&self) -> Option<[u8; 32]> {
        if let Some(block_hash) = &self.mined_in_block {
            let mut hasher = DomainSeparatedHasher::<Blake2b<U32>, PaymentReferenceHashDomain>::new_with_label("payment_reference");
            hasher.update(block_hash.as_slice());
            hasher.update(self.commitment.as_bytes());
            let mut output = [0u8; 32];
            hasher.finalize_into_reset(digest::generic_array::GenericArray::from_mut_slice(&mut output));
            Some(output)
        } else {
            None
        }
    }

    /// Get the PayRef status based on confirmation requirements
    pub fn get_payment_reference_status(&self, current_tip_height: u64, required_confirmations: u64) -> PayRefStatus {
        if let Some(mined_height) = self.mined_height {
            // Handle unsynced wallet case: if current_tip_height is 0 or very low,
            // treat all mined outputs as having sufficient confirmations
            let (confirmations, is_unsynced_mode) = if current_tip_height == 0 || current_tip_height < mined_height {
                // For unsynced wallets, treat all mined outputs as confirmed
                // Use a high confirmation count to indicate it's definitely confirmed
                (required_confirmations + 1, true)
            } else {
                (current_tip_height.saturating_sub(mined_height) + 1, false)
            };
            
            log::debug!(
                target: "wallet::output_manager_service::models",
                "payref_debug: confirmation calc - current_tip: {}, mined_height: {}, confirmations: {}, required: {}, unsynced_mode: {}",
                current_tip_height, mined_height, confirmations, required_confirmations, is_unsynced_mode
            );
            
            if confirmations >= required_confirmations {
                if let Some(payref) = self.generate_payment_reference() {
                    PayRefStatus::Available(payref, confirmations)
                } else {
                    PayRefStatus::InvalidOutput
                }
            } else {
                let blocks_remaining = required_confirmations.saturating_sub(confirmations);
                PayRefStatus::Pending(confirmations, blocks_remaining)
            }
        } else {
            PayRefStatus::NotMined
        }
    }

    /// Check if this output's PayRef matches the given reference
    pub fn matches_payment_reference(&self, payref: &[u8; 32]) -> bool {
        if let Some(generated_payref) = self.generate_payment_reference() {
            generated_payref == *payref
        } else {
            false
        }
    }

    /// Get payment details for this output if it has a valid PayRef
    pub fn get_payment_details(&self, current_tip_height: u64, required_confirmations: u64) -> Option<PaymentDetails> {
        match self.get_payment_reference_status(current_tip_height, required_confirmations) {
            PayRefStatus::Available(payref, confirmations) => Some(PaymentDetails {
                payment_reference: payref,
                commitment: self.commitment.clone(),
                amount: self.wallet_output.value,
                block_height: self.mined_height?,
                block_hash: self.mined_in_block.clone()?,
                mined_timestamp: self.mined_timestamp,
                direction: self.infer_direction(),
                status: self.status,
                confirmations,
                payment_id: Some(self.payment_id.to_bytes()),
            }),
            _ => None,
        }
    }

    /// Infer the direction of this output based on its source
    pub fn infer_direction(&self) -> PaymentDirection {
        match self.source {
            OutputSource::Coinbase => PaymentDirection::Received,
            OutputSource::OneSided => PaymentDirection::Received,
            OutputSource::StealthOneSided => PaymentDirection::Received,
            OutputSource::HtlcRefund => PaymentDirection::Received,
            OutputSource::AtomicSwap => PaymentDirection::Received,
            OutputSource::Standard => {
                // For standard outputs, we need to determine based on other factors
                // This is a simplification - in practice we'd check transaction details
                if self.received_in_tx_id.is_some() {
                    PaymentDirection::Received
                } else {
                    PaymentDirection::Sent
                }
            },
            OutputSource::NonStandardScript => PaymentDirection::Received,
            OutputSource::Burn => PaymentDirection::Sent,
            OutputSource::ValidatorNodeRegistration => PaymentDirection::Sent,
            OutputSource::CodeTemplateRegistration => PaymentDirection::Sent,
        }
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
