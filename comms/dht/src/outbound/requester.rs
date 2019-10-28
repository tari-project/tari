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

use super::{broadcast_strategy::BroadcastStrategy, message::DhtOutboundRequest};
use crate::{
    envelope::{DhtMessageFlags, DhtMessageHeader, NodeDestination},
    outbound::{
        message::{ForwardRequest, OutboundEncryption, SendMessageRequest},
        DhtOutboundError,
    },
    proto::envelope::DhtMessageType,
};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};
use tari_comms::{
    message::{Frame, MessageExt, MessageFlags, MessageHeader},
    types::CommsPublicKey,
    wrap_in_envelope_body,
};
use tari_utilities::message_format::MessageFormat;

#[derive(Clone)]
pub struct OutboundMessageRequester {
    sender: mpsc::Sender<DhtOutboundRequest>,
}

impl OutboundMessageRequester {
    pub fn new(sender: mpsc::Sender<DhtOutboundRequest>) -> Self {
        Self { sender }
    }

    /// Send directly to a peer.
    pub async fn send_direct<T, MType>(
        &mut self,
        dest_public_key: CommsPublicKey,
        encryption: OutboundEncryption,
        message_type: MType,
        message: T,
    ) -> Result<bool, DhtOutboundError>
    where
        MessageHeader<MType>: MessageFormat,
        T: MessageFormat,
    {
        self.send_message(
            BroadcastStrategy::DirectPublicKey(dest_public_key.clone()),
            NodeDestination::PublicKey(dest_public_key),
            encryption,
            message_type,
            message,
        )
        .await
        .map(|count| {
            debug_assert!(count <= 1);
            count >= 1
        })
    }

    /// Send to a pre-configured number of closest peers.
    ///
    /// Each message is destined for each peer.
    pub async fn send_direct_neighbours<T, MType>(
        &mut self,
        encryption: OutboundEncryption,
        exclude_peers: Vec<CommsPublicKey>,
        message_type: MType,
        message: T,
    ) -> Result<usize, DhtOutboundError>
    where
        MessageHeader<MType>: MessageFormat,
        T: MessageFormat,
    {
        self.propagate(
            NodeDestination::Unknown,
            encryption,
            exclude_peers,
            message_type,
            message,
        )
        .await
    }

    /// Send to a pre-configured number of closest peers, for further message propagation.
    ///
    /// Optionally, the NodeDestination can be set to propagate to a particular peer, or network region
    /// in addition to each peer directly (Same as send_direct_neighbours).
    pub async fn propagate<T, MType>(
        &mut self,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        exclude_peers: Vec<CommsPublicKey>,
        message_type: MType,
        message: T,
    ) -> Result<usize, DhtOutboundError>
    where
        MessageHeader<MType>: MessageFormat,
        T: MessageFormat,
    {
        self.send_message(
            BroadcastStrategy::Neighbours(Box::new(exclude_peers)),
            destination,
            encryption,
            message_type,
            message,
        )
        .await
    }

    /// Send to _ALL_ known peers.
    ///
    /// This should be used with caution as, depending on the number of known peers, a lot of network
    /// traffic could be generated from this node.
    pub async fn send_flood<T, MType>(
        &mut self,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        message_type: MType,
        message: T,
    ) -> Result<usize, DhtOutboundError>
    where
        MessageHeader<MType>: MessageFormat,
        T: MessageFormat,
    {
        self.send_message(BroadcastStrategy::Flood, destination, encryption, message_type, message)
            .await
    }

    /// Send to a random subset of peers of size _n_.
    pub async fn send_random<T, MType>(
        &mut self,
        n: usize,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        message_type: MType,
        message: T,
    ) -> Result<usize, DhtOutboundError>
    where
        MessageHeader<MType>: MessageFormat,
        T: MessageFormat,
    {
        self.send_message(
            BroadcastStrategy::Random(n),
            destination,
            encryption,
            message_type,
            message,
        )
        .await
    }

    /// Send a message with custom parameters
    pub async fn send_message<T, MType>(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        message_type: MType,
        message: T,
    ) -> Result<usize, DhtOutboundError>
    where
        MessageHeader<MType>: MessageFormat,
        T: MessageFormat,
    {
        let flags = encryption.flags();
        // TODO: Temporary hack
        let header = MessageHeader::new(message_type)?;
        let body = wrap_in_envelope_body!(header.to_binary()?, message.to_binary()?)?.to_encoded_bytes()?;
        self.send(
            broadcast_strategy,
            destination,
            encryption,
            flags,
            DhtMessageType::None,
            body,
        )
        .await
    }

    /// Send a DHT-level message
    pub async fn send_dht_message<T>(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        message_type: DhtMessageType,
        message: T,
    ) -> Result<usize, DhtOutboundError>
    where
        T: prost::Message,
    {
        let flags = encryption.flags();
        let body = wrap_in_envelope_body!(message)?.to_encoded_bytes()?;
        self.send(broadcast_strategy, destination, encryption, flags, message_type, body)
            .await
    }

    /// Send a raw message
    pub async fn send(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        dht_flags: DhtMessageFlags,
        dht_message_type: DhtMessageType,
        body: Frame,
    ) -> Result<usize, DhtOutboundError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(DhtOutboundRequest::SendMsg(
                Box::new(SendMessageRequest {
                    broadcast_strategy,
                    destination,
                    encryption,
                    // Since NONE is the only option here, hard code to empty() rather than make this part of the public
                    // interface. If comms-level message flags become useful, it should be easy to add that to the
                    // public API from here up to domain-level
                    comms_flags: MessageFlags::empty(),
                    dht_flags,
                    dht_message_type,
                    body,
                }),
                reply_tx,
            ))
            .await?;

        reply_rx
            .await
            .map_err(|_| DhtOutboundError::RequesterReplyChannelClosed)
    }

    /// Send a forwarded message
    pub async fn forward_message(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        dht_header: DhtMessageHeader,
        body: Vec<u8>,
    ) -> Result<(), DhtOutboundError>
    {
        self.sender
            .send(DhtOutboundRequest::Forward(Box::new(ForwardRequest {
                broadcast_strategy,
                comms_flags: MessageFlags::FORWARDED,
                dht_header,
                body,
            })))
            .await
            .map_err(Into::into)
    }
}
