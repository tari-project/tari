// Copyright 2019, The Tari Project
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

use crate::{
    socks,
    socks::Socks5Client,
    transports::{tcp::TcpTransport, TcpSocket, Transport},
};
use futures::{Future, FutureExt};
use multiaddr::{AddrComponent, Multiaddr};
use std::io;

#[derive(Clone, Debug)]
pub struct SocksConfig {
    pub proxy_address: Multiaddr,
    pub authentication: socks::Authentication,
}

#[derive(Clone, Debug)]
pub struct SocksTransport {
    socks_config: SocksConfig,
    tcp_transport: TcpTransport,
}

impl SocksTransport {
    pub fn new(socks_config: SocksConfig) -> Self {
        let mut tcp_transport = TcpTransport::new();
        tcp_transport.set_nodelay(true);

        Self {
            socks_config,
            tcp_transport,
        }
    }

    async fn socks_connect(
        tcp: TcpTransport,
        socks_config: SocksConfig,
        dest_addr: Multiaddr,
    ) -> io::Result<(TcpSocket, Multiaddr)>
    {
        // Create a new connection to the SOCKS proxy
        let (socks_conn, _) = tcp.dial(socks_config.proxy_address).await?;
        let mut client = Socks5Client::new(socks_conn);

        client
            .with_authentication(socks_config.authentication)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        client
            .connect(&dest_addr)
            .await
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    fn extract_proxied_address(addr: &Multiaddr) -> io::Result<Multiaddr> {
        let mut addr_iter = addr.iter();

        match (addr_iter.next(), addr_iter.next(), addr_iter.next()) {
            (Some(AddrComponent::ONION(_, _)), Some(AddrComponent::IP4(ip)), Some(AddrComponent::TCP(port))) => {
                Ok(multiaddr_from_components!(IP4(ip), TCP(port)))
            },
            (Some(AddrComponent::ONION(_, _)), Some(AddrComponent::IP6(ip)), Some(AddrComponent::TCP(port))) => {
                Ok(multiaddr_from_components!(IP6(ip), TCP(port)))
            },
            (Some(AddrComponent::ONION3(_, _)), Some(AddrComponent::IP4(ip)), Some(AddrComponent::TCP(port))) => {
                Ok(multiaddr_from_components!(IP4(ip), TCP(port)))
            },
            (Some(AddrComponent::ONION3(_, _)), Some(AddrComponent::IP6(ip)), Some(AddrComponent::TCP(port))) => {
                Ok(multiaddr_from_components!(IP6(ip), TCP(port)))
            },
            (Some(AddrComponent::IP4(ip)), Some(AddrComponent::TCP(port)), None) => {
                Ok(multiaddr_from_components!(IP4(ip), TCP(port)))
            },
            (Some(AddrComponent::IP6(ip)), Some(AddrComponent::TCP(port)), None) => {
                Ok(multiaddr_from_components!(IP6(ip), TCP(port)))
            },
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("TorTransport does not support the address '{}'.", addr),
            )),
        }
    }
}

impl Transport for SocksTransport {
    type Error = <TcpTransport as Transport>::Error;
    type Inbound = <TcpTransport as Transport>::Inbound;
    type Listener = <TcpTransport as Transport>::Listener;
    type Output = <TcpTransport as Transport>::Output;

    type DialFuture = impl Future<Output = Result<Self::Output, Self::Error>> + Unpin;
    type ListenFuture = impl Future<Output = Result<(Self::Listener, Multiaddr), Self::Error>>;

    // impl Future<Output = Result<(Self::Listener, Multiaddr), Self::Error>> + Unpin;

    fn listen(&self, addr: Multiaddr) -> Self::ListenFuture {
        let tcp_transport = self.tcp_transport.clone();
        Box::pin(async move {
            match Self::extract_proxied_address(&addr) {
                Ok(proxied_addr) => tcp_transport.listen(proxied_addr).await,
                Err(err) => Err(err),
            }
        })

        // TODO: The BIND command is not supported by the tor SOCKS proxy (as that wouldn't really make sense).
        //       This means that we cannot use this transport to listen for normal TCP/IP connections.
        //       To listen on TCP/IP addresses, we would use the TcpTransport (no anonymity).
        //       To "listen" on onion addresses, the following steps must be taken (probably outside of this transport)
        //       - An valid hidden service would need to be created. Either by the tor control port or pre-configured by
        //         the user.
        //       - Check if the multiaddr contains information about the proxied address (e.g.
        //         /onion/xxxxxxxxxxx:9090/ip4/127.0.0.1/tcp/1234)
        //       - Use the tor control port to query the given onion address for either a configured (i.e. GET_CONF)
        //         onion address, or an (possibly ephemeral) onion address previously created using `ADD_ONION` (i.e.
        //         `GET_INFO`)
        //       - If a matching address is found, bind a TcpListener on that local address. Any incoming connections to
        //         the onion address will be proxied to the local socket and can be accepted in the normal TCP way.
    }

    fn dial(&self, addr: Multiaddr) -> Self::DialFuture {
        Self::socks_connect(self.tcp_transport.clone(), self.socks_config.clone(), addr).boxed()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn extract_proxied_address() {
        let addr = "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234/ip4/127.0.0.1/tcp/9080"
            .parse()
            .unwrap();
        let proxy_addr = SocksTransport::extract_proxied_address(&addr).unwrap();
        let mut addr_iter = proxy_addr.iter();
        assert_eq!(addr_iter.next(), Some(AddrComponent::IP4("127.0.0.1".parse().unwrap())));
        assert_eq!(addr_iter.next(), Some(AddrComponent::TCP(9080)));
        assert_eq!(addr_iter.next(), None);
    }
}
