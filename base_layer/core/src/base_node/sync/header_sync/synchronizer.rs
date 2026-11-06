//  Copyright 2020, The Tari Project
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
    collections::HashMap,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use log::*;
use tari_common_types::{chain_metadata::ChainMetadata, types::HashOutput};
use tari_comms::{
    PeerConnection,
    RefKind,
    connectivity::ConnectivityRequester,
    peer_manager::NodeId,
    protocol::rpc::{RpcClient, RpcError},
};
use tari_node_components::blocks::{BlockHeader, ChainBlock, ChainHeader};
use tari_transaction_components::{BanPeriod, tari_proof_of_work::PowData};
use tari_utilities::hex::Hex;

pub(crate) use super::{BlockHeaderSyncError, validator::BlockHeaderSyncValidator};
use crate::{
    base_node::sync::{
        BlockchainSyncConfig,
        SyncPeer,
        ban::{PeerBanManager, RECENT_OUTCOME_WINDOW, record_recent_outcome},
        header_sync::HEADER_SYNC_INITIAL_MAX_HEADERS,
        hooks::Hooks,
        rpc,
    },
    chain_storage::{
        BlockchainBackend,
        ChainStorageError,
        PendingChainRewind,
        async_db::AsyncBlockchainDb,
        reorg_reintroduces_pre_ghsa_blocks,
    },
    common::rolling_avg::RollingAverageTime,
    consensus::BaseNodeConsensusManager,
    proof_of_work::randomx_factory::RandomXFactory,
    proto::{
        base_node::{FindChainSplitRequest, SyncHeadersRequest},
        core::BlockHeader as ProtoBlockHeader,
    },
};

const LOG_TARGET: &str = "c::bn::header_sync";

const MAX_LATENCY_INCREASES: usize = 5;

/// A peer whose attempts end because our chain changed this many times within the window is banned.
const MAX_CHAIN_CHANGES_PER_PEER: usize = 3;

pub struct HeaderSynchronizer<'a, B> {
    config: BlockchainSyncConfig,
    db: AsyncBlockchainDb<B>,
    header_validator: BlockHeaderSyncValidator<B>,
    connectivity: ConnectivityRequester,
    sync_peers: &'a mut Vec<SyncPeer>,
    hooks: Hooks,
    local_cached_metadata: &'a ChainMetadata,
    peer_ban_manager: PeerBanManager,
    consensus_rules: BaseNodeConsensusManager,
    /// Time spent on each peer by the strong dial before its attempt
    predial_time: HashMap<NodeId, Duration>,
    /// Time the current attempt has spent waiting on the network, which is what its budget bounds
    attempt_network_time: Duration,
    attempt_start_block_hash: HashOutput,
    chain_changes: &'a mut HashMap<NodeId, Vec<Instant>>,
    /// Test only: `split_impact` misses a reset, as if the pruned height moved after it
    #[cfg(test)]
    stale_split_impact: bool,
    /// Test only: how many times `switch_to_pending_chain` asked the database to rewind
    #[cfg(test)]
    rewind_attempts: usize,
}

impl<'a, B: BlockchainBackend + 'static> HeaderSynchronizer<'a, B> {
    pub fn new(
        config: BlockchainSyncConfig,
        db: AsyncBlockchainDb<B>,
        consensus_rules: BaseNodeConsensusManager,
        connectivity: ConnectivityRequester,
        sync_peers: &'a mut Vec<SyncPeer>,
        randomx_factory: RandomXFactory,
        local_metadata: &'a ChainMetadata,
        chain_changes: &'a mut HashMap<NodeId, Vec<Instant>>,
    ) -> Self {
        let peer_ban_manager = PeerBanManager::new(config.clone(), connectivity.clone());
        Self {
            config,
            header_validator: BlockHeaderSyncValidator::new(db.clone(), consensus_rules.clone(), randomx_factory),
            db,
            connectivity,
            sync_peers,
            hooks: Default::default(),
            local_cached_metadata: local_metadata,
            peer_ban_manager,
            consensus_rules,
            predial_time: HashMap::new(),
            attempt_network_time: Duration::ZERO,
            attempt_start_block_hash: HashOutput::zero(),
            chain_changes,
            #[cfg(test)]
            stale_split_impact: false,
            #[cfg(test)]
            rewind_attempts: 0,
        }
    }

    pub fn on_starting<H>(&mut self, hook: H)
    where for<'r> H: FnOnce(&SyncPeer) + Send + Sync + 'static {
        self.hooks.add_on_starting_hook(hook);
    }

    pub fn on_progress<H>(&mut self, hook: H)
    where H: Fn(u64, u64, &SyncPeer) + Send + Sync + 'static {
        self.hooks.add_on_progress_header_hook(hook);
    }

    pub fn on_rewind<H>(&mut self, hook: H)
    where H: Fn(Vec<Arc<ChainBlock>>) + Send + Sync + 'static {
        self.hooks.add_on_rewind_hook(hook);
    }

    pub async fn synchronize(&mut self) -> Result<(SyncPeer, AttemptSyncResult), BlockHeaderSyncError> {
        debug!(target: LOG_TARGET, "Starting header sync.",);

        info!(
            target: LOG_TARGET,
            "Synchronizing headers ({} candidate peers selected)",
            self.sync_peers.len()
        );

        // Ensure every sync-candidate peer has a Strong PeerConnection attached for the
        // duration of header sync. The Strong handle bumps the per-connection strong counter,
        // signalling to background reapers (ConnectivityManager n-closest pruning,
        // inactive-connection reaping, DhtConnectivity random-pool pruning) that they must not
        // disconnect the peer. If a stored connection is already attached (e.g. from a prior
        // sync stage that downgraded it to Weak), upgrade in place rather than redialling.
        for peer in self.sync_peers.iter_mut() {
            if let Some(conn) = peer.connection() &&
                !conn.is_connected()
            {
                peer.clear_connection();
            }
            if peer.ensure_strong_connection() {
                continue;
            }
            let dial_started = Instant::now();
            let dial_result = tokio::time::timeout(
                self.config.rpc_deadline,
                self.connectivity.dial_peer(peer.node_id().clone(), RefKind::Strong),
            )
            .await;
            self.predial_time.insert(peer.node_id().clone(), dial_started.elapsed());
            match dial_result {
                Ok(Ok(conn)) => peer.set_connection(conn),
                Ok(Err(e)) => debug!(
                    target: LOG_TARGET,
                    "Failed to dial sync peer {} as strong: {e}", peer.node_id()
                ),
                Err(_) => debug!(
                    target: LOG_TARGET,
                    "Timed out dialing sync peer {} as strong", peer.node_id()
                ),
            }
        }

        let result = self.synchronize_inner().await;

        // Sync is done (success or failure): downgrade every Strong handle to Weak so reapers
        // may reclaim the connection if needed. The connections remain attached on the
        // SyncPeers, so the next sync stage can upgrade them back in place without redialling.
        for peer in self.sync_peers.iter_mut() {
            peer.downgrade_connection();
        }

        result
    }

    async fn synchronize_inner(&mut self) -> Result<(SyncPeer, AttemptSyncResult), BlockHeaderSyncError> {
        let mut max_latency = self.config.initial_max_sync_latency;
        let mut latency_increases_counter = 0usize;
        loop {
            match self.try_sync_from_all_peers(max_latency).await {
                Ok((peer, sync_result)) => break Ok((peer, sync_result)),
                Err(err @ BlockHeaderSyncError::AllSyncPeersExceedLatency) => {
                    // If we have few sync peers, throw this out to be retried later
                    if self.sync_peers.len() < 2 {
                        return Err(err);
                    }
                    max_latency = max_latency.saturating_add(self.config.max_latency_increase);
                    latency_increases_counter = latency_increases_counter.saturating_add(1);
                    if latency_increases_counter > MAX_LATENCY_INCREASES {
                        return Err(err);
                    }
                },
                Err(err) => break Err(err),
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn try_sync_from_all_peers(
        &mut self,
        max_latency: Duration,
    ) -> Result<(SyncPeer, AttemptSyncResult), BlockHeaderSyncError> {
        let sync_peer_node_ids = self.sync_peers.iter().map(|p| p.node_id()).cloned().collect::<Vec<_>>();
        info!(
            target: LOG_TARGET,
            "Attempting to sync headers ({} sync peers)",
            sync_peer_node_ids.len()
        );
        let mut latency_counter = 0usize;
        for node_id in sync_peer_node_ids {
            self.attempt_network_time = self.predial_time.remove(&node_id).unwrap_or_default();
            match self.connect_and_attempt_sync(&node_id, max_latency).await {
                Ok((peer, sync_result)) => {
                    self.chain_changes.remove(&node_id);
                    return Ok((peer, sync_result));
                },
                // Not this peer's fault, and the next round starts from the new chain
                Err(err @ BlockHeaderSyncError::ChainResetToGenesis { .. }) => return Err(err),
                Err(err) => {
                    let err = match err {
                        BlockHeaderSyncError::ChainChangedDuringSync => self.record_chain_change(&node_id),
                        err => err,
                    };
                    let ban_reason = BlockHeaderSyncError::get_ban_reason(&err);
                    if let Some(reason) = ban_reason {
                        warn!(target: LOG_TARGET, "{err}");
                        let duration = match reason.ban_duration {
                            BanPeriod::Short => self.config.short_ban_period,
                            BanPeriod::Long => self.config.ban_period,
                        };
                        self.peer_ban_manager
                            .ban_peer_if_required(&node_id, reason.reason, duration)
                            .await;
                    }
                    if let BlockHeaderSyncError::MaxLatencyExceeded { .. } = err {
                        latency_counter = latency_counter.saturating_add(1);
                    } else {
                        self.remove_sync_peer(&node_id);
                    }
                },
            }
        }

        if self.sync_peers.is_empty() {
            Err(BlockHeaderSyncError::NoMoreSyncPeers("Header sync failed".to_string()))
        } else if latency_counter >= self.sync_peers.len() {
            Err(BlockHeaderSyncError::AllSyncPeersExceedLatency)
        } else {
            Err(BlockHeaderSyncError::SyncFailedAllPeers)
        }
    }

    async fn connect_and_attempt_sync(
        &mut self,
        node_id: &NodeId,
        max_latency: Duration,
    ) -> Result<(SyncPeer, AttemptSyncResult), BlockHeaderSyncError> {
        let peer_index = self
            .get_sync_peer_index(node_id)
            .ok_or(BlockHeaderSyncError::PeerNotFound)?;
        let sync_peer = self.sync_peers.get(peer_index).expect("Already checked");
        self.hooks.call_on_starting_hook(sync_peer);

        // Prefer the connection pre-dialled (as Strong) in `synchronize()` and stashed on the
        // SyncPeer. Hand the per-attempt code a Weak clone — the strong handle living on the
        // SyncPeer keeps the connection pinned for the full sync, while dropping this clone
        // when the attempt ends has no effect on the counter.
        let mut conn = match sync_peer.connection() {
            Some(stored) if stored.is_connected() => stored.clone_weak(),
            _ => {
                let budget = self.attempt_budget();
                within_budget(
                    &mut self.attempt_network_time,
                    budget,
                    Self::dial_sync_peer(&self.connectivity, node_id),
                )
                .await??
            },
        };
        debug!(
            target: LOG_TARGET,
            "Attempting to synchronize headers with `{node_id}`"
        );
        // Defensive: the connection may have been torn down by another subsystem between
        // dial returning and this point (e.g. DhtConnectivity pruning). This is not the peer's
        // fault, so use NotInSync (no ban) to trigger a skip-and-retry with the next peer.
        if !conn.is_connected() {
            warn!(
                target: LOG_TARGET,
                "Sync peer `{node_id}` was disconnected before RPC negotiation could begin"
            );
            return Err(BlockHeaderSyncError::NotInSync);
        }

        let config = RpcClient::builder()
            .with_deadline(self.config.rpc_deadline)
            .with_deadline_grace_period(Duration::from_secs(5))
            // Headers are small, so let the client buffer many while they are validated
            .with_stream_buffer_size(100)
            // A streamed header is a few hundred bytes of fixed-size fields plus its pow data, which is at most
            // `PowData::max_size()` (64 KiB) at decode time. 4x that is a generous bound that still keeps a full
            // buffer of padded headers at ~25 MiB rather than ~800 MiB.
            .with_max_response_size(PowData::default().max_size().saturating_mul(4));
        // Bound RPC negotiation so a stuck negotiation cannot wedge the sync loop.
        let budget = self.attempt_budget();
        let mut client = within_budget(
            &mut self.attempt_network_time,
            budget,
            tokio::time::timeout(
                self.config.rpc_deadline,
                conn.connect_rpc_using_builder::<rpc::BaseNodeSyncRpcClient>(config),
            ),
        )
        .await?
        .map_err(|_| BlockHeaderSyncError::RpcError(RpcError::ReplyTimeout))?
        .map_err(BlockHeaderSyncError::from_connect_error)?;

        let latency = client
            .get_last_request_latency()
            .expect("unreachable panic: last request latency must be set after connect");
        self.sync_peers
            .get_mut(peer_index)
            .ok_or(BlockHeaderSyncError::PeerNotFound)?
            .set_latency(latency);
        if latency > max_latency {
            return Err(BlockHeaderSyncError::MaxLatencyExceeded {
                peer: conn.peer_node_id().clone(),
                latency,
                max_latency,
            });
        }

        debug!(target: LOG_TARGET, "Sync peer latency is {latency:.2?}");
        let sync_peer = self
            .sync_peers
            .get(peer_index)
            .ok_or(BlockHeaderSyncError::PeerNotFound)?
            .clone();
        let sync_result = self.attempt_sync(&sync_peer, client, max_latency).await?;
        Ok((sync_peer, sync_result))
    }

    async fn dial_sync_peer(
        connectivity: &ConnectivityRequester,
        node_id: &NodeId,
    ) -> Result<PeerConnection, BlockHeaderSyncError> {
        let timer = Instant::now();
        debug!(target: LOG_TARGET, "Dialing {node_id} sync peer");
        // `synchronize()` holds Strong references to every sync peer for the duration of this
        // sync, so the per-attempt redial can be Weak — the strong handles in the outer guard
        // list keep the connection pinned.
        let conn = connectivity.dial_peer(node_id.clone(), RefKind::Weak).await?;
        info!(
            target: LOG_TARGET,
            "Successfully dialed sync peer {} in {:.2?}",
            node_id,
            timer.elapsed()
        );
        Ok(conn)
    }

    async fn attempt_sync(
        &mut self,
        sync_peer: &SyncPeer,
        mut client: rpc::BaseNodeSyncRpcClient,
        max_latency: Duration,
    ) -> Result<AttemptSyncResult, BlockHeaderSyncError> {
        let latency = client.get_last_request_latency();
        debug!(
            target: LOG_TARGET,
            "Initiating header sync with peer `{}` (sync latency = {}ms)",
            sync_peer.node_id(),
            latency.unwrap_or_default().as_millis()
        );

        // Fetch best local data at the beginning of the sync process
        let best_block_metadata = self.db.get_chain_metadata().await?;
        let best_header = self.db.fetch_last_chain_header().await?;
        let best_block_header = self
            .db
            .fetch_chain_header(best_block_metadata.best_block_height())
            .await?;
        let best_header_height = best_header.height();
        let best_block_height = best_block_header.height();
        self.attempt_start_block_hash = *best_block_header.hash();

        if best_header_height < best_block_height || best_block_height < self.local_cached_metadata.best_block_height()
        {
            return Err(BlockHeaderSyncError::ChainStorageError(
                ChainStorageError::CorruptedDatabase("Inconsistent block and header data".to_string()),
            ));
        }

        // - At this point we may have more (InSyncOrAhead), equal (InSyncOrAhead), or less headers (Lagging) than the
        //   peer, but they claimed better POW before we attempted sync.
        // - This method will return ban-able errors for certain offenses.
        let (header_sync_status, peer_response) = self
            .determine_sync_status(
                sync_peer,
                best_header.clone(),
                best_block_header.clone(),
                self.config.max_reorg_depth_allowed,
                &mut client,
            )
            .await?;

        match header_sync_status.clone() {
            HeaderSyncStatus::InSyncOrAhead => {
                debug!(
                    target: LOG_TARGET,
                    "Headers are in sync at height {best_header_height} but tip is {best_block_height}. Proceeding to archival/pruned block sync"
                );

                Ok(AttemptSyncResult {
                    headers_returned: peer_response.peer_headers.len() as u64,
                    peer_fork_hash_index: peer_response.peer_fork_hash_index,
                    header_sync_status,
                })
            },
            HeaderSyncStatus::Lagging(split_info) => {
                self.hooks.call_on_progress_header_hooks(
                    split_info
                        .best_block_header
                        .height()
                        .saturating_sub(split_info.reorg_steps_back),
                    sync_peer.claimed_chain_metadata().best_block_height(),
                    sync_peer,
                );
                self.synchronize_headers(sync_peer.clone(), &mut client, *split_info, max_latency)
                    .await?;
                Ok(AttemptSyncResult {
                    headers_returned: peer_response.peer_headers.len() as u64,
                    peer_fork_hash_index: peer_response.peer_fork_hash_index,
                    header_sync_status,
                })
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn find_chain_split(
        &mut self,
        peer_node_id: &NodeId,
        max_reorg_depth_allowed: usize,
        client: &mut rpc::BaseNodeSyncRpcClient,
        header_count: u64,
    ) -> Result<FindChainSplitResult, BlockHeaderSyncError> {
        const NUM_CHAIN_SPLIT_HEADERS: usize = 500;
        // Limit how far back we're willing to go. A peer might just say it does not have a chain split
        // and keep us busy going back until the genesis.
        // 20 x 500 = max 10,000 block split can be detected. The 10_000 limit is default, but can be overridden
        let max_chain_split_iters = max_reorg_depth_allowed.saturating_div(NUM_CHAIN_SPLIT_HEADERS);

        let mut offset = 0;
        let mut sent_hashes = Vec::new();
        let mut iter_count = 0usize;
        loop {
            iter_count = iter_count.saturating_add(1);
            if iter_count > max_chain_split_iters {
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` did not provide a chain split after {} headers requested. Peer will be banned.",
                    peer_node_id,
                    NUM_CHAIN_SPLIT_HEADERS.saturating_mul(max_chain_split_iters),
                );
                return Err(BlockHeaderSyncError::ChainSplitNotFound(peer_node_id.clone()));
            }

            let block_hashes = self
                .db
                .fetch_block_hashes_from_header_tip(NUM_CHAIN_SPLIT_HEADERS, offset)
                .await?;
            debug!(
                target: LOG_TARGET,
                "Determining if chain splits between {} and {} headers back from the tip (peer: `{}`, {} hashes sent)",
                offset,
                offset.saturating_add(NUM_CHAIN_SPLIT_HEADERS),
                peer_node_id,
                block_hashes.len()
            );

            // No further hashes to send.
            if block_hashes.is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` did not provide a chain split after {} headers requested. Peer will be banned.",
                    peer_node_id,
                    NUM_CHAIN_SPLIT_HEADERS.saturating_mul(max_chain_split_iters),
                );
                return Err(BlockHeaderSyncError::ChainSplitNotFound(peer_node_id.clone()));
            }

            sent_hashes.extend(block_hashes.iter().copied());
            let request = FindChainSplitRequest {
                block_hashes: block_hashes.clone().iter().map(|v| v.to_vec()).collect(),
                header_count,
            };

            let budget = self.attempt_budget();
            let resp = match within_budget(&mut self.attempt_network_time, budget, client.find_chain_split(request))
                .await?
            {
                Ok(r) => r,
                Err(RpcError::RequestFailed(err)) if err.as_status_code().is_not_found() => {
                    // This round we sent less hashes than the max, so the next round will not have any more hashes to
                    // send. Exit early in this case.
                    if block_hashes.len() < NUM_CHAIN_SPLIT_HEADERS {
                        warn!(
                            target: LOG_TARGET,
                            "Peer `{}` did not provide a chain split after {} headers requested. Peer will be banned.",
                            peer_node_id,
                            NUM_CHAIN_SPLIT_HEADERS.saturating_mul(max_chain_split_iters),
                        );
                        return Err(BlockHeaderSyncError::ChainSplitNotFound(peer_node_id.clone()));
                    }
                    // Chain split not found, let's go further back
                    offset = NUM_CHAIN_SPLIT_HEADERS.saturating_mul(iter_count);
                    continue;
                },
                Err(err) => {
                    return Err(err.into());
                },
            };
            if resp.headers.len() > HEADER_SYNC_INITIAL_MAX_HEADERS {
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` sent too many headers {}, only requested {}. Peer will be banned.",
                    peer_node_id,
                    resp.headers.len(),
                    HEADER_SYNC_INITIAL_MAX_HEADERS,
                );
                return Err(BlockHeaderSyncError::PeerSentTooManyHeaders(resp.headers.len()));
            }
            if resp.fork_hash_index >= block_hashes.len() as u64 {
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` sent hash index {} out of range {}. Peer will be banned.",
                    peer_node_id,
                    resp.fork_hash_index,
                    block_hashes.len(),
                );
                return Err(BlockHeaderSyncError::FoundHashIndexOutOfRange(
                    block_hashes.len() as u64,
                    resp.fork_hash_index,
                ));
            }
            #[allow(clippy::cast_possible_truncation)]
            if !resp.headers.is_empty() &&
                *resp.headers.first().expect("Already checked").prev_hash !=
                    *block_hashes
                        .get(resp.fork_hash_index as usize)
                        .expect("Already checked")
            {
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` sent hash an invalid protocol response, incorrect fork hash index {}. Peer will be banned.",
                    peer_node_id,
                    resp.fork_hash_index,
                );
                return Err(BlockHeaderSyncError::InvalidProtocolResponse(
                    "Peer sent incorrect fork hash index".into(),
                ));
            }
            #[allow(clippy::cast_possible_truncation)]
            let chain_split_hash = *block_hashes
                .get(resp.fork_hash_index as usize)
                .expect("Already checked");

            return Ok(FindChainSplitResult {
                reorg_steps_back: resp.fork_hash_index.saturating_add(offset as u64),
                peer_headers: resp.headers,
                peer_fork_hash_index: resp.fork_hash_index,
                chain_split_hash,
                sent_hashes,
            });
        }
    }

    /// Attempt to determine the point at which the remote and local chain diverge, returning the relevant information
    /// of the chain split (see [HeaderSyncStatus]).
    ///
    /// If the local node is behind the remote chain (i.e. `HeaderSyncStatus::Lagging`), the appropriate
    /// `ChainSplitInfo` is returned, the header validator is initialized and the preliminary headers are validated.
    #[allow(clippy::too_many_lines)]
    async fn determine_sync_status(
        &mut self,
        sync_peer: &SyncPeer,
        best_header: ChainHeader,
        best_block_header: ChainHeader,
        max_reorg_depth_allowed: usize,
        client: &mut rpc::BaseNodeSyncRpcClient,
    ) -> Result<(HeaderSyncStatus, FindChainSplitResult), BlockHeaderSyncError> {
        // This method will return ban-able errors for certain offenses.
        let chain_split_result = self
            .find_chain_split(
                sync_peer.node_id(),
                max_reorg_depth_allowed,
                client,
                HEADER_SYNC_INITIAL_MAX_HEADERS as u64,
            )
            .await?;
        if chain_split_result.reorg_steps_back > 0 {
            debug!(
                target: LOG_TARGET,
                "Found chain split {} blocks back, received {} headers from peer `{}`",
                chain_split_result.reorg_steps_back,
                chain_split_result.peer_headers.len(),
                sync_peer
            );

            self.check_deep_reorg_anchor(sync_peer, &best_block_header, chain_split_result.chain_split_hash)
                .await?;
        }

        // If the peer returned no new headers, they may still have more blocks than we have, thus have a higher
        // accumulated difficulty.
        if chain_split_result.peer_headers.is_empty() {
            // Our POW is less than the peer's POW, as verified before the attempted header sync, therefore, if the
            // peer did not supply any headers and we know we are behind based on the peer's claimed metadata, then
            // we can ban the peer.
            if best_header.height() == best_block_header.height() {
                let local_metadata = self.db.get_chain_metadata().await?;
                if sync_peer.claimed_chain_metadata().accumulated_difficulty() <=
                    local_metadata.accumulated_difficulty()
                {
                    debug!(
                        target: LOG_TARGET,
                        "Peer `{}` sent no headers, but our chain has reached their claim since sync started",
                        sync_peer.node_id()
                    );
                    return Err(BlockHeaderSyncError::ChainChangedDuringSync);
                }
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` did not provide any headers although they have a better chain and more headers: their \
                    difficulty: {}, our difficulty: {}. Peer will be banned.",
                    sync_peer.node_id(),
                    sync_peer.claimed_chain_metadata().accumulated_difficulty(),
                    best_block_header.accumulated_data().total_accumulated_difficulty,
                );
                return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                    claimed: sync_peer.claimed_chain_metadata().accumulated_difficulty(),
                    actual: None,
                    local: best_block_header.accumulated_data().total_accumulated_difficulty,
                });
            }
            // Our headers are ahead of our blocks, so the claim must be covered by our header chain
            let claimed = sync_peer.claimed_chain_metadata().accumulated_difficulty();
            if claimed > best_header.accumulated_data().total_accumulated_difficulty {
                let header_tip = self.db.fetch_last_chain_header().await?;
                let local = header_tip.accumulated_data().total_accumulated_difficulty;
                if claimed > local {
                    warn!(
                        target: LOG_TARGET,
                        "Peer `{}` did not provide any headers although they claim more work ({}) than our header \
                         chain ({}). Peer will be banned.",
                        sync_peer.node_id(),
                        claimed,
                        local,
                    );
                    return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                        claimed,
                        actual: None,
                        local,
                    });
                }
            }
            debug!(target: LOG_TARGET, "Peer `{}` sent no headers; headers already in sync with peer.", sync_peer.node_id());
            return Ok((HeaderSyncStatus::InSyncOrAhead, chain_split_result));
        }

        let headers = chain_split_result
            .peer_headers
            .clone()
            .into_iter()
            .map(BlockHeader::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BlockHeaderSyncError::ReceivedInvalidHeader)?;
        let num_new_headers = headers.len();
        // Do a cheap check to verify that we do not have these series of headers in the db already - if the 1st one is
        // not there most probably the rest are not either - the peer could still have returned old headers later on in
        // the list
        let first_hash = headers.first().expect("Already checked").hash();
        if self.db.fetch_header_by_block_hash(first_hash).await?.is_some() {
            // It follows a hash we sent, so if we did not send it too it joined our chain after we sent them
            if !chain_split_result.sent_hashes.contains(&first_hash) {
                debug!(
                    target: LOG_TARGET,
                    "Peer `{}` sent a header we added since sync started", sync_peer.node_id()
                );
                return Err(BlockHeaderSyncError::ChainChangedDuringSync);
            }
            return Err(BlockHeaderSyncError::ReceivedInvalidHeader(
                "Header already in database".to_string(),
            ));
        };

        // The split hash is one of ours, so if it is gone our chain changed after we sent it
        self.header_validator
            .initialize_state(&chain_split_result.chain_split_hash)
            .await
            .map_err(|err| match err {
                BlockHeaderSyncError::StartHashNotFound(_) => BlockHeaderSyncError::ChainChangedDuringSync,
                err => err,
            })?;
        for header in headers {
            debug!(
                target: LOG_TARGET,
                "Validating header #{} (Pow: {}) with hash: ({})",
                header.height,
                header.pow_algo(),
                header.hash().to_hex(),
            );
            self.header_validator.validate(header).await?;
        }

        debug!(
            target: LOG_TARGET,
            "Peer `{}` has submitted {} valid header(s)", sync_peer.node_id(), num_new_headers
        );

        let chain_split_info = ChainSplitInfo {
            best_block_header,
            reorg_steps_back: chain_split_result.reorg_steps_back,
            chain_split_hash: chain_split_result.chain_split_hash,
        };
        Ok((
            HeaderSyncStatus::Lagging(Box::new(chain_split_info)),
            chain_split_result,
        ))
    }

    /// GHSA-3qmx-q9pv-f3m4 deep reorg anchor, header sync half.
    ///
    /// `swap_to_highest_pow_chain` anchors the gossip driven reorg path, but header sync never goes through it:
    /// `switch_to_pending_chain` rewinds the local chain itself, via `rewind_blockchain`. That is the route a
    /// genuinely deep fork arrives by, so it has to be anchored here too or the rule is decorative.
    ///
    /// This is called right after the chain split is located and before a single header has been validated, so
    /// the refusal costs nothing and the peer is banned rather than being left to keep feeding us a chain we will
    /// throw away. `chain_split_hash` is a hash we sent the peer, from our own chain, so its height is ours to
    /// look up and cannot be asserted by them. The blocks a reorg would add start at `split height + 1`; see
    /// `reorg_reintroduces_pre_ghsa_blocks` for why the rule is about the added blocks rather than the split
    /// point, and `GHSA_DEEP_REORG_CONFIRMATION_WINDOW` for why it does not engage until our tip is well past the
    /// activation height. The ban here is what makes the window load bearing: below it the rule must not fire at
    /// all, because an honest peer serving an ordinary reorg would be banned for it.
    async fn check_deep_reorg_anchor(
        &self,
        sync_peer: &SyncPeer,
        best_block_header: &ChainHeader,
        chain_split_hash: HashOutput,
    ) -> Result<(), BlockHeaderSyncError> {
        let split_header = self
            .db
            .fetch_header_by_block_hash(chain_split_hash)
            .await?
            // The split hash is one of ours, so if it is gone our chain changed after we sent it
            .ok_or(BlockHeaderSyncError::ChainChangedDuringSync)?;
        let activation_height = self.consensus_rules.derived_monero_coinbase_activation_height();
        let local_tip_height = best_block_header.height();
        if !reorg_reintroduces_pre_ghsa_blocks(
            local_tip_height,
            split_header.height.saturating_add(1),
            activation_height,
        ) {
            return Ok(());
        }

        error!(
            target: LOG_TARGET,
            "REFUSED header sync from peer `{}` (GHSA-3qmx-q9pv-f3m4): their chain splits from ours at height {}, \
             below the activation height {} of the advisory's proof of work rules, while our tip is already at {}. \
             Blocks below that height are still validated with the pre-fork verifiers the advisory describes as \
             forgeable, so a chain rooted there can be minted at no proof of work cost. Refusing the rewind and \
             banning the peer. If you believe this is the honest chain, this node's database is on the wrong side \
             of the fork and has to be resynced from scratch.",
            sync_peer.node_id(),
            split_header.height,
            activation_height,
            local_tip_height,
        );
        Err(BlockHeaderSyncError::ReorgBelowGhsaActivation {
            split_height: split_header.height,
            activation_height,
            local_tip_height,
        })
    }

    /// Rewinds to the split to make way for the pending chain, unless that would reset the chain for a pending chain
    /// that is not stronger than everything we hold, or before the claim checks (`may_reset` false)
    async fn rewind_blockchain(
        &self,
        split_hash: HashOutput,
        may_reset: bool,
    ) -> Result<PendingChainRewind, BlockHeaderSyncError> {
        debug!(
            target: LOG_TARGET,
            "Deleting headers that no longer form part of the main chain up until split at {}",
            split_hash.to_hex()
        );
        let pending_tip = self
            .header_validator
            .current_valid_chain_tip_header()
            .cloned()
            .ok_or_else(|| {
                BlockHeaderSyncError::InvalidProtocolResponse("No pending headers to switch to".to_string())
            })?;
        let rewind = self
            .db
            .rewind_for_pending_chain(split_hash, pending_tip, may_reset)
            .await?;
        debug!(target: LOG_TARGET, "Rewind in preparation for header sync: {rewind:?}");
        Ok(rewind)
    }

    #[allow(clippy::too_many_lines)]
    async fn synchronize_headers(
        &mut self,
        mut sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        split_info: ChainSplitInfo,
        max_latency: Duration,
    ) -> Result<(), BlockHeaderSyncError> {
        info!(target: LOG_TARGET, "Starting header sync from peer {sync_peer}");
        const COMMIT_EVERY_N_HEADERS: usize = 1000;

        let mut has_switched_to_new_chain = false;
        // Set once the pending chain may replace ours only by resetting it, which waits for the claim checks
        let mut switch_after_claim_checks = false;
        // Set once the locked rewind refused a reset that the split impact missed, so it is not asked again
        let mut refused_locked = false;
        let pending_len = self.header_validator.valid_headers().len();

        // Find the hash to start syncing the rest of the headers.
        // The expectation cannot fail because there has been at least one valid header returned (checked in
        // determine_sync_status)
        let (start_header_height, start_header_hash, total_accumulated_difficulty) = self
            .header_validator
            .current_valid_chain_tip_header()
            .map(|h| (h.height(), *h.hash(), h.accumulated_data().total_accumulated_difficulty))
            .expect("synchronize_headers: expected there to be a valid tip header but it was None");

        let claimed_total_accumulated_diff = sync_peer.claimed_chain_metadata().accumulated_difficulty();
        // The split is fixed for this attempt; the rewind checks the same rule again under its lock
        let impact = self.split_impact(&split_info).await?;
        let verdict = self.pending_chain_verdict(&split_info, &impact);

        if pending_len < HEADER_SYNC_INITIAL_MAX_HEADERS {
            // Peer returned less than the max number of requested headers. This indicates that we have all the
            // available headers from the peer, so they must cover what the peer claimed before we commit them.
            if verdict == PendingChainVerdict::Weaker || total_accumulated_difficulty < claimed_total_accumulated_diff {
                debug!(target: LOG_TARGET, "No further headers to download");
                return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                    claimed: claimed_total_accumulated_diff,
                    actual: Some(total_accumulated_difficulty),
                    local: split_info
                        .best_block_header
                        .accumulated_data()
                        .total_accumulated_difficulty,
                });
            }
            // More difficulty than claimed at a lower height means the claim is stale (a reorg), not a lie
            let claimed_height = sync_peer.claimed_chain_metadata().best_block_height();
            if claimed_height > start_header_height && total_accumulated_difficulty <= claimed_total_accumulated_diff {
                return Err(BlockHeaderSyncError::PeerSentInaccurateChainHeight {
                    claimed: claimed_height,
                    actual: start_header_height,
                });
            }
            // The peer's claim holds, but our headers are stronger still
            if verdict == PendingChainVerdict::Refused {
                return Err(BlockHeaderSyncError::WeakerThanLocalHeaders);
            }
            debug!(
                target: LOG_TARGET,
                "Remote chain from peer {} has higher PoW. Switching",
                sync_peer.node_id()
            );
            // The claim checks above have passed
            if self.switch_to_pending_chain(&split_info, true).await? != PendingChainSwitch::Switched {
                return Err(BlockHeaderSyncError::WeakerThanLocalHeaders);
            }
            return Ok(());
        }

        // If we already have a stronger chain at this point, switch over to it.
        // just in case we happen to be exactly HEADER_SYNC_INITIAL_MAX_HEADERS headers behind.
        if verdict == PendingChainVerdict::Switch {
            debug!(
                target: LOG_TARGET,
                "Remote chain from peer {} has higher PoW. Switching",
                sync_peer.node_id()
            );
            // Otherwise the stream and the claim checks after it decide
            match self.switch_to_pending_chain(&split_info, false).await? {
                PendingChainSwitch::Switched => has_switched_to_new_chain = true,
                PendingChainSwitch::Refused => refused_locked = true,
                PendingChainSwitch::Deferred => switch_after_claim_checks = true,
            }
        }

        debug!(
            target: LOG_TARGET,
            "Download remaining headers starting from header #{} from peer `{}`",
            start_header_height,
            sync_peer.node_id()
        );
        let request = SyncHeadersRequest {
            start_hash: start_header_hash.to_vec(),
            // To the tip!
            count: 0,
        };

        let mut header_stream = client.sync_headers(request).await?;
        debug!(
            target: LOG_TARGET,
            "Reading headers from peer `{}`",
            sync_peer.node_id()
        );

        let mut last_sync_timer = Instant::now();

        let mut last_total_accumulated_difficulty = total_accumulated_difficulty;
        let mut avg_latency = RollingAverageTime::new(20);
        let mut prev_height: Option<u64> = None;
        let mut prev_hash = start_header_hash;
        while let Some(header) = header_stream.next().await {
            let latency = last_sync_timer.elapsed();
            avg_latency.add_sample(latency);
            let header = BlockHeader::try_from(header?).map_err(BlockHeaderSyncError::ReceivedInvalidHeader)?;
            debug!(
                target: LOG_TARGET,
                "Validating header #{} (Pow: {}) with hash: ({}). Latency: {:.2?}",
                header.height,
                header.pow_algo(),
                header.hash().to_hex(),
                latency
            );
            trace!(
                target: LOG_TARGET,
                "{header}"
            );
            if let Some(prev_header_height) = prev_height &&
                header.height != prev_header_height.saturating_add(1)
            {
                warn!(
                    target: LOG_TARGET,
                    "Received header #{} `{}` does not follow previous header",
                    header.height,
                    header.hash().to_hex()
                );
                return Err(BlockHeaderSyncError::ReceivedInvalidHeader(
                    "Header does not follow previous header".to_string(),
                ));
            }
            let existing_header = self.db.fetch_header_by_block_hash(header.hash()).await?;
            if let Some(h) = existing_header {
                let local_tip_hash = *self.db.get_chain_metadata().await?.best_block_hash();
                if header.prev_hash == prev_hash && local_tip_hash != self.attempt_start_block_hash {
                    debug!(
                        target: LOG_TARGET,
                        "Peer `{}` sent header #{} that we added since sync started", sync_peer.node_id(), h.height
                    );
                    return Err(BlockHeaderSyncError::ChainChangedDuringSync);
                }
                warn!(
                    target: LOG_TARGET,
                    "Received header #{} `{}` that we already have.",
                    h.height,
                    h.hash().to_hex()
                );
                return Err(BlockHeaderSyncError::ReceivedInvalidHeader(
                    "Header already in database".to_string(),
                ));
            }
            let current_height = header.height;
            prev_hash = header.hash();
            last_total_accumulated_difficulty = self.header_validator.validate(header).await?;

            if has_switched_to_new_chain {
                // If we've switched to the new chain, we simply commit every COMMIT_EVERY_N_HEADERS headers
                if self.header_validator.valid_headers().len() >= COMMIT_EVERY_N_HEADERS {
                    self.commit_pending_headers().await?;
                }
            } else {
                // The remote chain has not (yet) been accepted.
                // We check the tip difficulties, switching over to the new chain if a higher accumulated difficulty is
                // achieved.
                if !switch_after_claim_checks &&
                    !refused_locked &&
                    self.pending_chain_verdict(&split_info, &impact) == PendingChainVerdict::Switch
                {
                    match self.switch_to_pending_chain(&split_info, false).await? {
                        PendingChainSwitch::Switched => has_switched_to_new_chain = true,
                        PendingChainSwitch::Refused => refused_locked = true,
                        PendingChainSwitch::Deferred => switch_after_claim_checks = true,
                    }
                }
                // A chain that can only replace ours by resetting it is never committed, so only its tip is kept
                if switch_after_claim_checks || refused_locked || impact.reset_reference.is_some() {
                    self.header_validator.keep_only_tip();
                }
            }

            sync_peer.set_latency(latency);
            sync_peer.add_sample(last_sync_timer.elapsed());
            self.hooks.call_on_progress_header_hooks(
                current_height,
                sync_peer.claimed_chain_metadata().best_block_height(),
                &sync_peer,
            );

            let last_avg_latency = avg_latency.calculate_average_with_min_samples(5);
            if let Some(avg_latency) = last_avg_latency &&
                avg_latency > max_latency
            {
                return Err(BlockHeaderSyncError::MaxLatencyExceeded {
                    peer: sync_peer.node_id().clone(),
                    latency: avg_latency,
                    max_latency,
                });
            }

            last_sync_timer = Instant::now();
            prev_height = Some(current_height);
        }

        let mut refused = false;
        if !has_switched_to_new_chain {
            let best_local_before_sync = split_info
                .best_block_header
                .accumulated_data()
                .total_accumulated_difficulty;
            match self
                .header_validator
                .current_valid_chain_tip_header()
                .map(|h| h.accumulated_data().total_accumulated_difficulty)
            {
                Some(validated_total_accumulated_diff) => {
                    if claimed_total_accumulated_diff > validated_total_accumulated_diff {
                        // Over-claim: peer advertised more PoW than their headers actually provide.
                        return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                            claimed: claimed_total_accumulated_diff,
                            actual: Some(validated_total_accumulated_diff),
                            local: best_local_before_sync,
                        });
                    }
                    let mut verdict = self.pending_chain_verdict(&split_info, &impact);
                    if switch_after_claim_checks {
                        verdict = PendingChainVerdict::SwitchAfterClaimChecks;
                    }
                    if refused_locked {
                        verdict = PendingChainVerdict::Refused;
                    }
                    if verdict == PendingChainVerdict::Switch {
                        match self.switch_to_pending_chain(&split_info, false).await? {
                            PendingChainSwitch::Switched => {},
                            PendingChainSwitch::Refused => verdict = PendingChainVerdict::Refused,
                            PendingChainSwitch::Deferred => verdict = PendingChainVerdict::SwitchAfterClaimChecks,
                        }
                    }
                    if verdict == PendingChainVerdict::Switch {
                        has_switched_to_new_chain = true;
                        info!(
                            target: LOG_TARGET,
                            "Received PoW from peer exceeds local tip. Before sync: {}, received: {}. Committed.",
                            best_local_before_sync,
                            validated_total_accumulated_diff,
                        );
                    } else if verdict == PendingChainVerdict::SwitchAfterClaimChecks {
                        switch_after_claim_checks = true;
                    } else if verdict == PendingChainVerdict::Refused {
                        // Decided after the claim checks below
                        refused = true;
                    } else {
                        // We have a stronger chain, so we do not commit the headers.
                        debug!(
                            target: LOG_TARGET,
                            "Not committing headers as we have a stronger chain, ours: {} theirs: {}",
                            best_local_before_sync,
                            validated_total_accumulated_diff,
                        );
                    };
                },
                None => {
                    // No validated headers at this stage, but there should have been.
                    return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                        claimed: claimed_total_accumulated_diff,
                        actual: None,
                        local: best_local_before_sync,
                    });
                },
            }
        }

        // Commit the last blocks only if we have switched to the new chain.
        if has_switched_to_new_chain && !self.header_validator.valid_headers().is_empty() {
            self.commit_pending_headers().await?;
        }

        // This rule is strict: if the peer advertised a higher PoW than they were able to provide (without
        // some other external factor like a disconnect etc), we detect the and ban the peer.
        if last_total_accumulated_difficulty < claimed_total_accumulated_diff {
            return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                claimed: claimed_total_accumulated_diff,
                actual: Some(last_total_accumulated_difficulty),
                local: split_info
                    .best_block_header
                    .accumulated_data()
                    .total_accumulated_difficulty,
            });
        }
        // Same strict rule for the claimed height, unless more difficulty than claimed makes the claim stale
        let claimed_height = sync_peer.claimed_chain_metadata().best_block_height();
        let validated_height = prev_height.unwrap_or(start_header_height);
        if claimed_height > validated_height && last_total_accumulated_difficulty <= claimed_total_accumulated_diff {
            return Err(BlockHeaderSyncError::PeerSentInaccurateChainHeight {
                claimed: claimed_height,
                actual: validated_height,
            });
        }
        // The peer's claim holds, but our headers are stronger still
        if refused {
            return Err(BlockHeaderSyncError::WeakerThanLocalHeaders);
        }
        // The peer's claim holds, so its chain may now reset ours
        if switch_after_claim_checks &&
            self.switch_to_pending_chain(&split_info, true).await? != PendingChainSwitch::Switched
        {
            return Err(BlockHeaderSyncError::WeakerThanLocalHeaders);
        }

        Ok(())
    }

    async fn commit_pending_headers(&mut self) -> Result<ChainHeader, BlockHeaderSyncError> {
        let chain_headers = self.header_validator.take_valid_headers();
        let num_headers = chain_headers.len();
        let start = Instant::now();

        let new_tip = chain_headers.last().cloned().unwrap();
        let mut txn = self.db.write_transaction();
        chain_headers.into_iter().for_each(|chain_header| {
            txn.insert_chain_header(chain_header);
        });

        txn.commit().await?;

        debug!(
            target: LOG_TARGET,
            "{} header(s) committed (tip = {}) to the blockchain db in {:.2?}",
            num_headers,
            new_tip.height(),
            start.elapsed()
        );

        Ok(new_tip)
    }

    fn pending_chain_has_higher_pow(&self, current_tip: &ChainHeader) -> bool {
        let chain_headers = self.header_validator.valid_headers();
        if chain_headers.is_empty() {
            return false;
        }

        // Check that the remote tip is stronger than the local tip, equal should not have ended up here, so we treat
        // equal as less
        let proposed_tip = chain_headers.last().unwrap();
        self.header_validator.compare_chains(current_tip, proposed_tip).is_lt()
    }

    /// What switching to a chain from this split would do to ours
    async fn split_impact(&self, split_info: &ChainSplitInfo) -> Result<SplitImpact, BlockHeaderSyncError> {
        #[cfg(test)]
        if self.stale_split_impact {
            return Ok(SplitImpact { reset_reference: None });
        }
        if split_info.reorg_steps_back == 0 {
            return Ok(SplitImpact { reset_reference: None });
        }
        Ok(SplitImpact {
            reset_reference: self.db.reset_reference(split_info.chain_split_hash).await?,
        })
    }

    /// Whether to switch to the pending chain. It has to be stronger than our block tip, and if switching would reset
    /// this pruned node to the genesis block, stronger than everything we hold too, and only after the peer's claim
    /// checks.
    fn pending_chain_verdict(&self, split_info: &ChainSplitInfo, impact: &SplitImpact) -> PendingChainVerdict {
        if !self.pending_chain_has_higher_pow(&split_info.best_block_header) {
            return PendingChainVerdict::Weaker;
        }
        match &impact.reset_reference {
            Some(reference) if !self.pending_chain_has_higher_pow(reference) => PendingChainVerdict::Refused,
            Some(_) => PendingChainVerdict::SwitchAfterClaimChecks,
            None => PendingChainVerdict::Switch,
        }
    }

    /// Switches to the pending chain, changing nothing if it is refused, or if it would reset the chain and `may_reset`
    /// is false.
    async fn switch_to_pending_chain(
        &mut self,
        split_info: &ChainSplitInfo,
        may_reset: bool,
    ) -> Result<PendingChainSwitch, BlockHeaderSyncError> {
        // Stop accepting propagated blocks while we swap to the peer's headers, then make sure no block that
        // landed during the attempt has made our chain the stronger one.
        self.db.inner().set_disable_add_block_flag();
        let local_metadata = self.db.get_chain_metadata().await?;
        let local_tip = self.db.fetch_chain_header(local_metadata.best_block_height()).await?;
        if !self.pending_chain_has_higher_pow(&local_tip) {
            self.db.inner().clear_disable_add_block_flag();
            debug!(
                target: LOG_TARGET,
                "Not switching to the peer's chain: our chain became stronger during the sync attempt"
            );
            return Err(BlockHeaderSyncError::ChainChangedDuringSync);
        }
        // Decide the rewind from the current header tip, which may have moved since the split was found
        let split_hash = split_info.chain_split_hash;
        if self.db.fetch_header_by_block_hash(split_hash).await?.is_none() {
            self.db.inner().clear_disable_add_block_flag();
            debug!(
                target: LOG_TARGET,
                "Not switching to the peer's chain: the split point {} is no longer on our chain",
                split_hash.to_hex()
            );
            return Err(BlockHeaderSyncError::ChainChangedDuringSync);
        }
        if *self.db.fetch_last_chain_header().await?.hash() != split_hash {
            debug!(
                target: LOG_TARGET,
                "Reorg: Rewinding the chain to the split at {}",
                split_hash.to_hex()
            );
            #[cfg(test)]
            {
                self.rewind_attempts += 1;
            }
            match self.rewind_blockchain(split_info.chain_split_hash, may_reset).await? {
                PendingChainRewind::Rewound(blocks) => {
                    if !blocks.is_empty() {
                        self.hooks.call_on_rewind_hooks(blocks);
                    }
                },
                // A reset drops the split unless it is the genesis block, and every peer in this round has a stale
                // view of our chain
                PendingChainRewind::Reset(blocks) => {
                    return Err(BlockHeaderSyncError::ChainResetToGenesis { removed: blocks });
                },
                PendingChainRewind::Refused => {
                    self.db.inner().clear_disable_add_block_flag();
                    return Ok(PendingChainSwitch::Refused);
                },
                PendingChainRewind::WouldReset => {
                    self.db.inner().clear_disable_add_block_flag();
                    return Ok(PendingChainSwitch::Deferred);
                },
            }
        }

        // Commit the forked chain. At this point
        // 1. Headers have been validated
        // 2. The forked chain has a higher PoW than the local chain
        //
        // After this we commit headers every `n` blocks
        self.commit_pending_headers().await?;

        Ok(PendingChainSwitch::Switched)
    }

    fn attempt_budget(&self) -> Duration {
        self.config.rpc_deadline.saturating_mul(2)
    }

    /// Records that an attempt with the peer ended because our chain changed, and returns the error to act on: a
    /// ban once the peer has done this too often.
    fn record_chain_change(&mut self, node_id: &NodeId) -> BlockHeaderSyncError {
        let count = record_recent_outcome(self.chain_changes, node_id);
        if count >= MAX_CHAIN_CHANGES_PER_PEER {
            warn!(
                target: LOG_TARGET,
                "Sync attempts with peer `{node_id}` ended {count} times in {RECENT_OUTCOME_WINDOW:.0?} because our \
                 chain changed. Peer will be banned.",
            );
            return BlockHeaderSyncError::RepeatedChainChangesDuringSync { count };
        }
        BlockHeaderSyncError::ChainChangedDuringSync
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
}

/// Waits on the dial, RPC setup or a chain-split request within what is left of the attempt's `budget`, adding the
/// wait to `spent`. Only these network waits count against a peer; the header stream and local database work do not.
async fn within_budget<F: Future>(
    spent: &mut Duration,
    budget: Duration,
    fut: F,
) -> Result<F::Output, BlockHeaderSyncError> {
    let remaining = budget.saturating_sub(*spent);
    let started = Instant::now();
    let result = tokio::time::timeout(remaining, fut).await;
    *spent = spent.saturating_add(started.elapsed());
    result.map_err(|_| BlockHeaderSyncError::SyncAttemptTimedOut { budget })
}

/// Whether the pending chain should replace ours
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingChainVerdict {
    Switch,
    /// Stronger than our chain, but switching resets it to the genesis block, so only once the peer's claim has been
    /// checked
    SwitchAfterClaimChecks,
    /// Not stronger than our block tip
    Weaker,
    /// Stronger than our block tip, but switching would reset the chain and it is not stronger than everything we hold
    Refused,
}

/// What switching to a chain from the split would do to ours. Fixed for a sync attempt.
struct SplitImpact {
    /// The chain the pending chain has to beat, if switching would reset this pruned node to the genesis block
    reset_reference: Option<ChainHeader>,
}

/// What `switch_to_pending_chain` did
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingChainSwitch {
    Switched,
    /// Nothing changed: switching would reset the chain, and the pending chain is not stronger than everything we hold
    Refused,
    /// Nothing changed: switching would reset the chain, which has to wait for the peer's claim checks
    Deferred,
}

#[derive(Debug, Clone)]
struct FindChainSplitResult {
    reorg_steps_back: u64,
    peer_headers: Vec<ProtoBlockHeader>,
    peer_fork_hash_index: u64,
    chain_split_hash: HashOutput,
    /// Every hash we sent the peer while looking for the split
    sent_hashes: Vec<HashOutput>,
}

/// Information about the chain split from the remote node.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainSplitInfo {
    /// The best block's header on the local chain.
    pub best_block_header: ChainHeader,
    /// The number of blocks to reorg back to the fork.
    pub reorg_steps_back: u64,
    /// The hash of the block at the fork.
    pub chain_split_hash: HashOutput,
}

/// The result of an attempt to synchronize headers with a peer.
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptSyncResult {
    /// The number of headers that were returned.
    pub headers_returned: u64,
    /// The fork hash index of the remote peer.
    pub peer_fork_hash_index: u64,
    /// The header sync status.
    pub header_sync_status: HeaderSyncStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeaderSyncStatus {
    /// Local and remote node are in sync or ahead
    InSyncOrAhead,
    /// Local node is lagging behind remote node
    Lagging(Box<ChainSplitInfo>),
}

#[cfg(test)]
mod test {
    #![allow(clippy::arithmetic_side_effects)]
    #![allow(clippy::indexing_slicing)]
    use primitive_types::U512;
    use tari_common_types::types::FixedHash;
    use tari_comms::{
        peer_manager::PeerFeatures,
        protocol::rpc::{NamedProtocolService, mock::MockRpcServer},
        test_utils::{mocks::create_connectivity_mock, node_identity::build_node_identity},
    };
    use tari_service_framework::reply_channel;
    use tari_utilities::epoch_time::EpochTime;
    use tokio::sync::broadcast;

    use super::*;
    use crate::{
        base_node::{LocalNodeCommsInterface, chain_metadata_service::PeerChainMetadata},
        chain_storage::{BlockchainDatabase, BlockchainDatabaseConfig, DbTransaction, Validators},
        test_helpers::{
            blockchain::{TempDatabase, create_main_chain, create_store_with_consensus_and_validators_and_config},
            create_consensus_rules,
        },
        validation::{ValidationError, mocks::MockValidator},
    };

    /// A pruned chain of five blocks with its pruned height at 2, and the split at block 1 below it
    fn create_pruned_chain() -> (
        BlockchainDatabase<TempDatabase>,
        BaseNodeConsensusManager,
        ChainSplitInfo,
    ) {
        let rules = create_consensus_rules();
        let validators = Validators::new(
            MockValidator::new(true),
            MockValidator::new(true),
            MockValidator::new(true),
        );
        let config = BlockchainDatabaseConfig {
            pruning_horizon: 2,
            pruning_interval: 1,
            ..Default::default()
        };
        let db = create_store_with_consensus_and_validators_and_config(rules.clone(), validators, config);
        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"], ["D->C"], ["E->D"]));
        let metadata = db.get_chain_metadata().unwrap();
        assert_eq!(metadata.pruned_height(), 2);
        let split_info = ChainSplitInfo {
            best_block_header: db.fetch_tip_header().unwrap(),
            reorg_steps_back: metadata.best_block_height() - 1,
            chain_split_hash: *chain.get("A").unwrap().hash(),
        };
        (db, rules, split_info)
    }

    /// Validates the next header on the validator's chain
    async fn validate_next_header(validator: &mut BlockHeaderSyncValidator<TempDatabase>, prev: &BlockHeader) {
        let mut next = BlockHeader::from_previous(prev);
        next.timestamp = prev.timestamp.checked_add(EpochTime::from(1)).unwrap();
        // Unique keys for the database's MMR size indexes when the headers are stored
        next.kernel_mmr_size += 1;
        next.output_smt_size += 1;
        validator.validate(next).await.unwrap();
    }

    /// Validates `n` headers on top of the tip of `db` and stores them
    async fn add_headers(db: &BlockchainDatabase<TempDatabase>, rules: &BaseNodeConsensusManager, n: usize) {
        let mut validator = BlockHeaderSyncValidator::new(db.clone().into(), rules.clone(), RandomXFactory::default());
        let tip = db.fetch_last_chain_header().unwrap();
        validator.initialize_state(tip.hash()).await.unwrap();
        let mut prev = tip.header().clone();
        for _ in 0..n {
            validate_next_header(&mut validator, &prev).await;
            prev = validator.valid_headers().last().unwrap().header().clone();
        }
        db.insert_valid_headers(validator.take_valid_headers()).unwrap();
    }

    fn new_synchronizer<'a>(
        db: &BlockchainDatabase<TempDatabase>,
        rules: &BaseNodeConsensusManager,
        sync_peers: &'a mut Vec<SyncPeer>,
        metadata: &'a ChainMetadata,
    ) -> HeaderSynchronizer<'a, TempDatabase> {
        let (connectivity, _mock) = create_connectivity_mock();
        HeaderSynchronizer::new(
            BlockchainSyncConfig::default(),
            AsyncBlockchainDb::new(db.clone()),
            rules.clone(),
            connectivity,
            sync_peers,
            RandomXFactory::default(),
            metadata,
        )
    }

    /// Grows a chain of headers on `split_hash`, one at a time up to `max_len`, and returns it with the verdict on
    /// each length
    async fn grow_pending_chain(
        db: &BlockchainDatabase<TempDatabase>,
        rules: &BaseNodeConsensusManager,
        split_info: &ChainSplitInfo,
        max_len: usize,
    ) -> (Vec<ChainHeader>, Vec<PendingChainVerdict>) {
        let metadata = db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![];
        let mut synchronizer = new_synchronizer(db, rules, &mut sync_peers, &metadata);
        synchronizer
            .header_validator
            .initialize_state(&split_info.chain_split_hash)
            .await
            .unwrap();
        let mut prev = db
            .fetch_header_by_block_hash(split_info.chain_split_hash)
            .unwrap()
            .unwrap();
        let impact = synchronizer.split_impact(split_info).await.unwrap();
        let mut verdicts = Vec::new();
        for _ in 0..max_len {
            validate_next_header(&mut synchronizer.header_validator, &prev).await;
            prev = synchronizer
                .header_validator
                .valid_headers()
                .last()
                .unwrap()
                .header()
                .clone();
            verdicts.push(synchronizer.pending_chain_verdict(split_info, &impact));
        }
        (synchronizer.header_validator.take_valid_headers(), verdicts)
    }

    type SyncRpcServer = MockRpcServer<rpc::BaseNodeSyncRpcServer<rpc::BaseNodeSyncRpcService<TempDatabase>>>;

    /// Serves `peer_db` to a client over a mock connection, with the peer claiming the given chain
    async fn connect_to_peer(
        peer_db: &BlockchainDatabase<TempDatabase>,
        claimed_height: u64,
        claimed_difficulty: U512,
    ) -> (rpc::BaseNodeSyncRpcClient, SyncPeer, SyncRpcServer, PeerConnection) {
        let (req_tx, _) = reply_channel::unbounded();
        let (block_tx, _) = reply_channel::unbounded();
        let (block_event_tx, _) = broadcast::channel(1);
        let service = rpc::BaseNodeSyncRpcServer::new(rpc::BaseNodeSyncRpcService::new(
            peer_db.clone().into(),
            LocalNodeCommsInterface::new(req_tx, block_tx, block_event_tx),
            10,
        ));
        let protocol = service.as_protocol_name();
        let peer_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let mut server = MockRpcServer::new(service, peer_identity.clone());
        server.serve();
        let our_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let mut conn = server.create_connection(our_identity.to_peer(), protocol.into()).await;
        let client = conn.connect_rpc::<rpc::BaseNodeSyncRpcClient>().await.unwrap();
        let claim = ChainMetadata::new(claimed_height, FixedHash::zero(), 0, 0, claimed_difficulty, 0).unwrap();
        let sync_peer = SyncPeer::from(PeerChainMetadata::new(peer_identity.node_id().clone(), claim, None));
        (client, sync_peer, server, conn)
    }

    /// A pruned chain of 5 blocks with 8 validated headers above it, and a peer holding headers from the genesis
    /// block that are stronger than our blocks but not our headers
    async fn create_peer_weaker_than_our_headers() -> (
        BlockchainDatabase<TempDatabase>,
        BaseNodeConsensusManager,
        BlockchainDatabase<TempDatabase>,
        ChainHeader,
    ) {
        let (db, rules, mut split_info) = create_pruned_chain();
        add_headers(&db, &rules, 8).await;
        split_info.chain_split_hash = *db.fetch_genesis_block().hash();
        let (headers, verdicts) = grow_pending_chain(&db, &rules, &split_info, 20).await;
        let len = verdicts
            .iter()
            .position(|verdict| *verdict == PendingChainVerdict::Refused)
            .unwrap() +
            1;
        let peer_db = create_store_with_consensus_and_validators_and_config(
            rules.clone(),
            Validators::new(
                MockValidator::new(true),
                MockValidator::new(true),
                MockValidator::new(true),
            ),
            BlockchainDatabaseConfig::default(),
        );
        let headers = headers.into_iter().take(len).collect::<Vec<_>>();
        let peer_tip = headers.last().unwrap().clone();
        peer_db.insert_valid_headers(headers).unwrap();
        (db, rules, peer_db, peer_tip)
    }

    async fn sync_from_peer(
        db: &BlockchainDatabase<TempDatabase>,
        rules: &BaseNodeConsensusManager,
        peer_db: &BlockchainDatabase<TempDatabase>,
        claimed_height: u64,
        claimed_difficulty: U512,
    ) -> Result<AttemptSyncResult, BlockHeaderSyncError> {
        let metadata = db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![];
        let mut synchronizer = new_synchronizer(db, rules, &mut sync_peers, &metadata);
        // The server and connection have to outlive the sync
        let (client, sync_peer, _server, _conn) = connect_to_peer(peer_db, claimed_height, claimed_difficulty).await;
        synchronizer
            .attempt_sync(&sync_peer, client, Duration::from_secs(60))
            .await
    }

    #[tokio::test]
    async fn a_peer_weaker_than_our_headers_is_refused_without_a_ban() {
        let (db, rules, peer_db, peer_tip) = create_peer_weaker_than_our_headers().await;
        let metadata = db.get_chain_metadata().unwrap();

        let err = sync_from_peer(
            &db,
            &rules,
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::WeakerThanLocalHeaders), "{err}");
        assert!(err.get_ban_reason().is_none());
        assert_eq!(db.get_chain_metadata().unwrap(), metadata);
        assert_eq!(db.fetch_last_header().unwrap().height, 13);
    }

    #[tokio::test]
    async fn a_peer_weaker_than_our_headers_that_inflates_its_height_is_banned() {
        let (db, rules, peer_db, peer_tip) = create_peer_weaker_than_our_headers().await;

        let err = sync_from_peer(
            &db,
            &rules,
            &peer_db,
            peer_tip.height() + 5,
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, BlockHeaderSyncError::PeerSentInaccurateChainHeight { .. }),
            "{err}"
        );
        assert!(err.get_ban_reason().is_some());
        assert_eq!(db.fetch_last_header().unwrap().height, 13);
    }

    #[tokio::test]
    async fn a_peer_weaker_than_our_headers_that_inflates_its_work_is_banned() {
        let (db, rules, peer_db, peer_tip) = create_peer_weaker_than_our_headers().await;

        let err = sync_from_peer(
            &db,
            &rules,
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty + 1,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, BlockHeaderSyncError::PeerSentInaccurateChainMetadata { .. }),
            "{err}"
        );
        assert!(err.get_ban_reason().is_some());
        assert_eq!(db.fetch_last_header().unwrap().height, 13);
    }

    /// A peer with a full first batch of headers, so the decision is made after streaming the rest
    #[tokio::test]
    async fn a_long_peer_chain_weaker_than_our_headers_is_refused_without_a_ban() {
        let (db, rules, mut split_info) = create_pruned_chain();
        add_headers(&db, &rules, HEADER_SYNC_INITIAL_MAX_HEADERS + 10).await;
        let last_header_height = db.fetch_last_header().unwrap().height;
        let metadata = db.get_chain_metadata().unwrap();
        split_info.chain_split_hash = *db.fetch_genesis_block().hash();
        let (headers, verdicts) = grow_pending_chain(&db, &rules, &split_info, HEADER_SYNC_INITIAL_MAX_HEADERS).await;
        assert_eq!(verdicts.last(), Some(&PendingChainVerdict::Refused));
        let peer_db = create_store_with_consensus_and_validators_and_config(
            rules.clone(),
            Validators::new(
                MockValidator::new(true),
                MockValidator::new(true),
                MockValidator::new(true),
            ),
            BlockchainDatabaseConfig::default(),
        );
        let peer_tip = headers.last().unwrap().clone();
        peer_db.insert_valid_headers(headers).unwrap();

        let err = sync_from_peer(
            &db,
            &rules,
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::WeakerThanLocalHeaders), "{err}");
        assert!(err.get_ban_reason().is_none());
        assert_eq!(db.get_chain_metadata().unwrap(), metadata);
        assert_eq!(db.fetch_last_header().unwrap().height, last_header_height);
    }

    /// A pruned chain of 5 blocks with more validated headers above it than fit in a batch, and a peer serving
    /// `peer_len` headers from the genesis block as its chain. Headers beyond the first batch are streamed. Returns the
    /// verdict on each length of the peer's chain.
    async fn create_streaming_peer(
        peer_len: usize,
    ) -> (
        BlockchainDatabase<TempDatabase>,
        BaseNodeConsensusManager,
        BlockchainDatabase<TempDatabase>,
        ChainHeader,
        Vec<PendingChainVerdict>,
    ) {
        let (db, rules, mut split_info) = create_pruned_chain();
        add_headers(&db, &rules, HEADER_SYNC_INITIAL_MAX_HEADERS + 10).await;
        split_info.chain_split_hash = *db.fetch_genesis_block().hash();
        let (headers, verdicts) = grow_pending_chain(&db, &rules, &split_info, peer_len).await;
        // Stronger than our blocks but not our headers from the first batch on
        assert_eq!(
            verdicts[HEADER_SYNC_INITIAL_MAX_HEADERS - 1],
            PendingChainVerdict::Refused
        );
        let peer_db = create_store_with_consensus_and_validators_and_config(
            rules.clone(),
            Validators::new(
                MockValidator::new(true),
                MockValidator::new(true),
                MockValidator::new(true),
            ),
            BlockchainDatabaseConfig::default(),
        );
        let peer_tip = headers.last().unwrap().clone();
        peer_db.insert_valid_headers(headers).unwrap();
        // The peer serves headers up to its best block
        let mut txn = DbTransaction::new();
        txn.set_best_block(
            peer_tip.height(),
            *peer_tip.hash(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
            *peer_db.fetch_genesis_block().hash(),
            peer_tip.timestamp(),
        );
        peer_db.write(txn).unwrap();
        (db, rules, peer_db, peer_tip, verdicts)
    }

    #[tokio::test]
    async fn a_streamed_peer_chain_weaker_than_our_headers_is_refused_without_a_ban() {
        let (db, rules, peer_db, peer_tip, verdicts) = create_streaming_peer(HEADER_SYNC_INITIAL_MAX_HEADERS + 5).await;
        assert_eq!(verdicts.last(), Some(&PendingChainVerdict::Refused));
        let metadata = db.get_chain_metadata().unwrap();
        let last_header_height = db.fetch_last_header().unwrap().height;

        let err = sync_from_peer(
            &db,
            &rules,
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::WeakerThanLocalHeaders), "{err}");
        assert!(err.get_ban_reason().is_none());
        assert_eq!(db.get_chain_metadata().unwrap(), metadata);
        assert_eq!(db.fetch_last_header().unwrap().height, last_header_height);
    }

    /// A chain that could only replace ours by resetting it is never committed, so its headers are not kept
    #[tokio::test]
    async fn a_streamed_peer_chain_that_would_reset_us_is_not_kept_in_memory() {
        let (db, rules, peer_db, peer_tip, _) = create_streaming_peer(HEADER_SYNC_INITIAL_MAX_HEADERS + 5).await;
        let metadata = db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![];
        let mut synchronizer = new_synchronizer(&db, &rules, &mut sync_peers, &metadata);
        let (client, sync_peer, _server, _conn) = connect_to_peer(
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await;

        let err = synchronizer
            .attempt_sync(&sync_peer, client, Duration::from_secs(60))
            .await
            .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::WeakerThanLocalHeaders), "{err}");
        assert_eq!(synchronizer.header_validator.valid_headers().len(), 1);
        assert_eq!(
            synchronizer
                .header_validator
                .current_valid_chain_tip_header()
                .unwrap()
                .hash(),
            peer_tip.hash()
        );
    }

    /// The pruned height can move after the split impact is worked out, so that the locked rewind refuses a reset the
    /// fast path did not expect. It is asked once, and the chain is then handled as refused.
    #[tokio::test]
    async fn a_reset_the_split_impact_missed_is_refused_once() {
        let (db, rules, peer_db, peer_tip, _) = create_streaming_peer(HEADER_SYNC_INITIAL_MAX_HEADERS + 5).await;
        let metadata = db.get_chain_metadata().unwrap();
        let last_header_height = db.fetch_last_header().unwrap().height;
        let mut sync_peers = vec![];
        let mut synchronizer = new_synchronizer(&db, &rules, &mut sync_peers, &metadata);
        synchronizer.stale_split_impact = true;
        let (client, sync_peer, _server, _conn) = connect_to_peer(
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await;

        let err = synchronizer
            .attempt_sync(&sync_peer, client, Duration::from_secs(60))
            .await
            .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::WeakerThanLocalHeaders), "{err}");
        assert!(err.get_ban_reason().is_none());
        assert_eq!(synchronizer.rewind_attempts, 1);
        assert_eq!(synchronizer.header_validator.valid_headers().len(), 1);
        assert_eq!(db.get_chain_metadata().unwrap(), metadata);
        assert_eq!(db.fetch_last_header().unwrap().height, last_header_height);
    }

    #[tokio::test]
    async fn a_streamed_peer_chain_weaker_than_our_headers_that_inflates_its_claim_is_banned() {
        let (db, rules, peer_db, peer_tip, _) = create_streaming_peer(HEADER_SYNC_INITIAL_MAX_HEADERS + 5).await;
        let difficulty = peer_tip.accumulated_data().total_accumulated_difficulty;

        let err = sync_from_peer(&db, &rules, &peer_db, peer_tip.height() + 5, difficulty)
            .await
            .unwrap_err();
        assert!(
            matches!(err, BlockHeaderSyncError::PeerSentInaccurateChainHeight { .. }),
            "{err}"
        );
        assert!(err.get_ban_reason().is_some());

        let err = sync_from_peer(&db, &rules, &peer_db, peer_tip.height(), difficulty + 1)
            .await
            .unwrap_err();
        assert!(
            matches!(err, BlockHeaderSyncError::PeerSentInaccurateChainMetadata { .. }),
            "{err}"
        );
        assert!(err.get_ban_reason().is_some());
    }

    #[tokio::test]
    async fn a_streamed_peer_chain_that_beats_our_headers_resets_the_chain_and_ends_header_sync() {
        let (db, rules, peer_db, peer_tip, verdicts) =
            create_streaming_peer(HEADER_SYNC_INITIAL_MAX_HEADERS + 30).await;
        // It only beats our header tip after the first batch
        assert_eq!(verdicts.last(), Some(&PendingChainVerdict::SwitchAfterClaimChecks));

        let err = sync_from_peer(
            &db,
            &rules,
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::ChainResetToGenesis { .. }), "{err}");
        assert!(err.get_ban_reason().is_none());
        assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), 0);
    }

    #[tokio::test]
    async fn a_streamed_peer_chain_that_would_reset_us_but_inflates_its_claim_is_banned_before_the_reset() {
        let (db, rules, peer_db, peer_tip, verdicts) =
            create_streaming_peer(HEADER_SYNC_INITIAL_MAX_HEADERS + 30).await;
        assert_eq!(verdicts.last(), Some(&PendingChainVerdict::SwitchAfterClaimChecks));
        let metadata = db.get_chain_metadata().unwrap();
        let last_header_height = db.fetch_last_header().unwrap().height;
        let difficulty = peer_tip.accumulated_data().total_accumulated_difficulty;

        let err = sync_from_peer(&db, &rules, &peer_db, peer_tip.height() + 5, difficulty)
            .await
            .unwrap_err();
        assert!(
            matches!(err, BlockHeaderSyncError::PeerSentInaccurateChainHeight { .. }),
            "{err}"
        );
        assert!(err.get_ban_reason().is_some());

        let err = sync_from_peer(&db, &rules, &peer_db, peer_tip.height(), difficulty + 1)
            .await
            .unwrap_err();
        assert!(
            matches!(err, BlockHeaderSyncError::PeerSentInaccurateChainMetadata { .. }),
            "{err}"
        );
        assert!(err.get_ban_reason().is_some());

        assert_eq!(db.get_chain_metadata().unwrap(), metadata);
        assert_eq!(db.fetch_last_header().unwrap().height, last_header_height);
    }

    /// A peer one header past our block tip, with as much work as the one header we hold there whose body was
    /// withheld, replaces it, and our header is remembered for any later reset
    #[tokio::test]
    async fn a_peer_chain_stronger_than_our_blocks_replaces_our_unbacked_header() {
        let (db, rules, _) = create_pruned_chain();
        add_headers(&db, &rules, 1).await;
        let our_header = db.fetch_last_chain_header().unwrap();
        let peer_db = create_store_with_consensus_and_validators_and_config(
            rules.clone(),
            Validators::new(
                MockValidator::new(true),
                MockValidator::new(true),
                MockValidator::new(true),
            ),
            BlockchainDatabaseConfig::default(),
        );
        let our_blocks = (1..=5).map(|height| db.fetch_chain_header(height).unwrap()).collect();
        peer_db.insert_valid_headers(our_blocks).unwrap();
        // One header on our block tip that differs from our own next header
        let mut validator =
            BlockHeaderSyncValidator::new(peer_db.clone().into(), rules.clone(), RandomXFactory::default());
        let split = peer_db.fetch_last_chain_header().unwrap();
        validator.initialize_state(split.hash()).await.unwrap();
        let mut next = BlockHeader::from_previous(split.header());
        next.timestamp = split.header().timestamp.checked_add(EpochTime::from(2)).unwrap();
        next.kernel_mmr_size += 1;
        next.output_smt_size += 1;
        validator.validate(next).await.unwrap();
        peer_db.insert_valid_headers(validator.take_valid_headers()).unwrap();
        let peer_tip = peer_db.fetch_last_chain_header().unwrap();

        sync_from_peer(
            &db,
            &rules,
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await
        .unwrap();

        assert_eq!(db.fetch_last_header().unwrap().hash(), *peer_tip.hash());
        assert_eq!(
            db.db_read_access().unwrap().fetch_lost_header_tip().unwrap(),
            Some(our_header)
        );
    }

    #[tokio::test]
    async fn a_peer_chain_through_a_bad_block_is_refused_with_a_ban() {
        let (db, rules, mut split_info) = create_pruned_chain();
        split_info.chain_split_hash = *db.fetch_genesis_block().hash();
        let (headers, _) = grow_pending_chain(&db, &rules, &split_info, 10).await;
        // A block of the peer's chain whose body we found invalid
        let bad = headers[7].clone();
        db.insert_bad_body(*bad.hash(), bad.height(), "test".to_string())
            .unwrap();
        let peer_db = create_store_with_consensus_and_validators_and_config(
            rules.clone(),
            Validators::new(
                MockValidator::new(true),
                MockValidator::new(true),
                MockValidator::new(true),
            ),
            BlockchainDatabaseConfig::default(),
        );
        let peer_tip = headers.last().unwrap().clone();
        peer_db.insert_valid_headers(headers).unwrap();
        let metadata = db.get_chain_metadata().unwrap();

        let err = sync_from_peer(
            &db,
            &rules,
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(
                err,
                BlockHeaderSyncError::ValidationFailed(ValidationError::BadBlockFound { .. })
            ),
            "{err}"
        );
        assert!(err.get_ban_reason().is_some());
        assert_eq!(db.get_chain_metadata().unwrap(), metadata);
        assert_eq!(db.fetch_last_header().unwrap().height, 5);
    }

    #[tokio::test]
    async fn a_stronger_peer_split_at_genesis_resets_the_chain_and_ends_header_sync() {
        let (db, rules, mut split_info) = create_pruned_chain();
        split_info.chain_split_hash = *db.fetch_genesis_block().hash();
        let (headers, verdicts) = grow_pending_chain(&db, &rules, &split_info, 20).await;
        let len = verdicts
            .iter()
            .position(|verdict| *verdict == PendingChainVerdict::SwitchAfterClaimChecks)
            .unwrap() +
            1;
        let peer_db = create_store_with_consensus_and_validators_and_config(
            rules.clone(),
            Validators::new(
                MockValidator::new(true),
                MockValidator::new(true),
                MockValidator::new(true),
            ),
            BlockchainDatabaseConfig::default(),
        );
        let headers = headers.into_iter().take(len).collect::<Vec<_>>();
        let peer_tip = headers.last().unwrap().clone();
        peer_db.insert_valid_headers(headers).unwrap();

        let err = sync_from_peer(
            &db,
            &rules,
            &peer_db,
            peer_tip.height(),
            peer_tip.accumulated_data().total_accumulated_difficulty,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::ChainResetToGenesis { .. }), "{err}");
        assert!(err.get_ban_reason().is_none());
        assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), 0);
        assert_eq!(db.fetch_last_header().unwrap().height, 0);
    }

    /// Weaker than the block tip, then stronger than it but not the header tip, then stronger than both
    fn assert_verdicts_in_order(verdicts: &[PendingChainVerdict], last: PendingChainVerdict) {
        let refused = verdicts
            .iter()
            .position(|v| *v == PendingChainVerdict::Refused)
            .unwrap();
        let switch = verdicts.iter().position(|v| *v == last).unwrap();
        assert!(verdicts[..refused].iter().all(|v| *v == PendingChainVerdict::Weaker));
        assert!(
            verdicts[refused..switch]
                .iter()
                .all(|v| *v == PendingChainVerdict::Refused)
        );
        assert!(verdicts[switch..].iter().all(|v| *v == last));
    }

    /// Without a reset, a chain only has to beat our block tip, even if it deletes headers we hold above it
    #[tokio::test]
    async fn a_chain_split_above_the_pruned_height_only_has_to_beat_the_block_tip() {
        let (db, rules, mut split_info) = create_pruned_chain();
        add_headers(&db, &rules, 8).await;
        let split = db.fetch_chain_header(3).unwrap();
        split_info.chain_split_hash = *split.hash();
        split_info.reorg_steps_back = 2;

        let (_, verdicts) = grow_pending_chain(&db, &rules, &split_info, 10).await;

        let first_stronger = verdicts
            .iter()
            .position(|verdict| *verdict != PendingChainVerdict::Weaker)
            .unwrap();
        assert!(
            verdicts[first_stronger..]
                .iter()
                .all(|v| *v == PendingChainVerdict::Switch)
        );
    }

    /// Validates `n` headers on the split as the synchronizer's pending chain
    async fn add_pending_headers(
        synchronizer: &mut HeaderSynchronizer<'_, TempDatabase>,
        db: &BlockchainDatabase<TempDatabase>,
        split_info: &ChainSplitInfo,
        n: usize,
    ) {
        synchronizer
            .header_validator
            .initialize_state(&split_info.chain_split_hash)
            .await
            .unwrap();
        let mut prev = db
            .fetch_header_by_block_hash(split_info.chain_split_hash)
            .unwrap()
            .unwrap();
        for _ in 0..n {
            validate_next_header(&mut synchronizer.header_validator, &prev).await;
            prev = synchronizer
                .header_validator
                .valid_headers()
                .last()
                .unwrap()
                .header()
                .clone();
        }
    }

    #[tokio::test]
    async fn a_rewind_that_resets_the_chain_ends_header_sync() {
        let (db, rules, split_info) = create_pruned_chain();
        let metadata = db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![];
        let mut synchronizer = new_synchronizer(&db, &rules, &mut sync_peers, &metadata);
        add_pending_headers(&mut synchronizer, &db, &split_info, 10).await;

        let err = synchronizer
            .switch_to_pending_chain(&split_info, true)
            .await
            .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::ChainResetToGenesis { .. }));
        assert!(err.get_ban_reason().is_none());
        assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), 0);
        assert_eq!(db.fetch_last_header().unwrap().height, 0);
    }

    /// The switch itself refuses a reset for a chain weaker than our headers, whatever the verdict before it said
    #[tokio::test]
    async fn a_switch_that_would_reset_for_a_chain_weaker_than_our_headers_changes_nothing() {
        let (db, rules, split_info) = create_pruned_chain();
        add_headers(&db, &rules, 20).await;
        let metadata = db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![];
        let mut synchronizer = new_synchronizer(&db, &rules, &mut sync_peers, &metadata);
        add_pending_headers(&mut synchronizer, &db, &split_info, 10).await;

        assert_eq!(
            synchronizer.switch_to_pending_chain(&split_info, true).await.unwrap(),
            PendingChainSwitch::Refused
        );

        assert_eq!(db.get_chain_metadata().unwrap(), metadata);
        assert_eq!(db.fetch_last_header().unwrap().height, 25);
        assert_eq!(synchronizer.header_validator.valid_headers().len(), 10);
    }

    #[tokio::test]
    async fn a_chain_that_would_reset_must_beat_the_header_tip() {
        let (db, rules, split_info) = create_pruned_chain();
        // Validated headers far above the block tip
        add_headers(&db, &rules, 8).await;
        let metadata = db.get_chain_metadata().unwrap();

        let (_, verdicts) = grow_pending_chain(&db, &rules, &split_info, 30).await;

        // A reset waits for the peer's claim checks
        assert_verdicts_in_order(&verdicts, PendingChainVerdict::SwitchAfterClaimChecks);
        // Deciding rewinds nothing, and refusing is no fault of the peer
        assert_eq!(db.get_chain_metadata().unwrap(), metadata);
        assert_eq!(db.fetch_last_header().unwrap().height, 13);
        assert!(BlockHeaderSyncError::WeakerThanLocalHeaders.get_ban_reason().is_none());
    }
}
