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
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    sync::Arc,
};

use chrono::{Duration, Utc};
use log::*;
use tari_common_types::types::{BlockHash, FixedHash};
use tari_comms::{peer_manager::Peer, protocol::rpc::RpcError::RequestFailed};
use tari_core::{
    base_node::rpc::BaseNodeWalletRpcClient,
    blocks::BlockHeader,
    proto::base_node::{QueryDeletedRequest, UtxoQueryRequest},
};
use tari_utilities::hex::Hex;
use tokio::sync::watch;

use crate::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::{
        config::OutputManagerServiceConfig,
        error::{OutputManagerError, OutputManagerProtocolError, OutputManagerProtocolErrorExt},
        handle::{OutputManagerEvent, OutputManagerEventSender},
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::DbWalletOutput,
        },
    },
};

const LOG_TARGET: &str = "wallet::output_service::txo_validation_task";

pub struct TxoValidationTask<TBackend, TWalletConnectivity> {
    operation_id: u64,
    db: OutputManagerDatabase<TBackend>,
    base_node_watch: watch::Receiver<Option<Peer>>,
    connectivity: TWalletConnectivity,
    event_publisher: OutputManagerEventSender,
    config: OutputManagerServiceConfig,
}

impl<TBackend, TWalletConnectivity> TxoValidationTask<TBackend, TWalletConnectivity>
where
    TBackend: OutputManagerBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
{
    pub fn new(
        operation_id: u64,
        db: OutputManagerDatabase<TBackend>,
        connectivity: TWalletConnectivity,
        event_publisher: OutputManagerEventSender,
        config: OutputManagerServiceConfig,
    ) -> Self {
        Self {
            operation_id,
            db,
            base_node_watch: connectivity.get_current_base_node_watcher(),
            connectivity,
            event_publisher,
            config,
        }
    }

    pub async fn execute(mut self) -> Result<u64, OutputManagerProtocolError> {
        let mut base_node_client = self
            .connectivity
            .obtain_base_node_wallet_rpc_client()
            .await
            .ok_or(OutputManagerError::Shutdown)
            .for_protocol(self.operation_id)?;

        let base_node_peer = self
            .base_node_watch
            .borrow()
            .as_ref()
            .map(|p| p.node_id.clone())
            .ok_or_else(|| OutputManagerProtocolError::new(self.operation_id, OutputManagerError::BaseNodeChanged))?;
        debug!(
            target: LOG_TARGET,
            "Starting TXO validation protocol with peer {} (Id: {})", base_node_peer, self.operation_id,
        );

        let last_mined_header = self.check_for_reorgs(&mut base_node_client).await?;

        self.update_unconfirmed_outputs(&mut base_node_client).await?;

        self.update_spent_outputs(&mut base_node_client, last_mined_header)
            .await?;

        self.update_invalid_outputs(&mut base_node_client).await?;

        self.publish_event(OutputManagerEvent::TxoValidationSuccess(self.operation_id));
        debug!(
            target: LOG_TARGET,
            "Finished TXO validation protocol from base node {} (Id: {})", base_node_peer, self.operation_id,
        );
        Ok(self.operation_id)
    }

    async fn update_invalid_outputs(
        &self,
        wallet_client: &mut BaseNodeWalletRpcClient,
    ) -> Result<(), OutputManagerProtocolError> {
        let invalid_outputs = self
            .db
            .fetch_invalid_outputs(
                (Utc::now() -
                    Duration::seconds(
                        self.config
                            .num_of_seconds_to_revalidate_invalid_utxos
                            .try_into()
                            .map_err(|_| {
                                OutputManagerProtocolError::new(self.operation_id, OutputManagerError::InvalidConfig)
                            })?,
                    ))
                .timestamp(),
            )
            .for_protocol(self.operation_id)?;

        for batch in invalid_outputs.chunks(self.config.tx_validator_batch_size) {
            let (mined, unmined, tip_height) = self
                .query_base_node_for_outputs(batch, wallet_client)
                .await
                .for_protocol(self.operation_id)?;
            debug!(
                target: LOG_TARGET,
                "Base node returned {} outputs as mined and {} outputs as unmined (Operation ID: {})",
                mined.len(),
                unmined.len(),
                self.operation_id
            );
            for (output, mined_height, mined_in_block, mined_timestamp) in &mined {
                info!(
                    target: LOG_TARGET,
                    "Updating output comm:{}: hash {} as mined at height {} with current tip at {} (Operation ID:
                {})",
                    output.commitment.to_hex(),
                    output.hash.to_hex(),
                    mined_height,
                    tip_height,
                    self.operation_id
                );
                self.update_output_as_mined(output, mined_in_block, *mined_height, tip_height, *mined_timestamp)
                    .await?;
            }
            for output in unmined {
                self.db
                    .update_last_validation_timestamp(output.hash)
                    .for_protocol(self.operation_id)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn update_spent_outputs(
        &self,
        wallet_client: &mut BaseNodeWalletRpcClient,
        last_mined_header_hash: Option<BlockHash>,
    ) -> Result<(), OutputManagerProtocolError> {
        let mined_outputs = self.db.fetch_mined_unspent_outputs().for_protocol(self.operation_id)?;
        if mined_outputs.is_empty() {
            return Ok(());
        }

        for batch in mined_outputs.chunks(self.config.tx_validator_batch_size) {
            debug!(
                target: LOG_TARGET,
                "Asking base node for status of {} commitments (Operation ID: {})",
                batch.len(),
                self.operation_id
            );

            // We have to send positions to the base node because if the base node cannot find the hash of the output
            // we can't tell if the output ever existed, as opposed to existing and was spent.
            // This assumes that the base node has not reorged since the last time we asked.
            let response = wallet_client
                .query_deleted(QueryDeletedRequest {
                    chain_must_include_header: last_mined_header_hash.map(|v| v.to_vec()).unwrap_or_default(),
                    hashes: batch.iter().map(|o| o.hash.to_vec()).collect(),
                })
                .await
                .for_protocol(self.operation_id)?;

            if response.data.len() != batch.len() {
                return Err(OutputManagerProtocolError::new(
                    self.operation_id,
                    OutputManagerError::InconsistentBaseNodeDataError(
                        "Base node did not send back information for all utxos",
                    ),
                ));
            }

            for (output, data) in batch.iter().zip(response.data.iter()) {
                if data.mined_height == 0 {
                    // base node thinks this is unmined or does not know of it.
                    self.db
                        .set_output_to_unmined_and_invalid(output.hash)
                        .for_protocol(self.operation_id)?;
                    continue;
                };
                if data.height_deleted_at == 0 && output.marked_deleted_at_height.is_some() {
                    // this is mined but not yet spent
                    self.db
                        .mark_output_as_unspent(output.hash)
                        .for_protocol(self.operation_id)?;
                    info!(
                        target: LOG_TARGET,
                        "Updating output comm:{}: hash {} as unspent at tip height {} (Operation ID: {})",
                        output.commitment.to_hex(),
                        output.hash.to_hex(),
                        response.height_of_longest_chain,
                        self.operation_id
                    );
                    continue;
                };

                if data.height_deleted_at > 0 {
                    let confirmed = (response.height_of_longest_chain.saturating_sub(data.height_deleted_at)) >=
                        self.config.num_confirmations_required;
                    let block_hash = data.block_deleted_in.clone().try_into().map_err(|_| {
                        OutputManagerProtocolError::new(
                            self.operation_id,
                            OutputManagerError::InconsistentBaseNodeDataError("Base node sent malformed hash"),
                        )
                    })?;
                    self.db
                        .mark_output_as_spent(output.hash, data.height_deleted_at, block_hash, confirmed)
                        .for_protocol(self.operation_id)?;
                    info!(
                        target: LOG_TARGET,
                        "Updating output comm:{}: hash {} as spent at tip height {} (Operation ID: {})",
                        output.commitment.to_hex(),
                        output.hash.to_hex(),
                        response.height_of_longest_chain,
                        self.operation_id
                    );
                }
            }
        }
        Ok(())
    }

    async fn update_unconfirmed_outputs(
        &self,
        wallet_client: &mut BaseNodeWalletRpcClient,
    ) -> Result<(), OutputManagerProtocolError> {
        let unconfirmed_outputs = self.db.fetch_unconfirmed_outputs().for_protocol(self.operation_id)?;

        for batch in unconfirmed_outputs.chunks(self.config.tx_validator_batch_size) {
            debug!(
                target: LOG_TARGET,
                "Asking base node for location of {} unconfirmed outputs by hash (Operation ID: {})",
                batch.len(),
                self.operation_id
            );

            let (mined, unmined, tip_height) = self
                .query_base_node_for_outputs(batch, wallet_client)
                .await
                .for_protocol(self.operation_id)?;
            debug!(
                target: LOG_TARGET,
                "Base node returned {} outputs as mined and {} outputs as unmined (Operation ID: {})",
                mined.len(),
                unmined.len(),
                self.operation_id
            );
            for (output, mined_height, mined_in_block, mined_timestamp) in &mined {
                info!(
                    target: LOG_TARGET,
                    "Updating output comm:{}: hash {} as mined at height {} with current tip at {} (Operation ID: {})",
                    output.commitment.to_hex(),
                    output.hash.to_hex(),
                    mined_height,
                    tip_height,
                    self.operation_id
                );
                self.update_output_as_mined(output, mined_in_block, *mined_height, tip_height, *mined_timestamp)
                    .await?;
            }
        }

        Ok(())
    }

    // Returns the last header found still in the chain
    #[allow(clippy::too_many_lines)]
    async fn check_for_reorgs(
        &mut self,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<Option<BlockHash>, OutputManagerProtocolError> {
        let mut last_mined_header_hash = None;
        debug!(
            target: LOG_TARGET,
            "Checking last mined TXO to see if the base node has re-orged (Operation ID: {})", self.operation_id
        );

        while let Some(last_spent_output) = self.db.get_last_spent_output().for_protocol(self.operation_id)? {
            let mined_height = if let Some(height) = last_spent_output.marked_deleted_at_height {
                height
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Spent output {} should have `marked_deleted_at_height`, setting as unmined to revalidate \
                     (Operation ID: {})",
                    last_spent_output.commitment.to_hex(),
                    self.operation_id
                );
                self.db
                    .set_output_to_unmined_and_invalid(last_spent_output.hash)
                    .for_protocol(self.operation_id)?;
                continue;
            };
            let mined_in_block_hash = if let Some(hash) = last_spent_output.marked_deleted_in_block {
                hash
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Spent output {} should have `marked_deleted_in_block`, setting as unmined to revalidate \
                     (Operation ID: {})",
                    last_spent_output.commitment.to_hex(),
                    self.operation_id
                );
                self.db
                    .set_output_to_unmined_and_invalid(last_spent_output.hash)
                    .for_protocol(self.operation_id)?;
                continue;
            };
            let block_at_height = self
                .get_base_node_block_at_height(mined_height, client)
                .await
                .for_protocol(self.operation_id)?;

            if block_at_height.is_none() || block_at_height.unwrap() != mined_in_block_hash {
                // Chain has reorged since we last
                warn!(
                    target: LOG_TARGET,
                    "The block that output ({}) was spent in has been reorged out, will try to find this output \
                     again, but these funds have potentially been re-orged out of the chain (Operation ID: {})",
                    last_spent_output.commitment.to_hex(),
                    self.operation_id
                );
                self.db
                    .mark_output_as_unspent(last_spent_output.hash)
                    .for_protocol(self.operation_id)?;
            } else {
                debug!(
                    target: LOG_TARGET,
                    "Last mined transaction is still in the block chain according to base node. (Operation ID: {})",
                    self.operation_id
                );
                break;
            }
        }

        while let Some(last_mined_output) = self.db.get_last_mined_output().for_protocol(self.operation_id)? {
            if last_mined_output.mined_height.is_none() || last_mined_output.mined_in_block.is_none() {
                warn!(
                    target: LOG_TARGET,
                    "Output ({}) marked as mined, but mined_height or mined_in_block was empty, invalidating so we \
                     can try to find this output again (Operation ID: {})",
                    last_mined_output.commitment.to_hex(),
                    self.operation_id
                );
                self.db
                    .set_output_to_unmined_and_invalid(last_mined_output.hash)
                    .for_protocol(self.operation_id)?;
                continue;
            }
            let mined_height = last_mined_output.mined_height.unwrap();
            let mined_in_block_hash = last_mined_output.mined_in_block.unwrap();
            let block_at_height = self
                .get_base_node_block_at_height(mined_height, client)
                .await
                .for_protocol(self.operation_id)?;
            if block_at_height.is_none() || block_at_height.unwrap() != mined_in_block_hash {
                // Chain has reorged since we last
                warn!(
                    target: LOG_TARGET,
                    "The block that output ({}) was in has been reorged out, will try to find this output again, but \
                     these funds have potentially been re-orged out of the chain (Operation ID: {})",
                    last_mined_output.commitment.to_hex(),
                    self.operation_id
                );
                self.db
                    .set_output_to_unmined_and_invalid(last_mined_output.hash)
                    .for_protocol(self.operation_id)?;
            } else {
                debug!(
                    target: LOG_TARGET,
                    "Last mined transaction is still in the block chain according to base node (Operation ID: {}).",
                    self.operation_id
                );
                last_mined_header_hash = Some(mined_in_block_hash);
                break;
            }
        }
        Ok(last_mined_header_hash)
    }

    async fn get_base_node_block_at_height(
        &mut self,
        height: u64,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<Option<BlockHash>, OutputManagerError> {
        let result = match client.get_header_by_height(height).await {
            Ok(r) => r,
            Err(rpc_error) => {
                warn!(
                    target: LOG_TARGET,
                    "Error asking base node for header:{} (Operation ID: {})", rpc_error, self.operation_id
                );
                match &rpc_error {
                    RequestFailed(status) => {
                        if status.as_status_code().is_not_found() {
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

        let block_header: BlockHeader = result
            .try_into()
            .map_err(|s| OutputManagerError::InvalidMessageError(format!("Could not convert block header: {}", s)))?;
        Ok(Some(block_header.hash()))
    }

    async fn query_base_node_for_outputs(
        &self,
        batch: &[DbWalletOutput],
        base_node_client: &mut BaseNodeWalletRpcClient,
    ) -> Result<(Vec<(DbWalletOutput, u64, BlockHash, u64)>, Vec<DbWalletOutput>, u64), OutputManagerError> {
        let batch_hashes = batch.iter().map(|o| o.hash.to_vec()).collect();
        trace!(
            target: LOG_TARGET,
            "UTXO hashes queried from base node: {:?}",
            batch.iter().map(|o| o.hash.to_hex()).collect::<Vec<String>>()
        );

        let batch_response = base_node_client
            .utxo_query(UtxoQueryRequest {
                output_hashes: batch_hashes,
            })
            .await?;

        let mut mined = vec![];
        let mut unmined = vec![];

        let mut returned_outputs = HashMap::new();
        for output_proto in &batch_response.responses {
            match FixedHash::try_from(output_proto.output_hash.clone()) {
                Ok(v) => {
                    returned_outputs.insert(v, output_proto);
                },
                Err(_) => {
                    warn!(
                        target: LOG_TARGET,
                        "Malformed utxo hash received from node: {:?}", output_proto
                    )
                },
            };
        }

        for output in batch {
            if let Some(returned_output) = returned_outputs.get(&output.hash) {
                match returned_output.mined_in_block.clone().try_into() {
                    Ok(block_hash) => mined.push((
                        output.clone(),
                        returned_output.mined_height,
                        block_hash,
                        returned_output.mined_timestamp,
                    )),
                    Err(_) => {
                        warn!(
                            target: LOG_TARGET,
                            "Malformed block hash received from node: {:?}", returned_output
                        )
                    },
                };
            } else {
                unmined.push(output.clone());
            }
        }

        Ok((mined, unmined, batch_response.height_of_longest_chain))
    }

    #[allow(clippy::ptr_arg)]
    async fn update_output_as_mined(
        &self,
        tx: &DbWalletOutput,
        mined_in_block: &BlockHash,
        mined_height: u64,
        tip_height: u64,
        mined_timestamp: u64,
    ) -> Result<(), OutputManagerProtocolError> {
        let confirmed = (tip_height - mined_height) >= self.config.num_confirmations_required;

        self.db
            .set_received_output_mined_height_and_status(
                tx.hash,
                mined_height,
                *mined_in_block,
                confirmed,
                mined_timestamp,
            )
            .for_protocol(self.operation_id)?;

        Ok(())
    }

    fn publish_event(&self, event: OutputManagerEvent) {
        if let Err(e) = self.event_publisher.send(Arc::new(event)) {
            debug!(
                target: LOG_TARGET,
                "Error sending event because there are no subscribers: {:?}", e
            );
        }
    }
}
