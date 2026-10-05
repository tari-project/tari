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
    connectivity::{ConnectivityError, ConnectivityRequester},
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
pub(super) const MAX_HOSTILE_RELAYED_CLAIMS: usize = 5;

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
                Ok((conn, was_connected)) => {
                    let peer_node_id = conn.peer_node_id().clone();
                    self.stats.sync_peers.push(peer_node_id.clone());
                    debug!(target: LOG_TARGET, "Discovering: Attempting to sync from peer `{peer_node_id}`" );

                    if self.request_from_peers(conn, was_connected).await.is_ok() {
                        self.stats.num_succeeded = self.stats.num_succeeded.saturating_add(1);
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

    /// `was_connected` is true if the connection existed before this round dialled it. Only a connection this round
    /// created is hung up, and not if it has since become a DHT pool peer (or is strongly held): anything else belongs
    /// to someone else.
    async fn request_from_peers(
        &mut self,
        mut conn: PeerConnection,
        was_connected: bool,
    ) -> Result<(), NetworkDiscoveryError> {
        let created_here = !was_connected;
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
                self.hang_up_if_ours(&mut conn, created_here, "Discovering RPC connect failed")
                    .await;
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
                self.hang_up_if_ours(&mut conn, created_here, "Discovering RPC connect timeout")
                    .await;
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
        self.ban_on_offence(peer_node_id.clone(), result, was_connected).await?;
        self.hang_up_if_ours(&mut conn, created_here, "Discovering sync complete")
            .await;

        Ok(())
    }

    /// Hangs up a connection this round created, unless the DHT pool has taken it (or it is strongly held).
    async fn hang_up_if_ours(&self, conn: &mut PeerConnection, created_here: bool, reason: &str) {
        let is_pool_peer = self
            .context
            .pool_peers
            .read()
            .map(|pool| pool.contains(conn.peer_node_id()))
            .unwrap_or(true);
        if created_here && !is_pool_peer && !conn.is_strongly_held() {
            let _unused = conn.disconnect(Minimized::Yes, reason).await;
        }
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
        let mut counter = 0u32;
        let mut hostile_claims = 0usize;
        #[allow(clippy::mutable_key_type)]
        let mut peers_received = HashSet::new();
        while let Some(resp) = self.get_peer_response(&mut stream, sync_peer).await? {
            counter = counter.saturating_add(1);
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
                            hostile_claims = hostile_claims.saturating_add(1);
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
                    self.stats.num_duplicate_peers = self.stats.num_duplicate_peers.saturating_add(1);
                } else {
                    self.stats.num_new_peers = self.stats.num_new_peers.saturating_add(1);
                }
                self.add_peer(valid_peer).await?;
                Ok(())
            },
            Err(err) => Err(err.into()),
        }
    }

    /// Bans `peer` if `result` is its fault. A peer we were already connected to (`was_connected`) may be one of a
    /// starved node's last connections, so it is only banned for bad peer data, not for RPC failures.
    pub(super) async fn ban_on_offence<T>(
        &mut self,
        peer: NodeId,
        result: Result<T, NetworkDiscoveryError>,
        was_connected: bool,
    ) -> Result<T, NetworkDiscoveryError> {
        if let Err(err) = &result {
            let severity = if was_connected {
                is_bad_data_offence(err).then_some(OffenceSeverity::High)
            } else {
                offence_severity(err)
            };
            if let Some(severity) = severity {
                ban_peer(&self.context, peer, severity, err).await;
            }
        }
        result
    }

    fn config(&self) -> &DhtConfig {
        &self.context.config
    }

    /// Dials every candidate. Each connection comes with whether it already existed before the dial.
    fn dial_all_candidates(&self) -> impl Stream<Item = Result<(PeerConnection, bool), ConnectivityError>> + 'static {
        let pending_dials = self
            .params
            .peers
            .iter()
            .map(|peer| {
                let mut connectivity = self.context.connectivity.clone();
                let peer = peer.clone();
                async move {
                    let was_connected = is_connected(&mut connectivity, &peer).await;
                    let conn = connectivity.dial_peer(peer, RefKind::Weak).await?;
                    Ok((conn, was_connected))
                }
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

/// Returns true if the sync peer sent bad peer data, as opposed to an RPC failure (no free sessions, timeouts) that
/// can happen to any honest peer.
pub(super) fn is_bad_data_offence(err: &NetworkDiscoveryError) -> bool {
    matches!(
        err,
        NetworkDiscoveryError::EmptyPeerMessageReceived |
            NetworkDiscoveryError::InvalidPeerDataReceived(_) |
            NetworkDiscoveryError::DuplicatePeerReceived |
            NetworkDiscoveryError::TooManyPeersReceived |
            NetworkDiscoveryError::TooManyInvalidPeersReceived
    )
}

/// Returns how severely to ban a sync peer for this error, or `None` if it is not the sync peer's fault.
pub(super) fn offence_severity(err: &NetworkDiscoveryError) -> Option<OffenceSeverity> {
    match err {
        NetworkDiscoveryError::EmptyPeerMessageReceived |
        NetworkDiscoveryError::InvalidPeerDataReceived(_) |
        NetworkDiscoveryError::DuplicatePeerReceived |
        NetworkDiscoveryError::TooManyPeersReceived |
        NetworkDiscoveryError::TooManyInvalidPeersReceived => Some(OffenceSeverity::High),
        NetworkDiscoveryError::RpcError(rpc_err) if rpc_err.is_caused_by_server() => Some(OffenceSeverity::High),
        NetworkDiscoveryError::RpcStatus(status) if !status.is_ok() => Some(OffenceSeverity::Low),
        // Other errors - no banning needed
        NetworkDiscoveryError::RpcStatus(_) |
        NetworkDiscoveryError::NoSyncPeers |
        NetworkDiscoveryError::PeerManagerError(_) |
        NetworkDiscoveryError::RpcError(_) |
        NetworkDiscoveryError::ConnectivityError(_) |
        NetworkDiscoveryError::PeerValidationError(_) |
        NetworkDiscoveryError::JoinError(_) |
        NetworkDiscoveryError::Timeout { .. } |
        NetworkDiscoveryError::PeerConnectionClosed { .. } => None,
    }
}

/// Bans a sync peer for `err` for the duration that goes with `severity`.
pub(super) async fn ban_peer<T: ToString>(
    context: &NetworkDiscoveryContext,
    peer: NodeId,
    severity: OffenceSeverity,
    err: T,
) {
    let duration = context.config.ban_duration_from_severity(severity);
    match context
        .connectivity
        .clone()
        .ban_peer_until(peer.clone(), duration, err.to_string())
        .await
    {
        Ok(_) => {
            warn!(
                target: LOG_TARGET,
                "Banned sync peer `{}` for {:.2?} due to '{}'",
                peer, duration, err.to_string()
            );
        },
        Err(e) => {
            warn!(target: LOG_TARGET, "Failed to ban sync peer `{peer}`: {e}");
        },
    }
}

/// Returns true if there is already a live connection to `node_id`.
pub(super) async fn is_connected(connectivity: &mut ConnectivityRequester, node_id: &NodeId) -> bool {
    matches!(
        connectivity.get_connection(node_id.clone(), RefKind::Weak).await,
        Ok(Some(conn)) if conn.is_connected()
    )
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
