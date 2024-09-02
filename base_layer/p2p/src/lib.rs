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

#[cfg(test)]
#[macro_use]
mod test_utils;

#[cfg(feature = "auto-update")]
pub mod auto_update;
pub mod comms_connector;
mod config;
pub mod domain_message;
pub mod initialization;
pub mod peer;
pub mod peer_seeds;
pub mod proto;
pub mod services;
mod socks_authentication;
pub mod tari_message;
mod tor_authentication;
pub mod transport;

mod dns;

// Re-export
pub use socks_authentication::SocksAuthentication;
pub use tari_common::configuration::Network;
pub use tor_authentication::TorControlAuthentication;
pub use transport::{Socks5TransportConfig, TcpTransportConfig, TorTransportConfig, TransportConfig, TransportType};

pub use self::config::{P2pConfig, PeerSeedsConfig};

/// Default DNS resolver set to cloudflare's private 1.1.1.1 resolver
pub const DEFAULT_DNS_NAME_SERVER: &str = "1.1.1.1:853/cloudflare-dns.com";

/// Major network version. Peers will refuse connections if this value differs
pub const MAJOR_NETWORK_VERSION: u8 = 0;
/// Minor network version. This should change with each time the network protocol has changed in a backward-compatible
/// way.
pub const MINOR_NETWORK_VERSION: u8 = 0;

// This function returns the network wire byte for any chosen network. Increase these numbers for any given network when
// network traffic separation is required.
// Note: Do not re-use previous values.
fn get_network_wire_byte(network: Network) -> Result<u8, anyhow::Error> {
    let network_wire_byte = match network {
        Network::MainNet => 0,
        Network::StageNet => 40,
        Network::NextNet => 80,
        Network::LocalNet => 120,
        Network::Igor => 160,
        Network::Esmeralda => 200,
    };
    verify_network_wire_byte_range(network_wire_byte, network)?;
    Ok(network_wire_byte)
}

// This function bins the range of u8 numbers for any chosen network to a valid network_wire_byte_range.
// Note: Do not change these ranges.
fn verify_network_wire_byte_range(network_wire_byte: u8, network: Network) -> Result<(), anyhow::Error> {
    if network_wire_byte == 0x46 {
        return Err(anyhow::anyhow!("Invalid network wire byte, cannot be 0x46 (E)"));
    }

    let valid = match network {
        Network::MainNet => (0..40).contains(&network_wire_byte),
        Network::StageNet => (40..80).contains(&network_wire_byte),
        Network::NextNet => (80..120).contains(&network_wire_byte),
        Network::LocalNet => (120..160).contains(&network_wire_byte),
        Network::Igor => (160..200).contains(&network_wire_byte),
        Network::Esmeralda => (200..240).contains(&network_wire_byte),
    };
    if !valid {
        return Err(anyhow::anyhow!(
            "Invalid network wire byte `{}` for network `{}`",
            network_wire_byte,
            network
        ));
    }
    Ok(())
}
