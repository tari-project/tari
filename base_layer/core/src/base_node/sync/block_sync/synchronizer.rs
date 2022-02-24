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
use num_format::{Locale, ToFormattedString};
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId, PeerConnection};
use tari_utilities::{hex::Hex, Hashable};
use tracing;

use super::error::BlockSyncError;
use crate::{
    base_node::{
        sync::{hooks::Hooks, rpc, SyncPeer},
        BlockchainSyncConfig,
    },
    blocks::{Block, BlockValidationError, ChainBlock},
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    proto::base_node::SyncBlocksRequest,
    transactions::aggregated_body::AggregateBody,
    validation::{BlockSyncBodyValidation, ValidationError},
};

const LOG_TARGET: &str = "c::bn::block_sync";

pub struct BlockSynchronizer<B> {
    config: BlockchainSyncConfig,
    db: AsyncBlockchainDb<B>,
    connectivity: ConnectivityRequester,
    sync_peers: Vec<SyncPeer>,
    block_validator: Arc<dyn BlockSyncBodyValidation>,
    hooks: Hooks,
}

impl<B: BlockchainBackend + 'static> BlockSynchronizer<B> {
    pub fn new(
        config: BlockchainSyncConfig,
        db: AsyncBlockchainDb<B>,
        connectivity: ConnectivityRequester,
        sync_peers: Vec<SyncPeer>,
        block_validator: Arc<dyn BlockSyncBodyValidation>,
    ) -> Self {
        Self {
            config,
            db,
            connectivity,
            sync_peers,
            block_validator,
            hooks: Default::default(),
        }
    }

    pub fn on_progress<H>(&mut self, hook: H)
    where H: Fn(Arc<ChainBlock>, u64, &SyncPeer) + Send + Sync + 'static {
        self.hooks.add_on_progress_block_hook(hook);
    }

    pub fn on_complete<H>(&mut self, hook: H)
    where H: Fn(Arc<ChainBlock>) + Send + Sync + 'static {
        self.hooks.add_on_complete_hook(hook);
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn synchronize(&mut self) -> Result<(), BlockSyncError> {
        let mut max_latency = self.config.initial_max_sync_latency;
        loop {
            match self.attempt_block_sync(max_latency).await {
                Ok(_) => return Ok(()),
                Err(err @ BlockSyncError::AllSyncPeersExceedLatency) => {
                    warn!(target: LOG_TARGET, "{}", err);
                    if self.sync_peers.len() == 1 {
                        warn!(
                            target: LOG_TARGET,
                            "Insufficient sync peers to continue with block sync"
                        );
                        return Err(err);
                    }
                    max_latency += self.config.max_latency_increase;
                    warn!(
                        target: LOG_TARGET,
                        "Retrying block sync with increased max latency {:.2?} with {} sync peers",
                        max_latency,
                        self.sync_peers.len()
                    );
                    continue;
                },
                Err(err) => return Err(err),
            }
        }
    }

    async fn attempt_block_sync(&mut self, max_latency: Duration) -> Result<(), BlockSyncError> {
        let sync_peer_node_ids = self.sync_peers.iter().map(|p| p.node_id()).cloned().collect::<Vec<_>>();
        for (i, node_id) in sync_peer_node_ids.iter().enumerate() {
            let mut conn = self.connect_to_sync_peer(node_id.clone()).await?;
            let mut client = conn
                .connect_rpc_using_builder(rpc::BaseNodeSyncRpcClient::builder().with_deadline(Duration::from_secs(60)))
                .await?;
            let latency = client
                .get_last_request_latency()
                .expect("unreachable panic: last request latency must be set after connect");
            self.sync_peers[i].set_latency(latency);
            let sync_peer = self.sync_peers[i].clone();
            info!(
                target: LOG_TARGET,
                "Attempting to synchronize blocks with `{}` latency: {:.2?}", node_id, latency
            );
            match self.synchronize_blocks(sync_peer, client, max_latency).await {
                Ok(_) => {
                    self.db.cleanup_orphans().await?;
                    return Ok(());
                },
                Err(err @ BlockSyncError::ValidationError(ValidationError::AsyncTaskFailed(_))) => return Err(err),
                Err(BlockSyncError::ValidationError(err)) => {
                    match &err {
                        ValidationError::BlockHeaderError(_) => {},
                        ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots { .. }) |
                        ValidationError::BadBlockFound { .. } |
                        ValidationError::BlockError(BlockValidationError::MismatchedMmrSize { .. }) => {
                            let num_cleared = self.db.clear_all_pending_headers().await?;
                            warn!(
                                target: LOG_TARGET,
                                "Cleared {} incomplete headers from bad chain", num_cleared
                            );
                        },
                        _ => {},
                    }
                    warn!(
                        target: LOG_TARGET,
                        "Banning peer because provided block failed validation: {}", err
                    );
                    self.ban_peer(node_id, &err).await?;
                    return Err(err.into());
                },
                Err(err @ BlockSyncError::MaxLatencyExceeded { .. }) => {
                    warn!(target: LOG_TARGET, "{}", err);
                    if i == self.sync_peers.len() - 1 {
                        return Err(BlockSyncError::AllSyncPeersExceedLatency);
                    }
                    continue;
                },
                Err(err @ BlockSyncError::ProtocolViolation(_)) => {
                    warn!(target: LOG_TARGET, "Banning peer: {}", err);
                    self.ban_peer(node_id, &err).await?;
                    return Err(err);
                },
                Err(err) => return Err(err),
            }
        }

        Err(BlockSyncError::NoSyncPeers)
    }

    async fn connect_to_sync_peer(&self, peer: NodeId) -> Result<PeerConnection, BlockSyncError> {
        let connection = self.connectivity.dial_peer(peer).await?;
        Ok(connection)
    }

    async fn synchronize_blocks(
        &mut self,
        mut sync_peer: SyncPeer,
        mut client: rpc::BaseNodeSyncRpcClient,
        max_latency: Duration,
    ) -> Result<(), BlockSyncError> {
        self.hooks.call_on_starting_hook();

        let tip_header = self.db.fetch_last_header().await?;
        let local_metadata = self.db.get_chain_metadata().await?;
        if tip_header.height <= local_metadata.height_of_longest_chain() {
            debug!(
                target: LOG_TARGET,
                "Blocks already synchronized to height {}.", tip_header.height
            );
            return Ok(());
        }

        let tip_hash = tip_header.hash();
        let tip_height = tip_header.height;
        let best_height = local_metadata.height_of_longest_chain();
        let chain_header = self.db.fetch_chain_header(best_height).await?;

        let best_full_block_hash = chain_header.accumulated_data().hash.clone();
        debug!(
            target: LOG_TARGET,
            "Starting block sync from peer `{}`. Current best block is #{} `{}`. Syncing to #{} ({}).",
            sync_peer,
            best_height,
            best_full_block_hash.to_hex(),
            tip_height,
            tip_hash.to_hex()
        );
        let request = SyncBlocksRequest {
            start_hash: best_full_block_hash.clone(),
            // To the tip!
            end_hash: tip_hash.clone(),
        };

        let mut block_stream = client.sync_blocks(request).await?;
        let mut prev_hash = best_full_block_hash;
        let mut current_block = None;
        let mut last_sync_timer = Instant::now();
        while let Some(block) = block_stream.next().await {
            let latency = last_sync_timer.elapsed();
            let block = block?;

            let header = self
                .db
                .fetch_chain_header_by_block_hash(block.hash.clone())
                .await?
                .ok_or_else(|| {
                    BlockSyncError::ProtocolViolation(format!(
                        "Peer sent hash ({}) for block header we do not have",
                        block.hash.to_hex()
                    ))
                })?;

            let current_height = header.height();
            let header_hash = header.hash().clone();

            if header.header().prev_hash != prev_hash {
                return Err(BlockSyncError::PeerSentBlockThatDidNotFormAChain {
                    expected: prev_hash.to_hex(),
                    got: header.header().prev_hash.to_hex(),
                });
            }

            prev_hash = header_hash.clone();

            let body = block
                .body
                .map(AggregateBody::try_from)
                .ok_or_else(|| BlockSyncError::ProtocolViolation("Block body was empty".to_string()))?
                .map_err(BlockSyncError::ProtocolViolation)?;

            debug!(
                target: LOG_TARGET,
                "Validating block body #{} (PoW = {}, {}, latency: {:.2?})",
                current_height,
                header.header().pow_algo(),
                body.to_counts_string(),
                latency
            );

            let timer = Instant::now();
            let (header, header_accum_data) = header.into_parts();

            let block = match self.block_validator.validate_body(Block::new(header, body)).await {
                Ok(block) => block,
                Err(err @ ValidationError::BadBlockFound { .. }) |
                Err(err @ ValidationError::FatalStorageError(_)) |
                Err(err @ ValidationError::AsyncTaskFailed(_)) |
                Err(err @ ValidationError::CustomError(_)) => return Err(err.into()),
                Err(err) => {
                    // Add to bad blocks
                    if let Err(err) = self
                        .db
                        .write_transaction()
                        .insert_bad_block(header_hash, current_height)
                        .commit()
                        .await
                    {
                        error!(target: LOG_TARGET, "Failed to insert bad block: {}", err);
                    }
                    return Err(err.into());
                },
            };

            let block = ChainBlock::try_construct(Arc::new(block), header_accum_data)
                .map(Arc::new)
                .ok_or(BlockSyncError::FailedToConstructChainBlock)?;

            debug!(
                target: LOG_TARGET,
                "Validated in {:.0?}. Storing block body #{} (PoW = {}, {})",
                timer.elapsed(),
                block.header().height,
                block.header().pow_algo(),
                block.block().body.to_counts_string(),
            );

            let timer = Instant::now();
            self.db
                .write_transaction()
                .insert_block_body(block.clone())
                .set_best_block(
                    block.height(),
                    header_hash,
                    block.accumulated_data().total_accumulated_difficulty,
                    block.header().prev_hash.clone(),
                )
                .commit()
                .await?;

            sync_peer.set_latency(latency);
            sync_peer.add_sample(last_sync_timer.elapsed());
            self.hooks
                .call_on_progress_block_hooks(block.clone(), tip_height, &sync_peer);

            debug!(
                target: LOG_TARGET,
                "Block body #{} added in {:.0?}, Tot_acc_diff {}, Monero {}, SHA3 {}, latency: {:.2?}",
                block.height(),
                timer.elapsed(),
                block
                    .accumulated_data()
                    .total_accumulated_difficulty
                    .to_formatted_string(&Locale::en),
                block.accumulated_data().accumulated_monero_difficulty,
                block.accumulated_data().accumulated_sha_difficulty,
                latency
            );
            if latency > max_latency {
                return Err(BlockSyncError::MaxLatencyExceeded {
                    peer: sync_peer.node_id().clone(),
                    latency,
                    max_latency,
                });
            }

            current_block = Some(block);
            last_sync_timer = Instant::now();
        }

        if let Some(block) = current_block {
            self.hooks.call_on_complete_hooks(block);
        }

        debug!(target: LOG_TARGET, "Completed block sync with peer `{}`", sync_peer);

        Ok(())
    }

    async fn ban_peer<T: ToString>(&mut self, node_id: &NodeId, reason: T) -> Result<(), BlockSyncError> {
        let reason = reason.to_string();
        if self.config.forced_sync_peers.contains(node_id) {
            debug!(
                target: LOG_TARGET,
                "Not banning peer that is allowlisted for sync. Ban reason = {}", reason
            );
            return Ok(());
        }
        if let Some(pos) = self.sync_peers.iter().position(|p| p.node_id() == node_id) {
            self.sync_peers.remove(pos);
            if self.sync_peers.is_empty() {
                return Err(BlockSyncError::NoSyncPeers);
            }
        }
        warn!(target: LOG_TARGET, "Banned sync peer because {}", reason);
        if let Err(err) = self
            .connectivity
            .ban_peer_until(node_id.clone(), self.config.ban_period, reason)
            .await
        {
            error!(target: LOG_TARGET, "Failed to ban peer: {}", err);
        }
        Ok(())
    }
}
