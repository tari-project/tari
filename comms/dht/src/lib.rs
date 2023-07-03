// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![doc(html_root_url = "https://docs.rs/tower-filter/0.3.0-alpha.2")]

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
//! * metrics: monitors the number of inbound messages
//! * decryption: deserializes and decrypts the `InboundMessage` and produces a
//!   [DecryptedDhtMessage](crate::inbound::DecryptedDhtMessage).
//! * dedup: discards the message if previously received.
//! * logging: message logging
//! * SAF storage: stores certain messages for other peers in the SAF store.
//! * message storage: forwards messages for other peers.
//! * SAF message handler: handles SAF protocol messages (requests for SAF messages, SAF message responses).
//! * DHT message handler: handles DHT protocol messages (discovery, join etc.)
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
//! * broadcast layer: produces multiple outbound messages according on the `BroadcastStrategy` from the received
//!   `DhtOutboundRequest` message. The `next_service` is called for each resulting message.
//! * message logger layer.
//! * serialization: wraps the body in a [DhtOutboundMessage](crate::outbound::DhtOutboundMessage), serializes the
//!   result, constructs an `OutboundMessage` and calls `next_service`. Typically, `next_service` will be a
//!   `SinkMiddleware` which send the message to comms messaging.

#![recursion_limit = "256"]
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
pub use connectivity::MetricsCollectorHandle;

mod config;
pub use config::{DhtConfig, DhtConnectivityConfig};

mod crypt;

mod dht;
pub use dht::{Dht, DhtInitializationError};

mod discovery;
pub use discovery::{DhtDiscoveryError, DhtDiscoveryRequester};

mod error;
pub use error::DhtEncryptError;

mod network_discovery;
pub use network_discovery::NetworkDiscoveryConfig;

mod storage;
pub use storage::DbConnectionUrl;

mod dedup;
pub use dedup::DedupLayer;

mod filter;
mod logging_middleware;
mod message_signature;
mod peer_validator;
mod proto;
mod rpc;
mod schema;

mod version;
pub use version::DhtProtocolVersion;

pub mod broadcast_strategy;
pub mod domain_message;
pub mod envelope;
pub mod event;
pub mod inbound;
pub mod outbound;
pub mod store_forward;

use tari_comms::types::CommsChallenge;
use tari_crypto::{hash_domain, hashing::DomainSeparatedHasher};

hash_domain!(DHTCommsHashDomain, "com.tari.comms.dht");

pub fn comms_dht_hash_domain_challenge() -> DomainSeparatedHasher<CommsChallenge, DHTCommsHashDomain> {
    DomainSeparatedHasher::<CommsChallenge, DHTCommsHashDomain>::new_with_label("challenge")
}

pub fn comms_dht_hash_domain_key_message() -> DomainSeparatedHasher<CommsChallenge, DHTCommsHashDomain> {
    DomainSeparatedHasher::<CommsChallenge, DHTCommsHashDomain>::new_with_label("key_message")
}

pub fn comms_dht_hash_domain_key_mask() -> DomainSeparatedHasher<CommsChallenge, DHTCommsHashDomain> {
    DomainSeparatedHasher::<CommsChallenge, DHTCommsHashDomain>::new_with_label("key_mask")
}

pub fn comms_dht_hash_domain_message_signature() -> DomainSeparatedHasher<CommsChallenge, DHTCommsHashDomain> {
    DomainSeparatedHasher::<CommsChallenge, DHTCommsHashDomain>::new_with_label("message_signature")
}
