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

use super::message::DhtOutboundRequest;
use crate::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{
        message::{OutboundEncryption, SendMessageResponse},
        message_params::{FinalSendMessageParams, SendMessageParams},
        DhtOutboundError,
    },
};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};
use tari_comms::{message::MessageExt, peer_manager::NodeId, types::CommsPublicKey, wrap_in_envelope_body};

#[derive(Clone)]
pub struct OutboundMessageRequester {
    sender: mpsc::Sender<DhtOutboundRequest>,
}

impl OutboundMessageRequester {
    pub fn new(sender: mpsc::Sender<DhtOutboundRequest>) -> Self {
        Self { sender }
    }

    /// Send directly to a peer.
    pub async fn send_direct<T>(
        &mut self,
        dest_public_key: CommsPublicKey,
        encryption: OutboundEncryption,
        message: OutboundDomainMessage<T>,
    ) -> Result<SendMessageResponse, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .direct_public_key(dest_public_key)
                .with_encryption(encryption)
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
        encryption: OutboundEncryption,
        message: OutboundDomainMessage<T>,
    ) -> Result<SendMessageResponse, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .direct_node_id(dest_node_id.clone())
                .with_destination(NodeDestination::NodeId(dest_node_id))
                .with_encryption(encryption)
                .finish(),
            message,
        )
        .await
    }

    /// Send to a pre-configured number of closest peers.
    ///
    /// Each message is destined for each peer.
    pub async fn send_direct_neighbours<T>(
        &mut self,
        encryption: OutboundEncryption,
        exclude_peers: Vec<CommsPublicKey>,
        message: OutboundDomainMessage<T>,
    ) -> Result<SendMessageResponse, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.propagate(NodeDestination::Unknown, encryption, exclude_peers, message)
            .await
    }

    /// Send to a pre-configured number of closest peers, for further message propagation.
    ///
    /// Optionally, the NodeDestination can be set to propagate to a particular peer, or network region
    /// in addition to each peer directly (Same as send_direct_neighbours).
    pub async fn propagate<T>(
        &mut self,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        exclude_peers: Vec<CommsPublicKey>,
        message: OutboundDomainMessage<T>,
    ) -> Result<SendMessageResponse, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .neighbours(exclude_peers)
                .with_encryption(encryption)
                .with_destination(destination)
                .finish(),
            message,
        )
        .await
    }

    /// Send to _ALL_ known peers.
    ///
    /// This should be used with caution as, depending on the number of known peers, a lot of network
    /// traffic could be generated from this node.
    pub async fn send_flood<T>(
        &mut self,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        message: OutboundDomainMessage<T>,
    ) -> Result<SendMessageResponse, DhtOutboundError>
    where
        T: prost::Message,
    {
        self.send_message(
            SendMessageParams::new()
                .flood()
                .with_destination(destination)
                .with_encryption(encryption)
                .finish(),
            message,
        )
        .await
    }

    /// Send to a random subset of peers of size _n_.
    pub async fn send_random<T>(
        &mut self,
        n: usize,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        message: OutboundDomainMessage<T>,
    ) -> Result<SendMessageResponse, DhtOutboundError>
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
        .await
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
        let body = wrap_in_envelope_body!(message.to_header(), message.into_inner())?.to_encoded_bytes()?;
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
        let body = wrap_in_envelope_body!(message)?.to_encoded_bytes()?;
        self.send_raw(params, body).await
    }

    /// Send a raw message
    pub async fn send_raw(
        &mut self,
        params: FinalSendMessageParams,
        body: Vec<u8>,
    ) -> Result<SendMessageResponse, DhtOutboundError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(DhtOutboundRequest::SendMessage(Box::new(params), body, reply_tx))
            .await?;

        reply_rx
            .await
            .map_err(|_| DhtOutboundError::RequesterReplyChannelClosed)
    }

    #[cfg(test)]
    pub fn get_mpsc_sender(&self) -> mpsc::Sender<DhtOutboundRequest> {
        self.sender.clone()
    }
}
