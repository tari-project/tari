//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::fmt::{Display, Formatter};

use libp2p::{identity, Multiaddr, PeerId, StreamProtocol};
use tari_crypto::ristretto::RistrettoPublicKey;

use crate::identity::PublicKey;

#[derive(Debug, Clone)]
pub struct Peer {
    pub(crate) public_key: PublicKey,
    pub(crate) addresses: Vec<Multiaddr>,
}

impl Peer {
    pub fn new(public_key: PublicKey, addresses: Vec<Multiaddr>) -> Self {
        Self { public_key, addresses }
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn addresses(&self) -> &[Multiaddr] {
        &self.addresses
    }

    pub fn to_peer_id(&self) -> PeerId {
        self.public_key.to_peer_id()
    }
}

impl Display for Peer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Peer({}, {}, [",
            self.public_key.to_peer_id(),
            self.public_key.key_type()
        )?;
        for addr in &self.addresses {
            write!(f, "{}, ", addr)?;
        }
        write!(f, "])")
    }
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub public_key: PublicKey,
    pub protocol_version: String,
    pub agent_version: String,
    pub listen_addrs: Vec<Multiaddr>,
    pub protocols: Vec<StreamProtocol>,
}

impl Display for PeerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_key_value("PeerId", &self.peer_id, f)?;
        write_key_value("Protocol version", &self.protocol_version, f)?;
        write_key_value("Agent version", &self.agent_version, f)?;

        if self.listen_addrs.is_empty() {
            writeln!(f, "No listener addresses")?;
        } else {
            write_key("Listen addresses", f)?;
            for addr in &self.listen_addrs {
                writeln!(f, "\t- {addr:?}")?;
            }
        }
        if !self.protocols.is_empty() {
            write_key("Protocols", f)?;
            for protocol in &self.protocols {
                writeln!(f, "\t- {protocol}")?;
            }
        }

        Ok(())
    }
}
fn write_key_value<V: std::fmt::Debug>(k: &str, v: &V, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "{k}: {v:?}")
}
fn write_key(k: &str, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "{k}:")
}

pub trait ToPeerId {
    fn to_peer_id(&self) -> PeerId;
}

impl ToPeerId for PeerId {
    fn to_peer_id(&self) -> PeerId {
        *self
    }
}

impl ToPeerId for PublicKey {
    fn to_peer_id(&self) -> PeerId {
        PublicKey::to_peer_id(self)
    }
}

impl ToPeerId for RistrettoPublicKey {
    fn to_peer_id(&self) -> PeerId {
        PublicKey::from(identity::sr25519::PublicKey::from(self.clone())).to_peer_id()
    }
}
