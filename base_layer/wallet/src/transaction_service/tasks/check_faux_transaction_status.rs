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

const SAFETY_HEIGHT_MARGIN: u64 = 1000;

use std::sync::Arc;

use log::*;
use tari_common_types::types::FixedHash;

use crate::{
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{
        config::TransactionServiceConfig,
        handle::{TransactionEvent, TransactionEventSender},
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::CompletedTransaction,
        },
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::service";

#[allow(clippy::too_many_lines)]
pub async fn check_detected_transactions<TBackend: 'static + TransactionBackend>(
    mut output_manager: OutputManagerHandle,
    db: TransactionDatabase<TBackend>,
    event_publisher: TransactionEventSender,
    tip_height: u64,
) {
    // Reorged faux transactions cannot be detected by excess signature, thus use last known confirmed transaction
    // height or current tip height with safety margin to determine if these should be returned
    let last_mined_transaction = match db.fetch_last_mined_transaction() {
        Ok(tx) => tx,
        Err(_) => None,
    };

    let height_with_margin = tip_height.saturating_sub(SAFETY_HEIGHT_MARGIN);
    let check_height = if let Some(tx) = last_mined_transaction {
        tx.mined_height.unwrap_or(height_with_margin)
    } else {
        height_with_margin
    };

    let mut all_detected_transactions: Vec<CompletedTransaction> = match db.get_imported_transactions() {
        Ok(txs) => txs,
        Err(e) => {
            error!(target: LOG_TARGET, "Problem retrieving imported transactions: {}", e);
            return;
        },
    };
    let mut unconfirmed_detected = match db.get_unconfirmed_detected_transactions() {
        Ok(txs) => txs,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Problem retrieving unconfirmed detected transactions: {}", e
            );
            return;
        },
    };
    all_detected_transactions.append(&mut unconfirmed_detected);

    let mut unmined_coinbases_detected = match db.get_unmined_coinbase_transactions(height_with_margin) {
        Ok(txs) => txs,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Problem retrieving unmined coinbase transactions: {}", e
            );
            return;
        },
    };
    all_detected_transactions.append(&mut unmined_coinbases_detected);

    let mut confirmed_dectected = match db.get_confirmed_detected_transactions_from_height(check_height) {
        Ok(txs) => txs,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Problem retrieving confirmed detected transactions: {}", e
            );
            return;
        },
    };
    all_detected_transactions.append(&mut confirmed_dectected);

    debug!(
        target: LOG_TARGET,
        "Checking {} detected transaction statuses",
        all_detected_transactions.len()
    );
    for tx in all_detected_transactions {
        let output_info_for_tx_id = match output_manager.get_output_info_for_tx_id(tx.tx_id).await {
            Ok(s) => s,
            Err(e) => {
                error!(target: LOG_TARGET, "Problem retrieving output statuses: {}", e);
                return;
            },
        };
        // Its safe to assume that statuses should be the same as they are all in the same transaction and they cannot
        // be different.
        let output_status = output_info_for_tx_id.statuses[0];
        if output_info_for_tx_id.mined_height.is_none() || output_info_for_tx_id.block_hash.is_none() {
            // this means the transaction is not detected as mined
            if let Err(e) = db.set_transaction_as_unmined(tx.tx_id) {
                error!(
                    target: LOG_TARGET,
                    "Error setting faux transaction to unmined: {}", e
                );
            }
            continue;
        }
        let mined_height = output_info_for_tx_id.mined_height.unwrap_or(0);
        let mined_in_block = output_info_for_tx_id.block_hash.unwrap_or(FixedHash::zero());
        let is_valid = tip_height >= mined_height;
        let previously_confirmed = tx.status.is_confirmed();
        let must_be_confirmed =
            tip_height.saturating_sub(mined_height) >= TransactionServiceConfig::default().num_confirmations_required;
        let num_confirmations = tip_height.saturating_sub(mined_height);
        debug!(
            target: LOG_TARGET,
            "Updating faux transaction: TxId({}), mined_height({}), must_be_confirmed({}), num_confirmations({}), \
             output_status({}), is_valid({})",
            tx.tx_id,
            mined_height,
            must_be_confirmed,
            num_confirmations,
            output_status,
            is_valid,
        );
        let result = db.set_transaction_mined_height(
            tx.tx_id,
            mined_height,
            mined_in_block,
            tx.mined_timestamp
                .map_or(0, |mined_timestamp| mined_timestamp.timestamp() as u64),
            num_confirmations,
            must_be_confirmed,
            &tx.status,
        );
        if let Err(e) = result {
            error!(
                target: LOG_TARGET,
                "Error setting faux transaction to mined confirmed: {}", e
            );
        } else {
            // Only send an event if the transaction was not previously confirmed OR was previously confirmed and is
            // now not confirmed (i.e. confirmation changed)
            if !(previously_confirmed && must_be_confirmed) {
                let transaction_event = if must_be_confirmed {
                    TransactionEvent::DetectedTransactionConfirmed {
                        tx_id: tx.tx_id,
                        is_valid,
                    }
                } else {
                    TransactionEvent::DetectedTransactionUnconfirmed {
                        tx_id: tx.tx_id,
                        num_confirmations: 0,
                        is_valid,
                    }
                };
                let _size = event_publisher.send(Arc::new(transaction_event)).map_err(|e| {
                    trace!(
                        target: LOG_TARGET,
                        "Error sending event, usually because there are no subscribers: {:?}",
                        e
                    );
                    e
                });
            }
        }
    }
}
