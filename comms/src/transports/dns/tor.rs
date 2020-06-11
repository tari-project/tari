//  Copyright 2020, The Tari Project
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

use super::{DnsResolver, DnsResolverError};
use crate::{
    multiaddr::Multiaddr,
    socks::Socks5Client,
    transports::{dns::common, SocksConfig, SocksTransport, TcpTransport, Transport},
};
use futures::future::BoxFuture;
use log::*;
use std::{io, net::SocketAddr};

const LOG_TARGET: &str = "comms::dns::tor_resolver";

/// Resolves DNS addresses using the tor proxy
#[derive(Debug, Clone)]
pub struct TorDnsResolver {
    socks_config: SocksConfig,
}

type TcpSocks5Client = Socks5Client<<TcpTransport as Transport>::Output>;

impl TorDnsResolver {
    pub fn new(socks_config: SocksConfig) -> Self {
        Self { socks_config }
    }

    pub async fn connect(self) -> Result<TcpSocks5Client, DnsResolverError> {
        let mut client = connect_inner(self.socks_config.proxy_address)
            .await
            .map_err(DnsResolverError::ProxyConnectFailed)?;
        client.with_authentication(self.socks_config.authentication)?;
        Ok(client)
    }
}

async fn connect_inner(addr: Multiaddr) -> io::Result<TcpSocks5Client> {
    let socket = SocksTransport::get_tcp_transport().dial(addr)?.await?;
    Ok(Socks5Client::new(socket))
}

impl DnsResolver for TorDnsResolver {
    fn resolve(&self, addr: Multiaddr) -> BoxFuture<'static, Result<SocketAddr, DnsResolverError>> {
        let resolver = self.clone();
        Box::pin(async move {
            let addr = if common::is_dns4_addr(&addr) {
                let mut client = resolver.connect().await?;
                debug!(target: LOG_TARGET, "Resolving address `{}` using tor", addr);
                let resolved = match client.tor_resolve(&addr).await {
                    Ok(a) => a,
                    Err(err) => {
                        error!(target: LOG_TARGET, "{}", err);
                        return Err(err.into());
                    },
                };
                debug!(target: LOG_TARGET, "Resolved address `{}` using tor", resolved);
                resolved
            } else {
                addr
            };
            common::convert_tcpip_multiaddr_to_socketaddr(&addr).map_err(Into::into)
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // This only works when a tor proxy is running
    #[ignore]
    #[crate::runtime::test]
    async fn resolve() {
        let resolver = TorDnsResolver::new(SocksConfig {
            proxy_address: "/ip4/127.0.0.1/tcp/9050".parse().unwrap(),
            authentication: Default::default(),
        });

        let addr = resolver
            .resolve("/dns4/tari.com/tcp/443".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(addr.port(), 443);
    }
}
