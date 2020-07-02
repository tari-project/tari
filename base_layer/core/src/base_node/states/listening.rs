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
        states::{StateEvent, StateEvent::FatalError, StatusInfo, SyncStatus},
        BaseNodeStateMachine,
    },
    chain_storage::{async_db, BlockchainBackend, ChainMetadata},
    proof_of_work::Difficulty,
};
use futures::stream::StreamExt;
use log::*;
use std::fmt::{Display, Formatter};
use tari_comms::peer_manager::NodeId;

const LOG_TARGET: &str = "c::bn::states::listening";

#[derive(Clone, Copy, Debug, PartialEq, Default)]
/// This struct contains info that is use full for external viewing of state info
pub struct ListeningInfo {}

impl Display for ListeningInfo {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str("Node in listening state\n")
    }
}

impl ListeningInfo {
    /// Creates a new ListeningData
    pub fn new() -> ListeningInfo {
        // todo fill in with good info
        ListeningInfo {}
    }
}

/// This state listens for chain metadata events received from the liveness and chain metadata service. Based on the
/// received metadata, if it detects that the current node is lagging behind the network it will switch to block sync
/// state.
#[derive(Clone, Debug, PartialEq)]
pub struct ListeningData;

impl ListeningData {
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent
    {
        info!(target: LOG_TARGET, "Listening for chain metadata updates");
        shared.info = StatusInfo::Listening(ListeningInfo::new());
        shared.publish_event_info().await;
        while let Some(metadata_event) = shared.metadata_event_stream.next().await {
            match &*metadata_event {
                ChainMetadataEvent::PeerChainMetadataReceived(ref peer_metadata_list) => {
                    if !peer_metadata_list.is_empty() {
                        debug!(target: LOG_TARGET, "Loading local blockchain metadata.");
                        let local = match async_db::get_metadata(shared.db.clone()).await {
                            Ok(m) => m,
                            Err(e) => {
                                let msg = format!("Could not get local blockchain metadata. {}", e.to_string());
                                return FatalError(msg);
                            },
                        };
                        // Find the best network metadata and set of sync peers with the best tip.
                        let best_metadata = best_metadata(peer_metadata_list.as_slice());
                        let local_tip_height = local.height_of_longest_chain.unwrap_or(0);
                        let sync_peers = select_sync_peers(local_tip_height, &best_metadata, &peer_metadata_list);

                        let sync_mode = determine_sync_mode(&local, best_metadata, sync_peers);
                        if sync_mode.is_lagging() {
                            return StateEvent::FallenBehind(sync_mode);
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

// Finds the set of sync peers that have the best tip on their main chain and have all the data required to update the
// local node.
fn select_sync_peers(
    local_tip_height: u64,
    best_metadata: &ChainMetadata,
    peer_metadata_list: &[PeerChainMetadata],
) -> Vec<NodeId>
{
    let mut sync_peers = Vec::<NodeId>::new();
    for peer_metadata in peer_metadata_list {
        let peer_tip_height = peer_metadata.chain_metadata.height_of_longest_chain;
        let peer_horizon_height = peer_metadata.chain_metadata.horizon_block(peer_tip_height.unwrap_or(0));
        if (peer_horizon_height <= local_tip_height) &&
            (peer_metadata.chain_metadata.best_block == best_metadata.best_block)
        {
            sync_peers.push(peer_metadata.node_id.clone());
        }
    }
    sync_peers
}

/// Determine the best metadata from a set of metadata received from the network.
fn best_metadata(metadata_list: &[PeerChainMetadata]) -> ChainMetadata {
    // TODO: Use heuristics to weed out outliers / dishonest nodes.
    metadata_list.iter().fold(ChainMetadata::default(), |best, current| {
        if current
            .chain_metadata
            .accumulated_difficulty
            .unwrap_or_else(Difficulty::min) >=
            best.accumulated_difficulty.unwrap_or_else(|| 0.into())
        {
            current.chain_metadata.clone()
        } else {
            best
        }
    })
}

/// Given a local and the network chain state respectively, figure out what synchronisation state we should be in.
fn determine_sync_mode(local: &ChainMetadata, network: ChainMetadata, sync_peers: Vec<NodeId>) -> SyncStatus {
    use crate::base_node::states::SyncStatus::*;
    match network.accumulated_difficulty {
        None => {
            info!(
                target: LOG_TARGET,
                "The rest of the network doesn't appear to have any up-to-date chain data, so we're going to assume \
                 we're at the tip"
            );
            UpToDate
        },
        Some(network_tip_accum_difficulty) => {
            let local_tip_accum_difficulty = local.accumulated_difficulty.unwrap_or_else(|| 0.into());
            if local_tip_accum_difficulty < network_tip_accum_difficulty {
                let local_tip_height = local.height_of_longest_chain.unwrap_or(0);
                let network_tip_height = network.height_of_longest_chain.unwrap_or(0);
                info!(
                    target: LOG_TARGET,
                    "Our local blockchain accumulated difficulty is a little behind that of the network. We're at \
                     block #{} with an accumulated difficulty of {}, and the network chain tip is at #{} with an \
                     accumulated difficulty of {}",
                    local_tip_height,
                    local_tip_accum_difficulty,
                    network_tip_height,
                    network_tip_accum_difficulty,
                );

                let network_horizon_block = local.horizon_block(network_tip_height);
                if local_tip_height < network_horizon_block {
                    LaggingBehindHorizon(network, sync_peers)
                } else {
                    Lagging(network, sync_peers)
                }
            } else {
                info!(
                    target: LOG_TARGET,
                    "Our blockchain is up-to-date. We're at block {} with an accumulated difficulty of {} and the \
                     network chain tip is at {} with an accumulated difficulty of {}",
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::blocks::BlockHash;
    use rand::rngs::OsRng;
    use tari_comms::types::CommsPublicKey;
    use tari_crypto::keys::PublicKey;

    fn random_node_id() -> NodeId {
        let (_secret_key, public_key) = CommsPublicKey::random_keypair(&mut OsRng);
        NodeId::from_key(&public_key).unwrap()
    }

    #[test]
    fn sync_peer_selection() {
        let local_tip_height: u64 = 4000;
        let network_tip_height = 5000;
        let block_hash1: BlockHash = vec![0, 1, 2, 3];
        let block_hash2: BlockHash = vec![4, 5, 6, 7];
        let accumulated_difficulty1 = Difficulty::from(200000);
        let accumulated_difficulty2 = Difficulty::from(100000);

        let mut peer_metadata_list = Vec::<PeerChainMetadata>::new();
        let best_network_metadata = best_metadata(peer_metadata_list.as_slice());
        assert_eq!(best_network_metadata, ChainMetadata::default());
        let sync_peers = select_sync_peers(local_tip_height, &best_network_metadata, &peer_metadata_list);
        assert_eq!(sync_peers.len(), 0);

        let node_id1 = random_node_id();
        let node_id2 = random_node_id();
        let node_id3 = random_node_id();
        let node_id4 = random_node_id();
        let node_id5 = random_node_id();
        let peer1 = PeerChainMetadata::new(
            node_id1.clone(),
            ChainMetadata::new(network_tip_height, block_hash1.clone(), 0, accumulated_difficulty1),
        ); // Archival node
        let peer2 = PeerChainMetadata::new(
            node_id2,
            ChainMetadata::new(network_tip_height, block_hash1.clone(), 500, accumulated_difficulty1),
        ); // Pruning horizon is to short to sync from
        let peer3 = PeerChainMetadata::new(
            node_id3.clone(),
            ChainMetadata::new(network_tip_height, block_hash1.clone(), 1440, accumulated_difficulty1),
        );
        let peer4 = PeerChainMetadata::new(
            node_id4,
            ChainMetadata::new(network_tip_height, block_hash2, 2880, accumulated_difficulty2),
        ); // Node running a fork
        let peer5 = PeerChainMetadata::new(
            node_id5.clone(),
            ChainMetadata::new(network_tip_height, block_hash1.clone(), 2880, accumulated_difficulty1),
        );
        peer_metadata_list.push(peer1);
        peer_metadata_list.push(peer2);
        peer_metadata_list.push(peer3);
        peer_metadata_list.push(peer4);
        peer_metadata_list.push(peer5);

        let best_network_metadata = best_metadata(peer_metadata_list.as_slice());
        assert_eq!(best_network_metadata.height_of_longest_chain, Some(network_tip_height));
        assert_eq!(best_network_metadata.best_block, Some(block_hash1));
        assert_eq!(
            best_network_metadata.accumulated_difficulty,
            Some(accumulated_difficulty1)
        );
        let sync_peers = select_sync_peers(local_tip_height, &best_network_metadata, &peer_metadata_list);
        assert_eq!(sync_peers.len(), 3);
        assert!(sync_peers.contains(&node_id1));
        assert!(sync_peers.contains(&node_id3));
        assert!(sync_peers.contains(&node_id5));
    }

    #[test]
    fn sync_mode_selection() {
        let mut local = ChainMetadata::default();
        local.accumulated_difficulty = Some(Difficulty::from(500000));
        match determine_sync_mode(&local, local.clone(), vec![]) {
            SyncStatus::UpToDate => assert!(true),
            _ => assert!(false),
        }

        let mut network = ChainMetadata::default();
        network.accumulated_difficulty = Some(Difficulty::from(499999));
        match determine_sync_mode(&local, network, vec![]) {
            SyncStatus::UpToDate => assert!(true),
            _ => assert!(false),
        }

        let mut network = ChainMetadata::default();
        network.accumulated_difficulty = Some(Difficulty::from(500001));
        match determine_sync_mode(&local, network.clone(), vec![]) {
            SyncStatus::Lagging(n, _) => assert_eq!(n, network),
            _ => assert!(false),
        }

        local.pruning_horizon = 50;
        local.height_of_longest_chain = Some(100);
        let mut network = ChainMetadata::default();
        network.accumulated_difficulty = Some(Difficulty::from(500001));
        network.height_of_longest_chain = Some(150);
        match determine_sync_mode(&local, network.clone(), vec![]) {
            SyncStatus::Lagging(n, _) => assert_eq!(n, network),
            _ => assert!(false),
        }

        local.pruning_horizon = 50;
        local.height_of_longest_chain = None;
        let mut network = ChainMetadata::default();
        network.accumulated_difficulty = Some(Difficulty::from(500001));
        network.height_of_longest_chain = Some(100);
        match determine_sync_mode(&local, network.clone(), vec![]) {
            SyncStatus::LaggingBehindHorizon(n, _) => assert_eq!(n, network),
            _ => assert!(false),
        }

        local.pruning_horizon = 50;
        local.height_of_longest_chain = Some(99);
        let mut network = ChainMetadata::default();
        network.accumulated_difficulty = Some(Difficulty::from(500001));
        network.height_of_longest_chain = Some(150);
        match determine_sync_mode(&local, network.clone(), vec![]) {
            SyncStatus::LaggingBehindHorizon(n, _) => assert_eq!(n, network),
            _ => assert!(false),
        }
    }
}
