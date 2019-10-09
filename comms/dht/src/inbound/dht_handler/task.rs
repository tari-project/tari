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

use super::message::{DiscoverMessage, JoinMessage};
use crate::{
    config::DhtConfig,
    inbound::{error::DhtInboundError, message::DecryptedDhtMessage},
    message::{DhtMessageFlags, DhtMessageType, NodeDestination},
    outbound::{BroadcastStrategy, OutboundMessageRequester},
};
use log::*;
use std::sync::Arc;
use tari_comms::{
    connection::NetAddress,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFlags, PeerManager},
    types::CommsPublicKey,
};
use tari_comms_middleware::MiddlewareError;
use tari_utilities::message_format::MessageFormat;
use tower::{Service, ServiceExt};

pub struct ProcessDhtMessages<S> {
    config: DhtConfig,
    next_service: S,
    peer_manager: Arc<PeerManager>,
    outbound_service: OutboundMessageRequester,
    node_identity: Arc<NodeIdentity>,
    message: Option<DecryptedDhtMessage>,
}

impl<S> ProcessDhtMessages<S>
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
            .expect("DhtInboundMessageTask initialized without message");
        match message.dht_header.message_type {
            DhtMessageType::Join => self.handle_join(message).await.map_err(Into::into),
            DhtMessageType::Discover => self.handle_discover(message).await.map_err(Into::into),
            // Not a DHT message, call downstream middleware
            DhtMessageType::None => {
                self.next_service.ready().await.map_err(Into::into)?;
                self.next_service.call(message).await.map_err(Into::into)
            },
        }
    }

    fn add_or_update_peer(
        &self,
        pubkey: &CommsPublicKey,
        node_id: NodeId,
        net_addresses: Vec<NetAddress>,
    ) -> Result<(), DhtInboundError>
    {
        let peer_manager = &self.peer_manager;
        // Add peer or modify existing peer using received join request
        if peer_manager.exists(pubkey)? {
            peer_manager.update_peer(pubkey, Some(node_id), Some(net_addresses), None)?;
        } else {
            peer_manager.add_peer(Peer::new(
                pubkey.clone(),
                node_id,
                net_addresses.into(),
                PeerFlags::default(),
            ))?;
        }

        Ok(())
    }

    async fn handle_join(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        let join_msg = JoinMessage::from_binary(&message.inner_success().body)?;

        // TODO: Check/Verify the received peers information
        self.add_or_update_peer(
            &message.dht_header.origin_public_key,
            join_msg.node_id.clone(),
            join_msg.net_addresses,
        )?;

        // Send a join request back to the source peer of the join request if that peer is from the same region
        // of network. Also, only Send a join request back if this copy of the received join
        // request was not sent directly from the original source peer but was forwarded. If it
        // was not forwarded then that source peer already has the current peers info in its
        // PeerManager.
        if message.dht_header.origin_public_key != message.source_peer.public_key &&
            self.peer_manager.in_network_region(
                &join_msg.node_id,
                &self.node_identity.identity.node_id,
                self.config.max_nodes_join_request,
            )?
        {
            self.send_join_direct(message.dht_header.origin_public_key.clone())
                .await?;
        }

        // Propagate message to closer peers
        //            oms.forward_message(
        //                BroadcastStrategy::Closest(ClosestRequest {
        //                    n: DHT_BROADCAST_NODE_COUNT,
        //                    node_id: join_msg.node_id.clone(),
        //                    excluded_peers: vec![info.origin_source, info.peer_source.public_key],
        //                }),
        //                info.message_envelope,
        //            )?;
        Ok(())
    }

    async fn handle_discover(&mut self, message: DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        let discover_msg = DiscoverMessage::from_binary(&message.inner_success().body)?;
        // TODO: Check/Verify the received peers information
        self.add_or_update_peer(
            &message.dht_header.origin_public_key,
            discover_msg.node_id,
            discover_msg.net_addresses,
        )?;

        // Send the origin the current nodes latest contact info
        self.send_join_direct(message.dht_header.origin_public_key).await?;

        Ok(())
    }

    /// Send a network join update request directly to a specific known peer
    async fn send_join_direct(&mut self, dest_public_key: CommsPublicKey) -> Result<(), DhtInboundError> {
        let join_msg = JoinMessage {
            node_id: self.node_identity.identity.node_id.clone(),
            net_addresses: vec![self.node_identity.control_service_address()],
        };

        trace!("Sending direct join request to {}", dest_public_key);
        self.outbound_service
            .send_message(
                BroadcastStrategy::DirectPublicKey(dest_public_key.clone()),
                NodeDestination::PublicKey(dest_public_key),
                DhtMessageFlags::ENCRYPTED,
                DhtMessageType::Join,
                join_msg,
            )
            .await?;

        Ok(())
    }
}
