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
    base_node::{states::StateEvent, BaseNodeStateMachine},
    chain_storage::{BlockchainBackend, DbTransaction, MmrTree, MutableMmrState},
};
use croaring::Bitmap;
use log::*;
use tari_mmr::MutableMmrLeafNodes;
use tari_transactions::types::HashOutput;

const LOG_TARGET: &str = "base_node::fetching_horizon_state";

// TODO: Find better chunk sizes to limit the maximum size of the response messages
// The number of MMR leaf nodes that can be requested in a single query from remote nodes.
const HORIZON_SYNC_CHUNK_SIZE_LEAF_NODES: usize = 1000;
// The number of headers that can be requested in a single query from remote nodes.
const HORIZON_SYNC_CHUNK_SIZE_HEADERS: usize = 1000;
// The number of kernels that can be requested in a single query from remote nodes.
const HORIZON_SYNC_CHUNK_SIZE_KERNELS: usize = 1000;
// The number of utxos that can be requested in a single query from remote nodes.
const HORIZON_SYNC_CHUNK_SIZE_UTXOS: usize = 1000;

/// Configuration for the Horizon Synchronization.
#[derive(Clone, Copy)]
pub struct HorizonSyncConfig {
    pub leaf_nodes_sync_chunk_size: usize,
    pub headers_sync_chunk_size: usize,
    pub kernels_sync_chunk_size: usize,
    pub utxos_sync_chunk_size: usize,
}

impl Default for HorizonSyncConfig {
    fn default() -> Self {
        Self {
            leaf_nodes_sync_chunk_size: HORIZON_SYNC_CHUNK_SIZE_LEAF_NODES,
            headers_sync_chunk_size: HORIZON_SYNC_CHUNK_SIZE_HEADERS,
            kernels_sync_chunk_size: HORIZON_SYNC_CHUNK_SIZE_KERNELS,
            utxos_sync_chunk_size: HORIZON_SYNC_CHUNK_SIZE_UTXOS,
        }
    }
}

/// Local state used when synchronizing the node to the pruning horizon.
pub struct HorizonInfo {
    /// The block that we've synchronised to when exiting this state
    horizon_block: u64,
}

impl HorizonInfo {
    pub fn new(horizon_block: u64) -> Self {
        HorizonInfo { horizon_block }
    }

    pub async fn next_event<B: BlockchainBackend>(&mut self, shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        info!(
            target: LOG_TARGET,
            "Starting horizon synchronisation at block {}", self.horizon_block
        );

        info!(
            target: LOG_TARGET,
            "Synchronising kernel merkle mountain range to pruning horizon."
        );
        if let Err(e) = self.synchronize_kernel_mmr(shared).await {
            return StateEvent::FatalError(format!("Synchronizing kernel MMR failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising range proof MMR to pruning horizon.");
        if let Err(e) = self.synchronize_range_proof_mmr(shared).await {
            return StateEvent::FatalError(format!("Synchronizing range proof MMR failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising TXO MMR to pruning horizon.");
        if let Err(e) = self.synchronize_output_mmr(shared).await {
            return StateEvent::FatalError(format!("Synchronizing output MMR failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising headers to pruning horizon.");
        if let Err(e) = self.synchronize_headers(shared).await {
            return StateEvent::FatalError(format!("Synchronizing block headers failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising kernels to pruning horizon.");
        if let Err(e) = self.synchronize_kernels(shared).await {
            return StateEvent::FatalError(format!("Synchronizing kernels failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising UTXO set at pruning horizon.");
        if let Err(e) = self.synchronize_utxo_set(shared).await {
            return StateEvent::FatalError(format!("Synchronizing UTXO set failed. {}", e));
        }

        info!(target: LOG_TARGET, "Validating downloaded horizon state.");
        // TODO #1147: Attempt to recover from failed horizon state validation.
        if let Err(e) = self.validate_horizon_state(shared).await {
            return StateEvent::FatalError(format!("Validation of downloaded horizon state failed. {}", e));
        }

        info!(target: LOG_TARGET, "Pruning horizon state has synchronised");
        StateEvent::HorizonStateFetched
    }

    // Retrieve the full base mmr state for the specified MmrTree by performing multiple queries and reassembling the
    // full state from the received chunks.
    async fn download_mmr_base_state<B: BlockchainBackend>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
        tree: MmrTree,
    ) -> Result<MutableMmrLeafNodes, String>
    {
        let leaf_nodes_sync_chunk_size = shared.config.horizon_sync_config.leaf_nodes_sync_chunk_size as u64;
        let mut index = 0;
        let mut complete_leaf_count = leaf_nodes_sync_chunk_size;
        let mut base_state = MutableMmrLeafNodes::new(Vec::new(), Bitmap::create());
        while index < complete_leaf_count {
            let MutableMmrState {
                total_leaf_count,
                leaf_nodes,
            } = shared
                .comms
                .fetch_mmr_state(tree.clone(), index, leaf_nodes_sync_chunk_size)
                .await
                .map_err(|e| e.to_string())?;
            base_state.combine(leaf_nodes);
            complete_leaf_count = total_leaf_count as u64;
            index += leaf_nodes_sync_chunk_size;
        }
        Ok(base_state)
    }

    async fn synchronize_headers<B: BlockchainBackend>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> Result<(), String>
    {
        let height_indices = (0..=self.horizon_block).collect::<Vec<u64>>();
        for block_nums in height_indices.chunks(shared.config.horizon_sync_config.headers_sync_chunk_size) {
            let headers = shared
                .comms
                .fetch_headers(block_nums.to_vec())
                .await
                .map_err(|e| e.to_string())?;

            let mut txn = DbTransaction::new();
            headers.into_iter().for_each(|header| txn.insert_header(header));
            shared.db.commit(txn).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    async fn synchronize_kernels<B: BlockchainBackend>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> Result<(), String>
    {
        let mmr_state = shared
            .db
            .fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 1)
            .map_err(|e| e.to_string())?;
        let kernel_hashes = shared
            .db
            .fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, mmr_state.total_leaf_count)
            .map_err(|e| e.to_string())?
            .leaf_nodes
            .leaf_hashes;

        for hashes in kernel_hashes.chunks(shared.config.horizon_sync_config.kernels_sync_chunk_size) {
            let kernels = shared
                .comms
                .fetch_kernels(hashes.to_vec())
                .await
                .map_err(|e| e.to_string())?;

            let mut txn = DbTransaction::new();
            kernels.into_iter().for_each(|kernel| txn.insert_kernel(kernel, false));
            shared.db.commit(txn).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    async fn synchronize_utxo_set<B: BlockchainBackend>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> Result<(), String>
    {
        let total_leaf_count = shared
            .db
            .fetch_mmr_base_leaf_node_count(MmrTree::Utxo)
            .map_err(|e| e.to_string())?;

        for index in (0..total_leaf_count).step_by(shared.config.horizon_sync_config.utxos_sync_chunk_size) {
            let MutableMmrLeafNodes { leaf_hashes, deleted } = shared
                .db
                .fetch_mmr_base_leaf_nodes(
                    MmrTree::Utxo,
                    index,
                    shared.config.horizon_sync_config.utxos_sync_chunk_size,
                )
                .map_err(|e| e.to_string())?
                .leaf_nodes;
            let leaf_hashes: Vec<HashOutput> = leaf_hashes
                .into_iter()
                .enumerate()
                .filter(|(local_index, _h)| !deleted.contains((*local_index + index) as u32))
                .map(|(_, h)| h)
                .collect();

            let utxos = shared.comms.fetch_utxos(leaf_hashes).await.map_err(|e| e.to_string())?;

            let mut txn = DbTransaction::new();
            utxos.into_iter().for_each(|utxo| txn.insert_utxo(utxo, false));
            shared.db.commit(txn).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    async fn synchronize_kernel_mmr<B: BlockchainBackend>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> Result<(), String>
    {
        let base_state = self.download_mmr_base_state(shared, MmrTree::Kernel).await?;
        shared
            .db
            .assign_mmr(MmrTree::Kernel, base_state)
            .map_err(|e| e.to_string())
    }

    async fn synchronize_range_proof_mmr<B: BlockchainBackend>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> Result<(), String>
    {
        let base_state = self.download_mmr_base_state(shared, MmrTree::RangeProof).await?;
        shared
            .db
            .assign_mmr(MmrTree::RangeProof, base_state)
            .map_err(|e| e.to_string())
    }

    async fn synchronize_output_mmr<B: BlockchainBackend>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> Result<(), String>
    {
        let base_state = self.download_mmr_base_state(shared, MmrTree::Utxo).await?;
        shared
            .db
            .assign_mmr(MmrTree::Utxo, base_state)
            .map_err(|e| e.to_string())
    }

    async fn validate_horizon_state<B: BlockchainBackend>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> Result<(), String>
    {
        shared.db.validate_horizon_state().map_err(|e| e.to_string())
    }
}
