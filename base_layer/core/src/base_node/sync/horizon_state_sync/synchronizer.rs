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
    convert::{TryFrom, TryInto},
    mem,
    sync::Arc,
    time::{Duration, Instant},
};

use croaring::Bitmap;
use futures::{stream::FuturesUnordered, StreamExt};
use log::*;
use tari_common_types::types::{Commitment, RangeProofService};
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::NodeId,
    protocol::rpc::{RpcClient, RpcError},
};
use tari_crypto::{commitment::HomomorphicCommitment, tari_utilities::hex::Hex};
use tokio::task;

use super::error::HorizonSyncError;
use crate::{
    base_node::sync::{
        hooks::Hooks,
        horizon_state_sync::{HorizonSyncInfo, HorizonSyncStatus},
        rpc,
        BlockchainSyncConfig,
        SyncPeer,
    },
    blocks::{BlockHeader, ChainHeader, UpdateBlockAccumulatedData},
    chain_storage::{
        async_db::AsyncBlockchainDb,
        BlockchainBackend,
        ChainStorageError,
        DbTransaction,
        MmrTree,
        PrunedOutput,
    },
    common::rolling_avg::RollingAverageTime,
    consensus::ConsensusManager,
    proto::base_node::{
        sync_utxo as proto_sync_utxo,
        sync_utxos_response::UtxoOrDeleted,
        SyncKernelsRequest,
        SyncUtxo,
        SyncUtxosRequest,
        SyncUtxosResponse,
    },
    transactions::transaction_components::{
        transaction_output::batch_verify_range_proofs,
        TransactionKernel,
        TransactionOutput,
    },
    validation::{helpers, FinalHorizonStateValidation},
    MutablePrunedOutputMmr,
    PrunedKernelMmr,
    PrunedOutputMmr,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::horizon_state_sync";

pub struct HorizonStateSynchronization<'a, B> {
    config: BlockchainSyncConfig,
    db: AsyncBlockchainDb<B>,
    rules: ConsensusManager,
    sync_peers: &'a [SyncPeer],
    horizon_sync_height: u64,
    prover: Arc<RangeProofService>,
    num_kernels: u64,
    num_outputs: u64,
    full_bitmap: Option<Bitmap>,
    hooks: Hooks,
    connectivity: ConnectivityRequester,
    final_state_validator: Arc<dyn FinalHorizonStateValidation<B>>,
    max_latency: Duration,
}

impl<'a, B: BlockchainBackend + 'static> HorizonStateSynchronization<'a, B> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: BlockchainSyncConfig,
        db: AsyncBlockchainDb<B>,
        connectivity: ConnectivityRequester,
        rules: ConsensusManager,
        sync_peers: &'a [SyncPeer],
        horizon_sync_height: u64,
        prover: Arc<RangeProofService>,
        final_state_validator: Arc<dyn FinalHorizonStateValidation<B>>,
    ) -> Self {
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
            full_bitmap: None,
            hooks: Hooks::default(),
            final_state_validator,
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
        let header = self.db().fetch_header(self.horizon_sync_height).await?.ok_or_else(|| {
            ChainStorageError::ValueNotFound {
                entity: "Header",
                field: "height",
                value: self.horizon_sync_height.to_string(),
            }
        })?;

        loop {
            match self.sync(&header).await {
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
                },
                Err(err) => return Err(err),
            }
        }
    }

    async fn sync(&mut self, header: &BlockHeader) -> Result<(), HorizonSyncError> {
        info!(
            target: LOG_TARGET,
            "Attempting to sync blocks({} sync peers)",
            self.sync_peers.len()
        );
        for (i, sync_peer) in self.sync_peers.iter().enumerate() {
            self.hooks.call_on_starting_hook(sync_peer);
            let mut connection = match self.connectivity.dial_peer(sync_peer.node_id().clone()).await {
                Ok(conn) => conn,
                Err(err) => {
                    warn!(target: LOG_TARGET, "Failed to connect to sync peer `{}`: {}", sync_peer.node_id(), err);
                    continue;
                },
            };
            let config = RpcClient::builder()
                .with_deadline(self.config.rpc_deadline)
                .with_deadline_grace_period(Duration::from_secs(3));
            let mut client = match connection.connect_rpc_using_builder(config).await {
                Ok(rpc_client) => rpc_client,
                Err(err) => {
                    warn!(target: LOG_TARGET, "Failed to establish RPC coonection with sync peer `{}`: {}", sync_peer.node_id(), err);
                    continue;
                },
            };

            match self.begin_sync(sync_peer.clone(), &mut client, header).await {
                Ok(_) => match self.finalize_horizon_sync(sync_peer).await {
                    Ok(_) => return Ok(()),
                    Err(err) => {
                        self.ban_peer_on_bannable_error(sync_peer, &err).await?;
                        warn!(target: LOG_TARGET, "Error during sync:{}", err);
                    },
                },
                Err(err @ HorizonSyncError::RpcError(RpcError::ReplyTimeout)) |
                Err(err @ HorizonSyncError::MaxLatencyExceeded { .. }) => {
                    self.ban_peer_on_bannable_error(sync_peer, &err).await?;
                    warn!(target: LOG_TARGET, "{}", err);
                    if i == self.sync_peers.len() - 1 {
                        return Err(HorizonSyncError::AllSyncPeersExceedLatency);
                    }
                },
                Err(err) => {
                    self.ban_peer_on_bannable_error(sync_peer, &err).await?;
                    warn!(target: LOG_TARGET, "Error during sync:{}", err);
                },
            }
        }

        Err(HorizonSyncError::FailedSyncAllPeers)
    }

    async fn ban_peer_on_bannable_error(
        &mut self,
        peer: &SyncPeer,
        error: &HorizonSyncError,
    ) -> Result<(), HorizonSyncError> {
        match error {
            HorizonSyncError::ChainStorageError(_) |
            HorizonSyncError::JoinError(_) |
            HorizonSyncError::RpcError(_) |
            HorizonSyncError::RpcStatus(_) |
            HorizonSyncError::ConnectivityError(_) |
            HorizonSyncError::NoSyncPeers |
            HorizonSyncError::FailedSyncAllPeers |
            HorizonSyncError::AllSyncPeersExceedLatency => {
                // these are local errors so we dont ban die per
            },
            HorizonSyncError::MaxLatencyExceeded { .. } => {
                warn!(target: LOG_TARGET, "Banned sync peer for short while because peer exceeded max latency: {}",error.to_string());
                if let Err(err) = self
                    .connectivity
                    .ban_peer_until(peer.node_id().clone(), self.config.short_ban_period, error.to_string())
                    .await
                {
                    error!(target: LOG_TARGET, "Failed to ban peer: {}", err);
                }
            },
            _ => {
                warn!(target: LOG_TARGET, "Banned sync peer for because: {}",error.to_string());
                if let Err(err) = self
                    .connectivity
                    .ban_peer_until(peer.node_id().clone(), self.config.ban_period, error.to_string())
                    .await
                {
                    error!(target: LOG_TARGET, "Failed to ban peer: {}", err);
                }
            },
        }
        Ok(())
    }

    async fn begin_sync(
        &mut self,
        sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        debug!(target: LOG_TARGET, "Initializing");
        self.initialize().await?;

        debug!(target: LOG_TARGET, "Synchronizing kernels");
        self.synchronize_kernels(sync_peer.clone(), client, to_header).await?;
        debug!(target: LOG_TARGET, "Synchronizing outputs");
        self.synchronize_outputs(sync_peer, client, to_header).await?;
        Ok(())
    }

    async fn initialize(&mut self) -> Result<(), HorizonSyncError> {
        let db = self.db();
        let local_metadata = db.get_chain_metadata().await?;

        let new_prune_height = cmp::min(local_metadata.height_of_longest_chain(), self.horizon_sync_height);
        if local_metadata.pruned_height() < new_prune_height {
            debug!(target: LOG_TARGET, "Pruning block chain to height {}", new_prune_height);
            db.prune_to_height(new_prune_height).await?;
        }

        self.full_bitmap = Some(db.fetch_deleted_bitmap_at_tip().await?.into_bitmap());

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn synchronize_kernels(
        &mut self,
        mut sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        info!(target: LOG_TARGET, "Starting kernel sync from peer {}", sync_peer);
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
            remote_num_kernels - local_num_kernels,
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
                return Err(HorizonSyncError::IncorrectResponse(format!(
                    "Peer sent too many kernels",
                )));
            }

            let mmr_position_u32 = u32::try_from(mmr_position).map_err(|_| HorizonSyncError::InvalidMmrPosition {
                at_height: current_header.height(),
                mmr_position,
            })?;
            txn.insert_kernel_via_horizon_sync(kernel, *current_header.hash(), mmr_position_u32);
            if mmr_position == current_header.header().kernel_mmr_size - 1 {
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
                let kernel_pruned_set = block_data.dissolve().0;
                let mut kernel_mmr = PrunedKernelMmr::new(kernel_pruned_set);

                for hash in kernel_hashes.drain(..) {
                    kernel_mmr.push(hash.to_vec())?;
                }

                let mmr_root = kernel_mmr.get_merkle_root()?;
                if mmr_root.as_slice() != current_header.header().kernel_mr.as_slice() {
                    return Err(HorizonSyncError::InvalidMmrRoot {
                        mmr_tree: MmrTree::Kernel,
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
                    end - (mmr_position + 1)
                );
                if mmr_position < end - 1 {
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
        if let Some(avg_latency) = avg_latency.calculate_average_with_min_samples(5) {
            if avg_latency > self.max_latency {
                return Err(HorizonSyncError::MaxLatencyExceeded {
                    peer: peer.clone(),
                    latency: avg_latency,
                    max_latency: self.max_latency,
                });
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn synchronize_outputs(
        &mut self,
        mut sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        info!(target: LOG_TARGET, "Starting output sync from peer {}", sync_peer);
        let local_num_outputs = self.db().fetch_mmr_size(MmrTree::Utxo).await?;

        let remote_num_outputs = to_header.output_mmr_size;
        self.num_outputs = remote_num_outputs;

        if local_num_outputs >= remote_num_outputs {
            debug!(target: LOG_TARGET, "Local output set already synchronized");
            return Ok(());
        }

        let info = HorizonSyncInfo::new(vec![sync_peer.node_id().clone()], HorizonSyncStatus::Outputs {
            current: local_num_outputs,
            total: self.num_outputs,
            sync_peer: sync_peer.clone(),
        });
        self.hooks.call_on_progress_horizon_hooks(info);

        debug!(
            target: LOG_TARGET,
            "Requesting outputs from {} to {} ({} remaining)",
            local_num_outputs,
            remote_num_outputs,
            remote_num_outputs - local_num_outputs,
        );

        let start = local_num_outputs;
        let end = remote_num_outputs;
        let end_hash = to_header.hash();

        let latency = client.get_last_request_latency();
        debug!(
            target: LOG_TARGET,
            "Initiating output sync with peer `{}` (latency = {}ms)",
            sync_peer.node_id(),
            latency.unwrap_or_default().as_millis()
        );

        let req = SyncUtxosRequest {
            start,
            end_header_hash: end_hash.to_vec(),
            include_deleted_bitmaps: true,
            include_pruned_utxos: true,
        };

        let mut current_header = self.db().fetch_header_containing_utxo_mmr(start).await?;
        let mut output_stream = client.sync_utxos(req).await?;

        debug!(
            target: LOG_TARGET,
            "Found header for utxos at mmr pos: {} - {} height: {}",
            start,
            current_header.header().output_mmr_size,
            current_header.height()
        );

        let db = self.db().clone();

        let mut txn = db.write_transaction();
        let mut unpruned_outputs = vec![];
        let mut mmr_position = start;
        let mut height_utxo_counter = 0u64;
        let mut height_txo_counter = 0u64;
        let mut timer = Instant::now();

        let block_data = db
            .fetch_block_accumulated_data(current_header.header().prev_hash)
            .await?;
        let (_, output_pruned_set, _) = block_data.dissolve();

        let mut output_mmr = PrunedOutputMmr::new(output_pruned_set);
        let mut constants = self.rules.consensus_constants(current_header.height()).clone();
        let mut last_sync_timer = Instant::now();
        let mut avg_latency = RollingAverageTime::new(20);

        while let Some(response) = output_stream.next().await {
            let latency = last_sync_timer.elapsed();
            avg_latency.add_sample(latency);
            let res: SyncUtxosResponse = response?;

            if mmr_position > end {
                return Err(HorizonSyncError::IncorrectResponse(format!(
                    "Peer sent too many outputs",
                )));
            }

            if res.mmr_index != 0 && res.mmr_index != mmr_position {
                return Err(HorizonSyncError::IncorrectResponse(format!(
                    "Expected MMR position of {} but got {}",
                    mmr_position, res.mmr_index,
                )));
            }

            let txo = res
                .utxo_or_deleted
                .ok_or_else(|| HorizonSyncError::IncorrectResponse("Peer sent no transaction output data".into()))?;

            match txo {
                UtxoOrDeleted::Utxo(SyncUtxo {
                    utxo: Some(proto_sync_utxo::Utxo::Output(output)),
                }) => {
                    trace!(
                        target: LOG_TARGET,
                        "UTXO {} received from sync peer for header #{}",
                        res.mmr_index,
                        current_header.height()
                    );
                    height_utxo_counter += 1;
                    let output = TransactionOutput::try_from(output).map_err(HorizonSyncError::ConversionError)?;
                    helpers::check_tari_script_byte_size(&output.script, constants.max_script_byte_size())?;
                    unpruned_outputs.push(output.clone());

                    output_mmr.push(output.hash().to_vec())?;

                    txn.insert_output_via_horizon_sync(
                        output,
                        *current_header.hash(),
                        current_header.height(),
                        u32::try_from(mmr_position)?,
                        current_header.timestamp(),
                    );
                    mmr_position += 1;
                },
                UtxoOrDeleted::Utxo(SyncUtxo {
                    utxo: Some(proto_sync_utxo::Utxo::PrunedOutput(utxo)),
                }) => {
                    trace!(
                        target: LOG_TARGET,
                        "UTXO {} (pruned) received from sync peer for header #{}",
                        res.mmr_index,
                        current_header.height()
                    );
                    height_txo_counter += 1;
                    output_mmr.push(utxo.hash.clone())?;

                    txn.insert_pruned_output_via_horizon_sync(
                        utxo.hash.try_into()?,
                        *current_header.hash(),
                        current_header.height(),
                        u32::try_from(mmr_position)?,
                        current_header.timestamp(),
                    );
                    mmr_position += 1;
                },
                UtxoOrDeleted::DeletedDiff(diff_bitmap) => {
                    if mmr_position != current_header.header().output_mmr_size {
                        return Err(HorizonSyncError::IncorrectResponse(format!(
                            "Peer unexpectedly sent a deleted bitmap. Expected at MMR index {} but it was sent at {}",
                            current_header.header().output_mmr_size,
                            mmr_position
                        )));
                    }

                    // Check that the difference bitmap isn't excessively large. Bitmap::deserialize panics if greater
                    // than isize::MAX, however isize::MAX is still an inordinate amount of data. An
                    // arbitrary 4 MiB limit is used.
                    const MAX_DIFF_BITMAP_BYTE_LEN: usize = 4 * 1024 * 1024;
                    if diff_bitmap.len() > MAX_DIFF_BITMAP_BYTE_LEN {
                        return Err(HorizonSyncError::IncorrectResponse(format!(
                            "Received difference bitmap (size = {}) that exceeded the maximum size limit of {} from \
                             peer {}",
                            diff_bitmap.len(),
                            MAX_DIFF_BITMAP_BYTE_LEN,
                            sync_peer.node_id()
                        )));
                    }

                    let diff_bitmap = Bitmap::try_deserialize(&diff_bitmap).ok_or_else(|| {
                        HorizonSyncError::IncorrectResponse(format!(
                            "Peer {} sent an invalid difference bitmap",
                            sync_peer.node_id()
                        ))
                    })?;

                    // Merge the differences into the final bitmap so that we can commit to the entire spend state
                    // in the output MMR
                    let bitmap = self.full_bitmap_mut();
                    bitmap.or_inplace(&diff_bitmap);

                    let pruned_output_set = output_mmr.get_pruned_hash_set()?;
                    let output_mmr = MutablePrunedOutputMmr::new(pruned_output_set.clone(), bitmap.clone())?;

                    let mmr_root = output_mmr.get_merkle_root()?;
                    if mmr_root.as_slice() != current_header.header().output_mr.as_slice() {
                        return Err(HorizonSyncError::InvalidMmrRoot {
                            mmr_tree: MmrTree::Utxo,
                            at_height: current_header.height(),
                            expected_hex: current_header.header().output_mr.to_hex(),
                            actual_hex: mmr_root.to_hex(),
                        });
                    }

                    self.validate_rangeproofs(mem::take(&mut unpruned_outputs)).await?;

                    txn.update_deleted_bitmap(diff_bitmap.clone());

                    txn.update_block_accumulated_data_via_horizon_sync(
                        *current_header.hash(),
                        UpdateBlockAccumulatedData {
                            utxo_hash_set: Some(pruned_output_set),
                            deleted_diff: Some(diff_bitmap.into()),
                            ..Default::default()
                        },
                    );
                    txn.commit().await?;

                    debug!(
                        target: LOG_TARGET,
                        "UTXO: {}/{}, Header #{}, added {} utxos, added {} txos in {:.2?}",
                        mmr_position,
                        end,
                        current_header.height(),
                        height_utxo_counter,
                        height_txo_counter,
                        timer.elapsed()
                    );
                    height_txo_counter = 0;
                    height_utxo_counter = 0;
                    timer = Instant::now();

                    if mmr_position == end {
                        debug!(
                            target: LOG_TARGET,
                            "Sync complete at mmr position {}, height #{}",
                            mmr_position,
                            current_header.height()
                        );
                        break;
                    } else {
                        current_header = db.fetch_chain_header(current_header.height() + 1).await?;
                        constants = self.rules.consensus_constants(current_header.height()).clone();
                        debug!(
                            target: LOG_TARGET,
                            "Expecting to receive the next UTXO set {}-{} for header #{}",
                            mmr_position,
                            current_header.header().output_mmr_size,
                            current_header.height()
                        );
                    }
                },
                v => {
                    error!(target: LOG_TARGET, "Remote node returned an invalid response {:?}", v);
                    return Err(HorizonSyncError::IncorrectResponse(
                        "Invalid sync utxo returned".to_string(),
                    ));
                },
            }

            sync_peer.set_latency(latency);
            sync_peer.add_sample(last_sync_timer.elapsed());
            if mmr_position % 100 == 0 || mmr_position == self.num_outputs {
                let info = HorizonSyncInfo::new(vec![sync_peer.node_id().clone()], HorizonSyncStatus::Outputs {
                    current: mmr_position,
                    total: self.num_outputs,
                    sync_peer: sync_peer.clone(),
                });
                self.hooks.call_on_progress_horizon_hooks(info);
            }

            self.check_latency(sync_peer.node_id(), &avg_latency)?;

            last_sync_timer = Instant::now();
        }

        if !unpruned_outputs.is_empty() {
            return Err(HorizonSyncError::IncorrectResponse(
                "Sync node sent leftover unpruned outputs".to_string(),
            ));
        }

        if mmr_position != end {
            return Err(HorizonSyncError::IncorrectResponse(
                "Sync node did not send all utxos requested".to_string(),
            ));
        }

        Ok(())
    }

    async fn validate_rangeproofs(&self, mut unpruned_outputs: Vec<TransactionOutput>) -> Result<(), HorizonSyncError> {
        let concurrency = self.config.validation_concurrency;
        let mut chunk_size = unpruned_outputs.len() / concurrency;
        if unpruned_outputs.len() % concurrency > 0 {
            chunk_size += 1;
        }
        // Validate rangeproofs in parallel
        let mut tasks = (0..concurrency)
            .filter_map(|_| {
                if unpruned_outputs.is_empty() {
                    None
                } else {
                    let end = cmp::min(unpruned_outputs.len(), chunk_size);
                    Some(unpruned_outputs.drain(..end).collect::<Vec<_>>())
                }
            })
            .map(|chunk| {
                let prover = self.prover.clone();
                task::spawn_blocking(move || -> Result<(), HorizonSyncError> {
                    let outputs = chunk.iter().collect::<Vec<_>>();
                    batch_verify_range_proofs(&prover, &outputs)?;
                    Ok(())
                })
            })
            .collect::<FuturesUnordered<_>>();

        while let Some(result) = tasks.next().await {
            result??;
        }
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
                *metadata.best_block(),
                header.timestamp(),
            )
            .set_pruned_height(header.height())
            .set_horizon_data(calc_kernel_sum, calc_utxo_sum)
            .commit()
            .await?;

        Ok(())
    }

    fn take_final_bitmap(&mut self) -> Arc<Bitmap> {
        self.full_bitmap
            .take()
            .map(Arc::new)
            .expect("take_full_bitmap called before initialize")
    }

    fn full_bitmap_mut(&mut self) -> &mut Bitmap {
        self.full_bitmap
            .as_mut()
            .expect("full_bitmap_mut called before initialize")
    }

    /// (UTXO sum, Kernel sum)
    async fn calculate_commitment_sums(
        &mut self,
        header: &ChainHeader,
    ) -> Result<(Commitment, Commitment, Commitment), HorizonSyncError> {
        let mut utxo_sum = HomomorphicCommitment::default();
        let mut kernel_sum = HomomorphicCommitment::default();
        let mut burned_sum = HomomorphicCommitment::default();

        let mut prev_mmr = 0;
        let mut prev_kernel_mmr = 0;

        let height = header.height();
        let bitmap = self.take_final_bitmap();
        let db = self.db().inner().clone();
        task::spawn_blocking(move || {
            let mut txn = DbTransaction::new();
            let mut utxo_mmr_position = 0;
            let mut prune_positions = vec![];

            for h in 0..=height {
                let curr_header = db.fetch_chain_header(h)?;
                trace!(
                    target: LOG_TARGET,
                    "Fetching utxos from db: height:{}, header.output_mmr:{}, prev_mmr:{}, end:{}",
                    curr_header.height(),
                    curr_header.header().output_mmr_size,
                    prev_mmr,
                    curr_header.header().output_mmr_size - 1
                );
                let (utxos, _) = db.fetch_utxos_in_block(*curr_header.hash(), None)?;
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
                    curr_header.header().kernel_mmr_size - 1
                );

                trace!(target: LOG_TARGET, "Number of utxos returned: {}", utxos.len());
                let mut pruned_counter = 0;
                for u in utxos {
                    match u {
                        PrunedOutput::NotPruned { output } => {
                            if bitmap.contains(utxo_mmr_position) {
                                debug!(
                                    target: LOG_TARGET,
                                    "Found output that needs pruning at height: {} position: {}", h, utxo_mmr_position
                                );
                                prune_positions.push(utxo_mmr_position);
                                pruned_counter += 1;
                            } else {
                                utxo_sum = &output.commitment + &utxo_sum;
                            }
                        },
                        _ => {
                            pruned_counter += 1;
                        },
                    }
                    utxo_mmr_position += 1;
                }
                if pruned_counter > 0 {
                    trace!(target: LOG_TARGET, "{} pruned output(s)", pruned_counter);
                }
                prev_mmr = curr_header.header().output_mmr_size;

                let kernels = db.fetch_kernels_in_block(*curr_header.hash())?;
                trace!(target: LOG_TARGET, "Number of kernels returned: {}", kernels.len());
                for k in kernels {
                    kernel_sum = &k.excess + &kernel_sum;
                    if k.is_burned() {
                        burned_sum = k.get_burn_commitment()? + &burned_sum;
                    }
                }
                prev_kernel_mmr = curr_header.header().kernel_mmr_size;

                if h % 1000 == 0 {
                    debug!(
                        target: LOG_TARGET,
                        "Final Validation: {:.2}% complete. Height: {}, mmr_position: {}, {} outputs to prune after \
                         sync",
                        (h as f32 / height as f32) * 100.0,
                        h,
                        utxo_mmr_position,
                        prune_positions.len()
                    );
                }
            }

            if !prune_positions.is_empty() {
                debug!(target: LOG_TARGET, "Pruning {} spent outputs", prune_positions.len());
                txn.prune_outputs_at_positions(prune_positions);
                db.write(txn)?;
            }

            Ok((utxo_sum, kernel_sum, burned_sum))
        })
        .await?
    }

    #[inline]
    fn db(&self) -> &AsyncBlockchainDb<B> {
        &self.db
    }
}
