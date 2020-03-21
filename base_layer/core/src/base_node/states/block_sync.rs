// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    base_node::{
        base_node::BaseNodeStateMachine,
        comms_interface::OutboundNodeCommsInterface,
        states::{ListeningInfo, StateEvent},
    },
    blocks::{
        blockheader::{BlockHash, BlockHeader},
        Block,
    },
    chain_storage::{async_db, BlockchainBackend, BlockchainDatabase, ChainMetadata, ChainStorageError},
};
use core::cmp::min;
use derive_error::Error;
use log::*;
use rand::seq::SliceRandom;
use tari_comms::peer_manager::NodeId;
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

const LOG_TARGET: &str = "c::bn::states::block_sync";

// If more than one sync peer discovered with the correct chain, enable or disable the selection of a random sync peer
// to query headers and blocks.
const RANDOM_SYNC_PEER_WITH_CHAIN: bool = true;
// The maximum number of retry attempts a node can perform to request a particular block from remote nodes.
const MAX_HEADER_REQUEST_RETRY_ATTEMPTS: usize = 5;
const MAX_BLOCK_REQUEST_RETRY_ATTEMPTS: usize = 5;
// The maximum number of retry attempts for attempting to validly request and add the block at a specific block height
// to the chain.
const MAX_ADD_BLOCK_RETRY_ATTEMPTS: usize = 3;
// The number of headers that can be requested in a single query.
const HEADER_REQUEST_SIZE: usize = 100;

/// Configuration for the Block Synchronization.
#[derive(Clone, Copy)]
pub struct BlockSyncConfig {
    pub random_sync_peer_with_chain: bool,
    pub max_header_request_retry_attempts: usize,
    pub max_block_request_retry_attempts: usize,
    pub max_add_block_retry_attempts: usize,
    pub header_request_size: usize,
}

impl Default for BlockSyncConfig {
    fn default() -> Self {
        Self {
            random_sync_peer_with_chain: RANDOM_SYNC_PEER_WITH_CHAIN,
            max_header_request_retry_attempts: MAX_HEADER_REQUEST_RETRY_ATTEMPTS,
            max_block_request_retry_attempts: MAX_BLOCK_REQUEST_RETRY_ATTEMPTS,
            max_add_block_retry_attempts: MAX_ADD_BLOCK_RETRY_ATTEMPTS,
            header_request_size: HEADER_REQUEST_SIZE,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum BlockSyncError {
    MaxRequestAttemptsReached,
    MaxAddBlockAttemptsReached,
    ForkChainNotLinked,
    InvalidChainLink,
    EmptyBlockchain,
    EmptyNetworkBestBlock,
    ChainStorageError(ChainStorageError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockSyncInfo;

impl BlockSyncInfo {
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        network_tip: &ChainMetadata,
        sync_peers: &[NodeId],
    ) -> StateEvent
    {
        info!(target: LOG_TARGET, "Synchronizing missing blocks.");
        match synchronize_blocks(
            &shared.db,
            &mut shared.comms,
            &shared.config.block_sync_config,
            network_tip,
            sync_peers,
        )
        .await
        {
            Ok(()) => {
                info!(target: LOG_TARGET, "Block sync state has synchronised.");
                StateEvent::BlocksSynchronized
            },
            Err(BlockSyncError::MaxRequestAttemptsReached) => {
                warn!(
                    target: LOG_TARGET,
                    "Maximum unsuccessful header/block request attempts reached."
                );
                StateEvent::BlockSyncFailure
            },
            Err(BlockSyncError::MaxAddBlockAttemptsReached) => {
                warn!(
                    target: LOG_TARGET,
                    "Maximum unsuccessful block request and add attempts reached."
                );
                StateEvent::BlockSyncFailure
            },
            Err(BlockSyncError::ForkChainNotLinked) => {
                warn!(target: LOG_TARGET, "The network fork chain not linked to local chain.",);
                StateEvent::BlockSyncFailure
            },
            Err(BlockSyncError::InvalidChainLink) => {
                warn!(
                    target: LOG_TARGET,
                    "The network fork chain linked with invalid header sequence.",
                );
                StateEvent::BlockSyncFailure
            },
            Err(BlockSyncError::EmptyNetworkBestBlock) => {
                warn!(target: LOG_TARGET, "An empty network best block hash was received.",);
                StateEvent::BlockSyncFailure
            },
            Err(e) => StateEvent::FatalError(format!("Synchronizing blocks failed. {}", e)),
        }
    }
}

async fn synchronize_blocks<B: BlockchainBackend + 'static>(
    db: &BlockchainDatabase<B>,
    comms: &mut OutboundNodeCommsInterface,
    config: &BlockSyncConfig,
    network_metadata: &ChainMetadata,
    sync_peers: &[NodeId],
) -> Result<(), BlockSyncError>
{
    let local_metadata = db.get_metadata()?;
    if let Some(local_block_hash) = local_metadata.best_block.clone() {
        if let Some(network_block_hash) = network_metadata.best_block.clone() {
            info!(
                target: LOG_TARGET,
                "Checking if current chain lagging on best network chain."
            );
            let local_tip_height = local_metadata.height_of_longest_chain.unwrap_or(0);
            let network_tip_height = network_metadata.height_of_longest_chain.unwrap_or(0);
            let mut sync_height = local_tip_height + 1;
            if check_chain_split(
                comms,
                config,
                sync_peers,
                local_tip_height,
                network_tip_height,
                &local_block_hash,
                &network_block_hash,
            )
            .await?
            {
                info!(target: LOG_TARGET, "Chain split detected, finding chain split height.");
                let min_tip_height = min(local_tip_height, network_tip_height);
                sync_height = find_chain_split_height(db, comms, config, sync_peers, min_tip_height).await?;
                info!(target: LOG_TARGET, "Chain split found at height {}.", sync_height);
            } else {
                trace!(
                    target: LOG_TARGET,
                    "Block hash {} is common between our chain and the network.",
                    local_block_hash.to_hex()
                );
            }

            info!(target: LOG_TARGET, "Synchronize missing blocks.");
            for height in sync_height..=network_tip_height {
                request_and_add_block(db, comms, config, sync_peers, height).await?;
            }
            return Ok(());
        }
        return Err(BlockSyncError::EmptyNetworkBestBlock);
    }
    Err(BlockSyncError::EmptyBlockchain)
}

// Perform a basic check to determine if a chain split has occurred between the local and network chain. The
// determine_sync_mode from the listening state would have ensured that when we reach this code that the network tip has
// a higher accumulated difficulty compared to the local chain. In the case when the network height is lower, but has a
// higher accumulated difficulty, then a network split must have occurred as the local chain will have a different block
// at the shared height if the local tip has a lower accumulated difficulty compared to the network tip.
async fn check_chain_split(
    comms: &mut OutboundNodeCommsInterface,
    config: &BlockSyncConfig,
    sync_peers: &[NodeId],
    local_tip_height: u64,
    network_tip_height: u64,
    local_block_hash: &BlockHash,
    network_block_hash: &BlockHash,
) -> Result<bool, BlockSyncError>
{
    Ok(if network_tip_height > local_tip_height {
        let header = request_header(comms, config, sync_peers, local_tip_height).await?;
        *local_block_hash != header.hash()
    } else if network_tip_height == local_tip_height {
        *local_block_hash != *network_block_hash
    } else {
        true
    })
}

// Find the block height where the chain split occurs. The chain split height is the height of the first block that is
// not common between the local and network chains.
async fn find_chain_split_height<B: BlockchainBackend + 'static>(
    db: &BlockchainDatabase<B>,
    comms: &mut OutboundNodeCommsInterface,
    config: &BlockSyncConfig,
    sync_peers: &[NodeId],
    tip_height: u64,
) -> Result<u64, BlockSyncError>
{
    for block_nums in (1..=tip_height)
        .rev()
        .collect::<Vec<u64>>()
        .chunks(config.header_request_size)
    {
        let headers = request_headers(comms, config, sync_peers, block_nums).await?;
        for header in headers {
            // Check if header is linked to local chain
            if let Ok(prev_header) = async_db::fetch_header_with_block_hash(db.clone(), header.prev_hash.clone()).await
            {
                if prev_header.height + 1 == header.height {
                    return Ok(header.height);
                } else {
                    return Err(BlockSyncError::InvalidChainLink);
                }
            }
        }
    }
    Err(BlockSyncError::ForkChainNotLinked)
}

// Request a block from a remote sync peer and attempt to add it to the local blockchain.
async fn request_and_add_block<B: BlockchainBackend + 'static>(
    db: &BlockchainDatabase<B>,
    comms: &mut OutboundNodeCommsInterface,
    config: &BlockSyncConfig,
    sync_peers: &[NodeId],
    height: u64,
) -> Result<(), BlockSyncError>
{
    for attempt in 0..config.max_add_block_retry_attempts {
        let block = request_block(comms, config, sync_peers, height).await?;
        let block_hash = block.hash();
        match db.add_block(block.clone()) {
            Ok(_) => {
                info!(
                    target: LOG_TARGET,
                    "Block #{} ({}) successfully added to database",
                    block.header.height,
                    block_hash.to_hex()
                );
                debug!(target: LOG_TARGET, "Block added to database: {}", block,);
                return Ok(());
            },
            Err(ChainStorageError::InvalidBlock) => {
                warn!(
                    target: LOG_TARGET,
                    "Invalid block {} received from peer. Retrying",
                    block_hash.to_hex(),
                );
            },
            Err(ChainStorageError::ValidationError(_)) => {
                warn!(
                    target: LOG_TARGET,
                    "Validation on block {} from peer failed. Retrying",
                    block_hash.to_hex(),
                );
            },
            Err(e) => return Err(BlockSyncError::ChainStorageError(e)),
        }
        info!(target: LOG_TARGET, "Retrying block add. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxAddBlockAttemptsReached)
}

// Request a block from a remote sync peer.
async fn request_block(
    comms: &mut OutboundNodeCommsInterface,
    config: &BlockSyncConfig,
    sync_peers: &[NodeId],
    height: u64,
) -> Result<Block, BlockSyncError>
{
    for attempt in 1..=config.max_block_request_retry_attempts {
        let selected_sync_peer = select_sync_peer(config, sync_peers);
        let peer_note = selected_sync_peer
            .as_ref()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "a random peer".into());
        trace!(target: LOG_TARGET, "Requesting block {} from {}.", height, peer_note);
        match comms
            .request_blocks_from_peer(vec![height], selected_sync_peer.clone())
            .await
        {
            Ok(blocks) => {
                debug!(target: LOG_TARGET, "Received {} blocks from peer", blocks.len());
                if let Some(hist_block) = blocks.first() {
                    let block = hist_block.block();
                    trace!(target: LOG_TARGET, "{}", block);
                    if block.header.height == height {
                        return Ok(block.clone());
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            "This was NOT the block we were expecting. Expected {}. Got {}",
                            height,
                            block.header.height
                        );
                    }
                }
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to fetch blocks from peer: {:?}. Retrying.", e,
                );
            },
        }
        debug!(target: LOG_TARGET, "Retrying block download. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

// Request a header from a remote sync peer.
async fn request_header(
    comms: &mut OutboundNodeCommsInterface,
    config: &BlockSyncConfig,
    sync_peers: &[NodeId],
    height: u64,
) -> Result<BlockHeader, BlockSyncError>
{
    if let Some(header) = request_headers(comms, config, sync_peers, &[height]).await?.first() {
        return Ok(header.clone());
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

// Request a set of headers from a remote sync peer.
async fn request_headers(
    comms: &mut OutboundNodeCommsInterface,
    config: &BlockSyncConfig,
    sync_peers: &[NodeId],
    block_nums: &[u64],
) -> Result<Vec<BlockHeader>, BlockSyncError>
{
    for attempt in 1..=config.max_header_request_retry_attempts {
        let selected_sync_peer = select_sync_peer(config, sync_peers);
        let peer_note = selected_sync_peer
            .as_ref()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "a random peer".into());
        trace!(target: LOG_TARGET, "Requesting headers from {}.", peer_note);
        match comms
            .request_headers_from_peer(block_nums.to_vec(), selected_sync_peer.clone())
            .await
        {
            Ok(headers) => {
                debug!(target: LOG_TARGET, "Received {} headers from peer", headers.len());
                if block_nums.len() == headers.len() {
                    if (0..block_nums.len()).all(|i| headers[i].height == block_nums[i]) {
                        return Ok(headers);
                    } else {
                        debug!(target: LOG_TARGET, "This was NOT the headers we were expecting.");
                    }
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "Incorrect number of headers returned. Expected {}. Got {}",
                        block_nums.len(),
                        headers.len()
                    );
                }
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to fetch header from peer: {:?}. Retrying.", e,
                );
            },
        }
        debug!(target: LOG_TARGET, "Retrying header download. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

// Selects the first sync peer or a random peer from the set of sync peers that have the current network tip depending
// on the selected configuration.
fn select_sync_peer(config: &BlockSyncConfig, sync_peers: &[NodeId]) -> Option<NodeId> {
    if config.random_sync_peer_with_chain {
        sync_peers.choose(&mut rand::thread_rng())
    } else {
        sync_peers.first()
    }
    .map(Clone::clone)
}

/// State management for BlockSync -> Listening.
impl From<BlockSyncInfo> for ListeningInfo {
    fn from(_old_state: BlockSyncInfo) -> Self {
        ListeningInfo {}
    }
}

/// State management for Listening -> BlockSync. This change happens when a node has been temporarily disconnected
/// from the network, or a reorg has occurred.
impl From<ListeningInfo> for BlockSyncInfo {
    fn from(_old: ListeningInfo) -> Self {
        BlockSyncInfo {}
    }
}
