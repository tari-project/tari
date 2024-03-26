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
    convert::{TryFrom, TryInto},
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use log::*;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId, protocol::rpc::RpcClient, PeerConnection};
use tari_utilities::hex::Hex;
use tokio::task;

use super::error::BlockSyncError;
use crate::{
    base_node::{
        sync::{ban::PeerBanManager, hooks::Hooks, rpc, SyncPeer},
        BlockchainSyncConfig,
    },
    blocks::{Block, ChainBlock},
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    common::{rolling_avg::RollingAverageTime, BanPeriod},
    proto::base_node::SyncBlocksRequest,
    transactions::aggregated_body::AggregateBody,
    validation::{BlockBodyValidator, ValidationError},
};

const LOG_TARGET: &str = "c::bn::block_sync";

const MAX_LATENCY_INCREASES: usize = 5;

pub struct BlockSynchronizer<'a, B> {
    config: BlockchainSyncConfig,
    db: AsyncBlockchainDb<B>,
    connectivity: ConnectivityRequester,
    sync_peers: &'a mut Vec<SyncPeer>,
    block_validator: Arc<dyn BlockBodyValidator<B>>,
    hooks: Hooks,
    peer_ban_manager: PeerBanManager,
}

impl<'a, B: BlockchainBackend + 'static> BlockSynchronizer<'a, B> {
    pub fn new(
        config: BlockchainSyncConfig,
        db: AsyncBlockchainDb<B>,
        connectivity: ConnectivityRequester,
        sync_peers: &'a mut Vec<SyncPeer>,
        block_validator: Arc<dyn BlockBodyValidator<B>>,
    ) -> Self {
        let peer_ban_manager = PeerBanManager::new(config.clone(), connectivity.clone());
        Self {
            config,
            db,
            connectivity,
            sync_peers,
            block_validator,
            hooks: Default::default(),
            peer_ban_manager,
        }
    }

    pub fn on_starting<H>(&mut self, hook: H)
    where for<'r> H: FnOnce(&SyncPeer) + Send + Sync + 'static {
        self.hooks.add_on_starting_hook(hook);
    }

    pub fn on_progress<H>(&mut self, hook: H)
    where H: Fn(Arc<ChainBlock>, u64, &SyncPeer) + Send + Sync + 'static {
        self.hooks.add_on_progress_block_hook(hook);
    }

    pub fn on_complete<H>(&mut self, hook: H)
    where H: Fn(Arc<ChainBlock>, u64) + Send + Sync + 'static {
        self.hooks.add_on_complete_hook(hook);
    }

    pub async fn synchronize(&mut self) -> Result<(), BlockSyncError> {
        let mut max_latency = self.config.initial_max_sync_latency;
        let mut sync_round = 0;
        let mut latency_increases_counter = 0;
        loop {
            match self.attempt_block_sync(max_latency).await {
                Ok(_) => return Ok(()),
                Err(err @ BlockSyncError::AllSyncPeersExceedLatency) => {
                    warn!(target: LOG_TARGET, "{}", err);
                    max_latency += self.config.max_latency_increase;
                    warn!(
                        target: LOG_TARGET,
                        "Retrying block sync with increased max latency {:.2?} with {} sync peers",
                        max_latency,
                        self.sync_peers.len()
                    );
                    latency_increases_counter += 1;
                    if latency_increases_counter > MAX_LATENCY_INCREASES {
                        return Err(err);
                    }
                    // Prohibit using a few slow sync peers only, rather get new sync peers assigned
                    if self.sync_peers.len() < 2 {
                        return Err(err);
                    } else {
                        continue;
                    }
                },
                Err(err @ BlockSyncError::SyncRoundFailed) => {
                    sync_round += 1;
                    warn!(target: LOG_TARGET, "{} ({})", err, sync_round);
                    continue;
                },
                Err(err) => {
                    return Err(err);
                },
            }
        }
    }

    async fn attempt_block_sync(&mut self, max_latency: Duration) -> Result<(), BlockSyncError> {
        let sync_peer_node_ids = self.sync_peers.iter().map(|p| p.node_id()).cloned().collect::<Vec<_>>();
        info!(
            target: LOG_TARGET,
            "Attempting to sync blocks({} sync peers)",
            sync_peer_node_ids.len()
        );
        let mut latency_counter = 0usize;
        for node_id in sync_peer_node_ids {
            let peer_index = self.get_sync_peer_index(&node_id).ok_or(BlockSyncError::PeerNotFound)?;
            let sync_peer = &self.sync_peers[peer_index];
            self.hooks.call_on_starting_hook(sync_peer);
            let mut conn = match self.connect_to_sync_peer(node_id.clone()).await {
                Ok(val) => val,
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to connect to sync peer `{}`: {}", node_id, e
                    );
                    self.remove_sync_peer(&node_id);
                    continue;
                },
            };
            let config = RpcClient::builder()
                .with_deadline(self.config.rpc_deadline)
                .with_deadline_grace_period(Duration::from_secs(5));
            let mut client = match conn
                .connect_rpc_using_builder::<rpc::BaseNodeSyncRpcClient>(config)
                .await
            {
                Ok(val) => val,
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to obtain RPC connection from sync peer `{}`: {}", node_id, e
                    );
                    self.remove_sync_peer(&node_id);
                    continue;
                },
            };
            let latency = client
                .get_last_request_latency()
                .expect("unreachable panic: last request latency must be set after connect");
            self.sync_peers[peer_index].set_latency(latency);
            let sync_peer = self.sync_peers[peer_index].clone();
            info!(
                target: LOG_TARGET,
                "Attempting to synchronize blocks with `{}` latency: {:.2?}", node_id, latency
            );
            match self.synchronize_blocks(sync_peer, client, max_latency).await {
                Ok(_) => return Ok(()),
                Err(err) => {
                    warn!(target: LOG_TARGET, "{}", err);
                    let ban_reason = BlockSyncError::get_ban_reason(&err);
                    if let Some(reason) = ban_reason {
                        let duration = match reason.ban_duration {
                            BanPeriod::Short => self.config.short_ban_period,
                            BanPeriod::Long => self.config.ban_period,
                        };
                        warn!(target: LOG_TARGET, "{}", err);
                        self.peer_ban_manager
                            .ban_peer_if_required(&node_id, reason.reason, duration)
                            .await;
                    }
                    if let BlockSyncError::MaxLatencyExceeded { .. } = err {
                        latency_counter += 1;
                    } else {
                        self.remove_sync_peer(&node_id);
                    }
                },
            }
        }

        if self.sync_peers.is_empty() {
            Err(BlockSyncError::NoMoreSyncPeers("Block sync failed".to_string()))
        } else if latency_counter >= self.sync_peers.len() {
            Err(BlockSyncError::AllSyncPeersExceedLatency)
        } else {
            Err(BlockSyncError::SyncRoundFailed)
        }
    }

    async fn connect_to_sync_peer(&self, peer: NodeId) -> Result<PeerConnection, BlockSyncError> {
        let connection = self.connectivity.dial_peer(peer).await?;
        Ok(connection)
    }

    #[allow(clippy::too_many_lines)]
    async fn synchronize_blocks(
        &mut self,
        mut sync_peer: SyncPeer,
        mut client: rpc::BaseNodeSyncRpcClient,
        max_latency: Duration,
    ) -> Result<(), BlockSyncError> {
        info!(target: LOG_TARGET, "Starting block sync from peer {}", sync_peer);

        let tip_header = self.db.fetch_last_header().await?;
        let local_metadata = self.db.get_chain_metadata().await?;

        if tip_header.height <= local_metadata.best_block_height() {
            debug!(
                target: LOG_TARGET,
                "Blocks already synchronized to height {}.", tip_header.height
            );
            return Ok(());
        }

        let tip_hash = tip_header.hash();
        let tip_height = tip_header.height;
        let best_height = local_metadata.best_block_height();
        let chain_header = self.db.fetch_chain_header(best_height).await?;

        let best_full_block_hash = chain_header.accumulated_data().hash;
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
            start_hash: best_full_block_hash.to_vec(),
            // To the tip!
            end_hash: tip_hash.to_vec(),
        };

        let mut block_stream = client.sync_blocks(request).await?;
        let mut prev_hash = best_full_block_hash;
        let mut current_block = None;
        let mut last_sync_timer = Instant::now();
        let mut avg_latency = RollingAverageTime::new(20);
        while let Some(block_result) = block_stream.next().await {
            let latency = last_sync_timer.elapsed();
            avg_latency.add_sample(latency);
            let block_body_response = block_result?;

            let header = self
                .db
                .fetch_chain_header_by_block_hash(block_body_response.hash.clone().try_into()?)
                .await?
                .ok_or_else(|| {
                    BlockSyncError::UnknownHeaderHash(format!(
                        "Peer sent hash ({}) for block header we do not have",
                        block_body_response.hash.to_hex()
                    ))
                })?;

            let current_height = header.height();
            let header_hash = *header.hash();
            let timestamp = header.timestamp();

            if header.header().prev_hash != prev_hash {
                return Err(BlockSyncError::BlockWithoutParent {
                    expected: prev_hash.to_hex(),
                    got: header.header().prev_hash.to_hex(),
                });
            }

            prev_hash = header_hash;

            let body = block_body_response
                .body
                .map(AggregateBody::try_from)
                .ok_or_else(|| BlockSyncError::InvalidBlockBody("Peer sent empty block".to_string()))?
                .map_err(BlockSyncError::InvalidBlockBody)?;

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
            let block = Block::new(header, body);

            // Validate the block inside a tokio task
            let task_block = block.clone();
            let db = self.db.inner().clone();
            let validator = self.block_validator.clone();
            let res = task::spawn_blocking(move || {
                let txn = db.db_read_access()?;
                validator.validate_body(&*txn, &task_block)
            })
            .await?;

            let block = match res {
                Ok(block) => block,
                Err(err @ ValidationError::BadBlockFound { .. }) | Err(err @ ValidationError::FatalStorageError(_)) => {
                    return Err(err.into());
                },
                Err(err) => {
                    // Add to bad blocks
                    if let Err(err) = self
                        .db
                        .write_transaction()
                        .delete_orphan(header_hash)
                        .insert_bad_block(header_hash, current_height, err.to_string())
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
            trace!(
                target: LOG_TARGET,
                "{}",block
            );

            let timer = Instant::now();
            self.db
                .write_transaction()
                .delete_orphan(header_hash)
                .insert_tip_block_body(block.clone())
                .set_best_block(
                    block.height(),
                    header_hash,
                    block.accumulated_data().total_accumulated_difficulty,
                    block.header().prev_hash,
                    timestamp,
                )
                .commit()
                .await?;

            // Average time between receiving blocks from the peer - used to detect a slow sync peer
            let last_avg_latency = avg_latency.calculate_average_with_min_samples(5);
            if let Some(latency) = last_avg_latency {
                sync_peer.set_latency(latency);
            }
            // Includes time to add block to database, used to show blocks/s on status line
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
                    .total_accumulated_difficulty,
                block.accumulated_data().accumulated_randomx_difficulty,
                block.accumulated_data().accumulated_sha3x_difficulty,
                latency
            );
            if let Some(avg_latency) = last_avg_latency {
                if avg_latency > max_latency {
                    return Err(BlockSyncError::MaxLatencyExceeded {
                        peer: sync_peer.node_id().clone(),
                        latency: avg_latency,
                        max_latency,
                    });
                }
            }

            current_block = Some(block);
            last_sync_timer = Instant::now();
        }

        let accumulated_difficulty = self.db.get_chain_metadata().await?.accumulated_difficulty();
        if accumulated_difficulty < sync_peer.claimed_chain_metadata().accumulated_difficulty() {
            return Err(BlockSyncError::PeerDidNotSupplyAllClaimedBlocks(format!(
                "Their claimed difficulty: {}, our local difficulty after block sync: {}",
                sync_peer.claimed_chain_metadata().accumulated_difficulty(),
                accumulated_difficulty
            )));
        }

        if let Some(block) = current_block {
            self.hooks.call_on_complete_hooks(block, best_height);
        }

        debug!(target: LOG_TARGET, "Completed block sync with peer `{}`", sync_peer);

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
