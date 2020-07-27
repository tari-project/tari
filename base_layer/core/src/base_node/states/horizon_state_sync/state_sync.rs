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

use super::error::HorizonSyncError;
use crate::{
    base_node::{
        states::{helpers, BlockSyncInfo, StateEvent, StatusInfo},
        BaseNodeStateMachine,
    },
    blocks::BlockHeader,
    chain_storage::{async_db, BlockchainBackend, BlockchainDatabase, ChainMetadata, MmrTree},
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use croaring::Bitmap;
use log::*;
use std::cmp;
use tari_comms::peer_manager::NodeId;
use tari_crypto::tari_utilities::Hashable;
use tokio::task::spawn_blocking;

const LOG_TARGET: &str = "c::bn::states::horizon_state_sync";

#[derive(Clone, Debug)]
pub struct HorizonStateSync {
    network_metadata: ChainMetadata,
    sync_peers: Vec<NodeId>,
}

impl HorizonStateSync {
    pub fn new(network_metadata: ChainMetadata, sync_peers: Vec<NodeId>) -> Self {
        Self {
            network_metadata,
            sync_peers,
        }
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent
    {
        match async_db::get_metadata(shared.db.clone()).await {
            Ok(local_metadata) => {
                shared
                    .set_status_info(StatusInfo::HorizonSync(BlockSyncInfo::new(
                        self.network_metadata.height_of_longest_chain(),
                        local_metadata.height_of_longest_chain(),
                        self.sync_peers.clone(),
                    )))
                    .await;

                if !local_metadata.is_pruned_node() {
                    warn!(
                        target: LOG_TARGET,
                        "HorizonStateSync invoked but node is not in pruned mode"
                    );
                    return StateEvent::HorizonStateSynchronized;
                }

                info!(target: LOG_TARGET, "Synchronizing horizon state.");
                let horizon_sync_height = self.get_horizon_sync_height(&shared, &local_metadata);
                let local_tip_height = local_metadata.height_of_longest_chain.unwrap_or(0);
                if local_tip_height >= horizon_sync_height {
                    debug!(target: LOG_TARGET, "Horizon state already synchronized.");
                    return StateEvent::HorizonStateSynchronized;
                }
                debug!(
                    target: LOG_TARGET,
                    "Horizon sync starting to height {}", horizon_sync_height
                );

                let mut horizon_header_sync = HorizonStateSynchronization {
                    shared,
                    local_metadata,
                    sync_peers: &mut self.sync_peers,
                    horizon_sync_height,
                };
                match horizon_header_sync.synchronize().await {
                    Ok(()) => {
                        info!(target: LOG_TARGET, "Horizon state has synchronised.");
                        StateEvent::HorizonStateSynchronized
                    },
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Synchronizing horizon state has failed. {:?}", err);
                        StateEvent::HorizonStateSyncFailure
                    },
                }
            },
            Err(err) => StateEvent::FatalError(format!("Unable to retrieve local chain metadata. {:?}", err)),
        }
    }

    // Calculate the target horizon sync height from the horizon height, network tip and a height offset.
    fn get_horizon_sync_height<B: BlockchainBackend>(
        &self,
        shared: &BaseNodeStateMachine<B>,
        local_metadata: &ChainMetadata,
    ) -> u64
    {
        let network_tip_height = self.network_metadata.height_of_longest_chain.unwrap_or(0);
        let horizon_height = local_metadata.horizon_block(network_tip_height);
        let horizon_sync_height_offset = shared.config.horizon_sync_config.horizon_sync_height_offset;
        cmp::min(horizon_height + horizon_sync_height_offset, network_tip_height)
    }

    pub fn network_metadata(&self) -> &ChainMetadata {
        &self.network_metadata
    }

    pub fn sync_peers(&self) -> &[NodeId] {
        &self.sync_peers
    }
}

impl PartialEq for HorizonStateSync {
    fn eq(&self, other: &Self) -> bool {
        self.sync_peers == other.sync_peers && self.network_metadata == other.network_metadata
    }
}

struct HorizonStateSynchronization<'a, 'b, B> {
    shared: &'a mut BaseNodeStateMachine<B>,
    local_metadata: ChainMetadata,
    sync_peers: &'b mut Vec<NodeId>,
    horizon_sync_height: u64,
}

impl<B: BlockchainBackend + 'static> HorizonStateSynchronization<'_, '_, B> {
    pub async fn synchronize(&mut self) -> Result<(), HorizonSyncError> {
        let local_tip_height = self.local_metadata.height_of_longest_chain.unwrap_or(0);
        debug!(
            target: LOG_TARGET,
            "Syncing from height {} to horizon sync height {}.", local_tip_height, self.horizon_sync_height
        );

        let tip_header = async_db::fetch_tip_header(self.db()).await?;
        if tip_header.height >= self.horizon_sync_height {
            return Ok(());
        }

        // During horizon state syncing the blockchain backend will be in an inconsistent state until the entire horizon
        // state has been synced. Reset the local chain metadata will limit other nodes and local service from
        // requesting data while the horizon sync is in progress.
        trace!(target: LOG_TARGET, "Resetting chain metadata.");
        self.reset_chain_metadata_to_genesis().await?;
        trace!(target: LOG_TARGET, "Synchronizing headers");
        self.synchronize_headers(tip_header.height).await?;
        trace!(target: LOG_TARGET, "Synchronizing kernels");
        self.synchronize_kernels().await?;
        trace!(target: LOG_TARGET, "Check the deletion state of current UTXOs");
        self.check_state_of_current_utxos(tip_header.height).await?;
        trace!(target: LOG_TARGET, "Synchronizing UTXOs and RangeProofs");
        self.synchronize_utxos_and_rangeproofs().await?;
        trace!(target: LOG_TARGET, "Finalizing horizon synchronizing");
        self.finalize_horizon_sync().await?;

        Ok(())
    }

    async fn synchronize_headers(&mut self, tip_height: u64) -> Result<(), HorizonSyncError> {
        let config = self.shared.config.horizon_sync_config;

        let block_height_range = ((tip_height + 1)..=self.horizon_sync_height).collect::<Vec<_>>();
        for block_nums in block_height_range.chunks(config.header_request_size) {
            for attempt in 1..=config.max_sync_request_retry_attempts {
                let (headers, sync_peer) = helpers::request_headers(
                    LOG_TARGET,
                    self.shared,
                    self.sync_peers,
                    block_nums,
                    self.shared.config.horizon_sync_config.max_header_request_retry_attempts,
                )
                .await?;

                match self.validate_and_insert_headers(block_nums, headers).await {
                    Ok(_) => {
                        trace!(
                            target: LOG_TARGET,
                            "Headers successfully added to database: {:?}",
                            block_nums
                        );
                        break;
                    },
                    Err(err @ HorizonSyncError::EmptyResponse) |
                    Err(err @ HorizonSyncError::IncorrectResponse) |
                    Err(err @ HorizonSyncError::InvalidHeader(_)) => {
                        warn!(target: LOG_TARGET, "Peer `{}`: {}", sync_peer, err);
                        debug!(
                            target: LOG_TARGET,
                            "Banning peer {} from local node, because they supplied an invalid response", sync_peer
                        );
                        self.ban_sync_peer(sync_peer).await?;
                    },
                    // Fatal
                    Err(e) => return Err(e),
                }

                if attempt == config.max_sync_request_retry_attempts {
                    debug!(target: LOG_TARGET, "Reached maximum ({}) attempts", attempt);
                    return Err(HorizonSyncError::MaxSyncAttemptsReached);
                }
                debug!(
                    target: LOG_TARGET,
                    "Retrying header sync. Attempt {} of {}", attempt, config.max_sync_request_retry_attempts
                );
            }
        }

        Ok(())
    }

    // Synchronize kernels upto the horizon sync height from remote sync peers.
    async fn synchronize_kernels(&mut self) -> Result<(), HorizonSyncError> {
        let config = self.shared.config.horizon_sync_config;
        let local_num_kernels =
            async_db::fetch_mmr_node_count(self.db(), MmrTree::Kernel, self.horizon_sync_height).await?;
        let (remote_num_kernels, sync_peer) = helpers::request_mmr_node_count(
            LOG_TARGET,
            self.shared,
            self.sync_peers,
            MmrTree::Kernel,
            self.horizon_sync_height,
            config.max_mmr_node_request_retry_attempts,
        )
        .await?;

        if local_num_kernels >= remote_num_kernels {
            debug!(target: LOG_TARGET, "Local kernel set already synchronized");
            return Ok(());
        }

        debug!(
            target: LOG_TARGET,
            "Requesting kernels from peers between {} and {}", local_num_kernels, remote_num_kernels
        );
        let kernel_num_range = (local_num_kernels..remote_num_kernels).collect::<Vec<_>>();
        for indices in kernel_num_range.chunks(config.mmr_node_or_utxo_request_size) {
            for attempt in 1..=config.max_sync_request_retry_attempts {
                let pos = indices.first().cloned().unwrap_or(0);
                let count = indices.len() as u32;
                let (kernel_hashes, _, sync_peer1) = helpers::request_mmr_nodes(
                    LOG_TARGET,
                    self.shared,
                    self.sync_peers,
                    MmrTree::Kernel,
                    pos,
                    count,
                    self.horizon_sync_height,
                    config.max_mmr_node_request_retry_attempts,
                )
                .await?;
                let (kernels, sync_peer2) = helpers::request_kernels(
                    LOG_TARGET,
                    self.shared,
                    self.sync_peers,
                    kernel_hashes.clone(),
                    config.max_kernel_request_retry_attempts,
                )
                .await?;

                match self.validate_kernels(&kernel_hashes, &kernels) {
                    Ok(_) => {
                        async_db::insert_kernels(self.db(), kernels).await?;
                        trace!(
                            target: LOG_TARGET,
                            "Kernels successfully added to database: {:?}",
                            kernel_hashes
                        );
                        break;
                    },
                    Err(err @ HorizonSyncError::EmptyResponse { .. }) |
                    Err(err @ HorizonSyncError::IncorrectResponse { .. }) => {
                        warn!(target: LOG_TARGET, "{}", err);
                        // TODO: Fetching mmr nodes and kernels should both be attempted for the same peer
                        if sync_peer1 == sync_peer2 {
                            debug!(
                                target: LOG_TARGET,
                                "Banning peer {} from local node, because they supplied invalid kernels", sync_peer
                            );
                            self.ban_sync_peer(sync_peer.clone()).await?;
                        }
                    },
                    Err(e) => return Err(e),
                };
                debug!(
                    target: LOG_TARGET,
                    "Retrying kernel sync. Attempt {} of {}", attempt, config.max_sync_request_retry_attempts
                );
                if attempt == config.max_sync_request_retry_attempts {
                    return Err(HorizonSyncError::MaxSyncAttemptsReached);
                }
            }
        }
        Ok(())
    }

    async fn ban_sync_peer(&mut self, sync_peer: NodeId) -> Result<(), HorizonSyncError> {
        helpers::ban_sync_peer(
            LOG_TARGET,
            &mut self.shared.connectivity,
            self.sync_peers,
            sync_peer,
            self.shared.config.sync_peer_config.peer_ban_duration,
        )
        .await?;
        Ok(())
    }

    async fn check_state_of_current_utxos(&mut self, local_tip_height: u64) -> Result<(), HorizonSyncError> {
        let config = self.shared.config.horizon_sync_config;
        let local_num_utxo_nodes = async_db::fetch_mmr_node_count(self.db(), MmrTree::Utxo, local_tip_height).await?;
        let range = (0..local_num_utxo_nodes).collect::<Vec<_>>();
        for indices in range.chunks(config.mmr_node_or_utxo_request_size) {
            for attempt in 1..=config.max_sync_request_retry_attempts {
                let pos = indices.first().cloned().unwrap_or(0);
                let count = indices.len() as u32;
                let (remote_utxo_hashes, remote_utxo_deleted, sync_peer) = helpers::request_mmr_nodes(
                    LOG_TARGET,
                    self.shared,
                    self.sync_peers,
                    MmrTree::Utxo,
                    pos,
                    count,
                    self.horizon_sync_height,
                    config.max_mmr_node_request_retry_attempts,
                )
                .await?;
                let (local_utxo_hashes, local_utxo_bitmap_bytes) = self
                    .shared
                    .local_node_interface
                    .fetch_mmr_nodes(MmrTree::Utxo, pos, count, self.horizon_sync_height)
                    .await?;
                let local_utxo_deleted = Bitmap::deserialize(&local_utxo_bitmap_bytes);

                match self.validate_utxo_hashes(&remote_utxo_hashes, &local_utxo_hashes) {
                    Ok(_) => {
                        trace!(
                            target: LOG_TARGET,
                            "Existing UTXOs is being checked and updated: {:?}",
                            local_utxo_hashes
                        );
                        for (index, utxo_hash) in local_utxo_hashes.into_iter().enumerate() {
                            let local_deleted = local_utxo_deleted.contains(index as u32);
                            let remote_deleted = remote_utxo_deleted.contains(index as u32);
                            if remote_deleted && !local_deleted {
                                async_db::delete_mmr_node(self.db(), MmrTree::Utxo, utxo_hash).await?;
                            }
                        }

                        break;
                    },
                    Err(err @ HorizonSyncError::IncorrectResponse) => {
                        // TODO: not sure you can ban here as the local node might have the incorrect chain.
                        warn!(
                            target: LOG_TARGET,
                            "Invalid UTXO hashes received from peer `{}`: {}", sync_peer, err
                        );
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

    // Synchronize UTXO MMR Nodes, RangeProof MMR Nodes and the UTXO set upto the horizon sync height from remote sync
    // peers.
    async fn synchronize_utxos_and_rangeproofs(&mut self) -> Result<(), HorizonSyncError> {
        let config = self.shared.config.horizon_sync_config;
        let local_num_utxo_nodes =
            async_db::fetch_mmr_node_count(self.shared.db.clone(), MmrTree::Utxo, self.horizon_sync_height).await?;
        let (remote_num_utxo_nodes, _sync_peer) = helpers::request_mmr_node_count(
            LOG_TARGET,
            self.shared,
            self.sync_peers,
            MmrTree::Utxo,
            self.horizon_sync_height,
            config.max_mmr_node_request_retry_attempts,
        )
        .await?;

        for indices in (local_num_utxo_nodes..remote_num_utxo_nodes)
            .collect::<Vec<u32>>()
            .chunks(config.mmr_node_or_utxo_request_size)
        {
            for attempt in 1..=config.max_sync_request_retry_attempts {
                let pos = indices.first().cloned().unwrap_or(0);
                let count = indices.len() as u32;
                let (utxo_hashes, utxo_bitmap, sync_peer1) = helpers::request_mmr_nodes(
                    LOG_TARGET,
                    self.shared,
                    self.sync_peers,
                    MmrTree::Utxo,
                    pos,
                    count,
                    self.horizon_sync_height,
                    config.max_mmr_node_request_retry_attempts,
                )
                .await?;
                let (rp_hashes, _, sync_peer2) = helpers::request_mmr_nodes(
                    LOG_TARGET,
                    self.shared,
                    self.sync_peers,
                    MmrTree::RangeProof,
                    pos,
                    count,
                    self.horizon_sync_height,
                    config.max_mmr_node_request_retry_attempts,
                )
                .await?;

                // Construct the list of hashes of the UTXOs that need to be requested.
                let mut request_utxo_hashes = Vec::new();
                let mut request_rp_hashes = Vec::new();
                let mut is_stxos = Vec::with_capacity(utxo_hashes.len());
                for index in 0..utxo_hashes.len() {
                    let deleted = utxo_bitmap.contains(index as u32 + 1);
                    is_stxos.push(deleted);
                    if !deleted {
                        request_utxo_hashes.push(utxo_hashes[index].clone());
                        request_rp_hashes.push(rp_hashes[index].clone());
                    }
                }
                // Download a partial UTXO set
                let (mut utxos, sync_peer3) = helpers::request_txos(
                    LOG_TARGET,
                    self.shared,
                    self.sync_peers,
                    &request_utxo_hashes,
                    config.max_txo_request_retry_attempts,
                )
                .await?;

                let db = &self.shared.db;
                match self.validate_utxos_and_rangeproofs(
                    &utxo_hashes,
                    &rp_hashes,
                    &request_utxo_hashes,
                    &request_rp_hashes,
                    &utxos,
                ) {
                    Ok(_) => {
                        // The order of these inserts are important to ensure the MMRs are constructed correctly and the
                        // roots match.
                        for (index, is_stxo) in is_stxos.into_iter().enumerate() {
                            if is_stxo {
                                async_db::insert_mmr_node(
                                    db.clone(),
                                    MmrTree::Utxo,
                                    utxo_hashes[index].clone(),
                                    is_stxo,
                                )
                                .await?;
                                async_db::insert_mmr_node(
                                    db.clone(),
                                    MmrTree::RangeProof,
                                    rp_hashes[index].clone(),
                                    false,
                                )
                                .await?;
                            } else {
                                // Inserting the UTXO will also insert the corresponding UTXO and RangeProof MMR Nodes.
                                async_db::insert_utxo(db.clone(), utxos.remove(0)).await?;
                            }
                        }
                        trace!(
                            target: LOG_TARGET,
                            "UTXOs and MMR nodes inserted into database: {:?}",
                            utxo_hashes
                        );

                        break;
                    },
                    Err(err @ HorizonSyncError::EmptyResponse { .. }) |
                    Err(err @ HorizonSyncError::IncorrectResponse { .. }) => {
                        warn!(
                            target: LOG_TARGET,
                            "Invalid UTXOs or MMR Nodes received from peer. {}", err
                        );
                        if (sync_peer1 == sync_peer2) && (sync_peer1 == sync_peer3) {
                            debug!(
                                target: LOG_TARGET,
                                "Banning peer {} from local node, because they supplied invalid UTXOs or MMR Nodes",
                                sync_peer1
                            );

                            self.ban_sync_peer(sync_peer1.clone()).await?;
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

    // Finalize the horizon state synchronization by setting the chain metadata to the local tip and committing the
    // horizon state to the blockchain backend.
    async fn finalize_horizon_sync(&self) -> Result<(), HorizonSyncError> {
        // TODO: Perform final validation on full synced horizon state before committing horizon state checkpoint.
        // TODO: Verify header and kernel set using Mmr Roots

        async_db::commit_horizon_state(self.db()).await?;

        Ok(())
    }

    // Validate the received UTXO set and, UTXO and RangeProofs MMR nodes.
    fn validate_utxos_and_rangeproofs(
        &self,
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

    // Validate the received UTXO set and, UTXO and RangeProofs MMR nodes.
    fn validate_utxo_hashes(
        &self,
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

    // Check the received set of kernels.
    fn validate_kernels(
        &self,
        kernel_hashes: &[HashOutput],
        kernels: &[TransactionKernel],
    ) -> Result<(), HorizonSyncError>
    {
        if kernels.is_empty() {
            return Err(HorizonSyncError::EmptyResponse);
        }
        // Check if the correct number of kernels returned
        if kernel_hashes.len() != kernels.len() {
            return Err(HorizonSyncError::IncorrectResponse);
        }

        // Check that kernel set is the requested kernels
        if kernel_hashes
            .iter()
            .enumerate()
            .any(|(i, kernel_hash)| &kernels[i].hash() != kernel_hash)
        {
            return Err(HorizonSyncError::IncorrectResponse);
        }
        Ok(())
    }

    // Check the received set of headers.
    async fn validate_and_insert_headers(
        &self,
        block_nums: &[u64],
        headers: Vec<BlockHeader>,
    ) -> Result<(), HorizonSyncError>
    {
        if headers.is_empty() {
            return Err(HorizonSyncError::EmptyResponse);
        }
        // Check that the received headers are the requested headers
        if (0..block_nums.len()).any(|i| headers[i].height != block_nums[i]) {
            return Err(HorizonSyncError::IncorrectResponse);
        }
        // Check that header set forms a sequence
        for index in 1..headers.len() {
            let prev_header = &headers[index - 1];
            let curr_header = &headers[index];
            if prev_header.height + 1 != curr_header.height {
                return Err(HorizonSyncError::InvalidHeader(format!(
                    "Headers heights are not in sequence. (Previous height: {}, Current height: {})",
                    prev_header.height, curr_header.height
                )));
            }
            if curr_header.prev_hash != prev_header.hash() {
                return Err(HorizonSyncError::InvalidHeader(
                    "Headers do not form a chain.".to_string(),
                ));
            }
        }
        // Check that the first header is linked to the chain tip header
        assert_eq!(
            headers.is_empty(),
            false,
            "validate_headers: headers.is_empty() assertion failed"
        );
        let first_header = &headers[0];
        let db = &self.shared.db;
        let tip_header = async_db::fetch_tip_header(db.clone()).await?;
        if tip_header.height + 1 != first_header.height {
            return Err(HorizonSyncError::InvalidHeader(format!(
                "Headers do not link to the current chain tip header (Tip height = {}, Received header height = {})",
                tip_header.height, first_header.height
            )));
        }
        if first_header.prev_hash != tip_header.hash() {
            return Err(HorizonSyncError::InvalidHeader(
                "Headers do not form a chain from the current tip.".to_string(),
            ));
        }

        // Validate and insert each header
        let validator = self.shared.horizon_sync_validators.header.clone();
        let db = self.db();
        spawn_blocking(move || -> Result<(), HorizonSyncError> {
            for header in headers {
                validator
                    .validate(&header)
                    .map_err(HorizonSyncError::HeaderValidationFailed)?;
                db.insert_valid_headers(vec![header])?;
            }
            Ok(())
        })
        .await
        .map_err(HorizonSyncError::JoinError)??;

        Ok(())
    }

    // Reset the chain metadata to the genesis block while in horizon sync mode. The chain metadata will be restored to
    // the latest data once the horizon sync has been finalized.
    async fn reset_chain_metadata_to_genesis(&self) -> Result<(), HorizonSyncError> {
        let genesis_header = async_db::fetch_header(self.db(), 0).await?;
        let mut metadata = async_db::get_metadata(self.db()).await?;
        metadata.height_of_longest_chain = Some(genesis_header.height);
        metadata.best_block = Some(genesis_header.hash());
        metadata.accumulated_difficulty = Some(genesis_header.achieved_difficulty());
        async_db::write_metadata(self.db(), metadata).await?;
        Ok(())
    }

    #[inline]
    fn db(&self) -> BlockchainDatabase<B> {
        self.shared.db.clone()
    }
}
