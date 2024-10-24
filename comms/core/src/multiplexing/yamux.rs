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

use std::{future::poll_fn, io, marker::PhantomData, pin::Pin, task::Poll};

use futures::{channel::oneshot, task::Context, Stream};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::mpsc,
};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{debug, error, warn};
// Reexport
use yamux::Mode;

use crate::{
    connection_manager::ConnectionDirection,
    multiplexing::YamuxControlError,
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

impl Yamux {
    /// Upgrade the underlying socket to use yamux
    pub fn upgrade_connection<TSocket>(socket: TSocket, direction: ConnectionDirection) -> io::Result<Self>
    where TSocket: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static {
        let mode = match direction {
            ConnectionDirection::Inbound => Mode::Server,
            ConnectionDirection::Outbound => Mode::Client,
        };

        let config = yamux::Config::default();

        let substream_counter = AtomicRefCounter::new();
        let connection = yamux::Connection::new(socket.compat(), config, mode);
        let (control, incoming) = Self::spawn_incoming_stream_worker(connection, substream_counter.clone());

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
    ) -> (Control, IncomingSubstreams)
    where
        TSocket: futures::AsyncRead + futures::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        let (incoming_tx, incoming_rx) = mpsc::channel(10);
        let (request_tx, request_rx) = mpsc::channel(1);
        let incoming = YamuxWorker::new(incoming_tx, request_rx, counter.clone());
        let control = Control::new(request_tx);
        tokio::spawn(incoming.run(connection));
        (control, IncomingSubstreams::new(incoming_rx, counter))
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

#[derive(Debug)]
pub enum YamuxRequest {
    OpenStream {
        reply: oneshot::Sender<yamux::Result<Substream>>,
    },
    Close {
        reply: oneshot::Sender<yamux::Result<()>>,
    },
}

#[derive(Clone)]
pub struct Control {
    request_tx: mpsc::Sender<YamuxRequest>,
}

impl Control {
    pub fn new(request_tx: mpsc::Sender<YamuxRequest>) -> Self {
        Self { request_tx }
    }

    /// Open a new stream to the remote.
    pub async fn open_stream(&mut self) -> Result<Substream, YamuxControlError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request_tx.send(YamuxRequest::OpenStream { reply }).await?;
        let stream = reply_rx.await??;
        Ok(stream)
    }

    /// Close the connection.
    pub async fn close(&mut self) -> Result<(), YamuxControlError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request_tx.send(YamuxRequest::Close { reply }).await?;
        Ok(reply_rx.await??)
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

struct YamuxWorker<TSocket> {
    incoming_substreams: mpsc::Sender<yamux::Stream>,
    request_rx: mpsc::Receiver<YamuxRequest>,
    counter: AtomicRefCounter,
    _phantom: PhantomData<TSocket>,
}

impl<TSocket> YamuxWorker<TSocket>
where TSocket: futures::AsyncRead + futures::AsyncWrite + Unpin + Send + Sync + 'static
{
    pub fn new(
        incoming_substreams: mpsc::Sender<yamux::Stream>,
        request_rx: mpsc::Receiver<YamuxRequest>,
        counter: AtomicRefCounter,
    ) -> Self {
        Self {
            incoming_substreams,
            request_rx,
            counter,
            _phantom: PhantomData,
        }
    }

    async fn run(mut self, mut connection: yamux::Connection<TSocket>) {
        loop {
            tokio::select! {
                biased;

                _ = self.incoming_substreams.closed() => {
                    debug!(
                        target: LOG_TARGET,
                        "{} Incoming peer substream task is stopping because the internal stream sender channel was \
                         closed",
                        self.counter.get()
                    );
                    // Ignore: we already log the error variant in Self::close
                    let _ignore = Self::close(&mut connection).await;
                    break
                },

                Some(request) = self.request_rx.recv() => {
                    if let Err(err) = self.handle_request(&mut connection, request).await {
                        error!(target: LOG_TARGET, "Error handling request: {err}");
                        break;
                    }
                },

                result = Self::next_inbound_stream(&mut connection) => {
                     match result {
                        Some(Ok(stream)) => {
                            if self.incoming_substreams.send(stream).await.is_err() {
                                debug!(
                                    target: LOG_TARGET,
                                    "{} Incoming peer substream task is stopping because the internal stream sender channel was closed",
                                    self.counter.get()
                                );
                                break;
                            }
                        },
                        None =>{
                            debug!(
                                target: LOG_TARGET,
                                "{} Incoming peer substream ended.",
                                self.counter.get()
                            );
                            break;
                        }
                        Some(Err(err)) => {
                            error!(
                                target: LOG_TARGET,
                                "{} Incoming peer substream task received an error because '{}'",
                                self.counter.get(),
                                err
                            );
                            break;
                        },
                    }
                }
            }
        }
    }

    async fn handle_request(
        &self,
        connection_mut: &mut yamux::Connection<TSocket>,
        request: YamuxRequest,
    ) -> io::Result<()> {
        match request {
            YamuxRequest::OpenStream { reply } => {
                let result = poll_fn(move |cx| connection_mut.poll_new_outbound(cx)).await;
                if reply
                    .send(result.map(|stream| Substream {
                        stream: stream.compat(),
                        _counter_guard: self.counter.new_guard(),
                    }))
                    .is_err()
                {
                    warn!(target: LOG_TARGET, "Request to open substream was aborted before reply was sent");
                }
            },
            YamuxRequest::Close { reply } => {
                if reply.send(Self::close(connection_mut).await).is_err() {
                    warn!(target: LOG_TARGET, "Request to close substream was aborted before reply was sent");
                }
            },
        }
        Ok(())
    }

    async fn next_inbound_stream(
        connection_mut: &mut yamux::Connection<TSocket>,
    ) -> Option<yamux::Result<yamux::Stream>> {
        poll_fn(|cx| connection_mut.poll_next_inbound(cx)).await
    }

    async fn close(connection: &mut yamux::Connection<TSocket>) -> yamux::Result<()> {
        if let Err(err) = poll_fn(|cx| connection.poll_close(cx)).await {
            error!(target: LOG_TARGET, "Error while closing yamux connection: {}", err);
            return Err(err);
        }
        debug!(target: LOG_TARGET, "Yamux connection has closed");
        Ok(())
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
            substream.shutdown().await.unwrap();
        });

        let mut listener = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)?;
        let mut substream = listener
            .incoming
            .next()
            .await
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no substream"))?;

        let mut buf = Vec::new();
        substream.read_to_end(&mut buf).await?;
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
                let mut stream = dialer_control.open_stream().await.unwrap();
                // Since Yamux 0.12.0 the client does not initiate a substream unless you actually write something
                stream.write_all(b"hello").await.unwrap();
                substreams.push(stream);
            }
            substreams
        });

        let mut listener = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound).unwrap();

        let substreams_in = collect_stream!(
            &mut listener.incoming,
            take = NUM_SUBSTREAMS,
            timeout = Duration::from_secs(10)
        );

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

        let mut listener = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)?;
        let mut substream = listener.incoming.next().await.unwrap();

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
        let substream_counter = dialer.substream_counter();
        let mut dialer_control = dialer.get_yamux_control();

        tokio::spawn(async move {
            assert_eq!(substream_counter.get(), 0);
            let mut substream = dialer_control.open_stream().await.unwrap();
            assert_eq!(substream_counter.get(), 1);

            let msg = vec![0x55u8; MSG_LEN];
            substream.write_all(msg.as_slice()).await.unwrap();

            let mut buf = vec![0u8; MSG_LEN];
            substream.read_exact(&mut buf).await.unwrap();
            substream.shutdown().await.unwrap();

            assert_eq!(buf.len(), MSG_LEN);
            assert_eq!(buf, vec![0xAAu8; MSG_LEN]);
        });

        let mut listener = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)?;
        assert_eq!(listener.substream_count(), 0);
        let mut substream = listener.incoming.next().await.unwrap();
        assert_eq!(listener.substream_count(), 1);

        let mut buf = vec![0u8; MSG_LEN];
        substream.read_exact(&mut buf).await?;
        assert_eq!(buf, vec![0x55u8; MSG_LEN]);

        let msg = vec![0xAAu8; MSG_LEN];
        substream.write_all(msg.as_slice()).await?;
        substream.shutdown().await?;
        drop(substream);

        assert_eq!(listener.substream_count(), 0);

        Ok(())
    }
}
