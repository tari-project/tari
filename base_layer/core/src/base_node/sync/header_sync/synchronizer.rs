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

use super::{validator::BlockHeaderSyncValidator, BlockHeaderSyncError};
use crate::{
    base_node::sync::{hooks::Hooks, rpc, BlockSyncConfig},
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, ChainBlock, ChainHeader},
    consensus::ConsensusManager,
    proof_of_work::randomx_factory::RandomXFactory,
    proto::{
        base_node as proto,
        base_node::{FindChainSplitRequest, SyncHeadersRequest},
    },
    tari_utilities::{hex::Hex, Hashable},
    transactions::types::HashOutput,
    validation::ValidationError,
};
use futures::{future, stream::FuturesUnordered, StreamExt};
use log::*;
use std::{
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_comms::{
    connectivity::{ConnectivityError, ConnectivityRequester, ConnectivitySelection},
    peer_manager::NodeId,
    protocol::rpc::{RpcError, RpcHandshakeError},
    PeerConnection,
};

const LOG_TARGET: &str = "c::bn::header_sync";

const NUM_INITIAL_HEADERS_TO_REQUEST: u64 = 1000;

pub struct HeaderSynchronizer<'a, B> {
    config: BlockSyncConfig,
    db: AsyncBlockchainDb<B>,
    header_validator: BlockHeaderSyncValidator<B>,
    connectivity: ConnectivityRequester,
    sync_peers: &'a [NodeId],
    hooks: Hooks,
}

impl<'a, B: BlockchainBackend + 'static> HeaderSynchronizer<'a, B> {
    pub fn new(
        config: BlockSyncConfig,
        db: AsyncBlockchainDb<B>,
        consensus_rules: ConsensusManager,
        connectivity: ConnectivityRequester,
        sync_peers: &'a [NodeId],
        randomx_factory: RandomXFactory,
    ) -> Self
    {
        Self {
            config,
            header_validator: BlockHeaderSyncValidator::new(db.clone(), consensus_rules, randomx_factory),
            db,
            connectivity,
            sync_peers,
            hooks: Default::default(),
        }
    }

    pub fn on_progress<H>(&mut self, hook: H)
    where H: FnMut(u64, u64, &[NodeId]) + Send + Sync + 'static {
        self.hooks.add_on_progress_header_hook(hook);
    }

    pub fn on_rewind<H>(&mut self, hook: H)
    where H: FnMut(Vec<Arc<ChainBlock>>) + Send + Sync + 'static {
        self.hooks.add_on_rewind_hook(hook);
    }

    pub async fn synchronize(&mut self) -> Result<PeerConnection, BlockHeaderSyncError> {
        debug!(target: LOG_TARGET, "Starting header sync.",);
        let sync_peers = self.select_sync_peers().await?;
        info!(
            target: LOG_TARGET,
            "Synchronizing headers ({} candidate peers selected)",
            sync_peers.len()
        );

        for peer_conn in sync_peers {
            let node_id = peer_conn.peer_node_id().clone();
            debug!(
                target: LOG_TARGET,
                "Attempting to synchronize headers with `{}`", node_id
            );
            match self.attempt_sync(peer_conn.clone()).await {
                Ok(()) => return Ok(peer_conn),
                // Try another peer
                Err(err @ BlockHeaderSyncError::NotInSync) => {
                    debug!(target: LOG_TARGET, "{}", err);
                },

                Err(err @ BlockHeaderSyncError::RpcError(RpcError::HandshakeError(RpcHandshakeError::TimedOut))) => {
                    debug!(target: LOG_TARGET, "{}", err);
                    self.ban_peer_short(node_id, BanReason::RpcNegotiationTimedOut).await?;
                },
                Err(BlockHeaderSyncError::ValidationFailed(err)) => {
                    debug!(target: LOG_TARGET, "Block header validation failed: {}", err);
                    self.ban_peer_long(node_id, err.into()).await?;
                },
                Err(err @ BlockHeaderSyncError::InvalidBlockHeight { .. }) => {
                    debug!(target: LOG_TARGET, "{}", err);
                    self.ban_peer_long(node_id, BanReason::GeneralHeaderSyncFailure(err))
                        .await?;
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to synchronize headers from peer `{}`: {}", node_id, err
                    );
                },
            }
        }

        Err(BlockHeaderSyncError::SyncFailedAllPeers)
    }

    async fn wait_until_online(&mut self) -> Result<(), BlockHeaderSyncError> {
        const MAX_ONLINE_ATTEMPTS: usize = 5;
        let mut attempts = 0;
        loop {
            match self.connectivity.wait_for_connectivity(Duration::from_secs(10)).await {
                Ok(_) => break Ok(()),
                Err(ConnectivityError::OnlineWaitTimeout(n)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Still waiting for this node to come online ({} peer(s) connected)", n
                    );
                    // If we have waited a long enough for more node connections and we have some connections, let's try
                    // and sync
                    if attempts > MAX_ONLINE_ATTEMPTS && n > 0 {
                        warn!(
                            target: LOG_TARGET,
                            "This node is still not well connected, attempting to sync with {} node(s).", n
                        );
                        break Ok(());
                    }
                    if attempts > MAX_ONLINE_ATTEMPTS && n == 0 {
                        warn!(
                            target: LOG_TARGET,
                            "This node is still not connected to any other nodes. Assuming that this is the only node.",
                        );
                        break Err(BlockHeaderSyncError::NetworkSilence);
                    }

                    attempts += 1;
                },
                Err(err) => break Err(err.into()),
            }
        }
    }

    async fn select_sync_peers(&mut self) -> Result<Vec<PeerConnection>, BlockHeaderSyncError> {
        if self.sync_peers.is_empty() {
            self.wait_until_online().await?;
            let sync_peers = self
                .connectivity
                .select_connections(ConnectivitySelection::all_nodes(vec![]))
                .await?;

            debug!(
                target: LOG_TARGET,
                "Selecting all connected nodes ({})",
                sync_peers.len()
            );

            return Ok(sync_peers);
        }

        debug!(target: LOG_TARGET, "Dialing {} sync peer(s)", self.sync_peers.len());
        let tasks = self
            .sync_peers
            .iter()
            .map(|node_id| {
                let mut c = self.connectivity.clone();
                let node_id = node_id.clone();
                async move { c.dial_peer(node_id).await }
            })
            .collect::<FuturesUnordered<_>>();

        let connections = tasks
            .filter_map(|r| match r {
                Ok(conn) => future::ready(Some(conn)),
                Err(err) => {
                    debug!(target: LOG_TARGET, "Failed to dial sync peer: {}", err);
                    future::ready(None)
                },
            })
            .collect::<Vec<_>>()
            .await;
        debug!(
            target: LOG_TARGET,
            "Successfully dialed {} of {} sync peer(s)",
            connections.len(),
            self.sync_peers.len()
        );
        Ok(connections)
    }

    async fn ban_peer_long(&mut self, node_id: NodeId, reason: BanReason) -> Result<(), BlockHeaderSyncError> {
        self.ban_peer_for(node_id, reason, self.config.ban_period).await
    }

    async fn ban_peer_short(&mut self, node_id: NodeId, reason: BanReason) -> Result<(), BlockHeaderSyncError> {
        self.ban_peer_for(node_id, reason, self.config.short_ban_period).await
    }

    async fn ban_peer_for(
        &mut self,
        node_id: NodeId,
        reason: BanReason,
        duration: Duration,
    ) -> Result<(), BlockHeaderSyncError>
    {
        if self.config.sync_peers.contains(&node_id) {
            debug!(
                target: LOG_TARGET,
                "Not banning peer that is allowlisted for sync. Ban reason = {}", reason
            );
            return Ok(());
        }
        warn!(target: LOG_TARGET, "Banned sync peer because {}", reason);
        self.connectivity
            .ban_peer_until(node_id, duration, reason.to_string())
            .await
            .map_err(BlockHeaderSyncError::FailedToBan)?;
        Ok(())
    }

    async fn attempt_sync(&mut self, mut conn: PeerConnection) -> Result<(), BlockHeaderSyncError> {
        let peer = conn.peer_node_id().clone();
        let mut client = conn.connect_rpc::<rpc::BaseNodeSyncRpcClient>().await?;
        let latency = client.get_last_request_latency().await?;
        debug!(
            target: LOG_TARGET,
            "Initiating header sync with peer `{}` (latency = {}ms)",
            conn.peer_node_id(),
            latency.unwrap_or_default().as_millis()
        );

        let sync_status = self.determine_sync_status(&peer, &mut client).await?;
        match sync_status {
            SyncStatus::InSync => Ok(()),
            // We're ahead of this peer, try another peer if possible
            SyncStatus::Ahead => Err(BlockHeaderSyncError::NotInSync),
            SyncStatus::Lagging(split_info) => {
                self.synchronize_headers(&peer, &mut client, *split_info).await?;
                Ok(())
            },
        }
    }

    async fn find_chain_split(
        &mut self,
        peer: &NodeId,
        client: &mut rpc::BaseNodeSyncRpcClient,
        header_count: u64,
    ) -> Result<(proto::FindChainSplitResponse, Vec<HashOutput>, u64), BlockHeaderSyncError>
    {
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
                self.ban_peer_long(peer.clone(), BanReason::ChainSplitNotFound).await?;
                return Err(BlockHeaderSyncError::ChainSplitNotFound(peer.clone()));
            }

            let block_hashes = self
                .db
                .fetch_block_hashes_from_header_tip(NUM_CHAIN_SPLIT_HEADERS, offset)
                .await?;
            debug!(
                target: LOG_TARGET,
                "Determining if chain splits between {} and {} headers back from peer `{}`",
                offset,
                offset + NUM_CHAIN_SPLIT_HEADERS,
                peer,
            );

            let request = FindChainSplitRequest {
                block_hashes: block_hashes.clone(),
                header_count,
            };

            let resp = match client.find_chain_split(request).await {
                Ok(r) => r,
                Err(RpcError::RequestFailed(err)) if err.status_code().is_not_found() => {
                    // Chain split not found, let's go further back
                    offset = NUM_CHAIN_SPLIT_HEADERS * iter_count;
                    continue;
                },
                Err(err) => {
                    return Err(err.into());
                },
            };

            let steps_back = resp.fork_hash_index as u64 + offset as u64;
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
        peer: &NodeId,
        client: &mut rpc::BaseNodeSyncRpcClient,
    ) -> Result<SyncStatus, BlockHeaderSyncError>
    {
        // Fetch the local tip header at the beginning of the sync process
        let local_tip_header = self.db.fetch_tip_header().await?;

        let (resp, block_hashes, steps_back) = self
            .find_chain_split(peer, client, NUM_INITIAL_HEADERS_TO_REQUEST)
            .await?;
        if resp.headers.len() > NUM_INITIAL_HEADERS_TO_REQUEST as usize {
            self.ban_peer_long(peer.clone(), BanReason::PeerSentTooManyHeaders(resp.headers.len()))
                .await?;
            return Err(BlockHeaderSyncError::NotInSync);
        }
        let proto::FindChainSplitResponse {
            headers,
            fork_hash_index,
            tip_height: remote_tip_height,
        } = resp;
        debug!(
            target: LOG_TARGET,
            "Found split {} blocks back, received {} headers from peer `{}`",
            steps_back,
            headers.len(),
            peer
        );

        if fork_hash_index >= block_hashes.len() as u32 {
            let _ = self
                .ban_peer_long(peer.clone(), BanReason::SplitHashGreaterThanHashes {
                    fork_hash_index,
                    num_block_hashes: block_hashes.len(),
                })
                .await;
            return Err(BlockHeaderSyncError::FoundHashIndexOutOfRange(
                block_hashes.len() as u32,
                fork_hash_index,
            ));
        }

        // If the peer returned no new headers, this means header sync is done.
        if headers.is_empty() {
            if fork_hash_index > 0 {
                debug!(
                    target: LOG_TARGET,
                    "Peer `{}` has sent no headers but forked_hash_index is {}. The peer is behind our chain.",
                    peer,
                    fork_hash_index
                );

                return Ok(SyncStatus::Ahead);
            }

            debug!(target: LOG_TARGET, "Already in sync with peer `{}`.", peer);
            return Ok(SyncStatus::InSync);
        }

        let headers = headers
            .into_iter()
            .map(BlockHeader::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BlockHeaderSyncError::ReceivedInvalidHeader)?;
        let num_new_headers = headers.len();

        // We can trust that the header associated with this hash exists because block_hashes is data this node
        // supplied. usize conversion overflow has already been checked above
        let chain_split_hash = block_hashes[fork_hash_index as usize].clone();

        self.header_validator.initialize_state(chain_split_hash.clone()).await?;
        for header in headers {
            debug!(
                target: LOG_TARGET,
                "Validating header #{} (Pow: {})",
                header.height,
                header.pow_algo(),
            );
            self.header_validator.validate(header)?;
        }

        debug!(
            target: LOG_TARGET,
            "Peer {} has submitted {} valid header(s)", peer, num_new_headers
        );

        // Basic sanity check that the peer sent tip height greater than the split.
        let split_height = local_tip_header.height().saturating_sub(steps_back);
        if remote_tip_height < split_height {
            self.ban_peer_short(peer.clone(), BanReason::PeerSentInvalidTipHeight {
                actual: remote_tip_height,
                expected: split_height,
            })
            .await?;
            return Err(BlockHeaderSyncError::InvalidProtocolResponse(format!(
                "Peer {} sent invalid remote tip height",
                peer
            )));
        }

        let chain_split_info = ChainSplitInfo {
            local_tip_header,
            remote_tip_height,
            reorg_steps_back: steps_back,
            chain_split_hash,
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

    async fn synchronize_headers(
        &mut self,
        peer: &NodeId,
        client: &mut rpc::BaseNodeSyncRpcClient,
        split_info: ChainSplitInfo,
    ) -> Result<(), BlockHeaderSyncError>
    {
        const COMMIT_EVERY_N_HEADERS: usize = 1000;

        // Peer returned less than the max headers. This indicates that there are no further headers to request.
        if self.header_validator.valid_headers().len() < NUM_INITIAL_HEADERS_TO_REQUEST as usize {
            debug!(target: LOG_TARGET, "No further headers to download");
            if !self.pending_chain_has_higher_pow(&split_info.local_tip_header)? {
                return Err(BlockHeaderSyncError::WeakerChain);
            }

            debug!(
                target: LOG_TARGET,
                "Remote chain from peer {} has higher PoW. Switching", peer
            );
            // PoW is higher, switching over to the new chain
            self.switch_to_pending_chain(&split_info).await?;

            return Ok(());
        }

        // Find the hash to start syncing the rest of the headers.
        // The expectation cannot fail because the number of headers has been checked in determine_sync_status
        let start_header =
            self.header_validator.valid_headers().last().expect(
                "synchronize_headers: expected there to be at least one valid pending header but there were none",
            );

        debug!(
            target: LOG_TARGET,
            "Download remaining headers starting from header #{} from peer `{}`", start_header.header.height, peer
        );
        let request = SyncHeadersRequest {
            start_hash: start_header.header.hash(),
            // To the tip!
            count: 0,
        };

        let mut header_stream = client.sync_headers(request).await?;
        debug!(target: LOG_TARGET, "Reading headers from peer `{}`", peer);

        let mut has_switched_to_new_chain = false;

        while let Some(header) = header_stream.next().await {
            let header = BlockHeader::try_from(header?).map_err(BlockHeaderSyncError::ReceivedInvalidHeader)?;
            debug!(
                target: LOG_TARGET,
                "Validating header: #{} (PoW = {})",
                header.height,
                header.pow_algo()
            );
            let existing_header = self.db.fetch_header_by_block_hash(header.hash()).await?;
            // TODO: Due to a bug in a previous version of base node sync RPC, the duplicate headers can be sent. We
            //       should be a little more strict about this in future.
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
            self.header_validator.validate(header)?;

            if has_switched_to_new_chain {
                // If we've switched to the new chain, we simply commit every COMMIT_EVERY_N_HEADERS headers
                if self.header_validator.valid_headers().len() >= COMMIT_EVERY_N_HEADERS {
                    self.commit_pending_headers().await?;
                }
            } else {
                // The remote chain has not (yet) been accepted.
                // We check the tip difficulties, switching over to the new chain if a higher accumulated difficulty is
                // achieved.
                if self.pending_chain_has_higher_pow(&split_info.local_tip_header)? {
                    self.switch_to_pending_chain(&split_info).await?;
                    has_switched_to_new_chain = true;
                }
            }

            self.hooks
                .call_on_progress_header_hooks(current_height, split_info.remote_tip_height, self.sync_peers);
        }

        if !has_switched_to_new_chain {
            return Err(BlockHeaderSyncError::WeakerChain);
        }

        // Commit the last blocks that don't fit into the COMMIT_EVENT_N_HEADERS blocks
        if !self.header_validator.valid_headers().is_empty() {
            self.commit_pending_headers().await?;
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
            txn.insert_header(chain_header.header, chain_header.accumulated_data);
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

    fn pending_chain_has_higher_pow(&self, current_tip: &ChainHeader) -> Result<bool, BlockHeaderSyncError> {
        let chain_headers = self.header_validator.valid_headers();
        if chain_headers.is_empty() {
            return Ok(false);
        }

        // Check that the remote tip is stronger than the local tip
        let proposed_tip = chain_headers.last().unwrap();
        match self.header_validator.check_stronger_chain(current_tip, proposed_tip) {
            Ok(_) => Ok(true),
            Err(BlockHeaderSyncError::WeakerChain) => Ok(false),
            Err(err) => Err(err),
        }
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
            let blocks = self.rewind_blockchain(split_info.chain_split_hash.clone()).await?;
            // NOTE: `blocks` only contains full blocks that were reorged out, and not the headers.
            //       This may be unexpected for implementers of the rewind hook.
            self.hooks.call_on_rewind_hooks(blocks);
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
        fork_hash_index: u32,
        num_block_hashes: usize,
    },
    #[error("Peer sent invalid header: {0}")]
    ValidationFailed(#[from] ValidationError),
    #[error("Peer could not find the location of a chain split")]
    ChainSplitNotFound,
    #[error("Failed to synchronize headers from peer: {0}")]
    GeneralHeaderSyncFailure(BlockHeaderSyncError),
    #[error("Peer did not respond timeously during RPC negotiation")]
    RpcNegotiationTimedOut,
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
    Ahead,
    /// Local node is lagging behind remote node
    Lagging(Box<ChainSplitInfo>),
}
