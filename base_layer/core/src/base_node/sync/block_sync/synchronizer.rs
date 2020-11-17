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

use super::error::BlockSyncError;
use crate::{
    base_node::sync::{hooks::Hooks, rpc},
    blocks::Block,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, MetadataKey, MetadataValue},
    proto::base_node::SyncBlocksRequest,
    tari_utilities::{hex::Hex, Hashable},
    transactions::aggregated_body::AggregateBody,
    validation::Validation,
};
use futures::StreamExt;
use log::*;
use std::{
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_comms::{
    connectivity::{ConnectivityRequester, ConnectivitySelection},
    peer_manager::NodeId,
    PeerConnection,
};
use tokio::task;

const LOG_TARGET: &str = "c::bn::block_sync";

pub struct BlockSynchronizer<B> {
    db: AsyncBlockchainDb<B>,
    connectivity: ConnectivityRequester,
    sync_peer: Option<PeerConnection>,
    block_validator: Arc<dyn Validation<Block>>,
    hooks: Hooks,
}

impl<B: BlockchainBackend + 'static> BlockSynchronizer<B> {
    pub fn new(
        db: AsyncBlockchainDb<B>,
        connectivity: ConnectivityRequester,
        sync_peer: Option<PeerConnection>,
        block_validator: Arc<dyn Validation<Block>>,
    ) -> Self
    {
        Self {
            db,
            connectivity,
            sync_peer,
            block_validator,
            hooks: Default::default(),
        }
    }

    pub fn on_progress<H>(&mut self, hook: H)
    where H: FnMut(Arc<Block>, u64, &[NodeId]) + Send + Sync + 'static {
        self.hooks.add_on_progress_block_hook(hook);
    }

    pub fn on_complete<H>(&mut self, hook: H)
    where H: FnMut(Arc<Block>) + Send + Sync + 'static {
        self.hooks.add_on_complete_hook(hook);
    }

    pub async fn synchronize(&mut self) -> Result<(), BlockSyncError> {
        let peer_conn = self.get_next_sync_peer().await?;
        let node_id = peer_conn.peer_node_id().clone();
        debug!(
            target: LOG_TARGET,
            "Attempting to synchronize blocks with `{}`", node_id
        );
        self.attempt_block_sync(peer_conn).await?;

        Ok(())
    }

    async fn get_next_sync_peer(&mut self) -> Result<PeerConnection, BlockSyncError> {
        match self.sync_peer {
            Some(ref peer) => Ok(peer.clone()),
            None => {
                let mut peers = self
                    .connectivity
                    .select_connections(ConnectivitySelection::random_nodes(1, vec![]))
                    .await?;
                if peers.is_empty() {
                    return Err(BlockSyncError::NoSyncPeers);
                }
                Ok(peers.remove(0))
            },
        }
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
    ) -> Result<(), BlockSyncError>
    {
        let tip_header = self.db.fetch_tip_header().await?;
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
        let best_block = self
            .db
            .fetch_header(best_height)
            .await?
            .ok_or_else(|| BlockSyncError::ExpectedHeaderNotFound(best_height))?;
        let best_full_block_hash = best_block.hash();
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
            end_hash: tip_hash,
        };

        let mut block_stream = client.sync_blocks(request).await?;
        let mut prev_hash = best_full_block_hash;
        let mut current_block = None;
        while let Some(block) = block_stream.next().await {
            let block = block?;

            let header = self
                .db
                .fetch_header_by_block_hash(block.hash.clone())
                .await?
                .ok_or_else(|| {
                    BlockSyncError::ReceivedInvalidBlockBody("Peer sent hash for block header we do not have".into())
                })?;

            let header_hash = header.hash();

            if header.prev_hash != prev_hash {
                return Err(BlockSyncError::PeerSentBlockThatDidNotFormAChain {
                    expected: prev_hash.to_hex(),
                    got: header.prev_hash.to_hex(),
                });
            }

            let body = block
                .body
                .map(AggregateBody::try_from)
                .ok_or_else(|| BlockSyncError::ReceivedInvalidBlockBody("Block body was empty".to_string()))?
                .map_err(BlockSyncError::ReceivedInvalidBlockBody)?;

            prev_hash = header.hash();

            debug!(
                target: LOG_TARGET,
                "Validating block body #{} (PoW = {}, {})",
                header.height,
                header.pow_algo(),
                body.to_counts_string(),
            );

            let timer = Instant::now();
            let block = Arc::new(Block::new(header, body));
            self.validate_block(block.clone()).await?;

            debug!(
                target: LOG_TARGET,
                "Validated in {:.0?}. Storing block body #{} (PoW = {}, {})",
                timer.elapsed(),
                block.header.height,
                block.header.pow_algo(),
                block.body.to_counts_string(),
            );

            let timer = Instant::now();
            self.db
                .write_transaction()
                .insert_block(block.clone())
                .set_metadata(
                    MetadataKey::ChainHeight,
                    MetadataValue::ChainHeight(block.header.height),
                )
                .set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(header_hash))
                .commit()
                .await?;

            self.hooks
                .call_on_progress_block_hooks(block.clone(), tip_height, &[peer.clone()]);

            debug!(
                target: LOG_TARGET,
                "Block body #{} added in {:.0?}",
                block.header.height,
                timer.elapsed()
            );
            current_block = Some(block);
        }

        if let Some(block) = current_block {
            // Update metadata to last tip header
            let header = &block.header;
            let height = header.height;
            let best_block = header.hash();
            let accumulated_difficulty = header.get_proof_of_work()?.total_accumulated_difficulty();
            self.db
                .write_transaction()
                .set_metadata(MetadataKey::ChainHeight, MetadataValue::ChainHeight(height))
                .set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(best_block.to_vec()))
                .set_metadata(
                    MetadataKey::AccumulatedWork,
                    MetadataValue::AccumulatedWork(accumulated_difficulty),
                )
                .commit()
                .await?;

            self.hooks.call_on_complete_hooks(block);
        }

        debug!(target: LOG_TARGET, "Completed block sync with peer `{}`", peer);

        Ok(())
    }

    async fn validate_block(&self, block: Arc<Block>) -> Result<(), BlockSyncError> {
        let validator = self.block_validator.clone();
        task::spawn_blocking(move || {
            validator.validate(&block)?;
            Result::<_, BlockSyncError>::Ok(())
        })
        .await
        .expect("block validator panicked")
    }
}
