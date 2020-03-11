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
    broadcast_strategy::{BroadcastClosestRequest, BroadcastStrategy},
    envelope::{DhtMessageFlags, DhtMessageHeader, NodeDestination},
    outbound::OutboundEncryption,
    proto::envelope::DhtMessageType,
};
use std::{fmt, fmt::Display};
use tari_comms::{
    peer_manager::{NodeId, PeerFeatures},
    types::CommsPublicKey,
};

/// Configuration for outbound messages.
///
/// ```edition2018
/// # use tari_comms_dht::outbound::{SendMessageParams, OutboundEncryption};
///
/// // These params represent sending to 5 random peers, each encrypted for that peer
/// let params = SendMessageParams::new()
///   .random(5)
///   .with_encryption(OutboundEncryption::EncryptForPeer)
///   .finish();
/// ```
#[derive(Debug, Clone)]
pub struct SendMessageParams {
    params: Option<FinalSendMessageParams>,
}

impl Default for SendMessageParams {
    fn default() -> Self {
        Self {
            params: Some(Default::default()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FinalSendMessageParams {
    pub broadcast_strategy: BroadcastStrategy,
    pub destination: NodeDestination,
    pub encryption: OutboundEncryption,
    pub is_discovery_enabled: bool,
    pub force_origin: bool,
    pub dht_message_type: DhtMessageType,
    pub dht_message_flags: DhtMessageFlags,
    pub dht_header: Option<DhtMessageHeader>,
}

impl Default for FinalSendMessageParams {
    fn default() -> Self {
        Self {
            broadcast_strategy: BroadcastStrategy::Flood,
            destination: Default::default(),
            encryption: Default::default(),
            dht_message_type: Default::default(),
            dht_message_flags: Default::default(),
            force_origin: false,
            is_discovery_enabled: true,
            dht_header: None,
        }
    }
}

impl Display for FinalSendMessageParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "BroadcastStrategy: {}, Destination: {}",
            self.broadcast_strategy, self.destination
        )
    }
}

impl SendMessageParams {
    pub fn new() -> Self {
        Default::default()
    }

    /// Set broadcast_strategy to DirectPublicKey
    pub fn direct_public_key(&mut self, public_key: CommsPublicKey) -> &mut Self {
        self.params_mut().broadcast_strategy = BroadcastStrategy::DirectPublicKey(Box::new(public_key));
        self
    }

    /// Set broadcast_strategy to DirectNodeId
    pub fn direct_node_id(&mut self, node_id: NodeId) -> &mut Self {
        self.params_mut().broadcast_strategy = BroadcastStrategy::DirectNodeId(Box::new(node_id));
        self
    }

    /// Set broadcast_strategy to Closest.`excluded_peers` are excluded. Only Peers which have all `features` are
    /// included.
    pub fn closest(
        &mut self,
        node_id: NodeId,
        n: usize,
        excluded_peers: Vec<CommsPublicKey>,
        peer_features: PeerFeatures,
    ) -> &mut Self
    {
        self.params_mut().broadcast_strategy = BroadcastStrategy::Closest(Box::new(BroadcastClosestRequest {
            excluded_peers,
            node_id,
            peer_features,
            n,
        }));
        self
    }

    /// Set broadcast_strategy to Neighbours. `excluded_peers` are excluded. Only Peers that have
    /// `PeerFeatures::MESSAGE_PROPAGATION` are included.
    pub fn neighbours(&mut self, excluded_peers: Vec<CommsPublicKey>) -> &mut Self {
        self.neighbours_with_features(excluded_peers, PeerFeatures::MESSAGE_PROPAGATION);
        self
    }

    /// Set broadcast_strategy to Neighbours. `excluded_peers` are excluded.
    /// included. Only peers that have `features` will be included.
    pub fn neighbours_with_features(
        &mut self,
        excluded_peers: Vec<CommsPublicKey>,
        features: PeerFeatures,
    ) -> &mut Self
    {
        self.params_mut().broadcast_strategy = BroadcastStrategy::Neighbours(excluded_peers, features);
        self
    }

    /// Set broadcast_strategy to Flood
    pub fn flood(&mut self) -> &mut Self {
        self.params_mut().broadcast_strategy = BroadcastStrategy::Flood;
        self
    }

    /// Set broadcast_strategy to Random
    pub fn random(&mut self, n: usize) -> &mut Self {
        self.params_mut().broadcast_strategy = BroadcastStrategy::Random(n);
        self
    }

    /// Set destination field in message header.
    pub fn with_destination(&mut self, destination: NodeDestination) -> &mut Self {
        self.params_mut().destination = destination;
        self
    }

    /// Set encryption mode for message.
    pub fn with_encryption(&mut self, encryption: OutboundEncryption) -> &mut Self {
        self.params_mut().encryption = encryption;
        self
    }

    /// Set to true to enable discovery, otherwise false
    pub fn with_discovery(&mut self, is_enabled: bool) -> &mut Self {
        self.params_mut().is_discovery_enabled = is_enabled;
        self
    }

    /// Set the DHT message type
    pub fn with_dht_message_type(&mut self, message_type: DhtMessageType) -> &mut Self {
        self.params_mut().dht_message_type = message_type;
        self
    }

    /// Add a `DhtMessageFlag` to the header
    pub fn add_message_flag(&mut self, flag: DhtMessageFlags) -> &mut Self {
        self.params_mut().dht_message_flags |= flag;
        self
    }

    /// Override the DHtHeader of a message(s) with the given header
    pub fn with_dht_header(&mut self, dht_header: DhtMessageHeader) -> &mut Self {
        self.params_mut().dht_header = Some(dht_header);
        self
    }

    /// Force the message origin to be included in the message. The origin is usually not included in messages without
    /// encryption, however this setting will force the message origin and signature to be included.
    pub fn force_origin(&mut self) -> &mut Self {
        self.params_mut().force_origin = true;
        self
    }

    /// Return the final SendMessageParams
    pub fn finish(&mut self) -> FinalSendMessageParams {
        self.params.take().expect("cannot be None")
    }

    fn params_mut(&mut self) -> &mut FinalSendMessageParams {
        self.params.as_mut().expect("cannot be None")
    }
}
