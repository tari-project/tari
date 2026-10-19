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

use std::{
    future::Future,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{FutureExt, ready};
use multiaddr::Multiaddr;
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::Stream;

use super::{Transport, dns::DnsResolver};
use crate::{
    net_address::DialAddressPolicy,
    transports::dns::{DnsResolverRef, SystemDnsResolver},
    types::TransportProtocol,
    utils::{multiaddr::socketaddr_to_multiaddr, network::supports_ipv6},
};

/// Transport implementation for TCP
#[derive(Clone)]
pub struct TcpTransport {
    // recv_buffer_size: Option<usize>,
    // send_buffer_size: Option<usize>,
    ttl: Option<u32>,
    // #[allow(clippy::option_option)]
    // keepalive: Option<Option<Duration>>,
    nodelay: Option<bool>,
    dns_resolver: DnsResolverRef,
    supported_protocols: Vec<TransportProtocol>,
}

impl TcpTransport {
    // #[doc("Sets `SO_RCVBUF` i.e the size of the receive buffer.")]
    // setter_mut!(set_recv_buffer_size, recv_buffer_size, Option<usize>);
    //
    // #[doc("Sets `SO_SNDBUF` i.e. the size of the send buffer.")]
    // setter_mut!(set_send_buffer_size, send_buffer_size, Option<usize>);

    // #[doc("Sets `IP_TTL` i.e. the TTL of packets sent from this socket.")]
    setter_mut!(set_ttl, ttl, Option<u32>);

    // #[doc("Sets `SO_KEEPALIVE` i.e. the interval to send keepalive probes, or None to disable.")]
    // setter_mut!(set_keepalive, keepalive, Option<Option<Duration>>);

    // #[doc("Sets `TCP_NODELAY` i.e disable Nagle's algorithm if set to true.")]
    setter_mut!(set_nodelay, nodelay, Option<bool>);

    /// Create a new TcpTransport
    pub fn new() -> Self {
        let mut supported_protocols = vec![TransportProtocol::Ipv4];
        // Gate IPv6 on runtime capability
        if supports_ipv6() {
            supported_protocols.push(TransportProtocol::Ipv6);
        }
        Self {
            ttl: None,
            // Nagle's algorithm holds back the tail of each message until the peer ACKs, and delayed ACKs on the
            // peer turn that into a 40-200ms stall per message. Every transport (TCP, SOCKS, Tor) is built on this
            // one, so this applies to all dialed and accepted sockets.
            nodelay: Some(true),
            dns_resolver: Arc::new(SystemDnsResolver),
            supported_protocols,
        }
    }

    /// Set the DnsResolver for this TcpTransport. The resolver will be used when converting DNS addresses to IP
    /// addresses.
    pub fn set_dns_resolver<T: DnsResolver>(&mut self, dns_resolver: T) -> &mut Self {
        self.dns_resolver = Arc::new(dns_resolver);
        self
    }

    async fn connect(&self, addr: &Multiaddr, policy: Option<&DialAddressPolicy>) -> io::Result<TcpStream> {
        let socket_addr = self
            .dns_resolver
            .resolve(addr.clone())
            .await
            .map_err(|err| io::Error::other(format!("Address resolution failed: {err}")))?;
        if let Some(policy) = policy {
            policy.check_resolved(addr, &socket_addr)?;
        }

        let socket = TcpOutbound::new(TcpStream::connect(socket_addr).boxed(), self.clone()).await?;
        Ok(socket)
    }

    /// Apply socket options to `TcpStream`.
    fn configure(&self, socket: &TcpStream) -> io::Result<()> {
        // https://github.com/rust-lang/rust/issues/69774
        // if let Some(keepalive) = self.keepalive {
        //     socket.set_keepalive(keepalive)?;
        // }

        if let Some(ttl) = self.ttl {
            socket.set_ttl(ttl)?;
        }

        if let Some(nodelay) = self.nodelay {
            socket.set_nodelay(nodelay)?;
        }

        Ok(())
    }
}

impl Default for TcpTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[crate::async_trait]
impl Transport for TcpTransport {
    type Error = io::Error;
    type Listener = TcpInbound;
    type Output = TcpStream;

    async fn listen(&self, addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
        let socket_addr = self
            .dns_resolver
            .resolve(addr.clone())
            .await
            .map_err(|err| io::Error::other(format!("Failed to resolve address: {err}")))?;
        let listener = TcpListener::bind(&socket_addr).await?;
        let local_addr = socketaddr_to_multiaddr(&listener.local_addr()?);
        Ok((TcpInbound::new(self.clone(), listener), local_addr))
    }

    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
        self.connect(addr, None).await
    }

    async fn dial_with_policy(
        &self,
        addr: &Multiaddr,
        policy: &DialAddressPolicy,
    ) -> Result<Self::Output, Self::Error> {
        self.connect(addr, Some(policy)).await
    }

    fn supported_protocols(&self) -> Vec<TransportProtocol> {
        self.supported_protocols.clone()
    }
}

pub struct TcpOutbound<F> {
    future: F,
    config: TcpTransport,
}

impl<F> TcpOutbound<F> {
    pub fn new(future: F, config: TcpTransport) -> Self {
        Self { future, config }
    }
}

impl<F> Future for TcpOutbound<F>
where F: Future<Output = io::Result<TcpStream>> + Unpin
{
    type Output = io::Result<TcpStream>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let stream = ready!(Pin::new(&mut self.future).poll(cx))?;
        self.config.configure(&stream)?;
        Poll::Ready(Ok(stream))
    }
}

/// Wrapper around an Inbound stream. This ensures that any connecting `TcpStream` is configured according to the
/// transport
pub struct TcpInbound {
    listener: TcpListener,
    config: TcpTransport,
}

impl TcpInbound {
    pub fn new(config: TcpTransport, listener: TcpListener) -> Self {
        Self { listener, config }
    }
}

impl Stream for TcpInbound {
    type Item = io::Result<(TcpStream, Multiaddr)>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (socket, addr) = ready!(self.listener.poll_accept(cx))?;
        // Configure each socket
        self.config.configure(&socket)?;
        let peer_addr = socketaddr_to_multiaddr(&addr);
        Poll::Ready(Some(Ok((socket, peer_addr))))
    }
}

#[cfg(test)]
mod test {
    use std::net::SocketAddr;

    use futures::future::BoxFuture;
    use multiaddr::Protocol;

    use super::*;
    use crate::{net_address::IP4_TCP_TEST_ADDR_RANGE, transports::dns::DnsResolverError};

    #[test]
    fn nodelay_is_on_by_default() {
        let tcp = TcpTransport::new();
        assert_eq!(tcp.nodelay, Some(true));
        let tcp = TcpTransport::default();
        assert_eq!(tcp.nodelay, Some(true));
    }

    #[tokio::test]
    async fn dialed_and_accepted_sockets_have_nodelay() {
        let tcp = TcpTransport::new();
        let (mut listener, addr) = tcp.listen(&"/ip4/127.0.0.1/tcp/0".parse().unwrap()).await.unwrap();
        let dialed = tcp.dial(&addr).await.unwrap();
        let (accepted, _) = futures::StreamExt::next(&mut listener).await.unwrap().unwrap();
        assert!(dialed.nodelay().unwrap());
        assert!(accepted.nodelay().unwrap());
    }

    struct StubResolver(SocketAddr);

    impl DnsResolver for StubResolver {
        fn resolve(&self, _addr: Multiaddr) -> BoxFuture<'static, Result<SocketAddr, DnsResolverError>> {
            Box::pin(futures::future::ready(Ok(self.0)))
        }
    }

    #[tokio::test]
    async fn dns_name_resolving_to_loopback_is_not_dialled() {
        let (mut listener, listen_addr) = TcpTransport::new()
            .listen(&"/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .await
            .unwrap();
        let Some(Protocol::Tcp(port)) = listen_addr.iter().last() else {
            panic!("listener address has no port");
        };
        let mut tcp = TcpTransport::new();
        tcp.set_dns_resolver(StubResolver(([127, 0, 0, 1], port).into()));
        let addr = format!("/dns4/rebind.example.com/tcp/{port}").parse().unwrap();

        let deny_internal = DialAddressPolicy {
            allow_internal: false,
            excluded: vec![],
        };
        let err = tcp.dial_with_policy(&addr, &deny_internal).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        let exclude_loopback = DialAddressPolicy {
            allow_internal: true,
            excluded: vec![IP4_TCP_TEST_ADDR_RANGE.parse().unwrap()],
        };
        let err = tcp.dial_with_policy(&addr, &exclude_loopback).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        let allow_internal = DialAddressPolicy {
            allow_internal: true,
            excluded: vec![],
        };
        tcp.dial_with_policy(&addr, &allow_internal).await.unwrap();
        futures::StreamExt::next(&mut listener).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn ipv4_mapped_answer_does_not_bypass_ipv4_exclusion() {
        let mut tcp = TcpTransport::new();
        tcp.set_dns_resolver(StubResolver("[::ffff:23.23.23.23]:18189".parse().unwrap()));
        let addr = "/dns4/rebind.example.com/tcp/18189".parse().unwrap();
        let policy = DialAddressPolicy {
            allow_internal: false,
            excluded: vec!["/ip4/23.23.*.*/tcp/*".parse().unwrap()],
        };
        let err = tcp.dial_with_policy(&addr, &policy).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }

    #[tokio::test]
    async fn literal_nat64_address_of_loopback_is_not_dialled() {
        let tcp = TcpTransport::new();
        let addr = "/ip6/64:ff9b::7f00:1/tcp/18189".parse().unwrap();
        let policy = DialAddressPolicy {
            allow_internal: false,
            excluded: vec![],
        };
        let err = tcp.dial_with_policy(&addr, &policy).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn configure() {
        let mut tcp = TcpTransport::new();
        tcp.set_nodelay(true).set_ttl(789);

        assert_eq!(tcp.nodelay, Some(true));
        assert_eq!(tcp.ttl, Some(789));
    }
}
