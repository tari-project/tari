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
use prost::Message;
use tari_common_types::{
    payment_reference::generate_payment_reference,
    types::{CompressedCommitment, FixedHash, RangeProofService},
};
use tari_comms::{
    PeerConnection,
    RefKind,
    connectivity::ConnectivityRequester,
    peer_manager::NodeId,
    protocol::rpc::{RpcClient, RpcError, RpcStatus, RpcStatusCode},
};
use tari_crypto::commitment::HomomorphicCommitment;
use tari_node_components::blocks::{BlockHeader, ChainBlock, ChainHeader};
use tari_transaction_components::{
    BanPeriod,
    transaction_components::{
        OutputType,
        TransactionKernel,
        TransactionOutput,
        transaction_output::batch_verify_range_proofs,
    },
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
    proto::{
        self,
        base_node::{SyncKernelsRequest, SyncUtxosRequest, SyncUtxosResponse, sync_utxos_response::Txo},
    },
    validation::FinalHorizonStateValidation,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::horizon_state_sync";

const MAX_LATENCY_INCREASES: usize = 5;
const HORIZON_SYNC_BATCH_SIZE: usize = 10_000;
const PROGRESS_REPORT_INTERVAL: u64 = 100;
const HORIZON_SYNC_TRANCHE_SIZE: u64 = 5_000;
/// The slowest data rate, in bytes per second, a sync peer may deliver at the initial maximum latency: a slow Tor
/// link
const MIN_SYNC_THROUGHPUT: u64 = 40_000;
/// The time a slow peer may spend reading one transaction's outputs and inputs from disk to stream a tranche. A
/// kernel stands for a transaction of a few outputs and inputs, each costing an LMDB lookup or two on a cold disk.
const SERVER_TIME_PER_KERNEL: Duration = Duration::from_millis(10);
/// How often, and how long apart, the first request to a peer asks again while it still holds an earlier session
const SESSION_RETRY_ATTEMPTS: usize = 3;
const SESSION_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Where horizon sync gets the kernel stream and the TXO stream of each tranche from.
trait HorizonSyncSource {
    fn fetch_tranche(
        &mut self,
        request: SyncUtxosRequest,
    ) -> impl Future<Output = Result<BoxStream<'static, Result<SyncUtxosResponse, RpcError>>, RpcError>> + Send;

    fn fetch_kernels(
        &mut self,
        request: SyncKernelsRequest,
    ) -> impl Future<Output = Result<BoxStream<'static, Result<proto::types::TransactionKernel, RpcError>>, RpcError>> + Send;

    fn last_request_latency(&mut self) -> Option<Duration>;
}

impl HorizonSyncSource for BaseNodeSyncRpcClient {
    async fn fetch_tranche(
        &mut self,
        request: SyncUtxosRequest,
    ) -> Result<BoxStream<'static, Result<SyncUtxosResponse, RpcError>>, RpcError> {
        Ok(self.sync_utxos(request).await?.boxed())
    }

    async fn fetch_kernels(
        &mut self,
        request: SyncKernelsRequest,
    ) -> Result<BoxStream<'static, Result<proto::types::TransactionKernel, RpcError>>, RpcError> {
        Ok(self.sync_kernels(request).await?.boxed())
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
    max_items_per_block: u64,
    server_time_per_kernel: Duration,
    /// Peer wait of the current output attempt, the allowance of the tranches it streamed, and the bytes received
    attempt_waited: Duration,
    attempt_allowance: Duration,
    attempt_bytes: u64,
    /// Peer wait a failed resume charges to its same-peer retry
    carried_wait: Duration,
    /// Set until the first request of an attempt succeeds, if the peer may still hold an earlier session
    session_may_be_busy: bool,
    attempted_peers: Vec<NodeId>,
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
        let max_items_per_block = max_streamed_items_per_block(&rules);
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
            max_items_per_block,
            server_time_per_kernel: SERVER_TIME_PER_KERNEL,
            attempt_waited: Duration::ZERO,
            attempt_allowance: Duration::ZERO,
            attempt_bytes: 0,
            carried_wait: Duration::ZERO,
            session_may_be_busy: false,
            attempted_peers: Vec::new(),
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
                    // A fault in the local database fails every peer the same way
                    if err.is_local_failure() {
                        return Err(err);
                    }
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
                    if matches!(
                        err,
                        HorizonSyncError::MaxLatencyExceeded { .. } | HorizonSyncError::PeerWaitBudgetExceeded { .. }
                    ) {
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
        // The peer may still hold the session of an earlier attempt in this sync
        self.session_may_be_busy = self.attempted_peers.contains(node_id);
        if !self.session_may_be_busy {
            self.attempted_peers.push(node_id.clone());
        }

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
        // An interrupted tranche may have left outputs under any block up to the recorded target. Clean up only finds
        // them while that target is still canonical.
        if self
            .is_canonical(checkpoint.checkpoint_height, checkpoint.checkpoint_hash)
            .await? &&
            self.is_canonical(checkpoint.sync_target_height, checkpoint.sync_target_hash)
                .await? &&
            sync_to_header.height >= checkpoint.checkpoint_height
        {
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
            "Output sync checkpoint at height {} ({}) or its target at height {} ({}) is not on the canonical chain, or \
             the checkpoint is above target height {}. Discarding checkpoint and restarting output sync from scratch.",
            checkpoint.checkpoint_height,
            checkpoint.checkpoint_hash.to_hex(),
            checkpoint.sync_target_height,
            checkpoint.sync_target_hash.to_hex(),
            sync_to_header.height
        );
        self.db
            .write_transaction()
            .clear_horizon_sync_output_checkpoint()
            .commit()
            .await?;
        Ok(1)
    }

    async fn is_canonical(&self, height: u64, hash: FixedHash) -> Result<bool, HorizonSyncError> {
        Ok(self.db.fetch_header(height).await?.map(|header| header.hash()) == Some(hash))
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

    async fn sync_outputs_and_finalize<S: HorizonSyncSource>(
        &mut self,
        sync_peer: SyncPeer,
        source: &mut S,
        to_header: &BlockHeader,
    ) -> Result<(), HorizonSyncError> {
        let start_height = self.calculate_output_sync_start(to_header).await?;
        let err = match self
            .attempt_outputs_and_finalize(sync_peer.clone(), source, to_header, start_height)
            .await
        {
            Ok(()) => return Ok(()),
            Err(err) => err,
        };
        if start_height == 1 || !err.may_be_caused_by_resumed_state() {
            return Err(err);
        }
        // The outputs an earlier attempt committed may be what the peer's data does not fit, so the peer is not
        // blamed and is retried once from height 1
        warn!(
            target: LOG_TARGET,
            "Output sync resumed at height {start_height} failed: {err}. Discarding checkpoint and retrying from height 1."
        );
        self.db
            .write_transaction()
            .clear_horizon_sync_output_checkpoint()
            .commit()
            .await?;
        // The retry is charged what the resume waited beyond the allowance of the tranches it streamed, so the credit
        // for the skipped tranches is not spent twice
        self.carried_wait = self
            .attempt_waited
            .saturating_sub(self.peer_wait_budget(self.attempt_allowance, self.attempt_bytes));
        self.session_may_be_busy = true;
        self.attempt_outputs_and_finalize(sync_peer, source, to_header, 1).await
    }

    async fn attempt_outputs_and_finalize<S: HorizonSyncSource>(
        &mut self,
        sync_peer: SyncPeer,
        source: &mut S,
        to_header: &BlockHeader,
        start_height: u64,
    ) -> Result<(), HorizonSyncError> {
        debug!(target: LOG_TARGET, "Synchronizing outputs");
        let result = match self
            .synchronize_outputs(sync_peer.clone(), source, to_header, start_height)
            .await
        {
            Ok(()) => self.finalize_horizon_sync(&sync_peer, to_header).await,
            Err(err) => Err(err),
        };
        let Err(mut err) = result else {
            return Ok(());
        };
        // The caller retries this from height 1, which resets the output state
        if start_height > 1 && err.may_be_caused_by_resumed_state() {
            return Err(err);
        }
        // A failure honest peers can cause is not banned, unless the peer first kept us waiting for over 90% of its
        // budget
        let budget = self.peer_wait_budget(self.attempt_allowance, self.attempt_bytes);
        if start_height == 1 &&
            err.is_unblamed_peer_failure() &&
            self.attempt_waited.saturating_mul(10) > budget.saturating_mul(9)
        {
            err = HorizonSyncError::SlowPeerFailure(format!(
                "{err}, after waiting {:.2?} of {budget:.2?}",
                self.attempt_waited
            ));
        }
        // Undo everything above the checkpoint, or everything when there is none
        let clean_up = match self.db.fetch_horizon_sync_output_checkpoint().await? {
            Some(checkpoint) => self
                .clean_up_stale_synced_outputs(checkpoint.checkpoint_height.saturating_add(1))
                .await
                .map_err(HorizonSyncError::from),
            None => self.reset_output_state_to_genesis().await,
        };
        if let Err(e) = clean_up {
            warn!(target: LOG_TARGET, "Failed to clean up after failed output sync: {}", e);
        }
        debug!(target: LOG_TARGET, "Finished clearing failed output sync");
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
                txn.prune_output_copy(
                    current_header.hash(),
                    output.hash(),
                    output.commitment.clone(),
                    output.features.output_type,
                );
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
        Ok(())
    }

    /// Clears all output state and restores the genesis outputs in one write. Clean up only finds outputs under
    /// canonical blocks, so this is what guarantees a sync from height 1 starts from the genesis state.
    async fn reset_output_state_to_genesis(&self) -> Result<(), HorizonSyncError> {
        if self.output_state_is_genesis().await? {
            debug!(target: LOG_TARGET, "Output state is already the genesis state");
            return Ok(());
        }
        let genesis_block = self.db.fetch_genesis_block();
        let genesis_header_hash = *genesis_block.hash();
        let genesis_timestamp = genesis_block.block().header.timestamp.as_u64();
        let spent = genesis_spent_output_hashes(&genesis_block);
        // Cleared in its own write. If the restore below does not happen, the empty state is not the genesis state, so
        // the next attempt, which starts from height 1 as there is no checkpoint, resets again.
        self.db
            .write_transaction()
            .clear_horizon_sync_output_state()
            .commit()
            .await?;
        let mut txn = self.db.write_transaction();
        let mut state_tree_updates = Vec::new();
        for output in genesis_block.block().body.outputs() {
            txn.insert_output_via_horizon_sync(output.clone(), genesis_header_hash, 0, genesis_timestamp);
            if output.is_burned() {
                continue;
            }
            if spent.contains(&output.hash()) {
                // Spent by the genesis block itself
                txn.delete_utxo_commitment_index_entry(output.commitment.clone());
                continue;
            }
            let key_bytes: [u8; 32] = output.commitment.as_bytes().try_into().expect("Malformed commitment");
            state_tree_updates.push(HorizonStateTreeUpdate {
                key: FixedHash::from(key_bytes),
                value: Some(output.smt_hash(0)),
                replaces_value: false,
            });
        }
        if !state_tree_updates.is_empty() {
            txn.apply_horizon_state_tree_updates(state_tree_updates);
        }
        txn.commit().await?;

        match self
            .db
            .verify_horizon_sync_output_root(genesis_block.header().output_mr)
            .await
        {
            Ok(()) => Ok(()),
            Err(ChainStorageError::InvalidOperation(e)) => {
                error!(target: LOG_TARGET, "Output state does not match the genesis block after resetting it: {e}");
                Err(HorizonSyncError::LocalOutputStateInconsistent(e))
            },
            Err(e) => Err(e.into()),
        }
    }

    /// Whether the output tables and the output tree hold exactly what inserting the genesis block leaves
    async fn output_state_is_genesis(&self) -> Result<bool, HorizonSyncError> {
        let genesis_block = self.db.fetch_genesis_block();
        match self
            .db
            .verify_horizon_sync_output_root(genesis_block.header().output_mr)
            .await
        {
            Ok(()) => {},
            Err(ChainStorageError::InvalidOperation(_)) => return Ok(false),
            Err(e) => return Err(e.into()),
        }
        let outputs = genesis_block.block().body.outputs();
        let spent = genesis_spent_output_hashes(&genesis_block);
        let unspent = outputs
            .iter()
            .filter(|output| is_unspent_genesis_output(output, &spent))
            .count();
        // The tables `insert_output` writes and the output tree values, by their table names
        let mut tables_checked = 0usize;
        for stat in self.db.get_stats().await?.db_stats() {
            let expected = match stat.name {
                "utxos" | "txos_hash_to_index" | "payref_to_output_index" => outputs.len(),
                "utxo_commitment_index" | "jmt_values_data" => unspent,
                _ => continue,
            };
            if stat.entries != expected {
                return Ok(false);
            }
            tables_checked = tables_checked.saturating_add(1);
        }
        if tables_checked != 5 {
            return Ok(false);
        }
        // With the counts matching, every genesis output being stored as expected leaves no room for anything else
        let genesis_hash = *genesis_block.hash();
        for output in outputs {
            let hash = output.hash();
            let stored = self
                .db
                .fetch_outputs(hash)
                .await?
                .iter()
                .any(|row| row.header_hash == genesis_hash);
            let indexed_hash = if output.is_burned() {
                None
            } else {
                self.db
                    .fetch_unspent_output_hash_by_commitment(output.commitment.clone())
                    .await?
            };
            let expected_hash = is_unspent_genesis_output(output, &spent).then_some(hash);
            let has_payref = self
                .db
                .fetch_mined_info_by_payref(generate_payment_reference(&genesis_hash, &hash))
                .await
                .is_ok();
            if !stored || indexed_hash != expected_hash || !has_payref {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The number of unspent outputs stored, from the commitment index
    async fn stored_unspent_output_count(&self) -> Result<u64, HorizonSyncError> {
        let stats = self.db.get_stats().await?;
        let entries = stats
            .db_stats()
            .iter()
            .find(|stat| stat.name == "utxo_commitment_index")
            .map(|stat| stat.entries)
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "Database",
                field: "name",
                value: "utxo_commitment_index".to_string(),
            })?;
        Ok(u64::try_from(entries).unwrap_or(u64::MAX))
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
    async fn synchronize_kernels<S: HorizonSyncSource>(
        &mut self,
        mut sync_peer: SyncPeer,
        source: &mut S,
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

        let latency = source.last_request_latency();
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
        // The peer may wait the RPC deadline plus its read time for the kernels, and the data's time at the minimum
        // throughput, as in output sync. Only time spent waiting on the peer counts.
        let kernel_allowance =
            self.config
                .rpc_deadline
                .saturating_add(self.server_time_per_kernel.saturating_mul(
                    u32::try_from(remote_num_kernels.saturating_sub(local_num_kernels)).unwrap_or(u32::MAX),
                ));
        let mut refusals = 0;
        let mut refused_wait = Duration::ZERO;
        let (mut kernel_stream, mut peer_wait) = loop {
            let fetch_started = Instant::now();
            let mut stream = match source.fetch_kernels(req.clone()).await {
                Ok(stream) => stream,
                Err(err) => {
                    self.retry_after_session_refusal(err, &mut refusals).await?;
                    continue;
                },
            };
            if !self.session_may_be_busy {
                break (stream, refused_wait.saturating_add(fetch_started.elapsed()));
            }
            // A peer refuses a request for a busy session with the first stream item. Waiting for a refusal is still
            // waiting on the peer.
            match stream.next().await {
                Some(Err(err)) if is_existing_session_refusal(&err) => {
                    refused_wait = refused_wait.saturating_add(fetch_started.elapsed());
                    self.check_peer_wait(
                        sync_peer.node_id(),
                        refused_wait,
                        self.peer_wait_budget(kernel_allowance, 0),
                    )?;
                    self.retry_after_session_refusal(err, &mut refusals).await?
                },
                first => {
                    break (
                        futures::stream::iter(first).chain(stream).boxed(),
                        refused_wait.saturating_add(fetch_started.elapsed()),
                    );
                },
            }
        };
        self.session_may_be_busy = false;
        let mut bytes_received = 0u64;

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
            let kernel = kernel?;
            peer_wait = peer_wait.saturating_add(latency);
            bytes_received = bytes_received.saturating_add(u64::try_from(kernel.encoded_len()).unwrap_or(u64::MAX));
            self.check_peer_wait(
                sync_peer.node_id(),
                peer_wait,
                self.peer_wait_budget(kernel_allowance, bytes_received),
            )?;
            // Positions run from the local kernel count up to, but excluding, the end header's kernel count
            if mmr_position >= end {
                return Err(HorizonSyncError::IncorrectResponse(
                    "Peer sent too many kernels".to_string(),
                ));
            }
            let kernel: TransactionKernel = kernel.try_into().map_err(HorizonSyncError::ConversionError)?;
            kernel.verify_signature()?;

            kernel_hashes.push(kernel.hash());

            txn.insert_kernel_via_horizon_sync(kernel, *current_header.hash(), mmr_position);
            if mmr_position >= current_header.header().kernel_mmr_size {
                return Err(HorizonSyncError::IncorrectResponse(format!(
                    "Header #{} has non-increasing kernel_mmr_size {} at mmr_position {}",
                    current_header.height(),
                    current_header.header().kernel_mmr_size,
                    mmr_position
                )));
            }
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

        peer_wait = peer_wait.saturating_add(last_sync_timer.elapsed());
        self.check_peer_wait(
            sync_peer.node_id(),
            peer_wait,
            self.peer_wait_budget(kernel_allowance, bytes_received),
        )?;
        // A dropped connection also ends the stream without an error
        if mmr_position < end {
            return Err(HorizonSyncError::IncompleteKernelStream(format!(
                "received up to kernel {mmr_position} of {end}"
            )));
        }
        if !kernel_hashes.is_empty() {
            return Err(HorizonSyncError::IncorrectResponse(format!(
                "Header #{} ended with {} uncommitted kernel(s) at mmr_position {}",
                current_header.height(),
                kernel_hashes.len(),
                mmr_position
            )));
        }
        Ok(())
    }

    /// An attempt may wait on its peer for the allowance of each tranche started, plus the time its data needs at the
    /// minimum throughput. Both are relaxed in step with the maximum latency when all sync peers are slow.
    fn peer_wait_budget(&self, tranche_allowance: Duration, bytes_received: u64) -> Duration {
        let initial = self.config.initial_max_sync_latency.as_millis();
        let latency = self.max_latency.as_millis();
        let tranche_millis = tranche_allowance
            .as_millis()
            .saturating_mul(latency)
            .checked_div(initial)
            .unwrap_or(u128::MAX);
        let data_millis = u128::from(bytes_received)
            .saturating_mul(1000)
            .saturating_mul(latency)
            .checked_div(u128::from(MIN_SYNC_THROUGHPUT).saturating_mul(initial))
            .unwrap_or(u128::MAX);
        Duration::from_millis(u64::try_from(tranche_millis.saturating_add(data_millis)).unwrap_or(u64::MAX))
    }

    /// The RPC deadline plus the peer's time to read the tranche's transactions, counted by the kernels the headers
    /// commit to
    async fn tranche_allowance(
        &self,
        tranche_start_height: u64,
        tranche_end_header: &BlockHeader,
    ) -> Result<Duration, HorizonSyncError> {
        let previous = self
            .db
            .fetch_header(tranche_start_height.saturating_sub(1))
            .await?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "Header",
                field: "height",
                value: tranche_start_height.saturating_sub(1).to_string(),
            })?;
        let kernels = tranche_end_header
            .kernel_mmr_size
            .saturating_sub(previous.kernel_mmr_size);
        Ok(self.config.rpc_deadline.saturating_add(
            self.server_time_per_kernel
                .saturating_mul(u32::try_from(kernels).unwrap_or(u32::MAX)),
        ))
    }

    /// The allowance of the tranches from height 1 up to `sync_start_height`
    async fn skipped_tranche_allowance(&self, sync_start_height: u64) -> Result<Duration, HorizonSyncError> {
        let checkpoint_height = sync_start_height.saturating_sub(1);
        let checkpoint_header =
            self.db
                .fetch_header(checkpoint_height)
                .await?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "Header",
                    field: "height",
                    value: checkpoint_height.to_string(),
                })?;
        let tranches = checkpoint_height.div_ceil(self.tranche_size.saturating_add(1));
        // One tranche allowance over all the kernels, and the RPC deadline for each further tranche
        Ok(self.tranche_allowance(1, &checkpoint_header).await?.saturating_add(
            self.config
                .rpc_deadline
                .saturating_mul(u32::try_from(tranches.saturating_sub(1)).unwrap_or(u32::MAX)),
        ))
    }

    /// Checks the output attempt's peer wait against its budget, including a resumed attempt's credit for the tranches
    /// before its checkpoint
    fn check_attempt_wait(&self, peer: &NodeId, precredit: Duration) -> Result<(), HorizonSyncError> {
        self.check_peer_wait(
            peer,
            self.attempt_waited,
            self.peer_wait_budget(self.attempt_allowance.saturating_add(precredit), self.attempt_bytes),
        )
    }

    /// Handles a failed first request to a peer that may still hold an earlier session: waits and lets the caller ask
    /// again a few times, and gives up without blaming the peer. Any other failure is returned as it is.
    async fn retry_after_session_refusal(
        &mut self,
        err: RpcError,
        refusals: &mut usize,
    ) -> Result<(), HorizonSyncError> {
        if !self.session_may_be_busy || !is_existing_session_refusal(&err) {
            return Err(err.into());
        }
        if *refusals >= SESSION_RETRY_ATTEMPTS {
            self.session_may_be_busy = false;
            return Err(HorizonSyncError::SyncPeerUnavailable(err));
        }
        *refusals = refusals.saturating_add(1);
        debug!(target: LOG_TARGET, "Peer still holds an earlier sync session: {err}");
        tokio::time::sleep(SESSION_RETRY_DELAY).await;
        Ok(())
    }

    /// A peer cannot drip-feed an attempt just under the per-item latency limit
    fn check_peer_wait(&self, peer: &NodeId, waited: Duration, budget: Duration) -> Result<(), HorizonSyncError> {
        if waited > budget {
            return Err(HorizonSyncError::PeerWaitBudgetExceeded {
                peer: peer.clone(),
                waited,
                budget,
            });
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
    async fn queue_horizon_output(
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

        // An honest peer can send a commitment we already hold unspent when it pruned that output's spend after
        // streaming the output to us. Checked after validation, so this unbanned exit needs a valid output.
        if !spent_in_tranche &&
            self.db()
                .fetch_unspent_output_hash_by_commitment(output.commitment.clone())
                .await?
                .is_some()
        {
            return Err(HorizonSyncError::OutputAlreadyStored(format!(
                "commitment {} streamed at height {}",
                output.commitment.to_hex(),
                current_header.height
            )));
        }

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
        inputs_to_delete: &mut Vec<(FixedHash, FixedHash, CompressedCommitment, OutputType)>,
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
        // Only what pruning this copy of the output needs is kept until the tranche is applied
        inputs_to_delete.push((
            output_info.header_hash,
            output_hash,
            output_info.output.commitment,
            output_info.output.features.output_type,
        ));
        Ok(())
    }

    // Synchronize outputs in independently-verifiable tranches
    #[allow(clippy::too_many_lines)]
    async fn synchronize_outputs<S: HorizonSyncSource>(
        &mut self,
        mut sync_peer: SyncPeer,
        source: &mut S,
        to_header: &BlockHeader,
        sync_start_height: u64,
    ) -> Result<(), HorizonSyncError> {
        info!(target: LOG_TARGET, "Starting output sync from peer {sync_peer}");
        // Only time spent waiting on the peer counts against the attempt's budget, not local validation and writes
        self.attempt_waited = std::mem::take(&mut self.carried_wait);
        self.attempt_allowance = Duration::ZERO;
        self.attempt_bytes = 0;
        let db = self.db().clone();
        if sync_start_height == 1 {
            self.reset_output_state_to_genesis().await.map_err(|e| match e {
                HorizonSyncError::LocalOutputStateInconsistent(_) => e,
                e => HorizonSyncError::LocalOutputStateInconsistent(format!("resetting the output state failed: {e}")),
            })?;
        } else {
            self.clean_up_stale_synced_outputs(sync_start_height)
                .await
                .map_err(|e| {
                    HorizonSyncError::LocalOutputStateInconsistent(format!("cleaning up synced outputs failed: {e}"))
                })?;
            // Outputs this attempt commits before its next checkpoint sit under blocks up to this target
            if let Some(mut checkpoint) = db.fetch_horizon_sync_output_checkpoint().await? &&
                checkpoint.sync_target_hash != to_header.hash()
            {
                checkpoint.sync_target_height = to_header.height;
                checkpoint.sync_target_hash = to_header.hash();
                db.write_transaction()
                    .set_horizon_sync_output_checkpoint(checkpoint)
                    .commit()
                    .await?;
            }
        }

        // No tranche is left when the checkpoint is at the target, but the committed state is still verified below
        if sync_start_height > to_header.height {
            info!(
                target: LOG_TARGET,
                "Output sync already complete (sync_start_height {} > horizon height {})",
                sync_start_height,
                to_header.height
            );
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

        // A pruned peer also streams the spends of pruned genesis outputs at height 1
        let genesis_output_count =
            u64::try_from(db.fetch_genesis_block().block().body.outputs().len()).unwrap_or(u64::MAX);
        let timer = Instant::now();
        let mut total_utxo_counter = 0u64;
        let mut total_stxo_counter = 0u64;
        let mut tranche_start_height = sync_start_height;
        // A resumed attempt starts with the allowance the tranches before its checkpoint earned, as an attempt from
        // height 1 would have at this point
        let precredit = if sync_start_height > 1 {
            self.skipped_tranche_allowance(sync_start_height).await?
        } else {
            Duration::ZERO
        };

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
            self.attempt_allowance = self.attempt_allowance.saturating_add(
                self.tranche_allowance(tranche_start_height, &tranche_end_header)
                    .await?,
            );
            let mut refusals = 0;
            let mut output_stream = loop {
                let fetch_started = Instant::now();
                let result = tokio::time::timeout(self.config.rpc_deadline, source.fetch_tranche(req.clone()))
                    .await
                    .map_err(|_| {
                        HorizonSyncError::RpcStatus(RpcStatus::general(&format!(
                            "Timed out waiting for sync_utxos stream from peer {}",
                            sync_peer.node_id()
                        )))
                    })?;
                let mut stream = match result {
                    Ok(stream) => stream,
                    Err(err) => {
                        self.retry_after_session_refusal(err, &mut refusals).await?;
                        continue;
                    },
                };
                if !self.session_may_be_busy {
                    self.attempt_waited = self.attempt_waited.saturating_add(fetch_started.elapsed());
                    break stream;
                }
                // A peer refuses a request for a busy session with the first stream item. Waiting for a refusal is
                // still waiting on the peer.
                match stream.next().await {
                    Some(Err(err)) if is_existing_session_refusal(&err) => {
                        self.attempt_waited = self.attempt_waited.saturating_add(fetch_started.elapsed());
                        self.check_attempt_wait(sync_peer.node_id(), precredit)?;
                        self.retry_after_session_refusal(err, &mut refusals).await?
                    },
                    first => {
                        self.attempt_waited = self.attempt_waited.saturating_add(fetch_started.elapsed());
                        break futures::stream::iter(first).chain(stream).boxed();
                    },
                }
            };
            self.session_may_be_busy = false;
            self.check_attempt_wait(sync_peer.node_id(), precredit)?;

            let mut txn = db.write_transaction();
            let mut utxo_counter = 0u64;
            let mut stxo_counter = 0u64;
            let mut items_processed = 0u64;
            let mut avg_latency = RollingAverageTime::new(20);

            // Accumulate SMT updates for the current tranche only. These are not applied until the full tranche
            // stream has been received and the SMT root verified.
            let mut state_tree_updates = BTreeMap::<FixedHash, Option<FixedHash>>::new();
            let mut inputs_to_delete = Vec::new();
            let mut recreated_outputs = Vec::new();
            let mut batch_op_counter = 0usize;
            let mut saw_terminator = false;
            let mut items_received = 0u64;
            let item_budget = self
                .max_items_per_block
                .saturating_mul(
                    tranche_end_height
                        .saturating_sub(tranche_start_height)
                        .saturating_add(1),
                )
                .saturating_add(1)
                .saturating_add(if tranche_start_height == 1 {
                    genesis_output_count
                } else {
                    0
                });
            // An honest tranche holds no more outputs than the output tree has leaves at its end block, and no more
            // spends than the unspent outputs stored before it. Outputs that mine a commitment spent earlier in the
            // tranche count as outputs too, so the outputs held per tranche are bounded by the end block's leaf count,
            // and an attempt validates at most the sum of those over its tranches.
            let max_outputs = tranche_end_header.output_smt_size;
            let max_spends = self
                .stored_unspent_output_count()
                .await?
                .saturating_add(if tranche_start_height == 1 {
                    genesis_output_count
                } else {
                    0
                });
            let mut outputs_received = 0u64;
            let mut spends_received = 0u64;
            let mut previous_height = tranche_start_height;
            let mut current_header_hash = tranche_start_header.hash();
            let mut current_header = self
                .db()
                .fetch_header_by_block_hash(current_header_hash)
                .await?
                .ok_or_else(|| HorizonSyncError::IncorrectResponse("Starting header missing".into()))?;
            // Started after the local reads above, so only waiting on the peer is timed
            let mut last_sync_timer = Instant::now();
            while let Some(response) = output_stream.next().await {
                let latency = last_sync_timer.elapsed();
                avg_latency.add_sample(latency);
                let res: SyncUtxosResponse = response?;
                self.attempt_waited = self.attempt_waited.saturating_add(latency);
                self.attempt_bytes = self
                    .attempt_bytes
                    .saturating_add(u64::try_from(res.encoded_len()).unwrap_or(u64::MAX));
                self.check_attempt_wait(sync_peer.node_id(), precredit)?;
                items_received = items_received.saturating_add(1);
                if items_received > item_budget {
                    return Err(HorizonSyncError::IncorrectResponse(format!(
                        "Peer sent more than {item_budget} items for tranche \
                         {tranche_start_height}-{tranche_end_height}"
                    )));
                }

                let output_header_hash = FixedHash::try_from(res.mined_header).map_err(|e| {
                    HorizonSyncError::IncorrectResponse(format!("Peer sent invalid mined header: {}", e))
                })?;

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
                // Blocks are streamed in ascending order and only from within the requested tranche
                if current_header.height < previous_height || current_header.height > tranche_end_height {
                    return Err(HorizonSyncError::IncorrectResponse(format!(
                        "Peer sent an item mined at height {} after height {}, in tranche {}-{}",
                        current_header.height, previous_height, tranche_start_height, tranche_end_height
                    )));
                }
                previous_height = current_header.height;

                // A payload-less response is the stream terminator: it carries the tranche-end header hash
                // with no TXO so completeness is verified explicitly rather than inferred from the last
                // data-bearing message (the final block(s) of a tranche may legitimately stream no TXOs).
                let proto_output = match res.txo {
                    Some(proto_output) => proto_output,
                    None => {
                        if output_header_hash != tranche_end_header.hash() {
                            return Err(HorizonSyncError::IncorrectResponse(format!(
                                "Peer ended the output stream for tranche {}-{} at height {}",
                                tranche_start_height, tranche_end_height, current_header.height
                            )));
                        }
                        saw_terminator = true;
                        break;
                    },
                };

                match proto_output {
                    Txo::Output(output) => {
                        outputs_received = outputs_received.saturating_add(1);
                        if outputs_received > max_outputs {
                            return Err(HorizonSyncError::IncorrectResponse(format!(
                                "Peer sent more than {max_outputs} outputs for tranche \
                                 {tranche_start_height}-{tranche_end_height}"
                            )));
                        }
                        let output = TransactionOutput::try_from(output)
                            .map_err(|e| HorizonSyncError::ConversionError(e.to_string()))?;
                        // Burned outputs are never streamed
                        if output.is_burned() {
                            return Err(HorizonSyncError::IncorrectResponse(
                                "Peer sent a burned output".to_string(),
                            ));
                        }
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
                        if self
                            .queue_horizon_output(
                                output,
                                &current_header,
                                &mut state_tree_updates,
                                &mut recreated_outputs,
                                &mut txn,
                            )
                            .await?
                        {
                            batch_op_counter = batch_op_counter.saturating_add(1);
                        }
                    },
                    Txo::Commitment(commitment_bytes) => {
                        spends_received = spends_received.saturating_add(1);
                        // Each spend must match a stored output, which a resumed attempt's state may lack
                        if spends_received > max_spends {
                            return Err(HorizonSyncError::UnknownSpentOutput(format!(
                                "Peer sent more than {max_spends} spends for tranche \
                                 {tranche_start_height}-{tranche_end_height}"
                            )));
                        }
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
                self.check_latency(sync_peer.node_id(), &avg_latency)?;
                last_sync_timer = Instant::now();
            }

            // Verify the stream completed the full tranche before committing.
            if !saw_terminator {
                self.attempt_waited = self.attempt_waited.saturating_add(last_sync_timer.elapsed());
            }
            self.check_attempt_wait(sync_peer.node_id(), precredit)?;
            if !saw_terminator {
                return Err(HorizonSyncError::IncompleteOutputStream(format!(
                    "tranche {}-{}",
                    tranche_start_height, tranche_end_height
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

/// The most TXO items an honest peer streams for one block: an unspent output or a spend for each output or input
/// in the block. Every input and every non-coinbase output costs at least its base weight, and coinbase outputs,
/// which are not weighed, are capped in number.
fn max_streamed_items_per_block(rules: &BaseNodeConsensusManager) -> u64 {
    rules
        .consensus_constants_vec()
        .iter()
        .map(|c| {
            let max_weight = c.max_block_transaction_weight();
            let params = c.transaction_weight_params().params();
            max_weight
                .checked_div(params.output_weight)
                .unwrap_or(max_weight)
                .saturating_add(max_weight.checked_div(params.input_weight).unwrap_or(max_weight))
                .saturating_add(c.max_block_coinbase_count())
        })
        .max()
        .unwrap_or(0)
}

/// Whether a peer refused a request because this node already has a sync session with it
fn is_existing_session_refusal(err: &RpcError) -> bool {
    matches!(
        err,
        RpcError::RequestFailed(status)
            if status.as_status_code() == RpcStatusCode::Forbidden &&
                status.details().starts_with("Existing sync session")
    )
}

/// The outputs the genesis block's own inputs spend. Inserting the genesis block keeps their rows but removes them
/// from the commitment index and the output tree.
fn genesis_spent_output_hashes(genesis_block: &ChainBlock) -> BTreeSet<FixedHash> {
    genesis_block
        .block()
        .body
        .inputs()
        .iter()
        .map(|input| input.output_hash())
        .collect()
}

/// Whether a genesis output is indexed and in the output tree after inserting the genesis block
fn is_unspent_genesis_output(output: &TransactionOutput, spent: &BTreeSet<FixedHash>) -> bool {
    !output.is_burned() && !spent.contains(&output.hash())
}

/// Queues the tranche's tree updates, then the pruning of spent instances, then the outputs that mine a spent
/// commitment again, so that they all land in one write.
fn apply_tranche_output_updates<B: BlockchainBackend + 'static>(
    state_tree_updates: BTreeMap<FixedHash, Option<FixedHash>>,
    inputs_to_delete: Vec<(FixedHash, FixedHash, CompressedCommitment, OutputType)>,
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
    for (header_hash, output_hash, commitment, output_type) in inputs_to_delete {
        txn.prune_output_copy(header_hash, output_hash, commitment, output_type);
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
    use tari_comms::{
        protocol::rpc::Request,
        test_utils::mocks::create_connectivity_mock,
        types::{CommsPublicKey, CommsSecretKey},
    };
    use tari_crypto::keys::SecretKey as _;
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
        build_synchronizer_with_rules(db, sync_peers, horizon_sync_height, create_consensus_rules())
    }

    fn build_synchronizer_with_rules<'a>(
        db: BlockchainDatabase<TempDatabase>,
        sync_peers: &'a mut Vec<SyncPeer>,
        horizon_sync_height: u64,
        rules: BaseNodeConsensusManager,
    ) -> HorizonStateSynchronization<'a, TempDatabase> {
        let (connectivity, _mock) = create_connectivity_mock();
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
            .await
            .unwrap();
        assert_eq!(deletes.len(), 1);
        assert_eq!(recreated.len(), 1);
        assert!(matches!(
            sync.queue_horizon_output(output.clone(), remined.header(), &mut updates, &mut recreated, &mut txn)
                .await,
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
                .await
                .unwrap()
        );
        // Replay the remaining outputs removed by cleanup, as the resumed stream would.
        for (other, header) in other_outputs {
            assert!(
                sync.queue_horizon_output(other, &header, &mut updates, &mut recreated, &mut txn)
                    .await
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
                            .await
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
        item_delay: Duration,
        max_kernels: usize,
        extra_kernel: bool,
        /// Tranche requests, and the number of first kernel requests, refused for a busy session
        refused_requests: Vec<usize>,
        refused_kernel_requests: usize,
        refusal_delay: Duration,
    }

    impl DbTrancheSource {
        fn new(db: &BlockchainDatabase<TempDatabase>, edit: EditTranche) -> Self {
            Self {
                db: db.clone(),
                requests: 0,
                edit,
                item_delay: Duration::ZERO,
                max_kernels: usize::MAX,
                extra_kernel: false,
                refused_requests: Vec::new(),
                refused_kernel_requests: 0,
                refusal_delay: Duration::ZERO,
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

    impl HorizonSyncSource for DbTrancheSource {
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
            if self.refused_requests.contains(&self.requests) {
                return Ok(session_refusal(self.refusal_delay));
            }
            (self.edit)(self.requests, &mut items)?;
            let delay = self.item_delay;
            Ok(futures::stream::iter(items)
                .then(move |item| async move {
                    tokio::time::sleep(delay).await;
                    Ok(item)
                })
                .boxed())
        }

        /// Streams the kernels of every block from the one containing `start` to the end header, as a peer does
        async fn fetch_kernels(
            &mut self,
            request: SyncKernelsRequest,
        ) -> Result<BoxStream<'static, Result<proto::types::TransactionKernel, RpcError>>, RpcError> {
            if self.refused_kernel_requests > 0 {
                self.refused_kernel_requests = self.refused_kernel_requests.saturating_sub(1);
                return Ok(session_refusal(self.refusal_delay));
            }
            let start = self
                .db
                .fetch_header_containing_kernel_mmr(request.start)
                .unwrap()
                .height();
            let end_hash = FixedHash::try_from(request.end_header_hash).unwrap();
            let end = self.db.fetch_header_by_block_hash(end_hash).unwrap().unwrap().height;
            let mut kernels = Vec::new();
            for height in start..=end {
                let hash = self.db.fetch_header(height).unwrap().unwrap().hash();
                for kernel in self.db.fetch_kernels_in_block(hash).unwrap() {
                    kernels.push(proto::types::TransactionKernel::from(kernel));
                }
            }
            kernels.truncate(self.max_kernels);
            if self.extra_kernel {
                kernels.push(kernels.last().unwrap().clone());
            }
            let delay = self.item_delay;
            Ok(futures::stream::iter(kernels)
                .then(move |kernel| async move {
                    tokio::time::sleep(delay).await;
                    Ok(kernel)
                })
                .boxed())
        }

        fn last_request_latency(&mut self) -> Option<Duration> {
            None
        }
    }

    /// A sync peer with a new random node id
    /// A peer refuses a request for a busy session with the first item of the response stream
    fn session_refusal<T: Send + 'static>(delay: Duration) -> BoxStream<'static, Result<T, RpcError>> {
        futures::stream::once(async move {
            tokio::time::sleep(delay).await;
            Err(RpcError::RequestFailed(RpcStatus::forbidden(
                "Existing sync session found for this client. Only a single session is permitted",
            )))
        })
        .boxed()
    }

    fn test_sync_peer() -> SyncPeer {
        let public_key = CommsPublicKey::from_secret_key(&CommsSecretKey::random(&mut rand::rng()));
        PeerChainMetadata::new(
            NodeId::from_key(&public_key),
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
    /// that an honest peer's spends do not fit. The honest peer is not blamed and finishes the sync from height 1.
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

        // The resume fails on the poisoned state, then the same peer finishes the sync from height 1
        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 6, "one resumed tranche, then all five from height 1");
        assert_eq!(checkpoint_height(&db), None);
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
        assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), target.height);
    }

    /// A peer that plants a later block's output in an early tranche and then drops the connection leaves state
    /// that an honest peer's copy of that output collides with. The honest peer is not blamed and finishes the sync
    /// from height 1.
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
                if request == 4 {
                    return Err(RpcError::ClientClosed);
                }
                // Tranche 3 has room for an extra output: its end block's leaf count includes block "1"'s outputs
                if request == 3 {
                    let mined_header = items.last().unwrap().mined_header.clone();
                    items.insert(0, SyncUtxosResponse {
                        txo: Some(Txo::Output(coinbase.clone().try_into().unwrap())),
                        mined_header,
                    });
                }
                Ok(())
            }),
        );
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut planting, &target)
            .await
            .unwrap_err();
        assert_eq!(checkpoint_height(&db), Some(3));

        // The resume fails at the colliding tranche, then the same peer finishes the sync from height 1
        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 7, "two resumed tranches, then all five from height 1");
        assert_eq!(checkpoint_height(&db), None);
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }

    /// Output state that clean up cannot see is cleared before a sync from height 1, which then completes
    #[tokio::test]
    async fn stray_output_state_is_cleared_before_sync_from_height_one() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        // A tree value and a commitment index entry with no output row
        let stray = chain.get_block_and_smt_by_name("4").unwrap().block().body.outputs()[0].clone();
        let mut txn = DbTransaction::new();
        txn.apply_horizon_state_tree_updates(vec![HorizonStateTreeUpdate {
            key: FixedHash::from([7u8; 32]),
            value: Some(FixedHash::from([9u8; 32])),
            replaces_value: false,
        }]);
        txn.insert_utxo(stray.clone(), FixedHash::zero(), 0, 0);
        db.write(txn).unwrap();
        assert!(
            db.fetch_unspent_output_hash_by_commitment(stray.commitment.clone())
                .unwrap()
                .is_some()
        );
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 5);
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
        assert_eq!(
            db.fetch_outputs(stray.hash()).unwrap().len(),
            1,
            "only the synced instance of the output remains"
        );
    }

    /// The never-spent coinbase of block "1" (height 2)
    fn block_one_coinbase(chain: &TestBlockchain) -> TransactionOutput {
        chain
            .get_block_and_smt_by_name("1")
            .unwrap()
            .block()
            .body
            .outputs()
            .iter()
            .find(|output| output.is_coinbase())
            .unwrap()
            .clone()
    }

    /// An output sent again in a later tranche is indistinguishable from an honest pruned peer re-mining it after
    /// pruning its spend, so it fails the sync without a ban
    #[tokio::test]
    async fn output_sent_again_in_later_tranche_is_not_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let coinbase = block_one_coinbase(&chain);
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, items| {
                if request == 3 {
                    let mined_header = items.last().unwrap().mined_header.clone();
                    items.insert(0, SyncUtxosResponse {
                        txo: Some(Txo::Output(coinbase.clone().try_into().unwrap())),
                        mined_header,
                    });
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::OutputAlreadyStored(_)), "{err}");
        assert!(err.get_ban_reason().is_none());
    }

    /// An invalid output is banned even when its commitment is already stored, since outputs are validated first
    #[tokio::test]
    async fn invalid_output_with_stored_commitment_is_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut forged = block_one_coinbase(&chain);
        // Changing the minimum value promise invalidates the metadata signature
        forged.minimum_value_promise = MicroMinotari::from(1);
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, items| {
                if request == 3 {
                    let mined_header = items.last().unwrap().mined_header.clone();
                    items.insert(0, SyncUtxosResponse {
                        txo: Some(Txo::Output(forged.clone().try_into().unwrap())),
                        mined_header,
                    });
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long, "{err}");
    }

    /// A valid output whose commitment matches a different stored output, as after a spend the peer pruned, fails the
    /// sync without a ban
    #[tokio::test]
    async fn output_matching_a_different_stored_output_is_not_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let coinbase = block_one_coinbase(&chain);
        let block_one_hash = chain.get_block_and_smt_by_name("1").unwrap().header().hash();
        let mut stored = coinbase.clone();
        stored.minimum_value_promise = MicroMinotari::from(1);
        assert_ne!(stored.hash(), coinbase.hash());
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let receiving = db.clone();
        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, items| {
                if request == 2 {
                    // The peer leaves the coinbase out, and a different output with its commitment is stored instead
                    items.retain(|item| match &item.txo {
                        Some(Txo::Output(output)) => {
                            TransactionOutput::try_from(output.clone()).unwrap().hash() != coinbase.hash()
                        },
                        _ => true,
                    });
                    let mut txn = DbTransaction::new();
                    txn.insert_utxo(stored.clone(), block_one_hash, 2, 0);
                    receiving.write(txn).unwrap();
                }
                if request == 3 {
                    let mined_header = items.last().unwrap().mined_header.clone();
                    items.insert(0, SyncUtxosResponse {
                        txo: Some(Txo::Output(coinbase.clone().try_into().unwrap())),
                        mined_header,
                    });
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::OutputAlreadyStored(_)), "{err}");
        assert!(err.get_ban_reason().is_none());
    }

    /// A burned output is never streamed, so sending one is banned
    #[tokio::test]
    async fn burned_output_is_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);

        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(|_, items| {
                let item = items
                    .iter_mut()
                    .find(|item| matches!(item.txo, Some(Txo::Output(_))))
                    .unwrap();
                if let Some(Txo::Output(output)) = &item.txo {
                    let mut output = TransactionOutput::try_from(output.clone()).unwrap();
                    output.features.output_type = OutputType::Burn;
                    item.txo = Some(Txo::Output(output.try_into().unwrap()));
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::IncorrectResponse(_)), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long);
    }

    /// A tranche with more items than its blocks can hold is banned
    #[tokio::test]
    async fn tranche_over_item_budget_is_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;
        // Block "A" streams one output and the terminator; block "1" streams more
        sync.max_items_per_block = 1;

        let mut honest = DbTrancheSource::honest(chain.db());
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::IncorrectResponse(_)), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long);
        assert_eq!(honest.requests, 2);
    }

    /// A tranche with more outputs than the output tree has leaves at its end block is banned
    #[tokio::test]
    async fn tranche_with_more_outputs_than_end_block_holds_is_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        // Block "A" (height 1) holds one unspent output at its end
        assert_eq!(
            chain.get_block_and_smt_by_name("A").unwrap().header().output_smt_size,
            1
        );
        let extra = block_one_coinbase(&chain);
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, items| {
                if request == 1 {
                    let mined_header = items.last().unwrap().mined_header.clone();
                    items.insert(0, SyncUtxosResponse {
                        txo: Some(Txo::Output(extra.clone().try_into().unwrap())),
                        mined_header,
                    });
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::IncorrectResponse(_)), "{err}");
        assert!(err.to_string().contains("outputs for tranche"), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long);
    }

    /// A tranche with more spends than there are stored unspent outputs is banned
    #[tokio::test]
    async fn tranche_with_more_spends_than_stored_outputs_is_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        // Only the coinbase of block "A" is stored when block "1" (height 2) is streamed, and block "1" spends it
        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(|request, items| {
                if request == 2 {
                    let terminator = items.pop().unwrap();
                    items.push(SyncUtxosResponse {
                        txo: Some(Txo::Commitment(vec![1u8; 32])),
                        mined_header: terminator.mined_header.clone(),
                    });
                    items.push(terminator);
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::UnknownSpentOutput(_)), "{err}");
        assert!(err.to_string().contains("spends for tranche"), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long);
    }

    /// In tranches 1-3 and 4-5, block "4" (height 5) mines again the output block "3" (height 4) spent, which was
    /// mined before the second tranche
    #[tokio::test]
    async fn output_recreated_within_a_tranche_syncs() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 2;
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut DbTrancheSource::honest(chain.db()), &target)
            .await
            .unwrap();
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }

    /// A peer that delivers small items slower than the minimum throughput exceeds the maximum latency, even when each
    /// item arrives well within the latency limit
    #[tokio::test]
    async fn slow_stream_trips_throughput_floor() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.config.rpc_deadline = Duration::from_secs(1);

        // One tranche of all five blocks streams more than ten items of about a kilobyte, so the budget is about 1.4 s
        let mut slow = DbTrancheSource::honest(chain.db());
        slow.item_delay = Duration::from_millis(300);
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut slow, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::PeerWaitBudgetExceeded { .. }), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Short);
    }

    /// Block "1" (height 2) splits block "A"'s coinbase into many outputs, so one tranche streams more than a
    /// hundred items
    fn many_outputs_chain() -> TestBlockchain {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_coinbase_lockheight(0)
                    .build(),
            )
            .build()
            .unwrap();
        let mut chain = TestBlockchain::create(rules);
        let (_, coinbase) = chain.add_next_tip(block_spec!("A")).unwrap();
        let (txs, _) = schema_to_transaction(
            &[txn_schema!(from: vec![coinbase], to: vec![MicroMinotari::from(9000); 120])],
            &chain.km,
        );
        chain
            .add_next_tip(block_spec!("1", transactions: txs.iter().map(|tx| (**tx).clone()).collect()))
            .unwrap();
        chain
    }

    /// Time spent on local validation and writes does not count against the peer-wait budget
    #[tokio::test]
    async fn local_processing_does_not_count_against_peer_wait_budget() {
        let chain = many_outputs_chain();
        let target = chain.get_block_and_smt_by_name("1").unwrap().header().clone();
        let db = headers_only_db(&chain, &["A", "1"]);
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.config.rpc_deadline = Duration::from_secs(1);
        // The budget is about 1 s plus about 130 KB of data at 40 KB/s: under 5 s
        // Progress is reported every hundred items, inside the tranche; sleeping there stands in for slow local work
        sync.on_progress(|info| {
            if matches!(info.status, HorizonSyncStatus::Outputs { .. }) {
                std::thread::sleep(Duration::from_secs(5));
            }
        });

        sync.sync_outputs_and_finalize(test_sync_peer(), &mut DbTrancheSource::honest(chain.db()), &target)
            .await
            .unwrap();
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }

    /// The tranche allowances and the minimum throughput are relaxed in step with the maximum latency when slow peers
    /// make it relax
    #[test]
    fn peer_wait_budget_relaxes_with_max_latency() {
        let db = create_new_blockchain();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db, &mut peers, 0);
        let allowance = Duration::from_secs(10);
        assert_eq!(
            sync.peer_wait_budget(allowance, MIN_SYNC_THROUGHPUT),
            Duration::from_secs(11)
        );
        sync.max_latency = sync.config.initial_max_sync_latency.saturating_mul(2);
        assert_eq!(
            sync.peer_wait_budget(allowance, MIN_SYNC_THROUGHPUT),
            Duration::from_secs(22)
        );
    }

    /// A peer may spend longer than the RPC deadline reading a tranche with many transactions, within the allowance
    /// the tranche's kernels give, also on a resumed attempt
    #[tokio::test]
    async fn slow_server_reads_fit_the_kernel_allowance() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut DbTrancheSource::drops_at(chain.db(), 2), &target)
            .await
            .unwrap_err();
        assert_eq!(checkpoint_height(&db), Some(1));

        // Block "1" (height 2) has a coinbase and a transaction. The peer takes 1 s to read it, more than the RPC
        // deadline and the data allow, but within a second per kernel.
        sync.config.rpc_deadline = Duration::from_millis(200);
        sync.server_time_per_kernel = Duration::from_secs(1);
        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(|request, _| {
                if request == 1 {
                    std::thread::sleep(Duration::from_secs(1));
                }
                Ok(())
            }),
        );
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap();
        assert_eq!(source.requests, 4, "the attempt resumed");

        // Without the kernel allowance the same reads exceed the resumed attempt's budget
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut DbTrancheSource::drops_at(chain.db(), 2), &target)
            .await
            .unwrap_err();
        sync.config.rpc_deadline = Duration::from_millis(200);
        sync.server_time_per_kernel = Duration::ZERO;
        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(|request, _| {
                if request == 1 {
                    std::thread::sleep(Duration::from_secs(1));
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::PeerWaitBudgetExceeded { .. }), "{err}");
        assert_eq!(source.requests, 2, "the retry is charged the resume's excess wait");
    }

    /// Syncs from height 1 where the last tranche leaves out an output, after the peer kept us waiting for
    /// `percent` of the budget at the root check
    #[allow(clippy::arithmetic_side_effects)]
    async fn mismatch_after_waiting(percent: u32) -> HorizonSyncError {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;
        sync.config.rpc_deadline = Duration::from_secs(2);
        sync.server_time_per_kernel = Duration::ZERO;

        // Five tranches allow 10 s, plus the data at 40 KB/s, which the source counts as it serves it
        let mut bytes_served = 0u64;
        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, items| {
                if request == 5 {
                    let index = items
                        .iter()
                        .position(|item| matches!(item.txo, Some(Txo::Output(_))))
                        .unwrap();
                    items.remove(index);
                }
                bytes_served += items.iter().map(|item| item.encoded_len() as u64).sum::<u64>();
                if request == 5 {
                    let budget_millis = 10_000 + bytes_served * 1000 / MIN_SYNC_THROUGHPUT;
                    std::thread::sleep(Duration::from_millis(budget_millis * u64::from(percent) / 100));
                }
                Ok(())
            }),
        );
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err()
    }

    /// Syncs from height 1 where tranche 3 first resends the coinbase tranche 2 already sent, after the peer kept us
    /// waiting for `percent` of the budget at that point
    #[allow(clippy::arithmetic_side_effects)]
    async fn stored_output_resent_after_waiting(percent: u32) -> HorizonSyncError {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let coinbase = block_one_coinbase(&chain);
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;
        sync.config.rpc_deadline = Duration::from_secs(2);
        sync.server_time_per_kernel = Duration::ZERO;

        // Three tranches allow 6 s, plus the data received at 40 KB/s
        let mut bytes_served = 0u64;
        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, items| {
                if request < 3 {
                    bytes_served += items.iter().map(|item| item.encoded_len() as u64).sum::<u64>();
                    return Ok(());
                }
                if request == 3 {
                    let mined_header = items.last().unwrap().mined_header.clone();
                    let resent = SyncUtxosResponse {
                        txo: Some(Txo::Output(coinbase.clone().try_into().unwrap())),
                        mined_header,
                    };
                    bytes_served += resent.encoded_len() as u64;
                    items.insert(0, resent);
                    let budget_millis = 6000 + bytes_served * 1000 / MIN_SYNC_THROUGHPUT;
                    std::thread::sleep(Duration::from_millis(budget_millis * u64::from(percent) / 100));
                }
                Ok(())
            }),
        );
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err()
    }

    /// Any failure an honest peer can cause is short banned on an attempt from height 1 if the peer first used over
    /// 90% of its budget
    #[tokio::test]
    async fn slow_stored_output_resend_is_short_banned() {
        let err = stored_output_resent_after_waiting(95).await;
        assert!(matches!(err, HorizonSyncError::SlowPeerFailure(_)), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Short);
    }

    #[tokio::test]
    async fn moderately_slow_stored_output_resend_is_not_banned() {
        let err = stored_output_resent_after_waiting(80).await;
        assert!(matches!(err, HorizonSyncError::OutputAlreadyStored(_)), "{err}");
        assert!(err.get_ban_reason().is_none());
    }

    /// A resumed attempt that waits `delay` in its first tranche and then fails on the local state, with a checkpoint
    /// at height 3, credited about 3 s for the skipped tranches and allowed about 1 s per tranche
    async fn resume_waits_then_fails(delay: Duration) -> (Result<(), HorizonSyncError>, usize) {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let unknown = chain
            .get_block_and_smt_by_name("4")
            .unwrap()
            .block()
            .body
            .outputs()
            .iter()
            .find(|output| output.is_coinbase())
            .unwrap()
            .commitment
            .clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        checkpoint_at_three(&chain, &mut sync, &target).await;
        sync.config.rpc_deadline = Duration::from_secs(1);
        sync.server_time_per_kernel = Duration::ZERO;
        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, items| {
                if request == 1 {
                    std::thread::sleep(delay);
                    let mined_header = items.last().unwrap().mined_header.clone();
                    items.insert(0, SyncUtxosResponse {
                        txo: Some(Txo::Commitment(unknown.as_bytes().to_vec())),
                        mined_header,
                    });
                }
                Ok(())
            }),
        );
        let result = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await;
        (result, source.requests)
    }

    /// A resumer that spends the credit for the skipped tranches and then fails is retried from height 1 with that
    /// wait already charged, so the retry runs out of budget at its first tranche
    #[tokio::test]
    async fn spent_credit_is_charged_to_the_retry() {
        let (result, requests) = resume_waits_then_fails(Duration::from_millis(2600)).await;
        let err = result.unwrap_err();
        assert!(matches!(err, HorizonSyncError::PeerWaitBudgetExceeded { .. }), "{err}");
        assert_eq!(requests, 2, "the resumed tranche and the retry's first tranche");
    }

    /// An honest resumer that only used a little of the credit before failing on the local state finishes the retry
    #[tokio::test]
    async fn retry_after_honest_resume_completes() {
        let (result, requests) = resume_waits_then_fails(Duration::from_millis(1200)).await;
        result.unwrap();
        assert_eq!(requests, 6, "one resumed tranche, then all five from height 1");
    }

    /// A same-peer retry whose first request is refused because the peer still holds the failed resume's session
    /// asks again and completes, without blame; a refusal that persists ends the attempt without a ban
    #[tokio::test]
    async fn retry_waits_for_the_previous_session_to_close() {
        for refusals in [1, 4] {
            let (chain, db) = duplicate_output_sync_setup();
            let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
            let spent_hash = chain.get_block_and_smt_by_name("1").unwrap().block().body.inputs()[0].output_hash();
            let commitment = chain
                .db()
                .fetch_outputs(spent_hash)
                .unwrap()
                .pop()
                .unwrap()
                .output
                .commitment;
            let mut peers = Vec::new();
            let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
            sync.tranche_size = 0;
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
            sync.sync_outputs_and_finalize(test_sync_peer(), &mut poisoned, &target)
                .await
                .unwrap_err();

            // Request 1 is the resume, which fails on the poisoned state; the retry's first requests are refused
            let mut busy = DbTrancheSource::honest(chain.db());
            busy.refused_requests = (2..=1 + refusals).collect();
            let result = sync
                .sync_outputs_and_finalize(test_sync_peer(), &mut busy, &target)
                .await;
            if refusals == 1 {
                result.unwrap();
                assert_eq!(busy.requests, 7, "the resume, one refusal, then all five from height 1");
            } else {
                let err = result.unwrap_err();
                assert!(matches!(err, HorizonSyncError::SyncPeerUnavailable(_)), "{err}");
                assert!(err.get_ban_reason().is_none());
            }
        }
    }

    /// Kernel sync against a peer that may still hold an earlier session asks again while the peer refuses, and gives
    /// up without blame if the refusal persists
    #[tokio::test]
    async fn kernel_sync_waits_for_a_busy_session() {
        for refusals in [1, 4] {
            let (chain, db) = duplicate_output_sync_setup();
            let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
            let mut peers = Vec::new();
            let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
            sync.session_may_be_busy = true;
            let mut source = DbTrancheSource::honest(chain.db());
            source.refused_kernel_requests = refusals;
            let result = sync.synchronize_kernels(test_sync_peer(), &mut source, &target).await;
            if refusals == 1 {
                result.unwrap();
                assert_eq!(db.fetch_mmr_size(MmrTree::Kernel).unwrap(), target.kernel_mmr_size);
            } else {
                let err = result.unwrap_err();
                assert!(matches!(err, HorizonSyncError::SyncPeerUnavailable(_)), "{err}");
                assert!(err.get_ban_reason().is_none());
            }
        }
    }

    /// A peer that holds its refusals back keeps us waiting, which counts against its budget
    #[tokio::test]
    async fn slow_session_refusals_count_as_peer_wait() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.session_may_be_busy = true;
        // With no kernel allowance and no data the budget is the 1 s RPC deadline
        sync.config.rpc_deadline = Duration::from_secs(1);
        sync.server_time_per_kernel = Duration::ZERO;
        let mut source = DbTrancheSource::honest(chain.db());
        // Refusals that persist would otherwise end in a no-ban give-up
        source.refused_kernel_requests = 4;
        source.refusal_delay = Duration::from_millis(1500);
        let err = sync
            .synchronize_kernels(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::PeerWaitBudgetExceeded { .. }), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Short);
    }

    /// A from-height-1 attempt that ends in an output root mismatch after using over 90% of its time budget is short
    /// banned
    #[tokio::test]
    async fn slow_final_root_mismatch_is_short_banned() {
        let err = mismatch_after_waiting(94).await;
        assert!(matches!(err, HorizonSyncError::SlowPeerFailure(_)), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Short);
    }

    /// A mismatch after using less than 90% of the budget is not banned
    #[tokio::test]
    async fn moderately_slow_final_root_mismatch_is_not_banned() {
        let err = mismatch_after_waiting(80).await;
        assert!(matches!(err, HorizonSyncError::FinalOutputRootMismatch(_)), "{err}");
        assert!(err.get_ban_reason().is_none());
    }

    /// Leaves a checkpoint at height 3 of the duplicate output chain, synced in one-block tranches
    async fn checkpoint_at_three(
        chain: &TestBlockchain,
        sync: &mut HorizonStateSynchronization<'_, TempDatabase>,
        target: &BlockHeader,
    ) {
        sync.tranche_size = 0;
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut DbTrancheSource::drops_at(chain.db(), 4), target)
            .await
            .unwrap_err();
    }

    /// A slow source whose first tranche takes `delay` to read
    fn slow_first_read(chain: &TestBlockchain, delay: Duration) -> DbTrancheSource {
        DbTrancheSource::new(
            chain.db(),
            Box::new(move |request, _| {
                if request == 1 {
                    std::thread::sleep(delay);
                }
                Ok(())
            }),
        )
    }

    /// A resumed attempt starts with the allowance of the tranches before its checkpoint, so a slow tranche just after
    /// the checkpoint fits as it would from height 1
    #[tokio::test]
    async fn resumed_attempt_is_credited_the_skipped_tranches() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        checkpoint_at_three(&chain, &mut sync, &target).await;
        assert_eq!(checkpoint_height(&db), Some(3));

        // Without the three skipped tranches the budget is about 0.5 s; with them about 2 s
        sync.config.rpc_deadline = Duration::from_millis(500);
        sync.server_time_per_kernel = Duration::ZERO;
        let mut source = slow_first_read(&chain, Duration::from_millis(1200));
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap();
        assert_eq!(source.requests, 2, "the resume finished without a retry");
    }

    /// A resumed attempt that exceeds its budget anyway is not banned for it, but is retried from height 1 with the
    /// excess charged, so the retry runs out of budget at its first tranche
    #[tokio::test]
    async fn resumed_attempt_over_budget_is_charged_in_the_retry() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        checkpoint_at_three(&chain, &mut sync, &target).await;

        sync.config.rpc_deadline = Duration::from_millis(500);
        sync.server_time_per_kernel = Duration::ZERO;
        let mut source = slow_first_read(&chain, Duration::from_secs(3));
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::PeerWaitBudgetExceeded { .. }), "{err}");
        assert_eq!(source.requests, 2, "the resumed tranche and the retry's first tranche");
    }

    /// Kernel sync completes from an honest peer
    #[tokio::test]
    async fn kernels_sync_from_honest_peer() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.synchronize_kernels(test_sync_peer(), &mut DbTrancheSource::honest(chain.db()), &target)
            .await
            .unwrap();
        assert_eq!(db.fetch_mmr_size(MmrTree::Kernel).unwrap(), target.kernel_mmr_size);
    }

    /// A kernel stream with one kernel more than requested is banned for long
    #[tokio::test]
    async fn kernel_stream_with_extra_kernel_is_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        let mut source = DbTrancheSource::honest(chain.db());
        source.extra_kernel = true;
        let err = sync
            .synchronize_kernels(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::IncorrectResponse(_)), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long);
    }

    /// A kernel stream that ends early, as a dropped connection does, gets a short ban
    #[tokio::test]
    async fn kernel_stream_ending_early_is_short_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        let mut source = DbTrancheSource::honest(chain.db());
        source.max_kernels = 2;
        let err = sync
            .synchronize_kernels(test_sync_peer(), &mut source, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::IncompleteKernelStream(_)), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Short);
    }

    /// A peer that delivers kernels slower than the minimum throughput exceeds the peer-wait budget, even when each
    /// kernel arrives well within the latency limit
    #[tokio::test]
    async fn slow_kernel_stream_trips_throughput_floor() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.config.rpc_deadline = Duration::from_secs(1);
        sync.server_time_per_kernel = Duration::ZERO;

        // The chain's kernels are a few hundred bytes each, so the budget is about 1 s
        let mut slow = DbTrancheSource::honest(chain.db());
        slow.item_delay = Duration::from_millis(300);
        let err = sync
            .synchronize_kernels(test_sync_peer(), &mut slow, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::PeerWaitBudgetExceeded { .. }), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Short);
    }

    /// The honest per-block bound covers every non-coinbase output and input a block's weight allows, plus the
    /// coinbase limit
    #[test]
    fn item_budget_covers_a_full_block() {
        let rules = create_consensus_rules();
        let constants = rules.consensus_constants(0);
        let params = constants.transaction_weight_params().params();
        let max_weight = constants.max_block_transaction_weight();
        assert!(
            max_streamed_items_per_block(&rules) >=
                max_weight / params.output_weight +
                    max_weight / params.input_weight +
                    constants.max_block_coinbase_count()
        );
    }

    /// A commitment index entry with no output row is cleared before a sync from height 1, so the honest peer that
    /// sends that output is not banned
    #[tokio::test]
    async fn stray_commitment_index_entry_does_not_ban_honest_peer() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let coinbase = |name| {
            chain
                .get_block_and_smt_by_name(name)
                .unwrap()
                .block()
                .body
                .outputs()
                .iter()
                .find(|output| output.is_coinbase())
                .unwrap()
                .clone()
        };
        let stray = coinbase("4");
        let other = coinbase("3");
        // Pruning `stray` with the commitment of `other` leaves the commitment index entry of `stray` behind
        let mut txn = DbTransaction::new();
        txn.insert_utxo(stray.clone(), FixedHash::zero(), 0, 0);
        txn.insert_utxo(other.clone(), FixedHash::zero(), 0, 0);
        txn.prune_output_from_all_dbs(stray.hash(), other.commitment.clone(), other.features.output_type);
        db.write(txn).unwrap();
        assert!(db.fetch_outputs(stray.hash()).unwrap().is_empty());
        assert_eq!(
            db.fetch_unspent_output_hash_by_commitment(stray.commitment.clone())
                .unwrap(),
            Some(stray.hash())
        );
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }

    /// A LocalNet genesis block has no outputs, so its output tree has no root node yet. That counts as the empty
    /// tree, so a sync from height 1 passes the genesis check.
    #[tokio::test]
    async fn empty_genesis_output_tree_passes_genesis_check() {
        let (chain, db) = duplicate_output_sync_setup();
        let genesis = db.fetch_chain_header(0).unwrap();
        assert!(chain.db().fetch_outputs_in_block(*genesis.hash()).unwrap().is_empty());
        db.verify_horizon_sync_output_root(genesis.header().output_mr).unwrap();

        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;
        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 5);
    }

    /// A resumer that corrupts only the final tranche fails the root check after the checkpoint was cleared, and is
    /// still retried from height 1
    #[tokio::test]
    async fn resumer_corrupting_final_tranche_is_retried_from_height_one() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        sync.sync_outputs_and_finalize(test_sync_peer(), &mut DbTrancheSource::drops_at(chain.db(), 2), &target)
            .await
            .unwrap_err();
        assert_eq!(checkpoint_height(&db), Some(1));

        // Request 4 is the final tranche of the resume, at height 5
        let mut corrupting = DbTrancheSource::new(
            chain.db(),
            Box::new(|request, items| {
                if request == 4 {
                    let index = items
                        .iter()
                        .position(|item| matches!(item.txo, Some(Txo::Output(_))))
                        .unwrap();
                    items.remove(index);
                }
                Ok(())
            }),
        );
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut corrupting, &target)
            .await
            .unwrap();
        assert_eq!(
            corrupting.requests, 9,
            "four resumed tranches, then all five from height 1"
        );
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }

    /// A peer whose resume fails on the local state is retried from height 1, where malformed data still bans it
    #[tokio::test]
    async fn resumed_peer_that_misbehaves_from_height_one_is_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let spent_hash = chain.get_block_and_smt_by_name("1").unwrap().block().body.inputs()[0].output_hash();
        let commitment = chain
            .db()
            .fetch_outputs(spent_hash)
            .unwrap()
            .pop()
            .unwrap()
            .output
            .commitment;
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

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
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut poisoned, &target)
            .await
            .unwrap_err();
        assert_eq!(checkpoint_height(&db), Some(1));

        // Request 1 is the resumed tranche, which fails on the poisoned state. The retry from height 1 is malformed.
        let mut malicious = DbTrancheSource::new(
            chain.db(),
            Box::new(|request, items| {
                if request > 1 {
                    let mined_header = items.last().unwrap().mined_header.clone();
                    items.insert(0, SyncUtxosResponse {
                        txo: Some(Txo::Commitment(vec![1, 2, 3])),
                        mined_header,
                    });
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut malicious, &target)
            .await
            .unwrap_err();
        assert_eq!(malicious.requests, 2);
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long, "{err}");
    }

    /// An item tagged with a block below the requested tranche is banned, also when the sync resumed
    #[tokio::test]
    async fn item_mined_below_tranche_is_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let below = chain.get_block_and_smt_by_name("A").unwrap().header().hash();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut dropped = DbTrancheSource::drops_at(chain.db(), 2);
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut dropped, &target)
            .await
            .unwrap_err();
        assert_eq!(checkpoint_height(&db), Some(1));

        let mut mislabelled = DbTrancheSource::new(
            chain.db(),
            Box::new(move |_, items| {
                items[0].mined_header = below.to_vec();
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut mislabelled, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::IncorrectResponse(_)), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long);
        assert_eq!(mislabelled.requests, 1, "a data error is not retried from height 1");
    }

    /// A stream that ends without the terminator is incomplete, even if its last item is in the tranche-end block
    #[tokio::test]
    async fn stream_without_terminator_is_short_banned() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut truncated = DbTrancheSource::new(
            chain.db(),
            Box::new(|request, items| {
                if request == 2 {
                    let terminator = items.pop().unwrap();
                    assert!(terminator.txo.is_none());
                    assert!(!items.is_empty());
                }
                Ok(())
            }),
        );
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut truncated, &target)
            .await
            .unwrap_err();
        // A dropped connection also ends the stream without an error, so this is only a short ban
        assert!(matches!(err, HorizonSyncError::IncompleteOutputStream(_)), "{err}");
        assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Short);
        assert_eq!(
            checkpoint_height(&db),
            Some(1),
            "the truncated tranche must not be committed"
        );
    }

    /// A checkpoint at the target is only finalized if the committed outputs match the target's output root
    #[tokio::test]
    async fn checkpoint_at_target_is_verified_before_finalizing() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        // No outputs were synced, so the output tree does not match the target
        let mut txn = DbTransaction::new();
        txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: target.height,
            checkpoint_hash: target.hash(),
            sync_target_height: target.height,
            sync_target_hash: target.hash(),
        });
        db.write(txn).unwrap();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(
            honest.requests, 5,
            "the mismatch must restart output sync from height 1"
        );
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }

    /// A peer that drip-feeds the output stream exceeds the maximum latency
    #[tokio::test]
    async fn slow_output_stream_exceeds_max_latency() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.max_latency = Duration::from_millis(1);
        // Keeps the peer-wait budget, which scales with the maximum latency, at seconds
        sync.config.rpc_deadline = Duration::from_secs(1_000_000);

        let mut slow = DbTrancheSource::honest(chain.db());
        slow.item_delay = Duration::from_millis(20);
        let err = sync
            .sync_outputs_and_finalize(test_sync_peer(), &mut slow, &target)
            .await
            .unwrap_err();
        assert!(matches!(err, HorizonSyncError::MaxLatencyExceeded { .. }), "{err}");
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
    /// height 1. A final root mismatch does not ban the peer.
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
        // An honest pruned peer that pruned past the target cannot serve its state either
        assert!(err.get_ban_reason().is_none());
        assert_eq!(checkpoint_height(&db), None);
        // Only the genesis outputs, of which there are none here, are left
        for name in ["A", "1", "2", "3", "4"] {
            let block_hash = chain.get_block_and_smt_by_name(name).unwrap().header().hash();
            assert!(db.fetch_outputs_in_block(block_hash).unwrap().is_empty());
        }
        db.verify_horizon_sync_output_root(db.fetch_chain_header(0).unwrap().header().output_mr)
            .unwrap();

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
        block.header.output_smt_size = 1;
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

    /// Clearing stray output state before a sync from height 1 restores the genesis outputs
    #[tokio::test]
    async fn cleared_output_state_restores_genesis_outputs() {
        let (chain, genesis_output) = genesis_spend_chain();
        let target = chain.get_block_and_smt_by_name("3").unwrap().header().clone();
        let db = headers_only_db(&chain, &["1", "2", "3"]);
        let mut txn = DbTransaction::new();
        txn.apply_horizon_state_tree_updates(vec![HorizonStateTreeUpdate {
            key: FixedHash::from([7u8; 32]),
            value: Some(FixedHash::from([9u8; 32])),
            replaces_value: false,
        }]);
        db.write(txn).unwrap();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        // Block 1 spends the genesis output, so the sync only succeeds if it was restored first
        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
        assert!(
            db.fetch_unspent_output_hash_by_commitment(genesis_output.commitment)
                .unwrap()
                .is_none()
        );
    }

    /// A checkpoint whose recorded target left the canonical chain may have outputs under blocks clean up cannot
    /// find, so output sync restarts from height 1 and those outputs are removed
    #[tokio::test]
    async fn resume_discards_checkpoint_whose_target_left_the_chain() {
        let (chain, db) = duplicate_output_sync_setup();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let checkpoint = chain.get_block_and_smt_by_name("A").unwrap().header().clone();
        let real = chain
            .get_block_and_smt_by_name("4")
            .unwrap()
            .block()
            .body
            .outputs()
            .iter()
            .find(|output| output.is_coinbase())
            .unwrap()
            .clone();
        let mut stray = real.clone();
        stray.minimum_value_promise = MicroMinotari::from(1);
        // An interrupted attempt committed a batch under a block that a reorg has since replaced
        let mut txn = DbTransaction::new();
        txn.insert_utxo(stray.clone(), FixedHash::from([5u8; 32]), 4, 0);
        txn.set_horizon_sync_output_checkpoint(HorizonSyncOutputCheckpoint {
            checkpoint_height: checkpoint.height,
            checkpoint_hash: checkpoint.hash(),
            sync_target_height: target.height,
            sync_target_hash: FixedHash::from([6u8; 32]),
        });
        db.write(txn).unwrap();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, target.height);
        sync.tranche_size = 0;

        let mut honest = DbTrancheSource::honest(chain.db());
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut honest, &target)
            .await
            .unwrap();
        assert_eq!(honest.requests, 5, "output sync must start from height 1");
        assert!(db.fetch_outputs(stray.hash()).unwrap().is_empty());
        assert_eq!(
            db.fetch_unspent_output_hash_by_commitment(real.commitment.clone())
                .unwrap(),
            Some(real.hash())
        );
    }

    /// A resumed attempt records its target before streaming, so a crash leaves the target its outputs sit under
    #[tokio::test]
    async fn resume_records_its_target_before_streaming() {
        let (chain, db) = duplicate_output_sync_setup();
        let old_target = chain.get_block_and_smt_by_name("2").unwrap().header().clone();
        let target = chain.get_block_and_smt_by_name("4").unwrap().header().clone();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, old_target.height);
        sync.tranche_size = 0;

        sync.sync_outputs_and_finalize(
            test_sync_peer(),
            &mut DbTrancheSource::drops_at(chain.db(), 2),
            &old_target,
        )
        .await
        .unwrap_err();
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut DbTrancheSource::drops_at(chain.db(), 1), &target)
            .await
            .unwrap_err();
        let checkpoint = db.fetch_horizon_sync_output_checkpoint().unwrap().unwrap();
        assert_eq!(checkpoint.checkpoint_height, 1);
        assert_eq!(checkpoint.sync_target_hash, target.hash());
    }

    /// The output state check sees any output beyond the genesis outputs, and the reset restores exactly them
    #[tokio::test]
    async fn output_state_is_genesis_until_anything_else_is_stored() {
        let (chain, genesis_output) = genesis_spend_chain();
        let db = headers_only_db(&chain, &["1", "2", "3"]);
        let mut peers = Vec::new();
        let sync = build_synchronizer(db.clone(), &mut peers, 3);
        assert!(sync.output_state_is_genesis().await.unwrap());

        let stray = chain.get_block_and_smt_by_name("3").unwrap().block().body.outputs()[0].clone();
        let mut txn = DbTransaction::new();
        txn.insert_utxo(stray.clone(), FixedHash::zero(), 3, 0);
        db.write(txn).unwrap();
        assert!(!sync.output_state_is_genesis().await.unwrap());

        sync.reset_output_state_to_genesis().await.unwrap();
        assert!(sync.output_state_is_genesis().await.unwrap());
        assert!(db.fetch_outputs(stray.hash()).unwrap().is_empty());
        assert_eq!(
            db.fetch_unspent_output_hash_by_commitment(genesis_output.commitment.clone())
                .unwrap(),
            Some(genesis_output.hash())
        );
    }

    /// Esmeralda rules: its genesis block has inputs that spend some of its own outputs
    fn esmeralda_rules() -> BaseNodeConsensusManager {
        BaseNodeConsensusManager::builder(Network::Esmeralda)
            .add_consensus_constants(ConsensusConstantsBuilder::new(Network::Esmeralda).build())
            .build()
            .unwrap()
    }

    /// Esmeralda's genesis block only validates in binaries built for it, like the genesis sanity checks
    fn esmeralda_available() -> bool {
        tari_common::network_check::is_network_choice_valid(Network::Esmeralda).is_ok()
    }

    /// The genesis output state check matches what inserting a genesis block with inputs leaves, and the reset
    /// rebuilds exactly that
    #[tokio::test]
    async fn genesis_with_inputs_output_state_is_reset_exactly() {
        if !esmeralda_available() {
            return;
        }
        let rules = esmeralda_rules();
        let db = create_custom_blockchain(rules.clone());
        let genesis = db.fetch_genesis_block();
        let spent = genesis_spent_output_hashes(&genesis);
        assert!(!spent.is_empty(), "the genesis block spends some of its own outputs");
        let mut peers = Vec::new();
        let sync = build_synchronizer_with_rules(db.clone(), &mut peers, 0, rules);
        assert!(
            sync.output_state_is_genesis().await.unwrap(),
            "a fresh database holds the genesis state"
        );

        let stray = genesis.block().body.outputs()[0].clone();
        let mut txn = DbTransaction::new();
        txn.insert_utxo(stray, FixedHash::zero(), 1, 0);
        db.write(txn).unwrap();
        assert!(!sync.output_state_is_genesis().await.unwrap());

        sync.reset_output_state_to_genesis().await.unwrap();
        assert!(sync.output_state_is_genesis().await.unwrap());
        for output in genesis.block().body.outputs() {
            assert!(!db.fetch_outputs(output.hash()).unwrap().is_empty());
            if spent.contains(&output.hash()) {
                assert!(
                    db.fetch_unspent_output_hash_by_commitment(output.commitment.clone())
                        .unwrap()
                        .is_none()
                );
            }
        }
    }

    /// Syncs from height 1 on a chain whose genesis block spends some of its own outputs, from an archival or a
    /// pruned peer
    async fn sync_esmeralda_chain(pruned_source: bool) {
        if !esmeralda_available() {
            return;
        }
        let rules = esmeralda_rules();
        let mut chain = TestBlockchain::create(rules.clone());
        for name in ["1", "2", "3"] {
            chain.add_next_tip(block_spec!(name)).unwrap();
        }
        if pruned_source {
            let mut txn = DbTransaction::new();
            txn.set_pruning_horizon(1);
            chain.db().write(txn).unwrap();
            chain.db().prune_to_height(2).unwrap();
        }
        let target = chain.get_block_and_smt_by_name("3").unwrap().header().clone();
        let db = headers_only_db(&chain, &["1", "2", "3"]);
        let mut peers = Vec::new();
        let mut sync = build_synchronizer_with_rules(db.clone(), &mut peers, target.height, rules);
        sync.tranche_size = 0;

        // The genesis outputs the genesis block spends are never pruned, so no spend is sent for them
        let mut source = DbTrancheSource::new(
            chain.db(),
            Box::new(|request, items| {
                if request == 1 {
                    assert!(items.iter().all(|item| !matches!(item.txo, Some(Txo::Commitment(_)))));
                }
                Ok(())
            }),
        );
        sync.sync_outputs_and_finalize(test_sync_peer(), &mut source, &target)
            .await
            .unwrap();
        db.verify_horizon_sync_output_root(target.output_mr).unwrap();
    }

    #[tokio::test]
    async fn genesis_with_inputs_syncs_from_archival_peer() {
        sync_esmeralda_chain(false).await;
    }

    #[tokio::test]
    async fn genesis_with_inputs_syncs_from_pruned_peer() {
        sync_esmeralda_chain(true).await;
    }

    /// A node holding a genesis output's row, not indexed as if the genesis block spent it, plus an identical copy of
    /// it stored under block "2"
    fn genesis_row_with_later_copy() -> (TestBlockchain, BlockchainDatabase<TempDatabase>, TransactionOutput) {
        let (chain, genesis_output) = genesis_spend_chain();
        let db = headers_only_db(&chain, &["1", "2", "3"]);
        let block_two = chain.get_block_and_smt_by_name("2").unwrap().header().clone();
        let mut txn = DbTransaction::new();
        txn.delete_utxo_commitment_index_entry(genesis_output.commitment.clone());
        txn.insert_utxo(genesis_output.clone(), block_two.hash(), block_two.height, 0);
        db.write(txn).unwrap();
        assert_eq!(db.fetch_outputs(genesis_output.hash()).unwrap().len(), 2);
        (chain, db, genesis_output)
    }

    fn assert_only_genesis_row_left(db: &BlockchainDatabase<TempDatabase>, output: &TransactionOutput) {
        let rows = db.fetch_outputs(output.hash()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].mined_height, 0);
        assert!(
            db.fetch_unspent_output_hash_by_commitment(output.commitment.clone())
                .unwrap()
                .is_none()
        );
    }

    /// Cleaning up a block removes only the copies of outputs stored under that block
    #[tokio::test]
    async fn clean_up_keeps_copy_of_output_under_other_block() {
        let (_chain, db, genesis_output) = genesis_row_with_later_copy();
        let mut peers = Vec::new();
        let mut sync = build_synchronizer(db.clone(), &mut peers, 3);
        sync.clean_up_stale_synced_outputs(2).await.unwrap();
        assert_only_genesis_row_left(&db, &genesis_output);
    }

    /// A spend removes only the copy it spends
    #[tokio::test]
    async fn spend_keeps_copy_of_output_under_other_block() {
        let (_chain, db, genesis_output) = genesis_row_with_later_copy();
        let mut peers = Vec::new();
        let sync = build_synchronizer(db.clone(), &mut peers, 3);
        let mut txn = sync.db().write_transaction();
        let mut updates = BTreeMap::new();
        let mut deletes = Vec::new();
        sync.queue_horizon_spend(genesis_output.commitment.as_bytes(), 3, &mut updates, &mut deletes)
            .await
            .unwrap();
        apply_tranche_output_updates(updates, deletes, Vec::new(), &mut txn);
        txn.commit().await.unwrap();
        assert_only_genesis_row_left(&db, &genesis_output);
    }
}
