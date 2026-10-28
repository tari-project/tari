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
    collections::{BTreeMap, BTreeSet},
    convert::{TryFrom, TryInto},
    sync::Arc,
    time::{Duration, Instant},
};

use futures::{StreamExt, stream::BoxStream};
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
        async_db::{AsyncBlockchainDb, AsyncDbTransaction},
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

/// Where output sync gets the TXO stream of each tranche from.
trait UtxoTrancheSource {
    fn fetch_tranche(
        &mut self,
        request: SyncUtxosRequest,
    ) -> impl Future<Output = Result<BoxStream<'static, Result<SyncUtxosResponse, RpcError>>, RpcError>> + Send;

    fn last_request_latency(&mut self) -> Option<Duration>;
}

impl UtxoTrancheSource for BaseNodeSyncRpcClient {
    async fn fetch_tranche(
        &mut self,
        request: SyncUtxosRequest,
    ) -> Result<BoxStream<'static, Result<SyncUtxosResponse, RpcError>>, RpcError> {
        Ok(self.sync_utxos(request).await?.boxed())
    }

    fn last_request_latency(&mut self) -> Option<Duration> {
        self.get_last_request_latency()
    }
}

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
    tranche_size: u64,
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
            tranche_size: HORIZON_SYNC_TRANCHE_SIZE,
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
        let mut latency_increases_counter = 0usize;
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
                    self.max_latency = self.max_latency.saturating_add(self.config.max_latency_increase);
                    latency_increases_counter = latency_increases_counter.saturating_add(1);
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
        // Kept so that the terminal error below can name why the peers actually failed. Without it every
        // failure surfaces only as "no more sync peers", which says nothing about the underlying cause.
        let mut last_err = None;
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
                        latency_counter = latency_counter.saturating_add(1);
                    } else {
                        self.remove_sync_peer(&node_id);
                    }
                    last_err = Some(format!("{node_id}: {err}"));
                },
            }
        }

        let last_err = last_err.unwrap_or_else(|| "no sync peers were attempted".to_string());
        if self.sync_peers.is_empty() {
            Err(HorizonSyncError::NoMoreSyncPeers(format!(
                "horizon sync failed against every peer, last error was {last_err}"
            )))
        } else if latency_counter >= self.sync_peers.len() {
            Err(HorizonSyncError::AllSyncPeersExceedLatency)
        } else {
            Err(HorizonSyncError::FailedSyncAllPeers(last_err))
        }
    }

    async fn connect_and_attempt_sync(
        &mut self,
        node_id: &NodeId,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        // Connect. `_conn` is bound for the whole attempt on purpose: `PeerConnection::drop`
        // terminates every RPC client created from a handle once the last handle goes away, so
        // letting it fall out of scope here would tear down `client` mid-sync.
        let (mut client, sync_peer, _conn) = self.connect_sync_peer(node_id).await?;

        // Perform horizon sync
        debug!(target: LOG_TARGET, "Check if pruning is needed");
        self.prune_if_needed().await?;
        // Note: We do not need to rewind kernels if the sync fails due to it being validated when inserted into
        //       the database. Furthermore, these kernels will also be successfully removed when we need to rewind
        //       the blockchain for whatever reason.
        debug!(target: LOG_TARGET, "Synchronizing kernels");
        self.synchronize_kernels(sync_peer.clone(), &mut client, to_header)
            .await?;
        self.sync_outputs_and_finalize(sync_peer, &mut client, to_header).await
    }

    async fn connect_sync_peer(
        &mut self,
        node_id: &NodeId,
    ) -> Result<(BaseNodeSyncRpcClient, SyncPeer, PeerConnection), HorizonSyncError> {
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
            .with_deadline_grace_period(Duration::from_secs(5))
            // Kernels and outputs are small, so let the client buffer many while they are validated
            .with_stream_buffer_size(100)
            .with_max_response_size(self.max_streamed_item_size());

        // Bound RPC negotiation so a stuck negotiation cannot wedge the sync loop.
        let mut client = tokio::time::timeout(
            self.config.rpc_deadline,
            conn.connect_rpc_using_builder::<rpc::BaseNodeSyncRpcClient>(config),
        )
        .await
        .map_err(|_| HorizonSyncError::RpcError(RpcError::ReplyTimeout))?
        .map_err(HorizonSyncError::from_connect_error)?;

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
            conn,
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

    /// Output sync resumes after the checkpoint while the checkpoint block is still on the canonical chain. The
    /// outputs committed up to the checkpoint do not depend on the sync target, so the target may have moved.
    async fn calculate_output_sync_start(&self, sync_to_header: &BlockHeader) -> Result<u64, HorizonSyncError> {
        let Some(checkpoint) = self.db.fetch_horizon_sync_output_checkpoint().await? else {
            debug!(target: LOG_TARGET, "No stored checkpoint, starting output sync from scratch");
            return Ok(1);
        };
        let canonical_hash = self
            .db
            .fetch_header(checkpoint.checkpoint_height)
            .await?
            .map(|header| header.hash());
        if canonical_hash == Some(checkpoint.checkpoint_hash) && sync_to_header.height >= checkpoint.checkpoint_height {
            info!(
                target: LOG_TARGET,
                "Resuming output sync from checkpoint at height {} to target height {}",
                checkpoint.checkpoint_height,
                sync_to_header.height
            );
            return Ok(checkpoint.checkpoint_height.saturating_add(1));
        }
        warn!(
            target: LOG_TARGET,
            "Output sync checkpoint at height {} ({}) is not on the canonical chain or is above target height {}. \
             Discarding checkpoint and restarting output sync from scratch.",
            checkpoint.checkpoint_height,
            checkpoint.checkpoint_hash.to_hex(),
            sync_to_header.height
        );
        self.db
            .write_transaction()
            .clear_horizon_sync_output_checkpoint()
            .commit()
            .await?;
        Ok(1)
    }

    /// The largest kernel or output (each streamed one at a time) a peer can legitimately send. A non-coinbase
    /// output's features, script, covenant and payment id are charged `features_and_scripts_bytes_per_gram` bytes per
    /// gram of block weight, so they cannot exceed `max_block_transaction_weight * features_and_scripts_bytes_per_gram`
    /// bytes. The rest of an output (commitment, range proof, keys, signature, encrypted data) and an entire kernel are
    /// small and fixed-size, and coinbase outputs (which are not weighed) have their script and features extra capped
    /// far below this, so 64 KiB of margin covers them and the protobuf overhead.
    fn max_streamed_item_size(&self) -> usize {
        let max_weighed_bytes = self
            .rules
            .consensus_constants_vec()
            .iter()
            .map(|c| {
                c.max_block_transaction_weight().saturating_mul(
                    c.transaction_weight_params()
                        .params()
                        .features_and_scripts_bytes_per_gram
                        .get(),
                )
            })
            .max()
            .unwrap_or(0);
        usize::try_from(max_weighed_bytes)
            .unwrap_or(usize::MAX)
            .saturating_add(64 * 1024)
    }

    async fn sync_outputs_and_finalize<S: UtxoTrancheSource>(
        &mut self,
        sync_peer: SyncPeer,
        source: &mut S,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        let start_height = self.calculate_output_sync_start(to_header).await?;
        debug!(target: LOG_TARGET, "Synchronizing outputs");
        let result = match self
            .synchronize_outputs(sync_peer.clone(), source, to_header, start_height)
            .await
        {
            Ok(()) => self.finalize_horizon_sync(&sync_peer, to_header).await,
            Err(err) => Err(err),
        };
        let Err(err) = result else {
            return Ok(());
        };

        let resumed_state_failure = start_height > 1 && err.may_be_caused_by_resumed_state();
        if resumed_state_failure {
            // The outputs an earlier attempt committed may be what the peer's data does not fit, so the peer is not
            // blamed and output sync restarts from height 1.
            warn!(
                target: LOG_TARGET,
                "Output sync resumed at height {start_height} failed: {err}. Discarding checkpoint."
            );
            self.db
                .write_transaction()
                .clear_horizon_sync_output_checkpoint()
                .commit()
                .await?;
        }
        // Undo everything above the checkpoint, or everything when there is none
        let clean_up_height = match self.db.fetch_horizon_sync_output_checkpoint().await? {
            Some(checkpoint) => checkpoint.checkpoint_height.saturating_add(1),
            None => 1,
        };
        if let Err(e) = self.clean_up_stale_synced_outputs(clean_up_height).await {
            warn!(target: LOG_TARGET, "Failed to clean up after failed output sync: {}", e);
        }
        debug!(target: LOG_TARGET, "Finished clearing failed output sync");
        if resumed_state_failure {
            return Err(HorizonSyncError::ResumedOutputSyncFailed(err.to_string()));
        }
        Err(err)
    }

    /// Removes the outputs synced into blocks from `start_height` up. If this fails part way, the checkpoint is
    /// cleared so that the next attempt cleans up from height 1.
    async fn clean_up_stale_synced_outputs(&mut self, start_height: u64) -> Result<(), ChainStorageError> {
        let result = self.try_clean_up_stale_synced_outputs(start_height).await;
        if result.is_err() &&
            let Err(e) = self
                .db
                .write_transaction()
                .clear_horizon_sync_output_checkpoint()
                .commit()
                .await
        {
            warn!(target: LOG_TARGET, "Could not clear the output sync checkpoint after a failed clean up: {}", e);
        }
        result
    }

    async fn try_clean_up_stale_synced_outputs(&mut self, start_height: u64) -> Result<(), ChainStorageError> {
        let stop_height = self.db.fetch_last_header().await?.height;

        let mut current_height = start_height;
        let mut txn = self.db.write_transaction();
        let mut reporting_counter = 0usize;
        debug!(target: LOG_TARGET, "Cleaning up failed output sync, {}->{}", current_height, stop_height);
        while let Some(current_header) = self.db.fetch_header(current_height).await? {
            reporting_counter = reporting_counter.saturating_add(1);
            if reporting_counter.is_multiple_of(1000) {
                debug!(target: LOG_TARGET, "Cleaning up failed output sync, progress: {}->{}",current_height, stop_height);
            }

            let outputs = self.db.fetch_outputs_in_block(current_header.hash()).await?;
            let mut state_tree_updates = Vec::new();
            for (count, output) in (1..=outputs.len()).zip(outputs.iter()) {
                txn.prune_output_from_all_dbs(output.hash(), output.commitment.clone(), output.features.output_type);
                if !output.is_burned() {
                    let key_bytes: [u8; 32] = output.commitment.as_bytes().try_into().expect("Malformed commitment");
                    state_tree_updates.push(HorizonStateTreeUpdate {
                        key: FixedHash::from(key_bytes),
                        value: None,
                        replaces_value: false,
                    });
                }
                // Each commit removes its outputs from the tables and the output tree together, so a failed commit
                // leaves both in step.
                if count % 100 == 0 || count == outputs.len() {
                    txn.apply_horizon_state_tree_updates(std::mem::take(&mut state_tree_updates));
                    txn.commit().await?;
                }
            }
            current_height = current_height.saturating_add(1);
        }
        debug!(target: LOG_TARGET, "Finished cleaning up failed output sync, cleaned to height {}", stop_height);
        if start_height > 1 {
            return Ok(());
        }
        // Restore genesis-block outputs that an earlier horizon sync spent: `prune_output_from_all_dbs`
        // purged them from the UTXO LMDB tables when their STXO marker streamed in. A sync from height 1
        // streams their spends again. A clean up from a later height keeps spends committed below it.
        let genesis_block = self.db.fetch_genesis_block();
        let genesis_header_hash = *genesis_block.hash();
        let genesis_timestamp = genesis_block.block().header.timestamp.as_u64();
        let mut state_tree_updates = Vec::new();
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
            let key_bytes: [u8; 32] = output.commitment.as_bytes().try_into().expect("Malformed commitment");
            // A rolled-back tranche may have left a value for this key, which the restore replaces.
            state_tree_updates.push(HorizonStateTreeUpdate {
                key: FixedHash::from(key_bytes),
                value: Some(output.smt_hash(0)),
                replaces_value: true,
            });
            // first make sure its deleted everywhere so we can cleanly reinsert it.
            debug!(target: LOG_TARGET, "Restoring genesis output {} with commitment({}) from pruned state", output.hash(), output.commitment.to_hex());

            txn.prune_output_from_all_dbs(output.hash(), output.commitment.clone(), output.features.output_type);
            txn.insert_output_via_horizon_sync(output.clone(), genesis_header_hash, 0, genesis_timestamp);
        }
        if !state_tree_updates.is_empty() {
            txn.apply_horizon_state_tree_updates(state_tree_updates);
            txn.commit().await?;
        }
        debug!(target: LOG_TARGET, "Genesis outputs restored");
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
                    mmr_position.saturating_add(1),
                    end,
                    end.saturating_sub(mmr_position.saturating_add(1))
                );
                if mmr_position < end.saturating_sub(1) {
                    current_header = db.fetch_chain_header(current_header.height().saturating_add(1)).await?;
                }
            }
            mmr_position = mmr_position.saturating_add(1);

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

    /// Validates a streamed output and queues it in `txn`, returning whether it was queued there. An output that
    /// mines a commitment spent earlier in this tranche is held in `recreated_outputs` instead, because the spent
    /// instance is only pruned when the tranche is applied.
    fn queue_horizon_output(
        &self,
        output: TransactionOutput,
        current_header: &BlockHeader,
        state_tree_updates: &mut BTreeMap<FixedHash, Option<FixedHash>>,
        recreated_outputs: &mut Vec<(TransactionOutput, FixedHash, u64, u64)>,
        txn: &mut AsyncDbTransaction<'_, B>,
    ) -> Result<bool, HorizonSyncError> {
        let key_bytes: [u8; 32] = output
            .commitment
            .as_bytes()
            .try_into()
            .map_err(|e| HorizonSyncError::IncorrectResponse(format!("Peer sent malformed commitment: {}", e)))?;
        let key = FixedHash::from(key_bytes);

        let spent_in_tranche = match state_tree_updates.get(&key) {
            Some(Some(_)) => {
                return Err(HorizonSyncError::IncorrectResponse(
                    "Peer sent duplicate output commitment during horizon sync".into(),
                ));
            },
            Some(None) => true,
            None => false,
        };

        let constants = self.rules.consensus_constants(current_header.height).clone();
        validate_output_version(&constants, &output)?;
        validate_individual_output(&output, &constants, self.rules.network().as_network())?;
        batch_verify_range_proofs(&self.prover, &[&output])?;

        state_tree_updates.insert(key, Some(output.smt_hash(current_header.height)));
        if spent_in_tranche {
            recreated_outputs.push((
                output,
                current_header.hash(),
                current_header.height,
                current_header.timestamp.as_u64(),
            ));
            return Ok(false);
        }
        txn.insert_output_via_horizon_sync(
            output,
            current_header.hash(),
            current_header.height,
            current_header.timestamp.as_u64(),
        );
        Ok(true)
    }

    /// Queues the spend of a streamed commitment. The spent instance is the one mined before this tranche.
    async fn queue_horizon_spend(
        &self,
        commitment_bytes: &[u8],
        tranche_start_height: u64,
        state_tree_updates: &mut BTreeMap<FixedHash, Option<FixedHash>>,
        inputs_to_delete: &mut Vec<TransactionOutput>,
    ) -> Result<(), HorizonSyncError> {
        let commitment = CompressedCommitment::from_canonical_bytes(commitment_bytes)?;
        let key_bytes: [u8; 32] = commitment_bytes
            .try_into()
            .map_err(|e| HorizonSyncError::IncorrectResponse(format!("Peer sent malformed commitment: {}", e)))?;
        let key = FixedHash::from(key_bytes);

        // A commitment can only be spent once per tranche, and never after it was mined in the tranche.
        if state_tree_updates.contains_key(&key) {
            return Err(HorizonSyncError::ChainStorageError(ChainStorageError::UnspendableInput));
        }
        let output_hash = self
            .db()
            .fetch_unspent_output_hash_by_commitment(commitment)
            .await?
            .ok_or_else(|| HorizonSyncError::UnknownSpentOutput("Peer sent unknown commitment hash".into()))?;
        trace!(target: LOG_TARGET, "STXO hash `{output_hash}` received from sync peer");

        let output_info = self
            .db()
            .fetch_outputs(output_hash)
            .await?
            .into_iter()
            .filter(|o| o.mined_height < tranche_start_height)
            .max_by_key(|o| o.mined_height)
            .ok_or_else(|| {
                HorizonSyncError::UnknownSpentOutput("Could not fetch full output for spent commitment".into())
            })?;
        state_tree_updates.insert(key, None);
        inputs_to_delete.push(output_info.output);
        Ok(())
    }

    // Synchronize outputs in independently-verifiable tranches
    #[allow(clippy::too_many_lines)]
    async fn synchronize_outputs<S: UtxoTrancheSource>(
        &mut self,
        mut sync_peer: SyncPeer,
        source: &mut S,
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

        let latency = source.last_request_latency();
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
            let tranche_end_height = cmp::min(tranche_start_height.saturating_add(self.tranche_size), to_header.height);

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
                tranche_end_height.saturating_sub(tranche_start_height).saturating_add(1),
                sync_peer.node_id(),
            );

            let req = SyncUtxosRequest {
                start_header_hash: tranche_start_header.hash().to_vec(),
                end_header_hash: tranche_end_header.hash().to_vec(),
            };
            let mut output_stream = tokio::time::timeout(self.config.rpc_deadline, source.fetch_tranche(req))
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
            let mut recreated_outputs = Vec::new();
            let mut batch_op_counter = 0usize;
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

                // A payload-less response is the stream terminator: it carries the tranche-end header hash
                // with no TXO so completeness is verified explicitly rather than inferred from the last
                // data-bearing message (the final block(s) of a tranche may legitimately stream no TXOs).
                let proto_output = match res.txo {
                    Some(proto_output) => proto_output,
                    None => break,
                };

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

                match proto_output {
                    Txo::Output(output) => {
                        if current_header.height == 0 {
                            continue;
                        }
                        let output = TransactionOutput::try_from(output)
                            .map_err(|e| HorizonSyncError::ConversionError(e.to_string()))?;
                        if !output.is_burned() {
                            utxo_counter = utxo_counter.saturating_add(1);
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
                            if self.queue_horizon_output(
                                output,
                                &current_header,
                                &mut state_tree_updates,
                                &mut recreated_outputs,
                                &mut txn,
                            )? {
                                batch_op_counter = batch_op_counter.saturating_add(1);
                            }
                        }
                    },
                    Txo::Commitment(commitment_bytes) => {
                        stxo_counter = stxo_counter.saturating_add(1);
                        self.queue_horizon_spend(
                            &commitment_bytes,
                            tranche_start_height,
                            &mut state_tree_updates,
                            &mut inputs_to_delete,
                        )
                        .await?;
                    },
                }

                items_processed = items_processed.saturating_add(1);
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
                        utxo_counter.saturating_add(stxo_counter),
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

            apply_tranche_output_updates(state_tree_updates, inputs_to_delete, recreated_outputs, &mut txn);
            // Only checkpoint intermediate tranches for network recovery. The final tranche is committed before the
            // root is verified, so it clears the checkpoint: any later failure restarts output sync from height 1.
            if tranche_end_height < to_header.height {
                txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
                    checkpoint_height: tranche_end_height,
                    checkpoint_hash: tranche_end_header.hash(),
                    sync_target_height: to_header.height,
                    sync_target_hash: to_header.hash(),
                });
            } else {
                txn.clear_horizon_sync_output_checkpoint();
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

            total_utxo_counter = total_utxo_counter.saturating_add(utxo_counter);
            total_stxo_counter = total_stxo_counter.saturating_add(stxo_counter);
            tranche_start_height = tranche_end_height.saturating_add(1);
        }

        match db.verify_horizon_sync_output_root(to_header.output_mr).await {
            Ok(()) => {},
            // The root comparison is the only source of `InvalidOperation` here
            Err(ChainStorageError::InvalidOperation(e)) => return Err(HorizonSyncError::FinalOutputRootMismatch(e)),
            Err(e) => return Err(e.into()),
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
    async fn finalize_horizon_sync(
        &mut self,
        sync_peer: &SyncPeer,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        debug!(target: LOG_TARGET, "Validating horizon state");

        self.hooks.call_on_progress_horizon_hooks(HorizonSyncInfo::new(
            vec![sync_peer.node_id().clone()],
            HorizonSyncStatus::Finalizing,
        ));

        let header = self.db().fetch_chain_header(to_header.height).await?;
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
    // Ristretto point arithmetic on commitments, not integer arithmetic: cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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

/// Queues the tranche's tree updates, then the pruning of spent instances, then the outputs that mine a spent
/// commitment again, so that they all land in one write.
fn apply_tranche_output_updates<B: BlockchainBackend + 'static>(
    state_tree_updates: BTreeMap<FixedHash, Option<FixedHash>>,
    inputs_to_delete: Vec<TransactionOutput>,
    recreated_outputs: Vec<(TransactionOutput, FixedHash, u64, u64)>,
    txn: &mut AsyncDbTransaction<'_, B>,
) {
    let recreated_keys = recreated_outputs
        .iter()
        .filter_map(|(output, ..)| FixedHash::try_from(output.commitment.as_bytes()).ok())
        .collect::<BTreeSet<_>>();
    let tranche_updates = state_tree_updates
        .iter()
        .map(|(key, value)| HorizonStateTreeUpdate {
            key: *key,
            value: *value,
            replaces_value: recreated_keys.contains(key),
        })
        .collect::<Vec<_>>();
    txn.apply_horizon_state_tree_updates(tranche_updates);
    // Spent validator node registrations need no validator node set update: a registration spend never changes
    // the set in full block processing either (it is only allowed once the exit has taken effect). Note that
    // horizon sync does not reconstruct the validator node set at all (pre-existing gap): a pruned node that
    // horizon synced has an empty validator node set.
    for output in inputs_to_delete {
        txn.prune_output_from_all_dbs(output.hash(), output.commitment.clone(), output.features.output_type);
    }
    for (output, header_hash, height, timestamp) in recreated_outputs {
        txn.insert_output_via_horizon_sync(output, header_hash, height, timestamp);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use jmt::{JellyfishMerkleTree, KeyHash, mock::MockTreeStore};
    use primitive_types::U512;
    use tari_common::configuration::Network;
    use tari_common_types::chain_metadata::ChainMetadata;
    use tari_comms::{protocol::rpc::Request, test_utils::mocks::create_connectivity_mock};
    use tari_node_components::blocks::ChainBlock;
    use tari_script::script;
    use tari_transaction_components::{
        consensus::ConsensusConstantsBuilder,
        crypto_factories::CryptoFactories,
        key_manager::KeyManager,
        tari_amount::MicroMinotari,
        test_helpers::{TestParams, create_wallet_output_with_data, schema_to_transaction},
        transaction_components::OutputFeatures,
        txn_schema,
    };

    use super::*;
    use crate::{
        base_node::{chain_metadata_service::PeerChainMetadata, sync::rpc::SyncUtxosTask},
        chain_storage::{BlockchainDatabase, DbTransaction, HorizonSyncOutputCheckpoint, SmtHasher},
        test_helpers::{
            blockchain::{
                TempDatabase,
                TestBlockchain,
                create_custom_blockchain,
                create_main_chain,
                create_new_blockchain,
            },
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

        let mut peers: Vec<SyncPeer> = vec![];
        let sync = build_synchronizer(db, &mut peers, target.height);

        let start = sync.calculate_output_sync_start(&target).await.unwrap();
        assert_eq!(start, 1);
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

        let start = sync.calculate_output_sync_start(&target).await.unwrap();
        assert_eq!(start, 3);
    }

    #[tokio::test]
    async fn calculate_start_discards_checkpoint_after_reorg() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["1->GB"], ["2->1"], ["3->2"]));
        let target = chain.get("3").unwrap().header().clone();

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

        let start = sync.calculate_output_sync_start(&target).await.unwrap();
        assert_eq!(start, 1);
        assert!(
            db_clone.fetch_horizon_sync_output_checkpoint().unwrap().is_none(),
            "stale checkpoint must be cleared"
        );
    }

    #[tokio::test]
    async fn calculate_start_resumes_to_a_moved_target() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["1->GB"], ["2->1"], ["3->2"]));
        let checkpoint_block = chain.get("2").unwrap();
        let mut txn = DbTransaction::new();
        txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: 2,
            checkpoint_hash: *checkpoint_block.hash(),
            sync_target_height: 2,
            sync_target_hash: *checkpoint_block.hash(),
        });
        db.write(txn).unwrap();

        let mut peers: Vec<SyncPeer> = vec![];
        let sync = build_synchronizer(db, &mut peers, 3);

        // Only the target height is read, so the far target need not exist
        let mut target = chain.get("3").unwrap().header().clone();
        assert_eq!(sync.calculate_output_sync_start(&target).await.unwrap(), 3);
        target.height = 60_000;
        assert_eq!(sync.calculate_output_sync_start(&target).await.unwrap(), 3);
        target.height = 1;
        assert_eq!(sync.calculate_output_sync_start(&target).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn calculate_start_discards_checkpoint_replaced_by_reorg() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["1->GB"], ["2->1"], ["3->2"]));
        let target = chain.get("3").unwrap().header().clone();

        // The checkpoint height is still on the chain, but with another block
        let mut txn = DbTransaction::new();
        txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: 2,
            checkpoint_hash: *chain.get("1").unwrap().hash(),
            sync_target_height: target.height,
            sync_target_hash: target.hash(),
        });
        db.write(txn).unwrap();

        let db_clone = db.clone();
        let mut peers: Vec<SyncPeer> = vec![];
        let sync = build_synchronizer(db, &mut peers, target.height);

        assert_eq!(sync.calculate_output_sync_start(&target).await.unwrap(), 1);
        assert!(db_clone.fetch_horizon_sync_output_checkpoint().unwrap().is_none());
    }

    #[tokio::test]
    async fn failed_clean_up_commit_errors_and_clears_checkpoint() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["1->GB"], ["2->1"], ["3->2"]));
        let output = db.fetch_outputs_in_block(*chain.get("2").unwrap().hash()).unwrap()[0].clone();

        // Removing only the commitment index entry of a block 2 output makes the commit that prunes it fail
        let mut txn = DbTransaction::new();
        txn.prune_output_from_all_dbs(
            FixedHash::zero(),
            output.commitment.clone(),
            output.features.output_type,
        );
        txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: 1,
            checkpoint_hash: *chain.get("1").unwrap().hash(),
            sync_target_height: 3,
            sync_target_hash: *chain.get("3").unwrap().hash(),
        });
        db.write(txn).unwrap();

        let db_clone = db.clone();
        let mut peers: Vec<SyncPeer> = vec![];
        let mut sync = build_synchronizer(db, &mut peers, 3);

        assert!(sync.clean_up_stale_synced_outputs(2).await.is_err());
        assert!(db_clone.fetch_horizon_sync_output_checkpoint().unwrap().is_none());
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
            !db.fetch_outputs(output_hash).unwrap().is_empty(),
            "output must exist before cleanup"
        );

        let db_clone = db.clone();
        let mut peers: Vec<SyncPeer> = vec![];
        let mut sync = build_synchronizer(db, &mut peers, 3);

        sync.clean_up_stale_synced_outputs(1).await.unwrap();

        assert!(
            db_clone.fetch_outputs(output_hash).unwrap().is_empty(),
            "output above start_height must be pruned"
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn duplicate_output_horizon_spend_then_remine_preserves_new_instance() {
        let (chain, ..) = crate::validation::block_body::test::duplicate_output_chain();
        let first = chain.get_block_and_smt_by_name("2").unwrap();
        let spent = chain.get_block_and_smt_by_name("3").unwrap();
        let remined = chain.get_block_and_smt_by_name("4").unwrap();
        let hash = spent.block().body.inputs().first().unwrap().output_hash();
        let output = chain.db().fetch_outputs(hash).unwrap().pop().unwrap().output;
        // Reconstruct the receiving node's output state at the end of the preceding tranche.
        let mut reset = DbTransaction::new();
        // Horizon streams omit outputs created and spent within the same tranche.
        reset.prune_outputs_spent_at_hash(*remined.hash());
        reset.prune_output_from_all_dbs(hash, output.commitment.clone(), output.features.output_type);
        reset.insert_utxo(
            output.clone(),
            *first.hash(),
            first.height(),
            first.header().timestamp.as_u64(),
        );
        reset.apply_horizon_state_tree_updates(vec![HorizonStateTreeUpdate {
            key: FixedHash::try_from(output.commitment.as_bytes()).unwrap(),
            value: Some(output.smt_hash(first.height())),
            replaces_value: true,
        }]);
        reset.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: first.height(),
            checkpoint_hash: *first.hash(),
            sync_target_height: remined.height(),
            sync_target_hash: *remined.hash(),
        });
        chain.db().write(reset).unwrap();
        let mut other_outputs = Vec::new();
        for header in [spent.header(), remined.header()] {
            for other in chain.db().fetch_outputs_in_block(header.hash()).unwrap() {
                other_outputs.push((other, header.clone()));
            }
        }
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(chain.db().clone(), &mut peers, remined.height());
        let db = sync.db().clone();
        let mut txn = db.write_transaction();
        let mut updates = BTreeMap::new();
        let mut deletes = Vec::new();
        let mut recreated = Vec::new();
        sync.queue_horizon_spend(output.commitment.as_bytes(), spent.height(), &mut updates, &mut deletes)
            .await
            .unwrap();
        assert_eq!(deletes.len(), 1);
        assert!(matches!(
            sync.queue_horizon_spend(output.commitment.as_bytes(), spent.height(), &mut updates, &mut deletes)
                .await,
            Err(HorizonSyncError::ChainStorageError(ChainStorageError::UnspendableInput))
        ));
        sync.queue_horizon_output(output.clone(), remined.header(), &mut updates, &mut recreated, &mut txn)
            .unwrap();
        assert_eq!(deletes.len(), 1);
        assert_eq!(recreated.len(), 1);
        assert!(matches!(
            sync.queue_horizon_output(output.clone(), remined.header(), &mut updates, &mut recreated, &mut txn),
            Err(HorizonSyncError::IncorrectResponse(_))
        ));
        assert!(matches!(
            sync.queue_horizon_spend(output.commitment.as_bytes(), spent.height(), &mut updates, &mut deletes)
                .await,
            Err(HorizonSyncError::ChainStorageError(ChainStorageError::UnspendableInput))
        ));
        // Force the same intermediate commit as a full batch, then lose all tranche-local state.
        txn.commit().await.unwrap();
        drop(txn);
        drop(updates);
        drop(deletes);
        drop(recreated);
        let before_cleanup = chain.db().fetch_outputs(hash).unwrap();
        assert_eq!(before_cleanup.len(), 1);
        assert_eq!(before_cleanup[0].mined_height, first.height());
        sync.clean_up_stale_synced_outputs(spent.height()).await.unwrap();
        let surviving = chain.db().fetch_outputs(hash).unwrap();
        assert_eq!(surviving.len(), 1);
        assert_eq!(surviving[0].mined_height, first.height());
        assert_eq!(
            chain
                .db()
                .fetch_unspent_output_hash_by_commitment(output.commitment.clone())
                .unwrap(),
            Some(hash)
        );
        let resume_height = sync.calculate_output_sync_start(remined.header()).await.unwrap();
        assert_eq!(resume_height, spent.height());
        let mut txn = db.write_transaction();
        let mut updates = BTreeMap::new();
        let mut deletes = Vec::new();
        let mut recreated = Vec::new();
        sync.queue_horizon_spend(output.commitment.as_bytes(), resume_height, &mut updates, &mut deletes)
            .await
            .unwrap();
        assert!(
            !sync
                .queue_horizon_output(output.clone(), remined.header(), &mut updates, &mut recreated, &mut txn)
                .unwrap()
        );
        // Replay the remaining outputs removed by cleanup, as the resumed stream would.
        for (other, header) in other_outputs {
            assert!(
                sync.queue_horizon_output(other, &header, &mut updates, &mut recreated, &mut txn)
                    .unwrap()
            );
        }
        apply_tranche_output_updates(updates, deletes, recreated, &mut txn);
        txn.commit().await.unwrap();
        chain
            .db()
            .verify_horizon_sync_output_root(remined.header().output_mr)
            .unwrap();
        let surviving = chain.db().fetch_outputs(hash).unwrap();
        assert_eq!(surviving.len(), 1);
        assert_eq!(surviving.first().unwrap().mined_height, remined.height());
        assert_eq!(
            chain
                .db()
                .fetch_unspent_output_hash_by_commitment(output.commitment)
                .unwrap(),
            Some(hash)
        );
        // The old input survives output-only pruning and must not pair with the re-mined instance.
        assert!(
            !chain
                .db()
                .fetch_outputs_in_block_with_spend_state(*remined.hash(), Some(*remined.hash()))
                .unwrap()
                .into_iter()
                .find(|(o, _)| o.hash() == hash)
                .unwrap()
                .1
        );
        let info = chain.db().fetch_mined_info_by_output_hash(hash).unwrap();
        assert_eq!(info.len(), 2);
        assert!(info.first().unwrap().output.is_none());
        assert_eq!(
            info.first().unwrap().input.as_ref().unwrap().spent_height,
            spent.height()
        );
        assert!(info.last().unwrap().input.is_none());
        assert_eq!(
            info.last().unwrap().output.as_ref().unwrap().mined_height,
            remined.height()
        );
    }

    /// Streams `start..=end` from `source` and applies it to `sync`'s database as one tranche.
    async fn sync_tranche_from(
        sync: &HorizonStateSynchronization<'_, TempDatabase>,
        source: &BlockchainDatabase<TempDatabase>,
        start: &BlockHeader,
        end: &BlockHeader,
    ) {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1000);
        SyncUtxosTask::new(source.clone().into(), Arc::new(NodeId::new()))
            .run(
                Request::new(0.into(), SyncUtxosRequest {
                    start_header_hash: start.hash().to_vec(),
                    end_header_hash: end.hash().to_vec(),
                }),
                tx,
            )
            .await
            .unwrap();
        let mut txn = sync.db().write_transaction();
        let mut updates = BTreeMap::new();
        let mut deletes = Vec::new();
        let mut recreated = Vec::new();
        while let Some(response) = rx.recv().await {
            let response = response.unwrap();
            let header = sync
                .db()
                .fetch_header_by_block_hash(FixedHash::try_from(response.mined_header).unwrap())
                .await
                .unwrap()
                .unwrap();
            match response.txo {
                Some(Txo::Output(output)) => {
                    let output = TransactionOutput::try_from(output).unwrap();
                    if !output.is_burned() {
                        sync.queue_horizon_output(output, &header, &mut updates, &mut recreated, &mut txn)
                            .unwrap();
                    }
                },
                Some(Txo::Commitment(commitment)) => {
                    sync.queue_horizon_spend(&commitment, start.height, &mut updates, &mut deletes)
                        .await
                        .unwrap();
                },
                None => break,
            }
        }
        apply_tranche_output_updates(updates, deletes, recreated, &mut txn);
        txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: end.height,
            checkpoint_hash: end.hash(),
            sync_target_height: end.height,
            sync_target_hash: end.hash(),
        });
        txn.commit().await.unwrap();
    }

    #[tokio::test]
    async fn duplicate_output_horizon_sync_across_tranches() {
        let (chain, ..) = crate::validation::block_body::test::duplicate_output_chain();
        let mined = chain.get_block_and_smt_by_name("2").unwrap();
        let spent = chain.get_block_and_smt_by_name("3").unwrap();
        let remined = chain.get_block_and_smt_by_name("4").unwrap();
        let hash = spent.block().body.inputs()[0].output_hash();

        // The receiving node has every header but no outputs past genesis.
        let db = create_custom_blockchain(chain.rules().clone());
        let mut txn = DbTransaction::new();
        for name in ["A", "1", "2", "3", "4"] {
            txn.insert_chain_header(chain.get_block_and_smt_by_name(name).unwrap().to_chain_header());
        }
        db.write(txn).unwrap();
        let mut peers = Vec::new();
        let sync = build_synchronizer(db.clone(), &mut peers, remined.height());

        // Tranche A ends with the first instance; tranche B spends it and mines it again.
        let tranche_a_start = chain.get_block_and_smt_by_name("A").unwrap();
        sync_tranche_from(&sync, chain.db(), tranche_a_start.header(), mined.header()).await;
        db.verify_horizon_sync_output_root(mined.header().output_mr).unwrap();
        sync_tranche_from(&sync, chain.db(), spent.header(), remined.header()).await;
        db.verify_horizon_sync_output_root(remined.header().output_mr).unwrap();

        let outputs = db.fetch_outputs(hash).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].mined_height, remined.height());
        assert_eq!(
            db.fetch_unspent_output_hash_by_commitment(outputs[0].output.commitment.clone())
                .unwrap(),
            Some(hash)
        );

        // Only a key marked as re-created may replace an existing value.
        let mut txn = DbTransaction::new();
        txn.apply_horizon_state_tree_updates(vec![HorizonStateTreeUpdate {
            key: FixedHash::try_from(outputs[0].output.commitment.as_bytes()).unwrap(),
            value: Some(FixedHash::default()),
            replaces_value: false,
        }]);
        // The output tree reports a duplicate key, which a resumed sync treats as left-over state
        let err = HorizonSyncError::from(db.write(txn).unwrap_err());
        assert!(err.may_be_caused_by_resumed_state(), "{err}");
    }

    type EditTranche = Box<dyn FnMut(usize, &mut Vec<SyncUtxosResponse>) -> Result<(), RpcError> + Send>;

    /// Serves each tranche from a database the way a sync peer does. `edit` is given the 1-based request number and
    /// can change the tranche or fail the request.
    struct DbTrancheSource {
        db: BlockchainDatabase<TempDatabase>,
        requests: usize,
        edit: EditTranche,
    }

    impl DbTrancheSource {
        fn new(db: &BlockchainDatabase<TempDatabase>, edit: EditTranche) -> Self {
            Self {
                db: db.clone(),
                requests: 0,
                edit,
            }
        }

        fn honest(db: &BlockchainDatabase<TempDatabase>) -> Self {
            Self::new(db, Box::new(|_, _| Ok(())))
        }

        /// Serves tranches until request `request`, which fails as if the peer dropped the connection
        fn drops_at(db: &BlockchainDatabase<TempDatabase>, request: usize) -> Self {
            Self::new(
                db,
                Box::new(move |n, _| {
                    if n == request {
                        return Err(RpcError::ClientClosed);
                    }
                    Ok(())
                }),
            )
        }
    }

    impl UtxoTrancheSource for DbTrancheSource {
        async fn fetch_tranche(
            &mut self,
            request: SyncUtxosRequest,
        ) -> Result<BoxStream<'static, Result<SyncUtxosResponse, RpcError>>, RpcError> {
            let (tx, mut rx) = tokio::sync::mpsc::channel(1000);
            SyncUtxosTask::new(self.db.clone().into(), Arc::new(NodeId::new()))
                .run(Request::new(0.into(), request), tx)
                .await
                .unwrap();
            let mut items = Vec::new();
            while let Some(item) = rx.recv().await {
                items.push(item.unwrap());
            }
            self.requests = self.requests.saturating_add(1);
            (self.edit)(self.requests, &mut items)?;
            Ok(futures::stream::iter(items.into_iter().map(Ok)).boxed())
        }

        fn last_request_latency(&mut self) -> Option<Duration> {
            None
        }
    }

    fn test_sync_peer() -> SyncPeer {
        PeerChainMetadata::new(
            NodeId::new(),
            ChainMetadata::new(0, FixedHash::zero(), 0, 0, U512::from(1), 0).unwrap(),
            None,
        )
        .into()
    }

    /// A node with the named headers of `chain` but no outputs past genesis, as when horizon sync starts
    fn headers_only_db(chain: &TestBlockchain, names: &[&'static str]) -> BlockchainDatabase<TempDatabase> {
        let db = create_custom_blockchain(chain.rules().clone());
        let mut txn = DbTransaction::new();
        for name in names {
            txn.insert_chain_header(chain.get_block_and_smt_by_name(name).unwrap().to_chain_header());
        }
        db.write(txn).unwrap();
        db
    }

    fn checkpoint_height(db: &BlockchainDatabase<TempDatabase>) -> Option<u64> {
        db.fetch_horizon_sync_output_checkpoint()
            .unwrap()
            .map(|checkpoint| checkpoint.checkpoint_height)
    }

    /// The duplicate output chain at heights 0-5, the receiving node, and a synchronizer with one block per tranche
    fn duplicate_output_sync_setup() -> (TestBlockchain, BlockchainDatabase<TempDatabase>) {
        let (chain, ..) = crate::validation::block_body::test::duplicate_output_chain();
        let db = headers_only_db(&chain, &["A", "1", "2", "3", "4"]);
        (chain, db)
    }

    /// A peer that leaves out an output spent in a later tranche and then drops the connection leaves a checkpoint
    /// that an honest peer's spends do not fit. The honest peer is not banned and output sync restarts from height 1.
    #[tokio::test]
    async fn poisoned_checkpoint_resume_restarts_without_ban() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let spent_hash = chain.get_block_and_smt_by_name("1").unwrap().block().body.inputs()[0].output_hash();
        let spent = chain.db().fetch_outputs(spent_hash).unwrap().pop().unwrap();
        assert_eq!(spent.mined_height, 1);
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let commitment = spent.output.commitment;
        let mut poisoned = DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, items| {
                if request == 2 {
                    return Err(RpcError::ClientClosed);
                }
                items.retain(|item| match &item.txo {
                    Some(Txo::Output(output)) => {
                        TransactionOutput::try_from(output.clone()).unwrap().commitment != commitment
                    },
                    _ => true,
                });
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut poisoned, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::RpcError(_)), "{err}");
        assert_eq!(checkpoint_height(&db), Some(1));

        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut DbTrancheSource::honest(chain.db()), &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::ResumedOutputSyncFailed(_)), "{err}");
        assert!(err.get_ban_reason().is_none());
        assert_eq!(checkpoint_height(&db), None);

        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 5, "output sync must start from height 1");
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
        assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), target.height);
    }

    /// A peer that plants a later block's output in an early tranche and then drops the connection leaves state
    /// that an honest peer's copy of that output collides with. The honest peer is not banned and output sync
    /// restarts from height 1.
    #[tokio::test]
    async fn planted_output_resume_restarts_without_ban() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let coinbase = chain
            .get_block_and_smt_by_name("4")
            .unwrap()
            .block()
            .body
            .outputs()
            .iter()
            .find(|output| output.is_coinbase())
            .unwrap()
            .clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut planting = DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, items| {
                if request == 2 {
                    return Err(RpcError::ClientClosed);
                }
                let mined_header = items.last().unwrap().mined_header.clone();
                items.insert(0, SyncUtxosResponse {
                    txo: Some(Txo::Output(coinbase.clone().try_into().unwrap())),
                    mined_header,
                });
                Ok(())
            }),
        );
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut planting, &target)
            .await
            .unwrap_err();
        assert_eq!(checkpoint_height(&db), Some(1));

        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut DbTrancheSource::honest(chain.db()), &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::ResumedOutputSyncFailed(_)), "{err}");
        assert!(err.get_ban_reason().is_none());
        assert_eq!(checkpoint_height(&db), None);

        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 5, "output sync must start from height 1");
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }

    /// A malformed item from a peer is banned even when output sync resumed from a checkpoint
    #[tokio::test]
    async fn data_error_on_resumed_sync_is_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut dropped = DbTrancheSource::drops_at(chain.db(), 2);
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut dropped, &target)
            .await
            .unwrap_err();
        assert_eq!(checkpoint_height(&db), Some(1));

        let mut malformed = DbTrancheSource::new(
            chain.db(),
            Box::new(|_, items| {
                let mined_header = items.last().unwrap().mined_header.clone();
                items.insert(0, SyncUtxosResponse {
                    txo: Some(Txo::Commitment(vec![1, 2, 3])),
                    mined_header,
                });
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut malformed, &target)
            .await
            .unwrap_err();
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long, "{err}");
        assert_eq!(checkpoint_height(&db), Some(1));
    }

    /// A sync resumed after the target moved runs to the new target and finalizes there
    #[tokio::test]
    async fn resume_after_target_moved_finishes_at_new_target() {
        let (chain, db) = duplicate_output_sync_setup();
        let old_target = chain.get_block_and_smt_by_name("2").unwrap().header().clone();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, old_target.height);
        sync.tranche_size = 0;

        let mut dropped = DbTrancheSource::drops_at(chain.db(), 2);
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut dropped, &old_target)
            .await
            .unwrap_err();
        assert_eq!(checkpoint_height(&db), Some(1));

        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 4, "output sync must resume after the checkpoint");
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
        assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), target.height);
        assert_eq!(checkpoint_height(&db), None);
    }

    /// A failure after the last tranche committed leaves no checkpoint, so the retry cleans up and syncs from
    /// height 1. A final root mismatch in a sync from height 1 bans the peer.
    #[tokio::test]
    async fn failure_after_last_tranche_restarts_from_height_one() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        // Leaving an unspent output out of the last tranche fails the final root check
        let mut poisoned = DbTrancheSource::new(
            chain.db(),
            Box::new(|request, items| {
                if request == 5 {
                    let index = items
                        .iter()
                        .position(|item| matches!(item.txo, Some(Txo::Output(_))))
                        .unwrap();
                    items.remove(index);
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut poisoned, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::FinalOutputRootMismatch(_)), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long);
        assert_eq!(checkpoint_height(&db), None);

        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 5);
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }

    /// A chain whose genesis block has a spendable output, which block "1" spends
    fn genesis_spend_chain() -> (TestBlockchain, TransactionOutput) {
        let km = KeyManager::new_random().unwrap();
        let genesis_output = create_wallet_output_with_data(
            script!(Nop).unwrap(),
            OutputFeatures::default(),
            &TestParams::new(&km),
            MicroMinotari::from(100_000_000),
            &km,
        )
        .unwrap();
        let (spend, _) = schema_to_transaction(
            &[txn_schema!(from: vec![genesis_output.clone()], to: vec![MicroMinotari::from(50_000_000)])],
            &km,
        );
        let output = genesis_output.to_transaction_output().unwrap();

        let constants = ConsensusConstantsBuilder::new(Network::LocalNet)
            .with_coinbase_lockheight(0)
            .build();
        let genesis = BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(constants.clone())
            .build()
            .unwrap()
            .get_genesis_block();
        let mut block = genesis.block().clone();
        block.body.add_output(output.clone());
        let mock_store = MockTreeStore::new(true);
        let (root, _) = JellyfishMerkleTree::<_, SmtHasher>::new(&mock_store)
            .put_value_set(
                vec![(
                    KeyHash(output.commitment.as_bytes().try_into().unwrap()),
                    Some(output.smt_hash(0).to_vec()),
                )],
                0,
            )
            .unwrap();
        block.header.output_mr = FixedHash::try_from(root.0.as_slice()).unwrap();
        let mut accumulated_data = genesis.accumulated_data().clone();
        accumulated_data.hash = block.header.hash();
        let genesis = ChainBlock::try_construct(Arc::new(block), accumulated_data).unwrap();
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
            .with_block(genesis)
            .add_consensus_constants(constants)
            .build()
            .unwrap();

        let mut chain = TestBlockchain::create(rules);
        chain
            .add_next_tip(block_spec!("1", transactions: spend.iter().map(|t| (**t).clone()).collect()))
            .unwrap();
        chain.add_next_tip(block_spec!("2")).unwrap();
        chain.add_next_tip(block_spec!("3")).unwrap();
        (chain, output)
    }

    /// Cleaning up a failed tranche keeps the spend of a genesis output that an earlier tranche committed
    #[tokio::test]
    async fn clean_up_above_checkpoint_keeps_spent_genesis_output_spent() {
        let (chain, genesis_output) = genesis_spend_chain();
        let target = chain.get_block_and_smt_by_name("3").unwrap().header().clone();
        let db = headers_only_db(&chain, &["1", "2", "3"]);
        assert!(
            db.fetch_unspent_output_hash_by_commitment(genesis_output.commitment.clone())
                .unwrap()
                .is_some()
        );
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut dropped = DbTrancheSource::drops_at(chain.db(), 2);
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut dropped, &target)
            .await
            .unwrap_err();
        assert_eq!(checkpoint_height(&db), Some(1));
        assert!(
            db.fetch_unspent_output_hash_by_commitment(genesis_output.commitment)
                .unwrap()
                .is_none(),
            "the genesis output spent in the committed tranche must stay spent"
        );

        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 2, "output sync must resume after the checkpoint");
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }
}
