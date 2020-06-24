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
        state_machine::BaseNodeStateMachine,
        states::{
            block_sync::BlockSyncError,
            helpers::{
                ban_sync_peer,
                request_headers,
                request_kernels,
                request_mmr_node_count,
                request_mmr_nodes,
                request_txos,
            },
        },
    },
    blocks::blockheader::BlockHeader,
    chain_storage::{async_db, BlockchainBackend, BlockchainDatabase, ChainMetadata, ChainStorageError, MmrTree},
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use croaring::Bitmap;
use derive_error::Error;
use log::*;
use std::cmp::min;
use tari_comms::peer_manager::NodeId;
use tari_crypto::tari_utilities::hash::Hashable;

const LOG_TARGET: &str = "c::bn::states::horizon_state_sync";

// TODO: The horizon state tends to be stable, but when a pruned node was on the incorrect chain and the node was turned
// off until its tip falls below the horizon sync height then the current horizon state sync code cannot continue as it
// needs to delete some of the previous chain data before attempting to extend it.

// The selected horizon block height might be similar to other pruned nodes resulting in spent UTXOs being discarded
// before the horizon sync has completed. A height offset is used to help with this problem by selecting a future height
// after the current horizon block height.
const HORIZON_SYNC_HEIGHT_OFFSET: u64 = 50;
// The maximum number of retry attempts a node can perform a request from remote nodes.
const MAX_SYNC_REQUEST_RETRY_ATTEMPTS: usize = 3;
const MAX_HEADER_REQUEST_RETRY_ATTEMPTS: usize = 5;
const MAX_MMR_NODE_REQUEST_RETRY_ATTEMPTS: usize = 5;
const MAX_KERNEL_REQUEST_RETRY_ATTEMPTS: usize = 5;
const MAX_TXO_REQUEST_RETRY_ATTEMPTS: usize = 5;
// The number of headers that can be requested in a single query.
const HEADER_REQUEST_SIZE: usize = 100;
// The number of MMR nodes or UTXOs that can be requested in a single query.
const MMR_NODE_OR_UTXO_REQUEST_SIZE: usize = 1000;

/// Configuration for the Horizon State Synchronization.
#[derive(Clone, Copy)]
pub struct HorizonSyncConfig {
    pub horizon_sync_height_offset: u64,
    pub max_sync_request_retry_attempts: usize,
    pub max_header_request_retry_attempts: usize,
    pub max_mmr_node_request_retry_attempts: usize,
    pub max_kernel_request_retry_attempts: usize,
    pub max_txo_request_retry_attempts: usize,
    pub header_request_size: usize,
    pub mmr_node_or_utxo_request_size: usize,
}

impl Default for HorizonSyncConfig {
    fn default() -> Self {
        Self {
            horizon_sync_height_offset: HORIZON_SYNC_HEIGHT_OFFSET,
            max_sync_request_retry_attempts: MAX_SYNC_REQUEST_RETRY_ATTEMPTS,
            max_header_request_retry_attempts: MAX_HEADER_REQUEST_RETRY_ATTEMPTS,
            max_mmr_node_request_retry_attempts: MAX_MMR_NODE_REQUEST_RETRY_ATTEMPTS,
            max_kernel_request_retry_attempts: MAX_KERNEL_REQUEST_RETRY_ATTEMPTS,
            max_txo_request_retry_attempts: MAX_TXO_REQUEST_RETRY_ATTEMPTS,
            header_request_size: HEADER_REQUEST_SIZE,
            mmr_node_or_utxo_request_size: MMR_NODE_OR_UTXO_REQUEST_SIZE,
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum HorizonSyncError {
    EmptyResponse,
    IncorrectResponse,
    InvalidHeader,
    MaxSyncAttemptsReached,
    ChainStorageError(ChainStorageError),
    CommsInterfaceError(CommsInterfaceError),
    BlockSyncError(BlockSyncError),
}

/// Perform a horizon state sync by syncing the headers, kernels, UTXO MMR Nodes, RangeProof MMR Nodes and UTXO set.
pub async fn synchronize_horizon_state<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    network_metadata: &ChainMetadata,
    sync_peers: &mut Vec<NodeId>,
) -> Result<(), HorizonSyncError>
{
    info!(target: LOG_TARGET, "Synchronizing horizon state.");
    let local_metadata = shared.db.get_metadata()?;
    let network_tip_height = network_metadata.height_of_longest_chain.unwrap_or(0);
    let horizon_sync_height = select_horizon_sync_height(
        &local_metadata,
        network_tip_height,
        shared.config.horizon_sync_config.horizon_sync_height_offset,
    );
    let local_tip_height = local_metadata.height_of_longest_chain.unwrap_or(0);
    if local_tip_height >= horizon_sync_height {
        trace!(
            target: LOG_TARGET,
            "Horizon state already synchronized, continue with block sync."
        );
        return Ok(());
    }
    trace!(
        target: LOG_TARGET,
        "Syncing from height {} to horizon sync height {}.",
        local_tip_height,
        horizon_sync_height
    );

    // During horizon state syncing the blockchain backend will be in an inconsistent state until the entire horizon
    // state has been synced. Reset the local chain metadata will limit other nodes and local service from
    // requesting data while the horizon sync is in progress.
    trace!(target: LOG_TARGET, "Resetting chain metadata.");
    reset_chain_metadata(&shared.db).await?;
    trace!(target: LOG_TARGET, "Synchronizing headers");
    synchronize_headers(shared, sync_peers, horizon_sync_height).await?;
    trace!(target: LOG_TARGET, "Synchronizing kernels");
    synchronize_kernels(shared, sync_peers, horizon_sync_height).await?;
    trace!(target: LOG_TARGET, "Check the deletion state of current UTXOs");
    check_state_of_current_utxos(shared, sync_peers, local_tip_height, horizon_sync_height).await?;
    trace!(target: LOG_TARGET, "Synchronizing UTXOs and RangeProofs");
    synchronize_utxos_and_rangeproofs(shared, sync_peers, horizon_sync_height).await?;
    trace!(target: LOG_TARGET, "Finalizing horizon synchronizing");
    finalize_horizon_sync(shared).await?;

    Ok(())
}

// Calculate the target horizon sync height from the horizon height, network tip and a height offset.
fn select_horizon_sync_height(
    local_metadata: &ChainMetadata,
    network_tip_height: u64,
    horizon_sync_height_offset: u64,
) -> u64
{
    let horizon_height = local_metadata.horizon_block(network_tip_height);
    min(horizon_height + horizon_sync_height_offset, network_tip_height)
}

// Reset the chain metadata to the genesis block while in horizon sync mode. The chain metadata will be restored to the
// latest data once the horizon sync has been finalized.
async fn reset_chain_metadata<B: BlockchainBackend + 'static>(
    db: &BlockchainDatabase<B>,
) -> Result<(), HorizonSyncError> {
    let genesis_header = db.fetch_header(0)?;
    let mut metadata = db.get_metadata()?;
    metadata.height_of_longest_chain = Some(genesis_header.height);
    metadata.best_block = Some(genesis_header.hash());
    metadata.accumulated_difficulty = Some(genesis_header.achieved_difficulty());
    async_db::write_metadata(db.clone(), metadata).await?;
    Ok(())
}

// Check the received set of headers.
async fn validate_headers<B: BlockchainBackend + 'static>(
    db: &BlockchainDatabase<B>,
    block_nums: &[u64],
    headers: &[BlockHeader],
) -> Result<(), HorizonSyncError>
{
    if headers.is_empty() {
        return Err(HorizonSyncError::EmptyResponse);
    }
    // Check that the received headers are the requested headers
    if (0..block_nums.len()).any(|i| headers[i].height != block_nums[i]) {
        return Err(HorizonSyncError::IncorrectResponse);
    }
    // Check that the first header is linked to the chain tip header
    if let Some(curr_header) = headers.first() {
        let prev_header = async_db::fetch_tip_header(db.clone()).await?;
        if prev_header.height + 1 != curr_header.height {
            return Err(HorizonSyncError::InvalidHeader);
        }
        if curr_header.prev_hash != prev_header.hash() {
            return Err(HorizonSyncError::InvalidHeader);
        }
    }
    // Check that header set forms a sequence
    for index in 1..headers.len() {
        let prev_header = &headers[index.saturating_sub(1)];
        let curr_header = &headers[index];
        if prev_header.height + 1 != curr_header.height {
            return Err(HorizonSyncError::InvalidHeader);
        }
        if curr_header.prev_hash != (*prev_header).hash() {
            return Err(HorizonSyncError::InvalidHeader);
        }
    }

    // TODO: Check header PoW

    Ok(())
}

// Synchronize headers upto the horizon sync height from remote sync peers.
async fn synchronize_headers<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    horizon_sync_height: u64,
) -> Result<(), HorizonSyncError>
{
    let config = shared.config.horizon_sync_config;
    let tip_header = shared.db.fetch_tip_header()?;
    if tip_header.height >= horizon_sync_height {
        return Ok(());
    }

    for block_nums in ((tip_header.height + 1)..=horizon_sync_height)
        .collect::<Vec<u64>>()
        .chunks(config.header_request_size)
    {
        for attempt in 1..=config.max_sync_request_retry_attempts {
            let (headers, sync_peer) = request_headers(
                LOG_TARGET,
                shared,
                sync_peers,
                block_nums,
                config.max_header_request_retry_attempts,
            )
            .await?;
            match validate_headers(&shared.db, block_nums, &headers).await {
                Ok(_) => {
                    async_db::insert_headers(shared.db.clone(), headers).await?;
                    trace!(
                        target: LOG_TARGET,
                        "Headers successfully added to database: {:?}",
                        block_nums
                    );
                    break;
                },
                Err(HorizonSyncError::EmptyResponse) |
                Err(HorizonSyncError::IncorrectResponse) |
                Err(HorizonSyncError::InvalidHeader) => {
                    warn!(target: LOG_TARGET, "Invalid headers received from peer.",);
                    debug!(
                        target: LOG_TARGET,
                        "Banning peer {} from local node, because they supplied invalid headers", sync_peer
                    );
                    ban_sync_peer(
                        LOG_TARGET,
                        &mut shared.connectivity,
                        sync_peers,
                        sync_peer.clone(),
                        shared.config.sync_peer_config.peer_ban_duration,
                    )
                    .await?;
                },
                Err(e) => return Err(e),
            };
            debug!(target: LOG_TARGET, "Retrying header sync. Attempt {}", attempt);
            if attempt == config.max_sync_request_retry_attempts {
                return Err(HorizonSyncError::MaxSyncAttemptsReached);
            }
        }
    }
    Ok(())
}

// Check the received set of kernels.
fn validate_kernels(kernel_hashes: &[HashOutput], kernels: &[TransactionKernel]) -> Result<(), HorizonSyncError> {
    if kernels.is_empty() {
        return Err(HorizonSyncError::EmptyResponse);
    }
    // Check if the correct number of kernels returned
    if kernel_hashes.len() != kernels.len() {
        return Err(HorizonSyncError::IncorrectResponse);
    }

    // Check that kernel set is the requested kernels
    if (0..kernel_hashes.len()).any(|i| kernels[i].hash() != kernel_hashes[i]) {
        return Err(HorizonSyncError::IncorrectResponse);
    }
    Ok(())
}

// Synchronize kernels upto the horizon sync height from remote sync peers.
async fn synchronize_kernels<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    horizon_sync_height: u64,
) -> Result<(), HorizonSyncError>
{
    let config = shared.config.horizon_sync_config;
    let local_num_kernels =
        async_db::fetch_mmr_node_count(shared.db.clone(), MmrTree::Kernel, horizon_sync_height).await?;
    let (remote_num_kernels, sync_peer) = request_mmr_node_count(
        LOG_TARGET,
        shared,
        sync_peers,
        MmrTree::Kernel,
        horizon_sync_height,
        config.max_mmr_node_request_retry_attempts,
    )
    .await?;

    if local_num_kernels >= remote_num_kernels {
        return Ok(());
    }

    for indices in (local_num_kernels..remote_num_kernels)
        .collect::<Vec<u32>>()
        .chunks(config.mmr_node_or_utxo_request_size)
    {
        for attempt in 1..=config.max_sync_request_retry_attempts {
            let pos = indices.first().map(Clone::clone).unwrap_or(0);
            let count = indices.len() as u32;
            let (kernel_hashes, _, sync_peer1) = request_mmr_nodes(
                LOG_TARGET,
                shared,
                sync_peers,
                MmrTree::Kernel,
                pos,
                count,
                horizon_sync_height,
                config.max_mmr_node_request_retry_attempts,
            )
            .await?;
            let (kernels, sync_peer2) = request_kernels(
                LOG_TARGET,
                shared,
                sync_peers,
                kernel_hashes.clone(),
                config.max_kernel_request_retry_attempts,
            )
            .await?;

            match validate_kernels(&kernel_hashes, &kernels) {
                Ok(_) => {
                    async_db::insert_kernels(shared.db.clone(), kernels).await?;
                    trace!(
                        target: LOG_TARGET,
                        "Kernels successfully added to database: {:?}",
                        kernel_hashes
                    );
                    break;
                },
                Err(HorizonSyncError::EmptyResponse) | Err(HorizonSyncError::IncorrectResponse) => {
                    warn!(target: LOG_TARGET, "Invalid kernels received from peer.",);
                    if sync_peer1 == sync_peer2 {
                        debug!(
                            target: LOG_TARGET,
                            "Banning peer {} from local node, because they supplied invalid kernels", sync_peer
                        );
                        ban_sync_peer(
                            LOG_TARGET,
                            &mut shared.connectivity,
                            sync_peers,
                            sync_peer.clone(),
                            shared.config.sync_peer_config.peer_ban_duration,
                        )
                        .await?;
                    }
                },
                Err(e) => return Err(e),
            };
            debug!(target: LOG_TARGET, "Retrying kernel sync. Attempt {}", attempt);
            if attempt == config.max_sync_request_retry_attempts {
                return Err(HorizonSyncError::MaxSyncAttemptsReached);
            }
        }
    }
    Ok(())
}

// Validate the received UTXO set and, UTXO and RangeProofs MMR nodes.
fn validate_utxo_hashes(
    local_utxo_hashes: &[HashOutput],
    remote_utxo_hashes: &[HashOutput],
) -> Result<(), HorizonSyncError>
{
    // Check that the correct number of utxo hashes returned
    if local_utxo_hashes.len() != remote_utxo_hashes.len() {
        return Err(HorizonSyncError::IncorrectResponse);
    }

    // Check that the received utxo set is the same as local
    if (0..local_utxo_hashes.len()).any(|i| local_utxo_hashes[i] != remote_utxo_hashes[i]) {
        return Err(HorizonSyncError::IncorrectResponse);
    }

    Ok(())
}

async fn check_state_of_current_utxos<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    local_tip_height: u64,
    horizon_sync_height: u64,
) -> Result<(), HorizonSyncError>
{
    let config = shared.config.horizon_sync_config;
    let local_num_utxo_nodes =
        async_db::fetch_mmr_node_count(shared.db.clone(), MmrTree::Utxo, local_tip_height).await?;
    for indices in (0..local_num_utxo_nodes)
        .collect::<Vec<u32>>()
        .chunks(config.mmr_node_or_utxo_request_size)
    {
        for attempt in 1..=config.max_sync_request_retry_attempts {
            let pos = indices.first().map(Clone::clone).unwrap_or(0);
            let count = indices.len() as u32;
            let (remote_utxo_hashes, remote_utxo_deleted, _sync_peer) = request_mmr_nodes(
                LOG_TARGET,
                shared,
                sync_peers,
                MmrTree::Utxo,
                pos,
                count,
                horizon_sync_height,
                config.max_mmr_node_request_retry_attempts,
            )
            .await?;
            let (local_utxo_hashes, local_utxo_bitmap_bytes) = shared
                .local_node_interface
                .fetch_mmr_nodes(MmrTree::Utxo, pos, count, horizon_sync_height)
                .await?;
            let local_utxo_deleted = Bitmap::deserialize(&local_utxo_bitmap_bytes);

            match validate_utxo_hashes(&remote_utxo_hashes, &local_utxo_hashes) {
                Ok(_) => {
                    for (index, utxo_hash) in local_utxo_hashes.iter().enumerate() {
                        let local_deleted = local_utxo_deleted.contains(index as u32);
                        let remote_deleted = remote_utxo_deleted.contains(index as u32);
                        if remote_deleted && !local_deleted {
                            shared.db.delete_mmr_node(MmrTree::Utxo, &utxo_hash)?;
                        }
                    }
                    trace!(
                        target: LOG_TARGET,
                        "Existing UTXOs checked and updated: {:?}",
                        local_utxo_hashes
                    );

                    break;
                },
                Err(HorizonSyncError::IncorrectResponse) => {
                    // TODO: not sure you can ban here as the local node might have the incorrect chain.
                    warn!(target: LOG_TARGET, "Invalid UTXO hashes received from peer.",);
                },
                Err(e) => return Err(e),
            };
            debug!(target: LOG_TARGET, "Retrying UTXO state check. Attempt {}", attempt);
            if attempt == config.max_sync_request_retry_attempts {
                return Err(HorizonSyncError::MaxSyncAttemptsReached);
            }
        }
    }
    Ok(())
}

// Validate the received UTXO set and, UTXO and RangeProofs MMR nodes.
fn validate_utxos_and_rps(
    utxo_hashes: &[HashOutput],
    rp_hashes: &[HashOutput],
    request_utxo_hashes: &[HashOutput],
    request_rp_hashes: &[HashOutput],
    utxos: &[TransactionOutput],
) -> Result<(), HorizonSyncError>
{
    if utxo_hashes.is_empty() | rp_hashes.is_empty() | utxos.is_empty() {
        return Err(HorizonSyncError::EmptyResponse);
    }
    // Check if the same number of utxo and rp MMR nodes returned
    if utxo_hashes.len() != rp_hashes.len() {
        return Err(HorizonSyncError::IncorrectResponse);
    }
    // Check that the correct number of utxos returned
    if request_utxo_hashes.len() != utxos.len() {
        return Err(HorizonSyncError::IncorrectResponse);
    }

    // Check that utxo set is the requested utxos
    if (0..request_utxo_hashes.len()).any(|i| utxos[i].hash() != request_utxo_hashes[i]) {
        return Err(HorizonSyncError::IncorrectResponse);
    }

    // Check that utxo set matches the provided RangeProof MMR Nodes
    if (0..request_rp_hashes.len()).any(|i| utxos[i].proof.hash() != request_rp_hashes[i]) {
        return Err(HorizonSyncError::IncorrectResponse);
    }

    Ok(())
}

// Synchronize UTXO MMR Nodes, RangeProof MMR Nodes and the UTXO set upto the horizon sync height from remote sync
// peers.
async fn synchronize_utxos_and_rangeproofs<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_peers: &mut Vec<NodeId>,
    horizon_sync_height: u64,
) -> Result<(), HorizonSyncError>
{
    let config = shared.config.horizon_sync_config;
    let local_num_utxo_nodes =
        async_db::fetch_mmr_node_count(shared.db.clone(), MmrTree::Utxo, horizon_sync_height).await?;
    let (remote_num_utxo_nodes, _sync_peer) = request_mmr_node_count(
        LOG_TARGET,
        shared,
        sync_peers,
        MmrTree::Utxo,
        horizon_sync_height,
        config.max_mmr_node_request_retry_attempts,
    )
    .await?;

    for indices in (local_num_utxo_nodes..remote_num_utxo_nodes)
        .collect::<Vec<u32>>()
        .chunks(config.mmr_node_or_utxo_request_size)
    {
        for attempt in 1..=config.max_sync_request_retry_attempts {
            let pos = indices.first().map(Clone::clone).unwrap_or(0);
            let count = indices.len() as u32;
            let (utxo_hashes, utxo_bitmap, sync_peer1) = request_mmr_nodes(
                LOG_TARGET,
                shared,
                sync_peers,
                MmrTree::Utxo,
                pos,
                count,
                horizon_sync_height,
                config.max_mmr_node_request_retry_attempts,
            )
            .await?;
            let (rp_hashes, _, sync_peer2) = request_mmr_nodes(
                LOG_TARGET,
                shared,
                sync_peers,
                MmrTree::RangeProof,
                pos,
                count,
                horizon_sync_height,
                config.max_mmr_node_request_retry_attempts,
            )
            .await?;

            // Construct the list of hashes of the UTXOs that need to be requested.
            let mut request_utxo_hashes = Vec::<HashOutput>::new();
            let mut request_rp_hashes = Vec::<HashOutput>::new();
            let mut is_stxos = Vec::<bool>::new();
            for index in 0..utxo_hashes.len() {
                let deleted = utxo_bitmap.contains(index as u32 + 1);
                is_stxos.push(deleted);
                if !deleted {
                    request_utxo_hashes.push(utxo_hashes[index].clone());
                    request_rp_hashes.push(rp_hashes[index].clone());
                }
            }
            // Download a partial UTXO set
            let (utxos, sync_peer3) = request_txos(
                LOG_TARGET,
                shared,
                sync_peers,
                request_utxo_hashes.clone(),
                config.max_txo_request_retry_attempts,
            )
            .await?;

            match validate_utxos_and_rps(
                &utxo_hashes,
                &rp_hashes,
                &request_utxo_hashes,
                &request_rp_hashes,
                &utxos,
            ) {
                Ok(_) => {
                    // The order of these inserts are important to ensure the MMRs are constructed correctly and the
                    // roots match.
                    let mut utxos_index = 0;
                    for (index, is_stxo) in is_stxos.into_iter().enumerate() {
                        if is_stxo {
                            shared
                                .db
                                .insert_mmr_node(MmrTree::Utxo, utxo_hashes[index].clone(), is_stxo)?;
                            shared
                                .db
                                .insert_mmr_node(MmrTree::RangeProof, rp_hashes[index].clone(), false)?;
                        } else {
                            // Inserting the UTXO will also insert the corresponding UTXO and RangeProof MMR Nodes.
                            shared.db.insert_utxo(utxos[utxos_index].clone())?;
                            utxos_index += 1;
                        }
                    }
                    trace!(
                        target: LOG_TARGET,
                        "UTXOs and MMR nodes inserted into database: {:?}",
                        utxo_hashes
                    );

                    break;
                },
                Err(HorizonSyncError::EmptyResponse) | Err(HorizonSyncError::IncorrectResponse) => {
                    warn!(target: LOG_TARGET, "Invalid UTXOs or MMR Nodes received from peer.",);
                    if (sync_peer1 == sync_peer2) && (sync_peer1 == sync_peer3) {
                        debug!(
                            target: LOG_TARGET,
                            "Banning peer {} from local node, because they supplied invalid UTXOs or MMR Nodes",
                            sync_peer1
                        );
                        ban_sync_peer(
                            LOG_TARGET,
                            &mut shared.connectivity,
                            sync_peers,
                            sync_peer1.clone(),
                            shared.config.sync_peer_config.peer_ban_duration,
                        )
                        .await?;
                    }
                },
                Err(e) => return Err(e),
            };
            debug!(target: LOG_TARGET, "Retrying kernel sync. Attempt {}", attempt);
            if attempt == config.max_sync_request_retry_attempts {
                return Err(HorizonSyncError::MaxSyncAttemptsReached);
            }
        }
    }
    Ok(())
}

// Finalize the horizon state synchronization by setting the chain metadata to the local tip and committing the horizon
// state to the blockchain backend.
async fn finalize_horizon_sync<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
) -> Result<(), HorizonSyncError> {
    // TODO: Perform final validation on full synced horizon state before committing horizon state checkpoint.
    // TODO: Verify header and kernel set using Mmr Roots

    shared.db.commit_horizon_state()?;

    Ok(())
}
