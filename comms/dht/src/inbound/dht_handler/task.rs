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
    envelope::NodeDestination,
    inbound::{error::DhtInboundError, message::DecryptedDhtMessage},
    outbound::{BroadcastClosestRequest, BroadcastStrategy, OutboundEncryption, OutboundMessageRequester},
    proto::{
        dht::{DiscoverMessage, JoinMessage},
        envelope::DhtMessageType,
    },
};
use log::*;
use std::sync::Arc;
use tari_comms::{
    connection::NetAddress,
    message::MessageExt,
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
        message: DecryptedDhtMessage,
    ) -> Self
    {
        Self {
            config,
            next_service,
            peer_manager,
            outbound_service,
            node_identity,
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
            DhtMessageType::Discover => self.handle_discover(message).await?,
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
        net_addresses: Vec<NetAddress>,
        peer_features: PeerFeatures,
    ) -> Result<Peer, DhtInboundError>
    {
        let peer_manager = &self.peer_manager;
        // Add peer or modify existing peer using received join request
        if peer_manager.exists(pubkey) {
            peer_manager.update_peer(pubkey, Some(node_id), Some(net_addresses), None, Some(peer_features))?;
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
            .ok_or(DhtInboundError::InvalidMessageBody)?;

        let addresses = join_msg
            .addresses
            .into_iter()
            .filter_map(|addr| addr.parse().ok())
            .collect::<Vec<_>>();

        if addresses.len() == 0 {
            return Err(DhtInboundError::InvalidAddresses);
        }

        let node_id = self.validate_raw_node_id(&dht_header.origin_public_key, &join_msg.node_id)?;

        // TODO: Check/Verify the received peers information. We know that the join request was signed by
        //       the origin_public_key and that the node id is valid. All that may be needed is to ping
        //       the address to confirm that it is working. If it isn't, do we disregard the join request,
        //       or try other known addresses or ?
        let origin_peer = self.add_or_update_peer(
            &dht_header.origin_public_key,
            node_id,
            addresses,
            PeerFeatures::from_bits_truncate(join_msg.peer_features),
        )?;

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
            .forward_message(
                BroadcastStrategy::Closest(Box::new(BroadcastClosestRequest {
                    n: self.config.num_neighbouring_nodes,
                    node_id: origin_peer.node_id,
                    excluded_peers: vec![dht_header.origin_public_key.clone(), source_peer.public_key],
                })),
                dht_header,
                body.to_encoded_bytes()?,
            )
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
            .decode_part::<DiscoverMessage>(1)?
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
        self.add_or_update_peer(
            &message.dht_header.origin_public_key,
            node_id,
            addresses,
            PeerFeatures::from_bits_truncate(discover_msg.peer_features),
        )?;

        // Send the origin the current nodes latest contact info
        self.send_join_direct(message.dht_header.origin_public_key).await?;

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
            .send_dht_message(
                BroadcastStrategy::DirectPublicKey(dest_public_key.clone()),
                NodeDestination::PublicKey(dest_public_key),
                OutboundEncryption::EncryptForDestination,
                DhtMessageType::Join,
                join_msg,
            )
            .await?;

        Ok(())
    }
}
