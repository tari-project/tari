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

use crate::{
    event::DhtEvent,
    network_discovery::{
        state_machine::{NetworkDiscoveryContext, StateEvent},
        DhtNetworkDiscoveryRoundInfo,
        NetworkDiscoveryError,
    },
    proto::rpc::GetPeersRequest,
    rpc,
    DhtConfig,
};
use futures::StreamExt;
use log::*;
use std::{convert::TryInto, ops::Deref};
use tari_comms::{
    connectivity::ConnectivityEvent,
    peer_manager::{NodeId, Peer},
    validate_peer_addresses,
    PeerConnection,
};
use tokio::sync::broadcast;

const LOG_TARGET: &str = "comms::dht::network_discovery:onconnect";

#[derive(Debug)]
pub(super) struct OnConnect {
    context: NetworkDiscoveryContext,
    prev_synced: Vec<NodeId>,
}

impl OnConnect {
    pub fn new(context: NetworkDiscoveryContext) -> Self {
        Self {
            context,
            prev_synced: Vec::new(),
        }
    }

    pub async fn next_event(&mut self) -> StateEvent {
        let mut connectivity_events = self.context.connectivity.get_event_subscription();
        while let Some(event) = connectivity_events.next().await {
            match event.as_ref().map(|e| e.deref()) {
                Ok(ConnectivityEvent::PeerConnected(conn)) => {
                    if conn.peer_features().is_client() {
                        continue;
                    }
                    if self.prev_synced.contains(conn.peer_node_id()) {
                        debug!(
                            target: LOG_TARGET,
                            "Already synced from peer `{}`. Skipping",
                            conn.peer_node_id()
                        );
                        continue;
                    }

                    debug!(
                        target: LOG_TARGET,
                        "Node peer `{}` connected. Syncing peers...",
                        conn.peer_node_id()
                    );

                    match self.sync_peers(conn.clone()).await {
                        Ok(_) => continue,
                        Err(err) => debug!(
                            target: LOG_TARGET,
                            "Failed to peer sync from `{}`: {}",
                            conn.peer_node_id(),
                            err
                        ),
                    }

                    self.prev_synced.push(conn.peer_node_id().clone());
                },
                Ok(_) => { /* Nothing to do */ },
                Err(broadcast::RecvError::Lagged(n)) => {
                    warn!(target: LOG_TARGET, "Lagged behind on {} connectivity event(s)", n)
                },
                Err(broadcast::RecvError::Closed) => {
                    break;
                },
            }
        }

        StateEvent::Shutdown
    }

    async fn sync_peers(&self, mut conn: PeerConnection) -> Result<(), NetworkDiscoveryError> {
        let mut client = conn.connect_rpc::<rpc::DhtClient>().await?;
        let mut peer_stream = client
            .get_peers(GetPeersRequest {
                // Sync all peers
                n: 0,
                include_clients: true,
            })
            .await?;

        let sync_peer = conn.peer_node_id();
        let mut num_added = 0;
        while let Some(resp) = peer_stream.next().await {
            match resp {
                Ok(resp) => match resp.peer.and_then(|peer| peer.try_into().ok()) {
                    Some(peer) => {
                        let is_node_added = self.validate_and_add_peer(sync_peer, peer).await?;
                        if is_node_added {
                            num_added += 1;
                        }
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

        debug!(
            target: LOG_TARGET,
            "Added {} peer(s) from peer `{}`", num_added, sync_peer
        );
        if num_added > 0 {
            self.context
                .publish_event(DhtEvent::NetworkDiscoveryPeersAdded(DhtNetworkDiscoveryRoundInfo {
                    // TODO: num_new_neighbours could be incorrect here
                    num_new_neighbours: 0,
                    num_new_peers: num_added,
                    num_duplicate_peers: 0,
                    num_succeeded: num_added,
                    sync_peers: vec![conn.peer_node_id().clone()],
                }));
        }

        Ok(())
    }

    async fn validate_and_add_peer(&self, sync_peer: &NodeId, peer: Peer) -> Result<bool, NetworkDiscoveryError> {
        let peer_manager = &self.context.peer_manager;
        if peer_manager.exists_node_id(&peer.node_id).await {
            return Ok(false);
        }

        let addresses = peer.addresses.address_iter();
        match validate_peer_addresses(addresses, self.config().network.is_localtest()) {
            Ok(_) => {
                debug!(
                    target: LOG_TARGET,
                    "Adding peer `{}` from `{}`", peer.node_id, sync_peer
                );
                peer_manager.add_peer(peer).await?;
                Ok(true)
            },
            Err(err) => {
                // TODO: #banheuristic
                debug!(
                    target: LOG_TARGET,
                    "Failed to validate peer received from `{}`: {}. Peer not added.", sync_peer, err
                );
                Ok(false)
            },
        }
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }
}
