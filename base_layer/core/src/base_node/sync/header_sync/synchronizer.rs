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
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, ChainBlock},
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
use std::{convert::TryFrom, sync::Arc, time::Duration};
use tari_comms::{
    connectivity::{ConnectivityError, ConnectivityRequester, ConnectivitySelection},
    peer_manager::NodeId,
    protocol::rpc::RpcError,
    PeerConnection,
};

const LOG_TARGET: &str = "c::bn::header_sync";

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
                Err(BlockHeaderSyncError::ValidationFailed(err)) => {
                    debug!(target: LOG_TARGET, "Block header validation failed: {}", err);
                    self.ban_peer_temporarily(node_id, err.into()).await?;
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to synchronize headers from peer `{}`: {}", node_id, err
                    );
                    self.ban_peer_temporarily(node_id, BanReason::GeneralHeaderSyncFailure)
                        .await?;
                },
            }
        }

        Err(BlockHeaderSyncError::SyncFailedAllPeers)
    }

    async fn wait_until_online(&mut self) -> Result<(), BlockHeaderSyncError> {
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
                    if attempts > 6 && n > 0 {
                        warn!(
                            target: LOG_TARGET,
                            "This node is still not well connected, attempting to sync with {} node(s).", n
                        );
                        break Ok(());
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

    async fn ban_peer_temporarily(&mut self, node_id: NodeId, reason: BanReason) -> Result<(), BlockHeaderSyncError> {
        if self.config.sync_peers.contains(&node_id) {
            debug!(
                target: LOG_TARGET,
                "Not banning peer that is allowlisted for sync. Ban reason = {}", reason
            );
            return Ok(());
        }
        warn!(target: LOG_TARGET, "Banned sync peer because {}", reason);
        self.connectivity
            .ban_peer_until(node_id, self.config.ban_period, reason.to_string())
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

        let sync_complete = self.check_chain_split(&peer, &mut client).await?;
        // If sync is not complete after the chain split check, synchronize the rest of the headers
        if !sync_complete {
            self.synchronize_headers(&peer, &mut client).await?;
        }

        Ok(())
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
                self.ban_peer_temporarily(peer.clone(), BanReason::ChainSplitNotFound)
                    .await?;
                return Err(BlockHeaderSyncError::ChainSplitNotFound(peer.clone()));
            }

            let block_hashes = self
                .db
                .fetch_block_hashes_from_header_tip(NUM_CHAIN_SPLIT_HEADERS, offset)
                .await?;
            debug!(
                target: LOG_TARGET,
                "Determining where our chain splits with the remote peer `{}` ({} block hashes sent)",
                peer,
                block_hashes.len()
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

    /// Check for a chain split with the given peer, validate and add header state. State will be rewound if a higher
    /// proof of work is achieved. If, after this step, the headers are already in sync, true is returned, otherwise a
    /// header sync should proceed to download the remaining headers.
    async fn check_chain_split(
        &mut self,
        peer: &NodeId,
        client: &mut rpc::BaseNodeSyncRpcClient,
    ) -> Result<bool, BlockHeaderSyncError>
    {
        const NUM_HEADERS_TO_REQUEST: u64 = 1000;
        let (resp, block_hashes, steps_back) = self.find_chain_split(peer, client, NUM_HEADERS_TO_REQUEST).await?;
        if resp.headers.len() > NUM_HEADERS_TO_REQUEST as usize {
            self.ban_peer_temporarily(peer.clone(), BanReason::PeerSentTooManyHeaders(resp.headers.len()))
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
                .ban_peer_temporarily(peer.clone(), BanReason::SplitHashGreaterThanHashes {
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
                // Another peer will be attempted, if possible
                return Err(BlockHeaderSyncError::NotInSync);
            }

            debug!(target: LOG_TARGET, "Already in sync with peer `{}`.", peer);
            return Ok(true);
        }

        // We can trust that the header associated with this hash exists because block_hashes is data this node
        // supplied. usize conversion overflow has already been checked above
        let chain_split_hash = block_hashes[fork_hash_index as usize].clone();

        let headers = headers
            .into_iter()
            .map(BlockHeader::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BlockHeaderSyncError::ReceivedInvalidHeader)?;
        let num_new_headers = headers.len();

        self.header_validator.initialize_state(chain_split_hash).await?;
        let mut chain_headers = Vec::with_capacity(headers.len());
        for header in headers {
            debug!(
                target: LOG_TARGET,
                "Validating header #{} (Pow: {})",
                header.height,
                header.pow_algo(),
            );
            let height = header.height;
            chain_headers.push(self.header_validator.validate_and_calculate_metadata(header)?);
            debug!(target: LOG_TARGET, "Header #{} is VALID", height,)
        }

        if fork_hash_index > 0 {
            // If the peer is telling us that we have to rewind, check their headers are stronger than our current tip
            self.header_validator
                .check_stronger_chain(chain_headers.last().expect("already_checked"))
                .await?;

            // TODO: We've established that the peer has a chain that forks from ours, can provide valid headers for
            // _part_ of that chain and is stronger than our chain at the same (or less) height.
            // However, to know that the full chain is stronger than our tip header, we need to
            // download all headers and compare.

            debug!(target: LOG_TARGET, "Rewinding the chain by {} block(s)", steps_back);
            let blocks = self.rewind_blockchain(steps_back).await?;
            self.hooks.call_on_rewind_hooks(blocks);
        }

        let mut txn = self.db.write_transaction();
        let current_height = chain_headers.last().map(|h| h.height()).unwrap_or(remote_tip_height);
        chain_headers.into_iter().for_each(|header| {
            debug!(target: LOG_TARGET, "Adding header: #{}", header.header.height);
            txn.insert_header(header.header, header.accumulated_data);
        });
        txn.commit().await?;

        self.hooks
            .call_on_progress_header_hooks(current_height, remote_tip_height, self.sync_peers);

        // If less headers were returned than requested, the peer is indicating that we have the tip header.
        // To indicate that sync is complete, true is returned, otherwise false
        Ok(num_new_headers < NUM_HEADERS_TO_REQUEST as usize)
    }

    async fn rewind_blockchain(&self, steps_back: u64) -> Result<Vec<Arc<ChainBlock>>, BlockHeaderSyncError> {
        debug!(
            target: LOG_TARGET,
            "Deleting {} header(s) that no longer form part of the main chain", steps_back
        );

        let tip_header = self.db.fetch_last_header().await?;
        let new_tip_height = tip_header.height - steps_back;

        let blocks = self.db.rewind_to_height(new_tip_height).await?;
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
    ) -> Result<(), BlockHeaderSyncError>
    {
        let tip_header = self.db.fetch_last_header().await?;
        debug!(
            target: LOG_TARGET,
            "Requesting header stream starting from tip header #{} from peer `{}`", tip_header.height, peer
        );
        let request = SyncHeadersRequest {
            start_hash: tip_header.hash(),
            // To the tip!
            count: 0,
        };
        let mut header_stream = client.sync_headers(request).await?;

        debug!(target: LOG_TARGET, "Reading headers from peer `{}`", peer);

        while let Some(header) = header_stream.next().await {
            let header = BlockHeader::try_from(header?).map_err(BlockHeaderSyncError::ReceivedInvalidHeader)?;
            debug!(
                target: LOG_TARGET,
                "Validating and adding header: #{} (PoW = {}), ",
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
            let chain_header = self.header_validator.validate_and_calculate_metadata(header)?;
            let current_height = chain_header.height();
            self.db
                .write_transaction()
                .insert_header(chain_header.header, chain_header.accumulated_data)
                .commit()
                .await?;

            self.hooks
                .call_on_progress_header_hooks(current_height, current_height, self.sync_peers);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
enum BanReason {
    #[error("This peer sent too many headers ({0}) in response to a chain split request")]
    PeerSentTooManyHeaders(usize),
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
    #[error("Failed to synchronize headers from peer")]
    GeneralHeaderSyncFailure,
}
