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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! The peer list maintained by the Peer Manager are used when constructing outbound messages. Peers can be added and
//! removed from the list and can be found via their NodeId, Public key or Net Address. A subset of peers can be
//! requested from the Peer Manager based on a specific Broadcast Strategy.
//!
//! In an application a single Peer Manager should be initialized and passed around the application contained in an Arc.
//! All the functions in peer manager are thread-safe.
//!
//! If the Peer Manager is instantiated with a provided DataStore it will provide persistence via the provided DataStore
//! implementation.
//!
//! ```edition2018
//! # use tari_comms::peer_manager::{NodeId, Peer, PeerManager, PeerFlags};
//! # use tari_comms::types::CommsPublicKey;
//! # use tari_storage::lmdb::LMDBStore;
//! # use tari_comms::connection::{NetAddress, NetAddresses};
//! # use tari_crypto::keys::PublicKey;
//!
//! let mut rng = rand::OsRng::new().unwrap();
//! let (dest_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
//! let node_id = NodeId::from_key(&pk).unwrap();
//! let net_addresses = NetAddresses::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
//! let peer: Peer<CommsPublicKey> = Peer::<CommsPublicKey>::new(pk, node_id.clone(), net_addresses, PeerFlags::default());
//! let peer_manager = PeerManager::<CommsPublicKey, LMDBStore>::new(None).unwrap();
//! peer_manager.add_peer(peer.clone());
//!
//! let returned_peer = peer_manager.find_with_node_id(&node_id).unwrap();
//! ```

pub mod node_id;
pub mod node_identity;
pub mod peer;
pub mod peer_manager;
pub mod peer_storage;

pub use self::{
    node_id::NodeId,
    node_identity::{CommsNodeIdentity, NodeIdentity, PeerNodeIdentity},
    peer::{Peer, PeerFlags},
    peer_manager::{PeerManager, PeerManagerError},
};
