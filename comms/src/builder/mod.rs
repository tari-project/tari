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
//! use tari_comms::builder::{CommsBuilder, CommsRoutes};
//! use tari_comms::dispatcher::HandlerError;
//! use tari_comms::message::DomainMessageContext;
//! use tari_comms::control_service::ControlServiceConfig;
//! use tari_comms::peer_manager::NodeIdentity;
//! use std::sync::Arc;
//! use rand::OsRng;
//!
//! // This should be loaded up from storage
//! let my_node_identity = NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap();
//!
//! fn my_handler(_: DomainMessageContext) -> Result<(), HandlerError> {
//!     println!("Your handler is called!");
//!     Ok(())
//! }
//!
//! let services = CommsBuilder::new()
//!    .with_routes(CommsRoutes::<u8>::new())
//!    // This enables the control service - allowing another peer to connect to this node
//!    .configure_control_service(ControlServiceConfig::default())
//!    .with_node_identity(my_node_identity)
//!    .build()
//!    .unwrap();
//!
//! let handle = services.start().unwrap();
//! // Call shutdown when program shuts down
//! handle.shutdown();
//! ```
//!
//! [CommsBuilder]: ./builder/struct.CommsBuilder.html

mod builder;
mod routes;

pub use self::{
    builder::{CommsBuilder, CommsBuilderError, CommsServices, CommsServicesError},
    routes::CommsRoutes,
};
