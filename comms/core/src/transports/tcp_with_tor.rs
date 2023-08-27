// Copyright 2019, The Taiji Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::io;

use multiaddr::Multiaddr;
use tokio::net::TcpStream;

use super::Transport;
use crate::transports::{dns::TorDnsResolver, predicate::is_onion_address, SocksConfig, SocksTransport, TcpTransport};

/// Transport implementation for TCP with Tor support
#[derive(Clone, Default)]
pub struct TcpWithTorTransport {
    socks_transport: Option<SocksTransport>,
    tcp_transport: TcpTransport,
}

impl TcpWithTorTransport {
    /// Sets the SOCKS address to the Tor proxy to use for onion and DNS address resolution
    pub fn set_tor_socks_proxy(&mut self, socks_config: SocksConfig) -> &mut Self {
        self.socks_transport = Some(SocksTransport::new(socks_config.clone()));
        // Resolve DNS using the tor proxy
        self.tcp_transport.set_dns_resolver(TorDnsResolver::new(socks_config));
        self
    }

    /// Create a new TcpTransport with the Tor socks proxy enabled
    pub fn with_tor_socks_proxy(socks_config: SocksConfig) -> Self {
        let mut transport = Self::default();
        transport.set_tor_socks_proxy(socks_config);
        transport
    }

    /// Create a new TcpTransport
    pub fn new() -> Self {
        Default::default()
    }

    pub fn tcp_transport_mut(&mut self) -> &mut TcpTransport {
        &mut self.tcp_transport
    }
}

#[crate::async_trait]
impl Transport for TcpWithTorTransport {
    type Error = io::Error;
    type Listener = <TcpTransport as Transport>::Listener;
    type Output = TcpStream;

    async fn listen(&self, addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
        self.tcp_transport.listen(addr).await
    }

    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
        if addr.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid address '{}'", addr),
            ));
        }

        if is_onion_address(addr) {
            match self.socks_transport {
                Some(ref transport) => {
                    let socket = transport.dial(addr).await?;
                    Ok(socket)
                },
                None => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Tor SOCKS proxy is not set for TCP transport. Cannot dial peer with onion addresses.".to_owned(),
                )),
            }
        } else {
            let socket = self.tcp_transport.dial(addr).await?;
            Ok(socket)
        }
    }
}
