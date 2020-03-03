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
    multiaddr::Multiaddr,
    socks,
    socks::Socks5Client,
    transports::{tcp::TcpTransport, TcpSocket, Transport},
};
use futures::{Future, FutureExt};
use std::{io, time::Duration};

/// SO_KEEPALIVE setting for the SOCKS TCP connection
const SOCKS_SO_KEEPALIVE: Duration = Duration::from_millis(1500);

#[derive(Clone, Debug)]
struct SocksConfig {
    proxy_address: Multiaddr,
    authentication: socks::Authentication,
}

#[derive(Clone, Debug)]
pub struct SocksTransport {
    socks_config: SocksConfig,
    tcp_transport: TcpTransport,
}

impl SocksTransport {
    pub fn new(proxy_address: Multiaddr, authentication: socks::Authentication) -> Self {
        let mut tcp_transport = TcpTransport::new();
        tcp_transport.set_nodelay(true);
        tcp_transport.set_keepalive(Some(SOCKS_SO_KEEPALIVE));

        Self {
            socks_config: SocksConfig {
                proxy_address,
                authentication,
            },
            tcp_transport,
        }
    }

    async fn socks_connect(
        tcp: TcpTransport,
        socks_config: SocksConfig,
        dest_addr: Multiaddr,
    ) -> io::Result<TcpSocket>
    {
        // Create a new connection to the SOCKS proxy
        let socks_conn = tcp.dial(socks_config.proxy_address)?.await?;
        let mut client = Socks5Client::new(socks_conn);

        client
            .with_authentication(socks_config.authentication)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        client
            .connect(&dest_addr)
            .await
            .map(|(socket, _)| socket)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }
}

impl Transport for SocksTransport {
    type Error = <TcpTransport as Transport>::Error;
    type Inbound = <TcpTransport as Transport>::Inbound;
    type ListenFuture = <TcpTransport as Transport>::ListenFuture;
    type Listener = <TcpTransport as Transport>::Listener;
    type Output = <TcpTransport as Transport>::Output;

    type DialFuture = impl Future<Output = Result<Self::Output, Self::Error>> + Unpin;

    fn listen(&self, addr: Multiaddr) -> Result<Self::ListenFuture, Self::Error> {
        self.tcp_transport.listen(addr)
    }

    fn dial(&self, addr: Multiaddr) -> Result<Self::DialFuture, Self::Error> {
        Ok(Self::socks_connect(self.tcp_transport.clone(), self.socks_config.clone(), addr).boxed())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::socks::Authentication;

    #[test]
    fn new() {
        let proxy_address = "/ip4/127.0.0.1/tcp/1234".parse::<Multiaddr>().unwrap();
        let transport = SocksTransport::new(proxy_address.clone(), Default::default());

        assert_eq!(transport.socks_config.proxy_address, proxy_address);
        assert_eq!(transport.socks_config.authentication, Authentication::None);
    }
}
