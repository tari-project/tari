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

use crate::transports::Transport;
use futures::{ready, stream::BoxStream, Future, Poll, Stream, StreamExt};
use multiaddr::{AddrComponent, Multiaddr};
use std::{
    io,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    task::Context,
    time::Duration,
};
use tokio::net::{TcpListener, TcpStream};

/// Transport implementation for TCP
#[derive(Debug, Clone, Default)]
pub struct TcpTransport {
    recv_buffer_size: Option<usize>,
    send_buffer_size: Option<usize>,
    ttl: Option<u32>,
    keepalive: Option<Option<Duration>>,
    nodelay: Option<bool>,
}

impl TcpTransport {
    /// Sets `SO_RCVBUF` i.e the size of the receive buffer.
    setter_mut!(set_recv_buffer_size, recv_buffer_size, Option<usize>);

    /// Sets `SO_SNDBUF` i.e. the size of the send buffer.
    setter_mut!(set_send_buffer_size, send_buffer_size, Option<usize>);

    /// Sets `IP_TTL` i.e. the TTL of packets sent from this socket.
    setter_mut!(set_ttl, ttl, Option<u32>);

    /// Sets `SO_KEEPALIVE` i.e. the interval to send keepalive probes, or None to disable.
    setter_mut!(set_keepalive, keepalive, Option<Option<Duration>>);

    /// Sets `TCP_NODELAY` i.e enable/disable Nagle's algorithm.
    setter_mut!(set_nodelay, nodelay, Option<bool>);

    /// Create a new TcpTransport
    pub fn new() -> Self {
        Default::default()
    }

    /// Apply socket options to `TcpStream`.
    fn configure(&self, socket: &TcpStream) -> io::Result<()> {
        if let Some(keepalive) = self.keepalive {
            socket.set_keepalive(keepalive)?;
        }

        if let Some(ttl) = self.ttl {
            socket.set_ttl(ttl)?;
        }

        if let Some(nodelay) = self.nodelay {
            socket.set_nodelay(nodelay)?;
        }

        if let Some(recv_buffer_size) = self.recv_buffer_size {
            socket.set_recv_buffer_size(recv_buffer_size)?;
        }

        if let Some(send_buffer_size) = self.send_buffer_size {
            socket.set_send_buffer_size(send_buffer_size)?;
        }

        Ok(())
    }
}

impl Transport for TcpTransport {
    type Error = io::Error;
    type Inbound = TcpInbound<'static>;
    type Output = (TcpStream, Multiaddr);

    type DialFuture = impl Future<Output = io::Result<Self::Output>>;
    type ListenFuture = impl Future<Output = io::Result<(Self::Inbound, Multiaddr)>>;

    fn listen(&self, addr: Multiaddr) -> Self::ListenFuture {
        let config = self.clone();
        Box::pin(async move {
            let socket_addr = multiaddr_to_socketaddr(addr)?;
            let listener = TcpListener::bind(&socket_addr).await?;
            let local_addr = socketaddr_to_multiaddr(listener.local_addr()?);
            Ok((
                TcpInbound {
                    incoming: listener.incoming().boxed(),
                    config,
                },
                local_addr,
            ))
        })
    }

    fn dial(&self, addr: Multiaddr) -> Self::DialFuture {
        let config = self.clone();
        Box::pin(async move {
            let socket_addr = multiaddr_to_socketaddr(addr)?;
            let stream = TcpStream::connect(&socket_addr).await?;
            config.configure(&stream)?;
            let peer_addr = socketaddr_to_multiaddr(stream.peer_addr()?);
            Ok((stream, peer_addr))
        })
    }
}

/// Wrapper around an Inbound stream. This ensures that any connecting `TcpStream` is configured according to the
/// transport
pub struct TcpInbound<'a> {
    incoming: BoxStream<'a, io::Result<TcpStream>>,
    config: TcpTransport,
}

impl Stream for TcpInbound<'_> {
    type Item = io::Result<(TcpStream, Multiaddr)>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(self.incoming.poll_next_unpin(cx)) {
            Some(Ok(stream)) => {
                // Configure each socket
                self.config.configure(&stream)?;
                let peer_addr = socketaddr_to_multiaddr(stream.peer_addr()?);
                Poll::Ready(Some(Ok((stream, peer_addr))))
            },
            Some(Err(err)) => Poll::Ready(Some(Err(err))),
            None => Poll::Ready(None),
        }
    }
}

/// Convert a socket address to a multiaddress
fn socketaddr_to_multiaddr(socket_addr: SocketAddr) -> Multiaddr {
    let mut addr: Multiaddr = match socket_addr.ip() {
        IpAddr::V4(addr) => AddrComponent::IP4(addr).into(),
        IpAddr::V6(addr) => AddrComponent::IP6(addr).into(),
    };
    addr.append(AddrComponent::TCP(socket_addr.port()));
    addr
}

/// Convert a multiaddr to a socket address required for `TcpStream`
fn multiaddr_to_socketaddr(addr: Multiaddr) -> io::Result<SocketAddr> {
    let mut addr_iter = addr.iter();
    let network_proto = addr_iter.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("Invalid address '{}'", addr),
    ))?;
    let transport_proto = addr_iter.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("Invalid address '{}'", addr),
    ))?;

    if addr_iter.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid address '{}'", addr),
        ));
    }

    match (network_proto, transport_proto) {
        (AddrComponent::IP4(host), AddrComponent::TCP(port)) => Ok((host, port).into()),
        (AddrComponent::IP6(host), AddrComponent::TCP(port)) => Ok((host, port).into()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid address '{}'", addr),
        )),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn configure() {
        let mut tcp = TcpTransport::new();
        tcp.set_send_buffer_size(123)
            .set_recv_buffer_size(456)
            .set_nodelay(true)
            .set_ttl(789)
            .set_keepalive(Some(Duration::from_millis(100)));

        assert_eq!(tcp.send_buffer_size, Some(123));
        assert_eq!(tcp.recv_buffer_size, Some(456));
        assert_eq!(tcp.nodelay, Some(true));
        assert_eq!(tcp.ttl, Some(789));
        assert_eq!(tcp.keepalive, Some(Some(Duration::from_millis(100))));
    }

    #[test]
    fn multiaddr_to_socketaddr_ok() {
        fn expect_success(addr: &str, expected_ip: &str) {
            let addr = Multiaddr::from_str(addr).unwrap();
            let sock_addr = super::multiaddr_to_socketaddr(addr).unwrap();
            assert_eq!(sock_addr.ip().to_string(), expected_ip);
        }

        expect_success("/ip4/254.0.1.2/tcp/1234", "254.0.1.2");
        expect_success("/ip6/::1/tcp/1234", "::1");
    }

    #[test]
    fn multiaddr_to_socketaddr_err() {
        fn expect_fail(addr: &str) {
            let addr = Multiaddr::from_str(addr).unwrap();
            let err = super::multiaddr_to_socketaddr(addr).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        }

        expect_fail("/ip4/254.0.1.2/tcp/1234/quic");
        expect_fail("/ip4/254.0.1.2");
        expect_fail("/p2p/QmcgpsyWgH8Y8ajJz1Cu72KnS5uo2Aa2LpzU7kinSupNKC");
    }
}
