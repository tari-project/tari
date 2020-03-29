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
        states::{StateEvent, StateEvent::FatalError, SyncStatus},
        BaseNodeStateMachine,
    },
    chain_storage::{BlockchainBackend, ChainMetadata},
    proof_of_work::Difficulty,
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
        while let Some(metadata_event) = shared.metadata_event_stream.next().await {
            match &*metadata_event {
                ChainMetadataEvent::PeerChainMetadataReceived(ref peer_metadata_list) => {
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
                        let best_metadata = best_metadata(peer_metadata_list.as_slice());
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

/// Determine the best metadata from a set of metadata received from the network.
fn best_metadata(metadata_list: &[PeerChainMetadata]) -> ChainMetadata {
    // TODO: Use heuristics to weed out outliers / dishonest nodes.
    metadata_list
        .into_iter()
        .fold(ChainMetadata::default(), |best, current| {
            if current
                .chain_metadata
                .accumulated_difficulty
                .unwrap_or(Difficulty::min()) >=
                best.accumulated_difficulty.unwrap_or_else(|| 0.into())
            {
                current.chain_metadata.clone()
            } else {
                best
            }
        })
}

/// Given a local and the network chain state respectively, figure out what synchronisation state we should be in.
fn determine_sync_mode(
    local: &ChainMetadata,
    network: ChainMetadata,
    sync_peers: Vec<NodeId>,
    log_target: &str,
) -> SyncStatus
{
    use crate::base_node::states::SyncStatus::*;
    match network.accumulated_difficulty {
        None => {
            info!(
                target: log_target,
                "The rest of the network doesn't appear to have any up-to-date chain data, so we're going to assume \
                 we're at the tip"
            );
            UpToDate
        },
        Some(network_tip_accum_difficulty) => {
            let local_tip_accum_difficulty = local.accumulated_difficulty.unwrap_or_else(|| 0.into());
            if local_tip_accum_difficulty < network_tip_accum_difficulty {
                info!(
                    target: log_target,
                    "Our local blockchain accumulated difficulty is a little behind that of the network. We're at \
                     block #{} with an accumulated difficulty of {}, and the network chain tip is at #{} with an \
                     accumulated difficulty of {}",
                    local.height_of_longest_chain.unwrap_or(0),
                    local_tip_accum_difficulty,
                    network.height_of_longest_chain.unwrap_or(0),
                    network_tip_accum_difficulty,
                );
                Lagging(network, sync_peers)
            } else {
                info!(
                    target: log_target,
                    "Our local blockchain is up-to-date. We're at block #{} with an accumulated difficulty of {} and \
                     the network chain tip is at #{} with an accumulated difficulty of {}",
                    local.height_of_longest_chain.unwrap_or(0),
                    local_tip_accum_difficulty,
                    network.height_of_longest_chain.unwrap_or(0),
                    network_tip_accum_difficulty,
                );
                UpToDate
            }
        },
    }
}
