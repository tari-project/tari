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

use super::{
    state_machine::{DhtNetworkDiscoveryRoundInfo, DiscoveryParams, NetworkDiscoveryContext, StateEvent},
    NetworkDiscoveryError,
};
use crate::{proto::rpc::GetPeersRequest, rpc, DhtConfig};
use futures::{stream::FuturesUnordered, Stream, StreamExt};
use log::*;
use std::convert::TryInto;
use tari_comms::{
    connectivity::ConnectivityError,
    peer_manager::{node_id::NodeDistance, NodeId, Peer, PeerFeatures},
    validate_peer_addresses,
    PeerConnection,
};

const LOG_TARGET: &str = "comms::dht::network_discovery";

#[derive(Debug)]
pub struct Discovering {
    params: DiscoveryParams,
    context: NetworkDiscoveryContext,
    candidate_peers: Vec<PeerConnection>,
    stats: DhtNetworkDiscoveryRoundInfo,
    neighbourhood_threshold: NodeDistance,
    excluded_peers: Vec<NodeId>,
}

impl Discovering {
    pub fn new(params: DiscoveryParams, context: NetworkDiscoveryContext) -> Self {
        Self {
            params,
            context,
            candidate_peers: Vec::new(),
            stats: Default::default(),
            neighbourhood_threshold: NodeDistance::max_distance(),
            excluded_peers: Vec::new(),
        }
    }

    async fn initialize(&mut self) -> Result<(), NetworkDiscoveryError> {
        if self.params.peers.is_empty() {
            return Err(NetworkDiscoveryError::NoSyncPeers);
        }

        // The neighbourhood threshold is used to determine how many new neighbours we're receiving from a peer or
        // peers. When "bootstrapping" from a seed node, receiving many new neighbours is expected and acceptable.
        // However during a normal non-bootstrap sync receiving all new neighbours is a bit "fishy" and should be
        // treated as suspicious.
        self.neighbourhood_threshold = self
            .context
            .peer_manager
            .calc_region_threshold(
                self.context.node_identity.node_id(),
                self.config().num_neighbouring_nodes,
                PeerFeatures::COMMUNICATION_NODE,
            )
            .await?;

        Ok(())
    }

    pub async fn next_event(&mut self) -> StateEvent {
        debug!(
            target: LOG_TARGET,
            "Starting network discovery with params {}", self.params
        );

        if let Err(err) = self.initialize().await {
            return err.into();
        }

        let mut dial_stream = self.dial_all_candidates();
        while let Some(result) = dial_stream.next().await {
            match result {
                Ok(conn) => {
                    let peer_node_id = conn.peer_node_id().clone();
                    self.stats.sync_peers.push(peer_node_id.clone());
                    debug!(target: LOG_TARGET, "Attempting to sync from peer `{}`", peer_node_id);

                    match self.request_from_peers(conn).await {
                        Ok(_) => {
                            self.stats.num_succeeded += 1;
                        },
                        Err(err) => {
                            debug!(
                                target: LOG_TARGET,
                                "Failed to request peers from `{}`: {}", peer_node_id, err
                            );
                        },
                    }
                },
                Err(err) => {
                    debug!(target: LOG_TARGET, "Failed to connect to sync peer candidate: {}", err);
                },
            }
        }

        StateEvent::DiscoveryComplete(self.stats.clone())
    }

    async fn request_from_peers(&mut self, mut conn: PeerConnection) -> Result<(), NetworkDiscoveryError> {
        let client = conn.connect_rpc::<rpc::DhtClient>().await?;
        let peer_node_id = conn.peer_node_id();

        debug!(
            target: LOG_TARGET,
            "Established RPC connection to peer `{}`", peer_node_id
        );
        self.request_peers(peer_node_id, client).await?;

        Ok(())
    }

    async fn request_peers(
        &mut self,
        sync_peer: &NodeId,
        mut client: rpc::DhtClient,
    ) -> Result<(), NetworkDiscoveryError>
    {
        debug!(
            target: LOG_TARGET,
            "Requesting {} peers from `{}`", self.params.num_peers_to_request, sync_peer
        );
        match client
            .get_peers(GetPeersRequest {
                n: self.params.num_peers_to_request as u32,
            })
            .await
        {
            Ok(mut stream) => {
                while let Some(resp) = stream.next().await {
                    match resp {
                        Ok(resp) => match resp.peer.and_then(|peer| peer.try_into().ok()) {
                            Some(peer) => {
                                self.validate_and_add_peer(sync_peer, peer).await?;
                            },
                            None => {
                                debug!(target: LOG_TARGET, "Invalid response from peer `{}`", sync_peer);
                            },
                        },
                        Err(err) => {
                            debug!(target: LOG_TARGET, "Error response from peer `{}`: {}", sync_peer, err);
                        },
                    }
                }
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to request for peers from peer `{}`: {}", sync_peer, err
                );
            },
        }

        Ok(())
    }

    async fn validate_and_add_peer(&mut self, sync_peer: &NodeId, peer: Peer) -> Result<(), NetworkDiscoveryError> {
        let peer_manager = &self.context.peer_manager;
        if peer_manager.exists_node_id(&peer.node_id).await {
            self.stats.num_duplicate_peers += 1;
            debug!(
                target: LOG_TARGET,
                "Peer `{}` already exists and will not be added to the peer list", peer.node_id
            );
            return Ok(());
        }

        let peer_dist = peer.node_id.distance(self.context.node_identity.node_id());
        let is_neighbour = peer_dist <= self.neighbourhood_threshold;

        let addresses = peer.addresses.address_iter();
        match validate_peer_addresses(addresses, self.config().network.is_localtest()) {
            Ok(_) => {
                if is_neighbour {
                    self.stats.num_new_neighbours += 1;
                    debug!(
                        target: LOG_TARGET,
                        "Adding new neighbouring peer `{}`. {} (inclusive) have been added this round.",
                        peer.node_id,
                        self.stats.num_new_neighbours
                    );
                }

                debug!(
                    target: LOG_TARGET,
                    "Adding peer `{}` from `{}`", peer.node_id, sync_peer
                );
                self.stats.num_new_peers += 1;
                peer_manager.add_peer(peer).await?;
            },
            Err(err) => {
                // TODO: #banheuristic
                debug!(
                    target: LOG_TARGET,
                    "Failed to validate peer received from `{}`: {}. Peer not added.", sync_peer, err
                );
            },
        }

        Ok(())
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }

    fn dial_all_candidates(&self) -> impl Stream<Item = Result<PeerConnection, ConnectivityError>> + 'static {
        let pending_dials = self
            .params
            .peers
            .iter()
            .map(|peer| {
                let mut connectivity = self.context.connectivity.clone();
                let peer = peer.clone();
                async move { connectivity.dial_peer(peer).await }
            })
            .collect::<FuturesUnordered<_>>();

        debug!(
            target: LOG_TARGET,
            "Dialing {} candidate peer(s) for peer sync",
            pending_dials.len()
        );
        pending_dials
    }
}
