// Copyright 2019, The Taiji Project
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

use std::{
    convert::{TryFrom, TryInto},
    sync::Arc,
};

use log::*;
use taiji_comms::{
    message::MessageExt,
    multiaddr::Multiaddr,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{
        IdentitySignature,
        NodeId,
        NodeIdentity,
        Peer,
        PeerFeatures,
        PeerFlags,
        PeerIdentityClaim,
        PeerManager,
    },
    pipeline::PipelineError,
    types::CommsPublicKey,
    OrNotFound,
};
use tari_utilities::{hex::Hex, ByteArray};
use tower::{Service, ServiceExt};

use crate::{
    discovery::DhtDiscoveryRequester,
    envelope::NodeDestination,
    inbound::{error::DhtInboundError, message::DecryptedDhtMessage},
    outbound::{OutboundMessageRequester, SendMessageParams},
    peer_validator::PeerValidator,
    proto::{
        dht::{DiscoveryMessage, DiscoveryResponseMessage, JoinMessage},
        envelope::DhtMessageType,
    },
    rpc::PeerInfo,
    DhtConfig,
};

const LOG_TARGET: &str = "comms::dht::dht_handler";

pub struct ProcessDhtMessage<S> {
    next_service: S,
    peer_manager: Arc<PeerManager>,
    outbound_service: OutboundMessageRequester,
    node_identity: Arc<NodeIdentity>,
    message: Option<DecryptedDhtMessage>,
    discovery_requester: DhtDiscoveryRequester,
    config: Arc<DhtConfig>,
}

impl<S> ProcessDhtMessage<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    pub fn new(
        next_service: S,
        peer_manager: Arc<PeerManager>,
        outbound_service: OutboundMessageRequester,
        node_identity: Arc<NodeIdentity>,
        discovery_requester: DhtDiscoveryRequester,
        message: DecryptedDhtMessage,
        config: Arc<DhtConfig>,
    ) -> Self {
        Self {
            next_service,
            peer_manager,
            outbound_service,
            node_identity,
            discovery_requester,
            message: Some(message),
            config,
        }
    }

    pub async fn run(mut self) -> Result<(), PipelineError> {
        let message = self
            .message
            .take()
            .expect("ProcessDhtMessage initialized without message");

        // If this message failed to decrypt, we stop it going further at this layer
        if message.decryption_failed() {
            debug!(
                target: LOG_TARGET,
                "Message that failed to decrypt will be discarded here. DhtHeader={}", message.dht_header
            );
            return Ok(());
        }

        if message.is_duplicate() {
            debug!(
                target: LOG_TARGET,
                "Received message ({}) that has already been received {} time(s). Last sent by peer '{}', passing on \
                 to next service (Trace: {})",
                message.tag,
                message.dedup_hit_count,
                message.source_peer.node_id.short_str(),
                message.dht_header.message_tag,
            );
            self.next_service.oneshot(message).await?;
            return Ok(());
        }

        trace!(
            target: LOG_TARGET,
            "Received DHT message type `{}` (Source peer: {}, Tag: {}, Trace: {})",
            message.dht_header.message_type,
            message.source_peer.node_id,
            message.tag,
            message.dht_header.message_tag
        );
        match message.dht_header.message_type {
            DhtMessageType::Join => self.handle_join(message).await?,
            DhtMessageType::Discovery => self.handle_discover(message).await?,
            DhtMessageType::DiscoveryResponse => self.handle_discover_response(message).await?,
            // Not a DHT message, call downstream middleware
            _ => {
                trace!(
                    target: LOG_TARGET,
                    "Passing message {} onto next service (Trace: {})",
                    message.tag,
                    message.dht_header.message_tag
                );
                self.next_service.oneshot(message).await?;
            },
        }

        Ok(())
    }

    async fn handle_join(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        let DecryptedDhtMessage {
            decryption_result,
            dht_header,
            source_peer,
            authenticated_origin,
            is_saf_message,
            ..
        } = message;

        let authenticated_pk = authenticated_origin.ok_or_else(|| {
            DhtInboundError::OriginRequired("Authenticated origin is required for this message type".to_string())
        })?;

        if &authenticated_pk == self.node_identity.public_key() {
            debug!(target: LOG_TARGET, "Received our own join message. Discarding it.");
            return Ok(());
        }

        let body = decryption_result.expect("already checked that this message decrypted successfully");
        let join_msg = body
            .decode_part::<JoinMessage>(0)?
            .ok_or(DhtInboundError::InvalidMessageBody)?;

        debug!(
            target: LOG_TARGET,
            "Received join Message from '{}' {}", authenticated_pk, join_msg
        );

        let addresses = join_msg
            .addresses
            .iter()
            .filter_map(|addr| Multiaddr::try_from(addr.clone()).ok())
            .collect::<Vec<_>>();

        if addresses.is_empty() {
            return Err(DhtInboundError::InvalidAddresses);
        }
        let node_id = NodeId::from_public_key(&authenticated_pk);

        let features = PeerFeatures::from_bits_truncate(join_msg.peer_features);

        let identity_signature: IdentitySignature = join_msg
            .identity_signature
            .map(IdentitySignature::try_from)
            .transpose()
            .map_err(|err| DhtInboundError::InvalidPeerIdentitySignature(err.to_string()))?
            .ok_or(DhtInboundError::NoPeerIdentitySignature)?;

        let peer_identity_claim = PeerIdentityClaim::new(addresses.clone(), features, identity_signature, None);

        let new_peer = Peer::new(
            authenticated_pk,
            node_id.clone(),
            MultiaddressesWithStats::from_addresses_with_source(addresses, &PeerAddressSource::FromJoinMessage {
                peer_identity_claim,
            }),
            PeerFlags::empty(),
            features,
            vec![],
            String::new(),
        );

        self.peer_manager.add_peer(new_peer.clone()).await?;
        let origin_peer = self.peer_manager.find_by_node_id(&node_id).await.or_not_found()?;

        // DO NOT propagate this peer if this node has banned them
        if origin_peer.is_banned() {
            debug!(
                target: LOG_TARGET,
                "Received Join request for banned peer. This join request will not be propagated."
            );
            return Ok(());
        }

        if is_saf_message {
            debug!(
                target: LOG_TARGET,
                "Not re-propagating join message received from store and forward"
            );
            return Ok(());
        }

        let origin_public_key = origin_peer.public_key;

        // Only propagate a join that was not directly sent to this node
        if dht_header.destination != self.node_identity.public_key() {
            debug!(
                target: LOG_TARGET,
                "Propagating Join message from peer '{}'",
                origin_peer.node_id.short_str()
            );
            // Propagate message to closer peers
            self.outbound_service
                .send_raw_no_wait(
                    SendMessageParams::new()
                        .propagate(origin_public_key.clone().into(), vec![
                            origin_peer.node_id,
                            source_peer.node_id.clone(),
                        ])
                        .with_debug_info("Propagating join message".to_string())
                        .with_dht_header(dht_header)
                        .finish(),
                    body.encode_into_bytes_mut(),
                )
                .await?;
        }

        Ok(())
    }

    async fn handle_discover_response(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        trace!(
            target: LOG_TARGET,
            "Received Discover Response Message from {}",
            message
                .authenticated_origin
                .as_ref()
                .map(|pk| pk.to_hex())
                .unwrap_or_else(|| "<unknown>".to_string())
        );

        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");

        let discover_msg = msg
            .decode_part::<DiscoveryResponseMessage>(0)?
            .ok_or(DhtInboundError::InvalidMessageBody)?;

        self.discovery_requester
            .notify_discovery_response_received(discover_msg)
            .await?;

        Ok(())
    }

    async fn handle_discover(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");

        let discover_msg = msg
            .decode_part::<DiscoveryMessage>(0)?
            .ok_or(DhtInboundError::InvalidMessageBody)?;

        let nonce = discover_msg.nonce;
        let authenticated_pk = message.authenticated_origin.ok_or_else(|| {
            DhtInboundError::OriginRequired("Origin header required for Discovery message".to_string())
        })?;

        debug!(
            target: LOG_TARGET,
            "Received discovery message from '{}', forwarded by {}", authenticated_pk, message.source_peer
        );

        let new_peer: PeerInfo = discover_msg
            .try_into()
            .map_err(DhtInboundError::InvalidDiscoveryMessage)?;
        let node_id = NodeId::from_public_key(&new_peer.public_key);

        let peer_validator = PeerValidator::new(&self.peer_manager, &self.config);
        peer_validator.validate_and_add_peer(new_peer).await?;
        let origin_peer = self.peer_manager.find_by_node_id(&node_id).await.or_not_found()?;

        // Don't send a join request to the origin peer if they are banned
        if origin_peer.is_banned() {
            warn!(
                target: LOG_TARGET,
                "Received Discovery request for banned peer '{}'. This request will be ignored.", node_id
            );
            return Ok(());
        }

        // Send the origin the current nodes latest contact info
        self.send_discovery_response(origin_peer.public_key, nonce).await?;

        Ok(())
    }

    /// Send a `DiscoveryResponseMessage` in response to a `DiscoveryMessage` to the given public key
    /// using the given nonce which should come from the `DiscoveryMessage`
    async fn send_discovery_response(
        &mut self,
        dest_public_key: CommsPublicKey,
        nonce: u64,
    ) -> Result<(), DhtInboundError> {
        let response = DiscoveryResponseMessage {
            public_key: self.node_identity.public_key().to_vec(),
            addresses: self
                .node_identity
                .public_addresses()
                .iter()
                .map(|a| a.to_vec())
                .collect(),
            peer_features: self.node_identity.features().bits(),
            nonce,
            identity_signature: self.node_identity.identity_signature_read().as_ref().map(Into::into),
        };

        trace!(target: LOG_TARGET, "Sending discovery response to {}", dest_public_key);
        self.outbound_service
            .send_message_no_header_no_wait(
                SendMessageParams::new()
                    .direct_public_key(dest_public_key)
                    .with_debug_info("Sending discovery response".to_string())
                    .with_destination(NodeDestination::Unknown)
                    .with_dht_message_type(DhtMessageType::DiscoveryResponse)
                    .finish(),
                response,
            )
            .await?;

        Ok(())
    }
}
