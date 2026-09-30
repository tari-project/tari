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
        // Only the error kind: the full error can embed the (possibly encrypted) key id
        Err(e) => MaskCheck::Unverifiable(format!("verify_mask failed ({})", error_kind(&e))),
    }
}

/// The variant name of an error, without its payload, for logs that must not carry key material.
pub fn error_kind<E: std::fmt::Debug>(error: &E) -> String {
    let debug = format!("{error:?}");
    debug
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or_default()
        .to_string()
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

/// Statuses from which TXO validation revives outputs directly (the invalid-output pass). Used to decide which rows a
/// validation write may overwrite if they have become `Invalid` since they were read.
pub fn is_revivable_status(status: OutputStatus) -> bool {
    matches!(status, OutputStatus::Invalid | OutputStatus::CancelledInbound)
}

/// `true` if `output`'s commitment mask verifies, so it may be moved to a spendable status. Otherwise logs at warn with
/// `context` (commitment and status only; the value at debug) and returns `false`.
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
        "{context}: not moving output {} (status {}) to a spendable status: {check}",
        output.commitment.to_hex(),
        output.status,
    );
    debug!(
        target: LOG_TARGET,
        "{context}: refused output {} has stored value {}",
        output.commitment.to_hex(),
        output.wallet_output.value()
    );
    false
}

/// `true` if moving `output` into a spendable or pending status (Unspent, UnspentMinedUnconfirmed,
/// EncumberedToBeReceived) must be refused because its commitment mask does not verify. The check runs whatever the
/// output's current status is: a Spent or SpentMinedUnconfirmed row can be moved back to unspent by a reorg, so it
/// must not skip the check. Refusals are logged at warn with `context`.
pub fn blocks_revival<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    output: &DbWalletOutput,
    context: &str,
) -> bool {
    !mask_allows_revival(key_manager, output, context)
}

/// Split `outputs` into those that may be moved to a spendable status and those that [`blocks_revival`].
pub fn drop_unverifiable_revivals<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    outputs: Vec<DbWalletOutput>,
    context: &str,
) -> (Vec<DbWalletOutput>, Vec<DbWalletOutput>) {
    outputs
        .into_iter()
        .partition(|output| !blocks_revival(key_manager, output, context))
}
