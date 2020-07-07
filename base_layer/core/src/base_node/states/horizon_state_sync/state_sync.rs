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
    chain_storage::{async_db, BlockchainBackend, BlockchainDatabase, ChainMetadata, MmrTree},
    iterators::NonOverlappingIntegerPairIter,
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use croaring::Bitmap;
use log::*;
use tari_comms::peer_manager::NodeId;
use tari_crypto::tari_utilities::Hashable;
use tokio::task::spawn_blocking;

const LOG_TARGET: &str = "c::bn::states::horizon_state_sync";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HorizonStateSync {
    pub local_metadata: ChainMetadata,
    pub network_metadata: ChainMetadata,
    pub sync_peers: Vec<NodeId>,
    pub sync_height: u64,
}

impl HorizonStateSync {
    pub fn new(
        local_metadata: ChainMetadata,
        network_metadata: ChainMetadata,
        sync_peers: Vec<NodeId>,
        sync_height: u64,
    ) -> Self
    {
        Self {
            local_metadata,
            network_metadata,
            sync_peers,
            sync_height,
        }
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent
    {
        shared
            .set_status_info(StatusInfo::HorizonSync(BlockSyncInfo::new(
                self.network_metadata.height_of_longest_chain(),
                self.local_metadata.height_of_longest_chain(),
                self.sync_peers.clone(),
            )))
            .await;

        if !self.local_metadata.is_pruned_node() {
            warn!(
                target: LOG_TARGET,
                "HorizonStateSync invoked but node is not in pruned mode"
            );
            return StateEvent::HorizonStateSynchronized;
        }

        info!(
            target: LOG_TARGET,
            "Synchronizing horizon state to height {}. Network tip height is {}.",
            self.sync_height,
            self.network_metadata.height_of_longest_chain()
        );
        let local_tip_height = self.local_metadata.height_of_longest_chain();
        if local_tip_height >= self.sync_height {
            debug!(target: LOG_TARGET, "Horizon state already synchronized.");
            return StateEvent::HorizonStateSynchronized;
        }
        debug!(
            target: LOG_TARGET,
            "Horizon sync starting to height {}", self.sync_height
        );

        let mut horizon_header_sync = HorizonStateSynchronization {
            shared,
            local_metadata: &self.local_metadata,
            sync_peers: &mut self.sync_peers,
            horizon_sync_height: self.sync_height,
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
    }
}

struct HorizonStateSynchronization<'a, 'b, 'c, B> {
    shared: &'a mut BaseNodeStateMachine<B>,
    sync_peers: &'b mut Vec<NodeId>,
    local_metadata: &'c ChainMetadata,
    horizon_sync_height: u64,
}

impl<B: BlockchainBackend + 'static> HorizonStateSynchronization<'_, '_, '_, B> {
    pub async fn synchronize(&mut self) -> Result<(), HorizonSyncError> {
        let tip_header = async_db::fetch_tip_header(self.db()).await?;
        debug!(
            target: LOG_TARGET,
            "Syncing from height {} to horizon sync height {}.", tip_header.height, self.horizon_sync_height
        );

        trace!(target: LOG_TARGET, "Synchronizing kernels");
        self.synchronize_kernels().await?;
        trace!(target: LOG_TARGET, "Check the deletion state of current UTXOs");
        self.check_state_of_current_utxos().await?;
        trace!(target: LOG_TARGET, "Synchronizing UTXOs and RangeProofs");
        self.synchronize_utxos_and_rangeproofs().await?;
        trace!(target: LOG_TARGET, "Finalizing horizon synchronizing");
        self.finalize_horizon_sync().await?;

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

        let chunks = self.start_count_iter(
            local_num_kernels,
            remote_num_kernels,
            config.mmr_node_or_utxo_request_size,
        );
        for (pos, count) in chunks {
            for attempt in 1..=config.max_sync_request_retry_attempts {
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

                match self.validate_kernel_response(&kernel_hashes, &kernels) {
                    Ok(_) => {
                        let num_kernels = kernels.len();
                        async_db::insert_kernels(self.db(), kernels).await?;
                        trace!(
                            target: LOG_TARGET,
                            "{} kernels successfully added to database",
                            num_kernels
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

    // Checks if any existing UTXOs in the local database have been spent according to the remote state
    async fn check_state_of_current_utxos(&mut self) -> Result<(), HorizonSyncError> {
        let config = self.shared.config.horizon_sync_config;
        let local_tip_height = self.local_metadata.height_of_longest_chain();
        let local_num_utxo_nodes = async_db::fetch_mmr_node_count(self.db(), MmrTree::Utxo, local_tip_height).await?;

        debug!(
            target: LOG_TARGET,
            "Checking current utxo state between {} and {}", 0, local_num_utxo_nodes
        );

        let chunks = self.start_count_iter(0, local_num_utxo_nodes, config.mmr_node_or_utxo_request_size);
        for (pos, count) in chunks {
            for attempt in 1..=config.max_sync_request_retry_attempts {
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

                match self.validate_utxo_hashes_response(&remote_utxo_hashes, &local_utxo_hashes) {
                    Ok(_) => {
                        let num_hashes = local_utxo_hashes.len();
                        let mut num_deleted = 0u32;
                        for (index, utxo_hash) in local_utxo_hashes.into_iter().enumerate() {
                            let local_deleted = local_utxo_deleted.contains(pos + index as u32);
                            let remote_deleted = remote_utxo_deleted.contains(pos + index as u32);
                            if remote_deleted && !local_deleted {
                                async_db::delete_mmr_node(self.db(), MmrTree::Utxo, utxo_hash).await?;
                                num_deleted += 1;
                            }
                        }
                        debug!(
                            target: LOG_TARGET,
                            "Checked {} existing UTXO(s). {} were spent.", num_hashes, num_deleted
                        );

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

    // Synchronize UTXO MMR Nodes, RangeProof MMR Nodes and the UTXO set upto the horizon sync height from
    // remote sync peers.
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
        debug!(
            target: LOG_TARGET,
            "Synchronizing UTXO MMR nodes from {} to {}", local_num_utxo_nodes, remote_num_utxo_nodes
        );

        let chunks = self.start_count_iter(
            local_num_utxo_nodes,
            remote_num_utxo_nodes,
            config.mmr_node_or_utxo_request_size,
        );
        for (pos, count) in chunks {
            for attempt in 1..=config.max_sync_request_retry_attempts {
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
                    let deleted = utxo_bitmap.contains(pos + index as u32);
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

                debug!(
                    target: LOG_TARGET,
                    "Fetched {} UTXOs and {} STXOs",
                    utxos.len(),
                    is_stxos.iter().filter(|x| **x).count()
                );

                let db = &self.shared.db;
                match self.validate_utxo_and_rangeproof_response(
                    &utxo_hashes,
                    &rp_hashes,
                    &request_utxo_hashes,
                    &request_rp_hashes,
                    &utxos,
                ) {
                    Ok(_) => {
                        // The order of these inserts are important to ensure the MMRs are constructed correctly
                        // and the roots match.
                        for (index, is_stxo) in is_stxos.into_iter().enumerate() {
                            if is_stxo {
                                async_db::insert_mmr_node(db.clone(), MmrTree::Utxo, utxo_hashes[index].clone(), true)
                                    .await?;
                                async_db::insert_mmr_node(
                                    db.clone(),
                                    MmrTree::RangeProof,
                                    rp_hashes[index].clone(),
                                    false,
                                )
                                .await?;
                            } else {
                                // Inserting the UTXO will also insert the corresponding UTXO and RangeProof MMR
                                // Nodes.
                                async_db::insert_utxo(db.clone(), utxos.remove(0)).await?;
                            }
                        }
                        trace!(
                            target: LOG_TARGET,
                            "{} UTXOs with MMR nodes inserted into database",
                            utxo_hashes.len()
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

    // Finalize the horizon state synchronization by setting the chain metadata to the local tip and committing
    // the horizon state to the blockchain backend.
    async fn finalize_horizon_sync(&self) -> Result<(), HorizonSyncError> {
        let validator = self.shared.sync_validators.final_state.clone();
        let horizon_sync_height = self.horizon_sync_height;
        let validation_result = spawn_blocking(move || {
            validator
                .validate(&horizon_sync_height)
                .map_err(HorizonSyncError::FinalStateValidationFailed)
        })
        .await?;

        match validation_result {
            Ok(_) => {
                debug!(
                    target: LOG_TARGET,
                    "Horizon state validation succeeded! Committing horizon state."
                );
                async_db::commit_horizon_state(self.db()).await?;
                Ok(())
            },
            Err(err) => {
                debug!(target: LOG_TARGET, "Horizon state validation failed! Resetting state.");

                if let Err(err) = self.reset_state().await {
                    error!(
                        target: LOG_TARGET,
                        "Failed to reset horizon state to height {}: {}. **The blockchain is in an invalid state**",
                        0,
                        err
                    );
                }
                Err(err)
            },
        }
    }

    async fn reset_state(&self) -> Result<(), HorizonSyncError> {
        error!(
            target: LOG_TARGET,
            "Horizon state sync has failed and the database is in an inconsistent state. Reverting the database to \
             the last consistent state is not yet implemented."
        );
        // TODO: Revert the db back to the last good state, something like
        // async_db::revert_horizon_state(
        //     self.db(),
        //     self.good_state.utxo_node_count,
        //     self.good_state.kernel_node_count,
        //     self.deleted_utxos // (bitmap)
        // );
        Ok(())
    }

    // Validate the received UTXO set and, UTXO and RangeProofs MMR nodes.
    fn validate_utxo_and_rangeproof_response(
        &self,
        utxo_hashes: &[HashOutput],
        rp_hashes: &[HashOutput],
        request_utxo_hashes: &[HashOutput],
        request_rp_hashes: &[HashOutput],
        utxos: &[TransactionOutput],
    ) -> Result<(), HorizonSyncError>
    {
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
    fn validate_utxo_hashes_response(
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
    fn validate_kernel_response(
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

        for k in kernels {
            k.verify_signature()?;
        }

        Ok(())
    }

    fn start_count_iter(&self, start: u32, end: u32, chunk_size: usize) -> impl Iterator<Item = (u32, u32)> {
        NonOverlappingIntegerPairIter::new(start, end, chunk_size)
                    // Convert (start, end) into (start, count)
                    .map(|(pos, end)| (pos, end - pos + 1))
    }

    #[inline]
    fn db(&self) -> BlockchainDatabase<B> {
        self.shared.db.clone()
    }
}
