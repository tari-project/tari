//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{collections::HashSet, io, sync::Arc, time::Duration};

use asynchronous_codec::{BytesMut, Decoder, Encoder};
use blake2::{digest::consts::U64, Blake2b, Digest};
use libp2p::{identity, Multiaddr, PeerId};

use crate::{epoch_time::epoch_time_now, proto, Error, MAX_MESSAGE_SIZE};

#[derive(Debug, Clone)]
pub struct SignedPeerRecord {
    pub addresses: Vec<Multiaddr>,
    pub updated_at: Duration,
    pub signature: PeerSignature,
}

impl SignedPeerRecord {
    pub fn decode_from_proto(bytes: &[u8]) -> Result<Self, Error> {
        let rec = quick_protobuf_codec::Codec::<proto::SignedPeerRecord>::new(MAX_MESSAGE_SIZE)
            .decode(&mut BytesMut::from(bytes))?
            .ok_or_else(|| Error::CodecError(io::Error::new(io::ErrorKind::UnexpectedEof, "not enough bytes")))?;
        Self::try_from(rec)
    }

    pub fn encode_to_proto(&self) -> Result<BytesMut, Error> {
        let mut bytes = BytesMut::with_capacity(MAX_MESSAGE_SIZE);
        quick_protobuf_codec::Codec::<proto::SignedPeerRecord>::new(MAX_MESSAGE_SIZE)
            .encode(proto::SignedPeerRecord::from(self.clone()), &mut bytes)?;
        Ok(bytes)
    }

    pub fn is_valid(&self) -> bool {
        self.signature
            .is_valid(&peer_signature_challenge(&self.addresses, &self.updated_at))
    }

    pub fn to_peer_id(&self) -> PeerId {
        self.signature.public_key.to_peer_id()
    }

    pub fn public_key(&self) -> &identity::PublicKey {
        &self.signature.public_key
    }
}

impl TryFrom<proto::SignedPeerRecord> for SignedPeerRecord {
    type Error = Error;

    fn try_from(value: proto::SignedPeerRecord) -> Result<Self, Self::Error> {
        let addresses = value
            .addresses
            .into_iter()
            .map(|addr| Multiaddr::try_from(addr).map_err(Error::DecodeMultiaddr))
            .collect::<Result<_, _>>()?;

        Ok(Self {
            addresses,
            updated_at: Duration::from_secs(value.ts_updated_at),
            signature: value
                .signature
                .ok_or_else(|| Error::InvalidMessage {
                    peer_id: PeerId::random(),
                    details: "missing signature".to_string(),
                })?
                .try_into()?,
        })
    }
}

impl From<SignedPeerRecord> for proto::SignedPeerRecord {
    fn from(value: SignedPeerRecord) -> Self {
        Self {
            addresses: value.addresses.into_iter().map(|a| a.to_vec()).collect(),
            ts_updated_at: value.updated_at.as_secs(),
            signature: Some(value.signature.into()),
        }
    }
}

impl TryFrom<LocalPeerRecord> for SignedPeerRecord {
    type Error = Error;

    fn try_from(value: LocalPeerRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            addresses: value.addresses.into_iter().collect(),
            updated_at: value.updated_at,
            signature: value.signature.ok_or_else(|| Error::LocalPeerNotSigned)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct LocalPeerRecord {
    keypair: Arc<identity::Keypair>,
    addresses: HashSet<Multiaddr>,
    updated_at: Duration,
    signature: Option<PeerSignature>,
}

impl LocalPeerRecord {
    pub fn new(keypair: Arc<identity::Keypair>) -> Self {
        Self {
            keypair,
            addresses: HashSet::new(),
            updated_at: epoch_time_now(),
            signature: None,
        }
    }

    pub fn to_peer_id(&self) -> PeerId {
        self.keypair.public().to_peer_id()
    }

    pub fn add_address(&mut self, address: Multiaddr) {
        self.addresses.insert(address);
        self.sign();
    }

    pub fn remove_address(&mut self, address: &Multiaddr) {
        self.addresses.remove(address);
        self.sign();
    }

    pub fn addresses(&self) -> &HashSet<Multiaddr> {
        &self.addresses
    }

    pub fn is_signed(&self) -> bool {
        self.signature.is_some()
    }

    pub fn encode_to_proto(&self) -> Result<BytesMut, Error> {
        SignedPeerRecord::try_from(self.clone())?.encode_to_proto()
    }

    fn sign(&mut self) {
        self.updated_at = epoch_time_now();
        let msg = peer_signature_challenge(&self.addresses, &self.updated_at);
        self.signature = Some(PeerSignature::sign(&self.keypair, &msg));
    }
}

#[derive(Debug, Clone)]
pub struct PeerSignature {
    pub public_key: identity::PublicKey,
    pub signature: Vec<u8>,
}

impl PeerSignature {
    pub fn is_valid(&self, message: &[u8]) -> bool {
        self.public_key.verify(message, &self.signature)
    }

    pub fn sign(keypair: &identity::Keypair, message: &[u8]) -> Self {
        let signature = keypair
            .sign(message)
            .expect("RSA is the only fallible signature scheme and is not compiled in as a feature");
        Self {
            public_key: keypair.public().clone(),
            signature,
        }
    }
}

impl From<PeerSignature> for proto::PeerSignature {
    fn from(value: PeerSignature) -> Self {
        Self {
            public_key: value.public_key.encode_protobuf(),
            signature: value.signature,
        }
    }
}

impl TryFrom<proto::PeerSignature> for PeerSignature {
    type Error = Error;

    fn try_from(value: proto::PeerSignature) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: identity::PublicKey::try_decode_protobuf(&value.public_key).map_err(|_| {
                Error::InvalidMessage {
                    peer_id: PeerId::random(),
                    details: "invalid public key".to_string(),
                }
            })?,
            signature: value.signature,
        })
    }
}

fn peer_signature_challenge<'a, I: IntoIterator<Item = &'a Multiaddr>>(
    addresses: I,
    updated_at: &Duration,
) -> [u8; 64] {
    const PEER_SIGNATURE_DOMAIN: &[u8] = b"com.libp2p.peer_signature.v1";
    let mut hasher = Blake2b::<U64>::new();

    hasher.update(PEER_SIGNATURE_DOMAIN);
    for addr in addresses {
        hasher.update(addr.as_ref());
    }
    hasher.update(updated_at.as_secs().to_be_bytes());
    hasher.finalize().into()
}
