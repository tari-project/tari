//   Copyright 2022 The Tari Project
//   SPDX-License-Identifier: BSD-4-Clause

mod behaviour;
pub mod config;
mod error;
mod protocol_version;

pub use behaviour::*;
pub use config::Config;
pub use error::*;
pub use protocol_version::*;

pub type TariSwarm<TMsg> = libp2p::Swarm<TariNodeBehaviour<TMsg>>;

pub use libp2p_messaging as messaging;
pub use libp2p_peersync as peersync;
pub use libp2p_substream as substream;
