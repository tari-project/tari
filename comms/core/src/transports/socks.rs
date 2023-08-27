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

use std::{
    fmt::{Debug, Formatter},
    io,
    sync::Arc,
};

use log::debug;
use tokio::net::TcpStream;

use crate::{
    multiaddr::Multiaddr,
    socks,
    socks::Socks5Client,
    transports::{dns::SystemDnsResolver, predicate::Predicate, tcp::TcpTransport, Transport},
};

const LOG_TARGET: &str = "comms::transports::socks";

/// SOCKS proxy client config
#[derive(Clone)]
pub struct SocksConfig {
    pub proxy_address: Multiaddr,
    pub authentication: socks::Authentication,
    pub proxy_bypass_predicate: Arc<dyn Predicate<Multiaddr> + Send + Sync>,
}

impl Debug for SocksConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SocksConfig")
            .field("proxy_address", &self.proxy_address)
            .field("authentication", &self.authentication)
            .field("proxy_bypass_predicate", &"...")
            .finish()
    }
}

/// Transport over the SOCKS5 protocol
#[derive(Clone)]
pub struct SocksTransport {
    socks_config: SocksConfig,
    tcp_transport: TcpTransport,
}

impl SocksTransport {
    pub fn new(socks_config: SocksConfig) -> Self {
        Self {
            socks_config,
            tcp_transport: Self::create_socks_tcp_transport(),
        }
    }

    pub fn create_socks_tcp_transport() -> TcpTransport {
        let mut tcp_transport = TcpTransport::new();
        tcp_transport.set_dns_resolver(SystemDnsResolver);
        tcp_transport
    }

    async fn socks_connect(
        tcp: TcpTransport,
        socks_config: &SocksConfig,
        dest_addr: &Multiaddr,
    ) -> io::Result<TcpStream> {
        // Create a new connection to the SOCKS proxy
        let socks_conn = tcp.dial(&socks_config.proxy_address).await?;
        let mut client = Socks5Client::new(socks_conn);

        client
            .with_authentication(socks_config.authentication.clone())
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        client
            .connect(dest_addr)
            .await
            .map(|(socket, _)| socket)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }
}

#[crate::async_trait]
impl Transport for SocksTransport {
    type Error = <TcpTransport as Transport>::Error;
    type Listener = <TcpTransport as Transport>::Listener;
    type Output = <TcpTransport as Transport>::Output;

    async fn listen(&self, addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
        self.tcp_transport.listen(addr).await
    }

    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
        // Bypass the SOCKS proxy and connect to the address directly
        if self.socks_config.proxy_bypass_predicate.check(addr) {
            debug!(target: LOG_TARGET, "SOCKS proxy bypassed for '{}'. Using TCP.", addr);
            return self.tcp_transport.dial(addr).await;
        }

        let socket = Self::socks_connect(self.tcp_transport.clone(), &self.socks_config, addr).await?;
        Ok(socket)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{socks::Authentication, transports::predicate::FalsePredicate};

    #[test]
    fn new() {
        let proxy_address = "/ip4/127.0.0.1/tcp/1234".parse::<Multiaddr>().unwrap();
        let transport = SocksTransport::new(SocksConfig {
            proxy_address: proxy_address.clone(),
            authentication: Default::default(),
            proxy_bypass_predicate: Arc::new(FalsePredicate::new()),
        });

        assert_eq!(transport.socks_config.proxy_address, proxy_address);
        assert_eq!(transport.socks_config.authentication, Authentication::None);
    }
}
