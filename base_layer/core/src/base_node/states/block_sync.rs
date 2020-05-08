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
        comms_interface::{Broadcast, CommsInterfaceError},
        state_machine::BaseNodeStateMachine,
        states::{ForwardBlockSyncInfo, ListeningInfo, StateEvent},
    },
    blocks::{
        blockheader::{BlockHash, BlockHeader},
        Block,
    },
    chain_storage::{async_db, BlockchainBackend, ChainMetadata, ChainStorageError},
};
use core::cmp::min;
use derive_error::Error;
use log::*;
use rand::seq::SliceRandom;
use std::{str::FromStr, time::Duration};
use tari_comms::{
    connection_manager::ConnectionManagerError,
    peer_manager::{NodeId, PeerManagerError},
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

const LOG_TARGET: &str = "c::bn::states::block_sync";

// If more than one sync peer discovered with the correct chain, enable or disable the selection of a random sync peer
// to query headers and blocks.
const RANDOM_SYNC_PEER_WITH_CHAIN: bool = true;
// The maximum number of retry attempts a node can perform to request a particular block from remote nodes.
const MAX_METADATA_REQUEST_RETRY_ATTEMPTS: usize = 3;
const MAX_HEADER_REQUEST_RETRY_ATTEMPTS: usize = 5;
const MAX_BLOCK_REQUEST_RETRY_ATTEMPTS: usize = 5;
// The maximum number of retry attempts for attempting to validly request and add the block at a specific block height
// to the chain.
const MAX_ADD_BLOCK_RETRY_ATTEMPTS: usize = 3;
// The number of headers that can be requested in a single query.
const HEADER_REQUEST_SIZE: usize = 100;
// The number of blocks that can be requested in a single query.
const BLOCK_REQUEST_SIZE: usize = 5;
// The default length of time to ban a misbehaving/malfunctioning sync peer (24 hours)
const DEFAULT_PEER_BAN_DURATION: Duration = Duration::from_secs(24 * 60 * 60);
// The length of time for a short term ban of a misbehaving/malfunctioning sync peer (5 min)
const SHORT_TERM_PEER_BAN_DURATION: Duration = Duration::from_secs(5 * 60);

/// Configuration for the Block Synchronization.
#[derive(Clone, Copy)]
pub struct BlockSyncConfig {
    pub sync_strategy: BlockSyncStrategy,
    pub random_sync_peer_with_chain: bool,
    pub max_metadata_request_retry_attempts: usize,
    pub max_header_request_retry_attempts: usize,
    pub max_block_request_retry_attempts: usize,
    pub max_add_block_retry_attempts: usize,
    pub header_request_size: usize,
    pub block_request_size: usize,
    pub peer_ban_duration: Duration,
    pub short_term_peer_ban_duration: Duration,
}

impl Default for BlockSyncConfig {
    fn default() -> Self {
        Self {
            sync_strategy: BlockSyncStrategy::ViaBestChainMetadata(BestChainMetadataBlockSyncInfo),
            random_sync_peer_with_chain: RANDOM_SYNC_PEER_WITH_CHAIN,
            max_metadata_request_retry_attempts: MAX_METADATA_REQUEST_RETRY_ATTEMPTS,
            max_header_request_retry_attempts: MAX_HEADER_REQUEST_RETRY_ATTEMPTS,
            max_block_request_retry_attempts: MAX_BLOCK_REQUEST_RETRY_ATTEMPTS,
            max_add_block_retry_attempts: MAX_ADD_BLOCK_RETRY_ATTEMPTS,
            header_request_size: HEADER_REQUEST_SIZE,
            block_request_size: BLOCK_REQUEST_SIZE,
            peer_ban_duration: DEFAULT_PEER_BAN_DURATION,
            short_term_peer_ban_duration: SHORT_TERM_PEER_BAN_DURATION,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BlockSyncStrategy {
    ViaBestChainMetadata(BestChainMetadataBlockSyncInfo),
    ViaRandomPeer(ForwardBlockSyncInfo),
}

impl FromStr for BlockSyncStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ViaBestChainMetadata" => Ok(Self::ViaBestChainMetadata(BestChainMetadataBlockSyncInfo)),
            "ViaRandomPeer" => Ok(Self::ViaRandomPeer(ForwardBlockSyncInfo)),
            _ => Err("Unrecognized value for BlockSyncStrategy. Available values \
                      are:ViaBestChainMetadata,ViaRandomPeer"
                .to_string()),
        }
    }
}

impl BlockSyncStrategy {
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        network_tip: &ChainMetadata,
        sync_peers: &mut Vec<NodeId>,
    ) -> StateEvent
    {
        match self {
            BlockSyncStrategy::ViaBestChainMetadata(sync) => sync.next_event(shared, network_tip, sync_peers).await,
            BlockSyncStrategy::ViaRandomPeer(sync) => sync.next_event(shared).await,
        }
    }
}

/// State management for BlockSync -> Listening.
impl From<BlockSyncStrategy> for ListeningInfo {
    fn from(_old_state: BlockSyncStrategy) -> Self {
        ListeningInfo {}
    }
}

impl PartialEq for BlockSyncStrategy {
    fn eq(&self, other: &Self) -> bool {
        match self {
            BlockSyncStrategy::ViaBestChainMetadata(_) => match other {
                BlockSyncStrategy::ViaBestChainMetadata(_) => true,
                _ => false,
            },
            BlockSyncStrategy::ViaRandomPeer(_) => match other {
                BlockSyncStrategy::ViaRandomPeer(_) => true,
                _ => false,
            },
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum BlockSyncError {
    MaxRequestAttemptsReached,
    MaxAddBlockAttemptsReached,
    ForkChainNotLinked,
    InvalidChainLink,
    EmptyBlockchain,
    EmptyNetworkBestBlock,
    NoSyncPeers,
    ChainStorageError(ChainStorageError),
    PeerManagerError(PeerManagerError),
    ConnectionManagerError(ConnectionManagerError),
    CommsInterfaceError(CommsInterfaceError),
}

#[derive(Clone, Debug, PartialEq, Copy)]
pub struct BestChainMetadataBlockSyncInfo;

impl BestChainMetadataBlockSyncInfo {
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        network_tip: &ChainMetadata,
        sync_peers: &mut Vec<NodeId>,
    ) -> StateEvent
    {
        info!(target: LOG_TARGET, "Synchronizing missing blocks.");
        match synchronize_blocks(shared, network_tip, sync_peers).await {
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
            Err(BlockSyncError::NoSyncPeers) => {
                warn!(target: LOG_TARGET, "No remaining sync peers.",);
                StateEvent::BlockSyncFailure
            },
            Err(BlockSyncError::EmptyNetworkBestBlock) => {
                warn!(target: LOG_TARGET, "An empty network best block hash was received.",);
                StateEvent::BlockSyncFailure
            },
            Err(BlockSyncError::CommsInterfaceError(e)) => {
                warn!(target: LOG_TARGET, "Unable to perform network queries: {}", e);
                StateEvent::BlockSyncFailure
            },
            Err(e) => StateEvent::FatalError(format!("Synchronizing blocks failed. {:?}", e)),
        }
    }
}

async fn synchronize_blocks<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    network_metadata: &ChainMetadata,
    sync_peers: &mut Vec<NodeId>,
) -> Result<(), BlockSyncError>
{
    let local_metadata = shared.db.get_metadata()?;
    if let Some(local_block_hash) = local_metadata.best_block.clone() {
        if let Some(network_block_hash) = network_metadata.best_block.clone() {
            info!(
                target: LOG_TARGET,
                "Checking if current chain lagging on best network chain."
            );
            let local_tip_height = local_metadata.height_of_longest_chain.unwrap_or(0);
            let mut network_tip_height = network_metadata.height_of_longest_chain.unwrap_or(0);
            let mut sync_height = local_tip_height + 1;
            if check_chain_split(
                shared,
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
                sync_height = find_chain_split_height(shared, sync_peers, min_tip_height).await?;
                info!(target: LOG_TARGET, "Chain split found at height {}.", sync_height);
            } else {
                trace!(
                    target: LOG_TARGET,
                    "Block hash {} is common between our chain and the network.",
                    local_block_hash.to_hex()
                );
            }

            info!(target: LOG_TARGET, "Synchronize missing blocks.");
            while sync_height <= network_tip_height {
                let max_height = min(
                    sync_height + (shared.config.block_sync_config.block_request_size - 1) as u64,
                    network_tip_height,
                );
                let block_nums: Vec<u64> = (sync_height..=max_height).collect();
                request_and_add_blocks(shared, sync_peers, block_nums.clone()).await?;
                sync_height += block_nums.len() as u64;
                if sync_height == network_tip_height {
                    info!(target: LOG_TARGET, "Check if sync peer chain has been extended.");
                    network_tip_height = request_network_tip_height(shared, sync_peers).await?;
                }
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
async fn check_chain_split<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    local_tip_height: u64,
    network_tip_height: u64,
    local_block_hash: &BlockHash,
    network_block_hash: &BlockHash,
) -> Result<bool, BlockSyncError>
{
    Ok(if network_tip_height > local_tip_height {
        let (header, _) = request_header(shared, sync_peers, local_tip_height).await?;
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
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    tip_height: u64,
) -> Result<u64, BlockSyncError>
{
    for block_nums in (1..=tip_height)
        .rev()
        .collect::<Vec<u64>>()
        .chunks(shared.config.block_sync_config.header_request_size)
    {
        let (headers, sync_peer) = request_headers(shared, sync_peers, block_nums).await?;
        for header in headers {
            // Check if header is linked to local chain
            if let Ok(prev_header) =
                async_db::fetch_header_with_block_hash(shared.db.clone(), header.prev_hash.clone()).await
            {
                if prev_header.height + 1 == header.height {
                    return Ok(header.height);
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied invalid chain link", sync_peer
                    );
                    ban_sync_peer(
                        shared,
                        sync_peers,
                        sync_peer.clone(),
                        shared.config.block_sync_config.peer_ban_duration,
                    )
                    .await?;
                    return Err(BlockSyncError::InvalidChainLink);
                }
            }
        }
    }
    warn!(
        target: LOG_TARGET,
        "Banning all peers from local node, because they could not provide a valid chain link",
    );
    ban_all_sync_peers(shared, sync_peers, shared.config.block_sync_config.peer_ban_duration).await?;
    Err(BlockSyncError::ForkChainNotLinked)
}

// Request a block from a remote sync peer and attempt to add it to the local blockchain.
async fn request_and_add_blocks<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    mut block_nums: Vec<u64>,
) -> Result<(), BlockSyncError>
{
    let config = shared.config.block_sync_config;
    for attempt in 0..config.max_add_block_retry_attempts {
        let (blocks, sync_peer) = request_blocks(shared, sync_peers, block_nums.clone()).await?;
        for block in blocks {
            let block_hash = block.hash();
            match shared
                .local_node_interface
                .submit_block(block.clone(), Broadcast::from(false))
                .await
            {
                Ok(_) => {
                    info!(
                        target: LOG_TARGET,
                        "Block #{} ({}) successfully added to database",
                        block.header.height,
                        block_hash.to_hex()
                    );
                    trace!(target: LOG_TARGET, "Block added to database: {}", block,);
                    block_nums.remove(0);
                },
                Err(CommsInterfaceError::ChainStorageError(ChainStorageError::InvalidBlock)) => {
                    warn!(
                        target: LOG_TARGET,
                        "Invalid block {} received from peer. Retrying",
                        block_hash.to_hex(),
                    );
                    warn!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied invalid block", sync_peer
                    );
                    ban_sync_peer(
                        shared,
                        sync_peers,
                        sync_peer.clone(),
                        shared.config.block_sync_config.peer_ban_duration,
                    )
                    .await?;
                    break;
                },
                Err(CommsInterfaceError::ChainStorageError(ChainStorageError::ValidationError { source })) => {
                    warn!(
                        target: LOG_TARGET,
                        "Validation on block {} from peer failed due to: {:?}. Retrying",
                        block_hash.to_hex(),
                        source,
                    );
                    warn!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied invalid block", sync_peer
                    );
                    ban_sync_peer(
                        shared,
                        sync_peers,
                        sync_peer.clone(),
                        shared.config.block_sync_config.peer_ban_duration,
                    )
                    .await?;
                    break;
                },
                Err(e) => return Err(BlockSyncError::CommsInterfaceError(e)),
            }
        }
        if block_nums.is_empty() {
            return Ok(());
        }
        info!(target: LOG_TARGET, "Retrying block add. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxAddBlockAttemptsReached)
}

// Request a block from a remote sync peer.
async fn request_blocks<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    block_nums: Vec<u64>,
) -> Result<(Vec<Block>, NodeId), BlockSyncError>
{
    let config = shared.config.block_sync_config;
    for attempt in 1..=config.max_block_request_retry_attempts {
        let sync_peer = select_sync_peer(&config, sync_peers)?;
        trace!(
            target: LOG_TARGET,
            "Requesting blocks {:?} from {}.",
            block_nums,
            sync_peer
        );
        match shared
            .comms
            .request_blocks_from_peer(block_nums.clone(), Some(sync_peer.clone()))
            .await
        {
            Ok(hist_blocks) => {
                debug!(target: LOG_TARGET, "Received {} blocks from peer", hist_blocks.len());
                if block_nums.len() == hist_blocks.len() {
                    if (0..block_nums.len()).all(|i| hist_blocks[i].block().header.height == block_nums[i]) {
                        let blocks: Vec<Block> = hist_blocks
                            .into_iter()
                            .map(|hist_block| hist_block.block().clone())
                            .collect();
                        return Ok((blocks, sync_peer));
                    } else {
                        debug!(target: LOG_TARGET, "This was NOT the blocks we were expecting.");
                        warn!(
                            target: LOG_TARGET,
                            "Banning peer {} from local node, because they supplied the incorrect blocks", sync_peer
                        );
                        ban_sync_peer(
                            shared,
                            sync_peers,
                            sync_peer.clone(),
                            shared.config.block_sync_config.peer_ban_duration,
                        )
                        .await?;
                    }
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "Incorrect number of blocks returned. Expected {}. Got {}",
                        block_nums.len(),
                        hist_blocks.len()
                    );
                    warn!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied the incorrect number of blocks",
                        sync_peer
                    );
                    ban_sync_peer(
                        shared,
                        sync_peers,
                        sync_peer.clone(),
                        shared.config.block_sync_config.peer_ban_duration,
                    )
                    .await?;
                }
            },
            Err(CommsInterfaceError::UnexpectedApiResponse) => {
                debug!(target: LOG_TARGET, "Remote node provided an unexpected api response.",);
                ban_sync_peer(
                    shared,
                    sync_peers,
                    sync_peer.clone(),
                    shared.config.block_sync_config.peer_ban_duration,
                )
                .await?;
            },
            Err(CommsInterfaceError::RequestTimedOut) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to fetch blocks from peer: {:?}. Retrying.",
                    CommsInterfaceError::RequestTimedOut,
                );
                ban_sync_peer(
                    shared,
                    sync_peers,
                    sync_peer.clone(),
                    shared.config.block_sync_config.short_term_peer_ban_duration,
                )
                .await?;
            },
            Err(e) => return Err(BlockSyncError::CommsInterfaceError(e)),
        }
        debug!(target: LOG_TARGET, "Retrying block download. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

// Request a header from a remote sync peer.
async fn request_header<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    height: u64,
) -> Result<(BlockHeader, NodeId), BlockSyncError>
{
    let (headers, sync_peer) = request_headers(shared, sync_peers, &[height]).await?;
    if let Some(header) = headers.first() {
        return Ok((header.clone(), sync_peer));
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

// Request a set of headers from a remote sync peer.
async fn request_headers<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    block_nums: &[u64],
) -> Result<(Vec<BlockHeader>, NodeId), BlockSyncError>
{
    let config = shared.config.block_sync_config;
    for attempt in 1..=config.max_header_request_retry_attempts {
        let sync_peer = select_sync_peer(&config, sync_peers)?;
        trace!(target: LOG_TARGET, "Requesting headers from {}.", sync_peer);
        match shared
            .comms
            .request_headers_from_peer(block_nums.to_vec(), Some(sync_peer.clone()))
            .await
        {
            Ok(headers) => {
                debug!(target: LOG_TARGET, "Received {} headers from peer", headers.len());
                if block_nums.len() == headers.len() {
                    if (0..block_nums.len()).all(|i| headers[i].height == block_nums[i]) {
                        return Ok((headers, sync_peer));
                    } else {
                        debug!(target: LOG_TARGET, "This was NOT the headers we were expecting.");
                        warn!(
                            target: LOG_TARGET,
                            "Banning peer {} from local node, because they supplied the incorrect headers", sync_peer
                        );
                        ban_sync_peer(
                            shared,
                            sync_peers,
                            sync_peer.clone(),
                            shared.config.block_sync_config.peer_ban_duration,
                        )
                        .await?;
                    }
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "Incorrect number of headers returned. Expected {}. Got {}",
                        block_nums.len(),
                        headers.len()
                    );
                    warn!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied the incorrect number of headers",
                        sync_peer
                    );
                    ban_sync_peer(
                        shared,
                        sync_peers,
                        sync_peer.clone(),
                        shared.config.block_sync_config.peer_ban_duration,
                    )
                    .await?;
                }
            },
            Err(CommsInterfaceError::UnexpectedApiResponse) => {
                debug!(target: LOG_TARGET, "Remote node provided an unexpected api response.",);
                warn!(
                    target: LOG_TARGET,
                    "Banning peer {} from local node, because they provided an unexpected api response", sync_peer
                );
                ban_sync_peer(
                    shared,
                    sync_peers,
                    sync_peer.clone(),
                    shared.config.block_sync_config.peer_ban_duration,
                )
                .await?;
            },
            Err(CommsInterfaceError::RequestTimedOut) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to fetch header from peer: {:?}. Retrying.",
                    CommsInterfaceError::RequestTimedOut,
                );
                ban_sync_peer(
                    shared,
                    sync_peers,
                    sync_peer.clone(),
                    shared.config.block_sync_config.short_term_peer_ban_duration,
                )
                .await?;
            },
            Err(e) => return Err(BlockSyncError::CommsInterfaceError(e)),
        }
        debug!(target: LOG_TARGET, "Retrying header download. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

// Request the updated tip height from a remote sync peer.
async fn request_network_tip_height<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
) -> Result<u64, BlockSyncError>
{
    let config = shared.config.block_sync_config;
    for attempt in 1..=config.max_metadata_request_retry_attempts {
        let sync_peer = select_sync_peer(&config, sync_peers)?;
        trace!(target: LOG_TARGET, "Requesting updated metadata from {}.", sync_peer);
        match shared.comms.request_metadata_from_peer(Some(sync_peer.clone())).await {
            Ok(metadata) => {
                debug!(target: LOG_TARGET, "Received updated metadata from peer");
                match metadata.height_of_longest_chain {
                    Some(network_tip_height) => return Ok(network_tip_height),
                    None => {
                        warn!(
                            target: LOG_TARGET,
                            "Updated metadata had an undefined chain height: {:?}.",
                            CommsInterfaceError::RequestTimedOut,
                        );
                        ban_sync_peer(
                            shared,
                            sync_peers,
                            sync_peer.clone(),
                            shared.config.block_sync_config.peer_ban_duration,
                        )
                        .await?;
                    },
                }
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to fetch updated metadata from peer: {:?}. ", e,
                );
                ban_sync_peer(
                    shared,
                    sync_peers,
                    sync_peer.clone(),
                    shared.config.block_sync_config.short_term_peer_ban_duration,
                )
                .await?;
            },
        }
        debug!(
            target: LOG_TARGET,
            "Retrying chain metadata download. Attempt {}", attempt
        );
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

// Selects the first sync peer or a random peer from the set of sync peers that have the current network tip depending
// on the selected configuration.
fn select_sync_peer(config: &BlockSyncConfig, sync_peers: &[NodeId]) -> Result<NodeId, BlockSyncError> {
    if config.random_sync_peer_with_chain {
        sync_peers.choose(&mut rand::thread_rng())
    } else {
        sync_peers.first()
    }
    .map(Clone::clone)
    .ok_or(BlockSyncError::NoSyncPeers)
}

// Excluded the provided peer from the sync peers.
async fn exclude_sync_peer(sync_peers: &mut Vec<NodeId>, sync_peer: NodeId) -> Result<(), BlockSyncError> {
    trace!(target: LOG_TARGET, "Excluding peer ({}) from sync peers.", sync_peer,);
    sync_peers.retain(|p| *p != sync_peer);
    if sync_peers.is_empty() {
        return Err(BlockSyncError::NoSyncPeers);
    }
    Ok(())
}

// Ban and disconnect the provided sync peer.
async fn ban_sync_peer<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    sync_peer: NodeId,
    ban_duration: Duration,
) -> Result<(), BlockSyncError>
{
    warn!(target: LOG_TARGET, "Banning peer {} from local node.", sync_peer);
    sync_peers.retain(|p| *p != sync_peer);
    let peer = shared.peer_manager.find_by_node_id(&sync_peer).await?;
    shared.peer_manager.ban_for(&peer.public_key, ban_duration).await?;
    shared.connection_manager.disconnect_peer(sync_peer.clone()).await??;
    exclude_sync_peer(sync_peers, sync_peer).await
}

// Ban and disconnect entire set of sync peers.
async fn ban_all_sync_peers<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    ban_duration: Duration,
) -> Result<(), BlockSyncError>
{
    while !sync_peers.is_empty() {
        ban_sync_peer(shared, sync_peers, sync_peers[0].clone(), ban_duration).await?;
    }
    Ok(())
}
