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

use futures::{Stream, StreamExt, stream::FuturesUnordered};
use log::*;
use tari_comms::{
    Minimized,
    PeerConnection,
    RefKind,
    connectivity::ConnectivityError,
    peer_manager::{NodeId, Peer, PeerId},
    protocol::rpc::{ClientStreaming, RpcStatus},
    types::CommsPublicKey,
};
use tari_utilities::hex::Hex;

use super::{
    NetworkDiscoveryError,
    state_machine::{
        DhtNetworkDiscoveryRoundInfo,
        DiscoveryParams,
        DiscoveryPhase,
        NetworkDiscoveryContext,
        StateEvent,
    },
};
use crate::{
    DhtConfig,
    actor::OffenceSeverity,
    peer_validator::PeerValidator,
    proto::rpc::{GetPeersRequest, GetPeersResponse},
    rpc,
    rpc::{DhtClient, UnvalidatedPeerInfo},
};

const LOG_TARGET: &str = "comms::dht::network_discovery";

/// How many claims that only a broken or hostile node could produce a sync peer may relay in one
/// round before the round is aborted and the sync peer banned.
///
/// A peer that is behind NAT or misconfigured is common and costs nothing to skip, so those claims
/// are not counted here. A forged signature or a malformed multiaddress is not something an honest
/// relay produces, and without a bound a sync peer could burn the whole per-round budget on them.
const MAX_HOSTILE_RELAYED_CLAIMS: usize = 5;

/// Classifies a failure to add a single peer that a sync peer relayed to us.
///
/// Returns `None` when the failure is not about the relayed claim at all (storage, connectivity) and
/// the round therefore cannot continue. Otherwise the round continues without this peer, and the
/// boolean says whether the sync peer relayed something only a broken or hostile node would produce.
fn classify_relay_failure(err: &NetworkDiscoveryError) -> Option<bool> {
    match err {
        NetworkDiscoveryError::PeerValidationError(err) => Some(err.is_ban_offence()),
        _ => None,
    }
}

#[derive(Debug)]
pub(super) struct Discovering {
    params: DiscoveryParams,
    context: NetworkDiscoveryContext,
    stats: DhtNetworkDiscoveryRoundInfo,
}

impl Discovering {
    pub fn new(params: DiscoveryParams, context: NetworkDiscoveryContext) -> Self {
        Self {
            params,
            context,
            stats: Default::default(),
        }
    }

    async fn initialize(&mut self) -> Result<(), NetworkDiscoveryError> {
        if self.params.peers.is_empty() {
            return Err(NetworkDiscoveryError::NoSyncPeers);
        }

        // Set discovery phase and rounds information
        self.stats.phase = DiscoveryPhase::General;

        Ok(())
    }

    async fn find_by_public_key(&self, public_key: CommsPublicKey) -> Result<Option<Peer>, NetworkDiscoveryError> {
        Ok(self.context.peer_manager.find_by_public_key(&public_key).await?)
    }

    async fn add_peer(&self, peer: Peer) -> Result<PeerId, NetworkDiscoveryError> {
        Ok(self.context.peer_manager.add_or_update_peer(peer).await?)
    }

    pub async fn next_event(&mut self) -> StateEvent {
        debug!(
            target: LOG_TARGET,
            "Discovering: Starting network discovery with params {}", self.params
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
                    debug!(target: LOG_TARGET, "Discovering: Attempting to sync from peer `{peer_node_id}`" );

                    if self.request_from_peers(conn).await.is_ok() {
                        self.stats.num_succeeded += 1;
                    }
                },
                Err(err) => {
                    debug!(target: LOG_TARGET, "Discovering: Failed to connect to sync peer candidate: {err}");
                },
            }
        }

        if self.stats.num_succeeded == 0 && !self.stats.sync_peers.is_empty() {
            warn!(
                target: LOG_TARGET,
                "Discovering: Round completed with 0/{} successful peer syncs",
                self.stats.sync_peers.len(),
            );
        } else {
            debug!(
                target: LOG_TARGET,
                "Discovering: Round completed with {}/{} successful peer syncs",
                self.stats.num_succeeded,
                self.stats.sync_peers.len(),
            );
        }

        StateEvent::DiscoveryComplete(self.stats.clone())
    }

    async fn request_from_peers(&mut self, mut conn: PeerConnection) -> Result<(), NetworkDiscoveryError> {
        if !conn.is_connected() {
            debug!(
                target: LOG_TARGET,
                "Discovering: Skipping sync peer '{}' because connection is no longer active",
                conn.peer_node_id(),
            );
            return Err(NetworkDiscoveryError::PeerConnectionClosed {
                peer: conn.peer_node_id().to_hex(),
                reason: "Connection closed before RPC connect".to_string(),
            });
        }

        let rpc_connect_timeout = self.config().network_discovery.bootstrap_rpc_connect_timeout;
        let client = match tokio::time::timeout(rpc_connect_timeout, conn.connect_rpc::<DhtClient>()).await {
            Ok(Ok(client)) => client,
            Ok(Err(e)) => {
                warn!(
                    target: LOG_TARGET,
                    "Discovering: Failed to connect RPC client to sync peer {}: {}",
                    conn.peer_node_id(),
                    e
                );
                let _unused = conn.disconnect(Minimized::Yes, "Discovering RPC connect failed").await;
                return Err(e.into());
            },
            Err(_) => {
                // the peer most likely is not online or has an old address.
                debug!(
                    target: LOG_TARGET,
                    "Discovering: RPC connect_rpc to sync peer '{}' timed out after {:?}",
                    conn.peer_node_id(),
                    rpc_connect_timeout,
                );
                let _unused = conn.disconnect(Minimized::Yes, "Discovering RPC connect timeout").await;
                return Err(NetworkDiscoveryError::Timeout {
                    operation: "connect_rpc".to_string(),
                    peer: conn.peer_node_id().to_hex(),
                    duration: format!("{rpc_connect_timeout:.2?}"),
                });
            },
        };

        trace!(
            target: LOG_TARGET,
            "Discovering: Successfully connected RPC client to sync peer '{}'",
            conn.peer_node_id()
        );

        let peer_node_id = conn.peer_node_id();

        debug!(
            target: LOG_TARGET,
            "Discovering: Established RPC connection to sync peer `{peer_node_id}`"
        );
        let result = self.request_peers(peer_node_id, client).await;
        self.ban_on_offence(peer_node_id.clone(), result).await?;
        let _unused = conn.disconnect(Minimized::Yes, "Discovering sync complete").await;

        Ok(())
    }

    async fn get_stream(
        &mut self,
        mut client: rpc::DhtClient,
        sync_peer: &NodeId,
    ) -> Result<ClientStreaming<GetPeersResponse>, NetworkDiscoveryError> {
        let rpc_get_peers_stream_timeout = self.config().network_discovery.bootstrap_rpc_get_peers_stream_timeout;
        let peer_stream = tokio::time::timeout(
            rpc_get_peers_stream_timeout,
            client.get_peers(GetPeersRequest {
                n: self.params.num_peers_to_request,
                include_clients: false,
                max_claims: self.config().max_permitted_peer_claims.try_into().unwrap_or_else(|_| {
                    error!(
                        target: LOG_TARGET,
                        "Discovering: Node configured to accept more than u32::MAX claims per peer"
                    );
                    u32::MAX
                }),
                max_addresses_per_claim: self
                    .config()
                    .peer_validator_config
                    .max_permitted_peer_addresses_per_claim
                    .try_into()
                    .unwrap_or_else(|_| {
                        error!(
                            target: LOG_TARGET,
                            "Discovering: Node configured to accept more than u32::MAX addresses per claim"
                        );
                        u32::MAX
                    }),
            }),
        )
        .await
        .map_err(|_| {
            error!(
                target: LOG_TARGET,
                "Discovering: RPC get_peers from sync peer '{sync_peer}' timed out after {rpc_get_peers_stream_timeout:?}"
            );
            NetworkDiscoveryError::Timeout {
                operation: "get_peers".to_string(),
                peer: sync_peer.to_hex(),
                duration: format!("{rpc_get_peers_stream_timeout:.2?}"),
            }
        })?
        .inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "Discovering: Failed to initiate get_peers stream from sync peer '{sync_peer}': {e}. This sync peer will be \
                skipped."
            );
        })?;

        debug!(
            target: LOG_TARGET,
            "Discovering: Successfully initiated get_peers stream from sync peer '{sync_peer}'"

        );

        Ok(peer_stream)
    }

    async fn get_peer_response(
        &mut self,
        stream: &mut ClientStreaming<GetPeersResponse>,
        sync_peer: &NodeId,
    ) -> Result<Option<Result<GetPeersResponse, RpcStatus>>, NetworkDiscoveryError> {
        let rpc_streaming_timeout = self.config().network_discovery.bootstrap_rpc_streaming_timeout;

        tokio::time::timeout(rpc_streaming_timeout, stream.next())
            .await
            .map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Discovering: RPC get_peer_response from stream '{sync_peer}' timed out after {rpc_streaming_timeout:?}"
                );
                NetworkDiscoveryError::Timeout {
                    operation: "get_peer_response".to_string(),
                    peer: sync_peer.to_hex(),
                    duration: format!("{rpc_streaming_timeout:.2?}"),
                }
            })
    }

    async fn request_peers(&mut self, sync_peer: &NodeId, client: rpc::DhtClient) -> Result<(), NetworkDiscoveryError> {
        debug!(
            target: LOG_TARGET,
            "Discovering: Requesting {} peers from `{}`",
            self.params.num_peers_to_request,
            sync_peer
        );
        let mut stream = self.get_stream(client, sync_peer).await?;
        let mut counter = 0;
        let mut hostile_claims = 0;
        #[allow(clippy::mutable_key_type)]
        let mut peers_received = HashSet::new();
        while let Some(resp) = self.get_peer_response(&mut stream, sync_peer).await? {
            counter += 1;
            if counter > self.params.num_peers_to_request {
                warn!(target: LOG_TARGET, "Discovering: Sync peer `{sync_peer}` sent more peers than we requested.");
                return Err(NetworkDiscoveryError::TooManyPeersReceived);
            }
            let GetPeersResponse { peer } = resp.map_err(|err| {
                warn!(
                    target: LOG_TARGET,
                    "Discovering: Sync peer `{sync_peer}` sent an error response: {err:?}"
                );
                NetworkDiscoveryError::from(err)
            })?;
            let peer = peer
                .ok_or_else(|| NetworkDiscoveryError::EmptyPeerMessageReceived)
                .inspect_err(|err| {
                    warn!(
                        target: LOG_TARGET,
                        "Discovering: Sync peer `{sync_peer}` sent an empty peer message: {err:?}"
                    );
                })?;
            let new_peer: UnvalidatedPeerInfo = peer
                .try_into()
                .map_err(NetworkDiscoveryError::InvalidPeerDataReceived)
                .inspect_err(|err| {
                    warn!(
                        target: LOG_TARGET,
                        "Discovering: Sync peer `{sync_peer}` sent invalid peer data: {err:?}"
                    );
                })?;

            if !peers_received.insert(new_peer.public_key.clone()) {
                let err = NetworkDiscoveryError::DuplicatePeerReceived;
                warn!(target: LOG_TARGET, "Discovering: Sync peer `{sync_peer}` sent duplicate peer: {err:?}");
                return Err(err);
            }
            match self.validate_and_add_peer(new_peer).await {
                Ok(()) => {},
                Err(err) => match classify_relay_failure(&err) {
                    // A sync peer merely relays signed claims. Skip a bad or unusable claim without
                    // truncating the rest of the stream; otherwise the same entry can permanently
                    // block every subsequent peer in each discovery round.
                    Some(is_offence) => {
                        if is_offence {
                            hostile_claims += 1;
                        }
                        warn!(
                            target: LOG_TARGET,
                            "Discovering: Skipping peer relayed by sync peer `{sync_peer}` after validation failed \
                             (hostile: {is_offence}): {err:?}"
                        );
                        // Relaying the occasional unusable claim is normal - relaying claims that
                        // only a broken or hostile node could produce is not, and skipping them
                        // must not be free.
                        if hostile_claims > MAX_HOSTILE_RELAYED_CLAIMS {
                            warn!(
                                target: LOG_TARGET,
                                "Discovering: Sync peer `{sync_peer}` relayed more than {MAX_HOSTILE_RELAYED_CLAIMS} \
                                 unusable peer claims."
                            );
                            return Err(NetworkDiscoveryError::TooManyInvalidPeersReceived);
                        }
                    },
                    None => {
                        warn!(
                            target: LOG_TARGET,
                            "Discovering: Failed to add peer from sync peer `{sync_peer}`: {err:?}"
                        );
                        return Err(err);
                    },
                },
            }
        }

        Ok(())
    }

    async fn validate_and_add_peer(&mut self, new_peer: UnvalidatedPeerInfo) -> Result<(), NetworkDiscoveryError> {
        let node_id = NodeId::from_public_key(&new_peer.public_key);
        if self.context.node_identity.node_id() == &node_id {
            debug!(target: LOG_TARGET, "Discovering: Received our own node from peer sync. Ignoring.");
            return Ok(());
        }

        let maybe_existing_peer = self.find_by_public_key(new_peer.public_key.clone()).await?;
        let peer_exists = maybe_existing_peer.is_some();

        let peer_validator = PeerValidator::new(self.config());
        match peer_validator.validate_peer(new_peer, maybe_existing_peer) {
            Ok(valid_peer) => {
                if peer_exists {
                    self.stats.num_duplicate_peers += 1;
                } else {
                    self.stats.num_new_peers += 1;
                }
                self.add_peer(valid_peer).await?;
                Ok(())
            },
            Err(err) => Err(err.into()),
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
                    NetworkDiscoveryError::DuplicatePeerReceived |
                    NetworkDiscoveryError::TooManyPeersReceived |
                    NetworkDiscoveryError::TooManyInvalidPeersReceived => {
                        self.ban_peer(peer, OffenceSeverity::High, &err).await;
                    },
                    NetworkDiscoveryError::RpcError(rpc_err) if rpc_err.is_caused_by_server() => {
                        self.ban_peer(peer, OffenceSeverity::High, &err).await;
                    },
                    NetworkDiscoveryError::RpcStatus(status) if !status.is_ok() => {
                        self.ban_peer(peer, OffenceSeverity::Low, &err).await;
                    },
                    // Other errors - no banning needed
                    NetworkDiscoveryError::RpcStatus(_) |
                    NetworkDiscoveryError::NoSyncPeers |
                    NetworkDiscoveryError::PeerManagerError(_) |
                    NetworkDiscoveryError::RpcError(_) |
                    NetworkDiscoveryError::ConnectivityError(_) |
                    NetworkDiscoveryError::PeerValidationError(_) |
                    NetworkDiscoveryError::JoinError(_) |
                    NetworkDiscoveryError::Timeout { .. } |
                    NetworkDiscoveryError::PeerConnectionClosed { .. } => {},
                }
                Err(err)
            },
        }
    }

    async fn ban_peer<T: ToString>(&mut self, peer: NodeId, severity: OffenceSeverity, err: T) {
        match self
            .context
            .connectivity
            .ban_peer_until(
                peer.clone(),
                self.config().ban_duration_from_severity(severity),
                err.to_string(),
            )
            .await
        {
            Ok(_) => {
                warn!(
                    target: LOG_TARGET,
                    "Discovering: Banned peer `{}` for {:.2?} due to '{}'",
                    peer, self.config().ban_duration_from_severity(severity), err.to_string()
                );
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Discovering: Failed to ban peer `{peer}`: {e}"
                );
            },
        }
    }

    fn config(&self) -> &DhtConfig {
        &self.context.config
    }

    fn dial_all_candidates(&self) -> impl Stream<Item = Result<PeerConnection, ConnectivityError>> + 'static {
        let pending_dials = self
            .params
            .peers
            .iter()
            .map(|peer| {
                let connectivity = self.context.connectivity.clone();
                let peer = peer.clone();
                async move { connectivity.dial_peer(peer, RefKind::Weak).await }
            })
            .collect::<FuturesUnordered<_>>();

        debug!(
            target: LOG_TARGET,
            "Discovering: Dialing {} candidate peer(s) for peer sync",
            pending_dials.len()
        );
        pending_dials
    }
}

#[cfg(test)]
mod test {
    use tari_comms::{
        peer_manager::{NodeId, PeerManagerError},
        peer_validator::PeerValidatorError,
    };

    use super::*;
    use crate::peer_validator::DhtPeerValidatorError;

    fn validation_error(err: PeerValidatorError) -> NetworkDiscoveryError {
        DhtPeerValidatorError::ValidatorError(err).into()
    }

    #[test]
    fn a_relayed_claim_that_fails_validation_does_not_end_the_round() {
        // A peer behind NAT, or one we simply cannot use, is skipped for free.
        assert_eq!(
            classify_relay_failure(&validation_error(PeerValidatorError::PeerHasNoUsableAddresses {
                peer: NodeId::default()
            })),
            Some(false)
        );
        assert_eq!(
            classify_relay_failure(&validation_error(PeerValidatorError::PeerHasNoAddresses {
                peer: NodeId::default()
            })),
            Some(false)
        );

        // A forged signature or malformed address is skipped too, but counts against the sync peer.
        assert_eq!(
            classify_relay_failure(&validation_error(PeerValidatorError::InvalidPeerSignature {
                peer: NodeId::default()
            })),
            Some(true)
        );
        assert_eq!(
            classify_relay_failure(&validation_error(PeerValidatorError::InvalidMultiaddr(
                "bad address".to_string()
            ))),
            Some(true)
        );
        assert_eq!(
            classify_relay_failure(&NetworkDiscoveryError::PeerValidationError(
                DhtPeerValidatorError::IdentityTooManyClaims { length: 2, max: 1 }
            )),
            Some(true)
        );
    }

    #[test]
    fn a_local_failure_ends_the_round() {
        // Nothing to do with what the sync peer sent, so the round cannot continue.
        assert_eq!(
            classify_relay_failure(&NetworkDiscoveryError::PeerManagerError(PeerManagerError::BannedPeer)),
            None
        );
        assert_eq!(classify_relay_failure(&NetworkDiscoveryError::NoSyncPeers), None);
    }
}
