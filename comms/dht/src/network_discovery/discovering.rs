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

use std::{collections::HashSet, convert::TryInto};

use futures::{stream::FuturesUnordered, Stream, StreamExt};
use log::*;
use tari_comms::{
    connectivity::ConnectivityError,
    peer_manager::{NodeDistance, NodeId, PeerFeatures},
    PeerConnection,
    PeerManager,
};

use super::{
    state_machine::{DhtNetworkDiscoveryRoundInfo, DiscoveryParams, NetworkDiscoveryContext, StateEvent},
    NetworkDiscoveryError,
};
use crate::{
    actor::OffenceSeverity,
    peer_validator::PeerValidator,
    proto::rpc::{GetPeersRequest, GetPeersResponse},
    rpc,
    rpc::UnvalidatedPeerInfo,
    DhtConfig,
};

const LOG_TARGET: &str = "comms::dht::network_discovery";

#[derive(Debug)]
pub(super) struct Discovering {
    params: DiscoveryParams,
    context: NetworkDiscoveryContext,
    stats: DhtNetworkDiscoveryRoundInfo,
    neighbourhood_threshold: NodeDistance,
}

impl Discovering {
    pub fn new(params: DiscoveryParams, context: NetworkDiscoveryContext) -> Self {
        Self {
            params,
            context,
            stats: Default::default(),
            neighbourhood_threshold: NodeDistance::max_distance(),
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
        let result = self.request_peers(peer_node_id, client).await;
        self.ban_on_offence(peer_node_id.clone(), result).await?;

        Ok(())
    }

    async fn request_peers(
        &mut self,
        sync_peer: &NodeId,
        mut client: rpc::DhtClient,
    ) -> Result<(), NetworkDiscoveryError> {
        debug!(
            target: LOG_TARGET,
            "Requesting {} peers from `{}`",
            self.params
                .num_peers_to_request,
            sync_peer
        );
        let mut stream = client
            .get_peers(GetPeersRequest {
                n: self.params.num_peers_to_request,
                include_clients: true,
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
        let mut counter = 0;
        #[allow(clippy::mutable_key_type)]
        let mut peers_received = HashSet::new();
        while let Some(resp) = stream.next().await {
            counter += 1;
            if counter > self.params.num_peers_to_request {
                warn!(target: LOG_TARGET, "Remote peer sent more peers than we requested.");
                return Err(NetworkDiscoveryError::TooManyPeersReceived);
            }
            let GetPeersResponse { peer } = resp?;

            let peer = peer.ok_or_else(|| NetworkDiscoveryError::EmptyPeerMessageReceived)?;
            let new_peer: UnvalidatedPeerInfo = peer
                .try_into()
                .map_err(NetworkDiscoveryError::InvalidPeerDataReceived)?;
            if !peers_received.insert(new_peer.public_key.clone()) {
                warn!(target: LOG_TARGET, "Remote peer sent duplicate peer.");
                return Err(NetworkDiscoveryError::DuplicatePeerReceived);
            }
            self.validate_and_add_peer(sync_peer, new_peer).await?;
        }

        Ok(())
    }

    async fn validate_and_add_peer(
        &mut self,
        sync_peer: &NodeId,
        new_peer: UnvalidatedPeerInfo,
    ) -> Result<(), NetworkDiscoveryError> {
        let node_id = NodeId::from_public_key(&new_peer.public_key);
        if self.context.node_identity.node_id() == &node_id {
            debug!(target: LOG_TARGET, "Received our own node from peer sync. Ignoring.");
            return Ok(());
        }

        let maybe_existing_peer = self.peer_manager().find_by_public_key(&new_peer.public_key).await?;
        let peer_exists = maybe_existing_peer.is_some();

        let peer_validator = PeerValidator::new(self.config());
        match peer_validator.validate_peer(new_peer, maybe_existing_peer) {
            Ok(valid_peer) => {
                if peer_exists {
                    self.stats.num_duplicate_peers += 1;
                } else {
                    self.stats.num_new_peers += 1;
                }
                self.peer_manager().add_peer(valid_peer).await?;
                Ok(())
            },
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Received invalid peer from sync peer '{}': {}. Banning sync peer.", sync_peer, err
                );
                Err(err.into())
            },
        }
    }

    async fn ban_on_offence<T>(
        &mut self,
        peer: NodeId,
        result: Result<T, NetworkDiscoveryError>,
    ) -> Result<T, NetworkDiscoveryError> {
        match result {
            Ok(t) => Ok(t),
            Err(err) => {
                match &err {
                    NetworkDiscoveryError::EmptyPeerMessageReceived |
                    NetworkDiscoveryError::InvalidPeerDataReceived(_) |
                    NetworkDiscoveryError::PeerValidationError(_) |
                    NetworkDiscoveryError::DuplicatePeerReceived |
                    NetworkDiscoveryError::TooManyPeersReceived => {
                        self.ban_peer(peer, OffenceSeverity::High, &err).await;
                    },
                    NetworkDiscoveryError::RpcError(rpc_err) if rpc_err.is_caused_by_server() => {
                        self.ban_peer(peer, OffenceSeverity::High, &err).await;
                    },
                    NetworkDiscoveryError::RpcStatus(status) if !status.is_ok() => {
                        self.ban_peer(peer, OffenceSeverity::Low, &err).await;
                    },
                    // Other errors
                    NetworkDiscoveryError::RpcStatus(_) |
                    NetworkDiscoveryError::NoSyncPeers |
                    NetworkDiscoveryError::PeerManagerError(_) |
                    NetworkDiscoveryError::RpcError(_) |
                    NetworkDiscoveryError::ConnectivityError(_) => {},
                }
                Err(err)
            },
        }
    }

    async fn ban_peer<T: ToString>(&mut self, peer: NodeId, severity: OffenceSeverity, err: T) {
        if let Err(e) = self
            .context
            .connectivity
            .ban_peer_until(
                peer.clone(),
                self.config().ban_duration_from_severity(severity),
                err.to_string(),
            )
            .await
        {
            warn!(
                target: LOG_TARGET,
                "Failed to ban peer `{}`: {}", peer, e
            );
        }
    }

    fn config(&self) -> &DhtConfig {
        &self.context.config
    }

    fn peer_manager(&self) -> &PeerManager {
        &self.context.peer_manager
    }

    fn dial_all_candidates(&self) -> impl Stream<Item = Result<PeerConnection, ConnectivityError>> + 'static {
        let pending_dials = self
            .params
            .peers
            .iter()
            .map(|peer| {
                let connectivity = self.context.connectivity.clone();
                let peer = peer.clone();
                async move { connectivity.dial_peer(peer, false).await }
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
