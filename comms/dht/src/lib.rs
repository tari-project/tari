#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
//! # Tari Comms DHT
//!
//! ## Overview
//!
//! The `tari_comms_dht` crate adds DHT functionality to `tari_comms`.
//! It provides two sets of middleware (_inbound_ and _outbound_) which
//! process incoming requests and outgoing messages respectively.
//!
//! ### Attaching to comms
//!
//! In `tari_comms`, incoming and outgoing messages are connected using two mpsc sender/receiver pairs.
//! One for incoming messages (receiving `InboundMessage`s) and one for outbound messages (sending `OutboundMessage`s).
//!
//! The DHT module consists of two middleware layers (as in `tower_layer::Layer`) which form
//! an inbound and outbound pipeline for augmenting messages.
//!
//! #### Inbound Message Flow
//!
//! `InboundMessage`s are received from the incoming comms channel (as in the receiver side of
//! of the mpsc channel which goes into `CommsBuilder::new().incoming_message_sink(sender)`).
//! Typically, a `ServicePipeline` from the `tari_comms::middleware` crate is used to connect
//! a stream from comms to the middleware service.
//!
//! `InboundMessage`(comms) -> _DHT Inbound Middleware_ -> `DhtInboundMessage`(domain)
//!
//! The DHT inbound middleware consist of:
//! * `DeserializeMiddleware` deserializes the body of an `InboundMessage` into a `DhtEnvelope`.
//! * `DecryptionMiddleware` attempts to decrypt the body of a `DhtEnvelope` if required. The result of that decryption
//!   (success or failure) is passed to the next service.
//! * `ForwardMiddleware` uses the result of the decryption to determine if the message is destined for this node or
//!   not. If not, the message will be forwarded to the applicable peers using the OutboundRequester (i.e. the outbound
//!   DHT middleware).
//! * `DhtHandlerMiddleware` handles DHT messages, such as `Join` and `Discover`. If the messages are _not_ DHT messages
//!   the `next_service` is called.
//!
//! #### Outbound Message Flow
//!
//! `OutboundMessage`s are sent to the outgoing comms channel (as in the receiver side of
//! of the mpsc channel which goes into `CommsBuilder::new().outgoing_message_stream(receiver)`).
//! Typically, a `ServicePipeline` from the `tari_comms::middleware` crate is used to connect
//! a stream from the domain-level to the middleware service and a `SinkMiddleware` to connect
//! the middleware to the OMS in comms. Outbound requests to the DHT middleware are furnished by
//! the `OutboundMessageRequester`, obtained from the `Dht::outbound_requester` factory method.
//!
//! `DhtOutboundRequest` (domain) -> _DHT Outbound Middleware_ -> `OutboundMessage` (comms)
//!
//! The DHT outbound middleware consist of:
//! * `BroadcastMiddleware` produces multiple outbound messages according on the `BroadcastStrategy` from the received
//!   `DhtOutboundRequest` message. The `next_service` is called for each resulting message.
//! * `EncryptionMiddleware` encrypts the body of a message if `DhtMessagheFlags::ENCRYPTED` is given. The result is
//!   passed onto the `next_service`.
//! * `SerializeMiddleware` wraps the body in a `DhtEnvelope`, serializes the result, constructs an `OutboundMessage`
//!   and calls `next_service`. Typically, `next_service` will be a `SinkMiddleware` which send the message to the comms
//!   OMS.
//
//! ## Usage
//!
//! ```edition2018,compile_fail
//! #use tari_comms::middleware::ServicePipeline;
//! #use tari_comms_dht::DhtBuilder;
//! #use tari_comms::middleware::sink::SinkMiddleware;
//! #use tari_comms::peer_manager::NodeIdentity;
//! #use rand::rngs::OsRng;
//! #use std::sync::Arc;
//! #use tari_comms::CommsBuilder;
//! #use tokio::runtime::Runtime;
//! #use futures::channel::mpsc;
//!
//! let runtime = Runtime::new().unwrap();
//! // Channel from comms to inbound dht
//! let (comms_in_tx, comms_in_rx)= mpsc::channel(100);
//! let (comms_out_tx, comms_out_rx)= mpsc::channel(100);
//! let node_identity = NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap())
//!     .map(Arc::new).unwrap();
//! let comms  = CommsBuilder::new(runtime.executor())
//!         // Messages coming from comms
//!        .with_inbound_sink(comms_in_tx)
//!         // Messages going to comms
//!        .with_outbound_stream(comms_out_rx)
//!        .with_node_identity(node_identity)
//!        .build()
//!        .unwrap();
//! let peer_manager = comms.start().unwrap().peer_manager();
//! let dht = DhtBuilder::new(node_identity, peer_manager).finish();
//!
//! let inbound_pipeline = ServicePipeline::new(
//!    comms_in_rx,
//!     // In Tari's case, the service would be a InboundMessageConnector in `tari_p2p`
//!     dht.inbound_middleware_layer(/* some service which uses DhtInboundMessage */ )
//! );
//! // Use the given executor to spawn calls to the middleware
//! inbound_pipeline.spawn_with(rt.executor());
//!
//! let outbound_pipeline = ServicePipeline::new(
//!     dht.take_outbound_receiver(),
//!     // SinkMiddleware sends the resulting OutboundMessages to the comms OMS
//!     dht.outbound_middleware_layer(SinkMiddleware::new(comms_out_tx))
//! );
//! // Use the given executor to spawn calls to the middleware
//! outbound_pipeline.spawn_with(rt.executor());
//!
//! let oms = dht.outbound_requester();
//! oms.send_message(...).await;
//! ```

#![recursion_limit = "256"]
// Details: https://doc.rust-lang.org/beta/unstable-book/language-features/type-alias-impl-trait.html
#![feature(type_alias_impl_trait)]

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;

#[macro_use]
mod macros;

#[cfg(test)]
#[macro_use]
mod test_utils;

mod actor;
pub use actor::{DhtActorError, DhtRequest, DhtRequester};

mod builder;
pub use builder::DhtBuilder;

mod connectivity;

mod config;
pub use config::DhtConfig;

mod consts;
mod crypt;

mod dht;
pub use dht::{Dht, DhtInitializationError};

mod discovery;
pub use discovery::DhtDiscoveryRequester;

mod network_discovery;
pub use network_discovery::NetworkDiscoveryConfig;

mod storage;
pub use storage::DbConnectionUrl;

mod dedup;
pub use dedup::DedupLayer;

mod logging_middleware;
mod proto;
mod rpc;
mod schema;
mod tower_filter;

pub mod broadcast_strategy;
pub mod domain_message;
pub mod envelope;
pub mod event;
pub mod inbound;
pub mod outbound;
pub mod store_forward;
