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
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    sync::Arc,
};

use chrono::{DateTime, Duration, Utc};
use log::*;
use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_common_types::{
    transaction::TxId,
    types::{BlockHash, CompressedCommitment, FixedHash},
};
use tari_transaction_components::rpc::{
    MAX_ALLOWED_QUERY_SIZE,
    models::{DeletedUtxoInfo, GetUtxosDeletedInfoResponse, GetUtxosMinedInfoResponse, TxLocation},
};
use tari_transaction_key_manager::legacy_key_manager::LegacyTransactionKeyManagerInterface;
use tari_utilities::{ByteArray, hex::Hex};

use crate::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::{
        commitment_mask::{blocks_revival, drop_unverifiable_revivals, is_revivable_status},
        config::OutputManagerServiceConfig,
        error::{OutputManagerError, OutputManagerProtocolError, OutputManagerProtocolErrorExt},
        handle::{OutputManagerEvent, OutputManagerEventSender},
        storage::{
            OutputStatus,
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::DbWalletOutput,
            sqlite_db::{ReceivedOutputInfoForBatch, SpentOutputInfoForBatch},
        },
    },
};

const LOG_TARGET: &str = "wallet::output_service::txo_validation_task";


#[derive(Clone)]
pub struct TxoValidationTask<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    operation_id: u64,
    db: OutputManagerDatabase<TBackend>,
    connectivity: TWalletConnectivity,
    event_publisher: OutputManagerEventSender,
    config: OutputManagerServiceConfig,
    key_manager: TKeyManagerInterface,
}

struct MinedOutputInfo {
    output: DbWalletOutput,
    mined_at_height: u64,
    mined_block_hash: FixedHash,
    mined_timestamp: u64,
}

#[derive(Clone, Copy)]
enum DeletedOutputResolution {
    Unmined,
    MinedUnspent { found_height: u64, found_hash: FixedHash },
    MinedSpent { spent_height: u64, spent_hash: FixedHash },
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    TxoValidationTask<TBackend, TWalletConnectivity, TKeyManagerInterface>
where
    TBackend: OutputManagerBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: LegacyTransactionKeyManagerInterface + 'static,
{
    pub fn new(
        operation_id: u64,
        db: OutputManagerDatabase<TBackend>,
        connectivity: TWalletConnectivity,
        event_publisher: OutputManagerEventSender,
        config: OutputManagerServiceConfig,
        key_manager: TKeyManagerInterface,
    ) -> Self {
        Self {
            operation_id,
            db,
            connectivity,
            event_publisher,
            config,
            key_manager,
        }
    }

    /// The number of outputs to query from the base node in one request.
    ///
    /// `tx_validator_batch_size` is operator configurable, so it is clamped here: the base node rejects batch queries
    /// larger than `MAX_ALLOWED_QUERY_SIZE` outright, and a configured `0` would panic `chunks()`.
    fn batch_size(&self) -> usize {
        self.config.tx_validator_batch_size.clamp(1, MAX_ALLOWED_QUERY_SIZE)
    }

    pub async fn execute(mut self) -> Result<u64, OutputManagerProtocolError> {
        if self.config.tx_validator_batch_size != self.batch_size() {
            warn!(
                target: LOG_TARGET,
                "Configured `tx_validator_batch_size` of {} is out of range, using {} instead (Operation ID: {})",
                self.config.tx_validator_batch_size,
                self.batch_size(),
                self.operation_id,
            );
        }
        let mut base_node_client = self.connectivity.obtain_base_node_wallet_rpc_client().await;

        let base_node_peer = base_node_client.get_address().await;
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
        wallet_client: &mut TWalletConnectivity::BaseNodeClient,
    ) -> Result<(), OutputManagerProtocolError> {
        let invalid_outputs = self
            .db
            .fetch_invalid_outputs(
                Utc::now()
                    .checked_sub_signed(Duration::seconds(
                        self.config
                            .num_of_seconds_to_revalidate_invalid_utxos
                            .try_into()
                            .map_err(|_| {
                                OutputManagerProtocolError::new(self.operation_id, OutputManagerError::InvalidConfig)
                            })?,
                    ))
                    .unwrap_or(DateTime::<Utc>::MIN_UTC)
                    .timestamp(),
                &self.key_manager,
            )
            .for_protocol(self.operation_id)?;

        // An Invalid output whose commitment does not open to its stored value and mask must never be revived, even
        // if the base node reports its commitment mined or in the mempool: the stored value would be wrong.
        let (invalid_outputs, not_revivable) = drop_unverifiable_revivals(
            &self.key_manager,
            invalid_outputs,
            "TXO revalidation of invalid outputs",
        );
        if !not_revivable.is_empty() {
            warn!(
                target: LOG_TARGET,
                "{} invalid output(s) failed the commitment mask check and stay Invalid (Operation ID: {})",
                not_revivable.len(),
                self.operation_id
            );
            // Record the check so these are only re-examined after `num_of_seconds_to_revalidate_invalid_utxos`,
            // like invalid outputs the base node does not know
            self.db
                .update_last_validation_timestamps(not_revivable.into_iter().map(|o| o.commitment).collect())
                .for_protocol(self.operation_id)?;
        }

        for batch in invalid_outputs.chunks(self.batch_size()) {
            let (mined, in_mempool, unmined, tip_height) = self
                .query_base_node_for_outputs(batch, wallet_client)
                .await
                .for_protocol(self.operation_id)?;
            debug!(
                target: LOG_TARGET,
                "Base node returned {} mined, {} in mempool, {} unmined invalid outputs (Operation ID: {})",
                mined.len(),
                in_mempool.len(),
                unmined.len(),
                self.operation_id
            );

            let mut mined_updates = Vec::with_capacity(mined.len());
            for mined_info in &mined {
                info!(
                    target: LOG_TARGET,
                    "Updating output comm:{}: hash {} as mined at height {} with current tip at {} (Operation ID: {})",
                    mined_info.output.commitment.to_hex(),
                    mined_info.output.hash.to_hex(),
                    mined_info.mined_at_height,
                    tip_height,
                    self.operation_id
                );
                mined_updates.push(ReceivedOutputInfoForBatch {
                    commitment: mined_info.output.commitment.clone(),
                    mined_height: mined_info.mined_at_height,
                    mined_in_block: mined_info.mined_block_hash,
                    confirmed: tip_height
                        .checked_sub(mined_info.mined_at_height)
                        .is_some_and(|confirmations| confirmations >= self.config.num_confirmations_required),
                    mined_timestamp: mined_info.mined_timestamp,
                });
            }
            if !mined_updates.is_empty() {
                // This pass exists to revive Invalid outputs: every output here was read as Invalid (or cancelled
                // inbound) and has passed the commitment mask check, so it may overwrite an Invalid row.
                self.db
                    .set_received_outputs_mined_height_and_statuses(mined_updates)
                    .for_protocol(self.operation_id)?;
            }

            // Outputs found in the mempool are no longer invalid — restore to EncumberedToBeReceived
            if !in_mempool.is_empty() {
                let mempool_commitments: Vec<_> = in_mempool
                    .iter()
                    .map(|uo| {
                        info!(
                            target: LOG_TARGET,
                            "Restoring invalid output comm:{}: hash {} to EncumberedToBeReceived \
                             (found in mempool) (Operation ID: {})",
                            uo.commitment.to_hex(),
                            uo.hash.to_hex(),
                            self.operation_id
                        );
                        uo.commitment.clone()
                    })
                    .collect();
                self.db
                    .set_outputs_to_encumbered_to_be_received(mempool_commitments)
                    .for_protocol(self.operation_id)?;
            }

            let unmined_info: Vec<_> = unmined.iter().map(|o| o.commitment.clone()).collect();
            if !unmined_info.is_empty() {
                self.db
                    .update_last_validation_timestamps(unmined_info)
                    .for_protocol(self.operation_id)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn update_spent_outputs(
        &self,
        wallet_client: &mut TWalletConnectivity::BaseNodeClient,
        last_mined_header: Option<(u64, BlockHash)>,
    ) -> Result<(), OutputManagerProtocolError> {
        let last_scanned_height = self
            .db
            .get_last_scanned_height()
            .for_protocol(self.operation_id)?
            .unwrap_or(0);
        let mined_outputs = self
            .db
            .fetch_mined_unspent_outputs(&self.key_manager)
            .for_protocol(self.operation_id)?;
        // Every output here is sent for spent detection unfiltered: marking an output spent never makes it spendable.
        // The commitment mask is only checked, per output below, where this pass could lead to a spendable status.
        let mut not_revivable = 0usize;
        debug!(
            target: LOG_TARGET,
            "Found {} mined outputs to validate (Operation ID: {})",
            mined_outputs.len(),
            self.operation_id
        );
        if mined_outputs.is_empty() {
            return Ok(());
        }

        let must_include_header = select_must_include_header(last_mined_header, &mined_outputs)
            .for_protocol(self.operation_id)?
            .to_vec();

        let mut spent = Vec::with_capacity(mined_outputs.len());
        for batch in mined_outputs.chunks(self.batch_size()) {
            debug!(
                target: LOG_TARGET,
                "Asking base node for status of {} commitments (Operation ID: {})",
                batch.len(),
                self.operation_id
            );

            let (resolutions, best_block_height) = self
                .query_deleted_utxos_batch(batch, &must_include_header, wallet_client)
                .await
                .for_protocol(self.operation_id)?;

            let mut unmined_and_invalid = Vec::with_capacity(batch.len());
            let mut remined_candidates = Vec::new();
            let mut unspent = Vec::with_capacity(batch.len());
            for output in batch {
                let resolution = resolutions
                    .get(&output.hash)
                    .copied()
                    .ok_or(OutputManagerError::InconsistentBaseNodeDataError(
                        "Base node did not send back information for all utxos",
                    ))
                    .for_protocol(self.operation_id)?;

                match resolution {
                    DeletedOutputResolution::Unmined => {
                        unmined_and_invalid.push(output.hash);
                    },
                    DeletedOutputResolution::MinedSpent {
                        spent_height,
                        spent_hash,
                    } => {
                        // A spend is recorded whatever the output's mask: Spent is not spendable, and every way back
                        // out of Spent (reorg check, unconfirmed pass, this pass's unspent branch) re-checks the mask
                        spent.push(SpentOutputInfoForBatch {
                            commitment: output.commitment.clone(),
                            confirmed: spent_height
                                .checked_add(self.config.num_confirmations_required)
                                .is_some_and(|confirmed_height| confirmed_height <= last_scanned_height),
                            mark_deleted_at_height: spent_height,
                            mark_deleted_in_block: spent_hash,
                        });
                    },
                    DeletedOutputResolution::MinedUnspent {
                        found_height,
                        found_hash,
                    } => {
                        if output.mined_height != Some(found_height) || output.mined_in_block != Some(found_hash) {
                            // Output was re-mined in a different block or height below the `check_for_reorgs` anchor;
                            // refresh its mined metadata in place while preserving any pending input reservations.
                            if blocks_revival(&self.key_manager, output, "TXO revalidation of re-mined outputs") {
                                not_revivable = not_revivable.saturating_add(1);
                            } else {
                                remined_candidates.push(output.clone());
                            }
                        } else if output.marked_deleted_at_height.is_some() {
                            // only update to unspent if the output is currently marked as spent in our db, and never if
                            // its commitment mask does not verify (the only move into a spendable status in this pass)
                            if blocks_revival(&self.key_manager, output, "TXO revalidation of spent outputs") {
                                not_revivable = not_revivable.saturating_add(1);
                            } else {
                                let confirmed =
                                    best_block_height
                                        .checked_sub(found_height)
                                        .is_some_and(|confirmations| {
                                            confirmations >= self.config.num_confirmations_required
                                        });
                                unspent.push((output.hash, confirmed));
                            }
                        } else {
                            // Already unspent at the canonical mined block/height; leave untouched.
                        }
                    },
                }
            }
            if !remined_candidates.is_empty() {
                self.refresh_remined_outputs(&remined_candidates, &mut unmined_and_invalid, wallet_client)
                    .await?;
            }
            if !unmined_and_invalid.is_empty() {
                self.db
                    .set_outputs_to_unmined_and_invalid(unmined_and_invalid)
                    .for_protocol(self.operation_id)?;
            }
            if !unspent.is_empty() {
                // As in the unconfirmed pass: don't overwrite an Invalid row unless it was read as revivable
                self.db
                    .mark_outputs_as_unspent_guarded(unspent, revivable_hashes(batch))
                    .for_protocol(self.operation_id)?;
            }
        }
        if !spent.is_empty() {
            self.db.mark_outputs_as_spent(spent).for_protocol(self.operation_id)?;
        }
        if not_revivable > 0 {
            warn!(
                target: LOG_TARGET,
                "{not_revivable} mined output(s) failed the commitment mask check and are left unchanged (Operation ID: \
                 {})",
                self.operation_id
            );
        }
        Ok(())
    }

    async fn query_deleted_utxos_batch(
        &self,
        batch: &[DbWalletOutput],
        must_include_header: &[u8],
        wallet_client: &TWalletConnectivity::BaseNodeClient,
    ) -> Result<(HashMap<FixedHash, DeletedOutputResolution>, u64), OutputManagerError> {
        let (request_hashes, expected_hashes) = dedup_batch_hashes(batch);
        let response = wallet_client
            .query_deleted_utxos(request_hashes, must_include_header.to_vec())
            .await
            .map_err(|e| OutputManagerError::BaseNodeClientError(format!("Error querying base node: {e}")))?;

        let resolutions = parse_deleted_utxo_response(&expected_hashes, &response)?;
        Ok((resolutions, response.best_block_height))
    }

    async fn refresh_remined_outputs(
        &self,
        remined_candidates: &[DbWalletOutput],
        unmined_and_invalid: &mut Vec<FixedHash>,
        wallet_client: &mut TWalletConnectivity::BaseNodeClient,
    ) -> Result<(), OutputManagerProtocolError> {
        // Leave outputs reserved for a pending spend (EncumberedToBeSpent / ShortTermEncumberedToBeSpent) untouched so
        // TXO validation neither overwrites their reservation status nor invalidates them.
        let unreserved_candidates: Vec<DbWalletOutput> = remined_candidates
            .iter()
            .filter(|o| {
                !matches!(
                    o.status,
                    OutputStatus::EncumberedToBeSpent | OutputStatus::ShortTermEncumberedToBeSpent
                )
            })
            .cloned()
            .collect();
        if unreserved_candidates.is_empty() {
            return Ok(());
        }

        let (mined, in_mempool, unmined, tip_height) = self
            .query_base_node_for_outputs(&unreserved_candidates, wallet_client)
            .await
            .for_protocol(self.operation_id)?;

        if !mined.is_empty() {
            let mined_updates: Vec<_> = mined
                .iter()
                .map(|mined_info| ReceivedOutputInfoForBatch {
                    commitment: mined_info.output.commitment.clone(),
                    mined_height: mined_info.mined_at_height,
                    mined_in_block: mined_info.mined_block_hash,
                    confirmed: tip_height
                        .checked_sub(mined_info.mined_at_height)
                        .is_some_and(|confirmations| confirmations >= self.config.num_confirmations_required),
                    mined_timestamp: mined_info.mined_timestamp,
                })
                .collect();
            self.db
                .set_received_outputs_mined_height_and_statuses_guarded(
                    mined_updates,
                    revivable_commitments(&unreserved_candidates),
                )
                .for_protocol(self.operation_id)?;
        }

        for uo in &in_mempool {
            unmined_and_invalid.push(uo.hash);
        }
        for uo in &unmined {
            unmined_and_invalid.push(uo.hash);
        }
        Ok(())
    }

    async fn update_unconfirmed_outputs(
        &self,
        wallet_client: &mut TWalletConnectivity::BaseNodeClient,
    ) -> Result<(), OutputManagerProtocolError> {
        let unconfirmed_outputs = self
            .db
            .fetch_unconfirmed_outputs(&self.key_manager)
            .for_protocol(self.operation_id)?;
        // The unconfirmed set includes every output without a mined height, which covers Invalid outputs after a
        // reorg or a full revalidation, and outputs a reorg moved from spent back to unconfirmed. None of them may be
        // made spendable if their commitment mask does not verify.
        let (unconfirmed_outputs, not_revivable) = drop_unverifiable_revivals(
            &self.key_manager,
            unconfirmed_outputs,
            "TXO revalidation of unconfirmed outputs",
        );
        if !not_revivable.is_empty() {
            warn!(
                target: LOG_TARGET,
                "{} unconfirmed output(s) failed the commitment mask check and are left unchanged (Operation ID: {})",
                not_revivable.len(),
                self.operation_id
            );
        }
        debug!(
            target: LOG_TARGET,
            "Found {} unconfirmed outputs to validate (Operation ID: {})",
            unconfirmed_outputs.len(),
            self.operation_id
        );

        for batch in unconfirmed_outputs.chunks(self.batch_size()) {
            debug!(
                target: LOG_TARGET,
                "Asking base node for location of {} unconfirmed outputs by hash (Operation ID: {})",
                batch.len(),
                self.operation_id
            );

            let (mined, in_mempool, unmined, tip_height) = self
                .query_base_node_for_outputs(batch, wallet_client)
                .await
                .for_protocol(self.operation_id)?;
            debug!(
                target: LOG_TARGET,
                "Base node returned {} mined, {} in mempool, {} unmined outputs (Operation ID: {})",
                mined.len(),
                in_mempool.len(),
                unmined.len(),
                self.operation_id
            );

            let mut mined_updates = Vec::with_capacity(mined.len());
            for mined_info in &mined {
                info!(
                    target: LOG_TARGET,
                    "Updating output comm:{}: hash {} as mined at height {} with current tip at {} (Operation ID: {})",
                    mined_info.output.commitment.to_hex(),
                    mined_info.output.hash.to_hex(),
                    mined_info.mined_at_height,
                    tip_height,
                    self.operation_id
                );
                mined_updates.push(ReceivedOutputInfoForBatch {
                    commitment: mined_info.output.commitment.clone(),
                    mined_height: mined_info.mined_at_height,
                    mined_in_block: mined_info.mined_block_hash,
                    confirmed: tip_height
                        .checked_sub(mined_info.mined_at_height)
                        .is_some_and(|confirmations| confirmations >= self.config.num_confirmations_required),
                    mined_timestamp: mined_info.mined_timestamp,
                });
            }
            if !mined_updates.is_empty() {
                // Only outputs read in a revivable status (and so mask-checked above) may overwrite an Invalid row;
                // any other output that became Invalid after it was read keeps that status.
                self.db
                    .set_received_outputs_mined_height_and_statuses_guarded(mined_updates, revivable_commitments(batch))
                    .for_protocol(self.operation_id)?;
            }

            // Outputs confirmed in mempool remain in their current status
            for uo in &in_mempool {
                debug!(
                    target: LOG_TARGET,
                    "Output comm:{}: hash {} confirmed in mempool, keeping current status (Operation ID: {})",
                    uo.commitment.to_hex(),
                    uo.hash.to_hex(),
                    self.operation_id
                );
            }

            // Outputs not found anywhere are marked invalid
            let unmined_and_invalid: Vec<_> = unmined
                .iter()
                .map(|uo| {
                    info!(
                        target: LOG_TARGET,
                        "Updating output comm:{}: hash {} as unmined and invalid (Operation ID: {})",
                        uo.commitment.to_hex(),
                        uo.hash.to_hex(),
                        self.operation_id
                    );
                    uo.hash
                })
                .collect();
            if !unmined_and_invalid.is_empty() {
                self.db
                    .set_outputs_to_unmined_and_invalid(unmined_and_invalid)
                    .for_protocol(self.operation_id)?;
            }
        }

        Ok(())
    }

    // Returns the height and hash of the highest mined header found still in the chain
    #[allow(clippy::too_many_lines)]
    async fn check_for_reorgs(
        &mut self,
        client: &mut TWalletConnectivity::BaseNodeClient,
    ) -> Result<Option<(u64, BlockHash)>, OutputManagerProtocolError> {
        let mut last_mined_header = None;
        debug!(
            target: LOG_TARGET,
            "Checking last mined TXO to see if the base node has re-orged (Operation ID: {})", self.operation_id
        );

        while let Some(last_spent_output) = self
            .db
            .get_last_spent_output(&self.key_manager)
            .for_protocol(self.operation_id)?
        {
            let mined_height = match last_spent_output.marked_deleted_at_height {
                Some(height) if i64::try_from(height).is_ok() => height,
                _ => {
                    warn!(
                        target: LOG_TARGET,
                        "Spent output {} should have valid `marked_deleted_at_height`, setting as unmined to \
                         revalidate (Operation ID: {})",
                        last_spent_output.commitment.to_hex(),
                        self.operation_id
                    );
                    self.db
                        .set_outputs_to_unmined_and_invalid(vec![last_spent_output.hash])
                        .for_protocol(self.operation_id)?;
                    continue;
                },
            };
            let Some(mined_in_block_hash) = last_spent_output.marked_deleted_in_block else {
                warn!(
                    target: LOG_TARGET,
                    "Spent output {} should have `marked_deleted_in_block`, setting as unmined to revalidate \
                     (Operation ID: {})",
                    last_spent_output.commitment.to_hex(),
                    self.operation_id
                );
                self.db
                    .set_outputs_to_unmined_and_invalid(vec![last_spent_output.hash])
                    .for_protocol(self.operation_id)?;
                continue;
            };
            let block_at_height = self
                .get_base_node_block_at_height(mined_height, client)
                .await
                .for_protocol(self.operation_id)?;

            if block_at_height == Some(mined_in_block_hash) {
                debug!(
                    target: LOG_TARGET,
                    "Last mined transaction is still in the block chain according to base node. (Operation ID: {})",
                    self.operation_id
                );
                break;
            }
            // Chain has reorged since we last scanned
            warn!(
                target: LOG_TARGET,
                "The block that output ({}) was spent in has been reorged out, will try to find this output \
                 again, but these funds have potentially been re-orged out of the chain (Operation ID: {})",
                last_spent_output.commitment.to_hex(),
                self.operation_id
            );
            if blocks_revival(&self.key_manager, &last_spent_output, "TXO reorg check") {
                // Whatever its current status (Spent included), moving it to UnspentMinedUnconfirmed would let
                // the unconfirmed pass make it spendable. Mark it Invalid and clear the stale spent data so the
                // loop moves on to the next spent output.
                self.db
                    .set_outputs_to_unmined_and_invalid(vec![last_spent_output.hash])
                    .for_protocol(self.operation_id)?;
            } else {
                // we mark the output as UnspentMinedUnconfirmed so it wont get picked it by the OMS to be
                // spendable immediately as we first need to find out if this output is unspent, in a mempool,
                // or spent.
                self.db
                    .mark_outputs_as_unspent_guarded(
                        vec![(last_spent_output.hash, false)],
                        revivable_hashes(std::slice::from_ref(&last_spent_output)),
                    )
                    .for_protocol(self.operation_id)?;
            }
        }

        while let Some(last_mined_output) = self
            .db
            .get_last_mined_output(&self.key_manager)
            .for_protocol(self.operation_id)?
        {
            let (Some(mined_height), Some(mined_in_block_hash)) =
                (last_mined_output.mined_height, last_mined_output.mined_in_block)
            else {
                warn!(
                    target: LOG_TARGET,
                    "Output ({}) marked as mined, but mined_height or mined_in_block was empty, invalidating so we \
                     can try to find this output again (Operation ID: {})",
                    last_mined_output.commitment.to_hex(),
                    self.operation_id
                );
                self.db
                    .set_outputs_to_unmined_and_invalid(vec![last_mined_output.hash])
                    .for_protocol(self.operation_id)?;
                continue;
            };
            if i64::try_from(mined_height).is_err() {
                warn!(
                    target: LOG_TARGET,
                    "Output ({}) has out-of-range mined_height {}, invalidating to revalidate (Operation ID: {})",
                    last_mined_output.commitment.to_hex(),
                    mined_height,
                    self.operation_id
                );
                self.db
                    .set_outputs_to_unmined_and_invalid(vec![last_mined_output.hash])
                    .for_protocol(self.operation_id)?;
                continue;
            }
            let block_at_height = self
                .get_base_node_block_at_height(mined_height, client)
                .await
                .for_protocol(self.operation_id)?;
            if block_at_height == Some(mined_in_block_hash) {
                debug!(
                    target: LOG_TARGET,
                    "Last mined transaction is still in the block chain according to base node (Operation ID: {}).",
                    self.operation_id
                );
                last_mined_header = Some((mined_height, mined_in_block_hash));
                break;
            }
            // Chain has reorged since we last scanned
            warn!(
                target: LOG_TARGET,
                "The block that output ({}) was in has been reorged out, will try to find this output again, but \
                 these funds have potentially been re-orged out of the chain (Operation ID: {})",
                last_mined_output.commitment.to_hex(),
                self.operation_id
            );
            self.db
                .set_outputs_to_unmined_and_invalid(vec![last_mined_output.hash])
                .for_protocol(self.operation_id)?;
        }
        Ok(last_mined_header)
    }

    async fn get_base_node_block_at_height(
        &mut self,
        height: u64,
        client: &TWalletConnectivity::BaseNodeClient,
    ) -> Result<Option<BlockHash>, OutputManagerError> {
        let result = match client.get_header_by_height(height).await {
            Ok(r) => r,
            Err(rpc_error) => {
                warn!(
                    target: LOG_TARGET,
                    "Error asking base node for header:{} (Operation ID: {})", rpc_error, self.operation_id
                );
                return Err(OutputManagerError::BaseNodeClientError(format!(
                    "Error asking base node for header: {rpc_error}"
                )));
            },
        };

        Ok(result.map(|b| b.hash))
    }

    /// Query the base node for output status. Uses v2 endpoint which also checks the mempool.
    /// Returns (mined, in_mempool, unmined, tip_height).
    async fn query_base_node_for_outputs(
        &self,
        batch: &[DbWalletOutput],
        base_node_client: &mut TWalletConnectivity::BaseNodeClient,
    ) -> Result<(Vec<MinedOutputInfo>, Vec<DbWalletOutput>, Vec<DbWalletOutput>, u64), OutputManagerError> {
        let (batch_hashes, expected_hashes) = dedup_batch_hashes(batch);
        trace!(
            target: LOG_TARGET,
            "UTXO hashes queried from base node: {:?}",
            batch.iter().map(|o| o.hash.to_hex()).collect::<Vec<String>>()
        );

        let batch_response = base_node_client
            .get_utxos_mined_info(batch_hashes, 2)
            .await
            .map_err(|e| OutputManagerError::BaseNodeClientError(e.to_string()))?;
        let (returned_outputs, mempool_set) = index_mined_utxo_response(&expected_hashes, &batch_response)?;
        let best_block_height = batch_response.best_block_height;

        let mut mined = vec![];
        let mut in_mempool = vec![];
        let mut unmined = vec![];

        for output in batch {
            if let Some(&(mined_at_height, mined_block_hash, mined_timestamp)) = returned_outputs.get(&output.hash) {
                mined.push(MinedOutputInfo {
                    output: output.clone(),
                    mined_at_height,
                    mined_block_hash,
                    mined_timestamp,
                });
            } else if mempool_set.contains(&output.hash) {
                in_mempool.push(output.clone());
            } else {
                unmined.push(output.clone());
            }
        }

        // If the base node returned no mempool info (v1 fallback), use per-output
        // kernel signature queries for encumbered outputs that appear unmined.
        if mempool_set.is_empty() && unmined.iter().any(|o| o.is_encumbered()) {
            let mut still_unmined = Vec::new();
            for uo in unmined {
                if uo.is_encumbered() {
                    let tx_id = uo.received_in_tx_id.or(uo.spent_in_tx_id);
                    let found = if let Some(tx_id) = tx_id {
                        self.check_tx_in_mempool(tx_id, base_node_client).await
                    } else {
                        false
                    };
                    if found {
                        in_mempool.push(uo);
                    } else {
                        still_unmined.push(uo);
                    }
                } else {
                    still_unmined.push(uo);
                }
            }
            unmined = still_unmined;
        }

        Ok((mined, in_mempool, unmined, best_block_height))
    }

    /// Fallback: Check if the parent transaction of an encumbered output is still in the mempool
    /// by querying the kernel signature. Used when base node doesn't support v2.
    async fn check_tx_in_mempool(
        &self,
        tx_id: TxId,
        base_node_client: &mut TWalletConnectivity::BaseNodeClient,
    ) -> bool {
        let sig = match self.db.fetch_kernel_signature_for_tx(tx_id) {
            Ok(Some(sig)) => sig,
            Ok(None) => {
                debug!(
                    target: LOG_TARGET,
                    "No kernel signature found for tx_id {} (Operation ID: {})", tx_id, self.operation_id
                );
                return false;
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Error fetching kernel signature for tx_id {}: {} (Operation ID: {})",
                    tx_id, e, self.operation_id
                );
                return false;
            },
        };

        match base_node_client
            .transaction_query(
                sig.get_compressed_public_nonce().as_bytes().to_vec(),
                sig.get_signature().as_bytes().to_vec(),
            )
            .await
        {
            Ok(response) => {
                matches!(response.location, TxLocation::InMempool | TxLocation::Mined)
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Error querying mempool for tx_id {}: {} (Operation ID: {})",
                    tx_id, e, self.operation_id
                );
                // On error, don't invalidate - be conservative
                true
            },
        }
    }

    fn publish_event(&self, event: OutputManagerEvent) {
        if let Err(e) = self.event_publisher.send(Arc::new(event)) {
            debug!(
                target: LOG_TARGET,
                "Error sending event because there are no subscribers: {e:?}"
            );
        }
    }
}

/// Ensures block heights fit in non-negative SQLite `BIGINT` (`i64`) so `u64 as i64` storage casts cannot wrap
/// negative and sort below height `0` in `check_for_reorgs`.
fn validate_chain_height(height: u64) -> Result<(), OutputManagerError> {
    i64::try_from(height)
        .map(|_| ())
        .map_err(|_| OutputManagerError::InconsistentBaseNodeDataError("Base node sent out-of-range block height"))
}

fn dedup_batch_hashes(batch: &[DbWalletOutput]) -> (Vec<Vec<u8>>, HashSet<FixedHash>) {
    let mut expected_hashes = HashSet::with_capacity(batch.len());
    let mut request_hashes = Vec::with_capacity(batch.len());
    for output in batch {
        if expected_hashes.insert(output.hash) {
            request_hashes.push(output.hash.to_vec());
        }
    }
    (request_hashes, expected_hashes)
}

/// Picks the highest canonical header hash across `check_for_reorgs` and `mined_outputs` (which includes outputs newly
/// discovered as mined in `update_unconfirmed_outputs` when `check_for_reorgs` saw no prior mined outputs; a reorg
/// between the two calls simply fails the current run once and recovers on the next run).
fn select_must_include_header(
    last_mined_header: Option<(u64, BlockHash)>,
    mined_outputs: &[DbWalletOutput],
) -> Result<BlockHash, OutputManagerError> {
    let mut anchor = last_mined_header.filter(|(height, _)| i64::try_from(*height).is_ok());
    for output in mined_outputs {
        if let (Some(height), Some(block_hash)) = (output.mined_height, output.mined_in_block) &&
            i64::try_from(height).is_ok() &&
            anchor.is_none_or(|(best_height, _)| height > best_height)
        {
            anchor = Some((height, block_hash));
        }
    }
    anchor
        .map(|(_, block_hash)| block_hash)
        .ok_or(OutputManagerError::InconsistentBaseNodeDataError(
            "Missing canonical anchor header for mined outputs",
        ))
}

fn parse_deleted_utxo_response(
    expected_hashes: &HashSet<FixedHash>,
    response: &GetUtxosDeletedInfoResponse,
) -> Result<HashMap<FixedHash, DeletedOutputResolution>, OutputManagerError> {
    FixedHash::try_from(response.best_block_hash.as_slice())
        .map_err(|_| OutputManagerError::InconsistentBaseNodeDataError("Base node sent malformed best block hash"))?;
    validate_chain_height(response.best_block_height)?;

    if response.utxos.len() != expected_hashes.len() {
        return Err(OutputManagerError::InconsistentBaseNodeDataError(
            "Base node did not send back information for all utxos",
        ));
    }

    let mut resolutions = HashMap::with_capacity(response.utxos.len());
    for data in &response.utxos {
        let utxo_hash = FixedHash::try_from(data.utxo_hash.as_slice())
            .map_err(|_| OutputManagerError::InconsistentBaseNodeDataError("Base node sent malformed utxo hash"))?;
        if !expected_hashes.contains(&utxo_hash) {
            return Err(OutputManagerError::InconsistentBaseNodeDataError(
                "Base node sent unrequested utxo info",
            ));
        }
        let resolution = classify_deleted_utxo_info(data)?;
        if resolutions.insert(utxo_hash, resolution).is_some() {
            return Err(OutputManagerError::InconsistentBaseNodeDataError(
                "Base node sent duplicate utxo info",
            ));
        }
    }
    Ok(resolutions)
}

fn classify_deleted_utxo_info(data: &DeletedUtxoInfo) -> Result<DeletedOutputResolution, OutputManagerError> {
    match (&data.found_in_header, &data.spent_in_header) {
        (None, None) => Ok(DeletedOutputResolution::Unmined),
        (None, Some(_)) => Err(OutputManagerError::InconsistentBaseNodeDataError(
            "Base node reported spent header without mined header",
        )),
        (Some((found_height, found_hash_bytes)), spent_in_header) => {
            validate_chain_height(*found_height)?;
            let found_hash = FixedHash::try_from(found_hash_bytes.as_slice())
                .map_err(|_| OutputManagerError::InconsistentBaseNodeDataError("Base node sent malformed hash"))?;
            match spent_in_header {
                None => Ok(DeletedOutputResolution::MinedUnspent {
                    found_height: *found_height,
                    found_hash,
                }),
                Some((spent_height, spent_hash_bytes)) => {
                    validate_chain_height(*spent_height)?;
                    let spent_hash = FixedHash::try_from(spent_hash_bytes.as_slice()).map_err(|_| {
                        OutputManagerError::InconsistentBaseNodeDataError("Base node sent malformed hash")
                    })?;
                    // Base nodes up to v6.2.0 report the latest output and the latest input independently, so an output
                    // that was mined, spent, and re-mined with the same hash arrives with spent_height < found_height.
                    // Treat that spend as belonging to the earlier copy and classify the latest copy as MinedUnspent.
                    if *spent_height < *found_height {
                        return Ok(DeletedOutputResolution::MinedUnspent {
                            found_height: *found_height,
                            found_hash,
                        });
                    }
                    Ok(DeletedOutputResolution::MinedSpent {
                        spent_height: *spent_height,
                        spent_hash,
                    })
                },
            }
        },
    }
}

type IndexedMinedOutputs = (HashMap<FixedHash, (u64, FixedHash, u64)>, HashSet<FixedHash>);

fn index_mined_utxo_response(
    expected_hashes: &HashSet<FixedHash>,
    batch_response: &GetUtxosMinedInfoResponse,
) -> Result<IndexedMinedOutputs, OutputManagerError> {
    FixedHash::try_from(batch_response.best_block_hash.as_slice())
        .map_err(|_| OutputManagerError::InconsistentBaseNodeDataError("Base node sent malformed best block hash"))?;
    validate_chain_height(batch_response.best_block_height)?;

    let mut returned_outputs = HashMap::with_capacity(batch_response.utxos.len());
    for mined_info in &batch_response.utxos {
        let utxo_hash = FixedHash::try_from(mined_info.utxo_hash.as_slice())
            .map_err(|_| OutputManagerError::InconsistentBaseNodeDataError("Base node sent malformed utxo hash"))?;
        if !expected_hashes.contains(&utxo_hash) {
            return Err(OutputManagerError::InconsistentBaseNodeDataError(
                "Base node sent unrequested utxo info",
            ));
        }
        validate_chain_height(mined_info.mined_in_height)?;
        let mined_block_hash = FixedHash::try_from(mined_info.mined_in_hash.as_slice())
            .map_err(|_| OutputManagerError::InconsistentBaseNodeDataError("Base node sent malformed block hash"))?;
        if returned_outputs
            .insert(
                utxo_hash,
                (
                    mined_info.mined_in_height,
                    mined_block_hash,
                    mined_info.mined_in_timestamp,
                ),
            )
            .is_some()
        {
            return Err(OutputManagerError::InconsistentBaseNodeDataError(
                "Base node sent duplicate utxo info",
            ));
        }
    }

    let mut mempool_set = HashSet::with_capacity(batch_response.mempool_utxos.len());
    for raw_hash in &batch_response.mempool_utxos {
        let utxo_hash = FixedHash::try_from(raw_hash.as_slice()).map_err(|_| {
            OutputManagerError::InconsistentBaseNodeDataError("Base node sent malformed mempool utxo hash")
        })?;
        if !expected_hashes.contains(&utxo_hash) {
            return Err(OutputManagerError::InconsistentBaseNodeDataError(
                "Base node sent unrequested mempool utxo",
            ));
        }
        if returned_outputs.contains_key(&utxo_hash) {
            return Err(OutputManagerError::InconsistentBaseNodeDataError(
                "Base node reported utxo as both mined and in mempool",
            ));
        }
        if !mempool_set.insert(utxo_hash) {
            return Err(OutputManagerError::InconsistentBaseNodeDataError(
                "Base node sent duplicate mempool utxo",
            ));
        }
    }

    Ok((returned_outputs, mempool_set))
}

/// Commitments of the outputs that were read in a revivable status. Callers only pass outputs that have already been
/// through the commitment mask guard, so these are the ones allowed to overwrite a row that is now `Invalid`.
fn revivable_commitments(outputs: &[DbWalletOutput]) -> Vec<CompressedCommitment> {
    outputs
        .iter()
        .filter(|o| is_revivable_status(o.status))
        .map(|o| o.commitment.clone())
        .collect()
}

/// Hashes of the outputs that were read in a revivable status; see [`revivable_commitments`].
fn revivable_hashes(outputs: &[DbWalletOutput]) -> Vec<FixedHash> {
    outputs
        .iter()
        .filter(|o| is_revivable_status(o.status))
        .map(|o| o.hash)
        .collect()
}

#[cfg(test)]
mod test {
    use std::{collections::VecDeque, sync::Mutex};

    use anyhow::anyhow;
    use tari_common_sqlite::sqlite_connection_pool::PooledDbConnection;
    use tari_script::script;
    use tari_shutdown::ShutdownSignal;
    use tari_transaction_components::{
        MicroMinotari,
        rpc::models::{
            self,
            BlockHeader,
            FeePerGramStat,
            GenerateBurnOutputProofResponse,
            MinedUtxoInfo,
            SyncUtxosByBlockResponseV0,
            TxSubmissionResponse,
        },
        test_helpers::{TestParams, create_wallet_output_with_data},
        transaction_components::{OutputFeatures, Transaction, TransactionOutput},
    };
    use tari_transaction_key_manager::legacy_key_manager::{MemoryKeyManager, create_new_random_key_manager};
    use tokio::sync::{broadcast, mpsc, watch};

    use super::*;
    use crate::{
        connectivity_service::OnlineStatus,
        output_manager_service::storage::{
            OutputSource,
            sqlite_db::{NewOutputSql, OutputManagerSqliteDatabase},
        },
        schema::outputs,
        storage::sqlite_utilities::{WalletDbConnection, run_migration_and_create_sqlite_memory_connection},
        utxo_scanner_service::service::ScannedBlock,
    };

    #[derive(Default)]
    struct MockNodeState {
        headers_by_height: HashMap<u64, BlockHash>,
        known_header_hashes: HashSet<BlockHash>,
        mined_response_queue: VecDeque<GetUtxosMinedInfoResponse>,
        deleted_response_queue: VecDeque<GetUtxosDeletedInfoResponse>,
        mined_response_override: Option<GetUtxosMinedInfoResponse>,
        deleted_response_override: Option<GetUtxosDeletedInfoResponse>,
        mined_query_calls: usize,
        deleted_query_calls: usize,
        mined_utxos: HashMap<FixedHash, MinedUtxoInfo>,
        spent_utxos: HashMap<FixedHash, (u64, Vec<u8>)>,
        best_block_height: u64,
        best_block_hash: Vec<u8>,
        observed_must_include_headers: Vec<Vec<u8>>,
        enforce_must_include_header: bool,
    }

    #[derive(Clone, Default)]
    struct StrictMockClient {
        state: Arc<Mutex<MockNodeState>>,
    }

    impl StrictMockClient {
        fn new() -> Self {
            let mut state = MockNodeState {
                best_block_height: 20,
                best_block_hash: vec![0xAA; 32],
                enforce_must_include_header: true,
                ..Default::default()
            };
            state.known_header_hashes.insert(FixedHash::from([0xAA; 32]));
            Self {
                state: Arc::new(Mutex::new(state)),
            }
        }

        fn insert_canonical_block(&self, height: u64, hash: BlockHash) {
            let mut guard = self.state.lock().unwrap();
            guard.headers_by_height.insert(height, hash);
            guard.known_header_hashes.insert(hash);
        }

        fn set_tip(&self, height: u64, hash: BlockHash) {
            let mut guard = self.state.lock().unwrap();
            guard.best_block_height = height;
            guard.best_block_hash = hash.to_vec();
            guard.headers_by_height.insert(height, hash);
            guard.known_header_hashes.insert(hash);
        }

        fn set_mined_output(&self, hash: FixedHash, height: u64, block_hash: BlockHash, timestamp: u64) {
            let mut guard = self.state.lock().unwrap();
            guard.mined_utxos.insert(hash, MinedUtxoInfo {
                utxo_hash: hash.to_vec(),
                mined_in_hash: block_hash.to_vec(),
                mined_in_height: height,
                mined_in_timestamp: timestamp,
            });
        }

        fn take_must_include_headers(&self) -> Vec<Vec<u8>> {
            std::mem::take(&mut self.state.lock().unwrap().observed_must_include_headers)
        }
    }

    #[async_trait::async_trait]
    impl BaseNodeWalletClient for StrictMockClient {
        async fn get_address(&self) -> String {
            "http://127.0.0.1:18142".to_string()
        }

        async fn is_online(&self) -> bool {
            true
        }

        async fn get_tip_info(&self) -> Result<models::TipInfoResponse, anyhow::Error> {
            Err(anyhow!("unused in test"))
        }

        async fn get_header_by_height(&self, height: u64) -> Result<Option<BlockHeader>, anyhow::Error> {
            let guard = self.state.lock().unwrap();
            Ok(guard.headers_by_height.get(&height).copied().map(|hash| {
                let mut node_header = tari_node_components::blocks::BlockHeader::new(1);
                node_header.height = height;
                let mut rpc_header = BlockHeader::from(node_header);
                rpc_header.hash = hash;
                rpc_header
            }))
        }

        async fn get_height_at_time(&self, _epoch_time: u64) -> Result<u64, anyhow::Error> {
            Err(anyhow!("unused in test"))
        }

        async fn get_utxos_by_block(
            &self,
            _header_hash: Vec<u8>,
        ) -> Result<models::GetUtxosByBlockResponse, anyhow::Error> {
            Err(anyhow!("unused in test"))
        }

        async fn sync_utxos_by_block(
            &self,
            _start_header_hash: Vec<u8>,
            _shutdown: ShutdownSignal,
        ) -> Result<mpsc::Receiver<Result<SyncUtxosByBlockResponseV0, anyhow::Error>>, anyhow::Error> {
            Err(anyhow!("unused in test"))
        }

        async fn get_last_request_latency(&self) -> Option<std::time::Duration> {
            None
        }

        async fn get_utxos_mined_info(
            &self,
            hashes: Vec<Vec<u8>>,
            _version: u32,
        ) -> Result<GetUtxosMinedInfoResponse, anyhow::Error> {
            let mut guard = self.state.lock().unwrap();
            guard.mined_query_calls = guard.mined_query_calls.saturating_add(1);
            if let Some(queued) = guard.mined_response_queue.pop_front() {
                return Ok(queued);
            }
            if let Some(override_resp) = &guard.mined_response_override {
                return Ok(override_resp.clone());
            }
            let utxos = hashes
                .iter()
                .filter_map(|raw| FixedHash::try_from(raw.as_slice()).ok())
                .filter_map(|hash| guard.mined_utxos.get(&hash).cloned())
                .collect();
            Ok(GetUtxosMinedInfoResponse {
                utxos,
                best_block_hash: guard.best_block_hash.clone(),
                best_block_height: guard.best_block_height,
                mempool_utxos: Vec::new(),
            })
        }

        async fn fetch_utxo(&self, _hash: Vec<u8>) -> Result<Option<TransactionOutput>, anyhow::Error> {
            Ok(None)
        }

        async fn query_deleted_utxos(
            &self,
            hashes: Vec<Vec<u8>>,
            must_include_header: Vec<u8>,
        ) -> Result<GetUtxosDeletedInfoResponse, anyhow::Error> {
            let mut guard = self.state.lock().unwrap();
            guard.deleted_query_calls = guard.deleted_query_calls.saturating_add(1);
            guard.observed_must_include_headers.push(must_include_header.clone());
            if guard.enforce_must_include_header {
                let header_hash = FixedHash::try_from(must_include_header.as_slice())
                    .map_err(|e| anyhow!("Invalid must_include_header size: {e}"))?;
                if !guard.known_header_hashes.contains(&header_hash) {
                    return Err(anyhow!("HeaderHashNotFound"));
                }
            }
            if let Some(queued) = guard.deleted_response_queue.pop_front() {
                return Ok(queued);
            }
            if let Some(override_resp) = &guard.deleted_response_override {
                return Ok(override_resp.clone());
            }
            let utxos = hashes
                .into_iter()
                .map(|raw| {
                    let parsed = FixedHash::try_from(raw.as_slice()).ok();
                    let found_in_header = parsed
                        .and_then(|h| guard.mined_utxos.get(&h))
                        .map(|m| (m.mined_in_height, m.mined_in_hash.clone()));
                    let spent_in_header = parsed.and_then(|h| guard.spent_utxos.get(&h).cloned());
                    DeletedUtxoInfo {
                        utxo_hash: raw,
                        found_in_header,
                        spent_in_header,
                    }
                })
                .collect();
            Ok(GetUtxosDeletedInfoResponse {
                utxos,
                best_block_hash: guard.best_block_hash.clone(),
                best_block_height: guard.best_block_height,
            })
        }

        async fn submit_transaction(&self, _transaction: Transaction) -> Result<TxSubmissionResponse, anyhow::Error> {
            Err(anyhow!("unused in test"))
        }

        async fn transaction_query(
            &self,
            _excess_sig_nonce: Vec<u8>,
            _excess_sig_sig: Vec<u8>,
        ) -> Result<models::TxQueryResponse, anyhow::Error> {
            Err(anyhow!("unused in test"))
        }

        async fn get_mempool_fee_per_gram_stats(&self, _count: u64) -> Result<FeePerGramStat, anyhow::Error> {
            Err(anyhow!("unused in test"))
        }

        async fn get_burn_output_proof(
            &self,
            _commitment: &CompressedCommitment,
        ) -> Result<GenerateBurnOutputProofResponse, anyhow::Error> {
            Err(anyhow!("unused in test"))
        }
    }

    #[derive(Clone)]
    struct MockConnectivity {
        client: StrictMockClient,
        status_rx: watch::Receiver<OnlineStatus>,
    }

    impl MockConnectivity {
        fn new(client: StrictMockClient) -> Self {
            let (_tx, status_rx) = watch::channel(OnlineStatus::Offline);
            Self { client, status_rx }
        }
    }

    #[async_trait::async_trait]
    impl WalletConnectivityInterface for MockConnectivity {
        type BaseNodeClient = StrictMockClient;

        async fn obtain_base_node_wallet_rpc_client(&self) -> Self::BaseNodeClient {
            self.client.clone()
        }

        async fn get_connectivity_status(&self) -> OnlineStatus {
            OnlineStatus::Offline
        }

        fn get_connectivity_status_watch(&self) -> watch::Receiver<OnlineStatus> {
            self.status_rx.clone()
        }

        async fn get_last_request_latency(&self) -> Option<std::time::Duration> {
            None
        }

        async fn get_address(&self) -> String {
            "http://127.0.0.1:18142".to_string()
        }
    }

    struct TestHarness {
        connection: WalletDbConnection,
        db: OutputManagerDatabase<OutputManagerSqliteDatabase>,
        client: StrictMockClient,
        connectivity: MockConnectivity,
        key_manager: MemoryKeyManager,
        config: OutputManagerServiceConfig,
    }

    impl TestHarness {
        async fn new() -> Self {
            let connection = run_migration_and_create_sqlite_memory_connection().unwrap();
            let backend = OutputManagerSqliteDatabase::new(connection.clone());
            let db = OutputManagerDatabase::new(backend);
            let client = StrictMockClient::new();
            let connectivity = MockConnectivity::new(client.clone());
            let key_manager = create_new_random_key_manager().await.unwrap();
            Self {
                connection,
                db,
                client,
                connectivity,
                key_manager,
                config: OutputManagerServiceConfig::default(),
            }
        }

        fn make_task(
            &self,
            operation_id: u64,
        ) -> TxoValidationTask<OutputManagerSqliteDatabase, MockConnectivity, MemoryKeyManager> {
            let (event_tx, _event_rx) = broadcast::channel(16);
            TxoValidationTask::new(
                operation_id,
                self.db.clone(),
                self.connectivity.clone(),
                event_tx,
                self.config.clone(),
                self.key_manager.clone(),
            )
        }

        fn insert_output(
            &self,
            value: u64,
            status: OutputStatus,
            mined_in: Option<(u64, BlockHash)>,
            spent_in: Option<(u64, BlockHash)>,
        ) -> DbWalletOutput {
            let test_params = TestParams::new(&self.key_manager);
            let wallet_output = create_wallet_output_with_data(
                script!(Nop).unwrap(),
                OutputFeatures::default(),
                &test_params,
                MicroMinotari::from(value),
                &self.key_manager,
            )
            .unwrap();
            let mut db_output =
                DbWalletOutput::from_wallet_output(wallet_output, None, OutputSource::Standard, None, None);
            db_output.status = status;
            if let Some((height, block_hash)) = mined_in {
                db_output.mined_height = Some(height);
                db_output.mined_in_block = Some(block_hash);
            }
            if let Some((height, block_hash)) = spent_in {
                db_output.marked_deleted_at_height = Some(height);
                db_output.marked_deleted_in_block = Some(block_hash);
            }
            let row = NewOutputSql::new(db_output.clone(), Some(status), None).unwrap();
            let mut conn = self.connection.get_pooled_connection().unwrap();
            row.commit(&mut conn).unwrap();
            if let Some((min_h, min_b)) = mined_in {
                use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
                diesel::update(outputs::table.filter(outputs::commitment.eq(db_output.commitment.to_vec())))
                    .set((
                        outputs::mined_height.eq(Some(i64::try_from(min_h).unwrap())),
                        outputs::mined_in_block.eq(Some(min_b.to_vec())),
                    ))
                    .execute(&mut conn)
                    .unwrap();
            }
            if let Some((del_h, del_b)) = spent_in {
                use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
                diesel::update(outputs::table.filter(outputs::commitment.eq(db_output.commitment.to_vec())))
                    .set((
                        outputs::marked_deleted_at_height.eq(Some(i64::try_from(del_h).unwrap())),
                        outputs::marked_deleted_in_block.eq(Some(del_b.to_vec())),
                    ))
                    .execute(&mut conn)
                    .unwrap();
            }
            db_output
        }

        fn stored_output(&self, commitment: &CompressedCommitment) -> DbWalletOutput {
            self.db
                .fetch_by_commitment(commitment.clone(), &self.key_manager)
                .unwrap()
        }

        fn set_last_scanned_height(&self, height: u64, header_hash: BlockHash) {
            let backend = OutputManagerSqliteDatabase::new(self.connection.clone());
            backend
                .save_last_scanned_height(ScannedBlock {
                    header_hash,
                    height,
                    timestamp: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap().naive_utc(),
                })
                .unwrap();
        }
    }

    #[tokio::test]
    async fn newly_mined_outputs_supply_canonical_must_include_header_to_spent_query() {
        let harness = TestHarness::new().await;
        let block5 = FixedHash::from([0x05; 32]);
        let block10 = FixedHash::from([0x10; 32]);
        harness.client.insert_canonical_block(5, block5);
        harness.client.set_tip(10, block10);

        // Wallet starts with no previously mined outputs, so `check_for_reorgs` returns `None`.
        let incoming = harness.insert_output(5_000, OutputStatus::EncumberedToBeReceived, None, None);
        harness.client.set_mined_output(incoming.hash, 5, block5, 1_700_000_500);

        harness.make_task(1).execute().await.unwrap();

        // `update_spent_outputs` must send the newly discovered 32-byte `block5` rather than an empty slice.
        assert_eq!(harness.client.take_must_include_headers(), vec![block5.to_vec()]);
        let updated = harness.stored_output(&incoming.commitment);
        assert_eq!(updated.status, OutputStatus::Unspent);
        assert_eq!(updated.mined_height, Some(5));
        assert_eq!(updated.mined_in_block, Some(block5));
    }

    #[tokio::test]
    async fn update_spent_outputs_matches_by_hash_and_rejects_mismatched_or_duplicate_entries() {
        let harness = TestHarness::new().await;
        let block5 = FixedHash::from([0x05; 32]);
        let block6 = FixedHash::from([0x06; 32]);
        let block12 = FixedHash::from([0x12; 32]);
        harness.client.insert_canonical_block(5, block5);
        harness.client.insert_canonical_block(6, block6);
        harness.client.set_tip(12, block12);
        harness.set_last_scanned_height(12, block12);

        let out_a = harness.insert_output(1_000, OutputStatus::Unspent, Some((5, block5)), None);
        let out_b = harness.insert_output(2_000, OutputStatus::Unspent, Some((5, block5)), None);

        // 1. Mismatched / unrequested hash in `query_deleted_utxos` must fail closed without mutating DB.
        harness.client.state.lock().unwrap().deleted_response_override = Some(GetUtxosDeletedInfoResponse {
            utxos: vec![
                DeletedUtxoInfo {
                    utxo_hash: out_a.hash.to_vec(),
                    found_in_header: Some((5, block5.to_vec())),
                    spent_in_header: Some((6, block6.to_vec())),
                },
                DeletedUtxoInfo {
                    utxo_hash: vec![0xFF; 32],
                    found_in_header: Some((5, block5.to_vec())),
                    spent_in_header: None,
                },
            ],
            best_block_hash: block12.to_vec(),
            best_block_height: 12,
        });
        let err = harness.make_task(2).execute().await.unwrap_err();
        assert!(matches!(
            err.error,
            OutputManagerError::InconsistentBaseNodeDataError(_)
        ));
        assert_eq!(harness.stored_output(&out_a.commitment).status, OutputStatus::Unspent);
        assert_eq!(harness.stored_output(&out_b.commitment).status, OutputStatus::Unspent);

        // 2. Duplicate hash in `query_deleted_utxos` must also fail closed without mutating DB.
        harness.client.state.lock().unwrap().deleted_response_override = Some(GetUtxosDeletedInfoResponse {
            utxos: vec![
                DeletedUtxoInfo {
                    utxo_hash: out_a.hash.to_vec(),
                    found_in_header: Some((5, block5.to_vec())),
                    spent_in_header: None,
                },
                DeletedUtxoInfo {
                    utxo_hash: out_a.hash.to_vec(),
                    found_in_header: Some((5, block5.to_vec())),
                    spent_in_header: Some((6, block6.to_vec())),
                },
            ],
            best_block_hash: block12.to_vec(),
            best_block_height: 12,
        });
        let err = harness.make_task(3).execute().await.unwrap_err();
        assert!(matches!(
            err.error,
            OutputManagerError::InconsistentBaseNodeDataError(_)
        ));
        assert_eq!(harness.stored_output(&out_a.commitment).status, OutputStatus::Unspent);
        assert_eq!(harness.stored_output(&out_b.commitment).status, OutputStatus::Unspent);

        // 3. Reordered valid response (`out_b` first, `out_a` second) marks `out_b` spent while keeping `out_a`
        //    unspent.
        harness.client.state.lock().unwrap().deleted_response_override = Some(GetUtxosDeletedInfoResponse {
            utxos: vec![
                DeletedUtxoInfo {
                    utxo_hash: out_b.hash.to_vec(),
                    found_in_header: Some((5, block5.to_vec())),
                    spent_in_header: Some((6, block6.to_vec())),
                },
                DeletedUtxoInfo {
                    utxo_hash: out_a.hash.to_vec(),
                    found_in_header: Some((5, block5.to_vec())),
                    spent_in_header: None,
                },
            ],
            best_block_hash: block12.to_vec(),
            best_block_height: 12,
        });
        harness.make_task(4).execute().await.unwrap();
        assert_eq!(harness.stored_output(&out_a.commitment).status, OutputStatus::Unspent);
        assert_eq!(harness.stored_output(&out_b.commitment).status, OutputStatus::Spent);
    }

    #[tokio::test]
    async fn rejects_invalid_mined_info_heights_wraparound_and_mempool_overlap() {
        let mut harness = TestHarness::new().await;
        harness.config.num_confirmations_required = 0;
        let block5 = FixedHash::from([0x05; 32]);
        let block10 = FixedHash::from([0x10; 32]);
        harness.client.insert_canonical_block(5, block5);
        harness.client.set_tip(10, block10);

        let pending = harness.insert_output(1_500, OutputStatus::EncumberedToBeReceived, None, None);

        // 1. `best_block_height > i64::MAX as u64` is rejected before wrapping negative in SQLite.
        harness.client.state.lock().unwrap().mined_response_override = Some(GetUtxosMinedInfoResponse {
            utxos: vec![MinedUtxoInfo {
                utxo_hash: pending.hash.to_vec(),
                mined_in_hash: block10.to_vec(),
                mined_in_height: u64::MAX,
                mined_in_timestamp: 0,
            }],
            best_block_hash: block10.to_vec(),
            best_block_height: u64::MAX,
            mempool_utxos: Vec::new(),
        });
        let err = harness.make_task(11).execute().await.unwrap_err();
        assert!(matches!(
            err.error,
            OutputManagerError::InconsistentBaseNodeDataError(_)
        ));

        // 2. Output simultaneously reported in `utxos` and `mempool_utxos` is rejected.
        harness.client.state.lock().unwrap().mined_response_override = Some(GetUtxosMinedInfoResponse {
            utxos: vec![MinedUtxoInfo {
                utxo_hash: pending.hash.to_vec(),
                mined_in_hash: block5.to_vec(),
                mined_in_height: 5,
                mined_in_timestamp: 0,
            }],
            best_block_hash: block10.to_vec(),
            best_block_height: 10,
            mempool_utxos: vec![pending.hash.to_vec()],
        });
        let err = harness.make_task(12).execute().await.unwrap_err();
        assert!(matches!(
            err.error,
            OutputManagerError::InconsistentBaseNodeDataError(_)
        ));

        // 3. An output mined at height 11 when best_block_height is 10 (honest block-arrival race) is accepted and
        //    treated as unconfirmed by checked_sub rather than failing the validation run.
        harness.client.state.lock().unwrap().mined_response_override = Some(GetUtxosMinedInfoResponse {
            utxos: vec![MinedUtxoInfo {
                utxo_hash: pending.hash.to_vec(),
                mined_in_hash: block10.to_vec(),
                mined_in_height: 11,
                mined_in_timestamp: 0,
            }],
            best_block_hash: block10.to_vec(),
            best_block_height: 10,
            mempool_utxos: Vec::new(),
        });
        harness.client.state.lock().unwrap().deleted_response_override = Some(GetUtxosDeletedInfoResponse {
            utxos: vec![DeletedUtxoInfo {
                utxo_hash: pending.hash.to_vec(),
                found_in_header: Some((11, block10.to_vec())),
                spent_in_header: None,
            }],
            best_block_hash: block10.to_vec(),
            best_block_height: 10,
        });
        harness.make_task(13).execute().await.unwrap();
        assert_eq!(
            harness.stored_output(&pending.commitment).status,
            OutputStatus::UnspentMinedUnconfirmed
        );
    }

    #[tokio::test]
    async fn rejects_invalid_deleted_info_headers_while_allowing_same_block_and_remined_spend_shapes() {
        let harness = TestHarness::new().await;
        let block4 = FixedHash::from([0x04; 32]);
        let block5 = FixedHash::from([0x05; 32]);
        let block6 = FixedHash::from([0x06; 32]);
        let block10 = FixedHash::from([0x10; 32]);
        harness.client.insert_canonical_block(4, block4);
        harness.client.insert_canonical_block(5, block5);
        harness.client.insert_canonical_block(6, block6);
        harness.client.set_tip(10, block10);

        let first = harness.insert_output(1_500, OutputStatus::Unspent, Some((5, block5)), None);
        let mined = harness.insert_output(2_500, OutputStatus::Unspent, Some((5, block5)), None);
        harness.client.set_mined_output(first.hash, 5, block5, 100);
        harness.client.set_mined_output(mined.hash, 5, block5, 100);

        // spent_in_header without found_in_header is rejected
        harness.client.state.lock().unwrap().deleted_response_override = Some(GetUtxosDeletedInfoResponse {
            utxos: vec![
                DeletedUtxoInfo {
                    utxo_hash: first.hash.to_vec(),
                    found_in_header: Some((5, block5.to_vec())),
                    spent_in_header: None,
                },
                DeletedUtxoInfo {
                    utxo_hash: mined.hash.to_vec(),
                    found_in_header: None,
                    spent_in_header: Some((6, block10.to_vec())),
                },
            ],
            best_block_hash: block10.to_vec(),
            best_block_height: 10,
        });
        let err = harness.make_task(13).execute().await.unwrap_err();
        assert!(matches!(
            err.error,
            OutputManagerError::InconsistentBaseNodeDataError(_)
        ));
        assert_eq!(harness.stored_output(&mined.commitment).status, OutputStatus::Unspent);

        // Old-node reply shape (up to v6.2.0) for an output mined, spent at height 4, and re-mined at height 6:
        // `spent_height (4) < found_height (6)` is treated as MinedUnspent at height 6 and refreshes metadata.
        harness.client.set_mined_output(mined.hash, 6, block6, 200);
        harness.client.state.lock().unwrap().deleted_response_override = Some(GetUtxosDeletedInfoResponse {
            utxos: vec![
                DeletedUtxoInfo {
                    utxo_hash: first.hash.to_vec(),
                    found_in_header: Some((5, block5.to_vec())),
                    spent_in_header: None,
                },
                DeletedUtxoInfo {
                    utxo_hash: mined.hash.to_vec(),
                    found_in_header: Some((6, block6.to_vec())),
                    spent_in_header: Some((4, block4.to_vec())),
                },
            ],
            best_block_hash: block10.to_vec(),
            best_block_height: 10,
        });
        harness.make_task(14).execute().await.unwrap();
        let remined = harness.stored_output(&mined.commitment);
        assert_eq!(remined.status, OutputStatus::Unspent);
        assert_eq!(remined.mined_height, Some(6));
        assert_eq!(remined.mined_in_block, Some(block6));

        // Same-block spend (`spent_height == found_height`) IS valid in Mimblewimble and must succeed.
        harness.set_last_scanned_height(10, block10);
        harness.client.state.lock().unwrap().deleted_response_override = Some(GetUtxosDeletedInfoResponse {
            utxos: vec![
                DeletedUtxoInfo {
                    utxo_hash: first.hash.to_vec(),
                    found_in_header: Some((5, block5.to_vec())),
                    spent_in_header: None,
                },
                DeletedUtxoInfo {
                    utxo_hash: mined.hash.to_vec(),
                    found_in_header: Some((6, block6.to_vec())),
                    spent_in_header: Some((6, block6.to_vec())),
                },
            ],
            best_block_hash: block10.to_vec(),
            best_block_height: 10,
        });
        harness.make_task(15).execute().await.unwrap();
        assert_eq!(harness.stored_output(&mined.commitment).status, OutputStatus::Spent);
    }

    #[tokio::test]
    async fn unspending_marked_deleted_output_respects_required_confirmations() {
        let mut harness = TestHarness::new().await;
        harness.config.num_confirmations_required = 3;
        let block10 = FixedHash::from([0x10; 32]);
        let block11 = FixedHash::from([0x11; 32]);
        let block13 = FixedHash::from([0x13; 32]);
        harness.client.insert_canonical_block(10, block10);
        harness.client.set_tip(11, block11);

        // Output mined at 10, marked deleted at 11 (`SpentMinedUnconfirmed`), but chain reports it unspent at tip 11.
        // With `11 - 10 = 1 < 3` confirmations, unspending must demote to `UnspentMinedUnconfirmed`, not `Unspent`.
        let output = harness.insert_output(
            4_000,
            OutputStatus::SpentMinedUnconfirmed,
            Some((10, block10)),
            Some((11, block11)),
        );
        harness.client.set_mined_output(output.hash, 10, block10, 1_700_001_000);

        harness.make_task(20).execute().await.unwrap();
        let after_tip_11 = harness.stored_output(&output.commitment);
        assert_eq!(after_tip_11.status, OutputStatus::UnspentMinedUnconfirmed);
        assert_eq!(after_tip_11.marked_deleted_at_height, None);

        // Once tip advances to 13 (`13 - 10 = 3 >= 3`), the next validation promotes it to spendable `Unspent`.
        harness.client.set_tip(13, block13);
        harness.make_task(21).execute().await.unwrap();
        assert_eq!(harness.stored_output(&output.commitment).status, OutputStatus::Unspent);
    }

    #[tokio::test]
    async fn shallow_reorg_below_anchor_refreshes_unreserved_metadata_and_leaves_reservations_alone() {
        let harness = TestHarness::new().await;
        let stale_fork5 = FixedHash::from([0xF5; 32]);
        let canonical6 = FixedHash::from([0x06; 32]);
        let canonical20 = FixedHash::from([0x20; 32]);
        harness.client.insert_canonical_block(6, canonical6);
        harness.client.set_tip(20, canonical20);

        let earlier = harness.insert_output(3_000, OutputStatus::Unspent, Some((5, stale_fork5)), None);
        let reserved = harness.insert_output(4_000, OutputStatus::EncumberedToBeSpent, Some((5, stale_fork5)), None);
        let short_term_reserved = harness.insert_output(
            4_500,
            OutputStatus::ShortTermEncumberedToBeSpent,
            Some((5, stale_fork5)),
            None,
        );
        let anchor = harness.insert_output(6_000, OutputStatus::Unspent, Some((20, canonical20)), None);
        // Only `earlier` is reported as mined at height 6; `reserved` is left unmined in the mock to verify
        // `refresh_remined_outputs` leaves reserved candidates untouched even when unmined.
        harness.client.set_mined_output(earlier.hash, 6, canonical6, 1_700_000_600);
        harness
            .client
            .set_mined_output(short_term_reserved.hash, 6, canonical6, 1_700_000_600);
        harness
            .client
            .set_mined_output(anchor.hash, 20, canonical20, 1_700_002_000);

        harness.make_task(30).execute().await.unwrap();

        assert_eq!(harness.client.take_must_include_headers(), vec![canonical20.to_vec()]);
        let refreshed = harness.stored_output(&earlier.commitment);
        assert_eq!(refreshed.status, OutputStatus::Unspent);
        assert_eq!(refreshed.mined_height, Some(6));
        assert_eq!(refreshed.mined_in_block, Some(canonical6));

        // Both reserved outputs keep their reservation status and are not refreshed or invalidated
        let untouched_reserved = harness.stored_output(&reserved.commitment);
        assert_eq!(untouched_reserved.status, OutputStatus::EncumberedToBeSpent);
        assert_eq!(untouched_reserved.mined_height, Some(5));
        assert_eq!(untouched_reserved.mined_in_block, Some(stale_fork5));

        let untouched_short_term = harness.stored_output(&short_term_reserved.commitment);
        assert_eq!(
            untouched_short_term.status,
            OutputStatus::ShortTermEncumberedToBeSpent
        );
        assert_eq!(untouched_short_term.mined_height, Some(5));
        assert_eq!(untouched_short_term.mined_in_block, Some(stale_fork5));
    }
}
