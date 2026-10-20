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
    Bytes,
    BytesMut,
    OrNotFound,
    peer_manager::{NodeId, NodeIdentity, PeerManager},
    pipeline::PipelineError,
    types::CommsPublicKey,
};
use tari_utilities::{ByteArray, hex::Hex};
use tower::{Service, ServiceExt};

use crate::{
    DhtConfig,
    DhtRequester,
    actor::OffenceSeverity,
    discovery::DhtDiscoveryRequester,
    envelope::NodeDestination,
    inbound::{error::DhtInboundError, message::DecryptedDhtMessage},
    outbound::{OutboundMessageRequester, SendMessageParams},
    peer_validator::PeerValidator,
    proto::{
        DHT_MESSAGE_MAX_DECODE_ITEMS,
        dht::{DiscoveryMessage, DiscoveryResponseMessage, JoinMessage},
        envelope::DhtMessageType,
    },
    rpc::UnvalidatedPeerInfo,
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
            raw_body,
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
                "Received JoinMessage that did not have an authenticated origin from source peer {source_peer}. Banning source"
            );

            self.dht
                .ban_peer(
                    source_peer.public_key.clone(),
                    OffenceSeverity::Low,
                    "Received JoinMessage that did not have an authenticated origin",
                )
                .await;
            return Ok(());
        };

        // Honest nodes address a Join to their own key. Holding every Join to that also fixes its fan-out.
        if dht_header.destination != authenticated_pk {
            debug!(
                target: LOG_TARGET,
                "Received JoinMessage from '{authenticated_pk}' addressed to {} instead of its signer. Discarding it.",
                dht_header.destination
            );
            return Ok(());
        }

        if authenticated_pk == *self.node_identity.public_key() {
            debug!(target: LOG_TARGET, "Received our own join message. Discarding it.");
            return Ok(());
        }

        let body = decryption_result.expect("already checked that this message decrypted successfully");
        let Some(join_msg) = self
            .ban_on_offence(
                &authenticated_pk,
                body.decode_part_with_max_items::<JoinMessage>(0, DHT_MESSAGE_MAX_DECODE_ITEMS)
                    .map_err(Into::into)
                    .and_then(|o| o.ok_or(DhtInboundError::InvalidMessageBody)),
            )
            .await?
        else {
            return Ok(());
        };

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
            self.dht
                .ban_peer(
                    authenticated_pk,
                    OffenceSeverity::High,
                    "Received JoinMessage from peer with a public key that does not match the source peer",
                )
                .await;

            return Ok(());
        }

        debug!(
            target: LOG_TARGET,
            "Received join Message from '{authenticated_pk}' {join_msg}"
        );

        let Some(new_peer) = self
            .ban_on_offence(
                &authenticated_pk,
                join_msg.try_into().map_err(DhtInboundError::InvalidDiscoveryMessage),
            )
            .await?
        else {
            return Ok(());
        };
        let validator = PeerValidator::new(&self.config);
        let maybe_existing = self.peer_manager.find_by_public_key(&authenticated_pk).await?;
        let Some(valid_peer) = self
            .ban_on_offence(
                &authenticated_pk,
                validator.validate_peer(new_peer, maybe_existing).map_err(Into::into),
            )
            .await?
        else {
            return Ok(());
        };

        let is_banned = valid_peer.is_banned();
        let valid_peer_node_id = valid_peer.node_id.clone();
        let valid_peer_public_key = valid_peer.public_key.clone();
        // Update peer details. If the peer is banned we preserve the ban but still allow them to update their claims.
        self.peer_manager.add_or_update_peer(valid_peer).await?;

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
                    BytesMut::from(Bytes::from(raw_body)),
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
                "Received DiscoveryResponseMessage that did not have an authenticated origin: {message}. Banning source"
            );
            self.dht
                .ban_peer(
                    message.source_peer.public_key.clone(),
                    OffenceSeverity::Low,
                    "Received DiscoveryResponseMessage that did not have an authenticated origin",
                )
                .await;

            return Ok(());
        };

        // A DiscoveryResponse is never forwarded, so only its signer may deliver it. The source delivered one signed
        // by someone else (for example a replay of another node's response): the source is to blame, not the signer.
        if *authenticated_origin != message.source_peer.public_key {
            warn!(
                target: LOG_TARGET,
                "Received DiscoveryResponseMessage signed by {:#.6} from source {:#.6}, which did not sign it. Banning \
                 the source.",
                authenticated_origin, message.source_peer.public_key
            );
            self.dht
                .ban_peer(
                    message.source_peer.public_key.clone(),
                    OffenceSeverity::Medium,
                    "Delivered a DiscoveryResponse signed by another peer",
                )
                .await;

            return Ok(());
        }

        let Some(discover_msg) = self
            .ban_on_offence(
                authenticated_origin,
                msg.decode_part_with_max_items::<DiscoveryResponseMessage>(0, DHT_MESSAGE_MAX_DECODE_ITEMS)
                    .map_err(Into::into)
                    .and_then(|o| o.ok_or(DhtInboundError::InvalidMessageBody)),
            )
            .await?
        else {
            return Ok(());
        };

        if authenticated_origin.as_bytes() != discover_msg.public_key.as_slice() {
            warn!(
                target: LOG_TARGET,
                "Received DiscoveryResponseMessage signed by {:#.6} that claims to be for {}. Banning the message \
                 signer.",
                authenticated_origin, discover_msg.public_key.to_hex()
            );
            self.dht
                .ban_peer(
                    authenticated_origin.clone(),
                    OffenceSeverity::High,
                    "Signed a DiscoveryResponseMessage with a public key that is not its own",
                )
                .await;

            return Ok(());
        }

        self.discovery_requester
            .notify_discovery_response_received(discover_msg)
            .await?;

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_discover(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");

        let Some(authenticated_pk) = message.authenticated_origin.as_ref() else {
            warn!(
                target: LOG_TARGET,
                "Received Discover that did not have an authenticated origin from source peer {}. Banning source", message.source_peer
            );
            self.dht
                .ban_peer(
                    message.source_peer.public_key.clone(),
                    OffenceSeverity::Low,
                    "Received JoinMessage that did not have an authenticated origin",
                )
                .await;

            return Ok(());
        };

        let Some(discover_msg) = self
            .ban_on_offence(
                authenticated_pk,
                msg.decode_part_with_max_items::<DiscoveryMessage>(0, DHT_MESSAGE_MAX_DECODE_ITEMS)
                    .map_err(Into::into)
                    .and_then(|o| o.ok_or(DhtInboundError::InvalidMessageBody)),
            )
            .await?
        else {
            return Ok(());
        };

        if discover_msg.public_key.as_slice() != authenticated_pk.as_bytes() {
            warn!(
                target: LOG_TARGET,
                "Received DiscoveryMessage signed by {:#.6} that claims to be from {}. Banning the message signer.",
                authenticated_pk, discover_msg.public_key.to_hex()
            );
            self.dht
                .ban_peer(
                    authenticated_pk.clone(),
                    OffenceSeverity::High,
                    "Signed a DiscoveryMessage with a public key that is not its own",
                )
                .await;
            return Ok(());
        }

        let nonce = discover_msg.nonce;

        debug!(
            target: LOG_TARGET,
            "Received discovery message from '{}', forwarded by {}", authenticated_pk, message.source_peer
        );

        let Some(new_peer): Option<UnvalidatedPeerInfo> = self
            .ban_on_offence(
                authenticated_pk,
                discover_msg
                    .try_into()
                    .map_err(DhtInboundError::InvalidDiscoveryMessage),
            )
            .await?
        else {
            return Ok(());
        };
        if new_peer.public_key != *authenticated_pk {
            warn!(
                target: LOG_TARGET,
                "Received DiscoveryMessage signed by {:#.6} for peer {:#.6}. Banning the message signer.",
                authenticated_pk, new_peer.public_key
            );
            self.dht
                .ban_peer(
                    authenticated_pk.clone(),
                    OffenceSeverity::High,
                    "Signed a DiscoveryMessage with a public key that is not its own",
                )
                .await;
            return Ok(());
        }
        let node_id = NodeId::from_public_key(&new_peer.public_key);

        let peer_validator = PeerValidator::new(&self.config);
        let maybe_existing_peer = self.peer_manager.find_by_public_key(&new_peer.public_key).await?;
        let Some(peer) = self
            .ban_on_offence(
                authenticated_pk,
                peer_validator
                    .validate_peer(new_peer, maybe_existing_peer)
                    .map_err(Into::into),
            )
            .await?
        else {
            return Ok(());
        };
        self.peer_manager.add_or_update_peer(peer).await?;
        let origin_peer = self
            .peer_manager
            .find_by_node_id(&node_id)
            .await
            .or_not_found(&node_id)?;

        // Don't send a join request to the origin peer if they are banned
        if origin_peer.is_banned() {
            warn!(
                target: LOG_TARGET,
                "Received Discovery request for banned peer '{node_id}'. Not propagating further."
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

        trace!(target: LOG_TARGET, "Sending discovery response to {dest_public_key}");
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

    /// Bans the signer of a message that honest software cannot produce: one that does not decode, or a claim whose
    /// identity signature fails. Anything else (a claim that parses differently on another version, or is over this
    /// node's limits) is dropped without a ban.
    /// Bans the signer of a message that honest software cannot produce: one that does not decode, or a claim whose
    /// identity signature fails. Anything else (a claim that parses differently on another version, or is over this
    /// node's limits) is dropped without a ban and returns `Ok(None)`.
    async fn ban_on_offence<T>(
        &mut self,
        authenticated_pk: &CommsPublicKey,
        result: Result<T, DhtInboundError>,
    ) -> Result<Option<T>, DhtInboundError> {
        let err = match result {
            Ok(r) => return Ok(Some(r)),
            Err(err) => err,
        };
        match &err {
            // The message decoded, but the claim it carries failed validation
            DhtInboundError::PeerValidatorError(err) if err.is_ban_offence() => {
                self.dht
                    .ban_peer(authenticated_pk.clone(), OffenceSeverity::Medium, err)
                    .await;
            },
            DhtInboundError::PeerValidatorError(err) => {
                debug!(target: LOG_TARGET, "Dropping peer claim from {authenticated_pk:#.6}: {err}");
                return Ok(None);
            },
            // The message decoded, but its claim could not be converted (for example an address protocol this node
            // does not know)
            DhtInboundError::InvalidDiscoveryMessage(err) => {
                debug!(target: LOG_TARGET, "Dropping peer claim from {authenticated_pk:#.6}: {err}");
                return Ok(None);
            },
            // The message did not decode
            err @ DhtInboundError::MessageError(_) | err @ DhtInboundError::InvalidMessageBody => {
                self.dht
                    .ban_peer(authenticated_pk.clone(), OffenceSeverity::High, err)
                    .await;
            },
            DhtInboundError::PeerManagerError(_) => {},
            DhtInboundError::DhtOutboundError(_) => {},
            DhtInboundError::DhtDiscoveryError(_) => {},
            DhtInboundError::OriginRequired(_) => {},
            DhtInboundError::ConnectivityError(_) => {},
        }
        Err(err)
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use std::time::Duration;

    use tari_comms::{
        message::{MessageExt, MessageTag},
        multiaddr::Multiaddr,
        peer_manager::{NodeIdentity, PeerFeatures},
        wrap_in_envelope_body,
    };
    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        actor::DhtRequest,
        envelope::{DhtMessageFlags, DhtMessageHeader},
        inbound::DhtInboundMessage,
        outbound::DhtOutboundRequest,
        test_utils::{build_peer_manager, make_node_identity, service_spy},
        version::DhtProtocolVersion,
    };

    struct Received {
        result: Result<(), PipelineError>,
        bans: Vec<(CommsPublicKey, OffenceSeverity)>,
        outbound_sends: usize,
        outbound_bodies: Vec<BytesMut>,
        discovery_notifications: usize,
        peer_manager: Arc<PeerManager>,
    }

    /// Runs `message` through the DHT handler of a fresh node and returns the bans, sends and discovery notifications
    /// it caused
    async fn process(config: DhtConfig, message: DecryptedDhtMessage) -> Received {
        let (dht_tx, mut dht_rx) = mpsc::unbounded_channel();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();
        let (discovery_tx, mut discovery_rx) = mpsc::channel(10);
        let spy = service_spy();
        let peer_manager = build_peer_manager();
        let task = ProcessDhtMessage::new(
            spy.to_service::<PipelineError>(),
            peer_manager.clone(),
            OutboundMessageRequester::new(outbound_tx),
            make_node_identity(),
            DhtRequester::new(dht_tx),
            DhtDiscoveryRequester::new(discovery_tx, Duration::from_secs(10)),
            message,
            Arc::new(config),
        );
        let result = task.run().await;

        let mut bans = Vec::new();
        while let Ok(request) = dht_rx.try_recv() {
            if let DhtRequest::BanPeer {
                public_key, severity, ..
            } = request
            {
                bans.push((public_key, severity));
            }
        }
        let mut outbound_sends = 0usize;
        let mut outbound_bodies = Vec::new();
        while let Ok(DhtOutboundRequest::SendMessage(_, body, _)) = outbound_rx.try_recv() {
            outbound_sends = outbound_sends.saturating_add(1);
            outbound_bodies.push(body);
        }
        let mut discovery_notifications = 0usize;
        while discovery_rx.try_recv().is_ok() {
            discovery_notifications = discovery_notifications.saturating_add(1);
        }
        Received {
            result,
            bans,
            outbound_sends,
            outbound_bodies,
            discovery_notifications,
            peer_manager,
        }
    }

    /// A decrypted message of `message_type` signed by `signer`, addressed to the signer and delivered by `source`
    fn make_message<T: prost::Message>(
        source: &NodeIdentity,
        signer: &NodeIdentity,
        message_type: DhtMessageType,
        body: T,
    ) -> DecryptedDhtMessage {
        let header = DhtMessageHeader {
            version: DhtProtocolVersion::latest(),
            destination: signer.public_key().clone().into(),
            message_signature: Vec::new(),
            ephemeral_public_key: None,
            message_type,
            flags: DhtMessageFlags::NONE,
            message_tag: MessageTag::new(),
            expires: None,
        };
        let body = wrap_in_envelope_body!(body);
        let inbound = DhtInboundMessage::new(
            MessageTag::new(),
            header,
            Arc::new(source.to_peer()),
            body.to_encoded_bytes(),
        );
        DecryptedDhtMessage::succeeded(body, Some(signer.public_key().clone()), inbound)
    }

    fn identity_with_addresses(addresses: &[&str]) -> Arc<NodeIdentity> {
        let addresses = addresses.iter().map(|a| a.parse::<Multiaddr>().unwrap()).collect();
        Arc::new(NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            addresses,
            PeerFeatures::COMMUNICATION_NODE,
        ))
    }

    fn discovery_response_from(identity: &NodeIdentity) -> DiscoveryResponseMessage {
        DiscoveryResponseMessage {
            public_key: identity.public_key().to_vec(),
            addresses: identity.public_addresses().iter().map(|a| a.to_vec()).collect(),
            peer_features: identity.features().bits(),
            nonce: 1,
            identity_signature: identity.identity_signature_read().as_ref().map(Into::into),
        }
    }

    fn discovery_from(identity: &NodeIdentity) -> DiscoveryMessage {
        DiscoveryMessage {
            public_key: identity.public_key().to_vec(),
            addresses: identity.public_addresses().iter().map(|a| a.to_vec()).collect(),
            peer_features: identity.features().bits(),
            nonce: 1,
            identity_signature: identity.identity_signature_read().as_ref().map(Into::into),
        }
    }

    #[tokio::test]
    async fn a_discovery_response_delivered_by_another_peer_bans_the_source_not_the_signer() {
        let signer = make_node_identity();
        let source = make_node_identity();
        let message = make_message(
            &source,
            &signer,
            DhtMessageType::DiscoveryResponse,
            discovery_response_from(&signer),
        );

        let received = process(DhtConfig::default_local_test(), message).await;
        assert_eq!(received.bans.len(), 1);
        assert_eq!(received.bans[0].0, *source.public_key());
        assert!(matches!(received.bans[0].1, OffenceSeverity::Medium));
        assert_eq!(received.discovery_notifications, 0);
    }

    #[tokio::test]
    async fn a_discovery_response_claiming_another_key_bans_the_signer() {
        let signer = make_node_identity();
        let other = make_node_identity();
        let message = make_message(
            &signer,
            &signer,
            DhtMessageType::DiscoveryResponse,
            discovery_response_from(&other),
        );

        let received = process(DhtConfig::default_local_test(), message).await;
        assert_eq!(received.bans.len(), 1);
        assert_eq!(received.bans[0].0, *signer.public_key());
        assert!(matches!(received.bans[0].1, OffenceSeverity::High));
        assert_eq!(received.discovery_notifications, 0);
    }

    #[tokio::test]
    async fn a_valid_discovery_response_is_passed_to_the_discovery_service() {
        let signer = make_node_identity();
        let message = make_message(
            &signer,
            &signer,
            DhtMessageType::DiscoveryResponse,
            discovery_response_from(&signer),
        );

        let received = process(DhtConfig::default_local_test(), message).await;
        assert!(received.bans.is_empty());
        assert_eq!(received.discovery_notifications, 1);
    }

    #[tokio::test]
    async fn a_discovery_claiming_another_key_bans_the_signer_and_is_not_answered() {
        let signer = make_node_identity();
        let relay = make_node_identity();
        let other = make_node_identity();
        let message = make_message(&relay, &signer, DhtMessageType::Discovery, discovery_from(&other));

        let received = process(DhtConfig::default_local_test(), message).await;
        assert_eq!(received.bans.len(), 1);
        assert_eq!(received.bans[0].0, *signer.public_key());
        assert!(matches!(received.bans[0].1, OffenceSeverity::High));
        assert_eq!(received.outbound_sends, 0);
    }

    #[tokio::test]
    async fn a_valid_discovery_is_answered() {
        let signer = make_node_identity();
        let message = make_message(&signer, &signer, DhtMessageType::Discovery, discovery_from(&signer));

        let received = process(DhtConfig::default_local_test(), message).await;
        assert!(received.bans.is_empty());
        assert_eq!(received.outbound_sends, 1);
    }

    #[tokio::test]
    async fn a_discovery_with_a_claim_over_this_nodes_limits_is_dropped_without_a_ban() {
        let signer = identity_with_addresses(&[
            "/ip4/23.23.23.1/tcp/18189",
            "/ip4/23.23.23.2/tcp/18189",
            "/ip4/23.23.23.3/tcp/18189",
        ]);
        let mut config = DhtConfig::default_local_test();
        config.peer_validator_config.max_permitted_peer_addresses_per_claim = 2;
        let message = make_message(&signer, &signer, DhtMessageType::Discovery, discovery_from(&signer));

        let received = process(config, message).await;
        assert!(received.result.is_ok());
        assert!(received.bans.is_empty());
        assert_eq!(received.outbound_sends, 0);
    }

    #[tokio::test]
    async fn a_join_with_too_many_claims_or_addresses_is_dropped_without_a_ban() {
        let relay = make_node_identity();
        let signer = identity_with_addresses(&[
            "/ip4/23.23.23.1/tcp/18189",
            "/ip4/23.23.23.2/tcp/18189",
            "/ip4/23.23.23.3/tcp/18189",
        ]);

        let mut config = DhtConfig::default_local_test();
        config.max_permitted_peer_claims = 0;
        let message = make_message(&relay, &signer, DhtMessageType::Join, JoinMessage::from(&signer));
        let received = process(config, message).await;
        assert!(received.result.is_ok());
        assert!(received.bans.is_empty());

        let mut config = DhtConfig::default_local_test();
        config.peer_validator_config.max_permitted_peer_addresses_per_claim = 2;
        let message = make_message(&relay, &signer, DhtMessageType::Join, JoinMessage::from(&signer));
        let received = process(config, message).await;
        assert!(received.result.is_ok());
        assert!(received.bans.is_empty());
    }

    #[tokio::test]
    async fn a_join_with_no_usable_addresses_is_dropped_without_a_ban() {
        let relay = make_node_identity();
        let signer = identity_with_addresses(&["/ip4/127.0.0.1/tcp/18189"]);
        let mut config = DhtConfig::default_local_test();
        config.peer_validator_config.allow_test_addresses = false;
        let message = make_message(&relay, &signer, DhtMessageType::Join, JoinMessage::from(&signer));

        let received = process(config, message).await;
        assert!(received.result.is_ok());
        assert!(received.bans.is_empty());
        assert_eq!(received.outbound_sends, 0);
    }

    #[tokio::test]
    async fn a_join_whose_claim_does_not_convert_is_dropped_without_a_ban() {
        let relay = make_node_identity();
        let signer = identity_with_addresses(&["/ip4/23.23.23.1/tcp/18189"]);
        let mut join = JoinMessage::from(&signer);
        // An address protocol this node does not know
        join.addresses.push(vec![0xff, 0xff, 0xff]);
        let message = make_message(&relay, &signer, DhtMessageType::Join, join);

        let received = process(DhtConfig::default_local_test(), message).await;
        assert!(received.result.is_ok());
        assert!(received.bans.is_empty());
        assert_eq!(received.outbound_sends, 0);
    }

    #[tokio::test]
    async fn a_join_with_a_bad_identity_signature_bans_the_signer() {
        let relay = make_node_identity();
        let signer = identity_with_addresses(&["/ip4/23.23.23.1/tcp/18189"]);
        let mut join = JoinMessage::from(&signer);
        // Still a parseable address, but not the one the identity signature covers
        join.addresses = vec!["/ip4/23.23.23.2/tcp/18189".parse::<Multiaddr>().unwrap().to_vec()];
        let message = make_message(&relay, &signer, DhtMessageType::Join, join);

        let received = process(DhtConfig::default_local_test(), message).await;
        assert_eq!(received.bans.len(), 1);
        assert_eq!(received.bans[0].0, *signer.public_key());
        assert!(matches!(received.bans[0].1, OffenceSeverity::Medium));
    }

    #[tokio::test]
    async fn a_join_not_addressed_to_its_signer_is_dropped_without_a_ban() {
        let relay = make_node_identity();
        let signer = identity_with_addresses(&["/ip4/23.23.23.1/tcp/18189"]);
        let other = make_node_identity();

        for destination in [NodeDestination::Unknown, other.public_key().clone().into()] {
            let mut message = make_message(&relay, &signer, DhtMessageType::Join, JoinMessage::from(&signer));
            message.dht_header.destination = destination;

            let received = process(DhtConfig::default_local_test(), message).await;
            assert!(received.result.is_ok());
            assert!(received.bans.is_empty());
            assert_eq!(received.outbound_sends, 0);
            assert!(!received.peer_manager.exists(signer.public_key()).await.unwrap());
        }
    }

    #[tokio::test]
    async fn a_join_addressed_to_its_signer_is_stored_and_propagated_as_received() {
        let relay = make_node_identity();
        let signer = identity_with_addresses(&["/ip4/23.23.23.1/tcp/18189"]);
        let mut message = make_message(&relay, &signer, DhtMessageType::Join, JoinMessage::from(&signer));
        // Unknown field 2 (varint) = 1: the signature covers these bytes, so they must be relayed unchanged
        message.raw_body.extend_from_slice(&[0x10, 0x01]);
        let raw_body = message.raw_body.clone();

        let received = process(DhtConfig::default_local_test(), message).await;
        assert!(received.result.is_ok());
        assert!(received.bans.is_empty());
        assert!(received.peer_manager.exists(signer.public_key()).await.unwrap());
        assert_eq!(received.outbound_bodies.len(), 1);
        assert_eq!(received.outbound_bodies[0].to_vec(), raw_body);
    }
}
