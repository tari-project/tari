// Copyright 2024. The Tari Project
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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{BlockHash, CompressedCommitment, FixedHash};
use tari_core::transactions::tari_amount::MicroMinotari;
use tari_utilities::hex::Hex;
use tari_common_types::transaction::TxId;
use crate::output_manager_service::storage::OutputStatus;

/// Default number of block confirmations required before a PayRef becomes available
pub const DEFAULT_PAYREF_REQUIRED_CONFIRMATIONS: u64 = 5;

/// Configuration for Payment Reference system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayRefConfig {
    /// Number of confirmations required before PayRef is available
    pub required_confirmations: u64,
    /// Display format for PayRefs in UI
    pub display_format: PayRefDisplayFormat,
    /// Auto-copy PayRef to clipboard when clicked
    pub auto_copy_on_click: bool,
    /// Show progress of pending confirmations
    pub show_pending_progress: bool,
    /// Refresh interval for UI updates (seconds)
    pub refresh_interval_seconds: u64,
}

impl Default for PayRefConfig {
    fn default() -> Self {
        Self {
            required_confirmations: DEFAULT_PAYREF_REQUIRED_CONFIRMATIONS,
            display_format: PayRefDisplayFormat::Shortened,
            auto_copy_on_click: true,
            show_pending_progress: true,
            refresh_interval_seconds: 30,
        }
    }
}

/// Display format options for Payment References
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PayRefDisplayFormat {
    /// Show full 64-character hex string
    Full,
    /// Show shortened format (8...8)
    Shortened,
    /// Custom format with specified prefix and suffix character counts
    Custom { prefix_chars: u8, suffix_chars: u8 },
}

/// Direction of a payment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentDirection {
    /// Payment was received
    Received,
    /// Payment was sent
    Sent,
    /// This is change from a sent payment
    SentChange,
}

/// Complete payment details for a Payment Reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDetails {
    /// The Payment Reference (PayRef)
    pub payment_reference: FixedHash,
    /// The commitment of the output
    pub commitment: CompressedCommitment,
    /// Amount of the payment
    pub amount: MicroMinotari,
    /// Block height where the output was mined
    pub block_height: u64,
    /// Hash of the block where the output was mined
    pub block_hash: BlockHash,
    /// Timestamp when the output was mined
    pub mined_timestamp: Option<DateTime<Utc>>,
    /// Direction of the payment (sent/received)
    pub direction: PaymentDirection,
    /// Current status of the output
    pub status: OutputStatus,
    /// Number of confirmations
    pub confirmations: u64,
    /// The payment ID associated with this payment
    pub payment_id: Option<Vec<u8>>,
    /// The internal db ID of the linked transaction
    pub internal_transaction_id: TxId,
}

/// Summary record for Payment Reference listings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRecord {
    /// The Payment Reference (PayRef)
    pub payment_reference: FixedHash,
    /// Amount of the payment
    pub amount: MicroMinotari,
    /// Direction of the payment (sent/received)
    pub direction: PaymentDirection,
    /// Block height where the output was mined
    pub block_height: u64,
    /// Number of confirmations
    pub confirmations: u64,
    /// Timestamp when the output was mined
    pub timestamp: Option<DateTime<Utc>>,
    /// The payment ID associated with this payment
    pub payment_id: Option<Vec<u8>>,
}

/// Verification result for exchange/merchant use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Status of the verification
    pub status: VerificationStatus,
    /// The Payment Reference that was verified
    pub payment_reference: Option<String>,
    /// Amount of the payment (if found)
    pub amount: Option<MicroMinotari>,
    /// Block height where payment was received (if found)
    pub received_height: Option<u64>,
    /// Timestamp when payment was received (if found)  
    pub received_timestamp: Option<DateTime<Utc>>,
    /// Current number of confirmations (if found)
    pub confirmations: Option<u64>,
    /// Whether sufficient confirmations have been reached
    pub sufficient_confirmations: bool,
    /// Human-readable message about the verification result
    pub message: String,
}

/// Status of Payment Reference verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// Payment Reference is valid and has sufficient confirmations
    Verified,
    /// Payment Reference is valid but needs more confirmations
    InsufficientConfirmations,
    /// Payment Reference was not found
    NotFound,
    /// Payment Reference format is invalid
    Invalid,
}

/// Receipt for verified payments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentReceipt {
    /// The Payment Reference
    pub payment_reference: String,
    /// Amount of the payment
    pub amount: MicroMinotari,
    /// Date when payment was received
    pub received_date: DateTime<Utc>,
    /// Block height where payment was mined
    pub block_height: u64,
    /// Number of confirmations at time of verification
    pub confirmations: u64,
    /// Timestamp when verification was performed
    pub verification_timestamp: DateTime<Utc>,
    /// Status of the verification
    pub status: VerificationStatus,
}

/// Utility functions for Payment Reference handling
impl PaymentDetails {
    /// Format the Payment Reference as a hex string
    pub fn payref_hex(&self) -> String {
        self.payment_reference.to_hex()
    }

    /// Check if this payment has sufficient confirmations
    pub fn has_sufficient_confirmations(&self, required: u64) -> bool {
        self.confirmations >= required
    }
}

impl PaymentRecord {
    /// Format the Payment Reference as a hex string
    pub fn payref_hex(&self) -> String {
        self.payment_reference.to_hex()
    }

    /// Format the Payment Reference for display
    pub fn format_payref(&self, format: &PayRefDisplayFormat) -> String {
        let hex = self.payref_hex();
        match format {
            PayRefDisplayFormat::Full => hex,
            PayRefDisplayFormat::Shortened => {
                if hex.len() >= 16 {
                    format!("{}...{}", &hex[0..8], &hex[hex.len() - 8..])
                } else {
                    hex
                }
            },
            PayRefDisplayFormat::Custom {
                prefix_chars,
                suffix_chars,
            } => {
                let prefix = *prefix_chars as usize;
                let suffix = *suffix_chars as usize;
                if hex.len() >= (prefix + suffix) {
                    format!("{}...{}", &hex[0..prefix], &hex[hex.len() - suffix..])
                } else {
                    hex
                }
            },
        }
    }
}

