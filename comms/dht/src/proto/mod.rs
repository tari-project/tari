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

use crate::proto::dht::JoinMessage;
use chrono::NaiveDateTime;
use rand::{rngs::OsRng, RngCore};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{IdentitySignature, NodeId, Peer, PeerFeatures, PeerFlags},
    types::{CommsPublicKey, CommsSecretKey, Signature},
    NodeIdentity,
};
use tari_utilities::{hex::Hex, ByteArray};

pub mod common {
    tari_comms::outdir_include!("tari.dht.common.rs");
}

pub mod envelope {
    tari_comms::outdir_include!("tari.dht.envelope.rs");
}

pub mod dht {
    use super::common;
    tari_comms::outdir_include!("tari.dht.rs");
}

pub mod rpc {
    tari_comms::outdir_include!("tari.dht.rpc.rs");
}

pub mod store_forward {
    tari_comms::outdir_include!("tari.dht.store_forward.rs");
}

pub mod message_header {
    tari_comms::outdir_include!("tari.dht.message_header.rs");
}

//---------------------------------- JoinMessage --------------------------------------------//

impl<T: AsRef<NodeIdentity>> From<T> for JoinMessage {
    fn from(identity: T) -> Self {
        let node_identity = identity.as_ref();
        Self {
            node_id: node_identity.node_id().to_vec(),
            addresses: vec![node_identity.public_address().to_string()],
            peer_features: node_identity.features().bits(),
            nonce: OsRng.next_u64(),
            identity_signature: node_identity.identity_signature_read().as_ref().map(Into::into),
        }
    }
}

impl fmt::Display for dht::JoinMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "JoinMessage(NodeId = {}, Addresses = {:?}, Features = {:?})",
            self.node_id.to_hex(),
            self.addresses,
            PeerFeatures::from_bits_truncate(self.peer_features),
        )
    }
}

//---------------------------------- Rpc Message Conversions --------------------------------------------//

impl From<Peer> for rpc::Peer {
    fn from(peer: Peer) -> Self {
        rpc::Peer {
            public_key: peer.public_key.to_vec(),
            addresses: peer
                .addresses
                .addresses
                .iter()
                .map(|addr| addr.address.to_string())
                .collect(),
            peer_features: peer.features.bits(),
            identity_signature: peer.identity_signature.as_ref().map(Into::into),
        }
    }
}

impl TryInto<Peer> for rpc::Peer {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Peer, Self::Error> {
        let pk = CommsPublicKey::from_bytes(&self.public_key)?;
        let node_id = NodeId::from_public_key(&pk);
        let addresses = self
            .addresses
            .iter()
            .filter_map(|addr| addr.parse::<Multiaddr>().ok())
            .collect::<Vec<_>>();
        let mut peer = Peer::new(
            pk,
            node_id,
            addresses.into(),
            PeerFlags::NONE,
            PeerFeatures::from_bits_truncate(self.peer_features),
            Default::default(),
            String::new(),
        );

        peer.identity_signature = self.identity_signature.as_ref().map(TryInto::try_into).transpose()?;

        Ok(peer)
    }
}

impl TryFrom<&common::IdentitySignature> for IdentitySignature {
    type Error = anyhow::Error;

    fn try_from(value: &common::IdentitySignature) -> Result<Self, Self::Error> {
        let version = u8::try_from(value.version)
            .map_err(|_| anyhow::anyhow!("Invalid peer identity signature version {}", value.version))?;
        let public_nonce = CommsPublicKey::from_bytes(&value.public_nonce)?;
        let signature = CommsSecretKey::from_bytes(&value.signature)?;
        let updated_at = NaiveDateTime::from_timestamp_opt(value.updated_at, 0)
            .ok_or_else(|| anyhow::anyhow!("updated_at overflowed"))?;

        Ok(Self::new(version, Signature::new(public_nonce, signature), updated_at))
    }
}

impl From<&IdentitySignature> for common::IdentitySignature {
    fn from(identity_sig: &IdentitySignature) -> Self {
        common::IdentitySignature {
            version: identity_sig.version() as u32,
            signature: identity_sig.signature().get_signature().to_vec(),
            public_nonce: identity_sig.signature().get_public_nonce().to_vec(),
            updated_at: identity_sig.updated_at().timestamp(),
        }
    }
}
