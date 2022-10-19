// Copyright 2020, The Tari Project
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

// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    convert::TryFrom,
    io,
    num::NonZeroU16,
    pin::Pin,
    task::{Context, Poll},
};

use futures::stream::Stream;
use multiaddr::{Multiaddr, Protocol};

use crate::{
    memsocket,
    memsocket::{MemoryListener, MemorySocket},
    transports::Transport,
};

/// Transport to build in-memory connections
#[derive(Debug, Default, Clone)]
pub struct MemoryTransport;

impl MemoryTransport {
    /// Acquire a free memory socket port. This port will not be used when using `/memory/0` or by subsequent calls to
    /// `acquire_next_memsocket_port`.
    pub fn acquire_next_memsocket_port() -> NonZeroU16 {
        memsocket::acquire_next_memsocket_port()
    }

    /// Release a memory socket port. This port could be used when using `/memory/0` or when calling to
    /// `acquire_next_memsocket_port`.
    pub fn release_next_memsocket_port(port: NonZeroU16) {
        memsocket::release_memsocket_port(port);
    }
}

#[crate::async_trait]
impl Transport for MemoryTransport {
    type Error = io::Error;
    type Listener = Listener;
    type Output = MemorySocket;

    async fn listen(&self, addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
        // parse_addr is not used in the async block because of a rust ICE (internal compiler error)
        let port = parse_addr(addr)?;
        let listener = MemoryListener::bind(port)?;
        let actual_port = listener.local_addr();
        let mut actual_addr = Multiaddr::empty();
        actual_addr.push(Protocol::Memory(u64::from(actual_port)));
        Ok((Listener { inner: listener }, actual_addr))
    }

    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
        // parse_addr is not used in the async block because of a rust ICE (internal compiler error)
        let port = parse_addr(addr)?;
        Ok(MemorySocket::connect(port)?)
    }
}

fn parse_addr(addr: &Multiaddr) -> io::Result<u16> {
    let mut iter = addr.iter();

    let port = if let Some(Protocol::Memory(port)) = iter.next() {
        port
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid Multiaddr '{:?}'", addr),
        ));
    };

    if iter.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid Multiaddr '{:?}'", addr),
        ));
    }

    Ok(u16::try_from(port).unwrap())
}

#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Listener {
    inner: MemoryListener,
}

impl Stream for Listener {
    type Item = io::Result<(MemorySocket, Multiaddr)>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Option<Self::Item>> {
        let mut incoming = self.inner.incoming();
        match Pin::new(&mut incoming).poll_next(context) {
            Poll::Ready(Some(Ok(socket))) => {
                // Dialer addresses for MemoryTransport don't make a ton of sense,
                // so use port 0 to ensure they aren't used as an address to dial.
                let dialer_addr = Protocol::Memory(0).into();
                Poll::Ready(Some(Ok((socket, dialer_addr))))
            },
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod test {
    use futures::{future::join, stream::StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::runtime;

    #[runtime::test]
    async fn simple_listen_and_dial() -> Result<(), ::std::io::Error> {
        let t = MemoryTransport::default();

        let (listener, addr) = t.listen(&"/memory/0".parse().unwrap()).await?;

        let listener = async move {
            let (item, _listener) = listener.into_future().await;
            let (mut socket, _addr) = item.unwrap().unwrap();

            let mut buf = Vec::new();
            socket.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"hello world");
        };

        let mut outbound = t.dial(&addr).await?;

        let dialer = async move {
            outbound.write_all(b"hello world").await.unwrap();
            outbound.flush().await.unwrap();
        };

        join(dialer, listener).await;
        Ok(())
    }

    #[runtime::test]
    async fn unsupported_multiaddrs() {
        let t = MemoryTransport::default();

        let err = t.listen(&"/ip4/127.0.0.1/tcp/0".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err.kind(), io::ErrorKind::InvalidInput));

        let err = t.dial(&"/ip4/127.0.0.1/tcp/22".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err.kind(), io::ErrorKind::InvalidInput));
    }

    #[test]
    fn acquire_release_memsocket_port() {
        let port1 = MemoryTransport::acquire_next_memsocket_port();
        let port2 = MemoryTransport::acquire_next_memsocket_port();
        assert_ne!(port1, port2);
        MemoryTransport::release_next_memsocket_port(port1);
        MemoryTransport::release_next_memsocket_port(port2);
    }
}
