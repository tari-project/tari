//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

//! A libp2p protocol for synchronizing peer records between nodes.
//!
//! Peer sync establishes a single peer sync session when connecting to peers. A peer sync runs one at a time until all
//! peers in the want-list are obtained. The want-list is set to the entire validator peer set on startup. Shared peer
//! records are signed.
//!
//! The high-level process is as follows:
//! - All validators subscribe to the `peer-announce` gossipsub topic
//! - The local peer must first confirm at least one address
//! - Once confirmed, they gossipsub their peer record (this only happens once)
//! - This allows subscribed peers to update their peer record for the peer
//! - On connect, a peer will initiate peer sync requesting peers that are not in the peer store but in the want-list
//! - The responder will reply with all peers it has in the want-list
//! - The initiator adds peers if they are more up-to-date than the local store

mod behaviour;
mod config;
mod epoch_time;
pub mod error;
mod event;
mod handler;
mod inbound_task;
mod outbound_task;
mod peer_record;
pub mod proto;
pub mod store;

pub use behaviour::*;
pub use config::*;
pub use error::Error;
pub use event::*;
pub use peer_record::*;

/// The maximum message size permitted for peer messages
pub(crate) const MAX_MESSAGE_SIZE: usize = 1024;
