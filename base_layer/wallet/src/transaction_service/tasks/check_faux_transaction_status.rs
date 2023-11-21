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

use std::sync::Arc;

use log::*;
use tari_common_types::{
    transaction::TransactionStatus,
    types::{BlockHash, FixedHash},
};

use crate::{
    output_manager_service::{handle::OutputManagerHandle, storage::OutputStatus},
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
pub async fn check_faux_transactions<TBackend: 'static + TransactionBackend>(
    mut output_manager: OutputManagerHandle,
    db: TransactionDatabase<TBackend>,
    event_publisher: TransactionEventSender,
    tip_height: u64,
) {
    let mut all_faux_transactions: Vec<CompletedTransaction> = match db.get_imported_transactions() {
        Ok(txs) => txs,
        Err(e) => {
            error!(target: LOG_TARGET, "Problem retrieving imported transactions: {}", e);
            return;
        },
    };
    let mut unconfirmed_faux = match db.get_unconfirmed_faux_transactions() {
        Ok(txs) => txs,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Problem retrieving unconfirmed faux transactions: {}", e
            );
            return;
        },
    };
    all_faux_transactions.append(&mut unconfirmed_faux);
    // Reorged faux transactions cannot be detected by excess signature, thus use last known confirmed transaction
    // height or current tip height with safety margin to determine if these should be returned
    let last_mined_transaction = match db.fetch_last_mined_transaction() {
        Ok(tx) => tx,
        Err(_) => None,
    };
    let height_with_margin = tip_height.saturating_sub(100);
    let check_height = if let Some(tx) = last_mined_transaction {
        tx.mined_height.unwrap_or(height_with_margin)
    } else {
        height_with_margin
    };
    let mut confirmed_faux = match db.get_confirmed_faux_transactions_from_height(check_height) {
        Ok(txs) => txs,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Problem retrieving confirmed faux transactions: {}", e
            );
            return;
        },
    };
    all_faux_transactions.append(&mut confirmed_faux);

    debug!(
        target: LOG_TARGET,
        "Checking {} faux transaction statuses",
        all_faux_transactions.len()
    );
    for tx in all_faux_transactions {
        let output_statuses_for_tx_id = match output_manager.get_output_statuses_for_tx_id(tx.tx_id).await {
            Ok(s) => s,
            Err(e) => {
                error!(target: LOG_TARGET, "Problem retrieving output statuses: {}", e);
                return;
            },
        };
        let some_outputs_spent = !output_statuses_for_tx_id
            .statuses
            .iter()
            .any(|s| s != &OutputStatus::Unspent);
        let mined_height = if let Some(height) = output_statuses_for_tx_id.mined_height {
            height
        } else {
            tip_height
        };
        let mined_in_block: BlockHash = if let Some(hash) = output_statuses_for_tx_id.block_hash {
            hash
        } else {
            FixedHash::zero()
        };
        let is_valid = tip_height >= mined_height;
        let was_confirmed = tx.status == TransactionStatus::FauxConfirmed;
        let is_confirmed =
            tip_height.saturating_sub(mined_height) >= TransactionServiceConfig::default().num_confirmations_required;
        let num_confirmations = tip_height - mined_height;
        debug!(
            target: LOG_TARGET,
            "Updating faux transaction: TxId({}), mined_height({}), is_confirmed({}), num_confirmations({}), \
             no_outputs_spent({}), is_valid({})",
            tx.tx_id,
            mined_height,
            is_confirmed,
            num_confirmations,
            some_outputs_spent,
            is_valid,
        );
        let result = db.set_transaction_mined_height(
            tx.tx_id,
            mined_height,
            mined_in_block,
            tx.mined_timestamp
                .map_or(0, |mined_timestamp| mined_timestamp.timestamp() as u64),
            num_confirmations,
            is_confirmed,
            true,
        );
        if let Err(e) = result {
            error!(
                target: LOG_TARGET,
                "Error setting faux transaction to mined confirmed: {}", e
            );
        } else {
            // Only send an event if the transaction was not previously confirmed OR was previously confirmed and is
            // now not confirmed (i.e. confirmation changed)
            if !(was_confirmed && is_confirmed) {
                let transaction_event = if is_confirmed {
                    TransactionEvent::FauxTransactionConfirmed {
                        tx_id: tx.tx_id,
                        is_valid,
                    }
                } else {
                    TransactionEvent::FauxTransactionUnconfirmed {
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
