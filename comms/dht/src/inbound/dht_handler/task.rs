// Copyright 2019, The Tari Project
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

use std::{convert::TryInto, sync::Arc};

use log::*;
use tari_comms::{
    message::MessageExt,
    peer_manager::{NodeId, NodeIdentity, PeerManager},
    pipeline::PipelineError,
    types::CommsPublicKey,
    OrNotFound,
};
use tari_utilities::{hex::Hex, ByteArray};
use tower::{Service, ServiceExt};

use crate::{
    actor::OffenceSeverity,
    discovery::DhtDiscoveryRequester,
    envelope::NodeDestination,
    inbound::{error::DhtInboundError, message::DecryptedDhtMessage},
    outbound::{OutboundMessageRequester, SendMessageParams},
    peer_validator::{DhtPeerValidatorError, PeerValidator},
    proto::{
        dht::{DiscoveryMessage, DiscoveryResponseMessage, JoinMessage},
        envelope::DhtMessageType,
    },
    rpc::UnvalidatedPeerInfo,
    DhtConfig,
    DhtRequester,
};

const LOG_TARGET: &str = "comms::dht::dht_handler";

pub struct ProcessDhtMessage<S> {
    next_service: S,
    peer_manager: Arc<PeerManager>,
    outbound_service: OutboundMessageRequester,
    node_identity: Arc<NodeIdentity>,
    dht: DhtRequester,
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
        dht: DhtRequester,
        discovery_requester: DhtDiscoveryRequester,
        message: DecryptedDhtMessage,
        config: Arc<DhtConfig>,
    ) -> Self {
        Self {
            next_service,
            peer_manager,
            outbound_service,
            node_identity,
            dht,
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

    #[allow(clippy::too_many_lines)]
    async fn handle_join(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        let DecryptedDhtMessage {
            decryption_result,
            dht_header,
            source_peer,
            authenticated_origin,
            is_saf_message,
            ..
        } = message;

        // Ban the source peer. They should not have propagated a DHT discover response.
        let Some(authenticated_pk) = authenticated_origin else {
            warn!(
                target: LOG_TARGET,
                "Received JoinMessage that did not have an authenticated origin from source peer {}. Banning source", source_peer
            );
            self.ban_peer( &source_peer.public_key,
                OffenceSeverity::Low,
                "Received JoinMessage that did not have an authenticated origin",
            ).await;
            return Ok(());
        };

        if authenticated_pk == *self.node_identity.public_key() {
            debug!(target: LOG_TARGET, "Received our own join message. Discarding it.");
            return Ok(());
        }

        let body = decryption_result.expect("already checked that this message decrypted successfully");
        let join_msg = self
            .ban_on_offence(
                &authenticated_pk,
                body.decode_part::<JoinMessage>(0)
                    .map_err(Into::into)
                    .and_then(|o| o.ok_or(DhtInboundError::InvalidMessageBody)),
            )
            .await?;

        if join_msg.public_key.as_slice() != authenticated_pk.as_bytes() {
            warn!(
                target: LOG_TARGET,
                "Received JoinMessage from peer that mismatches the authenticated origin. \
                This message was signed by another party which may be attempting to get other nodes banned. \
                Banning the message signer."
            );

            warn!(
                target: LOG_TARGET,
                "Authenticated origin: {:#.6}, Source: {:#.6}, join message: {}",
                authenticated_pk, source_peer.public_key, join_msg.public_key.to_hex()
            );
            self.ban_peer(
                &authenticated_pk,
                OffenceSeverity::High,
                "Received JoinMessage from peer with a public key that does not match the source peer",
            )
            .await;

            return Ok(());
        }

        debug!(
            target: LOG_TARGET,
            "Received join Message from '{}' {}", authenticated_pk, join_msg
        );

        let validator = PeerValidator::new(&self.config);
        let maybe_existing = self.peer_manager.find_by_public_key(&authenticated_pk).await?;
        let valid_peer = self
            .ban_on_offence(
                &authenticated_pk,
                validator
                    .validate_peer(join_msg.try_into()?, maybe_existing)
                    .map_err(Into::into),
            )
            .await?;

        let is_banned = valid_peer.is_banned();
        let valid_peer_node_id = valid_peer.node_id.clone();
        let valid_peer_public_key = valid_peer.public_key.clone();
        // Update peer details. If the peer is banned we preserve the ban but still allow them to update their claims.
        self.peer_manager.add_peer(valid_peer).await?;

        // DO NOT propagate this peer if this node has banned them
        if is_banned {
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

        // Only propagate a join that was not directly sent to this node
        if dht_header.destination != self.node_identity.public_key() {
            debug!(
                target: LOG_TARGET,
                "Propagating Join message from peer '{}'",
                valid_peer_node_id.short_str()
            );
            // Propagate message to closer peers
            self.outbound_service
                .send_raw_no_wait(
                    SendMessageParams::new()
                        .propagate(valid_peer_public_key.into(), vec![
                            valid_peer_node_id,
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

        // Ban the source peer. They should not have propagated a DHT discover response.
        let Some(authenticated_origin) = message.authenticated_origin.as_ref() else {
            warn!(
                target: LOG_TARGET,
                "Received DiscoveryResponseMessage that did not have an authenticated origin: {}. Banning source", message
            );
            self.ban_peer(
                &message.source_peer.public_key,
                OffenceSeverity::Low,
                "Received DiscoveryResponseMessage that did not have an authenticated origin",
            ).await;

            return Ok(());
        };

        let discover_msg = self
            .ban_on_offence(
                authenticated_origin,
                msg.decode_part::<DiscoveryResponseMessage>(0)
                    .map_err(Into::into)
                    .and_then(|o| o.ok_or(DhtInboundError::InvalidMessageBody)),
            )
            .await?;

        if *authenticated_origin != message.source_peer.public_key ||
            authenticated_origin.as_bytes() != discover_msg.public_key.as_slice()
        {
            warn!(
                target: LOG_TARGET,
                "Received DiscoveryResponseMessage from peer that mismatches the discovery response. \
                This message was signed by another party which may be attempting to get other nodes banned. \
                Banning the message signer."
            );

            warn!(
                target: LOG_TARGET,
                "Authenticated origin: {:#.6}, Source: {:#.6}, discovery message: {}",
                authenticated_origin, message.source_peer.public_key, discover_msg.public_key.to_hex()
            );
            self.ban_peer(
                authenticated_origin,
                OffenceSeverity::High,
                "Received DiscoveryResponseMessage from peer with a public key that does not match the source peer",
            )
            .await;

            return Ok(());
        }

        self.discovery_requester
            .notify_discovery_response_received(discover_msg)
            .await?;

        Ok(())
    }

    async fn handle_discover(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");

        let Some(authenticated_pk) = message.authenticated_origin.as_ref() else {
            warn!(
                target: LOG_TARGET,
                "Received Discover that did not have an authenticated origin from source peer {}. Banning source", message.source_peer
            );
            self.ban_peer(
                &message.source_peer.public_key,
                OffenceSeverity::Low,
                "Received JoinMessage that did not have an authenticated origin",
            ).await;

            return Ok(());
        };

        let discover_msg = self
            .ban_on_offence(
                authenticated_pk,
                msg.decode_part::<DiscoveryMessage>(0)
                    .map_err(Into::into)
                    .and_then(|o| o.ok_or(DhtInboundError::InvalidMessageBody)),
            )
            .await?;

        let nonce = discover_msg.nonce;

        debug!(
            target: LOG_TARGET,
            "Received discovery message from '{}', forwarded by {}", authenticated_pk, message.source_peer
        );

        let new_peer: UnvalidatedPeerInfo = self
            .ban_on_offence(
                authenticated_pk,
                discover_msg
                    .try_into()
                    .map_err(DhtInboundError::InvalidDiscoveryMessage),
            )
            .await?;
        let node_id = NodeId::from_public_key(&new_peer.public_key);

        let peer_validator = PeerValidator::new(&self.config);
        let maybe_existing_peer = self.peer_manager.find_by_public_key(&new_peer.public_key).await?;
        let peer = peer_validator.validate_peer(new_peer, maybe_existing_peer)?;
        self.peer_manager.add_peer(peer).await?;
        let origin_peer = self.peer_manager.find_by_node_id(&node_id).await.or_not_found()?;

        // Don't send a join request to the origin peer if they are banned
        if origin_peer.is_banned() {
            warn!(
                target: LOG_TARGET,
                "Received Discovery request for banned peer '{}'. Not propagating further.", node_id
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
                    .force_origin()
                    .finish(),
                response,
            )
            .await?;

        Ok(())
    }

    async fn ban_on_offence<T>(
        &mut self,
        authenticated_pk: &CommsPublicKey,
        result: Result<T, DhtInboundError>,
    ) -> Result<T, DhtInboundError> {
        match result {
            Ok(r) => Ok(r),
            Err(err) => {
                match &err {
                    DhtInboundError::PeerValidatorError(err) => match err {
                        DhtPeerValidatorError::Banned { .. } => {},
                        err @ DhtPeerValidatorError::ValidatorError(_) |
                        err @ DhtPeerValidatorError::IdentityTooManyClaims { .. } => {
                            self.ban_peer(authenticated_pk, OffenceSeverity::Medium, err).await;
                        },
                    },
                    err @ DhtInboundError::MessageError(_) | err @ DhtInboundError::InvalidMessageBody => {
                        self.ban_peer(authenticated_pk, OffenceSeverity::High, err).await;
                    },
                    DhtInboundError::PeerManagerError(_) => {},
                    DhtInboundError::DhtOutboundError(_) => {},
                    DhtInboundError::DhtDiscoveryError(_) => {},
                    DhtInboundError::OriginRequired(_) => {},
                    err @ DhtInboundError::InvalidDiscoveryMessage(_) => {
                        self.ban_peer(authenticated_pk, OffenceSeverity::High, err).await;
                    },
                    DhtInboundError::ConnectivityError(_) => {},
                }
                Err(err)
            },
        }
    }

    async fn ban_peer<T: ToString>(&mut self, authenticated_pk: &CommsPublicKey, severity: OffenceSeverity, reason: T) {
        if let Err(err) = self
            .dht
            .ban_peer(authenticated_pk.clone(), severity, reason.to_string())
            .await
        {
            error!(
                target: LOG_TARGET,
                "Could not ban peer '{}': {}", authenticated_pk, err
            );
        }
    }
}
