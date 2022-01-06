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

use log::*;

use crate::{
    output_manager_service::{handle::OutputManagerHandle, storage::models::OutputStatus},
    transaction_service::{
        config::TransactionServiceConfig,
        storage::database::{TransactionBackend, TransactionDatabase},
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::service";

pub async fn check_imported_transactions<TBackend: 'static + TransactionBackend>(
    mut output_manager: OutputManagerHandle,
    db: TransactionDatabase<TBackend>,
) {
    let imported_transactions = match db.get_imported_transactions().await {
        Ok(txs) => txs,
        Err(e) => {
            error!(target: LOG_TARGET, "Problem retrieving imported transactions: {}", e);
            return;
        },
    };

    for tx in imported_transactions.into_iter() {
        let status = match output_manager.get_output_statuses_by_tx_id(tx.tx_id).await {
            Ok(s) => s,
            Err(e) => {
                error!(target: LOG_TARGET, "Problem retrieving output statuses: {}", e);
                return;
            },
        };
        if !status.iter().any(|s| s != &OutputStatus::Unspent) {
            debug!(
                target: LOG_TARGET,
                "Faux Transaction (TxId: {}) updated to confirmed", tx.tx_id
            );
            if let Err(e) = db
                .set_transaction_mined_height(
                    tx.tx_id,
                    true,
                    0,
                    vec![0u8; 32],
                    TransactionServiceConfig::default().num_confirmations_required,
                    true,
                )
                .await
            {
                error!(
                    target: LOG_TARGET,
                    "Error setting faux transaction to mined confirmed: {}", e
                );
            }
        }
    }
}
