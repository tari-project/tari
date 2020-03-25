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
use tari_comms::peer_manager::NodeId;

const LOG_TARGET: &str = "c::bn::states::listening";

/// This state listens for chain metadata events received from the liveness and chain metadata service. Based on the
/// received metadata, if it detects that the current node is lagging behind the network it will switch to block sync
/// state.
#[derive(Clone, Debug, PartialEq)]
pub struct ListeningInfo;

impl ListeningInfo {
    pub async fn next_event<B: BlockchainBackend>(&mut self, shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        info!(target: LOG_TARGET, "Listening for chain metadata updates");

        let mut metadata_event_stream = shared.metadata_event_stream.clone().fuse();
        while let Some(metadata_event) = metadata_event_stream.next().await {
            match &*metadata_event {
                ChainMetadataEvent::PeerChainMetadataReceived(peer_metadata_list) => {
                    if !peer_metadata_list.is_empty() {
                        info!(target: LOG_TARGET, "Loading local blockchain metadata.");
                        let local = match shared.db.get_metadata() {
                            Ok(m) => m,
                            Err(e) => {
                                let msg = format!("Could not get local blockchain metadata. {}", e.to_string());
                                return FatalError(msg);
                            },
                        };
                        // Find the best network metadata and set of sync peers with the best tip.
                        let metadata_list = peer_metadata_list
                            .iter()
                            .map(|peer_metadata| peer_metadata.chain_metadata.clone())
                            .collect::<Vec<_>>();
                        let best_metadata = best_metadata(metadata_list);
                        let sync_peers = find_sync_peers(&best_metadata, &peer_metadata_list);
                        if let SyncStatus::Lagging(network_tip, sync_peers) =
                            determine_sync_mode(&local, best_metadata, sync_peers, LOG_TARGET)
                        {
                            return StateEvent::FallenBehind(SyncStatus::Lagging(network_tip, sync_peers));
                        }
                    }
                },
            }
        }

        debug!(
            target: LOG_TARGET,
            "Event listener is complete because liveness metadata and timeout streams were closed"
        );
        StateEvent::UserQuit
    }
}

// Finds the set of sync peers that have the best tip on their main chain.
fn find_sync_peers(best_metadata: &ChainMetadata, peer_metadata_list: &Vec<PeerChainMetadata>) -> Vec<NodeId> {
    let mut sync_peers = Vec::<NodeId>::new();
    for peer_metadata in peer_metadata_list {
        if peer_metadata.chain_metadata == *best_metadata {
            sync_peers.push(peer_metadata.node_id.clone());
        }
    }
    sync_peers
}
