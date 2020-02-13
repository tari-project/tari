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

use crate::{
    memsocket::{MemoryListener, MemorySocket},
    transports::Transport,
};
use futures::{future, stream::Stream, Future};
use multiaddr::{Multiaddr, Protocol};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

/// Transport to build in-memory connections
#[derive(Debug, Default, Clone)]
pub struct MemoryTransport;

impl Transport for MemoryTransport {
    type Error = io::Error;
    type Inbound = future::Ready<Result<Self::Output, Self::Error>>;
    type Listener = Listener;
    type Output = MemorySocket;

    type DialFuture = impl Future<Output = io::Result<Self::Output>>;
    type ListenFuture = impl Future<Output = io::Result<(Self::Listener, Multiaddr)>>;

    fn listen(&self, addr: Multiaddr) -> Result<Self::ListenFuture, Self::Error> {
        // parse_addr is not used in the async block because of a rust ICE (internal compiler error)
        let port = parse_addr(&addr)?;
        let listener = MemoryListener::bind(port)?;
        let actual_port = listener.local_addr();
        let mut actual_addr = Multiaddr::empty();
        actual_addr.push(Protocol::Memory(u64::from(actual_port)));
        Ok(future::ready(Ok((Listener { inner: listener }, actual_addr))))
    }

    fn dial(&self, addr: Multiaddr) -> Result<Self::DialFuture, Self::Error> {
        // parse_addr is not used in the async block because of a rust ICE (internal compiler error)
        let port = parse_addr(&addr)?;
        Ok(future::ready(Ok(MemorySocket::connect(port)?)))
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

    Ok(port as u16)
}

#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Listener {
    inner: MemoryListener,
}

impl Stream for Listener {
    type Item = io::Result<(future::Ready<io::Result<MemorySocket>>, Multiaddr)>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Option<Self::Item>> {
        let mut incoming = self.inner.incoming();
        match Pin::new(&mut incoming).poll_next(context) {
            Poll::Ready(Some(Ok(socket))) => {
                // Dialer addresses for MemoryTransport don't make a ton of sense,
                // so use port 0 to ensure they aren't used as an address to dial.
                let dialer_addr = Protocol::Memory(0).into();
                Poll::Ready(Some(Ok((future::ready(Ok(socket)), dialer_addr))))
            },
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{
        future::join,
        io::{AsyncReadExt, AsyncWriteExt},
        stream::StreamExt,
    };

    #[tokio_macros::test]
    async fn simple_listen_and_dial() -> Result<(), ::std::io::Error> {
        let t = MemoryTransport::default();

        let (listener, addr) = t.listen("/memory/0".parse().unwrap())?.await?;

        let listener = async move {
            let (item, _listener) = listener.into_future().await;
            let (inbound, _addr) = item.unwrap().unwrap();
            let mut socket = inbound.await.unwrap();

            let mut buf = Vec::new();
            socket.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"hello world");
        };

        let mut outbound = t.dial(addr)?.await?;

        let dialer = async move {
            outbound.write_all(b"hello world").await.unwrap();
            outbound.flush().await.unwrap();
        };

        join(dialer, listener).await;
        Ok(())
    }

    #[test]
    fn unsupported_multiaddrs() {
        let t = MemoryTransport::default();

        let result = t.listen("/ip4/127.0.0.1/tcp/0".parse().unwrap());
        assert!(result.is_err());

        let result = t.dial("/ip4/127.0.0.1/tcp/22".parse().unwrap());
        assert!(result.is_err());
    }
}
