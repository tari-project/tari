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

use super::Transport;
use crate::{
    multiaddr::Protocol,
    transports::{SocksConfig, SocksTransport, TcpSocket, TcpTransport},
};
use futures::{Future, FutureExt};
use multiaddr::Multiaddr;
use std::io;

/// Transport implementation for TCP with Tor support
#[derive(Debug, Clone, Default)]
pub struct TcpWithTorTransport {
    socks_transport: Option<SocksTransport>,
    tcp_transport: TcpTransport,
}

impl TcpWithTorTransport {
    /// Sets the SOCKS proxy to use for onion addresses
    pub fn set_tor_socks_proxy(&mut self, socks_config: SocksConfig) -> &mut Self {
        self.socks_transport = Some(SocksTransport::new(socks_config));
        self
    }

    /// Create a new TcpTransport
    pub fn new() -> Self {
        Default::default()
    }

    pub fn tcp_transport_mut(&mut self) -> &mut TcpTransport {
        &mut self.tcp_transport
    }

    fn is_onion_address(addr: &Multiaddr) -> io::Result<bool> {
        let protocol = addr
            .iter()
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid address '{}'", addr)))?;

        match protocol {
            Protocol::Onion(_, _) | Protocol::Onion3(_) => Ok(true),
            _ => Ok(false),
        }
    }
}

impl Transport for TcpWithTorTransport {
    type Error = io::Error;
    type Inbound = <TcpTransport as Transport>::Inbound;
    type ListenFuture = <TcpTransport as Transport>::ListenFuture;
    type Listener = <TcpTransport as Transport>::Listener;
    type Output = TcpSocket;

    type DialFuture = impl Future<Output = io::Result<Self::Output>>;

    fn listen(&self, addr: Multiaddr) -> Result<Self::ListenFuture, Self::Error> {
        self.tcp_transport.listen(addr)
    }

    fn dial(&self, addr: Multiaddr) -> Result<Self::DialFuture, Self::Error> {
        if Self::is_onion_address(&addr)? {
            match self.socks_transport {
                Some(ref transport) => {
                    let dial_fut = transport.dial(addr)?;
                    Ok(dial_fut.boxed())
                },
                None => Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Tor SOCKS proxy is not set for TCP transport. Cannot dial peer with onion addresses."),
                )),
            }
        } else {
            let dial_fut = self.tcp_transport.dial(addr)?;
            Ok(dial_fut.boxed())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn is_onion_address() {
        let expect_true = [
            "/onion/aaimaq4ygg2iegci:1234",
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234",
        ];

        let expect_false = ["/dns4/mikes-node-nook.com:80", "/ip4/1.2.3.4/tcp/1234"];

        expect_true.iter().for_each(|addr| {
            let addr = addr.parse().unwrap();
            assert!(TcpWithTorTransport::is_onion_address(&addr).unwrap());
        });

        expect_false.iter().for_each(|addr| {
            let addr = addr.parse().unwrap();
            assert_eq!(TcpWithTorTransport::is_onion_address(&addr).unwrap(), false);
        });
    }
}
