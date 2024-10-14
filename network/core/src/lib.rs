//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

mod worker;

mod error;
pub use error::{DialError, NetworkError};

mod config;
mod connection;
mod event;
mod global_ip;
mod gossip;
mod handle;
mod message;
mod messaging;
mod notify;
mod peer;
mod peer_store;
mod relay_state;
mod service_trait;
mod spawn;
pub mod test_utils;

pub use config::*;
pub use connection::*;
pub use event::*;
pub use gossip::*;
pub use handle::*;
pub use libp2p::{identity, multiaddr, StreamProtocol};
pub use message::*;
pub use messaging::*;
pub use peer::*;
pub use service_trait::*;
pub use spawn::*;
pub use tari_swarm::{
    config::{Config as SwarmConfig, LimitPerInterval, RelayCircuitLimits, RelayReservationLimits},
    is_supported_multiaddr,
    swarm,
    ProtocolVersion,
};
