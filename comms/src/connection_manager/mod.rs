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

//! # ConnectionManager
//!
//! Responsible for establishing and managing active connections to peers.
//!
//! It consists of a number of components each with their own concern.
//!
//! - [ConnectionManager]
//!
//! The public interface for connection management. This uses the other components
//! to manage peer connections for tari_comms.
//!
//! - [LivePeerConnections]
//!
//! A container for [PeerConnection]s which have been created by the [ConnectionManager].
//!
//! - [ConnectionEstablisher]
//!
//! Responsible for creating [PeerConnection]s. This is basically a factory for [PeerConnection]s
//! which first checks that the connection is connected before passing it back to the caller.
//!
//! - [PeerConnectionProtocol]
//!
//! Uses the [ConnectionEstablisher] to connect to a given peer's [ControlService],
//! open an inbound [PeerConnection] and send an [RequestConnection] message with
//! to the peer's [ControlService].
//!
//! ```edition2018
//! # use std::time::Duration;
//! # use std::sync::Arc;
//! # use tari_comms::connection_manager::{ConnectionManager, PeerConnectionConfig};
//! # use tari_comms::peer_manager::{PeerManager, NodeIdentity};
//! # use tari_comms::connection::{ZmqContext, InprocAddress};
//! # use rand::OsRng;
//! # use tari_storage::lmdb_store::LMDBBuilder;
//! # use lmdb_zero::db;
//!
//! let node_identity = Arc::new(NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap());
//!
//! let context = ZmqContext::new();
//!
//! let database_name = "cm_peer_database";
//! let datastore = LMDBBuilder::new()
//!            .set_path("/tmp/")
//!            .set_environment_size(10)
//!            .set_max_number_of_databases(1)
//!            .add_database(database_name, lmdb_zero::db::CREATE)
//!           .build().unwrap();
//! let peer_database = datastore.get_handle(database_name).unwrap();
//! let peer_manager = Arc::new(PeerManager::new(peer_database).unwrap());
//!
//! let manager = ConnectionManager::new(context, node_identity, peer_manager, PeerConnectionConfig {
//!     peer_connection_establish_timeout: Duration::from_secs(5),
//!     max_message_size: 1024,
//!     max_connections: 10,
//!     host: "127.0.0.1".parse().unwrap(),
//!     max_connect_retries: 3,
//!     message_sink_address: InprocAddress::random(),
//!     socks_proxy_address: None,
//! });
//!
//! // No active connections
//! assert_eq!(manager.get_active_connection_count(), 0);
//! ```
//!
//! [ConnectionManager]: ./manager/struct.ConnectionManager.html
//! [LivePeerConnections]: ./connections/struct.LivePeerConnections.html
//! [ControlService]: ../control_service/index.html
//! [RequestConnection]: ../message/p2p/struct.RequestConnection.html
//! [Connecti]: ./connections/struct.LivePeerConnections.html
//! [PeerConnection]: ../connection/peer_connection/struct.PeerConnection.html
//! [ConnectionEstablisher]: ./establisher/struct.ConnectionEstablisher.html
mod connections;
mod error;
pub mod establisher;
mod manager;
mod protocol;
mod repository;
mod types;

pub(crate) use self::types::EstablishLockResult;
pub use self::{error::ConnectionManagerError, establisher::PeerConnectionConfig, manager::ConnectionManager};

type Result<T> = std::result::Result<T, ConnectionManagerError>;
