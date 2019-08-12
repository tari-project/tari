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

//! # Control Service
//!
//! The control service listens on the configured address for [RequestConnection] messages
//! and decides whether to connect to the requested address.
//!
//! Once a control port connection has been established. The protocol for establishing a
//! peer connection is as follows:
//!
//! Node A                  Node B
//!  +                       +
//!  |  ReqConn(n_id, addr)  |
//!  | +-------------------> |
//!  |                       | Create Inbound PeerConnection
//!  |  Accept(c_pk, addr)   |
//!  | <------------------+  | ---
//!  |                       |   | Either Accept or Reject
//!  |   Reject(reason)      |   |
//!  | <------------------+  | ---
//!  |                       |
//!  |                       |
//!  |  Connect to PeerConn  |
//!  | +-------------------> |
//!  |                       |
//!  +                       +
//!
//! ```edition2018
//! # use tari_comms::{connection::*, control_service::*, dispatcher::*, connection_manager::*, peer_manager::*, types::*};
//! # use std::{time::Duration, sync::Arc};
//! # use std::collections::HashMap;
//! # use rand::OsRng;
//! # use tari_storage::lmdb_store::LMDBBuilder;
//! # use lmdb_zero::db;
//! # use tari_storage::key_val_store::lmdb_database::LMDBWrapper;
//!
//! let node_identity = Arc::new(NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap());
//!
//! let context = ZmqContext::new();
//! let listener_address = "127.0.0.1:9000".parse::<NetAddress>().unwrap();
//!
//! let database_name = "cs_peer_database";
//! let datastore = LMDBBuilder::new()
//!            .set_path("/tmp/")
//!            .set_environment_size(10)
//!            .set_max_number_of_databases(2)
//!            .add_database(database_name, lmdb_zero::db::CREATE)
//!           .build().unwrap();
//! let peer_database = datastore.get_handle(database_name).unwrap();
//!     let peer_database = LMDBWrapper::new(Arc::new(peer_database));
//! let peer_manager = Arc::new(PeerManager::new(peer_database).unwrap());
//!
//! let conn_manager = Arc::new(ConnectionManager::new(context.clone(), node_identity.clone(), peer_manager.clone(), PeerConnectionConfig {
//!      max_message_size: 1024,
//!      max_connect_retries: 1,
//!      max_connections: 100,
//!      socks_proxy_address: None,
//!      message_sink_address: InprocAddress::random(),
//!      host: "127.0.0.1".parse().unwrap(),
//!      peer_connection_establish_timeout: Duration::from_secs(4),
//! }));
//!
//! let service = ControlService::with_default_config(
//!       context,
//!       node_identity,
//!     )
//!     .serve(conn_manager)
//!     .unwrap();
//!
//! service.shutdown().unwrap();
//! ```
//!
//! [RequestConnection]: ./messages/struct.RequestConnection.html
mod client;
mod error;
pub mod messages;
mod service;
mod types;
mod worker;

pub use self::{
    client::ControlServiceClient,
    error::ControlServiceError,
    messages::ControlServiceRequestType,
    service::{ControlService, ControlServiceConfig, ControlServiceHandle},
};
