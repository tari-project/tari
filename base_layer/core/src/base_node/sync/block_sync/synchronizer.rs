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
use tracing;

use super::error::BlockSyncError;
use crate::{
    base_node::{
        sync::{hooks::Hooks, rpc, SyncPeer},
        BlockSyncConfig,
    },
    blocks::{Block, BlockValidationError, ChainBlock},
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    proto::base_node::SyncBlocksRequest,
    transactions::aggregated_body::AggregateBody,
    validation::{BlockSyncBodyValidation, ValidationError},
};

const LOG_TARGET: &str = "c::bn::block_sync";

pub struct BlockSynchronizer<B> {
    config: BlockSyncConfig,
    db: AsyncBlockchainDb<B>,
    connectivity: ConnectivityRequester,
    sync_peer: SyncPeer,
    block_validator: Arc<dyn BlockSyncBodyValidation>,
    hooks: Hooks,
}

impl<B: BlockchainBackend + 'static> BlockSynchronizer<B> {
    pub fn new(
        config: BlockSyncConfig,
        db: AsyncBlockchainDb<B>,
        connectivity: ConnectivityRequester,
        sync_peer: SyncPeer,
        block_validator: Arc<dyn BlockSyncBodyValidation>,
    ) -> Self {
        Self {
            config,
            db,
            connectivity,
            sync_peer,
            block_validator,
            hooks: Default::default(),
        }
    }

    pub fn on_progress<H>(&mut self, hook: H)
    where H: FnMut(Arc<ChainBlock>, u64, &NodeId) + Send + Sync + 'static {
        self.hooks.add_on_progress_block_hook(hook);
    }

    pub fn on_complete<H>(&mut self, hook: H)
    where H: FnMut(Arc<ChainBlock>) + Send + Sync + 'static {
        self.hooks.add_on_complete_hook(hook);
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn synchronize(&mut self) -> Result<(), BlockSyncError> {
        let peer_conn = self.connect_to_sync_peer().await?;
        let node_id = peer_conn.peer_node_id().clone();
        info!(
            target: LOG_TARGET,
            "Attempting to synchronize blocks with `{}`", node_id
        );
        match self.attempt_block_sync(peer_conn).await {
            Ok(_) => {
                self.db.cleanup_orphans().await?;
                Ok(())
            },
            Err(err @ BlockSyncError::ValidationError(ValidationError::AsyncTaskFailed(_))) => Err(err),
            Err(BlockSyncError::ValidationError(err)) => {
                match &err {
                    ValidationError::BlockHeaderError(_) => {},
                    ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots) |
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
                Err(err.into())
            },
            Err(err @ BlockSyncError::ProtocolViolation(_)) => {
                warn!(target: LOG_TARGET, "Banning peer: {}", err);
                self.ban_peer(node_id, &err).await?;
                Err(err)
            },
            Err(err) => Err(err),
        }
    }

    async fn connect_to_sync_peer(&mut self) -> Result<PeerConnection, BlockSyncError> {
        let connection = self.connectivity.dial_peer(self.sync_peer.node_id().clone()).await?;
        Ok(connection)
    }

    async fn attempt_block_sync(&mut self, mut conn: PeerConnection) -> Result<(), BlockSyncError> {
        let mut client = conn
            .connect_rpc_using_builder(rpc::BaseNodeSyncRpcClient::builder().with_deadline(Duration::from_secs(60)))
            .await?;
        self.synchronize_blocks(conn.peer_node_id(), &mut client).await?;
        Ok(())
    }

    async fn synchronize_blocks(
        &mut self,
        peer: &NodeId,
        client: &mut rpc::BaseNodeSyncRpcClient,
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
            peer,
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
        while let Some(block) = block_stream.next().await {
            let block = block?;

            let header = self
                .db
                .fetch_chain_header_by_block_hash(block.hash.clone())
                .await?
                .ok_or_else(|| {
                    BlockSyncError::ProtocolViolation("Peer sent hash for block header we do not have".into())
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
                "Validating block body #{} (PoW = {}, {})",
                current_height,
                header.header().pow_algo(),
                body.to_counts_string(),
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

            self.hooks.call_on_progress_block_hooks(block.clone(), tip_height, peer);

            debug!(
                target: LOG_TARGET,
                "Block body #{} added in {:.0?}, Tot_acc_diff {}, Monero {}, SHA3 {}",
                block.height(),
                timer.elapsed(),
                block
                    .accumulated_data()
                    .total_accumulated_difficulty
                    .to_formatted_string(&Locale::en),
                block.accumulated_data().accumulated_monero_difficulty,
                block.accumulated_data().accumulated_sha_difficulty,
            );
            current_block = Some(block);
        }

        if let Some(block) = current_block {
            self.hooks.call_on_complete_hooks(block);
        }

        debug!(target: LOG_TARGET, "Completed block sync with peer `{}`", peer);

        Ok(())
    }

    async fn ban_peer<T: ToString>(&mut self, node_id: NodeId, reason: T) -> Result<(), BlockSyncError> {
        let reason = reason.to_string();
        if self.config.sync_peers.contains(&node_id) {
            debug!(
                target: LOG_TARGET,
                "Not banning peer that is allowlisted for sync. Ban reason = {}", reason
            );
            return Ok(());
        }
        warn!(target: LOG_TARGET, "Banned sync peer because {}", reason);
        self.connectivity
            .ban_peer_until(node_id, self.config.ban_period, reason)
            .await
            .map_err(BlockSyncError::FailedToBan)?;
        Ok(())
    }
}
