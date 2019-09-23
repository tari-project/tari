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

//! # CommsBuilder
//!
//! The [CommsBuilder] provides a simple builder API for getting Tari comms p2p messaging up and running.
//!
//! ```edition2018
//! # use tari_comms::builder::{CommsBuilder, CommsServices};
//! # use tari_comms::control_service::ControlServiceConfig;
//! # use tari_comms::peer_manager::NodeIdentity;
//! # use std::sync::Arc;
//! # use rand::OsRng;
//! # use tari_storage::lmdb_store::LMDBBuilder;
//! # use lmdb_zero::db;
//! # use tari_storage::LMDBWrapper;
//! # use futures::executor::ThreadPool;
//! # use tokio::runtime::Runtime;
//! # use futures::channel::mpsc;
//! # use tari_comms::middleware::SinkMiddleware;
//! // This should be loaded up from storage
//! let my_node_identity = NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap();
//!
//! let database_name = "b_peer_database";
//! let datastore = LMDBBuilder::new()
//!            .set_path("/tmp/")
//!            .set_environment_size(10)
//!            .set_max_number_of_databases(2)
//!            .add_database(database_name, lmdb_zero::db::CREATE)
//!           .build().unwrap();
//! let peer_database = datastore.get_handle(database_name).unwrap();
//! let peer_database = LMDBWrapper::new(Arc::new(peer_database));
//!
//! // Futures mpsc channel where all incoming messages will be received
//! let (sender, _receiver) = mpsc::channel(100);
//! let runtime = Runtime::new().unwrap();
//! let services = CommsBuilder::new(runtime.executor())
//!    .with_inbound_middleware(|_: CommsServices| SinkMiddleware::new(sender))
//!    // This enables the control service - allowing another peer to connect to this node
//!    .configure_control_service(ControlServiceConfig::default())
//!    .with_node_identity(Arc::new(my_node_identity))
//!    .with_peer_storage(peer_database)
//!    .build()
//!    .unwrap();
//!
//! let mut handle = services.start().unwrap();
//!
//! // use _receiver to receive comms messages
//! // _receiver.next().await
//!
//! // Call shutdown when program shuts down
//! handle.shutdown();
//! ```
//!
//! [CommsBuilder]: ./builder/struct.CommsBuilder.html

mod builder;

pub use self::builder::{CommsBuilder, CommsBuilderError, CommsError, CommsNode, CommsServices};
