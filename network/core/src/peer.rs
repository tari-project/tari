//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt::{Display, Formatter},
    time::{Duration, Instant},
};

use libp2p::{identity, Multiaddr, PeerId, StreamProtocol};
use tari_crypto::{ristretto::RistrettoPublicKey, tari_utilities::hex};

use crate::identity::{KeyType, PublicKey};

#[derive(Debug, Clone)]
pub struct Peer {
    pub(crate) public_key: PublicKey,
    pub(crate) peer_id: PeerId,
    pub(crate) addresses: Vec<Multiaddr>,
}

impl Peer {
    pub fn new(public_key: PublicKey, addresses: Vec<Multiaddr>) -> Self {
        Self {
            peer_id: public_key.to_peer_id(),
            public_key,
            addresses,
        }
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn into_public_key_and_addresses(self) -> (PublicKey, Vec<Multiaddr>) {
        (self.public_key, self.addresses)
    }

    pub fn addresses(&self) -> &[Multiaddr] {
        &self.addresses
    }

    pub fn add_address(&mut self, address: Multiaddr) -> &mut Self {
        if !self.addresses.contains(&address) {
            self.addresses.push(address);
        }
        self
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }
}

pub fn public_key_to_string(public_key: &PublicKey) -> String {
    match public_key.key_type() {
        KeyType::Sr25519 => public_key.clone().try_into_sr25519().unwrap().inner_key().to_string(),
        KeyType::Ed25519 => {
            let pk = public_key.clone().try_into_ed25519().unwrap();
            hex::to_hex(&pk.to_bytes())
        },
        KeyType::RSA | KeyType::Secp256k1 | KeyType::Ecdsa => "<notsupported>".to_string(),
    }
}

impl Display for Peer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Peer({}, {}, [", self.peer_id, self.public_key.key_type())?;
        for addr in &self.addresses {
            write!(f, "{}, ", addr)?;
        }
        write!(f, "])")
    }
}

#[derive(Debug, Clone)]
pub struct BannedPeer {
    pub peer_id: PeerId,
    pub banned_at: Instant,
    pub ban_duration: Option<Duration>,
    pub ban_reason: String,
}

impl BannedPeer {
    pub fn is_banned(&self) -> bool {
        self.ban_duration.map_or(true, |d| d >= self.banned_at.elapsed())
    }

    /// Returns None if the ban duration is infinite, otherwise returns the remaining duration of the ban.
    pub fn remaining_ban(&self) -> Option<Duration> {
        let d = self.ban_duration?;
        Some(self.banned_at.elapsed().saturating_sub(d))
    }
}

impl Display for BannedPeer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} banned for: ", self.peer_id)?;
        match self.ban_duration.map(|d| d.saturating_sub(self.banned_at.elapsed())) {
            Some(d) => {
                write!(f, "{}", humantime::format_duration(d))?;
            },
            None => {
                write!(f, "∞")?;
            },
        }
        write!(f, ", reason: {}", self.ban_reason)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub peer_id: PeerId,
    pub addresses: Vec<Multiaddr>,
}

#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub peers: Vec<DiscoveredPeer>,
    pub did_timeout: bool,
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

impl ToPeerId for &RistrettoPublicKey {
    fn to_peer_id(&self) -> PeerId {
        PublicKey::from(identity::sr25519::PublicKey::from((*self).clone())).to_peer_id()
    }
}
