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
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use log::*;
use tari_common_types::types::{Commitment, FixedHash, RangeProofService};
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId, protocol::rpc::RpcClient, PeerConnection};
use tari_crypto::commitment::HomomorphicCommitment;
use tari_mmr::sparse_merkle_tree::{DeleteResult, NodeKey, ValueHash};
use tari_utilities::{hex::Hex, ByteArray};
use tokio::task;

use super::error::HorizonSyncError;
use crate::{
    base_node::sync::{
        ban::PeerBanManager,
        hooks::Hooks,
        horizon_state_sync::{HorizonSyncInfo, HorizonSyncStatus},
        rpc,
        rpc::BaseNodeSyncRpcClient,
        BlockchainSyncConfig,
        SyncPeer,
    },
    blocks::{BlockHeader, ChainHeader, UpdateBlockAccumulatedData},
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, ChainStorageError, MmrTree},
    common::{rolling_avg::RollingAverageTime, BanPeriod},
    consensus::ConsensusManager,
    proto::base_node::{sync_utxos_response::Txo, SyncKernelsRequest, SyncUtxosRequest, SyncUtxosResponse},
    transactions::transaction_components::{
        transaction_output::batch_verify_range_proofs,
        OutputType,
        TransactionKernel,
        TransactionOutput,
    },
    validation::{helpers, FinalHorizonStateValidation},
    OutputSmt,
    PrunedKernelMmr,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::horizon_state_sync";

const MAX_LATENCY_INCREASES: usize = 5;

pub struct HorizonStateSynchronization<'a, B> {
    config: BlockchainSyncConfig,
    db: AsyncBlockchainDb<B>,
    rules: ConsensusManager,
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
        rules: ConsensusManager,
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

        let mut latency_increases_counter = 0;
        loop {
            match self.sync(&to_header).await {
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
                        warn!(target: LOG_TARGET, "{}", err);
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
        let sync_peer = &self.sync_peers[peer_index];
        self.hooks.call_on_starting_hook(sync_peer);

        let mut conn = self.dial_sync_peer(node_id).await?;
        debug!(
            target: LOG_TARGET,
            "Attempting to synchronize horizon state with `{}`", node_id
        );

        let config = RpcClient::builder()
            .with_deadline(self.config.rpc_deadline)
            .with_deadline_grace_period(Duration::from_secs(3));

        let mut client = conn
            .connect_rpc_using_builder::<rpc::BaseNodeSyncRpcClient>(config)
            .await?;

        let latency = client
            .get_last_request_latency()
            .expect("unreachable panic: last request latency must be set after connect");
        self.sync_peers[peer_index].set_latency(latency);
        if latency > self.max_latency {
            return Err(HorizonSyncError::MaxLatencyExceeded {
                peer: conn.peer_node_id().clone(),
                latency,
                max_latency: self.max_latency,
            });
        }
        debug!(target: LOG_TARGET, "Sync peer latency is {:.2?}", latency);

        Ok((client, self.sync_peers[peer_index].clone()))
    }

    async fn dial_sync_peer(&self, node_id: &NodeId) -> Result<PeerConnection, HorizonSyncError> {
        let timer = Instant::now();
        debug!(target: LOG_TARGET, "Dialing {} sync peer", node_id);
        let conn = self.connectivity.dial_peer(node_id.clone()).await?;
        info!(
            target: LOG_TARGET,
            "Successfully dialed sync peer {} in {:.2?}",
            node_id,
            timer.elapsed()
        );
        Ok(conn)
    }

    async fn sync_kernels_and_outputs(
        &mut self,
        sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        // Note: We do not need to rewind kernels if the sync fails due to it being validated when inserted into
        //       the database. Furthermore, these kernels will also be successfully removed when we need to rewind
        //       the blockchain for whatever reason.
        debug!(target: LOG_TARGET, "Synchronizing kernels");
        self.synchronize_kernels(sync_peer.clone(), client, to_header).await?;
        debug!(target: LOG_TARGET, "Synchronizing outputs");
        match self.synchronize_outputs(sync_peer, client, to_header).await {
            Ok(_) => Ok(()),
            Err(err) => {
                // We need to clean up the outputs
                let _ = self.clean_up_failed_output_sync(to_header).await;
                Err(err)
            },
        }
    }

    /// We clean up a failed output sync attempt and ignore any errors that occur during the clean up process.
    async fn clean_up_failed_output_sync(&mut self, to_header: &BlockHeader) {
        let tip_header = if let Ok(header) = self.db.fetch_tip_header().await {
            header
        } else {
            return;
        };
        let db = self.db().clone();
        let mut txn = db.write_transaction();
        let mut current_header = to_header.clone();
        loop {
            if let Ok(outputs) = self.db.fetch_outputs_in_block(current_header.hash()).await {
                for (count, output) in (1..=outputs.len()).zip(outputs.iter()) {
                    // Note: We do not need to clean up the SMT as it was not saved in the database yet, however, we
                    // need to clean up the outputs
                    txn.prune_output_from_all_dbs(
                        output.hash(),
                        output.commitment.clone(),
                        output.features.output_type,
                    );
                    if let Err(e) = txn.commit().await {
                        warn!(
                        target: LOG_TARGET,
                        "Clean up failed sync - prune output from all dbs for header '{}': {}",
                        current_header.hash(), e
                        );
                    }
                    if count % 100 == 0 || count == outputs.len() {
                        if let Err(e) = txn.commit().await {
                            warn!(
                                target: LOG_TARGET,
                                "Clean up failed sync - commit prune outputs for header '{}': {}",
                                current_header.hash(), e
                            );
                        }
                    }
                }
            }
            if let Err(e) = txn.commit().await {
                warn!(
                    target: LOG_TARGET, "Clean up failed output sync - commit delete kernels for header '{}': {}",
                    current_header.hash(), e
                );
            }
            if let Ok(header) = db.fetch_header_by_block_hash(current_header.prev_hash).await {
                if let Some(previous_header) = header {
                    current_header = previous_header;
                } else {
                    warn!(target: LOG_TARGET, "Could not clean up failed output sync, previous_header link missing frm db");
                    break;
                }
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Could not clean up failed output sync, header '{}' not in db",
                    current_header.prev_hash.to_hex()
                );
                break;
            }
            if &current_header.hash() == tip_header.hash() {
                debug!(target: LOG_TARGET, "Finished cleaning up failed output sync");
                break;
            }
        }
    }

    async fn prune_if_needed(&mut self) -> Result<(), HorizonSyncError> {
        let local_metadata = self.db.get_chain_metadata().await?;
        let new_prune_height = cmp::min(local_metadata.best_block_height(), self.horizon_sync_height);
        if local_metadata.pruned_height() < new_prune_height {
            debug!(target: LOG_TARGET, "Pruning block chain to height {}", new_prune_height);
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

    // Synchronize outputs, returning true if any keys were deleted from the output SMT.
    #[allow(clippy::too_many_lines)]
    async fn synchronize_outputs(
        &mut self,
        mut sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        info!(target: LOG_TARGET, "Starting output sync from peer {}", sync_peer);
        let db = self.db().clone();
        let tip_header = db.fetch_tip_header().await?;

        // Estimate the number of outputs to be downloaded; this cannot be known exactly until the sync is complete.
        let mut current_header = to_header.clone();
        self.num_outputs = 0;
        loop {
            current_header =
                if let Some(previous_header) = db.fetch_header_by_block_hash(current_header.prev_hash).await? {
                    self.num_outputs += current_header
                        .output_smt_size
                        .saturating_sub(previous_header.output_smt_size);
                    previous_header
                } else {
                    break;
                };
            if &current_header.hash() == tip_header.hash() {
                break;
            }
        }

        let info = HorizonSyncInfo::new(vec![sync_peer.node_id().clone()], HorizonSyncStatus::Outputs {
            current: 0,
            total: self.num_outputs,
            sync_peer: sync_peer.clone(),
        });
        self.hooks.call_on_progress_horizon_hooks(info);

        let latency = client.get_last_request_latency();
        debug!(
            target: LOG_TARGET,
            "Initiating output sync with peer `{}`, requesting ~{} outputs, tip_header height `{}`, \
            last_chain_header height `{}` (latency = {}ms)",
            sync_peer.node_id(),
            self.num_outputs,
            tip_header.height(),
            db.fetch_last_chain_header().await?.height(),
            latency.unwrap_or_default().as_millis(),
        );

        let start_chain_header = db.fetch_chain_header(tip_header.height() + 1).await?;
        let req = SyncUtxosRequest {
            start_header_hash: start_chain_header.hash().to_vec(),
            end_header_hash: to_header.hash().to_vec(),
        };
        let mut output_stream = client.sync_utxos(req).await?;

        let mut txn = db.write_transaction();
        let mut utxo_counter = 0u64;
        let mut stxo_counter = 0u64;
        let timer = Instant::now();
        let mut output_smt = db.fetch_tip_smt().await?;
        let mut last_sync_timer = Instant::now();
        let mut avg_latency = RollingAverageTime::new(20);

        let mut inputs_to_delete = Vec::new();
        while let Some(response) = output_stream.next().await {
            let latency = last_sync_timer.elapsed();
            avg_latency.add_sample(latency);
            let res: SyncUtxosResponse = response?;

            let output_header_hash = FixedHash::try_from(res.mined_header)
                .map_err(|_| HorizonSyncError::IncorrectResponse("Peer sent no mined header".into()))?;
            let current_header = self
                .db()
                .fetch_header_by_block_hash(output_header_hash)
                .await?
                .ok_or_else(|| {
                    HorizonSyncError::IncorrectResponse("Peer sent mined header we do not know of".into())
                })?;

            let proto_output = res
                .txo
                .ok_or_else(|| HorizonSyncError::IncorrectResponse("Peer sent no transaction output data".into()))?;
            match proto_output {
                Txo::Output(output) => {
                    utxo_counter += 1;
                    // Increase the estimate number of outputs to be downloaded (for display purposes only).
                    if utxo_counter >= self.num_outputs {
                        self.num_outputs = utxo_counter + u64::from(current_header.hash() != to_header.hash());
                    }

                    let constants = self.rules.consensus_constants(current_header.height).clone();
                    let output = TransactionOutput::try_from(output).map_err(HorizonSyncError::ConversionError)?;
                    if !output.is_burned() {
                        debug!(
                            target: LOG_TARGET,
                            "UTXO `{}` received from sync peer ({} of {})",
                            output.hash(),
                            utxo_counter,
                            self.num_outputs,
                        );
                        helpers::check_tari_script_byte_size(&output.script, constants.max_script_byte_size())?;

                        batch_verify_range_proofs(&self.prover, &[&output])?;
                        let smt_key = NodeKey::try_from(output.commitment.as_bytes())?;
                        let smt_node = ValueHash::try_from(output.smt_hash(current_header.height).as_slice())?;
                        if let Err(e) = output_smt.insert(smt_key, smt_node) {
                            error!(
                                target: LOG_TARGET,
                                "Output commitment({}) already in SMT",
                                output.commitment.to_hex(),
                            );
                            return Err(e.into());
                        }
                        txn.insert_output_via_horizon_sync(
                            output,
                            current_header.hash(),
                            current_header.height,
                            current_header.timestamp.as_u64(),
                        );

                        // We have checked the range proof, and we have checked that the linked to header exists.
                        txn.commit().await?;
                    }
                },
                Txo::Commitment(commitment_bytes) => {
                    stxo_counter += 1;

                    let commitment = Commitment::from_canonical_bytes(commitment_bytes.as_slice())?;
                    match self
                        .db()
                        .fetch_unspent_output_hash_by_commitment(commitment.clone())
                        .await?
                    {
                        Some(output_hash) => {
                            debug!(
                                target: LOG_TARGET,
                                "STXO hash `{}` received from sync peer ({})",
                                output_hash,
                                stxo_counter,
                            );
                            let smt_key = NodeKey::try_from(commitment_bytes.as_slice())?;
                            match output_smt.delete(&smt_key)? {
                                DeleteResult::Deleted(_value_hash) => {},
                                DeleteResult::KeyNotFound => {
                                    error!(
                                        target: LOG_TARGET,
                                        "Could not find input({}) in SMT",
                                        commitment.to_hex(),
                                    );
                                    return Err(HorizonSyncError::ChainStorageError(
                                        ChainStorageError::UnspendableInput,
                                    ));
                                },
                            };
                            // This will only be committed once the SMT has been verified due to rewind difficulties if
                            // we need to abort the sync
                            inputs_to_delete.push((output_hash, commitment));
                        },
                        None => {
                            return Err(HorizonSyncError::IncorrectResponse(
                                "Peer sent unknown commitment hash".into(),
                            ))
                        },
                    }
                },
            }

            if utxo_counter % 100 == 0 {
                let info = HorizonSyncInfo::new(vec![sync_peer.node_id().clone()], HorizonSyncStatus::Outputs {
                    current: utxo_counter,
                    total: self.num_outputs,
                    sync_peer: sync_peer.clone(),
                });
                self.hooks.call_on_progress_horizon_hooks(info);
            }
            sync_peer.set_latency(latency);
            sync_peer.add_sample(last_sync_timer.elapsed());
            last_sync_timer = Instant::now();
        }
        // The SMT can only be verified after all outputs have been downloaded, due to the way we optimize fetching
        // outputs from the sync peer. As an example:
        // 1. Initial sync:
        //    - We request outputs from height 0 to 100 (the tranche)
        //    - The sync peer only returns outputs per block that would still be unspent at height 100 and all inputs
        //      per block. All outputs that were created and spent within the tranche are never returned.
        //    - For example, an output is created in block 50 and spent in block 70. It would be included in the SMT for
        //      headers from height 50 to 69, but due to the optimization, the sync peer would never know about it.
        // 2. Consecutive sync:
        //    - We request outputs from height 101 to 200 (the tranche)
        //    - The sync peer only returns outputs per block that would still be unspent at height 200, as well as all
        //      inputs per block, but in this case, only those inputs that are not an output of the current tranche of
        //      outputs. Similarly, all outputs created and spent within the tranche are never returned.
        //    - For example, an output is created in block 110 and spent in block 180. It would be included in the SMT
        //      for headers from height 110 to 179, but due to the optimization, the sync peer would never know about
        //      it.
        // 3. In both cases it would be impossible to verify the SMT per block, as we would not be able to update the
        //    SMT with the outputs that were created and spent within the tranche.
        HorizonStateSynchronization::<B>::check_output_smt_root_hash(&mut output_smt, to_header)?;

        // Commit in chunks to avoid locking the database for too long
        let inputs_to_delete_len = inputs_to_delete.len();
        for (count, (output_hash, commitment)) in (1..=inputs_to_delete_len).zip(inputs_to_delete.into_iter()) {
            txn.prune_output_from_all_dbs(output_hash, commitment, OutputType::default());
            if count % 100 == 0 || count == inputs_to_delete_len {
                txn.commit().await?;
            }
        }
        // This has a very low probability of failure
        db.set_tip_smt(output_smt).await?;
        debug!(
            target: LOG_TARGET,
            "Finished syncing TXOs: {} unspent and {} spent downloaded in {:.2?}",
            utxo_counter,
            stxo_counter,
            timer.elapsed()
        );
        Ok(())
    }

    // Helper function to check the output SMT root hash against the expected root hash.
    fn check_output_smt_root_hash(output_smt: &mut OutputSmt, header: &BlockHeader) -> Result<(), HorizonSyncError> {
        let root = FixedHash::try_from(output_smt.hash().as_slice())?;
        if root != header.output_mr {
            warn!(
                target: LOG_TARGET,
                "Target root(#{}) did not match expected (#{})",
                    header.output_mr.to_hex(),
                    root.to_hex(),
            );
            return Err(HorizonSyncError::InvalidMrRoot {
                mr_tree: "UTXO SMT".to_string(),
                at_height: header.height,
                expected_hex: header.output_mr.to_hex(),
                actual_hex: root.to_hex(),
            });
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
                *metadata.best_block_hash(),
                header.timestamp(),
            )
            .set_pruned_height(header.height())
            .set_horizon_data(calc_kernel_sum, calc_utxo_sum)
            .commit()
            .await?;

        Ok(())
    }

    /// (UTXO sum, Kernel sum)
    async fn calculate_commitment_sums(
        &mut self,
        header: &ChainHeader,
    ) -> Result<(Commitment, Commitment, Commitment), HorizonSyncError> {
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
                        utxo_sum = &u.commitment + &utxo_sum;
                    }
                }

                let kernels = db.fetch_kernels_in_block(*curr_header.hash())?;
                trace!(target: LOG_TARGET, "Number of kernels returned: {}", kernels.len());
                for k in kernels {
                    kernel_sum = &k.excess + &kernel_sum;
                    if k.is_burned() {
                        burned_sum = k.get_burn_commitment()? + &burned_sum;
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

            Ok((utxo_sum, kernel_sum, burned_sum))
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
