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

use rand::rngs::OsRng;
use std::sync::Arc;
use tari_comms::{
    message::MessageTag,
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
};
use tari_comms_dht::{
    envelope::{DhtMessageFlags, DhtMessageHeader, DhtMessageType, Network, NodeDestination},
    inbound::DhtInboundMessage,
};
use tari_crypto::keys::PublicKey;

macro_rules! unwrap_oms_send_msg {
    ($var:expr, reply_value=$reply_value:expr) => {
        match $var {
            tari_comms_dht::outbound::DhtOutboundRequest::SendMessage(boxed, body, reply_tx) => {
                let _ = reply_tx.send($reply_value);
                (*boxed, body)
            },
        }
    };
    ($var:expr) => {
        unwrap_oms_send_msg!(
            $var,
            reply_value = tari_comms_dht::outbound::SendMessageResponse::Queued(vec![].into())
        );
    };
}

pub fn make_node_id() -> NodeId {
    let (public_key, _) = CommsPublicKey::random_keypair(&mut OsRng);
    NodeId::from_key(&public_key).unwrap()
}

pub fn make_node_identity() -> Arc<NodeIdentity> {
    Arc::new(
        NodeIdentity::random(
            &mut OsRng,
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap(),
    )
}

pub fn make_dht_header(trace: MessageTag) -> DhtMessageHeader {
    DhtMessageHeader {
        version: 0,
        destination: NodeDestination::Unknown,
        origin_mac: Vec::new(),
        ephemeral_public_key: None,
        message_type: DhtMessageType::None,
        network: Network::LocalTest,
        flags: DhtMessageFlags::NONE,
        message_tag: trace,
    }
}

pub fn make_dht_inbound_message(node_identity: &NodeIdentity, message: Vec<u8>) -> DhtInboundMessage {
    let msg_tag = MessageTag::new();
    DhtInboundMessage::new(
        msg_tag,
        make_dht_header(msg_tag),
        Arc::new(Peer::new(
            node_identity.public_key().clone(),
            node_identity.node_id().clone(),
            Vec::<Multiaddr>::new().into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            &[],
        )),
        message,
    )
}
