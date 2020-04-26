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

use crate::{
    config::DhtConfig,
    discovery::DhtDiscoveryRequester,
    envelope::NodeDestination,
    inbound::{error::DhtInboundError, message::DecryptedDhtMessage},
    outbound::{OutboundMessageRequester, SendMessageParams},
    proto::{
        dht::{DiscoveryMessage, DiscoveryResponseMessage, JoinMessage, RejectMessage},
        envelope::DhtMessageType,
    },
};
use log::*;
use std::sync::Arc;
use tari_comms::{
    message::MessageExt,
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags, PeerManager},
    pipeline::PipelineError,
    types::CommsPublicKey,
};
use tari_utilities::{hex::Hex, ByteArray};
use tower::{Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::dht_handler";

pub struct ProcessDhtMessage<S> {
    config: DhtConfig,
    next_service: S,
    peer_manager: Arc<PeerManager>,
    outbound_service: OutboundMessageRequester,
    node_identity: Arc<NodeIdentity>,
    message: Option<DecryptedDhtMessage>,
    discovery_requester: DhtDiscoveryRequester,
}

impl<S> ProcessDhtMessage<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    pub fn new(
        config: DhtConfig,
        next_service: S,
        peer_manager: Arc<PeerManager>,
        outbound_service: OutboundMessageRequester,
        node_identity: Arc<NodeIdentity>,
        discovery_requester: DhtDiscoveryRequester,
        message: DecryptedDhtMessage,
    ) -> Self
    {
        Self {
            config,
            next_service,
            peer_manager,
            outbound_service,
            node_identity,
            discovery_requester,
            message: Some(message),
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

        match message.dht_header.message_type {
            DhtMessageType::Join => self.handle_join(message).await.map_err(PipelineError::from_debug)?,
            DhtMessageType::Discovery => self.handle_discover(message).await.map_err(PipelineError::from_debug)?,
            DhtMessageType::DiscoveryResponse => self
                .handle_discover_response(message)
                .await
                .map_err(PipelineError::from_debug)?,
            DhtMessageType::RejectMsg => self
                .handle_message_reject(message)
                .await
                .map_err(PipelineError::from_debug)?,
            // Not a DHT message, call downstream middleware
            _ => {
                trace!(target: LOG_TARGET, "Passing message onto next service");
                self.next_service.oneshot(message).await?;
            },
        }

        Ok(())
    }

    async fn add_or_update_peer(
        &self,
        pubkey: &CommsPublicKey,
        node_id: NodeId,
        net_addresses: Vec<Multiaddr>,
        peer_features: PeerFeatures,
    ) -> Result<Peer, DhtInboundError>
    {
        let peer_manager = &self.peer_manager;
        // Add peer or modify existing peer using received join request
        if peer_manager.exists(pubkey).await {
            peer_manager
                .update_peer(
                    pubkey,
                    Some(node_id),
                    Some(net_addresses),
                    None,
                    None,
                    Some(false),
                    Some(peer_features),
                    None,
                    None,
                )
                .await?;
        } else {
            peer_manager
                .add_peer(Peer::new(
                    pubkey.clone(),
                    node_id,
                    net_addresses.into(),
                    PeerFlags::default(),
                    peer_features,
                    &[],
                ))
                .await?;
        }

        let peer = peer_manager.find_by_public_key(&pubkey).await?;

        Ok(peer)
    }

    fn validate_raw_node_id(&self, public_key: &CommsPublicKey, raw_node_id: &[u8]) -> Result<NodeId, DhtInboundError> {
        // The reason that we check the given node id against what we expect instead of just using the given node id
        // is in future the NodeId may not necessarily be derived from the public key (i.e. DAN node is registered on
        // the base layer)
        let expected_node_id = NodeId::from_key(public_key).map_err(|_| DhtInboundError::InvalidNodeId)?;
        let node_id = NodeId::from_bytes(raw_node_id).map_err(|_| DhtInboundError::InvalidNodeId)?;
        if expected_node_id == node_id {
            Ok(expected_node_id)
        } else {
            // TODO: Misbehaviour?
            Err(DhtInboundError::InvalidNodeId)
        }
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
            warn!(target: LOG_TARGET, "Received our own join message. Discarding it.");
            return Ok(());
        }

        let body = decryption_result.expect("already checked that this message decrypted successfully");
        let join_msg = body
            .decode_part::<JoinMessage>(0)?
            .ok_or_else(|| DhtInboundError::InvalidMessageBody)?;

        info!(
            target: LOG_TARGET,
            "Received join Message from '{}' {}", authenticated_pk, join_msg
        );

        let addresses = join_msg
            .addresses
            .into_iter()
            .filter_map(|addr| addr.parse().ok())
            .collect::<Vec<_>>();

        if addresses.is_empty() {
            return Err(DhtInboundError::InvalidAddresses);
        }

        let node_id = self.validate_raw_node_id(&authenticated_pk, &join_msg.node_id)?;

        let origin_peer = self
            .add_or_update_peer(
                &authenticated_pk,
                node_id,
                addresses,
                PeerFeatures::from_bits_truncate(join_msg.peer_features),
            )
            .await?;

        // DO NOT propagate this peer if this node has banned them
        if origin_peer.is_banned() {
            warn!(
                target: LOG_TARGET,
                "Received Join request for banned peer. This join request will not be propagated."
            );
            return Ok(());
        }

        // Send a join request back to the origin peer of the join request if:
        // - this join request was not sent directly from the origin peer but was forwarded (from the source peer), and
        // - that peer is from the same region of network.
        //
        // If it was not forwarded then we assume the source peer already has this node's details in
        // it's peer list.
        if source_peer.public_key != origin_peer.public_key &&
            self.peer_manager
                .in_network_region(
                    &origin_peer.node_id,
                    self.node_identity.node_id(),
                    self.config.num_neighbouring_nodes,
                )
                .await?
        {
            trace!(
                target: LOG_TARGET,
                "Sending Join to joining peer with public key '{}'",
                origin_peer.public_key
            );

            self.send_join_direct(origin_peer.public_key).await?;
        }

        if is_saf_message {
            debug!(
                target: LOG_TARGET,
                "Not re-propagating join message received from store and forward"
            );
            return Ok(());
        }

        debug!(
            target: LOG_TARGET,
            "Propagating join message to at most {} peer(s)", self.config.num_neighbouring_nodes
        );

        // Only propagate a join that was not directly sent to this node (presumably in response to a join this node
        // sent)
        // TODO: Join should have a response message type
        if dht_header.destination != self.node_identity.public_key() &&
            dht_header.destination != self.node_identity.node_id()
        {
            // Propagate message to closer peers
            self.outbound_service
                .send_raw(
                    SendMessageParams::new()
                        .closest(
                            origin_peer.node_id,
                            self.config.num_neighbouring_nodes,
                            vec![authenticated_pk, source_peer.public_key.clone()],
                            PeerFeatures::MESSAGE_PROPAGATION,
                        )
                        .with_dht_header(dht_header)
                        .finish(),
                    body.to_encoded_bytes(),
                )
                .await?;
        }

        Ok(())
    }

    async fn handle_message_reject(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        let body = message
            .decryption_result
            .expect("already checked that this message decrypted successfully");

        let reject_msg = body
            .decode_part::<RejectMessage>(0)?
            .ok_or_else(|| DhtInboundError::InvalidMessageBody)?;

        trace!(
            target: LOG_TARGET,
            "Received {} from '{}'",
            message.source_peer.node_id.short_str(),
            reject_msg,
        );

        // TODO: Perhaps we'll need some way to let the larger system know that the message was explicitly rejected

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
            .ok_or_else(|| DhtInboundError::InvalidMessageBody)?;

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
            .ok_or_else(|| DhtInboundError::InvalidMessageBody)?;

        let authenticated_pk = message.authenticated_origin.ok_or_else(|| {
            DhtInboundError::OriginRequired("Origin header required for Discovery message".to_string())
        })?;

        info!(
            target: LOG_TARGET,
            "Received discovery message from '{}', forwarded by {}", authenticated_pk, message.source_peer
        );

        let addresses = discover_msg
            .addresses
            .into_iter()
            .filter_map(|addr| addr.parse().ok())
            .collect::<Vec<_>>();

        if addresses.is_empty() {
            return Err(DhtInboundError::InvalidAddresses);
        }

        let node_id = self.validate_raw_node_id(&authenticated_pk, &discover_msg.node_id)?;
        let origin_peer = self
            .add_or_update_peer(
                &authenticated_pk,
                node_id,
                addresses,
                PeerFeatures::from_bits_truncate(discover_msg.peer_features),
            )
            .await?;

        // Don't send a join request to the origin peer if they are banned
        if origin_peer.is_banned() {
            warn!(
                target: LOG_TARGET,
                "Received Discovery request for banned peer '{}'. This request will be ignored.", authenticated_pk
            );
            return Ok(());
        }

        // Send the origin the current nodes latest contact info
        self.send_discovery_response(origin_peer.public_key, discover_msg.nonce)
            .await?;

        Ok(())
    }

    /// Send a network join update request directly to a specific known peer
    async fn send_join_direct(&mut self, dest_public_key: CommsPublicKey) -> Result<(), DhtInboundError> {
        let join_msg = JoinMessage::from(&self.node_identity);

        trace!(target: LOG_TARGET, "Sending direct join request to {}", dest_public_key);
        self.outbound_service
            .send_message_no_header(
                SendMessageParams::new()
                    .direct_public_key(dest_public_key.clone())
                    .with_destination(NodeDestination::PublicKey(Box::new(dest_public_key)))
                    .with_dht_message_type(DhtMessageType::Join)
                    .force_origin()
                    .finish(),
                join_msg,
            )
            .await?;

        Ok(())
    }

    /// Send a `DiscoveryResponseMessage` in response to a `DiscoveryMessage` to the given public key
    /// using the given nonce which should come from the `DiscoveryMessage`
    async fn send_discovery_response(
        &mut self,
        dest_public_key: CommsPublicKey,
        nonce: u64,
    ) -> Result<(), DhtInboundError>
    {
        let response = DiscoveryResponseMessage {
            node_id: self.node_identity.node_id().to_vec(),
            addresses: vec![self.node_identity.public_address().to_string()],
            peer_features: self.node_identity.features().bits(),
            nonce,
        };

        trace!(target: LOG_TARGET, "Sending discovery response to {}", dest_public_key);
        self.outbound_service
            .send_message_no_header(
                SendMessageParams::new()
                    .direct_public_key(dest_public_key)
                    .with_destination(NodeDestination::Unknown)
                    .with_dht_message_type(DhtMessageType::DiscoveryResponse)
                    .finish(),
                response,
            )
            .await?;

        Ok(())
    }
}
