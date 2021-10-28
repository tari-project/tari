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

use crate::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{
        config::TransactionServiceConfig,
        error::{TransactionServiceError, TransactionServiceProtocolError, TransactionServiceProtocolErrorExt},
        handle::{TransactionEvent, TransactionEventSender},
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::CompletedTransaction,
        },
    },
};
use log::*;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    sync::Arc,
};
use tari_common_types::{transaction::TransactionStatus, types::BlockHash};
use tari_comms::protocol::rpc::{RpcError::RequestFailed, RpcStatusCode::NotFound};
use tari_core::{
    base_node::{
        proto::wallet_rpc::{TxLocation, TxQueryBatchResponse},
        rpc::BaseNodeWalletRpcClient,
    },
    blocks::BlockHeader,
    proto::{base_node::Signatures as SignaturesProto, types::Signature as SignatureProto},
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tracing::instrument;

const LOG_TARGET: &str = "wallet::transaction_service::protocols::validation_protocol";

pub struct TransactionValidationProtocol<TTransactionBackend, TWalletConnectivity> {
    operation_id: u64,
    db: TransactionDatabase<TTransactionBackend>,
    connectivity: TWalletConnectivity,
    config: TransactionServiceConfig,
    event_publisher: TransactionEventSender,
    output_manager_handle: OutputManagerHandle,
}

#[allow(unused_variables)]
impl<TTransactionBackend, TWalletConnectivity> TransactionValidationProtocol<TTransactionBackend, TWalletConnectivity>
where
    TTransactionBackend: TransactionBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
{
    pub fn new(
        operation_id: u64,
        db: TransactionDatabase<TTransactionBackend>,
        connectivity: TWalletConnectivity,
        config: TransactionServiceConfig,
        event_publisher: TransactionEventSender,
        output_manager_handle: OutputManagerHandle,
    ) -> Self {
        Self {
            operation_id,
            db,
            connectivity,
            config,
            event_publisher,
            output_manager_handle,
        }
    }

    #[instrument(name = "validation_protocol::execute", skip(self))]
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        let mut base_node_wallet_client = self
            .connectivity
            .obtain_base_node_wallet_rpc_client()
            .await
            .ok_or(TransactionServiceError::Shutdown)
            .for_protocol(self.operation_id)?;

        self.check_for_reorgs(&mut *base_node_wallet_client).await?;
        info!(
            target: LOG_TARGET,
            "Checking if transactions have been mined since last we checked"
        );
        let unmined_transactions = self
            .db
            .fetch_unconfirmed_transactions()
            .await
            .for_protocol(self.operation_id)
            .unwrap();

        for batch in unmined_transactions.chunks(self.config.max_tx_query_batch_size) {
            let (mined, unmined, tip_info) = self
                .query_base_node_for_transactions(batch, &mut *base_node_wallet_client)
                .await
                .for_protocol(self.operation_id)?;
            info!(
                target: LOG_TARGET,
                "Base node returned {} as mined and {} as unmined",
                mined.len(),
                unmined.len()
            );
            for (tx, mined_height, mined_in_block, num_confirmations) in &mined {
                info!(target: LOG_TARGET, "Updating transaction {} as mined", tx.tx_id);
                self.update_transaction_as_mined(tx, mined_in_block, *mined_height, *num_confirmations)
                    .await?;
            }
            if let Some((tip_height, tip_block)) = tip_info {
                for tx in &unmined {
                    // Treat coinbases separately
                    if tx.is_coinbase() {
                        if tx.coinbase_block_height.unwrap_or_default() <= tip_height {
                            info!(target: LOG_TARGET, "Updated coinbase {} as abandoned", tx.tx_id);
                            self.update_coinbase_as_abandoned(
                                tx,
                                &tip_block,
                                tip_height,
                                tip_height.saturating_sub(tx.coinbase_block_height.unwrap_or_default()),
                            )
                            .await?;
                        } else {
                            info!(
                                target: LOG_TARGET,
                                "Coinbase not found, but it is for a block that is not yet in the chain. Coinbase \
                                 height: {}, tip height:{}",
                                tx.coinbase_block_height.unwrap_or_default(),
                                tip_height
                            );
                        }
                    } else {
                        info!(target: LOG_TARGET, "Updated transaction {} as unmined", tx.tx_id);
                        self.update_transaction_as_unmined(tx).await?;
                    }
                }
            }
        }
        self.publish_event(TransactionEvent::TransactionValidationSuccess(self.operation_id));
        Ok(self.operation_id)
    }

    fn publish_event(&self, event: TransactionEvent) {
        if let Err(e) = self.event_publisher.send(Arc::new(event)) {
            debug!(
                target: LOG_TARGET,
                "Error sending event because there are no subscribers: {:?}", e
            );
        }
    }

    #[instrument(name = "validation_protocol::check_for_reorgs", skip(self, client))]
    async fn check_for_reorgs(
        &mut self,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<(), TransactionServiceProtocolError> {
        info!(
            target: LOG_TARGET,
            "Checking last mined transactions to see if the base node has re-orged"
        );
        while let Some(last_mined_transaction) = self
            .db
            .fetch_last_mined_transaction()
            .await
            .for_protocol(self.operation_id)?
        {
            let mined_height = last_mined_transaction
                .mined_height
                .ok_or_else(|| {
                    TransactionServiceError::ServiceError(
                        "fetch_last_mined_transaction() should return a transaction with a mined_height".to_string(),
                    )
                })
                .for_protocol(self.operation_id)?;
            let mined_in_block_hash = last_mined_transaction
                .mined_in_block
                .clone()
                .ok_or_else(|| {
                    TransactionServiceError::ServiceError(
                        "fetch_last_mined_transaction() should return a transaction with a mined_in_block hash"
                            .to_string(),
                    )
                })
                .for_protocol(self.operation_id)?;

            let block_at_height = self
                .get_base_node_block_at_height(mined_height, client)
                .await
                .for_protocol(self.operation_id)?;

            if block_at_height.is_none() || block_at_height.unwrap() != mined_in_block_hash {
                // Chain has reorged since we last
                warn!(
                    target: LOG_TARGET,
                    "The block that transaction (excess:{}) was in has been reorged out, will try to find this \
                     transaction again, but these funds have potentially been re-orged out of the chain",
                    last_mined_transaction
                        .transaction
                        .body
                        .kernels()
                        .first()
                        .map(|k| k.excess.to_hex())
                        .unwrap()
                );
                self.update_transaction_as_unmined(&last_mined_transaction).await?;
            } else {
                info!(
                    target: LOG_TARGET,
                    "Last mined transaction is still in the block chain according to base node."
                );
                break;
            }
        }
        Ok(())
    }

    #[instrument(
        name = "validation_protocol::query_base_node_for_transactions",
        skip(self, batch, base_node_client)
    )]
    async fn query_base_node_for_transactions(
        &self,
        batch: &[CompletedTransaction],
        base_node_client: &mut BaseNodeWalletRpcClient,
    ) -> Result<
        (
            Vec<(CompletedTransaction, u64, BlockHash, u64)>,
            Vec<CompletedTransaction>,
            Option<(u64, BlockHash)>,
        ),
        TransactionServiceError,
    > {
        let mut mined = vec![];
        let mut unmined = vec![];

        let mut batch_signatures = HashMap::new();
        for tx in batch.iter() {
            // Imported transactions do not have a signature
            if let Some(sig) = tx.transaction.first_kernel_excess_sig() {
                batch_signatures.insert(sig.clone(), tx);
            }
        }

        if batch_signatures.is_empty() {
            info!(target: LOG_TARGET, "No transactions needed to query with the base node");
            return Ok((mined, unmined, None));
        }

        info!(
            target: LOG_TARGET,
            "Asking base node for location of {} transactions by excess signature",
            batch.len()
        );

        let batch_response = base_node_client
            .transaction_batch_query(SignaturesProto {
                sigs: batch_signatures
                    .keys()
                    .map(|s| SignatureProto::from(s.clone()))
                    .collect(),
            })
            .await?;

        for response_proto in batch_response.responses {
            let response = TxQueryBatchResponse::try_from(response_proto)
                .map_err(TransactionServiceError::ProtobufConversionError)?;
            let sig = response.signature;
            if let Some(completed_tx) = batch_signatures.get(&sig) {
                if response.location == TxLocation::Mined {
                    mined.push((
                        (*completed_tx).clone(),
                        response.block_height,
                        response.block_hash.unwrap(),
                        response.confirmations,
                    ));
                } else {
                    unmined.push((*completed_tx).clone());
                }
            }
        }
        Ok((
            mined,
            unmined,
            Some((
                batch_response.height_of_longest_chain,
                batch_response.tip_hash.ok_or_else(|| {
                    TransactionServiceError::ProtobufConversionError("Missing `tip_hash` field".to_string())
                })?,
            )),
        ))
    }

    #[instrument(
        name = "validation_protocol::get_base_node_block_at_height",
        skip(self, height, client)
    )]
    async fn get_base_node_block_at_height(
        &mut self,
        height: u64,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<Option<BlockHash>, TransactionServiceError> {
        let result = match client.get_header_by_height(height).await {
            Ok(r) => r,
            Err(rpc_error) => {
                warn!(target: LOG_TARGET, "Error asking base node for header:{}", rpc_error);
                match &rpc_error {
                    RequestFailed(status) => {
                        if status.status_code() == NotFound {
                            return Ok(None);
                        } else {
                            return Err(rpc_error.into());
                        }
                    },
                    _ => {
                        return Err(rpc_error.into());
                    },
                }
            },
        };

        let block_header: BlockHeader = result.try_into().map_err(|s| {
            TransactionServiceError::InvalidMessageError(format!("Could not convert block header: {}", s))
        })?;
        Ok(Some(block_header.hash()))
    }

    #[allow(clippy::ptr_arg)]
    #[instrument(
        name = "validation_protocol::update_transaction_as_mined",
        skip(self, tx, mined_in_block, mined_height, num_confirmations)
    )]
    async fn update_transaction_as_mined(
        &mut self,
        tx: &CompletedTransaction,
        mined_in_block: &BlockHash,
        mined_height: u64,
        num_confirmations: u64,
    ) -> Result<(), TransactionServiceProtocolError> {
        self.db
            .set_transaction_mined_height(
                tx.tx_id,
                true,
                mined_height,
                mined_in_block.clone(),
                num_confirmations,
                num_confirmations >= self.config.num_confirmations_required,
            )
            .await
            .for_protocol(self.operation_id)?;

        if num_confirmations >= self.config.num_confirmations_required {
            self.publish_event(TransactionEvent::TransactionMined {
                tx_id: tx.tx_id,
                is_valid: true,
            })
        } else {
            self.publish_event(TransactionEvent::TransactionMinedUnconfirmed {
                tx_id: tx.tx_id,
                num_confirmations,
                is_valid: true,
            })
        }

        if tx.status == TransactionStatus::Coinbase {
            if let Err(e) = self.output_manager_handle.set_coinbase_abandoned(tx.tx_id, false).await {
                warn!(
                    target: LOG_TARGET,
                    "Could not mark coinbase output for TxId: {} as not abandoned: {}", tx.tx_id, e
                );
            };
        }

        Ok(())
    }

    #[allow(clippy::ptr_arg)]
    #[instrument(
        name = "validation_protocol::update_coinbase_as_abandoned",
        skip(self, tx, mined_in_block, mined_height, num_confirmations)
    )]
    async fn update_coinbase_as_abandoned(
        &mut self,
        tx: &CompletedTransaction,
        mined_in_block: &BlockHash,
        mined_height: u64,
        num_confirmations: u64,
    ) -> Result<(), TransactionServiceProtocolError> {
        self.db
            .set_transaction_mined_height(
                tx.tx_id,
                false,
                mined_height,
                mined_in_block.clone(),
                num_confirmations,
                num_confirmations >= self.config.num_confirmations_required,
            )
            .await
            .for_protocol(self.operation_id)?;

        if let Err(e) = self.output_manager_handle.set_coinbase_abandoned(tx.tx_id, true).await {
            warn!(
                target: LOG_TARGET,
                "Could not mark coinbase output for TxId: {} as abandoned: {}", tx.tx_id, e
            );
        };

        self.publish_event(TransactionEvent::TransactionCancelled(tx.tx_id));

        Ok(())
    }

    #[instrument(name = "validation_protocol::update_transaction_as_unmined", skip(self, tx))]
    async fn update_transaction_as_unmined(
        &mut self,
        tx: &CompletedTransaction,
    ) -> Result<(), TransactionServiceProtocolError> {
        self.db
            .set_transaction_as_unmined(tx.tx_id)
            .await
            .for_protocol(self.operation_id)?;

        if tx.status == TransactionStatus::Coinbase {
            if let Err(e) = self.output_manager_handle.set_coinbase_abandoned(tx.tx_id, false).await {
                warn!(
                    target: LOG_TARGET,
                    "Could not mark coinbase output for TxId: {} as not abandoned: {}", tx.tx_id, e
                );
            };
        }

        self.publish_event(TransactionEvent::TransactionBroadcast(tx.tx_id));
        Ok(())
    }
}
