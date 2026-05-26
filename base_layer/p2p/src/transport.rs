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
use tari_common::configuration::MultiaddrList;
use tari_comms::{
    multiaddr::Multiaddr,
    socks,
    tor::{self, TorIdentity},
    transports::{SocksConfig, predicate::FalsePredicate},
    types::TransportProtocol,
    utils::multiaddr::multiaddr_to_socketaddr,
};

use crate::{SocksAuthentication, TorControlAuthentication, initialization::CommsInitializationError};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TransportConfig {
    #[serde(rename = "type")]
    pub transport_type: TransportType,
    pub tcp: TcpTransportConfig,
    pub tor: TorTransportConfig,
    pub socks: Socks5TransportConfig,
    pub memory: MemoryTransportConfig,
    #[serde(skip)]
    transport_override: TransportOverride,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
enum TransportOverride {
    #[default]
    None,
    Memory,
    Socks5,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum CommsTransport {
    Memory,
    Tcp,
    TorHiddenService,
    Socks5,
}

impl TransportConfig {
    pub fn new_memory(config: MemoryTransportConfig) -> Self {
        Self {
            memory: config,
            transport_override: TransportOverride::Memory,
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
            socks: config,
            tcp: TcpTransportConfig {
                listener_address: forward_address,
                ..Default::default()
            },
            transport_override: TransportOverride::Socks5,
            ..Default::default()
        }
    }

    pub fn get_supported_protocols(&self) -> Vec<TransportProtocol> {
        match self.transport_override {
            TransportOverride::None => self.transport_type.get_supported_protocols(),
            TransportOverride::Memory => vec![TransportProtocol::Memory],
            TransportOverride::Socks5 => vec![
                TransportProtocol::Onion,
                TransportProtocol::Ipv4,
                TransportProtocol::Ipv6,
            ],
        }
    }

    pub(crate) fn comms_transport(&self) -> CommsTransport {
        match self.transport_override {
            TransportOverride::Memory => CommsTransport::Memory,
            TransportOverride::Socks5 => CommsTransport::Socks5,
            TransportOverride::None => self.transport_type.comms_transport(),
        }
    }

    pub fn is_memory_transport(&self) -> bool {
        matches!(self.comms_transport(), CommsTransport::Memory)
    }

    pub fn is_socks5_transport(&self) -> bool {
        matches!(self.comms_transport(), CommsTransport::Socks5)
    }

    pub fn uses_tor_hidden_service(&self) -> bool {
        matches!(self.comms_transport(), CommsTransport::TorHiddenService)
    }

    pub fn is_tor(&self) -> bool {
        self.uses_tor_hidden_service()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum TransportType {
    /// Only onion peers are selected and dialed.
    Tor,
    /// Only TCP/IP peers are selected and dialed.
    Tcp,
    /// Onion and TCP/IP peers are selected, with onion preferred.
    TorTcp,
    /// TCP/IP and onion peers are selected, with TCP/IP preferred.
    #[default]
    TcpTor,
}

impl TransportType {
    pub(crate) fn comms_transport(&self) -> CommsTransport {
        match self {
            TransportType::Tor => CommsTransport::TorHiddenService,
            TransportType::Tcp | TransportType::TorTcp | TransportType::TcpTor => CommsTransport::Tcp,
        }
    }

    pub fn uses_tor_hidden_service(&self) -> bool {
        matches!(self.comms_transport(), CommsTransport::TorHiddenService)
    }

    pub fn get_supported_protocols(&self) -> Vec<TransportProtocol> {
        match self {
            TransportType::Tor => vec![TransportProtocol::Onion],
            TransportType::Tcp => vec![TransportProtocol::Ipv4, TransportProtocol::Ipv6],
            TransportType::TorTcp => vec![
                TransportProtocol::Onion,
                TransportProtocol::Ipv4,
                TransportProtocol::Ipv6,
            ],
            TransportType::TcpTor => vec![
                TransportProtocol::Ipv4,
                TransportProtocol::Ipv6,
                TransportProtocol::Onion,
            ],
        }
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
    pub proxy_bypass_addresses: MultiaddrList,
    /// When set to true, outbound TCP connections bypass the tor proxy. Defaults to 'true' for better network
    /// performance for TCP nodes; set it to 'false' for better privacy.
    pub proxy_bypass_for_outbound_tcp: bool,
    /// If set, instructs tor to forward traffic the provided address. Otherwise, an OS-assigned port on 127.0.0.1
    /// is used.
    pub forward_address: Option<Multiaddr>,
    /// If set, the listener will bind to this address instead of the forward_address.
    pub listener_address_override: Option<Multiaddr>,
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
            proxy_bypass_addresses: MultiaddrList::new(),
            proxy_bypass_for_outbound_tcp: true,
            forward_address: None,
            listener_address_override: None,
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
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TransportTypeToml {
        #[serde(rename = "type")]
        transport_type: TransportType,
    }

    #[test]
    fn transport_type_default_is_tcp_tor() {
        assert_eq!(TransportType::default(), TransportType::TcpTor);
        assert_eq!(TransportConfig::default().transport_type, TransportType::TcpTor);
    }

    #[test]
    fn transport_type_protocol_order_matches_peer_selection_modes() {
        assert_eq!(TransportType::Tor.get_supported_protocols(), vec![
            TransportProtocol::Onion
        ]);
        assert_eq!(TransportType::Tcp.get_supported_protocols(), vec![
            TransportProtocol::Ipv4,
            TransportProtocol::Ipv6
        ]);
        assert_eq!(TransportType::TorTcp.get_supported_protocols(), vec![
            TransportProtocol::Onion,
            TransportProtocol::Ipv4,
            TransportProtocol::Ipv6,
        ]);
        assert_eq!(TransportType::TcpTor.get_supported_protocols(), vec![
            TransportProtocol::Ipv4,
            TransportProtocol::Ipv6,
            TransportProtocol::Onion,
        ]);
    }

    #[test]
    fn transport_type_toml_names_are_stable() {
        for (transport_type, expected_name) in [
            (TransportType::Tor, "tor"),
            (TransportType::Tcp, "tcp"),
            (TransportType::TorTcp, "tor_tcp"),
            (TransportType::TcpTor, "tcp_tor"),
        ] {
            let encoded = toml::to_string(&TransportTypeToml { transport_type }).unwrap();
            assert_eq!(encoded, format!("type = \"{}\"\n", expected_name));

            let decoded: TransportTypeToml = toml::from_str(&format!("type = \"{}\"", expected_name)).unwrap();
            assert_eq!(decoded.transport_type, transport_type);
        }
    }

    #[test]
    fn internal_transports_keep_their_protocols_outside_transport_type() {
        let memory = TransportConfig::new_memory(MemoryTransportConfig::default());
        assert_eq!(memory.transport_type, TransportType::TcpTor);
        assert!(memory.is_memory_transport());
        assert_eq!(memory.comms_transport(), CommsTransport::Memory);
        assert_eq!(memory.get_supported_protocols(), vec![TransportProtocol::Memory]);

        let socks = TransportConfig::new_socks5(
            "/ip4/127.0.0.1/tcp/9050".parse().unwrap(),
            Socks5TransportConfig::default(),
        );
        assert_eq!(socks.transport_type, TransportType::TcpTor);
        assert!(socks.is_socks5_transport());
        assert_eq!(socks.comms_transport(), CommsTransport::Socks5);
        assert_eq!(socks.get_supported_protocols(), vec![
            TransportProtocol::Onion,
            TransportProtocol::Ipv4,
            TransportProtocol::Ipv6,
        ]);
    }

    #[test]
    fn only_tor_mode_uses_the_hidden_service_setup() {
        assert!(TransportType::Tor.uses_tor_hidden_service());
        assert!(!TransportType::Tcp.uses_tor_hidden_service());
        assert!(!TransportType::TorTcp.uses_tor_hidden_service());
        assert!(!TransportType::TcpTor.uses_tor_hidden_service());

        let tor = TransportConfig::new_tor(TorTransportConfig::default());
        assert_eq!(tor.comms_transport(), CommsTransport::TorHiddenService);
        assert!(tor.uses_tor_hidden_service());

        let tor_tcp = TransportConfig {
            transport_type: TransportType::TorTcp,
            ..Default::default()
        };
        assert_eq!(tor_tcp.comms_transport(), CommsTransport::Tcp);
        assert!(!tor_tcp.uses_tor_hidden_service());

        let tcp_tor = TransportConfig::default();
        assert_eq!(tcp_tor.comms_transport(), CommsTransport::Tcp);
        assert!(!tcp_tor.uses_tor_hidden_service());
    }
}
