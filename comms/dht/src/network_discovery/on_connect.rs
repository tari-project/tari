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

use std::convert::TryInto;

use futures::StreamExt;
use log::*;
use tari_comms::{connectivity::ConnectivityEvent, peer_manager::NodeId, PeerConnection};
use tokio::sync::broadcast;

use crate::{
    event::DhtEvent,
    network_discovery::{
        state_machine::{NetworkDiscoveryContext, StateEvent},
        DhtNetworkDiscoveryRoundInfo,
        NetworkDiscoveryError,
    },
    peer_validator::PeerValidator,
    proto::rpc::GetPeersRequest,
    rpc,
    rpc::UnvalidatedPeerInfo,
    DhtConfig,
};
const LOG_TARGET: &str = "comms::dht::network_discovery:onconnect";
const NUM_FETCH_PEERS: u32 = 100;

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
        loop {
            let event = connectivity_events.recv().await;
            match event {
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

                    match self.sync_peers(*conn.clone()).await {
                        Ok(_) => continue,
                        Err(err @ NetworkDiscoveryError::PeerValidationError(_)) => {
                            warn!(target: LOG_TARGET, "{}. Banning peer.", err);
                            if let Err(err) = self
                                .context
                                .connectivity
                                .ban_peer_until(
                                    conn.peer_node_id().clone(),
                                    self.config().ban_duration,
                                    err.to_string(),
                                )
                                .await
                            {
                                return err.into();
                            }
                        },
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
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(target: LOG_TARGET, "Lagged behind on {} connectivity event(s)", n)
                },
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                },
            }
        }

        StateEvent::Shutdown
    }

    async fn sync_peers(&mut self, mut conn: PeerConnection) -> Result<(), NetworkDiscoveryError> {
        let mut client = conn.connect_rpc::<rpc::DhtClient>().await?;
        let peer_stream = client
            .get_peers(GetPeersRequest {
                n: NUM_FETCH_PEERS,
                include_clients: false,
                max_claims: self.config().max_permitted_peer_claims.try_into().unwrap_or_else(|_| {
                    error!(target: LOG_TARGET, "Node configured to accept more than u32::MAX claims per peer");
                    u32::MAX
                }),
                max_addresses_per_claim: self
                    .config()
                    .peer_validator_config
                    .max_permitted_peer_addresses_per_claim
                    .try_into()
                    .unwrap_or_else(|_| {
                        error!(target: LOG_TARGET, "Node configured to accept more than u32::MAX addresses per claim");
                        u32::MAX
                    }),
            })
            .await?;

        // Take up to `NUM_FETCH_PEERS` then close the stream.
        let mut peer_stream = peer_stream.take(NUM_FETCH_PEERS as usize);

        let sync_peer = conn.peer_node_id();
        let mut num_added = 0;
        while let Some(resp) = peer_stream.next().await {
            match resp {
                Ok(resp) => match resp.peer.and_then(|peer| peer.try_into().ok()) {
                    Some(peer) => {
                        if self.validate_and_add_peer(peer).await? {
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
                    num_new_peers: num_added,
                    num_duplicate_peers: 0,
                    num_succeeded: num_added,
                    sync_peers: vec![conn.peer_node_id().clone()],
                }));
        }

        Ok(())
    }

    /// Returns true if the peer is a new peer
    async fn validate_and_add_peer(&self, peer: UnvalidatedPeerInfo) -> Result<bool, NetworkDiscoveryError> {
        let peer_validator = PeerValidator::new(self.config());
        let maybe_existing_peer = self.context.peer_manager.find_by_public_key(&peer.public_key).await?;
        let is_new_peer = maybe_existing_peer.is_none();
        let valid_peer = peer_validator.validate_peer(peer, maybe_existing_peer)?;
        self.context.peer_manager.add_peer(valid_peer).await?;
        Ok(is_new_peer)
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }
}
