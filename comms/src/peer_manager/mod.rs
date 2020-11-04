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
//! ```no_compile
//! # use tari_comms::peer_manager::{NodeId, Peer, PeerManager, PeerFlags, PeerFeatures};
//! # use tari_comms::types::CommsPublicKey;
//! # use tari_comms::connection::{NetAddress, NetAddressesWithStats};
//! # use tari_crypto::keys::PublicKey;
//! # use tari_storage::lmdb_store::LMDBBuilder;
//! # use lmdb_zero::db;
//! # use std::sync::Arc;
//! # use tari_storage::LMDBWrapper;
//! # use tari_storage::lmdb_store::LMDBConfig;
//!
//! let mut rng = rand::rngs::OsRng;
//! let (dest_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
//! let node_id = NodeId::from_key(&pk).unwrap();
//! let net_addresses = NetAddressesWithStats::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
//! let peer = Peer::new(
//!     pk,
//!     node_id.clone(),
//!     net_addresses,
//!     PeerFlags::default(),
//!     PeerFeatures::COMMUNICATION_NODE,
//!     Default::default(),
//! );
//! let database_name = "pm_peer_database";
//! let datastore = LMDBBuilder::new()
//!     .set_path("/tmp/")
//!     .set_env_config(LMDBConfig::default())
//!     .set_max_number_of_databases(1)
//!     .add_database(database_name, lmdb_zero::db::CREATE)
//!     .build()
//!     .unwrap();
//! let peer_database = datastore.get_handle(database_name).unwrap();
//! let peer_database = LMDBWrapper::new(Arc::new(peer_database));
//! let peer_manager = PeerManager::new(peer_database).unwrap();
//!
//! peer_manager.add_peer(peer.clone());
//!
//! let returned_peer = peer_manager.find_by_node_id(&node_id).unwrap();
//! ```

mod connection_stats;

mod error;
pub use error::PeerManagerError;

pub mod node_id;
pub use node_id::NodeId;

mod node_identity;
pub use node_identity::{NodeIdentity, NodeIdentityError};

mod peer;
pub use peer::{Peer, PeerFlags};

mod peer_features;
pub use peer_features::PeerFeatures;

mod peer_id;
pub use peer_id::PeerId;

mod manager;
pub use manager::PeerManager;

mod peer_query;
pub use peer_query::{PeerQuery, PeerQuerySortBy};

mod peer_storage;
pub use peer_storage::PeerStorage;

mod migrations;

mod wrapper;
