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
    outbound::{OutboundEncryption, OutboundMessageRequester, SendMessageParams},
    proto::{
        dht::{DiscoveryMessage, DiscoveryResponseMessage, JoinMessage},
        envelope::DhtMessageType,
    },
};
use log::*;
use std::sync::Arc;
use tari_comms::{
    message::MessageExt,
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags, PeerManager},
    types::CommsPublicKey,
};
use tari_comms_middleware::MiddlewareError;
use tari_utilities::ByteArray;
use tower::{Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::dht::dht_handler";

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
where
    S: Service<DecryptedDhtMessage, Response = ()>,
    S::Error: Into<MiddlewareError>,
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

    pub async fn run(mut self) -> Result<(), MiddlewareError> {
        let message = self
            .message
            .take()
            .expect("ProcessDhtMessage initialized without message");

        // If this message failed to decrypt, this middleware is not interested in it
        if message.decryption_failed() {
            self.next_service.oneshot(message).await.map_err(Into::into)?;
            return Ok(());
        }

        match message.dht_header.message_type {
            DhtMessageType::Join => self.handle_join(message).await?,
            DhtMessageType::Discovery => self.handle_discover(message).await?,
            DhtMessageType::DiscoveryResponse => self.handle_discover_response(message).await?,
            // Not a DHT message, call downstream middleware
            _ => {
                trace!(target: LOG_TARGET, "Passing message onto next service");
                self.next_service.oneshot(message).await.map_err(Into::into)?
            },
        }

        Ok(())
    }

    fn add_or_update_peer(
        &self,
        pubkey: &CommsPublicKey,
        node_id: NodeId,
        net_addresses: Vec<Multiaddr>,
        peer_features: PeerFeatures,
    ) -> Result<Peer, DhtInboundError>
    {
        let peer_manager = &self.peer_manager;
        // Add peer or modify existing peer using received join request
        if peer_manager.exists(pubkey) {
            peer_manager.update_peer(
                pubkey,
                Some(node_id),
                Some(net_addresses),
                None,
                Some(peer_features),
                None,
            )?;
        } else {
            peer_manager.add_peer(Peer::new(
                pubkey.clone(),
                node_id,
                net_addresses.into(),
                PeerFlags::default(),
                peer_features,
            ))?;
        }

        let peer = peer_manager.find_by_public_key(&pubkey)?;

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
        trace!(
            target: LOG_TARGET,
            "Received Join Message from {}",
            message.dht_header.origin_public_key
        );
        let DecryptedDhtMessage {
            decryption_result,
            dht_header,
            source_peer,
            ..
        } = message;
        let body = decryption_result.expect("already checked that this message decrypted successfully");
        let join_msg = body
            .decode_part::<JoinMessage>(0)?
            .ok_or(DhtInboundError::InvalidJoinNetAddresses)?;

        let addresses = join_msg
            .addresses
            .into_iter()
            .filter_map(|addr| addr.parse().ok())
            .collect::<Vec<_>>();

        if addresses.len() == 0 {
            return Err(DhtInboundError::InvalidAddresses);
        }

        let node_id = self.validate_raw_node_id(&dht_header.origin_public_key, &join_msg.node_id)?;

        let origin_peer = self.add_or_update_peer(
            &dht_header.origin_public_key,
            node_id,
            addresses,
            PeerFeatures::from_bits_truncate(join_msg.peer_features),
        )?;

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
            self.peer_manager.in_network_region(
                &origin_peer.node_id,
                self.node_identity.node_id(),
                self.config.num_neighbouring_nodes,
            )?
        {
            trace!(
                target: LOG_TARGET,
                "Sending Join to joining peer with public key '{}'",
                origin_peer.public_key
            );
            self.send_join_direct(origin_peer.public_key).await?;
        }

        trace!(
            target: LOG_TARGET,
            "Propagating join message to at most {} peer(s)",
            self.config.num_neighbouring_nodes
        );
        // Propagate message to closer peers
        self.outbound_service
            .send_raw(
                SendMessageParams::new()
                    .closest(origin_peer.node_id, self.config.num_neighbouring_nodes, vec![
                        dht_header.origin_public_key.clone(),
                        source_peer.public_key,
                    ])
                    .with_dht_header(dht_header)
                    .finish(),
                body.to_encoded_bytes()?,
            )
            .await?;

        Ok(())
    }

    async fn handle_discover_response(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        trace!(
            target: LOG_TARGET,
            "Received Discover Response Message from {}",
            message.dht_header.origin_public_key
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
        trace!(
            target: LOG_TARGET,
            "Received Discover Message from {}",
            message.dht_header.origin_public_key
        );

        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");

        let discover_msg = msg
            .decode_part::<DiscoveryMessage>(0)?
            .ok_or(DhtInboundError::InvalidMessageBody)?;

        let addresses = discover_msg
            .addresses
            .into_iter()
            .filter_map(|addr| addr.parse().ok())
            .collect::<Vec<_>>();

        if addresses.len() == 0 {
            return Err(DhtInboundError::InvalidAddresses);
        }

        let node_id = self.validate_raw_node_id(&message.dht_header.origin_public_key, &discover_msg.node_id)?;
        let origin_peer = self.add_or_update_peer(
            &message.dht_header.origin_public_key,
            node_id,
            addresses,
            PeerFeatures::from_bits_truncate(discover_msg.peer_features),
        )?;

        // Don't send a join request to the origin peer if they are banned
        if origin_peer.is_banned() {
            warn!(
                target: LOG_TARGET,
                "Received Discovery request for banned peer. This request will be ignored."
            );
            return Ok(());
        }

        // Send the origin the current nodes latest contact info
        self.send_discovery_response(message.dht_header.origin_public_key, discover_msg.nonce)
            .await?;

        Ok(())
    }

    /// Send a network join update request directly to a specific known peer
    async fn send_join_direct(&mut self, dest_public_key: CommsPublicKey) -> Result<(), DhtInboundError> {
        let join_msg = JoinMessage {
            node_id: self.node_identity.node_id().to_vec(),
            addresses: vec![self.node_identity.control_service_address().to_string()],
            peer_features: self.node_identity.features().bits(),
        };

        trace!("Sending direct join request to {}", dest_public_key);
        self.outbound_service
            .send_message_no_header(
                SendMessageParams::new()
                    .direct_public_key(dest_public_key.clone())
                    .with_destination(NodeDestination::PublicKey(dest_public_key))
                    .with_encryption(OutboundEncryption::EncryptForPeer)
                    .with_dht_message_type(DhtMessageType::Join)
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
            addresses: vec![self.node_identity.control_service_address().to_string()],
            peer_features: self.node_identity.features().bits(),
            nonce,
        };

        trace!("Sending discovery response to {}", dest_public_key);
        self.outbound_service
            .send_message_no_header(
                SendMessageParams::new()
                    .direct_public_key(dest_public_key.clone())
                    .with_destination(NodeDestination::PublicKey(dest_public_key))
                    .with_encryption(OutboundEncryption::EncryptForPeer)
                    .with_dht_message_type(DhtMessageType::DiscoveryResponse)
                    .finish(),
                response,
            )
            .await?;

        Ok(())
    }
}
