// Copyright 2020, The Taiji Project
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
    cmp,
    collections::{hash_map::Entry, HashMap},
    num::NonZeroU16,
    pin::Pin,
    sync::Mutex,
};

use bytes::{Buf, Bytes};
use futures::{
    channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
    ready,
    stream::{FusedStream, Stream},
    task::{Context, Poll},
};
use log::*;
use tokio::{
    io,
    io::{AsyncRead, AsyncWrite, ErrorKind, ReadBuf},
};

lazy_static! {
    static ref SWITCHBOARD: Mutex<SwitchBoard> = Mutex::new(SwitchBoard(HashMap::default(), 1));
}

enum Slot<T> {
    InUse(T),
    Acquired,
}

impl<T> Slot<T> {
    pub fn in_use(&self) -> Option<&T> {
        match &self {
            Slot::InUse(t) => Some(t),
            _ => None,
        }
    }
}

struct SwitchBoard(HashMap<NonZeroU16, Slot<UnboundedSender<MemorySocket>>>, u16);

pub fn acquire_next_memsocket_port() -> NonZeroU16 {
    let mut switchboard = (*SWITCHBOARD).lock().unwrap();
    let port = loop {
        let port = NonZeroU16::new(switchboard.1).unwrap_or_else(|| unreachable!());

        // The switchboard is full and all ports are in use
        assert!(
            switchboard.0.len() != (std::u16::MAX - 1) as usize,
            "All memsocket addresses in use!"
        );

        // Instead of overflowing to 0, resume searching at port 1 since port 0 isn't a
        // valid port to bind to.
        if switchboard.1 == std::u16::MAX {
            switchboard.1 = 1;
        } else {
            switchboard.1 += 1;
        }

        if !switchboard.0.contains_key(&port) {
            break port;
        }
    };

    switchboard.0.insert(port, Slot::Acquired);
    port
}

pub fn release_memsocket_port(port: NonZeroU16) {
    let mut switchboard = (*SWITCHBOARD).lock().unwrap();
    if let Entry::Occupied(entry) = switchboard.0.entry(port) {
        match *entry.get() {
            Slot::Acquired => {
                entry.remove_entry();
            },
            Slot::InUse(_) => panic!("cannot release memsocket port while InUse"),
        }
    }
}

/// An in-memory socket server, listening for connections.
///
/// After creating a `MemoryListener` by [`bind`]ing it to a socket address, it listens
/// for incoming connections. These can be accepted by awaiting elements from the
/// async stream of incoming connections, [`incoming`][`MemoryListener::incoming`].
///
/// The socket will be closed when the value is dropped.
///
/// [`bind`]: #method.bind
/// [`MemoryListener::incoming`]: #method.incoming
///
/// # Examples
///
/// ```rust,no_run
/// use std::io::Result;
///
/// use futures::prelude::*;
/// use taiji_comms::memsocket::{MemoryListener, MemorySocket};
/// use tokio::io::*;
///
/// async fn write_stormlight(mut stream: MemorySocket) -> Result<()> {
///     let msg = b"The most important step a person can take is always the next one.";
///     stream.write_all(msg).await?;
///     stream.flush().await
/// }
///
/// async fn listen() -> Result<()> {
///     let mut listener = MemoryListener::bind(16)?;
///     let mut incoming = listener.incoming();
///
///     // accept connections and process them serially
///     while let Some(stream) = incoming.next().await {
///         write_stormlight(stream?).await?;
///     }
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct MemoryListener {
    incoming: UnboundedReceiver<MemorySocket>,
    port: NonZeroU16,
}

impl Drop for MemoryListener {
    fn drop(&mut self) {
        let mut switchboard = (*SWITCHBOARD).lock().unwrap();
        // Remove the Sending side of the channel in the switchboard when
        // MemoryListener is dropped
        switchboard.0.remove(&self.port);
    }
}

impl MemoryListener {
    /// Creates a new `MemoryListener` which will be bound to the specified
    /// port.
    ///
    /// The returned listener is ready for accepting connections.
    ///
    /// Binding with a port number of 0 will request that a port be assigned
    /// to this listener. The port allocated can be queried via the
    /// [`local_addr`] method.
    ///
    /// # Examples
    /// Create a MemoryListener bound to port 16:
    ///
    /// ```rust,no_run
    /// use taiji_comms::memsocket::MemoryListener;
    ///
    /// # fn main () -> ::std::io::Result<()> {
    /// let listener = MemoryListener::bind(16)?;
    /// # Ok(())}
    /// ```
    ///
    /// [`local_addr`]: #method.local_addr
    pub fn bind(port: u16) -> io::Result<Self> {
        let mut switchboard = (*SWITCHBOARD).lock().unwrap();

        // Get the port we should bind to.  If 0 was given, use a random port
        let port = if let Some(port) = NonZeroU16::new(port) {
            match switchboard.0.get(&port) {
                Some(Slot::InUse(_)) => return Err(ErrorKind::AddrInUse.into()),
                Some(Slot::Acquired) | None => port,
            }
        } else {
            loop {
                let port = NonZeroU16::new(switchboard.1).unwrap_or_else(|| unreachable!());

                // The switchboard is full and all ports are in use
                if switchboard.0.len() == (std::u16::MAX - 1) as usize {
                    return Err(ErrorKind::AddrInUse.into());
                }

                // Instead of overflowing to 0, resume searching at port 1 since port 0 isn't a
                // valid port to bind to.
                if switchboard.1 == std::u16::MAX {
                    switchboard.1 = 1;
                } else {
                    switchboard.1 += 1;
                }

                if !switchboard.0.contains_key(&port) {
                    break port;
                }
            }
        };

        let (sender, receiver) = mpsc::unbounded();
        switchboard.0.insert(port, Slot::InUse(sender));

        Ok(Self {
            incoming: receiver,
            port,
        })
    }

    /// Returns the local address that this listener is bound to.
    ///
    /// This can be useful, for example, when binding to port 0 to figure out
    /// which port was actually bound.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use taiji_comms::memsocket::MemoryListener;
    ///
    /// # fn main () -> ::std::io::Result<()> {
    /// let listener = MemoryListener::bind(16)?;
    ///
    /// assert_eq!(listener.local_addr(), 16);
    /// # Ok(())}
    /// ```
    pub fn local_addr(&self) -> u16 {
        self.port.get()
    }

    /// Consumes this listener, returning a stream of the sockets this listener
    /// accepts.
    ///
    /// This method returns an implementation of the `Stream` trait which
    /// resolves to the sockets the are accepted on this listener.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use futures::prelude::*;
    /// use taiji_comms::memsocket::MemoryListener;
    ///
    /// # async fn work () -> ::std::io::Result<()> {
    /// let mut listener = MemoryListener::bind(16)?;
    /// let mut incoming = listener.incoming();
    ///
    /// // accept connections and process them serially
    /// while let Some(stream) = incoming.next().await {
    ///     match stream {
    ///         Ok(stream) => {
    ///             println!("new connection!");
    ///         },
    ///         Err(e) => { /* connection failed */ },
    ///     }
    /// }
    /// # Ok(())}
    /// ```
    pub fn incoming(&mut self) -> Incoming<'_> {
        Incoming { inner: self }
    }

    fn poll_accept(&mut self, context: &mut Context) -> Poll<io::Result<MemorySocket>> {
        match Pin::new(&mut self.incoming).poll_next(context) {
            Poll::Ready(Some(socket)) => Poll::Ready(Ok(socket)),
            Poll::Ready(None) => {
                let err = io::Error::new(ErrorKind::Other, "MemoryListener unknown error");
                Poll::Ready(Err(err))
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Stream returned by the `MemoryListener::incoming` function representing the
/// stream of sockets received from a listener.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Incoming<'a> {
    inner: &'a mut MemoryListener,
}

impl<'a> Stream for Incoming<'a> {
    type Item = io::Result<MemorySocket>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Option<Self::Item>> {
        let socket = ready!(self.inner.poll_accept(context)?);
        Poll::Ready(Some(Ok(socket)))
    }
}

/// An in-memory stream between two local sockets.
///
/// A `MemorySocket` can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [accepting] a connection from a [listener].
/// It can be read or written to using the `AsyncRead`, `AsyncWrite`, and related
/// extension traits in `futures::io`.
///
/// # Examples
///
/// ```rust, no_run
/// use futures::prelude::*;
/// use taiji_comms::memsocket::MemorySocket;
/// use tokio::io::*;
///
/// # async fn run() -> ::std::io::Result<()> {
/// let (mut socket_a, mut socket_b) = MemorySocket::new_pair();
///
/// socket_a.write_all(b"stormlight").await?;
/// socket_a.flush().await?;
///
/// let mut buf = [0; 10];
/// socket_b.read_exact(&mut buf).await?;
/// assert_eq!(&buf, b"stormlight");
///
/// # Ok(())}
/// ```
///
/// [`connect`]: struct.MemorySocket.html#method.connect
/// [accepting]: struct.MemoryListener.html#method.accept
/// [listener]: struct.MemoryListener.html
#[derive(Debug)]
pub struct MemorySocket {
    incoming: UnboundedReceiver<Bytes>,
    outgoing: UnboundedSender<Bytes>,
    current_buffer: Option<Bytes>,
    seen_eof: bool,
}

impl MemorySocket {
    /// Construct both sides of an in-memory socket.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use taiji_comms::memsocket::MemorySocket;
    ///
    /// let (socket_a, socket_b) = MemorySocket::new_pair();
    /// ```
    pub fn new_pair() -> (Self, Self) {
        let (a_tx, a_rx) = mpsc::unbounded();
        let (b_tx, b_rx) = mpsc::unbounded();
        let a = Self {
            incoming: a_rx,
            outgoing: b_tx,
            current_buffer: None,
            seen_eof: false,
        };
        let b = Self {
            incoming: b_rx,
            outgoing: a_tx,
            current_buffer: None,
            seen_eof: false,
        };

        (a, b)
    }

    /// Create a new in-memory Socket connected to the specified port.
    ///
    /// This function will create a new MemorySocket socket and attempt to connect it to
    /// the `port` provided.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use taiji_comms::memsocket::MemorySocket;
    ///
    /// # fn main () -> ::std::io::Result<()> {
    /// let socket = MemorySocket::connect(16)?;
    /// # Ok(())}
    /// ```
    pub fn connect(port: u16) -> io::Result<MemorySocket> {
        let mut switchboard = (*SWITCHBOARD).lock().unwrap();

        // Find port to connect to
        let port = NonZeroU16::new(port).ok_or(ErrorKind::AddrNotAvailable)?;

        let sender = switchboard
            .0
            .get_mut(&port)
            .and_then(|slot| slot.in_use())
            .ok_or(ErrorKind::AddrNotAvailable)?;

        let (socket_a, socket_b) = Self::new_pair();
        // Send the socket to the listener
        if let Err(e) = sender.unbounded_send(socket_a) {
            if e.is_disconnected() {
                return Err(ErrorKind::AddrNotAvailable.into());
            }

            unreachable!();
        }

        Ok(socket_b)
    }
}

impl AsyncRead for MemorySocket {
    /// Attempt to read from the `AsyncRead` into `buf`.
    fn poll_read(mut self: Pin<&mut Self>, context: &mut Context, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if self.incoming.is_terminated() {
            if self.seen_eof {
                return Poll::Ready(Err(ErrorKind::UnexpectedEof.into()));
            } else {
                self.seen_eof = true;
                return Poll::Ready(Ok(()));
            }
        }

        let mut bytes_read = 0;

        loop {
            // If we're already filled up the buffer then we can return
            if buf.remaining() == 0 {
                return Poll::Ready(Ok(()));
            }

            match self.current_buffer {
                // We have data to copy to buf
                Some(ref mut current_buffer) if !current_buffer.is_empty() => {
                    let bytes_to_read = cmp::min(buf.remaining(), current_buffer.len());
                    if bytes_to_read > 0 {
                        buf.initialize_unfilled_to(bytes_to_read)
                            .copy_from_slice(&current_buffer.slice(..bytes_to_read));
                        buf.advance(bytes_to_read);

                        current_buffer.advance(bytes_to_read);
                        trace!("reading {} bytes", bytes_to_read);

                        bytes_read += bytes_to_read;
                    }
                },

                // Either we've exhausted our current buffer or don't have one
                _ => {
                    self.current_buffer = {
                        match Pin::new(&mut self.incoming).poll_next(context) {
                            Poll::Pending => {
                                // If we've read anything up to this point return the bytes read
                                if bytes_read > 0 {
                                    return Poll::Ready(Ok(()));
                                } else {
                                    return Poll::Pending;
                                }
                            },
                            Poll::Ready(Some(buf)) => Some(buf),
                            Poll::Ready(None) => return Poll::Ready(Ok(())),
                        }
                    };
                },
            }
        }
    }
}

impl AsyncWrite for MemorySocket {
    /// Attempt to write bytes from `buf` into the outgoing channel.
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        let len = buf.len();

        match self.outgoing.poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                trace!("writing {} bytes", len);
                if let Err(e) = self.outgoing.start_send(Bytes::copy_from_slice(buf)) {
                    if e.is_disconnected() {
                        return Poll::Ready(Err(io::Error::new(ErrorKind::BrokenPipe, e)));
                    }

                    // Unbounded channels should only ever have "Disconnected" errors
                    unreachable!();
                }
                Poll::Ready(Ok(len))
            },
            Poll::Ready(Err(e)) => {
                if e.is_disconnected() {
                    return Poll::Ready(Err(io::Error::new(ErrorKind::BrokenPipe, e)));
                }

                // Unbounded channels should only ever have "Disconnected" errors
                unreachable!();
            },
            Poll::Pending => Poll::Pending,
        }
    }

    /// Attempt to flush the channel. Cannot Fail.
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context) -> Poll<io::Result<()>> {
        trace!("flush");
        Poll::Ready(Ok(()))
    }

    /// Attempt to close the channel. Cannot Fail.
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context) -> Poll<io::Result<()>> {
        self.outgoing.close_channel();

        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod test {
    use futures::SinkExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_stream::StreamExt;

    use super::*;
    use crate::framing;

    #[test]
    fn listener_bind() -> io::Result<()> {
        let port = acquire_next_memsocket_port().into();
        let listener = MemoryListener::bind(port)?;
        assert_eq!(listener.local_addr(), port);

        Ok(())
    }

    #[tokio::test]
    async fn simple_connect() -> io::Result<()> {
        let port = acquire_next_memsocket_port().into();
        let mut listener = MemoryListener::bind(port)?;

        let mut dialer = MemorySocket::connect(port)?;
        let mut listener_socket = listener.incoming().next().await.unwrap()?;

        dialer.write_all(b"foo").await?;
        dialer.flush().await?;

        let mut buf = [0; 3];
        listener_socket.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"foo");

        Ok(())
    }

    #[tokio::test]
    async fn listen_on_port_zero() -> io::Result<()> {
        let mut listener = MemoryListener::bind(0)?;
        let listener_addr = listener.local_addr();

        let mut dialer = MemorySocket::connect(listener_addr)?;
        let mut listener_socket = listener.incoming().next().await.unwrap()?;

        dialer.write_all(b"foo").await?;
        dialer.flush().await?;

        let mut buf = [0; 3];
        listener_socket.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"foo");

        listener_socket.write_all(b"bar").await?;
        listener_socket.flush().await?;

        let mut buf = [0; 3];
        dialer.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"bar");

        Ok(())
    }

    #[tokio::test]
    async fn listener_correctly_frees_port_on_drop() {
        async fn connect_on_port(port: u16) {
            let mut listener = MemoryListener::bind(port).unwrap();
            let mut dialer = MemorySocket::connect(port).unwrap();
            let mut listener_socket = listener.incoming().next().await.unwrap().unwrap();

            dialer.write_all(b"foo").await.unwrap();
            dialer.flush().await.unwrap();

            let mut buf = [0; 3];
            let n = listener_socket.read_exact(&mut buf).await.unwrap();
            assert_eq!(n, 3);
            assert_eq!(&buf, b"foo");
        }

        let port = acquire_next_memsocket_port().into();
        connect_on_port(port).await;
        connect_on_port(port).await;
    }

    #[tokio::test]
    async fn simple_write_read() -> io::Result<()> {
        let (mut a, mut b) = MemorySocket::new_pair();

        a.write_all(b"hello world").await?;
        a.flush().await?;
        drop(a);

        let mut v = Vec::new();
        b.read_to_end(&mut v).await?;
        assert_eq!(v, b"hello world");

        Ok(())
    }

    #[tokio::test]
    async fn partial_read() -> io::Result<()> {
        let (mut a, mut b) = MemorySocket::new_pair();

        a.write_all(b"foobar").await?;
        a.flush().await?;

        let mut buf = [0; 3];
        b.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"foo");
        b.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"bar");

        Ok(())
    }

    #[tokio::test]
    async fn partial_read_write_both_sides() -> io::Result<()> {
        let (mut a, mut b) = MemorySocket::new_pair();

        a.write_all(b"foobar").await?;
        a.flush().await?;
        b.write_all(b"stormlight").await?;
        b.flush().await?;

        let mut buf_a = [0; 5];
        let mut buf_b = [0; 3];
        a.read_exact(&mut buf_a).await?;
        assert_eq!(&buf_a, b"storm");
        b.read_exact(&mut buf_b).await?;
        assert_eq!(&buf_b, b"foo");

        a.read_exact(&mut buf_a).await?;
        assert_eq!(&buf_a, b"light");
        b.read_exact(&mut buf_b).await?;
        assert_eq!(&buf_b, b"bar");

        Ok(())
    }

    #[tokio::test]
    async fn many_small_writes() -> io::Result<()> {
        let (mut a, mut b) = MemorySocket::new_pair();

        a.write_all(b"words").await?;
        a.write_all(b" ").await?;
        a.flush().await?;
        a.write_all(b"of").await?;
        a.write_all(b" ").await?;
        a.flush().await?;
        a.write_all(b"radiance").await?;
        a.flush().await?;
        drop(a);

        let mut buf = [0; 17];
        b.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"words of radiance");

        Ok(())
    }

    #[tokio::test]
    async fn large_writes() -> io::Result<()> {
        let (mut a, mut b) = MemorySocket::new_pair();

        let large_data = vec![123u8; 1024];
        a.write_all(&large_data).await?;
        a.flush().await?;
        drop(a);

        let mut buf = Vec::new();
        b.read_to_end(&mut buf).await?;
        assert_eq!(buf.len(), 1024);

        Ok(())
    }

    #[tokio::test]
    async fn read_zero_bytes() -> io::Result<()> {
        let (mut a, mut b) = MemorySocket::new_pair();

        a.write_all(b"way of kings").await?;
        a.flush().await?;

        let mut buf = [0; 12];
        b.read_exact(&mut buf[0..0]).await?;
        assert_eq!(buf, [0; 12]);
        b.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"way of kings");

        Ok(())
    }

    #[tokio::test]
    async fn read_bytes_with_large_buffer() -> io::Result<()> {
        let (mut a, mut b) = MemorySocket::new_pair();

        a.write_all(b"way of kings").await?;
        a.flush().await?;

        let mut buf = [0; 20];
        let bytes_read = b.read(&mut buf).await?;
        assert_eq!(bytes_read, 12);
        assert_eq!(&buf[0..12], b"way of kings");

        Ok(())
    }

    #[tokio::test]
    async fn read_and_write_canonical_framing() -> io::Result<()> {
        let (a, b) = MemorySocket::new_pair();
        let mut a = framing::canonical(a, 1024);
        let mut b = framing::canonical(b, 1024);

        a.send(Bytes::from_static(b"frame-1")).await?;
        b.send(Bytes::from_static(b"frame-2")).await?;
        let msg = b.next().await.unwrap()?;
        assert_eq!(&msg[..], b"frame-1");

        let msg = a.next().await.unwrap()?;
        assert_eq!(&msg[..], b"frame-2");

        Ok(())
    }
}
