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

use std::{collections::HashMap, convert::TryInto, sync::Arc};

use log::*;
use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_common_types::{
    transaction::{TransactionStatus, TxId},
    types::{BlockHash, Signature},
};
use tari_core::{self, base_node::rpc::models::TxLocation};
use tari_utilities::{hex::Hex, ByteArray};

use crate::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{
        config::TransactionServiceConfig,
        error::{TransactionServiceError, TransactionServiceProtocolError, TransactionServiceProtocolErrorExt},
        handle::{TransactionEvent, TransactionEventSender},
        protocols::check_faux_transaction_status::check_detected_transactions,
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            sqlite_db::UnconfirmedTransactionInfo,
        },
    },
    OperationId,
};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::validation_protocol";

#[derive(Clone)]
pub struct TransactionValidationProtocol<TTransactionBackend, TWalletConnectivity> {
    operation_id: OperationId,
    db: TransactionDatabase<TTransactionBackend>,
    connectivity: TWalletConnectivity,
    config: TransactionServiceConfig,
    event_publisher: TransactionEventSender,
    output_manager: OutputManagerHandle,
}

#[allow(unused_variables)]
impl<TTransactionBackend, TWalletConnectivity> TransactionValidationProtocol<TTransactionBackend, TWalletConnectivity>
where
    TTransactionBackend: TransactionBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
{
    pub fn new(
        operation_id: OperationId,
        db: TransactionDatabase<TTransactionBackend>,
        connectivity: TWalletConnectivity,
        config: TransactionServiceConfig,
        event_publisher: TransactionEventSender,
        output_manager: OutputManagerHandle,
    ) -> Self {
        Self {
            operation_id,
            db,
            connectivity,
            config,
            event_publisher,
            output_manager,
        }
    }

    pub async fn execute(mut self) -> Result<OperationId, TransactionServiceProtocolError<OperationId>> {
        let base_node_wallet_client = self.connectivity.obtain_base_node_wallet_rpc_client().await;

        self.check_for_reorgs(&base_node_wallet_client).await?;
        debug!(
            target: LOG_TARGET,
            "Checking if transactions have been mined since last we checked (Operation ID: {})", self.operation_id
        );
        // Fetch completed but unconfirmed transactions that were not imported
        let (state_changed, tip) = self.check_unconfirmed(base_node_wallet_client).await?;
        debug!(target: LOG_TARGET, "Using tip height {} for validation", tip);
        check_detected_transactions(
            self.output_manager.clone(),
            self.db.clone(),
            self.event_publisher.clone(),
            tip,
        )
        .await;
        if state_changed {
            self.publish_event(TransactionEvent::TransactionValidationStateChanged(self.operation_id));
        }
        self.publish_event(TransactionEvent::TransactionValidationCompleted(self.operation_id));
        Ok(self.operation_id)
    }

    async fn check_unconfirmed(
        &mut self,
        base_node_wallet_client: <TWalletConnectivity as WalletConnectivityInterface>::BaseNodeClient,
    ) -> Result<(bool, u64), TransactionServiceProtocolError<OperationId>> {
        debug!(
            target: LOG_TARGET,
            "Checking unconfirmed transactions against base node (Operation ID: {})", self.operation_id
        );
        let unconfirmed_transactions = self
            .db
            .fetch_unconfirmed_transactions_info()
            .for_protocol(self.operation_id)
            .unwrap();
        let mut state_changed = false;
        let tip_info = base_node_wallet_client.get_tip_info().await.map_err(|e| {
            TransactionServiceProtocolError::new(self.operation_id, TransactionServiceError::Other(e.to_string()))
        })?;
        let tip = tip_info.metadata.map(|m| m.best_block_height()).unwrap_or(0);
        for batch in unconfirmed_transactions.chunks(self.config.max_tx_query_batch_size) {
            let (mined, unmined) = self
                .query_base_node_for_transactions(batch, &base_node_wallet_client)
                .await
                .for_protocol(self.operation_id)?;
            debug!(
                target: LOG_TARGET,
                "Base node returned {} as mined and {} as unmined (Operation ID: {})",
                mined.len(),
                unmined.len(),
                self.operation_id
            );
            for (mined_tx, mined_height, mined_in_block, mined_timestamp) in &mined {
                debug!(
                    target: LOG_TARGET,
                    "Updating transaction {} as mined (Operation ID: {})",
                    mined_tx.tx_id,
                    self.operation_id
                );
                self.update_transaction_as_mined(
                    mined_tx.tx_id,
                    &mined_tx.status,
                    mined_in_block,
                    *mined_height,
                    tip.saturating_sub(*mined_height),
                    *mined_timestamp,
                )
                .await?;
                state_changed = true;
            }
            for unmined_tx in &unmined {
                debug!(
                    target: LOG_TARGET,
                    "Updated transaction {} as unmined (Operation ID: {})", unmined_tx.tx_id, self.operation_id
                );
                self.update_transaction_as_unmined(unmined_tx.tx_id, &unmined_tx.status)
                    .await?;
            }
        }
        Ok((state_changed, tip))
    }

    fn publish_event(&self, event: TransactionEvent) {
        if let Err(e) = self.event_publisher.send(Arc::new(event)) {
            debug!(
                target: LOG_TARGET,
                "Error sending event because there are no subscribers: {:?}", e
            );
        }
    }

    async fn check_for_reorgs(
        &mut self,
        client: &TWalletConnectivity::BaseNodeClient,
    ) -> Result<(), TransactionServiceProtocolError<OperationId>> {
        debug!(
            target: LOG_TARGET,
            "Checking last mined transactions to see if the base node has re-orged (Operation ID: {})",
            self.operation_id
        );
        let op_id = self.operation_id;
        let last_mined_transaction = self.db.fetch_last_mined_transaction().for_protocol(op_id)?;
        if last_mined_transaction.is_none() {
            debug!(
                target: LOG_TARGET,
                "No last mined transaction found, skipping reorg check (Operation ID: {})", self.operation_id
            );
            return Ok(());
        }
        while let Some(last_mined_transaction) = self.db.fetch_last_mined_transaction().for_protocol(op_id)? {
            debug!(
                target: LOG_TARGET,
                "Checking last mined transaction with ID {}, mined in {:?} for reorgs (Operation ID: {})",
                last_mined_transaction.tx_id,
                last_mined_transaction.mined_height,
                self.operation_id
            );
            let mined_height = last_mined_transaction
                .mined_height
                .ok_or_else(|| {
                    TransactionServiceError::ServiceError(
                        "fetch_last_mined_transaction() should return a transaction with a mined_height".to_string(),
                    )
                })
                .for_protocol(op_id)?;
            let mined_in_block_hash = last_mined_transaction
                .mined_in_block
                .ok_or_else(|| {
                    TransactionServiceError::ServiceError(
                        "fetch_last_mined_transaction() should return a transaction with a mined_in_block hash"
                            .to_string(),
                    )
                })
                .for_protocol(op_id)?;

            let block_at_height = self
                .get_base_node_block_at_height(mined_height, client)
                .await
                .for_protocol(op_id)?;

            if block_at_height.is_none() || block_at_height.unwrap() != mined_in_block_hash {
                // Chain has reorged since we last
                warn!(
                    target: LOG_TARGET,
                    "The block that transaction (excess:{}) was in has been reorged out, will try to find this \
                     transaction again, but these funds have potentially been re-orged out of the chain (Operation \
                     ID: {})",
                    last_mined_transaction
                        .transaction
                        .body
                        .kernels()
                        .first()
                        .map(|k| k.excess.to_hex())
                        .unwrap_or_else(|| "{No Kernel found}".to_string()),
                    self.operation_id
                );
                self.update_transaction_as_unmined(last_mined_transaction.tx_id, &last_mined_transaction.status)
                    .await?;
                self.publish_event(TransactionEvent::TransactionValidationStateChanged(op_id));
            } else {
                debug!(
                    target: LOG_TARGET,
                    "Last mined transaction is still in the block chain according to base node (Operation ID: {}).",
                    self.operation_id
                );
                break;
            }
        }
        Ok(())
    }

    async fn query_base_node_for_transactions(
        &self,
        batch: &[UnconfirmedTransactionInfo],
        base_node_client: &TWalletConnectivity::BaseNodeClient,
    ) -> Result<
        (
            Vec<(UnconfirmedTransactionInfo, u64, BlockHash, u64)>,
            Vec<UnconfirmedTransactionInfo>,
        ),
        TransactionServiceError,
    > {
        let mut mined = vec![];
        let mut unmined = vec![];
        #[allow(clippy::mutable_key_type)]
        let mut batch_signatures = HashMap::new();
        for tx_info in batch {
            // Imported transactions do not have a signature; this is represented by the default signature in info
            if tx_info.signature != Signature::default() {
                batch_signatures.insert(tx_info.signature.clone(), tx_info);
            }
        }

        if batch_signatures.is_empty() {
            debug!(
                target: LOG_TARGET,
                "No transactions needed to query with the base node (Operation ID: {})", self.operation_id
            );
            return Ok((mined, unmined));
        }

        info!(
            target: LOG_TARGET,
            "Asking base node for location of {} transactions by excess signature (Operation ID: {})",
            batch_signatures.len(),
            self.operation_id
        );

        let tip_mined_timestamp = 0;
        for (sig, unconfirmed_tx) in batch_signatures {
            let response = base_node_client
                .transaction_query(
                    sig.get_compressed_public_nonce().as_bytes().to_vec(),
                    sig.get_signature().as_bytes().to_vec(),
                )
                .await
                .map_err(|e| TransactionServiceError::Other(e.to_string()))?;
            if response.location == TxLocation::Mined {
                let (mined_height, mined_hash, timestamp) = match response.mined_height {
                    Some(height) => {
                        let hash = response.mined_header_hash.ok_or_else(|| {
                            TransactionServiceError::Other("Mined header hash is missing".to_string())
                        })?;
                        let timestamp = response
                            .mined_timestamp
                            .ok_or_else(|| TransactionServiceError::Other("Mined timestamp is missing".to_string()))?;
                        (
                            height,
                            hash.try_into().map_err(|e| {
                                TransactionServiceError::Other(format!("Could not convert best block hash: {}", e))
                            })?,
                            timestamp,
                        )
                    },
                    None => {
                        warn!(
                            target: LOG_TARGET,
                            "Transaction {} is mined but has no height (Operation ID: {})",
                            &unconfirmed_tx.tx_id,
                            self.operation_id,
                        );
                        continue;
                    },
                };
                mined.push(((*unconfirmed_tx).clone(), mined_height, mined_hash, timestamp));
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Transaction {} is unmined (Operation ID: {})",
                    &unconfirmed_tx.tx_id,
                    self.operation_id,
                );
                unmined.push((*unconfirmed_tx).clone());
            }
        }

        Ok((mined, unmined))
    }

    async fn get_base_node_block_at_height(
        &mut self,
        height: u64,
        client: &TWalletConnectivity::BaseNodeClient,
    ) -> Result<Option<BlockHash>, TransactionServiceError> {
        let result = match client.get_header_by_height(height).await {
            Ok(r) => r,
            Err(rpc_error) => {
                warn!(
                    target: LOG_TARGET,
                    "Error asking base node for header:{} (Operation ID: {})", rpc_error, self.operation_id
                );
                return Err(TransactionServiceError::Other(format!(
                    "Error asking base node for header at height {}: {}",
                    height, rpc_error
                )));
            },
        };

        Ok(result.map(|x| x.hash))
    }

    #[allow(clippy::ptr_arg)]
    async fn update_transaction_as_mined(
        &mut self,
        tx_id: TxId,
        status: &TransactionStatus,
        mined_in_block: &BlockHash,
        mined_height: u64,
        num_confirmations: u64,
        mined_timestamp: u64,
    ) -> Result<(), TransactionServiceProtocolError<OperationId>> {
        self.db
            .set_transaction_mined_height(
                tx_id,
                mined_height,
                *mined_in_block,
                mined_timestamp,
                num_confirmations,
                num_confirmations >= self.config.num_confirmations_required,
                status,
            )
            .for_protocol(self.operation_id)?;

        if num_confirmations >= self.config.num_confirmations_required {
            if status.is_coinbase() || status.is_imported_from_chain() {
                self.publish_event(TransactionEvent::DetectedTransactionConfirmed { tx_id, is_valid: true })
            } else {
                self.publish_event(TransactionEvent::TransactionMined { tx_id, is_valid: true })
            }
        } else if status.is_coinbase() || status.is_imported_from_chain() {
            self.publish_event(TransactionEvent::DetectedTransactionUnconfirmed {
                tx_id,
                num_confirmations,
                is_valid: true,
            })
        } else {
            self.publish_event(TransactionEvent::TransactionMinedUnconfirmed {
                tx_id,
                num_confirmations,
                is_valid: true,
            })
        }

        Ok(())
    }

    async fn update_transaction_as_unmined(
        &mut self,
        tx_id: TxId,
        status: &TransactionStatus,
    ) -> Result<(), TransactionServiceProtocolError<OperationId>> {
        self.db
            .set_transaction_as_unmined(tx_id)
            .for_protocol(self.operation_id)?;

        self.publish_event(TransactionEvent::TransactionBroadcast(tx_id));
        Ok(())
    }
}
