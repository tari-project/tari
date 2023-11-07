// // Copyright 2023. The Tari Project
// //
// // Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// // following conditions are met:
// //
// // 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following // disclaimer.
// //
// // 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// // following disclaimer in the documentation and/or other materials provided with the distribution.
// //
// // 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// // products derived from this software without specific prior written permission.
// //
// // THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// // INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// // DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// // SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// // SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// // WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// // USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
mod harness;
use std::{iter, time::Duration};

use harness::*;
use rand::{rngs::OsRng, Rng, RngCore};
use tari_comms::{
    peer_manager::{IdentitySignature, PeerFeatures},
    NodeIdentity,
};
use tari_comms_dht::{envelope::DhtMessageType, outbound::SendMessageParams};
use tari_test_utils::async_assert_eventually;
use tari_utilities::ByteArray;

#[tokio::test(flavor = "multi_thread")]
async fn large_join_messages_with_many_addresses() {
    // Create 3 nodes where only Node B knows A and C, but A and C want to talk to each other

    // Node C knows no one
    let node_c = make_node("node_C", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    // Node B knows about Node C
    let node_b = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_c.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_a = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_b.to_peer()),
    )
    .await;

    node_a
        .comms
        .connectivity()
        .wait_for_connectivity(Duration::from_secs(10))
        .await
        .unwrap();
    node_b
        .comms
        .connectivity()
        .wait_for_connectivity(Duration::from_secs(10))
        .await
        .unwrap();

    let addresses = iter::repeat_with(random_multiaddr_bytes)
        .take(900 * 1024 / 32)
        .collect::<Vec<_>>();
    let node_identity = (*node_a.node_identity()).clone();
    let message = JoinMessage::from_node_identity(&node_identity, addresses);

    node_a
        .dht
        .outbound_requester()
        .send_message_no_header(
            SendMessageParams::new()
                .direct_node_id(node_b.node_identity().node_id().clone())
                .with_destination(node_c.comms.node_identity().public_key().clone().into())
                .with_dht_message_type(DhtMessageType::Join)
                .force_origin()
                .finish(),
            message,
        )
        .await
        .unwrap();

    let node_b_peer_manager = node_b.comms.peer_manager();
    let node_c_peer_manager = node_c.comms.peer_manager();

    // Check that Node B bans node A
    async_assert_eventually!(
        node_b_peer_manager
            .is_peer_banned(node_a.node_identity().node_id())
            .await
            .unwrap(),
        expect = true,
        max_attempts = 20,
        interval = Duration::from_secs(1)
    );
    // Node B did not propagate
    assert!(!node_c_peer_manager.exists(node_a.node_identity().public_key()).await);

    node_a.shutdown().await;
    node_b.shutdown().await;
    node_c.shutdown().await;
}

// Copies of non-public the JoinMessage and IdentitySignature structs too allow this test to manipulate them
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct JoinMessage {
    #[prost(bytes = "vec", tag = "1")]
    pub public_key: Vec<u8>,
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub addresses: Vec<Vec<u8>>,
    #[prost(uint32, tag = "3")]
    pub peer_features: u32,
    #[prost(uint64, tag = "4")]
    pub nonce: u64,
    #[prost(message, optional, tag = "5")]
    pub identity_signature: Option<IdentitySignatureProto>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IdentitySignatureProto {
    #[prost(uint32, tag = "1")]
    pub version: u32,
    #[prost(bytes = "vec", tag = "2")]
    pub signature: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub public_nonce: Vec<u8>,
    /// The EPOCH timestamp used in the identity signature challenge
    #[prost(int64, tag = "4")]
    pub updated_at: i64,
}

impl JoinMessage {
    fn from_node_identity(node_identity: &NodeIdentity, raw_addresses: Vec<Vec<u8>>) -> Self {
        Self {
            public_key: node_identity.public_key().to_vec(),
            addresses: raw_addresses,
            peer_features: node_identity.features().bits(),
            nonce: OsRng.next_u64(),
            identity_signature: node_identity.identity_signature_read().as_ref().map(Into::into),
        }
    }
}

impl From<&IdentitySignature> for IdentitySignatureProto {
    fn from(identity_sig: &IdentitySignature) -> Self {
        Self {
            version: u32::from(identity_sig.version()),
            signature: identity_sig.signature().get_signature().to_vec(),
            public_nonce: identity_sig.signature().get_public_nonce().to_vec(),
            updated_at: identity_sig.updated_at().timestamp(),
        }
    }
}

fn random_port() -> u16 {
    let mut rng = rand::thread_rng();
    rng.gen_range(1024..=65535)
}

fn random_multiaddr_bytes() -> Vec<u8> {
    let port = random_port();
    let mut rng = rand::thread_rng();

    let mut bytes = Vec::with_capacity(7);
    bytes.push(4); // IP4 code
    bytes.extend([rng.gen::<u8>(), rng.gen(), rng.gen(), rng.gen()]);
    bytes.push(6); // TCP code
    bytes.extend(&port.to_be_bytes());

    bytes
}
