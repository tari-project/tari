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
        chain_metadata_service::{ChainMetadataEvent, PeerChainMetadata},
        states::{
            helpers::{best_metadata, determine_sync_mode},
            StateEvent,
            StateEvent::FatalError,
            SyncStatus,
        },
        BaseNodeStateMachine,
    },
    chain_storage::{BlockchainBackend, ChainMetadata},
};
use futures::stream::StreamExt;
use log::*;
use std::collections::VecDeque;
use tari_comms::peer_manager::NodeId;

const LOG_TARGET: &str = "c::bn::states::listening";

// The number of liveness rounds that need to be included when determining the best network tip.
const METADATA_LIVENESS_ROUNDS: usize = 1;

/// Configuration for the Listening state.
#[derive(Clone, Copy, Debug)]
pub struct ListeningConfig {
    pub metadata_liveness_rounds: usize,
}

impl Default for ListeningConfig {
    fn default() -> Self {
        Self {
            metadata_liveness_rounds: METADATA_LIVENESS_ROUNDS,
        }
    }
}

/// This state listens for chain metadata events received from the liveness and chain metadata service. Based on the
/// received metadata, if it detects that the current node is lagging behind the network it will switch to block sync
/// state.
#[derive(Clone, Debug, PartialEq)]
pub struct ListeningInfo;

impl ListeningInfo {
    pub async fn next_event<B: BlockchainBackend>(&mut self, shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        info!(target: LOG_TARGET, "Listening for chain metadata updates");

        let mut metadata_rounds = VecDeque::<Vec<PeerChainMetadata>>::new();
        let mut metadata_event_stream = shared.metadata_event_stream.clone().fuse();
        loop {
            futures::select! {
                metadata_event = metadata_event_stream.select_next_some() => {
                    if let ChainMetadataEvent::PeerChainMetadataReceived(chain_metadata_list) = &*metadata_event {
                        if !chain_metadata_list.is_empty() {
                            // Update the metadata queue
                            if metadata_rounds.len()>=shared.config.listening_config.metadata_liveness_rounds {
                                metadata_rounds.pop_front();
                            }
                            metadata_rounds.push_back(chain_metadata_list.clone());

                            if metadata_rounds.len()==shared.config.listening_config.metadata_liveness_rounds {
                                info!(target: LOG_TARGET, "Loading local blockchain metadata.");
                                let local = match shared.db.get_metadata() {
                                    Ok(m) => m,
                                    Err(e) => {
                                        let msg = format!("Could not get local blockchain metadata. {}", e.to_string());
                                        return FatalError(msg);
                                    },
                                };
                                // Find the best network metadata and set of sync peers with the best tip.
                                let metadata_list = metadata_rounds.iter().flatten()
                                                    .map(|peer_metadata| peer_metadata.chain_metadata.clone())
                                                    .collect::<Vec<_>>();
                                let best_metadata=best_metadata(metadata_list);
                                let sync_peers=find_sync_peers(&best_metadata,&metadata_rounds);
                                if let SyncStatus::Lagging(network_tip,sync_peers) = determine_sync_mode(&local, best_metadata, sync_peers,LOG_TARGET) {
                                    return StateEvent::FallenBehind(SyncStatus::Lagging(network_tip,sync_peers));
                                }
                            }
                        }
                    }
                },

                complete => {
                    debug!(target: LOG_TARGET, "Event listener is complete because liveness metadata and timeout streams were closed");
                    return StateEvent::UserQuit;
                }
            }
        }
    }
}

// Finds the set of sync peers that have the best tip on their main chain.
fn find_sync_peers(best_metadata: &ChainMetadata, metadata_rounds: &VecDeque<Vec<PeerChainMetadata>>) -> Vec<NodeId> {
    let mut sync_peers = Vec::<NodeId>::new();
    for peer_metadata in metadata_rounds.iter().flatten() {
        if peer_metadata.chain_metadata == *best_metadata {
            sync_peers.push(peer_metadata.node_id.clone());
        }
    }
    sync_peers
}
