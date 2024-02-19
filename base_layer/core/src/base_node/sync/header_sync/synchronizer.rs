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
use primitive_types::U256;
use tari_common_types::{chain_metadata::ChainMetadata, types::HashOutput};
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::NodeId,
    protocol::rpc::{RpcClient, RpcError},
    PeerConnection,
};
use tari_utilities::hex::Hex;

use super::{validator::BlockHeaderSyncValidator, BlockHeaderSyncError};
use crate::{
    base_node::sync::{
        ban::PeerBanManager,
        header_sync::HEADER_SYNC_INITIAL_MAX_HEADERS,
        hooks::Hooks,
        rpc,
        BlockchainSyncConfig,
        SyncPeer,
    },
    blocks::{BlockHeader, ChainBlock, ChainHeader},
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, ChainStorageError},
    common::{rolling_avg::RollingAverageTime, BanPeriod},
    consensus::ConsensusManager,
    proof_of_work::randomx_factory::RandomXFactory,
    proto::{
        base_node::{FindChainSplitRequest, SyncHeadersRequest},
        core::BlockHeader as ProtoBlockHeader,
    },
};

const LOG_TARGET: &str = "c::bn::header_sync";

const MAX_LATENCY_INCREASES: usize = 5;

pub struct HeaderSynchronizer<'a, B> {
    config: BlockchainSyncConfig,
    db: AsyncBlockchainDb<B>,
    header_validator: BlockHeaderSyncValidator<B>,
    connectivity: ConnectivityRequester,
    sync_peers: &'a mut Vec<SyncPeer>,
    hooks: Hooks,
    local_cached_metadata: &'a ChainMetadata,
    peer_ban_manager: PeerBanManager,
}

impl<'a, B: BlockchainBackend + 'static> HeaderSynchronizer<'a, B> {
    pub fn new(
        config: BlockchainSyncConfig,
        db: AsyncBlockchainDb<B>,
        consensus_rules: ConsensusManager,
        connectivity: ConnectivityRequester,
        sync_peers: &'a mut Vec<SyncPeer>,
        randomx_factory: RandomXFactory,
        local_metadata: &'a ChainMetadata,
    ) -> Self {
        let peer_ban_manager = PeerBanManager::new(config.clone(), connectivity.clone());
        Self {
            config,
            header_validator: BlockHeaderSyncValidator::new(db.clone(), consensus_rules, randomx_factory),
            db,
            connectivity,
            sync_peers,
            hooks: Default::default(),
            local_cached_metadata: local_metadata,
            peer_ban_manager,
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
        let mut max_latency = self.config.initial_max_sync_latency;
        let mut latency_increases_counter = 0;
        loop {
            match self.try_sync_from_all_peers(max_latency).await {
                Ok((peer, sync_result)) => break Ok((peer, sync_result)),
                Err(err @ BlockHeaderSyncError::AllSyncPeersExceedLatency) => {
                    // If we have few sync peers, throw this out to be retried later
                    if self.sync_peers.len() < 2 {
                        return Err(err);
                    }
                    max_latency += self.config.max_latency_increase;
                    latency_increases_counter += 1;
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
            match self.connect_and_attempt_sync(&node_id, max_latency).await {
                Ok((peer, sync_result)) => return Ok((peer, sync_result)),
                Err(err) => {
                    let ban_reason = BlockHeaderSyncError::get_ban_reason(&err);
                    if let Some(reason) = ban_reason {
                        warn!(target: LOG_TARGET, "{}", err);
                        let duration = match reason.ban_duration {
                            BanPeriod::Short => self.config.short_ban_period,
                            BanPeriod::Long => self.config.ban_period,
                        };
                        self.peer_ban_manager
                            .ban_peer_if_required(&node_id, reason.reason, duration)
                            .await;
                    }
                    if let BlockHeaderSyncError::MaxLatencyExceeded { .. } = err {
                        latency_counter += 1;
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
        let sync_peer = &self.sync_peers[peer_index];
        self.hooks.call_on_starting_hook(sync_peer);

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
        let sync_result = self.attempt_sync(&sync_peer, client, max_latency).await?;
        Ok((sync_peer, sync_result))
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
            .determine_sync_status(sync_peer, best_header.clone(), best_block_header.clone(), &mut client)
            .await?;

        match header_sync_status.clone() {
            HeaderSyncStatus::InSyncOrAhead => {
                debug!(
                    target: LOG_TARGET,
                    "Headers are in sync at height {} but tip is {}. Proceeding to archival/pruned block sync",
                    best_header_height,
                    best_block_height
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
                        .checked_sub(split_info.reorg_steps_back)
                        .unwrap_or_default(),
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
        client: &mut rpc::BaseNodeSyncRpcClient,
        header_count: u64,
    ) -> Result<FindChainSplitResult, BlockHeaderSyncError> {
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
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` did not provide a chain split after {} headers requested. Peer will be banned.",
                    peer_node_id,
                    NUM_CHAIN_SPLIT_HEADERS * MAX_CHAIN_SPLIT_ITERS,
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
                offset + NUM_CHAIN_SPLIT_HEADERS,
                peer_node_id,
                block_hashes.len()
            );

            // No further hashes to send.
            if block_hashes.is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` did not provide a chain split after {} headers requested. Peer will be banned.",
                    peer_node_id,
                    NUM_CHAIN_SPLIT_HEADERS * MAX_CHAIN_SPLIT_ITERS,
                );
                return Err(BlockHeaderSyncError::ChainSplitNotFound(peer_node_id.clone()));
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
                        warn!(
                            target: LOG_TARGET,
                            "Peer `{}` did not provide a chain split after {} headers requested. Peer will be banned.",
                            peer_node_id,
                            NUM_CHAIN_SPLIT_HEADERS * MAX_CHAIN_SPLIT_ITERS,
                        );
                        return Err(BlockHeaderSyncError::ChainSplitNotFound(peer_node_id.clone()));
                    }
                    // Chain split not found, let's go further back
                    offset = NUM_CHAIN_SPLIT_HEADERS * iter_count;
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
            if !resp.headers.is_empty() && resp.headers[0].prev_hash != block_hashes[resp.fork_hash_index as usize] {
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
            let chain_split_hash = block_hashes[resp.fork_hash_index as usize];

            return Ok(FindChainSplitResult {
                reorg_steps_back: resp.fork_hash_index.saturating_add(offset as u64),
                peer_headers: resp.headers,
                peer_fork_hash_index: resp.fork_hash_index,
                chain_split_hash,
            });
        }
    }

    /// Attempt to determine the point at which the remote and local chain diverge, returning the relevant information
    /// of the chain split (see [HeaderSyncStatus]).
    ///
    /// If the local node is behind the remote chain (i.e. `HeaderSyncStatus::Lagging`), the appropriate
    /// `ChainSplitInfo` is returned, the header validator is initialized and the preliminary headers are validated.
    async fn determine_sync_status(
        &mut self,
        sync_peer: &SyncPeer,
        best_header: ChainHeader,
        best_block_header: ChainHeader,
        client: &mut rpc::BaseNodeSyncRpcClient,
    ) -> Result<(HeaderSyncStatus, FindChainSplitResult), BlockHeaderSyncError> {
        // This method will return ban-able errors for certain offenses.
        let chain_split_result = self
            .find_chain_split(sync_peer.node_id(), client, HEADER_SYNC_INITIAL_MAX_HEADERS as u64)
            .await?;
        if chain_split_result.reorg_steps_back > 0 {
            debug!(
                target: LOG_TARGET,
                "Found chain split {} blocks back, received {} headers from peer `{}`",
                chain_split_result.reorg_steps_back,
                chain_split_result.peer_headers.len(),
                sync_peer
            );
        }

        // If the peer returned no new headers, they may still have more blocks than we have, thus have a higher
        // accumulated difficulty.
        if chain_split_result.peer_headers.is_empty() {
            // Our POW is less than the peer's POW, as verified before the attempted header sync, therefore, if the
            // peer did not supply any headers and we know we are behind based on the peer's claimed metadata, then
            // we can ban the peer.
            if best_header.height() == best_block_header.height() {
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` did not provide any headers although they have a better chain and more headers: their \
                    difficulty: {}, our difficulty: {}. Peer will be banned.",
                    sync_peer.node_id(),
                    sync_peer.claimed_chain_metadata().accumulated_target_difficulty(),
                    best_block_header.accumulated_data().total_accumulated_target_difficulty,
                );
                return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                    claimed: sync_peer.claimed_chain_metadata().accumulated_target_difficulty(),
                    actual: None,
                    local: best_block_header.accumulated_data().total_accumulated_target_difficulty,
                });
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
        if self.db.fetch_header_by_block_hash(headers[0].hash()).await?.is_some() {
            return Err(BlockHeaderSyncError::ReceivedInvalidHeader(
                "Header already in database".to_string(),
            ));
        };

        self.header_validator
            .initialize_state(&chain_split_result.chain_split_hash)
            .await?;
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
            .map(|h| (h.height(), *h.hash(), h.accumulated_data().total_accumulated_target_difficulty))
            .expect("synchronize_headers: expected there to be a valid tip header but it was None");

        // If we already have a stronger chain at this point, switch over to it.
        // just in case we happen to be exactly HEADER_SYNC_INITIAL_MAX_HEADERS headers behind.
        let has_better_pow = self.pending_chain_has_higher_pow(&split_info.best_block_header);

        if has_better_pow {
            debug!(
                target: LOG_TARGET,
                "Remote chain from peer {} has higher PoW. Switching",
                sync_peer.node_id()
            );
            self.switch_to_pending_chain(&split_info).await?;
            has_switched_to_new_chain = true;
        }

        if pending_len < HEADER_SYNC_INITIAL_MAX_HEADERS {
            // Peer returned less than the max number of requested headers. This indicates that we have all the
            // available headers from the peer.
            if !has_better_pow {
                // Because the pow is less or equal than the current chain the peer had to have lied about their pow
                debug!(target: LOG_TARGET, "No further headers to download");
                return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                    claimed: sync_peer.claimed_chain_metadata().accumulated_target_difficulty(),
                    actual: Some(total_accumulated_difficulty),
                    local: split_info
                        .best_block_header
                        .accumulated_data()
                        .total_accumulated_target_difficulty,
                });
            }
            // The pow is higher, we swapped to the higher chain, we have all the better chain headers, we can move on
            // to block sync.
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

        let mut last_total_accumulated_difficulty = U256::zero();
        let mut avg_latency = RollingAverageTime::new(20);
        let mut prev_height: Option<u64> = None;
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
                "{}",
                header
            );
            if let Some(prev_header_height) = prev_height {
                if header.height != prev_header_height.saturating_add(1) {
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
            }
            let existing_header = self.db.fetch_header_by_block_hash(header.hash()).await?;
            if let Some(h) = existing_header {
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
                if self.pending_chain_has_higher_pow(&split_info.best_block_header) {
                    self.switch_to_pending_chain(&split_info).await?;
                    has_switched_to_new_chain = true;
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
            prev_height = Some(current_height);
        }

        if !has_switched_to_new_chain {
            if sync_peer.claimed_chain_metadata().accumulated_target_difficulty() <
                self.header_validator
                    .current_valid_chain_tip_header()
                    .map(|h| h.accumulated_data().total_accumulated_target_difficulty)
                    .unwrap_or_default()
            {
                // We should only return this error if the peer sent a PoW less than they advertised.
                return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                    claimed: sync_peer.claimed_chain_metadata().accumulated_target_difficulty(),
                    actual: self
                        .header_validator
                        .current_valid_chain_tip_header()
                        .map(|h| h.accumulated_data().total_accumulated_target_difficulty),
                    local: split_info
                        .best_block_header
                        .accumulated_data()
                        .total_accumulated_target_difficulty,
                });
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Received pow from peer matches claimed, difficulty #{} but local is higher: ({}) and we have not \
                     swapped. Ignoring",
                    sync_peer.claimed_chain_metadata().accumulated_target_difficulty(),
                    split_info
                        .best_block_header
                        .accumulated_data()
                        .total_accumulated_target_difficulty
                );
                return Ok(());
            }
        }

        // Commit the last blocks that don't fit into the COMMIT_EVENT_N_HEADERS blocks
        if !self.header_validator.valid_headers().is_empty() {
            self.commit_pending_headers().await?;
        }

        let claimed_total_accumulated_diff = sync_peer.claimed_chain_metadata().accumulated_target_difficulty();
        // This rule is strict: if the peer advertised a higher PoW than they were able to provide (without
        // some other external factor like a disconnect etc), we detect the and ban the peer.
        if last_total_accumulated_difficulty < claimed_total_accumulated_diff {
            return Err(BlockHeaderSyncError::PeerSentInaccurateChainMetadata {
                claimed: claimed_total_accumulated_diff,
                actual: Some(last_total_accumulated_difficulty),
                local: split_info
                    .best_block_header
                    .accumulated_data()
                    .total_accumulated_target_difficulty,
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

        // Check that the remote tip is stronger than the local tip, equal should not have ended up here, so we treat
        // equal as less
        let proposed_tip = chain_headers.last().unwrap();
        self.header_validator.compare_chains(current_tip, proposed_tip).is_lt()
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

#[derive(Debug, Clone)]
struct FindChainSplitResult {
    reorg_steps_back: u64,
    peer_headers: Vec<ProtoBlockHeader>,
    peer_fork_hash_index: u64,
    chain_split_hash: HashOutput,
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
