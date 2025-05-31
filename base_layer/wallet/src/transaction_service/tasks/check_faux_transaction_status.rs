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

const SAFETY_HEIGHT_MARGIN: u64 = 3000;

use std::sync::Arc;

use log::*;
use tari_common_types::types::FixedHash;
use tari_crypto::tari_utilities::ByteArray;

use crate::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{
        config::TransactionServiceConfig,
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionEventSender},
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::CompletedTransaction,
        },
    },
};
use tari_core::{
    proto::base_node as base_node_proto,
    transactions::transaction_components::Transaction,
};
use tari_common_types::types::Signature;

const LOG_TARGET: &str = "wallet::transaction_service::service";

#[allow(clippy::too_many_lines)]
pub async fn check_detected_transactions<TBackend: 'static + TransactionBackend, TWalletConnectivity: WalletConnectivityInterface>(
    mut output_manager: OutputManagerHandle,
    db: TransactionDatabase<TBackend>,
    event_publisher: TransactionEventSender,
    tip_height: u64,
    connectivity: TWalletConnectivity,
) {
    // Reorged faux transactions cannot be detected by excess signature, thus use last known confirmed transaction
    // height or current tip height with safety margin to determine if these should be returned
    let last_mined_transaction = db.fetch_last_mined_transaction().unwrap_or_default();

    let check_height = if let Some(tx) = last_mined_transaction {
        tx.mined_height
            .unwrap_or(tip_height)
            .saturating_sub(SAFETY_HEIGHT_MARGIN)
    } else {
        tip_height.saturating_sub(SAFETY_HEIGHT_MARGIN)
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

    let mut unmined_coinbases_detected = match db.get_unmined_coinbase_transactions(check_height) {
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

    // Also include ALL completed transactions to check for empty kernel signatures
    let mut all_completed = match db.get_completed_transactions(None, None, None) {
        Ok(txs) => txs,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Problem retrieving completed transactions for kernel signature check: {}", e
            );
            return;
        },
    };
    all_detected_transactions.append(&mut all_completed);

    // Remove duplicates based on tx_id
    all_detected_transactions.sort_by(|a, b| a.tx_id.as_u64().cmp(&b.tx_id.as_u64()));
    all_detected_transactions.dedup_by(|a, b| a.tx_id == b.tx_id);

    debug!(
        target: LOG_TARGET,
        "Checking {} detected transaction statuses",
        all_detected_transactions.len()
    );
    trace!(
        target: LOG_TARGET,
        "Checking transaction statuses for {:?} ",
        all_detected_transactions.iter().map(|tx| tx.tx_id).collect::<Vec<_>>()
    );
    for tx in all_detected_transactions {
        let output_info_for_tx_id = match output_manager.get_output_info_for_tx_id(tx.tx_id).await {
            Ok(s) => s,
            Err(e) => {
                error!(target: LOG_TARGET, "Problem retrieving output statuses: {}", e);
                return;
            },
        };
        trace!(
            target: LOG_TARGET,
            "TxId: {}, {:?} ",
            tx.tx_id, output_info_for_tx_id
        );
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

        let log_msg = format!(
            "Updating faux transaction: TxId({}), mined_height({}), must_be_confirmed({}), num_confirmations({}), \
             output_status({}), is_valid({})",
            tx.tx_id, mined_height, must_be_confirmed, num_confirmations, output_status, is_valid
        );
        if num_confirmations <= 5 {
            debug!(target: LOG_TARGET, "{}", log_msg);
        } else {
            trace!(target: LOG_TARGET, "{}", log_msg);
        }

        // Fix kernel signatures for confirmed transactions that have missing kernel signatures
        if must_be_confirmed {
            if tx.transaction.first_kernel_excess_sig().is_none() {
                info!(
                    target: LOG_TARGET,
                    "Transaction {} has no kernel signatures. Attempting to fetch correct signatures from base node.",
                    tx.tx_id
                );
                
                // Attempt to fix the missing kernel signatures by fetching from base node
                let transactions_to_fix = vec![tx.clone()];
                if let Err(e) = fetch_and_update_kernel_signatures(connectivity.clone(), db.clone(), transactions_to_fix).await {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to update kernel signatures for transaction {}: {}",
                        tx.tx_id, e
                    );
                } else {
                    info!(
                        target: LOG_TARGET,
                        "Successfully attempted kernel signature migration for transaction {}",
                        tx.tx_id
                    );
                }
            } else {
                trace!(
                    target: LOG_TARGET,
                    "Transaction {} has kernel signatures present",
                    tx.tx_id
                );
            }
        }

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

/// Fetch kernel signatures from the base node and update transactions that have empty signatures
async fn fetch_and_update_kernel_signatures<TBackend: 'static + TransactionBackend, TWalletConnectivity: WalletConnectivityInterface>(
    mut connectivity: TWalletConnectivity,
    db: TransactionDatabase<TBackend>,
    transactions: Vec<CompletedTransaction>,
) -> Result<(), TransactionServiceError> {
    let mut base_node_client = match connectivity.obtain_base_node_wallet_rpc_client().await {
        Some(client) => client,
        None => {
            return Err(TransactionServiceError::ServiceError(
                "Could not connect to base node wallet RPC client".to_string(),
            ));
        },
    };

    // Collect unique block heights for transactions that have mined_in_block
    let mut height_to_txs: std::collections::HashMap<u64, Vec<&CompletedTransaction>> = std::collections::HashMap::new();
    
    for tx in &transactions {
        if let (Some(_), Some(height)) = (&tx.mined_in_block, tx.mined_height) {
            height_to_txs.entry(height).or_insert_with(Vec::new).push(tx);
        }
    }

    if height_to_txs.is_empty() {
        info!(
            target: LOG_TARGET,
            "No transactions with mined height found for kernel signature migration"
        );
        return Ok(());
    }

    let heights: Vec<u64> = height_to_txs.keys().cloned().collect();
    debug!(
        target: LOG_TARGET,
        "Fetching {} blocks for kernel signature migration: {:?}",
        heights.len(),
        heights
    );

    // Fetch blocks using GetBlocks RPC
    let request = base_node_proto::GetBlocksRequest { heights };
    
    let response = match base_node_client.get_blocks(request).await {
        Ok(response) => response,
        Err(e) => {
            warn!(
                target: LOG_TARGET,
                "Failed to fetch blocks for kernel signature migration: {}",
                e
            );
            return Err(TransactionServiceError::ServiceError(format!(
                "Failed to fetch blocks: {}",
                e
            )));
        }
    };

    let mut updated_count = 0;

    // Process each block in the response
    for historical_block in response.blocks {
        let block_height = historical_block.block.as_ref()
            .and_then(|b| b.header.as_ref())
            .map(|h| h.height)
            .unwrap_or(0);

        // Get transactions for this block height
        let txs_for_height = match height_to_txs.get(&block_height) {
            Some(txs) => txs,
            None => continue,
        };

        let empty_kernels = Vec::new();
        let block_kernels = historical_block.block.as_ref()
            .and_then(|b| b.body.as_ref())
            .map(|b| &b.kernels)
            .unwrap_or(&empty_kernels);

        debug!(
            target: LOG_TARGET,
            "Processing block at height {} with {} kernels for {} transactions",
            block_height,
            block_kernels.len(),
            txs_for_height.len()
        );

        // Process each transaction for this block
        for tx in txs_for_height {
            let mut transaction_updated = false;
            let mut updated_kernels = Vec::new();
            
            for (kernel_index, wallet_kernel) in tx.transaction.body.kernels().iter().enumerate() {
                let wallet_excess = &wallet_kernel.excess;
                
                // Find matching kernel in block by excess commitment
                let matching_block_kernel = block_kernels.iter().find(|block_kernel| {
                    block_kernel.excess.as_ref().map_or(false, |e| e.data.as_slice() == wallet_excess.as_bytes())
                });
                
                if let Some(block_kernel) = matching_block_kernel {
                    // Compare signatures
                    let wallet_sig = &wallet_kernel.excess_sig;
                    let block_sig = block_kernel.excess_sig.as_ref();
                    
                    if let Some(block_sig) = block_sig {
                        // Convert proto signature to internal format
                        if let Ok(block_signature) = Signature::try_from((*block_sig).clone()) {
                            if wallet_sig != &block_signature {
                                info!(
                                    target: LOG_TARGET,
                                    "Found matching kernel for transaction {} kernel {}: updating signature",
                                    tx.tx_id,
                                    kernel_index
                                );
                                
                                // Create updated kernel with correct signature
                                let mut updated_kernel = wallet_kernel.clone();
                                updated_kernel.excess_sig = block_signature;
                                updated_kernels.push(updated_kernel);
                                transaction_updated = true;
                            } else {
                                // Kernel signature is already correct
                                updated_kernels.push(wallet_kernel.clone());
                            }
                        } else {
                            warn!(
                                target: LOG_TARGET,
                                "Failed to convert block kernel signature for transaction {} kernel {}",
                                tx.tx_id,
                                kernel_index
                            );
                            updated_kernels.push(wallet_kernel.clone());
                        }
                    } else {
                        warn!(
                            target: LOG_TARGET,
                            "Block kernel has no signature for transaction {} kernel {}",
                            tx.tx_id,
                            kernel_index
                        );
                        updated_kernels.push(wallet_kernel.clone());
                    }
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Could not find matching kernel in block at height {} for transaction {} kernel {}",
                        block_height,
                        tx.tx_id,
                        kernel_index
                    );
                    // Keep the original kernel if no match found
                    updated_kernels.push(wallet_kernel.clone());
                }
            }
            
            // Update the transaction if any kernels were modified
            if transaction_updated {
                // Create a new transaction with updated kernels
                let updated_transaction = Transaction::new(
                    tx.transaction.body.inputs().clone(),
                    tx.transaction.body.outputs().clone(),
                    updated_kernels,
                    tx.transaction.offset.clone(),
                    tx.transaction.script_offset.clone(),
                );
                
                let mut updated_tx = (*tx).clone();
                updated_tx.transaction = updated_transaction;
                
                // Save the updated transaction to the database
                if let Err(e) = db.update_completed_transaction(tx.tx_id, updated_tx.clone()) {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to update transaction {} with correct kernel signatures: {}",
                        tx.tx_id,
                        e
                    );
                } else {
                    updated_count += 1;
                    info!(
                        target: LOG_TARGET,
                        "Successfully updated transaction {} with correct kernel signatures",
                        tx.tx_id
                    );
                }
            }
        }
    }
    
    if updated_count > 0 {
        info!(
            target: LOG_TARGET,
            "Kernel signature migration completed: {} transactions updated with correct signatures",
            updated_count
        );
    } else {
        info!(
            target: LOG_TARGET,
            "Kernel signature migration completed: no transactions needed updating"
        );
    }
    
    Ok(())
}
