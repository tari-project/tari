// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The single commitment mask check shared by every path that decides whether a stored output may be (or stay)
//! spendable: the one-off invalid mask migration, TXO revalidation of Invalid outputs, and manually applied
//! validation state fixes. Keeping one implementation stops the paths from drifting apart.

use std::fmt::{Display, Formatter};

use log::*;
use tari_common_types::types::CompressedCommitment;
use tari_transaction_components::key_manager::{TariKeyId, TransactionKeyManagerInterface};
use tari_utilities::hex::Hex;

use crate::output_manager_service::storage::{OutputStatus, models::DbWalletOutput};

const LOG_TARGET: &str = "wallet::output_manager_service::commitment_mask";

/// Result of checking that a commitment opens to `value*H + mask*G` for a value and commitment mask key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskCheck {
    /// The commitment opens to the value under the mask
    Valid,
    /// The check ran and the commitment does not open to the value under the mask (`verify_mask` returned `false`)
    Mismatch,
    /// The check could not be performed (bad key id, bad commitment, key derivation failure, ...)
    Unverifiable(String),
}

impl MaskCheck {
    pub fn is_valid(&self) -> bool {
        matches!(self, MaskCheck::Valid)
    }
}

impl Display for MaskCheck {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MaskCheck::Valid => write!(f, "commitment mask verifies"),
            MaskCheck::Mismatch => write!(f, "commitment does not open to the stored value and mask"),
            MaskCheck::Unverifiable(reason) => write!(f, "commitment mask could not be verified: {reason}"),
        }
    }
}

/// Check that `commitment` opens to `value` under the commitment mask key `commitment_mask_key_id`.
pub fn check_commitment_mask<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    commitment: &CompressedCommitment,
    commitment_mask_key_id: &TariKeyId,
    value: u64,
) -> MaskCheck {
    match key_manager.verify_mask(commitment, commitment_mask_key_id, value) {
        Ok(true) => MaskCheck::Valid,
        Ok(false) => MaskCheck::Mismatch,
        Err(e) => MaskCheck::Unverifiable(format!("verify_mask failed: {e}")),
    }
}

/// Check a stored output's commitment against its stored value and commitment mask key.
pub fn check_output_mask<KM: TransactionKeyManagerInterface>(key_manager: &KM, output: &DbWalletOutput) -> MaskCheck {
    check_commitment_mask(
        key_manager,
        &output.commitment,
        output.wallet_output.commitment_mask_key_id(),
        output.wallet_output.value().as_u64(),
    )
}

/// Statuses from which an output can be moved back to a spendable (or pending incoming) status by revalidation.
pub fn is_revivable_status(status: OutputStatus) -> bool {
    matches!(status, OutputStatus::Invalid | OutputStatus::CancelledInbound)
}

/// `true` if `output`'s commitment mask verifies, so it may be moved back to a spendable status. Otherwise logs at warn
/// with `context` and returns `false`.
pub fn mask_allows_revival<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    output: &DbWalletOutput,
    context: &str,
) -> bool {
    let check = check_output_mask(key_manager, output);
    if check.is_valid() {
        return true;
    }
    warn!(
        target: LOG_TARGET,
        "{context}: not reviving output {} (status {}, value {}): {check}",
        output.commitment.to_hex(),
        output.status,
        output.wallet_output.value()
    );
    false
}

/// `true` if moving `output` to a spendable status must be refused: it is currently in a revivable status (see
/// [`is_revivable_status`]) and its commitment mask does not verify. Refusals are logged at warn with `context`.
pub fn blocks_revival<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    output: &DbWalletOutput,
    context: &str,
) -> bool {
    is_revivable_status(output.status) && !mask_allows_revival(key_manager, output, context)
}

/// Remove every output that [`blocks_revival`], returning the rest and the number removed. Outputs not in a revivable
/// status are passed through unchecked.
pub fn drop_unverifiable_revivals<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    outputs: Vec<DbWalletOutput>,
    context: &str,
) -> (Vec<DbWalletOutput>, usize) {
    let total = outputs.len();
    let kept: Vec<_> = outputs
        .into_iter()
        .filter(|output| !blocks_revival(key_manager, output, context))
        .collect();
    let dropped = total.saturating_sub(kept.len());
    (kept, dropped)
}
