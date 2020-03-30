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
    blocks::BlockHeader,
    chain_storage::{BlockchainBackend, BlockchainDatabase, ChainStorageError},
    transactions::types::HashOutput,
};
use log::*;
use rand::{rngs::OsRng, Rng};
use std::cmp;
use tari_comms::peer_manager::NodeId;
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

const LOG_TARGET: &str = "c::bn::states::block_sync";

// The maximum number of retry attempts a node can perform to request a particular block from remote nodes.
const MAX_BLOCK_REQUEST_RETRY_ATTEMPTS: usize = 5;
const MAX_HEADER_HASHES_TO_SEND: u64 = 128;
const MAX_BLOCKS_TO_DOWNLOAD: usize = 5;

#[derive(Clone, Debug, PartialEq, Copy)]
pub struct ForwardBlockSyncInfo;

impl ForwardBlockSyncInfo {
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent
    {
        info!(target: LOG_TARGET, "Synchronizing missing blocks");

        let peers = match shared.peer_manager.flood_peers().await {
            Ok(peers) => peers,
            Err(e) => return StateEvent::FatalError(format!("Cannot get peers to sync to: {}", e)),
        };
        let sync_peers: Vec<NodeId> = peers.into_iter().map(|peer| peer.node_id).collect();
        match synchronize_blocks(shared, &sync_peers).await {
            Ok(StateEvent::BlocksSynchronized) => {
                info!(target: LOG_TARGET, "Block sync state has synchronised");
                StateEvent::BlocksSynchronized
            },
            Ok(state_event) => state_event,
            Err(e) => StateEvent::FatalError(format!("Synchronizing blocks failed. {}", e)),
        }
    }
}

async fn synchronize_blocks<B: BlockchainBackend + 'static>(
    shared: &mut BaseNodeStateMachine<B>,
    sync_nodes: &[NodeId],
) -> Result<StateEvent, String>
{
    let mut sync_nodes = Vec::from(sync_nodes);
    let tip = shared.db.fetch_tip_header().map_err(|e| e.to_string())?;
    let mut from_headers = fetch_headers_to_send::<B>(&tip, &shared.db);
    let mut sync_node = next_sync_node(&mut sync_nodes);

    loop {
        if sync_node == None {
            return Err("No more valid nodes to sync to".to_string());
        }
        let sync_node_string = sync_node.clone().unwrap();

        info!(
            target: LOG_TARGET,
            "Attempting to sync with node:{} asking for headers between heights {} and {}",
            sync_node_string,
            from_headers.last().map(|h| h.height).unwrap(),
            from_headers.first().map(|h| h.height).unwrap(),
        );
        match shared
            .comms
            .fetch_headers_between(from_headers.iter().map(|h| h.hash()).collect(), None, sync_node.clone())
            .await
        {
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Could not sync with node '{}':{}", sync_node_string, e
                );
                sync_node = next_sync_node(&mut sync_nodes);
                continue;
            },
            Ok(headers) => {
                if let Some(first_header) = headers.first() {
                    if let Ok(block) = shared.db.fetch_header_with_block_hash(first_header.prev_hash.clone()) {
                        if &shared.db.fetch_tip_header().map_err(|e| e.to_string())? != &block {
                            // If peer returns genesis block, it means that there is a split, but it is further back
                            // than the headers we sent.
                            let oldest_header_sent = from_headers.last().unwrap();
                            if block.height == 0 && oldest_header_sent.height != 1 {
                                info!(
                                    target: LOG_TARGET,
                                    "No headers from peer {} matched with the headers we sent. Retrying with older \
                                     headers",
                                    sync_node_string
                                );
                                from_headers = fetch_headers_to_send::<B>(oldest_header_sent, &shared.db);
                                continue;
                            } else {
                                info!(
                                    target: LOG_TARGET,
                                    "Chain split at height:{} according to sync peer:{}",
                                    block.height,
                                    sync_node_string
                                );
                            }
                        } else {
                            info!(
                                target: LOG_TARGET,
                                "Still on the best chain according to sync peer:{}", sync_node_string
                            );
                        }
                    } else {
                        warn!(
                            target: LOG_TARGET,
                            "Could not sync with node '{}': Block hash {} was not found in our chain. Potentially bad \
                             node or node is on a different network/genesis block",
                            sync_node_string,
                            first_header.prev_hash.to_hex()
                        );
                        sync_node = next_sync_node(&mut sync_nodes);
                        continue;
                    }
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Could not sync with node '{}': Node did not return headers", sync_node_string
                    );
                    sync_node = sync_nodes.pop().map(|n| n.clone());
                    continue;
                }

                // TODO: verify headers POW. Can't do that at present,
                // so try to add them to the chain
                let mut page = 0;

                while page < headers.len() {
                    let curr_headers: Vec<HashOutput> = headers
                        .iter()
                        .skip(page)
                        .take(MAX_BLOCKS_TO_DOWNLOAD)
                        .map(|h| h.hash())
                        .collect();

                    if curr_headers.is_empty() {
                        break;
                    }

                    let mut attempts = 0;
                    loop {
                        if download_blocks(curr_headers.clone(), shared).await? {
                            break;
                        }
                        attempts += 1;
                        if attempts > MAX_BLOCK_REQUEST_RETRY_ATTEMPTS {
                            return Err("Maximum number of block download requests exceeded".to_string());
                        }
                    }

                    page += MAX_BLOCKS_TO_DOWNLOAD;
                }

                // TODO: Blocks may not be entirely synced, need to request more
                break;
            },
        }
    }
    Ok(StateEvent::BlocksSynchronized)
}

fn next_sync_node(sync_nodes: &mut Vec<NodeId>) -> Option<NodeId> {
    if sync_nodes.is_empty() {
        return None;
    }
    let index = OsRng.gen_range(0, sync_nodes.len());
    Some(sync_nodes.remove(index))
}

fn fetch_headers_to_send<B: BlockchainBackend + 'static>(
    most_recent_header: &BlockHeader,
    db: &BlockchainDatabase<B>,
) -> Vec<BlockHeader>
{
    let mut from_headers = vec![];
    from_headers.push(most_recent_header.clone());
    for i in 1..cmp::min(most_recent_header.height, MAX_HEADER_HASHES_TO_SEND) {
        if let Ok(header) = db.fetch_header(most_recent_header.height - i) {
            from_headers.push(header)
        }
    }

    from_headers
}

async fn download_blocks<B: BlockchainBackend + 'static>(
    curr_headers: Vec<HashOutput>,
    shared: &mut BaseNodeStateMachine<B>,
) -> Result<bool, String>
{
    // Request the block from a random peer node and add to chain.
    match shared.comms.fetch_blocks_with_hashes(curr_headers.clone()).await {
        Ok(blocks) => {
            info!(target: LOG_TARGET, "Received {} blocks from peer", blocks.len());
            for i in 0..blocks.len() {
                let hist_block = &blocks[i];
                let header = &curr_headers[i];
                let block_hash = hist_block.block().hash();
                if &block_hash == header {
                    match shared.db.add_block(hist_block.block().clone()) {
                        Ok(result) => {
                            info!(
                                target: LOG_TARGET,
                                "Added block {} during sync. Result:{:?}",
                                header.to_hex(),
                                result
                            );
                        },
                        Err(ChainStorageError::InvalidBlock) => {
                            warn!(
                                target: LOG_TARGET,
                                "Invalid block {} received from peer. Retrying",
                                block_hash.to_hex(),
                            );
                            return Ok(false);
                        },
                        Err(ChainStorageError::ValidationError(_)) => {
                            warn!(
                                target: LOG_TARGET,
                                "Validation on block {} from peer failed. Retrying",
                                block_hash.to_hex(),
                            );
                            return Ok(false);
                        },
                        Err(e) => return Err(e.to_string()),
                    }
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Block at height {} from peer does not match expected hash. Expected:{} Actual:{}",
                        hist_block.block.header.height,
                        header.to_hex(),
                        block_hash.to_hex(),
                    );
                }
            }
        },
        Err(e) => {
            warn!(
                target: LOG_TARGET,
                "Failed to fetch blocks from peer:{:?}. Retrying.", e,
            );
            return Ok(false);
        },
    }
    Ok(true)
}
