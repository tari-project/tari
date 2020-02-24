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
    chain_storage::{BlockchainBackend, ChainMetadata, ChainStorageError},
};
use log::*;
use std::collections::VecDeque;
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

const LOG_TARGET: &str = "c::bn::states::block_sync";

// The maximum number of retry attempts a node can perform to request a particular block from remote nodes.
const MAX_BLOCK_REQUEST_RETRY_ATTEMPTS: usize = 5;

/// Configuration for the Block Synchronization.
#[derive(Clone, Copy)]
pub struct BlockSyncConfig {
    pub max_block_request_retry_attempts: usize,
}

impl Default for BlockSyncConfig {
    fn default() -> Self {
        Self {
            max_block_request_retry_attempts: MAX_BLOCK_REQUEST_RETRY_ATTEMPTS,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockSyncInfo;

impl BlockSyncInfo {
    pub async fn next_event<B: BlockchainBackend>(&mut self, shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        info!(target: LOG_TARGET, "Synchronizing missing blocks");

        match synchronize_blocks(shared).await {
            Ok(StateEvent::BlocksSynchronized) => {
                info!(target: LOG_TARGET, "Block sync state has synchronised");
                StateEvent::BlocksSynchronized
            },
            Ok(StateEvent::MaxRequestAttemptsReached) => {
                warn!(
                    target: LOG_TARGET,
                    "Maximum unsuccessful block request attempts reached"
                );
                StateEvent::MaxRequestAttemptsReached
            },
            Ok(state_event) => state_event,
            Err(e) => StateEvent::FatalError(format!("Synchronizing blocks failed. {}", e)),
        }
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

async fn synchronize_blocks<B: BlockchainBackend>(shared: &mut BaseNodeStateMachine<B>) -> Result<StateEvent, String> {
    let start_height = match shared.db.get_height().map_err(|e| e.to_string())? {
        Some(height) => height + 1,
        None => 0u64,
    };
    let network_tip_height = network_chain_tip(shared).await?;

    let mut attempts = 0;
    let mut height_indices = (start_height..=network_tip_height).collect::<VecDeque<u64>>();
    while !height_indices.is_empty() {
        if let Some(block_num) = height_indices.pop_front() {
            debug!(target: LOG_TARGET, "Requesting block {} from peers", block_num,);

            // Request the block from a random peer node and add to chain.
            match shared.comms.fetch_blocks(vec![block_num]).await {
                Ok(hist_block) => {
                    debug!(target: LOG_TARGET, "Received {} blocks from peer", hist_block.len());
                    if let Some(hist_block) = hist_block.first() {
                        match shared.db.add_block(hist_block.block.clone()) {
                            Ok(_) => {
                                attempts = 0;
                                continue;
                            },
                            Err(ChainStorageError::InvalidBlock) => {
                                warn!(
                                    target: LOG_TARGET,
                                    "Invalid block {} received from peer. Retrying",
                                    hist_block.block.hash().to_hex(),
                                );
                            },
                            Err(ChainStorageError::ValidationError(_)) => {
                                warn!(
                                    target: LOG_TARGET,
                                    "Validation on block {} from peer failed. Retrying",
                                    hist_block.block.hash().to_hex(),
                                );
                            },
                            Err(e) => return Err(e.to_string()),
                        }
                    }
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to fetch blocks from peer:{:?}. Retrying.", e,
                    );
                },
            }
            // Attempt again to retrieve the correct block
            attempts += 1;
            if attempts >= shared.config.block_sync_config.max_block_request_retry_attempts {
                return Ok(StateEvent::MaxRequestAttemptsReached);
            }
            height_indices.push_front(block_num);
        }
    }

    Ok(StateEvent::BlocksSynchronized)
}
