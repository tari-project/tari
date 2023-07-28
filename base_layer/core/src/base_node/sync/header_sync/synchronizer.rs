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
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use log::*;
use tari_common_types::{chain_metadata::ChainMetadata, types::HashOutput};
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::NodeId,
    protocol::rpc::{RpcClient, RpcError, RpcHandshakeError},
    PeerConnection,
};
use tari_utilities::hex::Hex;
use tracing;

use super::{validator::BlockHeaderSyncValidator, BlockHeaderSyncError};
use crate::{
    base_node::sync::{hooks::Hooks, rpc, BlockchainSyncConfig, SyncPeer},
    blocks::{BlockHeader, ChainBlock, ChainHeader},
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    common::rolling_avg::RollingAverageTime,
    consensus::ConsensusManager,
    proof_of_work::randomx_factory::RandomXFactory,
    proto::{
        base_node as proto,
        base_node::{FindChainSplitRequest, SyncHeadersRequest},
    },
    validation::ValidationError,
};

const LOG_TARGET: &str = "c::bn::header_sync";

const NUM_INITIAL_HEADERS_TO_REQUEST: usize = 1000;

pub struct HeaderSynchronizer<'a, B> {
    config: BlockchainSyncConfig,
    db: AsyncBlockchainDb<B>,
    header_validator: BlockHeaderSyncValidator<B>,
    connectivity: ConnectivityRequester,
    sync_peers: &'a mut [SyncPeer],
    hooks: Hooks,
    local_metadata: &'a ChainMetadata,
}

impl<'a, B: BlockchainBackend + 'static> HeaderSynchronizer<'a, B> {
    pub fn new(
        config: BlockchainSyncConfig,
        db: AsyncBlockchainDb<B>,
        consensus_rules: ConsensusManager,
        connectivity: ConnectivityRequester,
        sync_peers: &'a mut [SyncPeer],
        randomx_factory: RandomXFactory,
        local_metadata: &'a ChainMetadata,
    ) -> Self {
        Self {
            config,
            header_validator: BlockHeaderSyncValidator::new(db.clone(), consensus_rules, randomx_factory),
            db,
            connectivity,
            sync_peers,
            hooks: Default::default(),
            local_metadata,
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

    pub async fn synchronize(&mut self) -> Result<SyncPeer, BlockHeaderSyncError> {
        debug!(target: LOG_TARGET, "Starting header sync.",);

        info!(
            target: LOG_TARGET,
            "Synchronizing headers ({} candidate peers selected)",
            self.sync_peers.len()
        );
        let mut max_latency = self.config.initial_max_sync_latency;
        loop {
            match self.try_sync_from_all_peers(max_latency).await {
                Ok(sync_peer) => break Ok(sync_peer),
                Err(err @ BlockHeaderSyncError::AllSyncPeersExceedLatency) => {
                    // If we have few sync peers, throw this out to be retried later
                    if self.sync_peers.len() < 2 {
                        return Err(err);
                    }
                    max_latency += self.config.max_latency_increase;
                },
                Err(err) => break Err(err),
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn try_sync_from_all_peers(&mut self, max_latency: Duration) -> Result<SyncPeer, BlockHeaderSyncError> {
        let sync_peer_node_ids = self.sync_peers.iter().map(|p| p.node_id()).cloned().collect::<Vec<_>>();
        info!(
            target: LOG_TARGET,
            "Attempting to sync headers ({} sync peers)",
            sync_peer_node_ids.len()
        );
        for (i, node_id) in sync_peer_node_ids.iter().enumerate() {
            match self.connect_and_attempt_sync(i, node_id, max_latency).await {
                Ok(peer) => return Ok(peer),
                // Try another peer
                Err(err @ BlockHeaderSyncError::NotInSync) => {
                    warn!(target: LOG_TARGET, "{}", err);
                },

                Err(err @ BlockHeaderSyncError::RpcError(RpcError::HandshakeError(RpcHandshakeError::TimedOut))) => {
                    warn!(target: LOG_TARGET, "{}", err);
                    self.ban_peer_short(node_id, BanReason::RpcNegotiationTimedOut).await?;
                },
                Err(BlockHeaderSyncError::ValidationFailed(err)) => {
                    warn!(target: LOG_TARGET, "Block header validation failed: {}", err);
                    self.ban_peer_long(node_id, err.into()).await?;
                },
                Err(BlockHeaderSyncError::ChainSplitNotFound(peer)) => {
                    warn!(target: LOG_TARGET, "Chain split not found for peer {}.", peer);
                    self.ban_peer_long(&peer, BanReason::ChainSplitNotFound).await?;
                },
                Err(ref err @ BlockHeaderSyncError::PeerSentInaccurateChainMetadata { claimed, actual, local }) => {
                    warn!(target: LOG_TARGET, "{}", err);
                    self.ban_peer_long(node_id, BanReason::PeerSentInaccurateChainMetadata {
                        claimed,
                        actual: actual.unwrap_or(0),
                        local,
                    })
                    .await?;
                },
                Err(BlockHeaderSyncError::ChainLinkBroken {
                    height,
                    actual,
                    expected,
                }) => {
                    let reason = BanReason::ChainLinkBroken {
                        height,
                        actual: actual.to_string(),
                        expected: expected.to_string(),
                    };
                    warn!(target: LOG_TARGET, "Chain link broken: {}", reason);
                    self.ban_peer_long(node_id, reason).await?;
                },
                Err(err @ BlockHeaderSyncError::RpcError(RpcError::ReplyTimeout)) |
                Err(err @ BlockHeaderSyncError::MaxLatencyExceeded { .. }) => {
                    warn!(target: LOG_TARGET, "{}", err);
                    if i == self.sync_peers.len() - 1 {
                        return Err(BlockHeaderSyncError::AllSyncPeersExceedLatency);
                    }
                    continue;
                },

                Err(err) => {
                    error!(
                        target: LOG_TARGET,
                        "Failed to synchronize headers from peer `{}`: {}", node_id, err
                    );
                },
            }
        }

        Err(BlockHeaderSyncError::SyncFailedAllPeers)
    }

    async fn connect_and_attempt_sync(
        &mut self,
        peer_index: usize,
        node_id: &NodeId,
        max_latency: Duration,
    ) -> Result<SyncPeer, BlockHeaderSyncError> {
        {
            let sync_peer = &self.sync_peers[peer_index];
            self.hooks.call_on_starting_hook(sync_peer);
        }
        let mut conn = self.dial_sync_peer(node_id).await?;
        debug!(
            target: LOG_TARGET,
            "Attempting to synchronize headers with `{}`", node_id
        );

        let config = RpcClient::builder()
            .with_deadline(self.config.rpc_deadline)
            .with_deadline_grace_period(Duration::from_secs(5));
        let mut client = conn
            .connect_rpc_using_builder::<rpc::BaseNodeSyncRpcClient>(config)
            .await?;

        let latency = client
            .get_last_request_latency()
            .expect("unreachable panic: last request latency must be set after connect");
        self.sync_peers[peer_index].set_latency(latency);
        if latency > max_latency {
            return Err(BlockHeaderSyncError::MaxLatencyExceeded {
                peer: conn.peer_node_id().clone(),
                latency,
                max_latency,
            });
        }

        debug!(target: LOG_TARGET, "Sync peer latency is {:.2?}", latency);
        let sync_peer = self.sync_peers[peer_index].clone();
        self.attempt_sync(&sync_peer, client, max_latency).await?;
        Ok(sync_peer)
    }

    async fn dial_sync_peer(&self, node_id: &NodeId) -> Result<PeerConnection, BlockHeaderSyncError> {
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

    async fn ban_peer_long(&mut self, node_id: &NodeId, reason: BanReason) -> Result<(), BlockHeaderSyncError> {
        self.ban_peer_for(node_id, reason, self.config.ban_period).await
    }

    async fn ban_peer_short(&mut self, node_id: &NodeId, reason: BanReason) -> Result<(), BlockHeaderSyncError> {
        self.ban_peer_for(node_id, reason, self.config.short_ban_period).await
    }

    async fn ban_peer_for(
        &mut self,
        node_id: &NodeId,
        reason: BanReason,
        duration: Duration,
    ) -> Result<(), BlockHeaderSyncError> {
        if self.config.forced_sync_peers.contains(node_id) {
            debug!(
                target: LOG_TARGET,
                "Not banning peer that is allowlisted for sync. Ban reason = {}", reason
            );
            return Ok(());
        }
        warn!(target: LOG_TARGET, "Banned sync peer because {}", reason);
        self.connectivity
            .ban_peer_until(node_id.clone(), duration, reason.to_string())
            .await
            .map_err(BlockHeaderSyncError::FailedToBan)?;
        Ok(())
    }

    #[tracing::instrument(skip(self, client), err)]
    async fn attempt_sync(
        &mut self,
        sync_peer: &SyncPeer,
        mut client: rpc::BaseNodeSyncRpcClient,
        max_latency: Duration,
    ) -> Result<(), BlockHeaderSyncError> {
        let latency = client.get_last_request_latency();
        debug!(
            target: LOG_TARGET,
            "Initiating header sync with peer `{}` (sync latency = {}ms)",
            sync_peer.node_id(),
            latency.unwrap_or_default().as_millis()
        );

        // Fetch the local tip header at the beginning of the sync process
        let local_tip_header = self.db.fetch_last_chain_header().await?;
        let local_total_accumulated_difficulty = local_tip_header.accumulated_data().total_accumulated_difficulty;
        let header_tip_height = local_tip_header.height();
        let sync_status = self
            .determine_sync_status(sync_peer, local_tip_header, &mut client)
            .await?;
        match sync_status {
            SyncStatus::InSync | SyncStatus::WereAhead => {
                let metadata = self.db.get_chain_metadata().await?;
                if metadata.height_of_longest_chain() < header_tip_height {
                    debug!(
                        target: LOG_TARGET,
                        "Headers are in sync at height {} but tip is {}. Proceeding to archival/pruned block sync",
                        header_tip_height,
                        metadata.height_of_longest_chain()
                    );
                    Ok(())
                } else {
                    // Check if the metadata that we had when we decided to enter header sync is behind the peer's
                    // claimed one. If so, our chain has updated in the meantime and the sync peer
                    // is behaving.
                    if self.local_metadata.accumulated_difficulty() <=
                        sync_peer.claimed_chain_metadata().accumulated_difficulty()
                    {
                        debug!(
                            target: LOG_TARGET,
                            "Local blockchain received a better block through propagation at height {} (was: {}). \
                             Proceeding to archival/pruned block sync",
                            metadata.height_of_longest_chain(),
                            self.local_metadata.height_of_longest_chain()
                        );
                        return Ok(());
                    }
                    debug!(
                        target: LOG_TARGET,
                        "Headers and block state are already in-sync (Header Tip: {}, Block tip: {}, Peer's height: \
                         {})",
                        header_tip_height,
                        metadata.height_of_longest_chain(),
                        sync_peer.claimed_chain_metadata().height_of_longest_chain(),
                    );
                    Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                        claimed: sync_peer.claimed_chain_metadata().accumulated_difficulty(),
                        actual: None,
                        local: local_total_accumulated_difficulty,
                    })
                }
            },
            SyncStatus::Lagging(split_info) => {
                self.hooks.call_on_progress_header_hooks(
                    split_info
                        .local_tip_header
                        .height()
                        .checked_sub(split_info.reorg_steps_back)
                        .unwrap_or_default(),
                    split_info.remote_tip_height,
                    sync_peer,
                );
                self.synchronize_headers(sync_peer.clone(), &mut client, *split_info, max_latency)
                    .await?;
                Ok(())
            },
        }
    }

    async fn find_chain_split(
        &mut self,
        peer: &NodeId,
        client: &mut rpc::BaseNodeSyncRpcClient,
        header_count: u64,
    ) -> Result<(proto::FindChainSplitResponse, Vec<HashOutput>, u64), BlockHeaderSyncError> {
        const NUM_CHAIN_SPLIT_HEADERS: usize = 500;
        // Limit how far back we're willing to go. A peer might just say it does not have a chain split
        // and keep us busy going back until the genesis.
        // 20 x 500 = max 10,000 block split can be detected
        const MAX_CHAIN_SPLIT_ITERS: usize = 20;

        let mut offset = 0;
        let mut iter_count = 0;
        loop {
            iter_count += 1;
            if iter_count > MAX_CHAIN_SPLIT_ITERS {
                return Err(BlockHeaderSyncError::ChainSplitNotFound(peer.clone()));
            }

            let block_hashes = self
                .db
                .fetch_block_hashes_from_header_tip(NUM_CHAIN_SPLIT_HEADERS, offset)
                .await?;
            debug!(
                target: LOG_TARGET,
                "Determining if chain splits between {} and {} headers back from the tip (peer: `{}`, {} hashes sent)",
                offset,
                offset + NUM_CHAIN_SPLIT_HEADERS,
                peer,
                block_hashes.len()
            );

            // No further hashes to send.
            if block_hashes.is_empty() {
                return Err(BlockHeaderSyncError::ChainSplitNotFound(peer.clone()));
            }

            let request = FindChainSplitRequest {
                block_hashes: block_hashes.clone().iter().map(|v| v.to_vec()).collect(),
                header_count,
            };

            let resp = match client.find_chain_split(request).await {
                Ok(r) => r,
                Err(RpcError::RequestFailed(err)) if err.as_status_code().is_not_found() => {
                    // This round we sent less hashes than the max, so the next round will not have any more hashes to
                    // send. Exit early in this case.
                    if block_hashes.len() < NUM_CHAIN_SPLIT_HEADERS {
                        return Err(BlockHeaderSyncError::ChainSplitNotFound(peer.clone()));
                    }
                    // Chain split not found, let's go further back
                    offset = NUM_CHAIN_SPLIT_HEADERS * iter_count;
                    continue;
                },
                Err(err) => {
                    return Err(err.into());
                },
            };

            let steps_back = resp.fork_hash_index + offset as u64;
            return Ok((resp, block_hashes, steps_back));
        }
    }

    /// Attempt to determine the point at which the remote and local chain diverge, returning the relevant information
    /// of the chain split (see [SyncStatus]).
    ///
    /// If the local node is behind the remote chain (i.e. `SyncStatus::Lagging`), the appropriate `ChainSplitInfo` is
    /// returned, the header validator is initialized and the preliminary headers are validated.
    async fn determine_sync_status(
        &mut self,
        sync_peer: &SyncPeer,
        local_tip_header: ChainHeader,
        client: &mut rpc::BaseNodeSyncRpcClient,
    ) -> Result<SyncStatus, BlockHeaderSyncError> {
        let (resp, block_hashes, steps_back) = self
            .find_chain_split(sync_peer.node_id(), client, NUM_INITIAL_HEADERS_TO_REQUEST as u64)
            .await?;
        if resp.headers.len() > NUM_INITIAL_HEADERS_TO_REQUEST {
            self.ban_peer_long(
                sync_peer.node_id(),
                BanReason::PeerSentTooManyHeaders(resp.headers.len()),
            )
            .await?;
            return Err(BlockHeaderSyncError::NotInSync);
        }
        let proto::FindChainSplitResponse {
            headers,
            fork_hash_index,
            tip_height: remote_tip_height,
        } = resp;

        if steps_back > 0 {
            debug!(
                target: LOG_TARGET,
                "Found chain split {} blocks back, received {} headers from peer `{}`",
                steps_back,
                headers.len(),
                sync_peer
            );
        }

        if fork_hash_index >= block_hashes.len() as u64 {
            let _result = self
                .ban_peer_long(sync_peer.node_id(), BanReason::SplitHashGreaterThanHashes {
                    fork_hash_index,
                    num_block_hashes: block_hashes.len(),
                })
                .await;
            return Err(BlockHeaderSyncError::FoundHashIndexOutOfRange(
                block_hashes.len() as u64,
                fork_hash_index,
            ));
        }

        // If the peer returned no new headers, this means header sync is done.
        if headers.is_empty() {
            if fork_hash_index > 0 {
                debug!(
                    target: LOG_TARGET,
                    "Peer `{}` has sent no headers but forked_hash_index is {}. The peer is behind our chain.",
                    sync_peer,
                    fork_hash_index
                );

                return Ok(SyncStatus::WereAhead);
            }

            debug!(target: LOG_TARGET, "Already in sync with peer `{}`.", sync_peer);
            return Ok(SyncStatus::InSync);
        }

        let headers = headers
            .into_iter()
            .map(BlockHeader::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BlockHeaderSyncError::ReceivedInvalidHeader)?;
        let num_new_headers = headers.len();

        // NOTE: We can trust that the header associated with this hash exists because `block_hashes` was supplied by
        // this node. Bounds checking for fork_hash_index has been done above.
        #[allow(clippy::cast_possible_truncation)]
        let chain_split_hash = block_hashes.get(fork_hash_index as usize).unwrap();

        self.header_validator.initialize_state(chain_split_hash).await?;
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
            "Peer {} has submitted {} valid header(s)", sync_peer, num_new_headers
        );

        // Basic sanity check that the peer sent tip height greater than the split.
        let split_height = local_tip_header.height().saturating_sub(steps_back);
        if remote_tip_height < split_height {
            self.ban_peer_short(sync_peer.node_id(), BanReason::PeerSentInvalidTipHeight {
                actual: remote_tip_height,
                expected: split_height,
            })
            .await?;
            return Err(BlockHeaderSyncError::InvalidProtocolResponse(format!(
                "Peer {} sent invalid remote tip height",
                sync_peer
            )));
        }

        let chain_split_info = ChainSplitInfo {
            local_tip_header,
            remote_tip_height,
            reorg_steps_back: steps_back,
            chain_split_hash: *chain_split_hash,
        };
        Ok(SyncStatus::Lagging(Box::new(chain_split_info)))
    }

    async fn rewind_blockchain(&self, split_hash: HashOutput) -> Result<Vec<Arc<ChainBlock>>, BlockHeaderSyncError> {
        debug!(
            target: LOG_TARGET,
            "Deleting headers that no longer form part of the main chain up until split at {}",
            split_hash.to_hex()
        );

        let blocks = self.db.rewind_to_hash(split_hash).await?;
        debug!(
            target: LOG_TARGET,
            "Rewound {} block(s) in preparation for header sync",
            blocks.len()
        );
        Ok(blocks)
    }

    #[allow(clippy::too_many_lines)]
    async fn synchronize_headers(
        &mut self,
        mut sync_peer: SyncPeer,
        client: &mut rpc::BaseNodeSyncRpcClient,
        split_info: ChainSplitInfo,
        max_latency: Duration,
    ) -> Result<(), BlockHeaderSyncError> {
        info!(target: LOG_TARGET, "Starting header sync from peer {}", sync_peer);
        const COMMIT_EVERY_N_HEADERS: usize = 1000;

        let mut has_switched_to_new_chain = false;
        let pending_len = self.header_validator.valid_headers().len();

        // Find the hash to start syncing the rest of the headers.
        // The expectation cannot fail because there has been at least one valid header returned (checked in
        // determine_sync_status)
        let (start_header_height, start_header_hash, total_accumulated_difficulty) = self
            .header_validator
            .current_valid_chain_tip_header()
            .map(|h| (h.height(), *h.hash(), h.accumulated_data().total_accumulated_difficulty))
            .expect("synchronize_headers: expected there to be a valid tip header but it was None");

        // If we already have a stronger chain at this point, switch over to it.
        // just in case we happen to be exactly NUM_INITIAL_HEADERS_TO_REQUEST headers behind.
        let has_better_pow = self.pending_chain_has_higher_pow(&split_info.local_tip_header);
        if has_better_pow {
            debug!(
                target: LOG_TARGET,
                "Remote chain from peer {} has higher PoW. Switching",
                sync_peer.node_id()
            );
            self.switch_to_pending_chain(&split_info).await?;
            has_switched_to_new_chain = true;
        }

        if pending_len < NUM_INITIAL_HEADERS_TO_REQUEST {
            // Peer returned less than the number of requested headers. This indicates that we have all the available
            // headers.
            debug!(target: LOG_TARGET, "No further headers to download");
            if !has_better_pow {
                return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                    claimed: sync_peer.claimed_chain_metadata().accumulated_difficulty(),
                    actual: Some(total_accumulated_difficulty),
                    local: split_info
                        .local_tip_header
                        .accumulated_data()
                        .total_accumulated_difficulty,
                });
            }

            return Ok(());
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

        let mut last_total_accumulated_difficulty = 0;
        let mut avg_latency = RollingAverageTime::new(20);
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
            let existing_header = self.db.fetch_header_by_block_hash(header.hash()).await?;
            if let Some(h) = existing_header {
                warn!(
                    target: LOG_TARGET,
                    "Received header #{} `{}` that we already have. Ignoring",
                    h.height,
                    h.hash().to_hex()
                );
                continue;
            }
            let current_height = header.height;
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
                if self.pending_chain_has_higher_pow(&split_info.local_tip_header) {
                    self.switch_to_pending_chain(&split_info).await?;
                    has_switched_to_new_chain = true;
                }
            }

            sync_peer.set_latency(latency);
            sync_peer.add_sample(last_sync_timer.elapsed());
            self.hooks
                .call_on_progress_header_hooks(current_height, split_info.remote_tip_height, &sync_peer);

            let last_avg_latency = avg_latency.calculate_average_with_min_samples(5);
            if let Some(avg_latency) = last_avg_latency {
                if avg_latency > max_latency {
                    return Err(BlockHeaderSyncError::MaxLatencyExceeded {
                        peer: sync_peer.node_id().clone(),
                        latency: avg_latency,
                        max_latency,
                    });
                }
            }

            last_sync_timer = Instant::now();
        }

        if !has_switched_to_new_chain {
            if sync_peer.claimed_chain_metadata().accumulated_difficulty() <
                self.header_validator
                    .current_valid_chain_tip_header()
                    .map(|h| h.accumulated_data().total_accumulated_difficulty)
                    .unwrap_or_default()
            {
                // We should only return this error if the peer sent a PoW less than they advertised.
                return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                    claimed: sync_peer.claimed_chain_metadata().accumulated_difficulty(),
                    actual: self
                        .header_validator
                        .current_valid_chain_tip_header()
                        .map(|h| h.accumulated_data().total_accumulated_difficulty),
                    local: split_info
                        .local_tip_header
                        .accumulated_data()
                        .total_accumulated_difficulty,
                });
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Received pow from peer matches claimed, difficulty #{} but local is higher: ({}) and we have not \
                     swapped. Ignoring",
                    sync_peer.claimed_chain_metadata().accumulated_difficulty(),
                    split_info
                        .local_tip_header
                        .accumulated_data()
                        .total_accumulated_difficulty
                );
                return Ok(());
            }
        }

        // Commit the last blocks that don't fit into the COMMIT_EVENT_N_HEADERS blocks
        if !self.header_validator.valid_headers().is_empty() {
            self.commit_pending_headers().await?;
        }

        let claimed_total_accumulated_diff = sync_peer.claimed_chain_metadata().accumulated_difficulty();
        // This rule is strict: if the peer advertised a higher PoW than they were able to provide (without
        // some other external factor like a disconnect etc), we detect the and ban the peer.
        if last_total_accumulated_difficulty < claimed_total_accumulated_diff {
            return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                claimed: claimed_total_accumulated_diff,
                actual: Some(last_total_accumulated_difficulty),
                local: split_info
                    .local_tip_header
                    .accumulated_data()
                    .total_accumulated_difficulty,
            });
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

        // Check that the remote tip is stronger than the local tip
        let proposed_tip = chain_headers.last().unwrap();
        self.header_validator.compare_chains(current_tip, proposed_tip).is_le()
    }

    async fn switch_to_pending_chain(&mut self, split_info: &ChainSplitInfo) -> Result<(), BlockHeaderSyncError> {
        // Reorg if required
        if split_info.reorg_steps_back > 0 {
            debug!(
                target: LOG_TARGET,
                "Reorg: Rewinding the chain by {} block(s) (split hash = {})",
                split_info.reorg_steps_back,
                split_info.chain_split_hash.to_hex()
            );
            let blocks = self.rewind_blockchain(split_info.chain_split_hash).await?;
            if !blocks.is_empty() {
                self.hooks.call_on_rewind_hooks(blocks);
            }
        }

        // Commit the forked chain. At this point
        // 1. Headers have been validated
        // 2. The forked chain has a higher PoW than the local chain
        //
        // After this we commit headers every `n` blocks
        self.commit_pending_headers().await?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
enum BanReason {
    #[error("This peer sent too many headers ({0}) in response to a chain split request")]
    PeerSentTooManyHeaders(usize),
    #[error("This peer sent an invalid tip height {actual} expected a height greater than or equal to {expected}")]
    PeerSentInvalidTipHeight { actual: u64, expected: u64 },
    #[error(
        "This peer sent a split hash index ({fork_hash_index}) greater than the number of block hashes sent \
         ({num_block_hashes})"
    )]
    SplitHashGreaterThanHashes {
        fork_hash_index: u64,
        num_block_hashes: usize,
    },
    #[error("Peer sent invalid header: {0}")]
    ValidationFailed(#[from] ValidationError),
    #[error("Peer could not find the location of a chain split")]
    ChainSplitNotFound,
    #[error("Peer did not respond timeously during RPC negotiation")]
    RpcNegotiationTimedOut,
    #[error("Header at height {height} did not form a chain. Expected {actual} to equal the previous hash {expected}")]
    ChainLinkBroken {
        height: u64,
        actual: String,
        expected: String,
    },
    #[error(
        "Peer sent inaccurate chain metadata. Claimed {claimed} but validated difficulty was {actual}, while local \
         was {local}"
    )]
    PeerSentInaccurateChainMetadata { claimed: u128, actual: u128, local: u128 },
}

struct ChainSplitInfo {
    local_tip_header: ChainHeader,
    remote_tip_height: u64,
    reorg_steps_back: u64,
    chain_split_hash: HashOutput,
}

enum SyncStatus {
    /// Local and remote node are in sync
    InSync,
    /// Local node is ahead of the remote node
    WereAhead,
    /// Local node is lagging behind remote node
    Lagging(Box<ChainSplitInfo>),
}
