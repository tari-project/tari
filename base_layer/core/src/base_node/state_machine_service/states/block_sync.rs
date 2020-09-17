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
        state_machine_service::{
            states::{
                helpers::{ban_all_sync_peers, ban_sync_peer, request_headers, select_sync_peer},
                sync_peers::SyncPeer,
                ForwardBlockSyncInfo,
                Listening,
                StateEvent,
                StateInfo,
                SyncPeers,
            },
            BaseNodeStateMachine,
        },
    },
    blocks::{blockheader::BlockHeader, Block},
    chain_storage::{async_db, BlockchainBackend, ChainMetadata, ChainStorageError},
};
use core::cmp::min;
use log::*;
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};
use tari_comms::{connectivity::ConnectivityError, peer_manager::PeerManagerError};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use thiserror::Error;

const LOG_TARGET: &str = "c::bn::state_machine_service::states::block_sync";

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

/// Configuration for the Block Synchronization.
#[derive(Clone, Copy)]
pub struct BlockSyncConfig {
    pub sync_strategy: BlockSyncStrategy,
    pub max_metadata_request_retry_attempts: usize,
    pub max_header_request_retry_attempts: usize,
    pub max_block_request_retry_attempts: usize,
    pub max_add_block_retry_attempts: usize,
    pub header_request_size: usize,
    pub block_request_size: usize,
}

#[derive(Clone, Debug, PartialEq, Default)]
/// This struct contains info that is use full for external viewing of state info
pub struct BlockSyncInfo {
    pub tip_height: u64,
    pub local_height: u64,
    pub sync_peers: SyncPeers,
}

impl BlockSyncInfo {
    /// Creates a new blockSyncInfo
    pub fn new(tip_height: u64, local_height: u64, sync_peers: SyncPeers) -> BlockSyncInfo {
        BlockSyncInfo {
            tip_height,
            local_height,
            sync_peers,
        }
    }
}

impl Display for BlockSyncInfo {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str("Syncing from the following peers: \n")?;
        for peer in &self.sync_peers {
            fmt.write_str(&format!("{}\n", peer.node_id))?;
        }
        fmt.write_str(&format!("Syncing {}/{}\n", self.local_height, self.tip_height))
    }
}

impl Default for BlockSyncConfig {
    fn default() -> Self {
        Self {
            sync_strategy: BlockSyncStrategy::ViaBestChainMetadata(BestChainMetadataBlockSyncInfo),
            max_metadata_request_retry_attempts: MAX_METADATA_REQUEST_RETRY_ATTEMPTS,
            max_header_request_retry_attempts: MAX_HEADER_REQUEST_RETRY_ATTEMPTS,
            max_block_request_retry_attempts: MAX_BLOCK_REQUEST_RETRY_ATTEMPTS,
            max_add_block_retry_attempts: MAX_ADD_BLOCK_RETRY_ATTEMPTS,
            header_request_size: HEADER_REQUEST_SIZE,
            block_request_size: BLOCK_REQUEST_SIZE,
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
        sync_peers: &mut SyncPeers,
    ) -> StateEvent
    {
        shared.info = StateInfo::BlockSync(BlockSyncInfo::default());
        if let StateInfo::BlockSync(ref mut info) = shared.info {
            info.sync_peers = Clone::clone(&*sync_peers);
        }
        shared.publish_event_info();
        match self {
            BlockSyncStrategy::ViaBestChainMetadata(sync) => sync.next_event(shared, network_tip, sync_peers).await,
            BlockSyncStrategy::ViaRandomPeer(sync) => sync.next_event(shared).await,
        }
    }
}

/// State management for BlockSync -> Listening.
impl From<BlockSyncStrategy> for Listening {
    fn from(_old_state: BlockSyncStrategy) -> Self {
        Listening { is_synced: true }
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

#[derive(Debug, Error)]
pub enum BlockSyncError {
    #[error("Maximum request attempts reached error")]
    MaxRequestAttemptsReached,
    #[error("Maximum add block attempts reached error")]
    MaxAddBlockAttemptsReached,
    #[error("Fork chain not linked error")]
    ForkChainNotLinked,
    #[error("Invalid chain link error")]
    InvalidChainLink,
    #[error("Empty blockchain error")]
    EmptyBlockchain,
    #[error("Empty network best block error")]
    EmptyNetworkBestBlock,
    #[error("No sync peers error")]
    NoSyncPeers,
    #[error("Chain storage error: `{0}`")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Peer manager error: `{0}`")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Connectivity error: `{0}`")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("Comms interface error: `{0}`")]
    CommsInterfaceError(#[from] CommsInterfaceError),
}

#[derive(Clone, Debug, PartialEq, Copy)]
pub struct BestChainMetadataBlockSyncInfo;

impl BestChainMetadataBlockSyncInfo {
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        network_tip: &ChainMetadata,
        sync_peers: &mut SyncPeers,
    ) -> StateEvent
    where
        B: 'static,
    {
        if let StateInfo::BlockSync(ref mut info) = shared.info {
            info.sync_peers.clear();
            info.sync_peers.append(&mut sync_peers.clone());
        }
        shared.publish_event_info();

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
    sync_peers: &mut SyncPeers,
) -> Result<(), BlockSyncError>
{
    let local_metadata = shared.db.get_chain_metadata()?;
    // Filter the peers we can sync from: any peer which has an effective pruning horizon less than this nodes
    // current height
    sync_peers.retain(|p| p.chain_metadata.effective_pruned_height <= local_metadata.height_of_longest_chain());
    if sync_peers.is_empty() {
        return Err(BlockSyncError::NoSyncPeers);
    }

    if let Some(local_block_hash) = local_metadata.best_block.as_ref() {
        if let Some(network_block_hash) = network_metadata.best_block.as_ref() {
            debug!(
                target: LOG_TARGET,
                "Checking if current chain lagging on best network chain."
            );
            let local_tip_height = local_metadata.height_of_longest_chain();
            let network_tip_height = network_metadata.height_of_longest_chain();
            let mut sync_height = local_tip_height + 1;
            if check_chain_split(
                shared,
                sync_peers,
                local_tip_height,
                network_tip_height,
                local_block_hash,
                network_block_hash,
            )
            .await?
            {
                debug!(target: LOG_TARGET, "Chain split detected, finding chain split height.");
                let min_tip_height = min(local_tip_height, network_tip_height);
                sync_height = find_chain_split_height(shared, sync_peers, min_tip_height).await?;
                info!(target: LOG_TARGET, "Chain split found at height {}.", sync_height);
            } else {
                debug!(
                    target: LOG_TARGET,
                    "Block hash {} is common between our chain and the network.",
                    local_block_hash.to_hex()
                );
            }

            if let StateInfo::BlockSync(ref mut info) = shared.info {
                info.tip_height = network_tip_height;
            }
            while sync_height <= network_tip_height {
                if let StateInfo::BlockSync(ref mut info) = shared.info {
                    info.local_height = sync_height;
                }

                shared.publish_event_info();
                let max_height = min(
                    sync_height + (shared.config.block_sync_config.block_request_size - 1) as u64,
                    network_tip_height,
                );
                let block_nums: Vec<u64> = (sync_height..=max_height).collect();
                let block_nums_count = block_nums.len() as u64;
                request_and_add_blocks(shared, sync_peers, block_nums).await?;
                sync_height += block_nums_count;
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
    sync_peers: &mut SyncPeers,
    local_tip_height: u64,
    network_tip_height: u64,
    local_block_hash: &[u8],
    network_block_hash: &[u8],
) -> Result<bool, BlockSyncError>
{
    match network_tip_height {
        tip if tip > local_tip_height => {
            let (header, _) = request_header(shared, sync_peers, local_tip_height).await?;
            Ok(header.hash() != local_block_hash)
        },
        tip if tip == local_tip_height => Ok(local_block_hash != network_block_hash),
        _ => Ok(true),
    }
}

// Find the block height where the chain split occurs. The chain split height is the height of the first block that is
// not common between the local and network chains.
async fn find_chain_split_height<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut SyncPeers,
    tip_height: u64,
) -> Result<u64, BlockSyncError>
{
    for block_nums in (1..=tip_height)
        .rev()
        .collect::<Vec<u64>>()
        .chunks(shared.config.block_sync_config.header_request_size)
    {
        let (headers, sync_peer) = request_headers(
            LOG_TARGET,
            shared,
            sync_peers,
            block_nums,
            shared.config.block_sync_config.max_header_request_retry_attempts,
        )
        .await?;
        for header in headers {
            // Check if header is linked to local chain
            if let Ok(prev_header) =
                async_db::fetch_header_by_block_hash(shared.db.clone(), header.prev_hash.clone()).await
            {
                return if prev_header.height + 1 == header.height {
                    Ok(header.height)
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied invalid chain link", sync_peer
                    );
                    ban_sync_peer(
                        LOG_TARGET,
                        &mut shared.connectivity,
                        sync_peers,
                        sync_peer.clone(),
                        shared.config.sync_peer_config.peer_ban_duration,
                    )
                    .await?;
                    Err(BlockSyncError::InvalidChainLink)
                };
            }
        }
    }
    warn!(
        target: LOG_TARGET,
        "Banning all peers from local node, because they could not provide a valid chain link",
    );
    ban_all_sync_peers(
        LOG_TARGET,
        shared,
        sync_peers,
        shared.config.sync_peer_config.peer_ban_duration,
    )
    .await?;
    Err(BlockSyncError::ForkChainNotLinked)
}

// Request a block from a remote sync peer and attempt to add it to the local blockchain.
async fn request_and_add_blocks<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut SyncPeers,
    mut block_nums: Vec<u64>,
) -> Result<(), BlockSyncError>
{
    if block_nums.is_empty() {
        return Ok(());
    }
    let config = shared.config.block_sync_config;
    for attempt in 0..config.max_add_block_retry_attempts {
        let (blocks, sync_peer) = request_blocks(shared, sync_peers, block_nums.clone()).await?;
        if let StateInfo::BlockSync(ref mut info) = shared.info {
            // assuming the numbers are ordered
            info.tip_height = block_nums[block_nums.len() - 1];
        }
        shared.publish_event_info();
        for block in blocks {
            let block_hash = block.hash();
            if let StateInfo::BlockSync(ref mut info) = shared.info {
                info.local_height = block.header.height;
            }

            shared.publish_event_info();
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
                    block_nums.remove(0);
                },
                Err(CommsInterfaceError::ChainStorageError(ChainStorageError::InvalidBlock)) => {
                    warn!(
                        target: LOG_TARGET,
                        "Invalid block {} received from peer.",
                        block_hash.to_hex(),
                    );
                    debug!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied invalid block", sync_peer
                    );
                    ban_sync_peer(
                        LOG_TARGET,
                        &mut shared.connectivity,
                        sync_peers,
                        sync_peer.clone(),
                        shared.config.sync_peer_config.peer_ban_duration,
                    )
                    .await?;
                    break;
                },
                Err(CommsInterfaceError::ChainStorageError(ChainStorageError::ValidationError { source })) => {
                    warn!(
                        target: LOG_TARGET,
                        "Validation on block {} from peer failed due to: {:?}.",
                        block_hash.to_hex(),
                        source,
                    );
                    debug!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied invalid block", sync_peer
                    );
                    ban_sync_peer(
                        LOG_TARGET,
                        &mut shared.connectivity,
                        sync_peers,
                        sync_peer.clone(),
                        shared.config.sync_peer_config.peer_ban_duration,
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
        debug!(target: LOG_TARGET, "Retrying block add. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxAddBlockAttemptsReached)
}

// Request a block from a remote sync peer.
async fn request_blocks<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut SyncPeers,
    block_nums: Vec<u64>,
) -> Result<(Vec<Block>, SyncPeer), BlockSyncError>
{
    let config = shared.config.sync_peer_config;
    for attempt in 1..=shared.config.block_sync_config.max_block_request_retry_attempts {
        let sync_peer = select_sync_peer(&config, sync_peers)?;
        if block_nums.is_empty() {
            return Ok((Vec::new(), sync_peer));
        }
        debug!(
            target: LOG_TARGET,
            "Requesting blocks {:?} from {}.", block_nums, sync_peer
        );
        if let StateInfo::BlockSync(ref mut info) = shared.info {
            info.local_height = block_nums[0];
            info.tip_height = block_nums[block_nums.len() - 1];
        }
        shared.publish_event_info();
        match shared
            .comms
            .request_blocks_from_peer(block_nums.clone(), Some(sync_peer.node_id.clone()))
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
                        debug!(
                            target: LOG_TARGET,
                            "Banning peer {} from local node, because they supplied the incorrect blocks", sync_peer
                        );
                        ban_sync_peer(
                            LOG_TARGET,
                            &mut shared.connectivity,
                            sync_peers,
                            sync_peer.clone(),
                            config.peer_ban_duration,
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
                    debug!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied the incorrect number of blocks",
                        sync_peer
                    );
                    ban_sync_peer(
                        LOG_TARGET,
                        &mut shared.connectivity,
                        sync_peers,
                        sync_peer.clone(),
                        config.peer_ban_duration,
                    )
                    .await?;
                }
            },
            Err(CommsInterfaceError::UnexpectedApiResponse) => {
                debug!(target: LOG_TARGET, "Remote node provided an unexpected api response.",);
                ban_sync_peer(
                    LOG_TARGET,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.peer_ban_duration,
                )
                .await?;
            },
            Err(CommsInterfaceError::RequestTimedOut) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to fetch blocks from peer: {:?}. Retrying.",
                    CommsInterfaceError::RequestTimedOut,
                );
                ban_sync_peer(
                    LOG_TARGET,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.short_term_peer_ban_duration,
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
    sync_peers: &mut SyncPeers,
    height: u64,
) -> Result<(BlockHeader, SyncPeer), BlockSyncError>
{
    let (headers, sync_peer) = request_headers(
        LOG_TARGET,
        shared,
        sync_peers,
        &[height],
        shared.config.block_sync_config.max_header_request_retry_attempts,
    )
    .await?;
    if let Some(header) = headers.first() {
        return Ok((header.clone(), sync_peer));
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}
