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
use std::{convert::TryInto, sync::Arc};

use rand::rngs::OsRng;
use tari_comms::{
    message::{InboundMessage, MessageExt, MessageTag},
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags, PeerManager},
    transports::MemoryTransport,
    types::{Challenge, CommsDatabase, CommsPublicKey, CommsSecretKey},
    utils::signature,
    Bytes,
};
use tari_crypto::{
    keys::PublicKey,
    tari_utilities::{message_format::MessageFormat, ByteArray},
};
use tari_storage::lmdb_store::{LMDBBuilder, LMDBConfig};
use tari_test_utils::{paths::create_temporary_data_path, random};

use crate::{
    crypt,
    envelope::{DhtMessageFlags, DhtMessageHeader, NodeDestination},
    inbound::DhtInboundMessage,
    outbound::message::DhtOutboundMessage,
    proto::envelope::{DhtEnvelope, DhtMessageType, OriginMac},
    version::DhtProtocolVersion,
};

pub fn make_identity(features: PeerFeatures) -> Arc<NodeIdentity> {
    let public_addr = format!("/memory/{}", MemoryTransport::acquire_next_memsocket_port())
        .parse()
        .unwrap();
    Arc::new(NodeIdentity::random(&mut OsRng, public_addr, features))
}

pub fn make_node_identity() -> Arc<NodeIdentity> {
    make_identity(PeerFeatures::COMMUNICATION_NODE)
}

pub fn make_client_identity() -> Arc<NodeIdentity> {
    make_identity(PeerFeatures::COMMUNICATION_CLIENT)
}

pub fn make_peer() -> Peer {
    make_identity(PeerFeatures::COMMUNICATION_NODE).to_peer()
}

pub fn make_comms_inbound_message(node_identity: &NodeIdentity, message: Bytes) -> InboundMessage {
    InboundMessage::new(node_identity.node_id().clone(), message)
}

pub fn make_dht_header(
    node_identity: &NodeIdentity,
    e_pk: &CommsPublicKey,
    e_sk: &CommsSecretKey,
    message: &[u8],
    flags: DhtMessageFlags,
    include_origin: bool,
    trace: MessageTag,
    include_destination: bool,
) -> DhtMessageHeader {
    let destination = if include_destination {
        NodeDestination::PublicKey(Box::new(node_identity.public_key().clone()))
    } else {
        NodeDestination::Unknown
    };
    let mut origin_mac = Vec::new();

    if include_origin {
        let challenge = crypt::create_origin_mac_challenge_parts(
            DhtProtocolVersion::latest(),
            &destination,
            &DhtMessageType::None,
            flags,
            None,
            Some(e_pk),
            message,
        );
        origin_mac = make_valid_origin_mac(node_identity, challenge);
        if flags.is_encrypted() {
            let shared_secret = crypt::generate_ecdh_secret(e_sk, node_identity.public_key());
            origin_mac = crypt::encrypt(&shared_secret, &origin_mac).unwrap()
        }
    }
    DhtMessageHeader {
        version: DhtProtocolVersion::latest(),
        destination,
        ephemeral_public_key: if flags.is_encrypted() { Some(e_pk.clone()) } else { None },
        origin_mac,
        message_type: DhtMessageType::None,
        flags,
        message_tag: trace,
        expires: None,
    }
}

pub fn make_valid_origin_mac(node_identity: &NodeIdentity, challenge: Challenge) -> Vec<u8> {
    let mac = OriginMac {
        public_key: node_identity.public_key().to_vec(),
        signature: signature::sign_challenge(&mut OsRng, node_identity.secret_key().clone(), challenge)
            .unwrap()
            .to_binary()
            .unwrap(),
    };
    mac.to_encoded_bytes()
}

pub fn make_dht_inbound_message(
    node_identity: &NodeIdentity,
    body: Vec<u8>,
    flags: DhtMessageFlags,
    include_origin: bool,
    include_destination: bool,
) -> DhtInboundMessage {
    let msg_tag = MessageTag::new();
    let envelope = make_dht_envelope(node_identity, body, flags, include_origin, msg_tag, include_destination);
    DhtInboundMessage::new(
        msg_tag,
        envelope.header.unwrap().try_into().unwrap(),
        Arc::new(Peer::new(
            node_identity.public_key().clone(),
            node_identity.node_id().clone(),
            Vec::<Multiaddr>::new().into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            Default::default(),
            Default::default(),
        )),
        envelope.body,
    )
}

pub fn make_keypair() -> (CommsSecretKey, CommsPublicKey) {
    CommsPublicKey::random_keypair(&mut OsRng)
}

pub fn make_dht_envelope(
    node_identity: &NodeIdentity,
    mut message: Vec<u8>,
    flags: DhtMessageFlags,
    include_origin: bool,
    trace: MessageTag,
    include_destination: bool,
) -> DhtEnvelope {
    let (e_sk, e_pk) = make_keypair();
    if flags.is_encrypted() {
        let shared_secret = crypt::generate_ecdh_secret(&e_sk, node_identity.public_key());
        message = crypt::encrypt(&shared_secret, &message).unwrap();
    }
    let header = make_dht_header(
        node_identity,
        &e_pk,
        &e_sk,
        &message,
        flags,
        include_origin,
        trace,
        include_destination,
    )
    .into();
    DhtEnvelope::new(header, message.into())
}

pub fn build_peer_manager() -> Arc<PeerManager> {
    let database_name = random::string(8);
    let path = create_temporary_data_path();
    let datastore = LMDBBuilder::new()
        .set_path(path.to_str().unwrap())
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(1)
        .add_database(&database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(&database_name).unwrap();

    PeerManager::new(CommsDatabase::new(Arc::new(peer_database)), None)
        .map(Arc::new)
        .unwrap()
}

pub fn create_outbound_message(body: &[u8]) -> DhtOutboundMessage {
    let msg_tag = MessageTag::new();
    DhtOutboundMessage {
        protocol_version: DhtProtocolVersion::latest(),
        tag: msg_tag,
        destination_node_id: NodeId::default(),
        destination: Default::default(),
        dht_message_type: Default::default(),
        dht_flags: Default::default(),
        custom_header: None,
        body: body.to_vec().into(),
        ephemeral_public_key: None,
        reply: None.into(),
        origin_mac: None,
        is_broadcast: false,
        expires: None,
    }
}
