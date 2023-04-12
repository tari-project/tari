//  Copyright 2022. The Tari Project
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
use std::{num::NonZeroU16, sync::Arc};

use serde::{Deserialize, Serialize};
use tari_comms::{
    multiaddr::Multiaddr,
    socks,
    tor,
    tor::TorIdentity,
    transports::{predicate::FalsePredicate, SocksConfig},
    utils::multiaddr::multiaddr_to_socketaddr,
};

use crate::{initialization::CommsInitializationError, SocksAuthentication, TorControlAuthentication};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TransportConfig {
    #[serde(rename = "type")]
    pub transport_type: TransportType,
    pub tcp: TcpTransportConfig,
    pub tor: TorTransportConfig,
    pub socks: Socks5TransportConfig,
    pub memory: MemoryTransportConfig,
}

impl TransportConfig {
    pub fn new_memory(config: MemoryTransportConfig) -> Self {
        Self {
            transport_type: TransportType::Memory,
            memory: config,
            ..Default::default()
        }
    }

    pub fn new_tcp(config: TcpTransportConfig) -> Self {
        Self {
            transport_type: TransportType::Tcp,
            tcp: config,
            ..Default::default()
        }
    }

    pub fn new_tor(config: TorTransportConfig) -> Self {
        Self {
            transport_type: TransportType::Tor,
            tor: config,
            ..Default::default()
        }
    }

    pub fn new_socks5(forward_address: Multiaddr, config: Socks5TransportConfig) -> Self {
        Self {
            transport_type: TransportType::Socks5,
            socks: config,
            tcp: TcpTransportConfig {
                listener_address: forward_address,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    pub fn is_tor(&self) -> bool {
        matches!(self.transport_type, TransportType::Tor)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum TransportType {
    /// Memory transport. Supports a single address type in the form '/memory/x' and can only communicate in-process.
    Memory,
    /// Use TCP to join the Tari network. By default, this transport can only contact TCP/IP nodes, however it can be
    /// configured to allow communication with peers using the tor transport.
    Tcp,
    /// Configures the node to run over a tor hidden service using the Tor proxy. This transport can connect to TCP/IP,
    /// onion v3 and DNS addresses.
    Tor,
    /// Use a SOCKS5 proxy transport. This transport allows any addresses supported by the proxy.
    Socks5,
}

impl Default for TransportType {
    fn default() -> Self {
        // The tor transport configures itself as long as it has access to the control port at
        // `TorConfig::control_address`
        Self::Tor
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TcpTransportConfig {
    /// Socket to bind the TCP listener
    pub listener_address: Multiaddr,
    /// Optional socket address of the tor SOCKS proxy, enabling the node to communicate with Tor nodes
    pub tor_socks_address: Option<Multiaddr>,
    /// Optional tor SOCKS proxy authentication
    pub tor_socks_auth: SocksAuthentication,
}

impl Default for TcpTransportConfig {
    fn default() -> Self {
        Self {
            listener_address: "/ip4/0.0.0.0/tcp/18189".parse().unwrap(),
            tor_socks_address: None,
            tor_socks_auth: SocksAuthentication::None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TorTransportConfig {
    /// The address of the control server
    pub control_address: Multiaddr,
    /// SOCKS proxy auth
    pub socks_auth: SocksAuthentication,
    /// Use this socks address instead of getting it from the tor proxy.
    pub socks_address_override: Option<Multiaddr>,
    pub control_auth: TorControlAuthentication,
    pub onion_port: NonZeroU16,
    /// When these peer addresses are encountered when dialing another peer, the tor proxy is bypassed and the
    /// connection is made directly over TCP. /ip4, /ip6, /dns, /dns4 and /dns6 are supported.
    pub proxy_bypass_addresses: Vec<Multiaddr>,
    /// When set to true, outbound TCP connections bypass the tor proxy. Defaults to false for better privacy, setting
    /// to true may improve network performance for TCP nodes.
    pub proxy_bypass_for_outbound_tcp: bool,
    /// If set, instructs tor to forward traffic the the provided address.
    pub forward_address: Option<Multiaddr>,
    /// The tor identity to use to create the hidden service. If None, a new one will be generated.
    #[serde(skip)]
    pub identity: Option<TorIdentity>,
}

impl TorTransportConfig {
    /// Returns a [self::tor::PortMapping] struct that maps the [onion_port] to an address that is listening for
    /// traffic. If [forward_address] is set, that address is used, otherwise 127.0.0.1:[onion_port] is used.
    ///
    /// [onion_port]: TorTransportConfig::onion_port
    /// [forward_address]: TorTransportConfig::forward_address
    pub fn to_port_mapping(&self) -> Result<tor::PortMapping, CommsInitializationError> {
        let forward_addr = self
            .forward_address
            .as_ref()
            .map(multiaddr_to_socketaddr)
            .transpose()
            .map_err(CommsInitializationError::InvalidTorForwardAddress)?
            .unwrap_or_else(|| ([127, 0, 0, 1], 0).into());

        Ok(tor::PortMapping::new(self.onion_port.get(), forward_addr))
    }

    pub fn to_control_auth(&self) -> Result<tor::Authentication, CommsInitializationError> {
        self.control_auth
            .clone()
            .make_tor_auth()
            .map_err(CommsInitializationError::from)
    }

    pub fn to_socks_auth(&self) -> socks::Authentication {
        self.socks_auth.clone().into()
    }
}

impl Default for TorTransportConfig {
    fn default() -> Self {
        Self {
            control_address: "/ip4/127.0.0.1/tcp/9051".parse().unwrap(),
            socks_auth: SocksAuthentication::None,
            socks_address_override: None,
            control_auth: TorControlAuthentication::Auto,
            onion_port: NonZeroU16::new(18141).unwrap(),
            proxy_bypass_addresses: vec![],
            proxy_bypass_for_outbound_tcp: false,
            forward_address: None,
            identity: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Socks5TransportConfig {
    pub proxy_address: Multiaddr,
    pub auth: SocksAuthentication,
}

impl From<Socks5TransportConfig> for SocksConfig {
    fn from(config: Socks5TransportConfig) -> Self {
        Self {
            proxy_address: config.proxy_address,
            authentication: config.auth.into(),
            proxy_bypass_predicate: Arc::new(FalsePredicate::new()),
        }
    }
}

impl Default for Socks5TransportConfig {
    fn default() -> Self {
        Self {
            proxy_address: "/ip4/127.0.0.1/tcp/8080".parse().unwrap(),
            auth: SocksAuthentication::None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryTransportConfig {
    pub listener_address: Multiaddr,
}

impl Default for MemoryTransportConfig {
    fn default() -> Self {
        Self {
            listener_address: "/memory/0".parse().unwrap(),
        }
    }
}
