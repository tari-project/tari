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

use futures::{ready, FutureExt};
use multiaddr::Multiaddr;
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::Stream;

use super::{dns::DnsResolver, Transport};
use crate::{
    transports::dns::{DnsResolverRef, SystemDnsResolver},
    utils::multiaddr::socketaddr_to_multiaddr,
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
        Default::default()
    }

    /// Set the DnsResolver for this TcpTransport. The resolver will be used when converting DNS addresses to IP
    /// addresses.
    pub fn set_dns_resolver<T: DnsResolver>(&mut self, dns_resolver: T) -> &mut Self {
        self.dns_resolver = Arc::new(dns_resolver);
        self
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
        Self {
            ttl: None,
            nodelay: None,
            dns_resolver: Arc::new(SystemDnsResolver),
        }
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
            .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("Failed to resolve address: {}", err)))?;
        let listener = TcpListener::bind(&socket_addr).await?;
        let local_addr = socketaddr_to_multiaddr(&listener.local_addr()?);
        Ok((TcpInbound::new(self.clone(), listener), local_addr))
    }

    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
        let socket_addr = self
            .dns_resolver
            .resolve(addr.clone())
            .await
            .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("Address resolution failed: {}", err)))?;

        let socket = TcpOutbound::new(TcpStream::connect(socket_addr).boxed(), self.clone()).await?;
        Ok(socket)
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
    use super::*;

    #[test]
    fn configure() {
        let mut tcp = TcpTransport::new();
        tcp.set_nodelay(true).set_ttl(789);

        assert_eq!(tcp.nodelay, Some(true));
        assert_eq!(tcp.ttl, Some(789));
    }
}
