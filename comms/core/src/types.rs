//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Common Tari comms types

use multiaddr::{Multiaddr, Protocol};
use serde::{Deserialize, Serialize};
use tari_crypto::{
    compressed_key::CompressedKey,
    dhke::DiffieHellmanSharedSecret,
    hash_domain,
    keys::PublicKey,
    ristretto::RistrettoPublicKey,
    signatures::{CompressedSchnorrSignature, SchnorrSignature},
};
use tari_storage::lmdb_store::LMDBStore;

use crate::peer_manager::database::PeerDatabaseSql;

/// Public key type
pub type CommsPublicKey = CompressedKey<RistrettoPublicKey>;
pub type UncompressedCommsPublicKey = RistrettoPublicKey;
pub type CommsSecretKey = <RistrettoPublicKey as PublicKey>::K;

// Diffie-Hellman key exchange type
pub type CommsDHKE = DiffieHellmanSharedSecret<RistrettoPublicKey>;

/// Comms signature type
pub type Signature = SchnorrSignature<RistrettoPublicKey, CommsSecretKey>;
pub type CompressedSignature = CompressedSchnorrSignature<RistrettoPublicKey, CommsSecretKey>;

/// Specify the RNG that should be used for random selection
pub type CommsRng = rand::rngs::ThreadRng;

/// Datastore and Database used for persistence storage
pub type CommsDataStore = LMDBStore;

pub type CommsDatabase = PeerDatabaseSql;

/// Specify the address protocol
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TransportProtocol {
    Ipv4,
    Ipv6,
    Onion,
    Memory,
}

impl TransportProtocol {
    pub fn get_all() -> Vec<TransportProtocol> {
        vec![
            TransportProtocol::Ipv4,
            TransportProtocol::Ipv6,
            TransportProtocol::Onion,
            TransportProtocol::Memory,
        ]
    }

    pub fn get_prefix(&self) -> &str {
        match self {
            TransportProtocol::Ipv4 => "/ip4",
            TransportProtocol::Ipv6 => "/ip6",
            TransportProtocol::Onion => "/onion",
            TransportProtocol::Memory => "/memory",
        }
    }

    pub fn preference_index(address: &Multiaddr, preferred_protocols: &[TransportProtocol]) -> Option<usize> {
        let protocol = TransportProtocol::from(address);
        preferred_protocols
            .iter()
            .position(|preferred_protocol| preferred_protocol == &protocol)
    }

    pub fn sort_multiaddrs_by_preference(
        mut addresses: Vec<Multiaddr>,
        preferred_protocols: &[TransportProtocol],
    ) -> Vec<Multiaddr> {
        addresses.sort_by_key(|address| Self::preference_index(address, preferred_protocols).unwrap_or(usize::MAX));
        addresses
    }
}

impl From<Multiaddr> for TransportProtocol {
    fn from(addr: Multiaddr) -> Self {
        match addr.iter().next() {
            Some(Protocol::Ip4(_)) => TransportProtocol::Ipv4,
            Some(Protocol::Ip6(_)) => TransportProtocol::Ipv6,
            Some(Protocol::Onion(_, _)) => TransportProtocol::Onion,
            Some(Protocol::Onion3(_)) => TransportProtocol::Onion,
            Some(Protocol::Memory(_)) => TransportProtocol::Memory,
            _ => TransportProtocol::Ipv4,
        }
    }
}

impl From<&Multiaddr> for TransportProtocol {
    fn from(addr: &Multiaddr) -> Self {
        match addr.iter().next() {
            Some(Protocol::Ip4(_)) => TransportProtocol::Ipv4,
            Some(Protocol::Ip6(_)) => TransportProtocol::Ipv6,
            Some(Protocol::Onion(_, _)) => TransportProtocol::Onion,
            Some(Protocol::Onion3(_)) => TransportProtocol::Onion,
            Some(Protocol::Memory(_)) => TransportProtocol::Memory,
            _ => TransportProtocol::Ipv4,
        }
    }
}

impl From<&Multiaddr> for &TransportProtocol {
    fn from(addr: &Multiaddr) -> Self {
        match addr.iter().next() {
            Some(Protocol::Ip4(_)) => &TransportProtocol::Ipv4,
            Some(Protocol::Ip6(_)) => &TransportProtocol::Ipv6,
            Some(Protocol::Onion(_, _)) => &TransportProtocol::Onion,
            Some(Protocol::Onion3(_)) => &TransportProtocol::Onion,
            Some(Protocol::Memory(_)) => &TransportProtocol::Memory,
            _ => &TransportProtocol::Ipv4,
        }
    }
}

hash_domain!(CommsCoreHashDomain, "com.tari.comms.core", 0);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sorts_multiaddrs_by_transport_protocol_preference() {
        let onion_host = "a".repeat(56);
        let onion = format!("/onion3/{onion_host}:18141").parse::<Multiaddr>().unwrap();
        let ipv4 = "/ip4/8.8.8.8/tcp/18189".parse::<Multiaddr>().unwrap();
        let ipv6 = "/ip6/::1/tcp/18189".parse::<Multiaddr>().unwrap();

        let addresses = vec![ipv4.clone(), onion.clone(), ipv6.clone()];

        assert_eq!(
            TransportProtocol::sort_multiaddrs_by_preference(
                addresses.clone(),
                &[
                    TransportProtocol::Onion,
                    TransportProtocol::Ipv4,
                    TransportProtocol::Ipv6
                ],
            ),
            vec![onion.clone(), ipv4.clone(), ipv6.clone()]
        );
        assert_eq!(
            TransportProtocol::sort_multiaddrs_by_preference(
                addresses,
                &[
                    TransportProtocol::Ipv4,
                    TransportProtocol::Ipv6,
                    TransportProtocol::Onion
                ],
            ),
            vec![ipv4, ipv6, onion]
        );
    }
}
