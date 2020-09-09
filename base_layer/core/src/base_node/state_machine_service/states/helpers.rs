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
        comms_interface::CommsInterfaceError,
        state_machine_service::{
            states::{block_sync::BlockSyncError, sync_peers::SyncPeer, SyncPeers},
            BaseNodeStateMachine,
        },
    },
    blocks::blockheader::BlockHeader,
    chain_storage::{BlockchainBackend, MmrTree},
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use croaring::Bitmap;
use log::*;
use rand::seq::SliceRandom;
use std::time::Duration;
use tari_comms::connectivity::ConnectivityRequester;

// If more than one sync peer discovered with the correct chain, enable or disable the selection of a random sync peer
// to query headers and blocks.
const RANDOM_SYNC_PEER_WITH_CHAIN: bool = true;
// The default length of time to ban a misbehaving/malfunctioning sync peer (24 hours)
const DEFAULT_PEER_BAN_DURATION: Duration = Duration::from_secs(24 * 60 * 60);
// The length of time for a short term ban of a misbehaving/malfunctioning sync peer (5 min)
const SHORT_TERM_PEER_BAN_DURATION: Duration = Duration::from_secs(5 * 60);

/// Configuration for the Sync Peer Selection and Banning.
#[derive(Clone, Copy)]
pub struct SyncPeerConfig {
    pub random_sync_peer_with_chain: bool,
    pub peer_ban_duration: Duration,
    pub short_term_peer_ban_duration: Duration,
}

impl Default for SyncPeerConfig {
    fn default() -> Self {
        Self {
            random_sync_peer_with_chain: RANDOM_SYNC_PEER_WITH_CHAIN,
            peer_ban_duration: DEFAULT_PEER_BAN_DURATION,
            short_term_peer_ban_duration: SHORT_TERM_PEER_BAN_DURATION,
        }
    }
}

/// Selects the first sync peer or a random peer from the set of sync peers that have the current network tip depending
/// on the selected configuration.
pub fn select_sync_peer(config: &SyncPeerConfig, sync_peers: &[SyncPeer]) -> Result<SyncPeer, BlockSyncError> {
    if config.random_sync_peer_with_chain {
        sync_peers.choose(&mut rand::thread_rng())
    } else {
        sync_peers.first()
    }
    .map(Clone::clone)
    .ok_or(BlockSyncError::NoSyncPeers)
}

/// Excluded the provided peer from the sync peers.
pub fn exclude_sync_peer(
    log_target: &str,
    sync_peers: &mut SyncPeers,
    sync_peer: SyncPeer,
) -> Result<(), BlockSyncError>
{
    trace!(target: log_target, "Excluding peer ({}) from sync peers.", sync_peer);
    sync_peers.retain(|p| p.node_id != sync_peer.node_id);
    if sync_peers.is_empty() {
        return Err(BlockSyncError::NoSyncPeers);
    }
    Ok(())
}

/// Ban and disconnect the provided sync peer if this node is online
pub async fn ban_sync_peer_if_online(
    log_target: &str,
    connectivity: &mut ConnectivityRequester,
    sync_peers: &mut SyncPeers,
    sync_peer: SyncPeer,
    ban_duration: Duration,
) -> Result<(), BlockSyncError>
{
    if !connectivity.get_connectivity_status().await?.is_online() {
        warn!(
            target: log_target,
            "Unable to ban peer {} because local node is offline.", sync_peer
        );
        return Ok(());
    }
    ban_sync_peer(log_target, connectivity, sync_peers, sync_peer, ban_duration).await
}

/// Ban and disconnect the provided sync peer.
pub async fn ban_sync_peer(
    log_target: &str,
    connectivity: &mut ConnectivityRequester,
    sync_peers: &mut SyncPeers,
    sync_peer: SyncPeer,
    ban_duration: Duration,
) -> Result<(), BlockSyncError>
{
    info!(target: log_target, "Banning peer {} from local node.", sync_peer);
    connectivity.ban_peer(sync_peer.node_id.clone(), ban_duration).await?;
    exclude_sync_peer(log_target, sync_peers, sync_peer)
}

/// Ban and disconnect entire set of sync peers.
pub async fn ban_all_sync_peers<B: BlockchainBackend + 'static>(
    log_target: &str,
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut SyncPeers,
    ban_duration: Duration,
) -> Result<(), BlockSyncError>
{
    while !sync_peers.is_empty() {
        ban_sync_peer(
            log_target,
            &mut shared.connectivity,
            sync_peers,
            sync_peers[0].clone(),
            ban_duration,
        )
        .await?;
    }
    Ok(())
}

/// Request a set of headers from a remote sync peer.
pub async fn request_headers<B: BlockchainBackend + 'static>(
    log_target: &str,
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut SyncPeers,
    block_nums: &[u64],
    request_retry_attempts: usize,
) -> Result<(Vec<BlockHeader>, SyncPeer), BlockSyncError>
{
    let config = shared.config.sync_peer_config;
    for attempt in 1..=request_retry_attempts {
        let sync_peer = select_sync_peer(&config, sync_peers)?;
        debug!(
            target: log_target,
            "Requesting {} headers from {}.",
            block_nums.len(),
            sync_peer.node_id
        );
        match shared
            .comms
            .request_headers_from_peer(block_nums.to_vec(), Some(sync_peer.node_id.clone()))
            .await
        {
            Ok(headers) => {
                debug!(target: log_target, "Received {} headers from peer", headers.len());
                if block_nums.len() == headers.len() {
                    if (0..block_nums.len()).all(|i| headers[i].height == block_nums[i]) {
                        return Ok((headers, sync_peer));
                    } else {
                        debug!(target: log_target, "This was NOT the headers we were expecting.");
                        debug!(
                            target: log_target,
                            "Banning peer {} from local node, because they supplied the incorrect headers", sync_peer
                        );
                        ban_sync_peer(
                            log_target,
                            &mut shared.connectivity,
                            sync_peers,
                            sync_peer.clone(),
                            config.peer_ban_duration,
                        )
                        .await?;
                    }
                } else {
                    debug!(
                        target: log_target,
                        "Incorrect number of headers returned. Expected {}. Got {}",
                        block_nums.len(),
                        headers.len()
                    );
                    debug!(
                        target: log_target,
                        "Banning peer {} from local node, because they supplied the incorrect number of headers",
                        sync_peer
                    );
                    ban_sync_peer(
                        log_target,
                        &mut shared.connectivity,
                        sync_peers,
                        sync_peer.clone(),
                        config.peer_ban_duration,
                    )
                    .await?;
                }
            },
            Err(CommsInterfaceError::UnexpectedApiResponse) => {
                debug!(target: log_target, "Remote node provided an unexpected api response.",);
                debug!(
                    target: log_target,
                    "Banning peer {} from local node, because they provided an unexpected api response", sync_peer
                );
                ban_sync_peer(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.peer_ban_duration,
                )
                .await?;
            },
            Err(CommsInterfaceError::RequestTimedOut) => {
                debug!(
                    target: log_target,
                    "Failed to fetch header from peer: {:?}. Retrying.",
                    CommsInterfaceError::RequestTimedOut,
                );
                ban_sync_peer_if_online(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.short_term_peer_ban_duration,
                )
                .await?;
            },
            Err(e) => return Err(BlockSyncError::CommsInterfaceError(e)),
        }
        debug!(target: log_target, "Retrying header download. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

/// Request the total merkle mountain range node count upto the specified height for the selected MMR from remote base
/// nodes.
pub async fn request_mmr_node_count<B: BlockchainBackend + 'static>(
    log_target: &str,
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut SyncPeers,
    tree: MmrTree,
    height: u64,
    request_retry_attempts: usize,
) -> Result<(u32, SyncPeer), BlockSyncError>
{
    let config = shared.config.sync_peer_config;
    for attempt in 1..=request_retry_attempts {
        let sync_peer = select_sync_peer(&config, sync_peers)?;
        debug!(
            target: log_target,
            "Requesting mmr node count to height {} from {}.", height, sync_peer.node_id
        );
        match shared
            .comms
            .fetch_mmr_node_count(tree, height, Some(sync_peer.node_id.clone()))
            .await
        {
            Ok(num_nodes) => {
                return Ok((num_nodes, sync_peer));
            },
            Err(CommsInterfaceError::UnexpectedApiResponse) => {
                debug!(target: log_target, "Remote node provided an unexpected api response.",);
                debug!(
                    target: log_target,
                    "Banning peer {} from local node, because they provided an unexpected api response", sync_peer
                );
                ban_sync_peer(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.peer_ban_duration,
                )
                .await?;
            },
            Err(CommsInterfaceError::RequestTimedOut) => {
                debug!(
                    target: log_target,
                    "Failed to fetch mmr node count from peer: {:?}. Retrying.",
                    CommsInterfaceError::RequestTimedOut,
                );
                ban_sync_peer_if_online(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.short_term_peer_ban_duration,
                )
                .await?;
            },
            Err(e) => return Err(BlockSyncError::CommsInterfaceError(e)),
        }
        debug!(
            target: log_target,
            "Retrying mmr node count request. Attempt {}", attempt
        );
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

/// Request the total merkle mountain range node count upto the specified height for the selected MMR from remote base
/// nodes.
pub async fn request_mmr_nodes<B: BlockchainBackend + 'static>(
    log_target: &str,
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut SyncPeers,
    tree: MmrTree,
    pos: u32,
    count: u32,
    height: u64,
    request_retry_attempts: usize,
) -> Result<(Vec<HashOutput>, Bitmap, SyncPeer), BlockSyncError>
{
    let config = shared.config.sync_peer_config;
    for attempt in 1..=request_retry_attempts {
        let sync_peer = select_sync_peer(&config, sync_peers)?;
        debug!(
            target: log_target,
            "Requesting {} mmr nodes ({}-{}) from {}.",
            tree,
            pos,
            pos + count,
            sync_peer.node_id
        );
        match shared
            .comms
            .fetch_mmr_nodes(tree, pos, count, height, Some(sync_peer.node_id.clone()))
            .await
        {
            Ok((added, deleted)) => {
                return Ok((added, Bitmap::deserialize(&deleted), sync_peer));
            },
            Err(CommsInterfaceError::UnexpectedApiResponse) => {
                debug!(target: log_target, "Remote node provided an unexpected api response.",);
                debug!(
                    target: log_target,
                    "Banning peer {} from local node, because they provided an unexpected api response", sync_peer
                );
                ban_sync_peer(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.peer_ban_duration,
                )
                .await?;
            },
            Err(CommsInterfaceError::RequestTimedOut) => {
                debug!(
                    target: log_target,
                    "Failed to fetch mmr nodes from peer: {:?}. Retrying.",
                    CommsInterfaceError::RequestTimedOut,
                );
                ban_sync_peer_if_online(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.short_term_peer_ban_duration,
                )
                .await?;
            },
            Err(e) => return Err(BlockSyncError::CommsInterfaceError(e)),
        }
        debug!(target: log_target, "Retrying mmr nodes download. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

/// Request the total merkle mountain range node count upto the specified height for the selected MMR from remote base
/// nodes.
pub async fn request_kernels<B: BlockchainBackend + 'static>(
    log_target: &str,
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut SyncPeers,
    hashes: Vec<HashOutput>,
    request_retry_attempts: usize,
) -> Result<(Vec<TransactionKernel>, SyncPeer), BlockSyncError>
{
    let config = shared.config.sync_peer_config;
    for attempt in 1..=request_retry_attempts {
        let sync_peer = select_sync_peer(&config, sync_peers)?;
        debug!(
            target: log_target,
            "Requesting {} kernels from {}.",
            hashes.len(),
            sync_peer.node_id
        );
        match shared
            .comms
            .request_kernels_from_peer(hashes.clone(), Some(sync_peer.node_id.clone()))
            .await
        {
            Ok(kernels) => {
                return Ok((kernels, sync_peer));
            },
            Err(CommsInterfaceError::UnexpectedApiResponse) => {
                debug!(target: log_target, "Remote node provided an unexpected api response.",);
                debug!(
                    target: log_target,
                    "Banning peer {} from local node, because they provided an unexpected api response", sync_peer
                );
                ban_sync_peer(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.peer_ban_duration,
                )
                .await?;
            },
            Err(CommsInterfaceError::RequestTimedOut) => {
                debug!(
                    target: log_target,
                    "Failed to fetch kernels from peer: {:?}. Retrying.",
                    CommsInterfaceError::RequestTimedOut,
                );
                ban_sync_peer_if_online(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.short_term_peer_ban_duration,
                )
                .await?;
            },
            Err(e) => return Err(BlockSyncError::CommsInterfaceError(e)),
        }
        debug!(target: log_target, "Retrying kernels download. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}

/// Request the total merkle mountain range node count upto the specified height for the selected MMR from remote base
/// nodes.
pub async fn request_txos<B: BlockchainBackend + 'static>(
    log_target: &str,
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut SyncPeers,
    hashes: &[&HashOutput],
    request_retry_attempts: usize,
) -> Result<(Vec<TransactionOutput>, SyncPeer), BlockSyncError>
{
    let config = shared.config.sync_peer_config;
    for attempt in 1..=request_retry_attempts {
        let sync_peer = select_sync_peer(&config, sync_peers)?;
        // If no hashes are requested, return an empty response without performing the request.
        if hashes.is_empty() {
            return Ok((Vec::new(), sync_peer));
        }
        debug!(
            target: log_target,
            "Requesting {} transaction outputs from {}.",
            hashes.len(),
            sync_peer.node_id
        );
        match shared
            .comms
            .request_txos_from_peer(
                hashes.into_iter().map(|c| Clone::clone(&**c)).collect(),
                Some(sync_peer.node_id.clone()),
            )
            .await
        {
            Ok(utxos) => {
                return Ok((utxos, sync_peer));
            },
            Err(CommsInterfaceError::UnexpectedApiResponse) => {
                debug!(target: log_target, "Remote node provided an unexpected api response.",);
                debug!(
                    target: log_target,
                    "Banning peer {} from local node, because they provided an unexpected api response", sync_peer
                );
                ban_sync_peer(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.peer_ban_duration,
                )
                .await?;
            },
            Err(CommsInterfaceError::RequestTimedOut) => {
                debug!(
                    target: log_target,
                    "Failed to fetch kernels from peer: {:?}. Retrying.",
                    CommsInterfaceError::RequestTimedOut,
                );
                ban_sync_peer_if_online(
                    log_target,
                    &mut shared.connectivity,
                    sync_peers,
                    sync_peer.clone(),
                    config.short_term_peer_ban_duration,
                )
                .await?;
            },
            Err(e) => return Err(BlockSyncError::CommsInterfaceError(e)),
        }
        debug!(target: log_target, "Retrying kernels download. Attempt {}", attempt);
    }
    Err(BlockSyncError::MaxRequestAttemptsReached)
}
