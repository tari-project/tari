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

use log::*;
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, wrap_in_envelope_body, BytesMut};
use tokio::sync::{mpsc, oneshot};

use super::message::DhtOutboundRequest;
use crate::{
    crypt::prepare_message,
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{
        message::{OutboundEncryption, SendMessageResponse},
        message_params::{FinalSendMessageParams, SendMessageParams},
        message_send_state::MessageSendState,
        DhtOutboundError,
        MessageSendStates,
    },
};

const LOG_TARGET: &str = "comms::dht::requests::outbound";

#[derive(Clone)]
pub struct OutboundMessageRequester {
    sender: mpsc::Sender<DhtOutboundRequest>,
}

impl OutboundMessageRequester {
    pub fn new(sender: mpsc::Sender<DhtOutboundRequest>) -> Self {
        Self { sender }
    }

    /// Send directly to a peer. If the peer does not exist in the peer list, a discovery will be initiated.
    pub async fn send_direct_encrypted<T>(
        &mut self,
        dest_public_key: CommsPublicKey,
        message: OutboundDomainMessage<T>,
        encryption: OutboundEncryption,
        source_info: String,
    ) -> Result<MessageSendStates, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .with_debug_info(format!("Send direct to {} from {}", &dest_public_key, source_info))
                .direct_public_key(dest_public_key.clone())
                .with_discovery(true)
                .with_encryption(encryption)
                .with_destination(dest_public_key.into())
                .finish(),
            message,
        )
        .await?
        .resolve()
        .await
        .map_err(Into::into)
    }

    /// Send directly to a peer unencrypted. If the peer does not exist in the peer list, a discovery will be initiated.
    pub async fn send_direct_unencrypted<T>(
        &mut self,
        dest_public_key: CommsPublicKey,
        message: OutboundDomainMessage<T>,
        source_info: String,
    ) -> Result<SendMessageResponse, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .with_debug_info(format!("Send direct to {} from {}", &dest_public_key, source_info))
                .direct_public_key(dest_public_key)
                .with_discovery(true)
                .finish(),
            message,
        )
        .await
    }

    /// Send directly to a peer.
    pub async fn send_direct_node_id<T>(
        &mut self,
        dest_node_id: NodeId,
        message: OutboundDomainMessage<T>,
        source_info: String,
    ) -> Result<MessageSendState, DhtOutboundError>
    where
        T: prost::Message,
    {
        let resp = self
            .send_message(
                SendMessageParams::new()
                    .direct_node_id(dest_node_id.clone())
                    .with_debug_info(format!("Send direct to {}. Source: {}", dest_node_id, source_info))
                    .finish(),
                message,
            )
            .await?;

        let send_stats = resp.resolve().await?;

        Ok(send_stats
            .into_inner()
            .pop()
            .expect("MessageSendStates::inner is empty!"))
    }

    /// Send to a pre-configured number of peers, for further message propagation.
    ///
    /// If the node destination is set, the message will be propagated to peers that are closer to the destination (if
    /// available). Otherwise, random peers are selected (gossip).
    pub async fn propagate<T>(
        &mut self,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        exclude_peers: Vec<NodeId>,
        message: OutboundDomainMessage<T>,
        source_info: String,
    ) -> Result<MessageSendStates, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .with_debug_info(source_info)
                .propagate(destination.clone(), exclude_peers)
                .with_encryption(encryption)
                .with_destination(destination)
                .finish(),
            message,
        )
        .await?
        .resolve()
        .await
        .map_err(Into::into)
    }

    /// Send to a pre-configured number of random peers, for further message propagation.
    ///
    /// Optionally, the NodeDestination can be set to propagate to a particular peer, or network region
    /// in addition to each peer directly.
    ///
    /// This strategy can be used to broadcast a message without a particular destination to the rest of the network.
    pub async fn broadcast<T>(
        &mut self,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        exclude_peers: Vec<NodeId>,
        message: OutboundDomainMessage<T>,
        source_info: String,
    ) -> Result<MessageSendStates, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .broadcast(exclude_peers)
                .with_debug_info(format!("broadcast requested from {}", source_info))
                .with_encryption(encryption)
                .with_destination(destination)
                .finish(),
            message,
        )
        .await?
        .resolve()
        .await
        .map_err(Into::into)
    }

    /// Send to peers closer to the given `NodeId`. This strategy will attempt to establish new some closer connections.
    ///
    /// Use this strategy to broadcast a message destined for a particular peer.
    pub async fn closest_broadcast<T>(
        &mut self,
        destination_public_key: CommsPublicKey,
        encryption: OutboundEncryption,
        exclude_peers: Vec<NodeId>,
        message: OutboundDomainMessage<T>,
    ) -> Result<MessageSendStates, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .closest(NodeId::from_public_key(&destination_public_key), exclude_peers)
                .with_encryption(encryption)
                .with_destination(destination_public_key.into())
                .finish(),
            message,
        )
        .await?
        .resolve()
        .await
        .map_err(Into::into)
    }

    /// Send to all _connected_ peers.
    pub async fn flood<T>(
        &mut self,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        exclude_peers: Vec<NodeId>,
        message: OutboundDomainMessage<T>,
        source_info: String,
    ) -> Result<MessageSendStates, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .with_debug_info(source_info)
                .flood(exclude_peers)
                .with_destination(destination)
                .with_encryption(encryption)
                .finish(),
            message,
        )
        .await?
        .resolve()
        .await
        .map_err(Into::into)
    }

    /// Send to a random subset of peers of size _n_.
    pub async fn send_random<T>(
        &mut self,
        n: usize,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        message: OutboundDomainMessage<T>,
    ) -> Result<MessageSendStates, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .random(n)
                .with_destination(destination)
                .with_encryption(encryption)
                .finish(),
            message,
        )
        .await?
        .resolve()
        .await
        .map_err(Into::into)
    }

    /// Send a message with custom parameters
    pub async fn send_message<T>(
        &mut self,
        params: FinalSendMessageParams,
        message: OutboundDomainMessage<T>,
    ) -> Result<SendMessageResponse, DhtOutboundError>
    where
        T: prost::Message,
    {
        if cfg!(debug_assertions) {
            trace!(
                target: LOG_TARGET,
                "Send Message: params:{} message:{:?}",
                params,
                message
            );
        }
        let header = if params.broadcast_strategy.is_direct() {
            message.to_header()
        } else {
            message.to_propagation_header()
        };
        let msg = wrap_in_envelope_body!(header, message.into_inner());
        let body = prepare_message(params.encryption.is_encrypt(), &msg)?;
        self.send_raw(params, body).await
    }

    /// Send a message without a domain header part
    pub async fn send_message_no_header<T>(
        &mut self,
        params: FinalSendMessageParams,
        message: T,
    ) -> Result<SendMessageResponse, DhtOutboundError>
    where
        T: prost::Message,
    {
        if cfg!(debug_assertions) {
            trace!(target: LOG_TARGET, "Send Message: {} {:?}", params, message);
        }
        let msg = wrap_in_envelope_body!(message);
        let body = prepare_message(params.encryption.is_encrypt(), &msg)?;
        self.send_raw(params, body).await
    }

    /// Send a message without a domain header part
    pub async fn send_message_no_header_no_wait<T>(
        &mut self,
        params: FinalSendMessageParams,
        message: T,
    ) -> Result<(), DhtOutboundError>
    where
        T: prost::Message,
    {
        if cfg!(debug_assertions) {
            trace!(target: LOG_TARGET, "Send Message: {} {:?}", params, message);
        }
        let msg = wrap_in_envelope_body!(message);
        let body = prepare_message(params.encryption.is_encrypt(), &msg)?;
        self.send_raw_no_wait(params, body).await
    }

    /// Send a raw message
    pub async fn send_raw(
        &mut self,
        params: FinalSendMessageParams,
        body: BytesMut,
    ) -> Result<SendMessageResponse, DhtOutboundError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(DhtOutboundRequest::SendMessage(Box::new(params), body, reply_tx))
            .await?;

        reply_rx
            .await
            .map_err(|_| DhtOutboundError::RequesterReplyChannelClosed)
    }

    /// Send a raw message
    pub async fn send_raw_no_wait(
        &mut self,
        params: FinalSendMessageParams,
        body: BytesMut,
    ) -> Result<(), DhtOutboundError> {
        let (reply_tx, _) = oneshot::channel();
        self.sender
            .send(DhtOutboundRequest::SendMessage(Box::new(params), body, reply_tx))
            .await?;
        Ok(())
    }

    #[cfg(test)]
    pub fn get_mpsc_sender(&self) -> mpsc::Sender<DhtOutboundRequest> {
        self.sender.clone()
    }
}
