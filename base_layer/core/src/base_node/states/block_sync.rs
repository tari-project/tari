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
        states::{InitialSync, ListeningInfo, StateEvent},
    },
    chain_storage::{BlockchainBackend, ChainMetadata},
};
use log::*;

const LOG_TARGET: &str = "c::bn::states::block_sync";

// The number of Blocks that can be requested in a single query from remote nodes.
const BLOCK_SYNC_CHUNK_SIZE: usize = 2;

/// Configuration for the Block Synchronization.
#[derive(Clone, Copy)]
pub struct BlockSyncConfig {
    pub block_sync_chunk_size: usize,
}

impl Default for BlockSyncConfig {
    fn default() -> Self {
        Self {
            block_sync_chunk_size: BLOCK_SYNC_CHUNK_SIZE,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockSyncInfo;

impl BlockSyncInfo {
    pub async fn next_event<B: BlockchainBackend>(&mut self, shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        info!(target: LOG_TARGET, "Synchronizing missing blocks");

        if let Err(e) = synchronize_blocks(shared).await {
            error!(
                target: LOG_TARGET,
                "Block sync state has failed with the following error {:?}.", e
            );
            return StateEvent::FatalError(format!("Synchronizing blocks failed. {}", e));
        }

        info!(target: LOG_TARGET, "Block sync state has synchronised");
        StateEvent::BlocksSynchronized
    }
}

/// State management for Listening -> BlockSync. This change happens when a node has been temporarily disconnected
/// from the network, or a reorg has occurred.
impl From<ListeningInfo> for BlockSyncInfo {
    fn from(_old: ListeningInfo) -> Self {
        BlockSyncInfo {}
    }
}

/// State management for InitialSync -> BlockSync. This change happens when a (previously synced) node is restarted
/// after being offline for some time.
impl From<InitialSync> for BlockSyncInfo {
    fn from(_old: InitialSync) -> Self {
        BlockSyncInfo {}
    }
}

async fn network_chain_tip<B: BlockchainBackend>(shared: &mut BaseNodeStateMachine<B>) -> Result<u64, String> {
    let metadata_list = shared.comms.get_metadata().await.map_err(|e| e.to_string())?;
    // TODO: Use heuristics to weed out outliers / dishonest nodes.
    Ok(metadata_list
        .into_iter()
        .fold(ChainMetadata::default(), |best, current| {
            if current.height_of_longest_chain.unwrap_or(0) >= best.height_of_longest_chain.unwrap_or(0) {
                current
            } else {
                best
            }
        })
        .height_of_longest_chain
        .unwrap_or(0))
}

async fn synchronize_blocks<B: BlockchainBackend>(shared: &mut BaseNodeStateMachine<B>) -> Result<(), String> {
    let start_height = match shared.db.get_height().map_err(|e| e.to_string())? {
        Some(height) => height + 1,
        None => 0u64,
    };
    let network_tip_height = network_chain_tip(shared).await?;

    let height_indices = (start_height..=network_tip_height).collect::<Vec<u64>>();
    for block_nums in height_indices.chunks(shared.config.block_sync_config.block_sync_chunk_size) {
        debug!(
            target: LOG_TARGET,
            "Requesting blocks {}..{} from peers",
            block_nums[0],
            block_nums[block_nums.len() - 1]
        );
        let hist_blocks = shared
            .comms
            .fetch_blocks(block_nums.to_vec())
            .await
            .map_err(|e| e.to_string())?;
        debug!(target: LOG_TARGET, "Received {} blocks from peer", hist_blocks.len());
        for hist_block in hist_blocks {
            shared.db.add_block(hist_block.block).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}
