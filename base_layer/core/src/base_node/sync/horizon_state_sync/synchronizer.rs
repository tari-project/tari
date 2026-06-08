//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    cmp,
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use log::*;
use tari_common_types::types::{CompressedCommitment, FixedHash, RangeProofService};
use tari_comms::{
    PeerConnection,
    RefKind,
    connectivity::ConnectivityRequester,
    peer_manager::NodeId,
    protocol::rpc::{RpcClient, RpcError, RpcStatus},
};
use tari_crypto::commitment::HomomorphicCommitment;
use tari_node_components::blocks::{BlockHeader, ChainHeader};
use tari_transaction_components::{
    BanPeriod,
    transaction_components::{TransactionKernel, TransactionOutput, transaction_output::batch_verify_range_proofs},
    validation::{aggregate_body::validate_individual_output, helpers::validate_output_version},
};
use tari_utilities::{ByteArray, hex::Hex};
use tokio::task;

use super::error::HorizonSyncError;
use crate::{
    PrunedKernelMmr,
    base_node::sync::{
        BlockchainSyncConfig,
        SyncPeer,
        ban::PeerBanManager,
        hooks::Hooks,
        horizon_state_sync::{HorizonSyncInfo, HorizonSyncStatus},
        rpc,
        rpc::BaseNodeSyncRpcClient,
    },
    blocks::UpdateBlockAccumulatedData,
    chain_storage::{
        BlockchainBackend,
        ChainStorageError,
        HorizonStateTreeUpdate,
        HorizonSyncOutputCheckpoint,
        MmrTree,
        async_db::AsyncBlockchainDb,
    },
    common::rolling_avg::RollingAverageTime,
    consensus::BaseNodeConsensusManager,
    proto::base_node::{SyncKernelsRequest, SyncUtxosRequest, SyncUtxosResponse, sync_utxos_response::Txo},
    validation::FinalHorizonStateValidation,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::horizon_state_sync";

const MAX_LATENCY_INCREASES: usize = 5;
const HORIZON_SYNC_BATCH_SIZE: usize = 10_000;
const PROGRESS_REPORT_INTERVAL: u64 = 100;
const HORIZON_SYNC_TRANCHE_SIZE: u64 = 5_000;

pub struct HorizonStateSynchronization<'a, B> {
    config: BlockchainSyncConfig,
    db: AsyncBlockchainDb<B>,
    rules: BaseNodeConsensusManager,
    sync_peers: &'a mut Vec<SyncPeer>,
    horizon_sync_height: u64,
    prover: Arc<RangeProofService>,
    num_kernels: u64,
    num_outputs: u64,
    hooks: Hooks,
    connectivity: ConnectivityRequester,
    final_state_validator: Arc<dyn FinalHorizonStateValidation<B>>,
    max_latency: Duration,
    peer_ban_manager: PeerBanManager,
}

impl<'a, B: BlockchainBackend + 'static> HorizonStateSynchronization<'a, B> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: BlockchainSyncConfig,
        db: AsyncBlockchainDb<B>,
        connectivity: ConnectivityRequester,
        rules: BaseNodeConsensusManager,
        sync_peers: &'a mut Vec<SyncPeer>,
        horizon_sync_height: u64,
        prover: Arc<RangeProofService>,
        final_state_validator: Arc<dyn FinalHorizonStateValidation<B>>,
    ) -> Self {
        let peer_ban_manager = PeerBanManager::new(config.clone(), connectivity.clone());
        Self {
            max_latency: config.initial_max_sync_latency,
            config,
            db,
            rules,
            connectivity,
            sync_peers,
            horizon_sync_height,
            prover,
            num_kernels: 0,
            num_outputs: 0,
            hooks: Hooks::default(),
            final_state_validator,
            peer_ban_manager,
        }
    }

    pub fn on_starting<H>(&mut self, hook: H)
    where for<'r> H: FnOnce(&SyncPeer) + Send + Sync + 'static {
        self.hooks.add_on_starting_hook(hook);
    }

    pub fn on_progress<H>(&mut self, hook: H)
    where H: Fn(HorizonSyncInfo) + Send + Sync + 'static {
        self.hooks.add_on_progress_horizon_hook(hook);
    }

    pub async fn synchronize(&mut self) -> Result<(), HorizonSyncError> {
        if self.sync_peers.is_empty() {
            return Err(HorizonSyncError::NoSyncPeers);
        }

        debug!(
            target: LOG_TARGET,
            "Preparing database for horizon sync to height #{}", self.horizon_sync_height
        );
        let to_header = self.db().fetch_header(self.horizon_sync_height).await?.ok_or_else(|| {
            ChainStorageError::ValueNotFound {
                entity: "Header",
                field: "height",
                value: self.horizon_sync_height.to_string(),
            }
        })?;

        // Sync peers typically arrive here with a Weak connection attached by block_sync (or
        // header_sync) on its way out. Upgrade in place to Strong for the duration of horizon
        // sync; dial Strong if no connection is attached. On exit (success or failure) we
        // downgrade back to Weak so any subsequent stage / future sync cycle can reuse the
        // handle without forcibly pinning it.
        for peer in self.sync_peers.iter_mut() {
            if let Some(conn) = peer.connection() &&
                !conn.is_connected()
            {
                peer.clear_connection();
            }
            if peer.ensure_strong_connection() {
                continue;
            }
            match self
                .connectivity
                .dial_peer(peer.node_id().clone(), RefKind::Strong)
                .await
            {
                Ok(conn) => peer.set_connection(conn),
                Err(e) => debug!(
                    target: LOG_TARGET,
                    "Failed to dial sync peer {} as strong: {e}", peer.node_id()
                ),
            }
        }

        let result = self.synchronize_inner(&to_header).await;

        for peer in self.sync_peers.iter_mut() {
            peer.downgrade_connection();
        }

        result
    }

    async fn synchronize_inner(&mut self, to_header: &BlockHeader) -> Result<(), HorizonSyncError> {
        let mut latency_increases_counter = 0;
        loop {
            match self.sync(to_header).await {
                Ok(()) => return Ok(()),
                Err(err @ HorizonSyncError::AllSyncPeersExceedLatency) => {
                    // If we don't have many sync peers to select from, return the listening state and see if we can get
                    // some more.
                    warn!(
                        target: LOG_TARGET,
                        "Slow sync peers detected: {}",
                        self.sync_peers
                            .iter()
                            .map(|p| format!("{} ({:.2?})", p.node_id(), p.latency().unwrap_or_default()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    if self.sync_peers.len() < 2 {
                        return Err(err);
                    }
                    self.max_latency += self.config.max_latency_increase;
                    latency_increases_counter += 1;
                    if latency_increases_counter > MAX_LATENCY_INCREASES {
                        return Err(err);
                    }
                },
                Err(err) => return Err(err),
            }
        }
    }

    async fn sync(&mut self, to_header: &BlockHeader) -> Result<(), HorizonSyncError> {
        let sync_peer_node_ids = self.sync_peers.iter().map(|p| p.node_id()).cloned().collect::<Vec<_>>();
        info!(
            target: LOG_TARGET,
            "Attempting to sync horizon state ({} sync peers)",
            sync_peer_node_ids.len()
        );
        let mut latency_counter = 0usize;
        for node_id in sync_peer_node_ids {
            match self.connect_and_attempt_sync(&node_id, to_header).await {
                Ok(_) => return Ok(()),
                // Try another peer
                Err(err) => {
                    let ban_reason = HorizonSyncError::get_ban_reason(&err);

                    if let Some(reason) = ban_reason {
                        let duration = match reason.ban_duration {
                            BanPeriod::Short => self.config.short_ban_period,
                            BanPeriod::Long => self.config.ban_period,
                        };
                        warn!(target: LOG_TARGET, "{err}");
                        self.peer_ban_manager
                            .ban_peer_if_required(&node_id, reason.reason, duration)
                            .await;
                    }
                    if let HorizonSyncError::MaxLatencyExceeded { .. } = err {
                        latency_counter += 1;
                    } else {
                        self.remove_sync_peer(&node_id);
                    }
                },
            }
        }

        if self.sync_peers.is_empty() {
            Err(HorizonSyncError::NoMoreSyncPeers("Header sync failed".to_string()))
        } else if latency_counter >= self.sync_peers.len() {
            Err(HorizonSyncError::AllSyncPeersExceedLatency)
        } else {
            Err(HorizonSyncError::FailedSyncAllPeers)
        }
    }

    async fn connect_and_attempt_sync(
        &mut self,
        node_id: &NodeId,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        // Connect
        let (mut client, sync_peer) = self.connect_sync_peer(node_id).await?;

        // Perform horizon sync
        debug!(target: LOG_TARGET, "Check if pruning is needed");
        self.prune_if_needed().await?;
        self.sync_kernels_and_outputs(sync_peer.clone(), &mut client, to_header)
            .await?;

        // Validate and finalize horizon sync
        self.finalize_horizon_sync(&sync_peer).await?;

        Ok(())
    }

    async fn connect_sync_peer(
        &mut self,
        node_id: &NodeId,
    ) -> Result<(BaseNodeSyncRpcClient, SyncPeer), HorizonSyncError> {
        let peer_index = self
            .get_sync_peer_index(node_id)
            .ok_or(HorizonSyncError::PeerNotFound)?;
        let sync_peer = self.sync_peers.get(peer_index).expect("Already checked");
        self.hooks.call_on_starting_hook(sync_peer);

        // Prefer the connection that travelled with this SyncPeer from header_sync (Strong).
        // We hand the attempt a Weak clone — the SyncPeer's Strong handle keeps the connection
        // pinned.
        let mut conn = match sync_peer.connection() {
            Some(stored) if stored.is_connected() => stored.clone_weak(),
            _ => self.dial_sync_peer(node_id).await?,
        };
        debug!(
            target: LOG_TARGET,
            "Attempting to synchronize horizon state with `{node_id}`"
        );
        // Defensive: the connection may have been torn down by another subsystem between
        // dial returning and this point (e.g. DhtConnectivity pruning).
        if !conn.is_connected() {
            warn!(
                target: LOG_TARGET,
                "Sync peer `{node_id}` was disconnected before RPC negotiation could begin"
            );
            return Err(HorizonSyncError::RpcError(RpcError::ClientClosed));
        }

        let config = RpcClient::builder()
            .with_deadline(self.config.rpc_deadline)
            .with_deadline_grace_period(Duration::from_secs(5));

        // Bound RPC negotiation so a stuck negotiation cannot wedge the sync loop.
        let mut client = tokio::time::timeout(
            self.config.rpc_deadline,
            conn.connect_rpc_using_builder::<rpc::BaseNodeSyncRpcClient>(config),
        )
        .await
        .map_err(|_| HorizonSyncError::RpcError(RpcError::ReplyTimeout))??;

        let latency = client
            .get_last_request_latency()
            .expect("unreachable panic: last request latency must be set after connect");
        self.sync_peers
            .get_mut(peer_index)
            .expect("Already checked")
            .set_latency(latency);
        if latency > self.max_latency {
            return Err(HorizonSyncError::MaxLatencyExceeded {
                peer: conn.peer_node_id().clone(),
                latency,
                max_latency: self.max_latency,
            });
        }
        debug!(target: LOG_TARGET, "Sync peer latency is {latency:.2?}");

        Ok((
            client,
            self.sync_peers.get(peer_index).expect("Already checked").clone(),
        ))
    }

    async fn dial_sync_peer(&self, node_id: &NodeId) -> Result<PeerConnection, HorizonSyncError> {
        let timer = Instant::now();
        debug!(target: LOG_TARGET, "Dialing {node_id} sync peer");
        // `synchronize()` holds Strong references to every sync peer for the duration of this
        // horizon sync, so the per-attempt redial can be Weak.
        let conn = self.connectivity.dial_peer(node_id.clone(), RefKind::Weak).await?;
        info!(
            target: LOG_TARGET,
            "Successfully dialed sync peer {} in {:.2?}",
            node_id,
            timer.elapsed()
        );
        Ok(conn)
    }

    async fn calculate_output_sync_start_stop(
        &self,
        sync_to_header: &BlockHeader,
    ) -> Result<(u64, BlockHeader), HorizonSyncError> {
        let stored_checkpoint = self.db.fetch_horizon_sync_output_checkpoint().await?;
        if stored_checkpoint.is_none() {
            debug!(target: LOG_TARGET, "No stored checkpoint, starting output sync from scratch");
            return Ok((1, sync_to_header.clone()));
        }
        let stored_checkpoint = stored_checkpoint.expect("Already checked");
        debug!(target: LOG_TARGET, "We have a stored checkpoint, with height {} and hash {}", stored_checkpoint.checkpoint_height, stored_checkpoint.checkpoint_hash.to_hex());
        let checkpoint_sync_to_header = if let Some(header) =
            self.db.fetch_header(stored_checkpoint.sync_target_height).await?
        {
            header
        } else {
            warn!(target: LOG_TARGET, "Horizon sync target at height {} is no longer on the canonical chain (reorg detected). Discarding checkpoint and restarting output sync from scratch.", stored_checkpoint.sync_target_height);
            self.db
                .write_transaction()
                .clear_horizon_sync_output_checkpoint()
                .commit()
                .await?;
            return Ok((1, sync_to_header.clone()));
        };
        if stored_checkpoint.sync_target_hash == sync_to_header.hash() {
            info!(target: LOG_TARGET, "Resuming output sync from checkpoint at height {}, target unchanged", stored_checkpoint.checkpoint_height);
            return Ok((stored_checkpoint.checkpoint_height + 1, sync_to_header.clone()));
        }
        // we have a checkpoint, its target is not the syncing target, so we have two choices, delete it and start over,
        // or sync to a lower height.
        if sync_to_header.height < stored_checkpoint.checkpoint_height {
            // new target is less, so we have to start over
            debug!(target: LOG_TARGET, "New target is less than stored checkpoint, starting output sync from scratch");
            self.db
                .write_transaction()
                .clear_horizon_sync_output_checkpoint()
                .commit()
                .await?;
            return Ok((1, sync_to_header.clone()));
        }
        if sync_to_header
            .height
            .saturating_sub(stored_checkpoint.checkpoint_height) >
            50_000
        {
            // we can sync to the checkpoint height first, and then do block sync from there
            debug!(target: LOG_TARGET, "New target is greater than stored checkpoint, but within 50_000 blocks, resuming output sync from checkpoint");
            return Ok((stored_checkpoint.checkpoint_height + 1, checkpoint_sync_to_header));
        }
        debug!(target: LOG_TARGET, "New target is too far away, starting over");
        self.db
            .write_transaction()
            .clear_horizon_sync_output_checkpoint()
            .commit()
            .await?;
        Ok((1, sync_to_header.clone()))
    }

    async fn sync_kernels_and_outputs(
        &mut self,
        sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        sync_to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        let (start_height, to_header) = self.calculate_output_sync_start_stop(sync_to_header).await?;
        // Note: We do not need to rewind kernels if the sync fails due to it being validated when inserted into
        //       the database. Furthermore, these kernels will also be successfully removed when we need to rewind
        //       the blockchain for whatever reason.
        debug!(target: LOG_TARGET, "Synchronizing kernels");
        self.synchronize_kernels(sync_peer.clone(), client, &to_header).await?;

        debug!(target: LOG_TARGET, "Synchronizing outputs");
        // let cloned_backup_smt = self.db.inner().smt_read_access()?.clone();
        match self
            .synchronize_outputs(sync_peer, client, &to_header, start_height)
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => {
                // We need to clean up the outputs
                let clean_up_height = match self.db.fetch_horizon_sync_output_checkpoint().await? {
                    Some(checkpoint) => checkpoint.checkpoint_height.saturating_add(1),
                    None => 1,
                };
                match self.clean_up_stale_synced_outputs(clean_up_height).await {
                    Ok(()) => {},
                    Err(e) => warn!(target: LOG_TARGET, "Failed to clean up after failed output sync: {}", e),
                };
                // let mut smt = self.db.inner().smt_write_access()?;
                // *smt = cloned_backup_smt;
                debug!(target: LOG_TARGET, "Finished clearing failed output sync");
                Err(err)
            },
        }
    }

    /// Cleanup stops at the last committed checkpoint so that previously-completed tranches are not disturbed.
    async fn clean_up_stale_synced_outputs(&mut self, start_height: u64) -> Result<(), ChainStorageError> {
        // Determine where to stop cleaning. If a tranche checkpoint exists, stop at the checkpoint block
        // Otherwise fall back to the current chain tip.

        let stop_height = self.db.fetch_last_header().await?.height;

        let mut current_height = start_height;
        let mut txn = self.db.write_transaction();
        let mut state_tree_updates = BTreeMap::<FixedHash, Option<FixedHash>>::new();
        let mut reporting_counter = 0;
        debug!(target: LOG_TARGET, "Cleaning up failed output sync, {}->{}", current_height, stop_height);
        while let Some(current_header) = self.db.fetch_header(current_height).await? {
            reporting_counter += 1;
            if reporting_counter % 1000 == 0 {
                debug!(target: LOG_TARGET, "Cleaning up failed output sync, progress: {}->{}",current_height, stop_height);
            }

            match self.db.fetch_outputs_in_block(current_header.hash()).await {
                Ok(outputs) => {
                    for (count, output) in (1..=outputs.len()).zip(outputs.iter()) {
                        txn.prune_output_from_all_dbs(
                            output.hash(),
                            output.commitment.clone(),
                            output.features.output_type,
                        );
                        if !output.is_burned() {
                            let key_bytes: [u8; 32] =
                                output.commitment.as_bytes().try_into().expect("Malformed commitment");
                            let smt_key = FixedHash::from(key_bytes);
                            state_tree_updates.insert(smt_key, None);
                        }
                        if (count % 100 == 0 || count == outputs.len()) &&
                            let Err(e) = txn.commit().await
                        {
                            warn!(
                                target: LOG_TARGET,
                                "Clean up failed sync - commit prune outputs for header '{}': {}",
                                current_header.hash(), e
                            );
                        }
                    }
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "Could not clean up failed output sync, error fetching outputs: {}", e);
                },
            }
            current_height += 1;
        }
        debug!(target: LOG_TARGET, "Finished cleaning up failed output sync, cleaned to height {}, proceeding to reinsert genesis outputs", stop_height);
        // Restore genesis-block outputs that an earlier horizon sync spent: `prune_output_from_all_dbs`
        // purged them from the UTXO LMDB tables when their STXO marker streamed in. If we don't
        // restore them after rewinding past their spend height, the next sync can't resolve the
        // STXO commitment Bob streams for the genesis-spending block and bans the peer with
        // "Peer sent unknown commitment hash".
        let genesis_block = self.db.fetch_genesis_block();
        let genesis_header_hash = *genesis_block.hash();
        let genesis_timestamp = genesis_block.block().header.timestamp.as_u64();
        for output in genesis_block.block().body.outputs() {
            if output.is_burned() {
                continue;
            }
            let present = self
                .db
                .fetch_unspent_output_hash_by_commitment(output.commitment.clone())
                .await?
                .is_some();
            if present {
                debug!(target: LOG_TARGET, "skipping genesis output {} with commitment({}) from pruned state", output.hash(), output.commitment.to_hex());
                continue;
            }
            let spent = self.db.fetch_inputs_mined_info(vec![output.hash()]).await?;
            if let Some(Some(spent_status)) = spent.first() &&
                spent_status.spent_height == 0
            {
                debug!(target: LOG_TARGET, "skipping genesis output {} with commitment({}) from pruned state, it was spent in block {}", output.hash(), output.commitment.to_hex(), spent_status.spent_height);
                continue;
            }
            let key_bytes: [u8; 32] = output.commitment.as_bytes().try_into().expect("Malformed commitment");
            let smt_key = FixedHash::from(key_bytes);
            let smt_node = output.smt_hash(0);
            state_tree_updates.insert(smt_key, Some(smt_node));
            // first make sure its deleted everywhere so we can cleanly reinsert it.
            debug!(target: LOG_TARGET, "Restoring genesis output {} with commitment({}) from pruned state", output.hash(), output.commitment.to_hex());

            txn.prune_output_from_all_dbs(output.hash(), output.commitment.clone(), output.features.output_type);
            txn.insert_output_via_horizon_sync(output.clone(), genesis_header_hash, 0, genesis_timestamp);
        }

        let tranche_updates = state_tree_updates
            .into_iter()
            .map(|(key, value)| HorizonStateTreeUpdate { key, value })
            .collect::<Vec<_>>();

        txn.apply_horizon_state_tree_updates(tranche_updates);

        txn.commit().await?;
        debug!(target: LOG_TARGET, "Finished cleaning up failed output sync, cleaned to height {}, genesis outputs restored", stop_height);
        Ok(())
    }

    async fn prune_if_needed(&mut self) -> Result<(), HorizonSyncError> {
        let local_metadata = self.db.get_chain_metadata().await?;
        let new_prune_height = cmp::min(local_metadata.best_block_height(), self.horizon_sync_height);
        if local_metadata.pruned_height() < new_prune_height {
            debug!(target: LOG_TARGET, "Pruning block chain to height {new_prune_height}");
            self.db.prune_to_height(new_prune_height).await?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn synchronize_kernels(
        &mut self,
        mut sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        info!(target: LOG_TARGET, "Starting kernel sync from peer {sync_peer}");
        let local_num_kernels = self.db().fetch_mmr_size(MmrTree::Kernel).await?;

        let remote_num_kernels = to_header.kernel_mmr_size;
        self.num_kernels = remote_num_kernels;

        if local_num_kernels >= remote_num_kernels {
            debug!(target: LOG_TARGET, "Local kernel set already synchronized");
            return Ok(());
        }

        let info = HorizonSyncInfo::new(vec![sync_peer.node_id().clone()], HorizonSyncStatus::Kernels {
            current: local_num_kernels,
            total: remote_num_kernels,
            sync_peer: sync_peer.clone(),
        });
        self.hooks.call_on_progress_horizon_hooks(info);

        debug!(
            target: LOG_TARGET,
            "Requesting kernels from {} to {} ({} remaining)",
            local_num_kernels,
            remote_num_kernels,
            remote_num_kernels.saturating_sub(local_num_kernels),
        );

        let latency = client.get_last_request_latency();
        debug!(
            target: LOG_TARGET,
            "Initiating kernel sync with peer `{}` (latency = {}ms)",
            sync_peer.node_id(),
            latency.unwrap_or_default().as_millis()
        );

        let mut current_header = self.db().fetch_header_containing_kernel_mmr(local_num_kernels).await?;
        let req = SyncKernelsRequest {
            start: local_num_kernels,
            end_header_hash: to_header.hash().to_vec(),
        };
        let mut kernel_stream = client.sync_kernels(req).await?;

        debug!(
            target: LOG_TARGET,
            "Found header for kernels at mmr pos: {} height: {}",
            local_num_kernels,
            current_header.height()
        );
        let mut kernel_hashes = vec![];
        let db = self.db().clone();
        let mut txn = db.write_transaction();
        let mut mmr_position = local_num_kernels;
        let end = remote_num_kernels;
        let mut last_sync_timer = Instant::now();
        let mut avg_latency = RollingAverageTime::new(20);
        while let Some(kernel) = kernel_stream.next().await {
            let latency = last_sync_timer.elapsed();
            avg_latency.add_sample(latency);
            let kernel: TransactionKernel = kernel?.try_into().map_err(HorizonSyncError::ConversionError)?;
            kernel.verify_signature()?;

            kernel_hashes.push(kernel.hash());

            if mmr_position > end {
                return Err(HorizonSyncError::IncorrectResponse(
                    "Peer sent too many kernels".to_string(),
                ));
            }

            txn.insert_kernel_via_horizon_sync(kernel, *current_header.hash(), mmr_position);
            if mmr_position == current_header.header().kernel_mmr_size.saturating_sub(1) {
                let num_kernels = kernel_hashes.len();
                debug!(
                    target: LOG_TARGET,
                    "Header #{} ({} kernels, latency: {:.2?})",
                    current_header.height(),
                    num_kernels,
                    latency
                );
                // Validate root
                let block_data = db
                    .fetch_block_accumulated_data(current_header.header().prev_hash)
                    .await?;
                let kernel_pruned_set = block_data.dissolve();
                let mut kernel_mmr = PrunedKernelMmr::new(kernel_pruned_set);

                for hash in kernel_hashes.drain(..) {
                    kernel_mmr.push(hash.to_vec())?;
                }

                let mmr_root = kernel_mmr.get_merkle_root()?;
                if mmr_root.as_slice() != current_header.header().kernel_mr.as_slice() {
                    return Err(HorizonSyncError::InvalidMrRoot {
                        mr_tree: MmrTree::Kernel.to_string(),
                        at_height: current_header.height(),
                        expected_hex: current_header.header().kernel_mr.to_hex(),
                        actual_hex: mmr_root.to_hex(),
                    });
                }

                let kernel_hash_set = kernel_mmr.get_pruned_hash_set()?;
                debug!(
                    target: LOG_TARGET,
                    "Updating block data at height {}",
                    current_header.height()
                );
                txn.update_block_accumulated_data_via_horizon_sync(
                    *current_header.hash(),
                    UpdateBlockAccumulatedData {
                        kernel_hash_set: Some(kernel_hash_set),
                        ..Default::default()
                    },
                );

                txn.commit().await?;
                debug!(
                    target: LOG_TARGET,
                    "Committed {} kernel(s), ({}/{}) {} remaining",
                    num_kernels,
                    mmr_position + 1,
                    end,
                    end.saturating_sub(mmr_position + 1)
                );
                if mmr_position < end.saturating_sub(1) {
                    current_header = db.fetch_chain_header(current_header.height() + 1).await?;
                }
            }
            mmr_position += 1;

            sync_peer.set_latency(latency);
            sync_peer.add_sample(last_sync_timer.elapsed());
            if mmr_position % 100 == 0 || mmr_position == self.num_kernels {
                let info = HorizonSyncInfo::new(vec![sync_peer.node_id().clone()], HorizonSyncStatus::Kernels {
                    current: mmr_position,
                    total: self.num_kernels,
                    sync_peer: sync_peer.clone(),
                });
                self.hooks.call_on_progress_horizon_hooks(info);
            }

            self.check_latency(sync_peer.node_id(), &avg_latency)?;

            last_sync_timer = Instant::now();
        }

        if mmr_position != end {
            return Err(HorizonSyncError::IncorrectResponse(
                "Sync node did not send all kernels requested".to_string(),
            ));
        }
        Ok(())
    }

    fn check_latency(&self, peer: &NodeId, avg_latency: &RollingAverageTime) -> Result<(), HorizonSyncError> {
        if let Some(avg_latency) = avg_latency.calculate_average_with_min_samples(5) &&
            avg_latency > self.max_latency
        {
            return Err(HorizonSyncError::MaxLatencyExceeded {
                peer: peer.clone(),
                latency: avg_latency,
                max_latency: self.max_latency,
            });
        }

        Ok(())
    }

    // Synchronize outputs in independently-verifiable tranches
    #[allow(clippy::too_many_lines)]
    async fn synchronize_outputs(
        &mut self,
        mut sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        to_header: &BlockHeader,
        sync_start_height: u64,
    ) -> Result<(), HorizonSyncError> {
        info!(target: LOG_TARGET, "Starting output sync from peer {sync_peer}");
        let db = self.db().clone();
        self.clean_up_stale_synced_outputs(sync_start_height).await?;

        if sync_start_height > to_header.height {
            info!(
                target: LOG_TARGET,
                "Output sync already complete (sync_start_height {} > horizon height {})",
                sync_start_height,
                to_header.height
            );
            return Ok(());
        }

        self.num_outputs = to_header.output_smt_size;

        // Progress is reported in block heights: unambiguous, monotonic and resume-correct.
        // The wire stream sends every output ever created in the range (then deletes spent ones
        // via STXO markers), so an output count would overshoot `output_smt_size`. Block heights
        // also preserve visible progress across a resume from a tranche checkpoint.
        let info = HorizonSyncInfo::new(vec![sync_peer.node_id().clone()], HorizonSyncStatus::Outputs {
            current: sync_start_height,
            total: to_header.height,
            sync_peer: sync_peer.clone(),
        });
        self.hooks.call_on_progress_horizon_hooks(info);

        let latency = client.get_last_request_latency();
        debug!(
            target: LOG_TARGET,
            "Initiating output sync with peer `{}`, requesting ~{} outputs from height {} to height {} \
            last_chain_header height `{}` (latency = {}ms)",
            sync_peer.node_id(),
            self.num_outputs,
            sync_start_height,
            to_header.height,
            db.fetch_last_chain_header().await?.height(),
            latency.unwrap_or_default().as_millis(),
        );

        let timer = Instant::now();
        let mut total_utxo_counter = 0u64;
        let mut total_stxo_counter = 0u64;
        let mut tranche_start_height = sync_start_height;

        // Process the full block range in tranches
        while tranche_start_height <= to_header.height {
            let tranche_end_height = cmp::min(
                tranche_start_height.saturating_add(HORIZON_SYNC_TRANCHE_SIZE),
                to_header.height,
            );

            let tranche_start_header =
                db.fetch_header(tranche_start_height)
                    .await?
                    .ok_or_else(|| ChainStorageError::ValueNotFound {
                        entity: "Header",
                        field: "height",
                        value: tranche_start_height.to_string(),
                    })?;
            let tranche_end_header = if tranche_end_height == to_header.height {
                to_header.clone()
            } else {
                db.fetch_header(tranche_end_height)
                    .await?
                    .ok_or_else(|| ChainStorageError::ValueNotFound {
                        entity: "Header",
                        field: "height",
                        value: tranche_end_height.to_string(),
                    })?
            };

            debug!(
                target: LOG_TARGET,
                "Syncing output tranche heights {}-{} ({} blocks) from peer {}",
                tranche_start_height,
                tranche_end_height,
                tranche_end_height.saturating_sub(tranche_start_height) + 1,
                sync_peer.node_id(),
            );

            let req = SyncUtxosRequest {
                start_header_hash: tranche_start_header.hash().to_vec(),
                end_header_hash: tranche_end_header.hash().to_vec(),
            };
            let mut output_stream = tokio::time::timeout(self.config.rpc_deadline, client.sync_utxos(req))
                .await
                .map_err(|_| {
                    HorizonSyncError::RpcStatus(RpcStatus::general(&format!(
                        "Timed out waiting for sync_utxos stream from peer {}",
                        sync_peer.node_id()
                    )))
                })??;

            let mut txn = db.write_transaction();
            let mut utxo_counter = 0u64;
            let mut stxo_counter = 0u64;
            let mut items_processed = 0u64;
            let mut last_sync_timer = Instant::now();
            let mut avg_latency = RollingAverageTime::new(20);

            // Accumulate SMT updates for the current tranche only. These are not applied until the full tranche
            // stream has been received and the SMT root verified.
            let mut state_tree_updates = BTreeMap::<FixedHash, Option<FixedHash>>::new();
            let mut inputs_to_delete = Vec::new();
            let mut batch_op_counter = 0;
            let mut last_mined_header: Option<FixedHash> = None;
            let mut current_header_hash = tranche_start_header.hash();
            let mut current_header = self
                .db()
                .fetch_header_by_block_hash(current_header_hash)
                .await?
                .ok_or_else(|| HorizonSyncError::IncorrectResponse("Starting header missing".into()))?;
            while let Some(response) = output_stream.next().await {
                let latency = last_sync_timer.elapsed();
                avg_latency.add_sample(latency);
                let res: SyncUtxosResponse = response?;

                let output_header_hash = FixedHash::try_from(res.mined_header).map_err(|e| {
                    HorizonSyncError::IncorrectResponse(format!("Peer sent invalid mined header: {}", e))
                })?;
                last_mined_header = Some(output_header_hash);
                if output_header_hash != current_header_hash {
                    current_header = self
                        .db()
                        .fetch_header_by_block_hash(output_header_hash)
                        .await?
                        .ok_or_else(|| {
                            HorizonSyncError::IncorrectResponse("Peer sent mined header we do not know of".into())
                        })?;
                    current_header_hash = output_header_hash;

                    debug!(
                        target: LOG_TARGET,
                        "Output sync reached new header( height: {}), synced {} outputs, spent {} inputs, \
                         latency: {:.2?}",
                        current_header.height,
                        utxo_counter,
                        stxo_counter,
                        latency
                    );
                }

                let proto_output = res.txo.ok_or_else(|| {
                    HorizonSyncError::IncorrectResponse("Peer sent no transaction output data".into())
                })?;
                match proto_output {
                    Txo::Output(output) => {
                        if current_header.height == 0 {
                            continue;
                        }
                        let output = TransactionOutput::try_from(output).map_err(HorizonSyncError::ConversionError)?;
                        if !output.is_burned() {
                            utxo_counter += 1;
                            let output_hash = output.hash();
                            trace!(
                                target: LOG_TARGET,
                                "UTXO `{}` received from sync peer at height {} (tranche {}-{}, {} in tranche)",
                                output_hash,
                                current_header.height,
                                tranche_start_height,
                                tranche_end_height,
                                utxo_counter,
                            );
                            let key_bytes: [u8; 32] = output.commitment.as_bytes().try_into().map_err(|e| {
                                HorizonSyncError::IncorrectResponse(format!("Peer sent malformed commitment: {}", e))
                            })?;
                            let key = FixedHash::from(key_bytes);

                            if state_tree_updates
                                .insert(key, Some(output.smt_hash(current_header.height)))
                                .is_some()
                            {
                                return Err(HorizonSyncError::IncorrectResponse(
                                    "Peer sent duplicate output commitment during horizon sync".into(),
                                ));
                            }

                            let constants = self.rules.consensus_constants(current_header.height).clone();
                            validate_output_version(&constants, &output)?;
                            validate_individual_output(&output, &constants)?;
                            batch_verify_range_proofs(&self.prover, &[&output])?;

                            txn.insert_output_via_horizon_sync(
                                output,
                                current_header.hash(),
                                current_header.height,
                                current_header.timestamp.as_u64(),
                            );
                            batch_op_counter += 1;
                        }
                    },
                    Txo::Commitment(commitment_bytes) => {
                        stxo_counter += 1;

                        let commitment = CompressedCommitment::from_canonical_bytes(commitment_bytes.as_slice())?;
                        match self
                            .db()
                            .fetch_unspent_output_hash_by_commitment(commitment.clone())
                            .await?
                        {
                            Some(output_hash) => {
                                trace!(
                                    target: LOG_TARGET,
                                    "STXO hash `{output_hash}` received from sync peer ({stxo_counter})",
                                );
                                let key_bytes: [u8; 32] = commitment_bytes.as_slice().try_into().map_err(|e| {
                                    HorizonSyncError::IncorrectResponse(format!(
                                        "Peer sent malformed commitment: {}",
                                        e
                                    ))
                                })?;
                                let key = FixedHash::from(key_bytes);

                                if matches!(state_tree_updates.get(&key), Some(None)) {
                                    return Err(HorizonSyncError::ChainStorageError(
                                        ChainStorageError::UnspendableInput,
                                    ));
                                }
                                state_tree_updates.insert(key, None);

                                let output_info = self.db().fetch_output(output_hash).await?.ok_or_else(|| {
                                    HorizonSyncError::IncorrectResponse(
                                        "Could not fetch full output for spent commitment".into(),
                                    )
                                })?;
                                inputs_to_delete.push(output_info.output);
                            },
                            None => {
                                return Err(HorizonSyncError::IncorrectResponse(
                                    "Peer sent unknown commitment hash".into(),
                                ));
                            },
                        }
                    },
                }

                items_processed += 1;
                if items_processed.is_multiple_of(PROGRESS_REPORT_INTERVAL) {
                    let info = HorizonSyncInfo::new(vec![sync_peer.node_id().clone()], HorizonSyncStatus::Outputs {
                        current: current_header.height,
                        total: to_header.height,
                        sync_peer: sync_peer.clone(),
                    });
                    self.hooks.call_on_progress_horizon_hooks(info);
                }

                if batch_op_counter >= HORIZON_SYNC_BATCH_SIZE {
                    txn.commit().await?;
                    txn = db.write_transaction();
                    batch_op_counter = 0;
                    debug!(
                        target: LOG_TARGET,
                        "Committed batch of {} output updates at height {} (tranche {}-{}, {} total in tranche)",
                        HORIZON_SYNC_BATCH_SIZE,
                        current_header.height,
                        tranche_start_height,
                        tranche_end_height,
                        utxo_counter + stxo_counter,
                    );
                }

                sync_peer.set_latency(latency);
                sync_peer.add_sample(last_sync_timer.elapsed());
                last_sync_timer = Instant::now();
            }

            // Verify the stream completed the full tranche before committing.
            if let Some(last_hash) = last_mined_header &&
                last_hash != tranche_end_header.hash()
            {
                return Err(HorizonSyncError::IncorrectResponse(format!(
                    "Sync peer did not complete output stream for tranche {}-{}. Last block received: {}, expected: {}",
                    tranche_start_height,
                    tranche_end_height,
                    last_hash.to_hex(),
                    tranche_end_header.hash().to_hex()
                )));
            }

            let tranche_updates = state_tree_updates
                .into_iter()
                .map(|(key, value)| HorizonStateTreeUpdate { key, value })
                .collect::<Vec<_>>();

            txn.apply_horizon_state_tree_updates(tranche_updates);
            for output in &inputs_to_delete {
                if let Some(sidechain_feature) = output.features.sidechain_feature.as_ref() &&
                    let Some(vn_reg) = sidechain_feature.validator_node_registration()
                {
                    txn.delete_validator_node(
                        sidechain_feature.sidechain_public_key().cloned(),
                        vn_reg.public_key().clone(),
                    );
                }
            }
            for output in inputs_to_delete {
                txn.prune_output_from_all_dbs(output.hash(), output.commitment.clone(), output.features.output_type);
            }
            // Only checkpoint intermediate tranches for network recovery.
            // We intentionally do not checkpoint the final tranche before verification.
            // If the final root verification fails, the ENTIRE state is considered poisoned.
            if tranche_end_height < to_header.height {
                txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
                    checkpoint_height: tranche_end_height,
                    checkpoint_hash: tranche_end_header.hash(),
                    sync_target_height: to_header.height,
                    sync_target_hash: to_header.hash(),
                });
            }
            txn.commit().await?;

            debug!(
                target: LOG_TARGET,
                "Committed output tranche heights {}-{}: {} UTXOs and {} STXOs",
                tranche_start_height,
                tranche_end_height,
                utxo_counter,
                stxo_counter,
            );

            total_utxo_counter += utxo_counter;
            total_stxo_counter += stxo_counter;
            tranche_start_height = tranche_end_height + 1;
        }

        if let Err(e) = db.verify_horizon_sync_output_root(to_header.output_mr).await {
            warn!(
                target: LOG_TARGET,
                "Final JMT root verification failed! The entire synced state is poisoned. Clearing checkpoint."
            );
            let _unused = db
                .write_transaction()
                .clear_horizon_sync_output_checkpoint()
                .commit()
                .await;
            return Err(HorizonSyncError::ChainStorageError(e));
        }

        // Mark output sync as complete
        db.write_transaction()
            .set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
                checkpoint_height: to_header.height,
                checkpoint_hash: to_header.hash(),
                sync_target_height: to_header.height,
                sync_target_hash: to_header.hash(),
            })
            .commit()
            .await?;

        debug!(
            target: LOG_TARGET,
            "Finished syncing TXOs: {} unspent and {} spent downloaded in {:.2?}",
            total_utxo_counter,
            total_stxo_counter,
            timer.elapsed()
        );
        Ok(())
    }

    // Finalize the horizon state synchronization by setting the chain metadata to the local tip and committing
    // the horizon state to the blockchain backend.
    async fn finalize_horizon_sync(&mut self, sync_peer: &SyncPeer) -> Result<(), HorizonSyncError> {
        debug!(target: LOG_TARGET, "Validating horizon state");

        self.hooks.call_on_progress_horizon_hooks(HorizonSyncInfo::new(
            vec![sync_peer.node_id().clone()],
            HorizonSyncStatus::Finalizing,
        ));

        let header = self.db().fetch_chain_header(self.horizon_sync_height).await?;
        let (calc_utxo_sum, calc_kernel_sum, calc_burned_sum) = self.calculate_commitment_sums(&header).await?;

        self.final_state_validator
            .validate(
                &*self.db().inner().db_read_access()?,
                header.height(),
                &calc_utxo_sum,
                &calc_kernel_sum,
                &calc_burned_sum,
            )
            .map_err(HorizonSyncError::FinalStateValidationFailed)?;

        let metadata = self.db().get_chain_metadata().await?;
        info!(
            target: LOG_TARGET,
            "Horizon state validation succeeded! Committing horizon state."
        );
        self.db()
            .write_transaction()
            .set_best_block(
                header.height(),
                *header.hash(),
                header.accumulated_data().total_accumulated_difficulty,
                *metadata.best_block_hash(),
                header.timestamp(),
            )
            .set_pruned_height(header.height())
            .set_horizon_data(calc_kernel_sum, calc_utxo_sum)
            .clear_horizon_sync_output_checkpoint()
            .commit()
            .await?;

        Ok(())
    }

    /// (UTXO sum, Kernel sum)
    async fn calculate_commitment_sums(
        &mut self,
        header: &ChainHeader,
    ) -> Result<(CompressedCommitment, CompressedCommitment, CompressedCommitment), HorizonSyncError> {
        let mut utxo_sum = HomomorphicCommitment::default();
        let mut kernel_sum = HomomorphicCommitment::default();
        let mut burned_sum = HomomorphicCommitment::default();

        let mut prev_kernel_mmr = 0;

        let height = header.height();
        let db = self.db().inner().clone();
        let header_hash = *header.hash();
        task::spawn_blocking(move || {
            for h in 0..=height {
                let curr_header = db.fetch_chain_header(h)?;
                trace!(
                    target: LOG_TARGET,
                    "Fetching utxos from db: height:{}",
                    curr_header.height(),
                );
                let utxos = db.fetch_outputs_in_block_with_spend_state(*curr_header.hash(), Some(header_hash))?;
                debug!(
                    target: LOG_TARGET,
                    "{} output(s) loaded for height {}",
                    utxos.len(),
                    curr_header.height()
                );
                trace!(
                    target: LOG_TARGET,
                    "Fetching kernels from db: height:{}, header.kernel_mmr:{}, prev_mmr:{}, end:{}",
                    curr_header.height(),
                    curr_header.header().kernel_mmr_size,
                    prev_kernel_mmr,
                    curr_header.header().kernel_mmr_size.saturating_sub(1)
                );

                trace!(target: LOG_TARGET, "Number of utxos returned: {}", utxos.len());
                for (u, spent) in utxos {
                    if !spent {
                        utxo_sum = &u.commitment.to_commitment()? + &utxo_sum;
                    }
                }

                let kernels = db.fetch_kernels_in_block(*curr_header.hash())?;
                trace!(target: LOG_TARGET, "Number of kernels returned: {}", kernels.len());
                for k in kernels {
                    kernel_sum = &k.excess.to_commitment()? + &kernel_sum;
                    if k.is_burned() {
                        burned_sum = &(k.get_burn_commitment()?.to_commitment()?) + &burned_sum;
                    }
                }
                prev_kernel_mmr = curr_header.header().kernel_mmr_size;

                if h % 1000 == 0 && height != 0 {
                    debug!(
                        target: LOG_TARGET,
                        "Final Validation: {:.2}% complete. Height: {} sync",
                        (h as f32 / height as f32) * 100.0,
                        h,
                    );
                }
            }

            Ok((
                CompressedCommitment::from_commitment(utxo_sum),
                CompressedCommitment::from_commitment(kernel_sum),
                CompressedCommitment::from_commitment(burned_sum),
            ))
        })
        .await?
    }

    // Sync peers are also removed from the list of sync peers if the ban duration is longer than the short ban period.
    fn remove_sync_peer(&mut self, node_id: &NodeId) {
        if let Some(pos) = self.sync_peers.iter().position(|p| p.node_id() == node_id) {
            self.sync_peers.remove(pos);
        }
    }

    // Helper function to get the index to the node_id inside of the vec of peers
    fn get_sync_peer_index(&mut self, node_id: &NodeId) -> Option<usize> {
        self.sync_peers.iter().position(|p| p.node_id() == node_id)
    }

    #[inline]
    fn db(&self) -> &AsyncBlockchainDb<B> {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use tari_comms::test_utils::mocks::create_connectivity_mock;
    use tari_transaction_components::crypto_factories::CryptoFactories;

    use super::*;
    use crate::{
        chain_storage::{BlockchainDatabase, DbTransaction, HorizonSyncOutputCheckpoint},
        test_helpers::{
            blockchain::{TempDatabase, create_main_chain, create_new_blockchain},
            create_consensus_rules,
        },
        validation::mocks::MockValidator,
    };

    fn build_synchronizer<'a>(
        db: BlockchainDatabase<TempDatabase>,
        sync_peers: &'a mut Vec<SyncPeer>,
        horizon_sync_height: u64,
    ) -> HorizonStateSynchronization<'a, TempDatabase> {
        let (connectivity, _mock) = create_connectivity_mock();
        let rules = create_consensus_rules();
        let prover = CryptoFactories::default().range_proof;
        let validator: Arc<dyn FinalHorizonStateValidation<TempDatabase>> = Arc::new(MockValidator::new(true));
        HorizonStateSynchronization::new(
            BlockchainSyncConfig::default(),
            db.into(),
            connectivity,
            rules,
            sync_peers,
            horizon_sync_height,
            prover,
            validator,
        )
    }

    #[tokio::test]
    async fn calculate_start_no_checkpoint_starts_from_height_one() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["1->GB"], ["2->1"], ["3->2"]));
        let target = chain.get("3").unwrap().header().clone();
        let target_hash = target.hash();

        let mut peers: Vec<SyncPeer> = vec![];
        let sync = build_synchronizer(db, &mut peers, target.height);

        let (start, stop_header) = sync.calculate_output_sync_start_stop(&target).await.unwrap();
        assert_eq!(start, 1);
        assert_eq!(stop_header.hash(), target_hash);
    }

    #[tokio::test]
    async fn calculate_start_resumes_when_target_unchanged() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["1->GB"], ["2->1"], ["3->2"]));
        let target = chain.get("3").unwrap().header().clone();
        let target_hash = target.hash();
        let checkpoint_block = chain.get("2").unwrap();

        let mut txn = DbTransaction::new();
        txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: 2,
            checkpoint_hash: *checkpoint_block.hash(),
            sync_target_height: target.height,
            sync_target_hash: target_hash,
        });
        db.write(txn).unwrap();

        let mut peers: Vec<SyncPeer> = vec![];
        let sync = build_synchronizer(db, &mut peers, target.height);

        let (start, stop_header) = sync.calculate_output_sync_start_stop(&target).await.unwrap();
        assert_eq!(start, 3);
        assert_eq!(stop_header.hash(), target_hash);
    }

    #[tokio::test]
    async fn calculate_start_discards_checkpoint_after_reorg() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["1->GB"], ["2->1"], ["3->2"]));
        let target = chain.get("3").unwrap().header().clone();
        let target_hash = target.hash();

        // Store a checkpoint at a height that no longer exists on the canonical chain
        let mut txn = DbTransaction::new();
        txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: 99,
            checkpoint_hash: FixedHash::zero(),
            sync_target_height: 120,
            sync_target_hash: FixedHash::zero(),
        });
        db.write(txn).unwrap();

        let db_clone = db.clone();
        let mut peers: Vec<SyncPeer> = vec![];
        let sync = build_synchronizer(db, &mut peers, target.height);

        let (start, stop_header) = sync.calculate_output_sync_start_stop(&target).await.unwrap();
        assert_eq!(start, 1);
        assert_eq!(stop_header.hash(), target_hash);
        assert!(
            db_clone.fetch_horizon_sync_output_checkpoint().unwrap().is_none(),
            "stale checkpoint must be cleared"
        );
    }

    #[tokio::test]
    async fn clean_up_stale_synced_outputs_prunes_outputs_above_start_height() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["1->GB"], ["2->1"], ["3->2"]));

        // Pick an output from a block above start_height (we'll clean from height 1)
        let block2 = chain.get("2").unwrap();
        let outputs = db.fetch_outputs_in_block(*block2.hash()).unwrap();
        let output_hash = outputs.first().expect("block 2 must have outputs").hash();
        assert!(
            db.fetch_output(output_hash).unwrap().is_some(),
            "output must exist before cleanup"
        );

        let db_clone = db.clone();
        let mut peers: Vec<SyncPeer> = vec![];
        let mut sync = build_synchronizer(db, &mut peers, 3);

        sync.clean_up_stale_synced_outputs(1).await.unwrap();

        assert!(
            db_clone.fetch_output(output_hash).unwrap().is_none(),
            "output above start_height must be pruned"
        );
    }
}
