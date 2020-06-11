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

use super::{dns::DnsResolver, Transport};
use crate::{
    transports::dns::{DnsResolverRef, SystemDnsResolver},
    utils::multiaddr::socketaddr_to_multiaddr,
};
use futures::{future, io::Error, ready, AsyncRead, AsyncWrite, Future, Stream, TryFutureExt};
use multiaddr::Multiaddr;
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead as TokioAsyncRead, AsyncWrite as TokioAsyncWrite},
    net::{TcpListener, TcpStream},
};

/// Transport implementation for TCP
#[derive(Clone)]
pub struct TcpTransport {
    recv_buffer_size: Option<usize>,
    send_buffer_size: Option<usize>,
    ttl: Option<u32>,
    #[allow(clippy::option_option)]
    keepalive: Option<Option<Duration>>,
    nodelay: Option<bool>,
    dns_resolver: DnsResolverRef,
}

impl TcpTransport {
    #[doc("Sets `SO_RCVBUF` i.e the size of the receive buffer.")]
    setter_mut!(set_recv_buffer_size, recv_buffer_size, Option<usize>);

    #[doc("Sets `SO_SNDBUF` i.e. the size of the send buffer.")]
    setter_mut!(set_send_buffer_size, send_buffer_size, Option<usize>);

    #[doc("Sets `IP_TTL` i.e. the TTL of packets sent from this socket.")]
    setter_mut!(set_ttl, ttl, Option<u32>);

    #[doc("Sets `SO_KEEPALIVE` i.e. the interval to send keepalive probes, or None to disable.")]
    setter_mut!(set_keepalive, keepalive, Option<Option<Duration>>);

    #[doc("Sets `TCP_NODELAY` i.e disable Nagle's algorithm if set to true.")]
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

impl Default for TcpTransport {
    fn default() -> Self {
        Self {
            recv_buffer_size: None,
            send_buffer_size: None,
            ttl: None,
            keepalive: None,
            nodelay: None,
            dns_resolver: Arc::new(SystemDnsResolver),
        }
    }
}

impl Transport for TcpTransport {
    type Error = io::Error;
    type Inbound = future::Ready<io::Result<Self::Output>>;
    type Listener = TcpInbound;
    type Output = TcpSocket;

    type DialFuture = impl Future<Output = io::Result<Self::Output>>;
    type ListenFuture = impl Future<Output = io::Result<(Self::Listener, Multiaddr)>>;

    fn listen(&self, addr: Multiaddr) -> Result<Self::ListenFuture, Self::Error> {
        let config = self.clone();
        let dns_resolver = self.dns_resolver.clone();

        Ok(Box::pin(async move {
            let socket_addr = dns_resolver
                .resolve(addr)
                .await
                .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("Failed to resolve address: {}", err)))?;
            let listener = TcpListener::bind(&socket_addr).await?;
            let local_addr = socketaddr_to_multiaddr(&listener.local_addr()?);
            Ok((TcpInbound::new(config, listener), local_addr))
        }))
    }

    fn dial(&self, addr: Multiaddr) -> Result<Self::DialFuture, Self::Error> {
        let config = self.clone();
        Ok(config
            .dns_resolver
            .resolve(addr)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("Address resolution failed: {}", err)))
            .and_then(|socket_addr| TcpOutbound::new(Box::pin(TcpStream::connect(socket_addr)), config)))
    }
}

pub struct TcpOutbound<F> {
    future: F,
    config: TcpTransport,
}

impl<F> TcpOutbound<F> {
    pub fn new(future: F, config: TcpTransport) -> Self {
        Self { config, future }
    }
}

impl<F> Future for TcpOutbound<F>
where F: Future<Output = io::Result<TcpStream>> + Unpin
{
    type Output = io::Result<TcpSocket>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let socket = ready!(Pin::new(&mut self.future).poll(cx))?;
        self.config.configure(&socket)?;
        Poll::Ready(Ok(TcpSocket::new(socket)))
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
    type Item = io::Result<(future::Ready<io::Result<TcpSocket>>, Multiaddr)>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (socket, addr) = ready!(self.listener.poll_accept(cx))?;
        // Configure each socket
        self.config.configure(&socket)?;
        let peer_addr = socketaddr_to_multiaddr(&addr);
        let fut = future::ready(Ok(TcpSocket::new(socket)));
        Poll::Ready(Some(Ok((fut, peer_addr))))
    }
}

/// TcpSocket is a wrapper struct for tokio `TcpStream` and implements
/// `futures-rs` AsyncRead/Write
pub struct TcpSocket {
    inner: TcpStream,
}

impl TcpSocket {
    pub fn new(stream: TcpStream) -> Self {
        Self { inner: stream }
    }
}

impl AsyncWrite for TcpSocket {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, Error>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl AsyncRead for TcpSocket {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<Result<usize, Error>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl From<TcpStream> for TcpSocket {
    fn from(stream: TcpStream) -> Self {
        Self { inner: stream }
    }
}

#[cfg(test)]
mod test {
    use super::*;

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
}
