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

use std::{future::Future, io, pin::Pin, task::Poll};

use futures::{task::Context, Stream};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::mpsc,
};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{self, debug, error};
// Reexport
pub use yamux::ConnectionError;
use yamux::Mode;

use crate::{
    connection_manager::ConnectionDirection,
    stream_id,
    stream_id::StreamId,
    utils::atomic_ref_counter::{AtomicRefCounter, AtomicRefCounterGuard},
};

const LOG_TARGET: &str = "comms::multiplexing::yamux";

pub struct Yamux {
    control: Control,
    incoming: IncomingSubstreams,
    substream_counter: AtomicRefCounter,
}

const MAX_BUFFER_SIZE: u32 = 8 * 1024 * 1024; // 8MiB
const RECEIVE_WINDOW: u32 = 5 * 1024 * 1024; // 5MiB

impl Yamux {
    /// Upgrade the underlying socket to use yamux
    pub fn upgrade_connection<TSocket>(socket: TSocket, direction: ConnectionDirection) -> io::Result<Self>
    where TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static {
        let mode = match direction {
            ConnectionDirection::Inbound => Mode::Server,
            ConnectionDirection::Outbound => Mode::Client,
        };

        let mut config = yamux::Config::default();

        config.set_window_update_mode(yamux::WindowUpdateMode::OnRead);
        // Because OnRead mode increases the RTT of window update, bigger buffer size and receive
        // window size perform better.
        config.set_max_buffer_size(MAX_BUFFER_SIZE as usize);
        config.set_receive_window(RECEIVE_WINDOW);

        let substream_counter = AtomicRefCounter::new();
        let connection = yamux::Connection::new(socket.compat(), config, mode);
        let control = Control::new(connection.control(), substream_counter.clone());
        let incoming = Self::spawn_incoming_stream_worker(connection, substream_counter.clone());

        Ok(Self {
            control,
            incoming,
            substream_counter,
        })
    }

    // yamux@0.4 requires the incoming substream stream be polled in order to make progress on requests from it's
    // Control api. Here we spawn off a worker which will do this job
    fn spawn_incoming_stream_worker<TSocket>(
        connection: yamux::Connection<TSocket>,
        counter: AtomicRefCounter,
    ) -> IncomingSubstreams
    where
        TSocket: futures::AsyncRead + futures::AsyncWrite + Unpin + Send + 'static,
    {
        let (incoming_tx, incoming_rx) = mpsc::channel(10);
        let incoming = IncomingWorker::new(connection, incoming_tx);
        tokio::spawn(incoming.run());
        IncomingSubstreams::new(incoming_rx, counter)
    }

    /// Get the yamux control struct
    pub fn get_yamux_control(&self) -> Control {
        self.control.clone()
    }

    /// Returns a mutable reference to a `Stream` that emits substreams initiated by the remote
    pub fn incoming_mut(&mut self) -> &mut IncomingSubstreams {
        &mut self.incoming
    }

    /// Consumes this object and returns a `Stream` that emits substreams initiated by the remote
    pub fn into_incoming(self) -> IncomingSubstreams {
        self.incoming
    }

    /// Return the number of active substreams
    pub fn substream_count(&self) -> usize {
        self.substream_counter.get()
    }

    /// Return a SubstreamCounter for this connection
    pub(crate) fn substream_counter(&self) -> AtomicRefCounter {
        self.substream_counter.clone()
    }
}

#[derive(Clone)]
pub struct Control {
    inner: yamux::Control,
    substream_counter: AtomicRefCounter,
}

impl Control {
    pub fn new(inner: yamux::Control, substream_counter: AtomicRefCounter) -> Self {
        Self {
            inner,
            substream_counter,
        }
    }

    /// Open a new stream to the remote.
    pub async fn open_stream(&mut self) -> Result<Substream, ConnectionError> {
        // Ensure that this counts as used while the substream is being opened
        let counter_guard = self.substream_counter.new_guard();
        let stream = self.inner.open_stream().await?;
        Ok(Substream {
            stream: stream.compat(),
            _counter_guard: counter_guard,
        })
    }

    /// Close the connection.
    pub fn close(&mut self) -> impl Future<Output = Result<(), ConnectionError>> + '_ {
        self.inner.close()
    }

    pub fn substream_count(&self) -> usize {
        self.substream_counter.get()
    }

    pub(crate) fn substream_counter(&self) -> AtomicRefCounter {
        self.substream_counter.clone()
    }
}

pub struct IncomingSubstreams {
    inner: mpsc::Receiver<yamux::Stream>,
    substream_counter: AtomicRefCounter,
}

impl IncomingSubstreams {
    pub(self) fn new(inner: mpsc::Receiver<yamux::Stream>, substream_counter: AtomicRefCounter) -> Self {
        Self {
            inner,
            substream_counter,
        }
    }

    pub fn substream_count(&self) -> usize {
        self.substream_counter.get()
    }
}

impl Stream for IncomingSubstreams {
    type Item = Substream;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match futures::ready!(Pin::new(&mut self.inner).poll_recv(cx)) {
            Some(stream) => Poll::Ready(Some(Substream {
                stream: stream.compat(),
                _counter_guard: self.substream_counter.new_guard(),
            })),
            None => Poll::Ready(None),
        }
    }
}

/// A yamux stream wrapper that can be read from and written to.
#[derive(Debug)]
pub struct Substream {
    stream: Compat<yamux::Stream>,
    _counter_guard: AtomicRefCounterGuard,
}

impl StreamId for Substream {
    fn stream_id(&self) -> stream_id::Id {
        self.stream.get_ref().id().into()
    }
}

impl tokio::io::AsyncRead for Substream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.stream).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                #[cfg(feature = "metrics")]
                super::metrics::TOTAL_BYTES_READ.inc_by(buf.filled().len() as u64);
                Poll::Ready(Ok(()))
            },
            res => res,
        }
    }
}

impl tokio::io::AsyncWrite for Substream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        #[cfg(feature = "metrics")]
        super::metrics::TOTAL_BYTES_WRITTEN.inc_by(buf.len() as u64);
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

impl From<yamux::StreamId> for stream_id::Id {
    fn from(id: yamux::StreamId) -> Self {
        stream_id::Id::new(id.val())
    }
}

struct IncomingWorker<TSocket> {
    connection: yamux::Connection<TSocket>,
    sender: mpsc::Sender<yamux::Stream>,
}

impl<TSocket> IncomingWorker<TSocket>
where TSocket: futures::AsyncRead + futures::AsyncWrite + Unpin + Send + 'static /*  */
{
    pub fn new(connection: yamux::Connection<TSocket>, sender: mpsc::Sender<yamux::Stream>) -> Self {
        Self { connection, sender }
    }

    #[tracing::instrument(name = "yamux::incoming_worker::run", skip(self), fields(connection = %self.connection))]
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                _ = self.sender.closed() => {
                    self.close().await;
                    break
                },

                result = self.connection.next_stream() => {
                     match result {
                        Ok(Some(stream)) => {
                            if self.sender.send(stream).await.is_err() {
                                debug!(
                                    target: LOG_TARGET,
                                    "{} Incoming peer substream task is stopping because the internal stream sender channel \
                                     was closed",
                                    self.connection
                                );
                                break;
                            }
                        },
                        Ok(None) =>{
                            debug!(
                                target: LOG_TARGET,
                                "{} Incoming peer substream ended.",
                                self.connection
                            );
                            break;
                        }
                        Err(err) => {
                            error!(
                                target: LOG_TARGET,
                                "{} Incoming peer substream task received an error because '{}'",
                                self.connection,
                                err
                            );
                            break;
                        },
                    }
                }
            }
        }
    }

    async fn close(&mut self) {
        let mut control = self.connection.control();
        // Sends the close message once polled, while continuing to poll the connection future
        let close_fut = control.close();
        tokio::pin!(close_fut);
        loop {
            tokio::select! {
                biased;

                result = &mut close_fut => {
                    match result {
                        Ok(_) => break,
                        Err(err) => {
                            error!(target: LOG_TARGET, "Failed to close yamux connection: {}", err);
                            break;
                        }
                    }
                },

                result = self.connection.next_stream() => {
                    match result {
                        Ok(Some(_)) => continue,
                        Ok(None) => break,
                        Err(err) => {
                            error!(target: LOG_TARGET, "Error while closing yamux connection: {}", err);
                            continue;
                        }
                    }
                }
            }
        }
        debug!(target: LOG_TARGET, "{} Yamux connection has closed", self.connection);
    }
}

#[cfg(test)]
mod test {
    use std::{io, sync::Arc, time::Duration};

    use tari_test_utils::collect_stream;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::Barrier,
    };
    use tokio_stream::StreamExt;

    use crate::{connection_manager::ConnectionDirection, memsocket::MemorySocket, multiplexing::yamux::Yamux};

    #[tokio::test]
    async fn open_substream() -> io::Result<()> {
        let (dialer, listener) = MemorySocket::new_pair();
        let msg = b"The Way of Kings";

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound)?;
        let mut dialer_control = dialer.get_yamux_control();

        tokio::spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

            substream.write_all(msg).await.unwrap();
            substream.flush().await.unwrap();
            substream.shutdown().await.unwrap();
        });

        let mut listener = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)?.into_incoming();
        let mut substream = listener
            .next()
            .await
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no substream"))?;

        let mut buf = Vec::new();
        tokio::select! {
            _ = substream.read_to_end(&mut buf) => {},
            _ = listener.next() => {},
        };
        assert_eq!(buf, msg);

        Ok(())
    }

    #[tokio::test]
    async fn substream_count() {
        const NUM_SUBSTREAMS: usize = 10;
        let (dialer, listener) = MemorySocket::new_pair();

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound).unwrap();
        let mut dialer_control = dialer.get_yamux_control();

        let substreams_out = tokio::spawn(async move {
            let mut substreams = Vec::with_capacity(NUM_SUBSTREAMS);
            for _ in 0..NUM_SUBSTREAMS {
                substreams.push(dialer_control.open_stream().await.unwrap());
            }
            substreams
        });

        let mut listener = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)
            .unwrap()
            .into_incoming();
        let substreams_in = collect_stream!(&mut listener, take = NUM_SUBSTREAMS, timeout = Duration::from_secs(10));

        assert_eq!(dialer.substream_count(), NUM_SUBSTREAMS);
        assert_eq!(listener.substream_count(), NUM_SUBSTREAMS);

        drop(substreams_in);
        drop(substreams_out);

        assert_eq!(dialer.substream_count(), 0);
        assert_eq!(listener.substream_count(), 0);
    }

    #[tokio::test]
    async fn close() -> io::Result<()> {
        let (dialer, listener) = MemorySocket::new_pair();
        let msg = b"Words of Radiance";

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound)?;
        let mut dialer_control = dialer.get_yamux_control();

        tokio::spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

            substream.write_all(msg).await.unwrap();
            substream.flush().await.unwrap();

            let mut buf = Vec::new();
            substream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"");
        });

        let mut incoming = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)?.into_incoming();
        let mut substream = incoming.next().await.unwrap();

        let mut buf = vec![0; msg.len()];
        substream.read_exact(&mut buf).await?;
        assert_eq!(buf, msg);

        // Close the substream and then try to write to it
        substream.shutdown().await?;

        let result = substream.write_all(b"ignored message").await;
        match result {
            Ok(()) => panic!("Write should have failed"),
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::WriteZero),
        }

        Ok(())
    }

    #[tokio::test]
    async fn rude_close_does_not_freeze() -> io::Result<()> {
        let (dialer, listener) = MemorySocket::new_pair();

        let barrier = Arc::new(Barrier::new(2));
        let b = barrier.clone();

        tokio::spawn(async move {
            // Drop immediately
            let incoming = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)
                .unwrap()
                .into_incoming();
            drop(incoming);
            b.wait().await;
        });

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut substream = dialer_control.open_stream().await.unwrap();
        barrier.wait().await;

        let mut buf = vec![];
        substream.read_to_end(&mut buf).await.unwrap();
        assert!(buf.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn send_big_message() -> io::Result<()> {
        #[allow(non_upper_case_globals)]
        static MiB: usize = 1 << 20;
        static MSG_LEN: usize = 16 * MiB;

        let (dialer, listener) = MemorySocket::new_pair();

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound)?;
        let mut dialer_control = dialer.get_yamux_control();

        tokio::spawn(async move {
            assert_eq!(dialer_control.substream_count(), 0);
            let mut substream = dialer_control.open_stream().await.unwrap();
            assert_eq!(dialer_control.substream_count(), 1);

            let msg = vec![0x55u8; MSG_LEN];
            substream.write_all(msg.as_slice()).await.unwrap();

            let mut buf = vec![0u8; MSG_LEN];
            substream.read_exact(&mut buf).await.unwrap();
            substream.shutdown().await.unwrap();

            assert_eq!(buf.len(), MSG_LEN);
            assert_eq!(buf, vec![0xAAu8; MSG_LEN]);
        });

        let mut incoming = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)?.into_incoming();
        assert_eq!(incoming.substream_count(), 0);
        let mut substream = incoming.next().await.unwrap();
        assert_eq!(incoming.substream_count(), 1);

        let mut buf = vec![0u8; MSG_LEN];
        substream.read_exact(&mut buf).await?;
        assert_eq!(buf, vec![0x55u8; MSG_LEN]);

        let msg = vec![0xAAu8; MSG_LEN];
        substream.write_all(msg.as_slice()).await?;
        substream.shutdown().await?;
        drop(substream);

        assert_eq!(incoming.substream_count(), 0);

        Ok(())
    }
}
