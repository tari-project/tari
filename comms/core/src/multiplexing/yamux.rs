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
    collections::VecDeque,
    future::{Future, poll_fn},
    io,
    marker::PhantomData,
    pin::Pin,
    task::Poll,
    time::{Duration, Instant},
};

use futures::{FutureExt, Stream, channel::oneshot, task::Context};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::mpsc,
};
use tokio_util::{
    compat::{Compat, FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt},
    sync::PollSender,
};
use tracing::{debug, error, warn};
// Reexport
use yamux::{ConnectionError, FrameDecodeError, Mode};

use crate::{
    connection_manager::{ConnectionDirection, PeerConnectionInfo},
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
    pub fn upgrade_connection<TSocket>(
        socket: TSocket,
        direction: ConnectionDirection,
        peer_connection_info: PeerConnectionInfo,
    ) -> io::Result<Self>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let mode = match direction {
            ConnectionDirection::Inbound => Mode::Server,
            ConnectionDirection::Outbound => Mode::Client,
        };

        let config = yamux::Config::default();

        let substream_counter = AtomicRefCounter::new();
        let connection = yamux::Connection::new(socket.compat(), config, mode);
        let (control, incoming) =
            Self::spawn_incoming_stream_worker(connection, substream_counter.clone(), peer_connection_info);

        Ok(Self {
            control,
            incoming,
            substream_counter,
        })
    }

    // yamux requires the connection to be polled (poll_next_inbound) in order to make progress on any of its
    // substreams or on requests from the Control api. Here we spawn off a worker which will do this job
    fn spawn_incoming_stream_worker<TSocket>(
        connection: yamux::Connection<TSocket>,
        counter: AtomicRefCounter,
        peer_connection_info: PeerConnectionInfo,
    ) -> (Control, IncomingSubstreams)
    where
        TSocket: futures::AsyncRead + futures::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        let (incoming_tx, incoming_rx) = mpsc::channel(10);
        let (request_tx, request_rx) = mpsc::channel(1);
        let incoming = YamuxWorker::new(incoming_tx.clone(), request_rx, counter.clone(), peer_connection_info);
        let control = Control::new(request_tx);
        tokio::spawn(incoming.run(connection, incoming_tx));
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

/// The maximum number of inbound substreams that may wait to be accepted. Further inbound substreams are reset.
const MAX_QUEUED_INBOUND_STREAMS: usize = 64;
/// The minimum time between warnings about reset inbound substreams
const INBOUND_DROP_WARN_INTERVAL: Duration = Duration::from_secs(10);

enum WorkerExit {
    /// The yamux connection closed or failed
    ConnectionClosed,
    /// The receiver of incoming substreams was dropped
    IncomingClosed,
    /// A request to close the connection was received
    CloseRequested(oneshot::Sender<yamux::Result<()>>),
}

struct YamuxWorker<TSocket> {
    incoming_substreams: PollSender<yamux::Stream>,
    request_rx: mpsc::Receiver<YamuxRequest>,
    is_request_rx_closed: bool,
    pending_inbound: VecDeque<yamux::Stream>,
    pending_outbound: VecDeque<oneshot::Sender<yamux::Result<Substream>>>,
    num_inbound_dropped: u64,
    last_inbound_drop_warning: Option<Instant>,
    counter: AtomicRefCounter,
    _phantom: PhantomData<TSocket>,
    peer_connection_info: PeerConnectionInfo,
    is_closed: bool,
}

impl<TSocket> YamuxWorker<TSocket>
where TSocket: futures::AsyncRead + futures::AsyncWrite + Unpin + Send + Sync + 'static
{
    pub fn new(
        incoming_substreams: mpsc::Sender<yamux::Stream>,
        request_rx: mpsc::Receiver<YamuxRequest>,
        counter: AtomicRefCounter,
        peer_connection_info: PeerConnectionInfo,
    ) -> Self {
        Self {
            incoming_substreams: PollSender::new(incoming_substreams),
            request_rx,
            is_request_rx_closed: false,
            pending_inbound: VecDeque::new(),
            pending_outbound: VecDeque::new(),
            num_inbound_dropped: 0,
            last_inbound_drop_warning: None,
            counter,
            _phantom: PhantomData,
            peer_connection_info,
            is_closed: false,
        }
    }

    /// Runs the worker until the connection closes. `incoming_tx` is a sender for the incoming substream channel,
    /// used to detect that the receiver has been dropped.
    async fn run(mut self, mut connection: yamux::Connection<TSocket>, incoming_tx: mpsc::Sender<yamux::Stream>) {
        let incoming_closed = incoming_tx.closed();
        tokio::pin!(incoming_closed);

        let exit = poll_fn(|cx| self.poll_worker(cx, &mut connection, incoming_closed.as_mut())).await;

        for reply in self.pending_outbound.drain(..) {
            // Ignore: the caller may have given up
            let _ignore = reply.send(Err(ConnectionError::Closed));
        }

        match exit {
            WorkerExit::ConnectionClosed => {
                debug!(
                    target: LOG_TARGET,
                    "{} Incoming peer ({}) substream task is stopping because the connection was closed",
                    self.counter.get(),
                    self.peer_connection_info,
                );
                // Hand over streams accepted before the connection closed, their buffered data is still readable.
                // This must not wait, so streams that do not fit in the channel are dropped.
                let _ignore = poll_fn(|cx| self.incoming_substreams.poll_send_done(cx)).now_or_never();
                for stream in self.pending_inbound.drain(..) {
                    if incoming_tx.try_send(stream).is_err() {
                        break;
                    }
                }
            },
            WorkerExit::IncomingClosed => {
                debug!(
                    target: LOG_TARGET,
                    "{} Incoming peer ({}) substream task is stopping because the internal stream sender channel was \
                     closed",
                    self.counter.get(),
                    self.peer_connection_info,
                );
                self.pending_inbound.clear();
                // Ignore: we already log the error variant in self.close
                let _ignore = self.close(&mut connection).await;
            },
            WorkerExit::CloseRequested(reply) => {
                self.pending_inbound.clear();
                if reply.send(self.close(&mut connection).await).is_err() {
                    warn!(target: LOG_TARGET, "Request to close substream was aborted before reply was sent");
                }
            },
        }
    }

    /// Drives the connection and serves the queues. Requests and the inbound queue are served before the connection
    /// is polled, because reading from the socket spends the task's tokio budget, after which channel polls return
    /// `Pending`. The connection is polled on every wake-up that does not exit the worker, because
    /// `poll_next_inbound` performs all reads and writes for every substream on the connection.
    fn poll_worker<F: Future<Output = ()>>(
        &mut self,
        cx: &mut Context<'_>,
        connection: &mut yamux::Connection<TSocket>,
        incoming_closed: Pin<&mut F>,
    ) -> Poll<WorkerExit> {
        // Move requests to the local queue straight away so that a slow open never blocks request intake
        while !self.is_request_rx_closed {
            match self.request_rx.poll_recv(cx) {
                Poll::Ready(Some(YamuxRequest::OpenStream { reply })) => self.pending_outbound.push_back(reply),
                Poll::Ready(Some(YamuxRequest::Close { reply })) => {
                    return Poll::Ready(WorkerExit::CloseRequested(reply));
                },
                Poll::Ready(None) => self.is_request_rx_closed = true,
                Poll::Pending => break,
            }
        }

        if incoming_closed.poll(cx).is_ready() {
            return Poll::Ready(WorkerExit::IncomingClosed);
        }

        if !self.flush_inbound(cx) {
            return Poll::Ready(WorkerExit::IncomingClosed);
        }

        let mut num_new_inbound = 0usize;
        loop {
            // Give the queue a chance to drain after a burst of new streams
            if num_new_inbound >= MAX_QUEUED_INBOUND_STREAMS {
                cx.waker().wake_by_ref();
                break;
            }
            match connection.poll_next_inbound(cx) {
                Poll::Ready(Some(Ok(stream))) => {
                    self.queue_inbound_stream(stream);
                    num_new_inbound = num_new_inbound.saturating_add(1);
                },
                Poll::Ready(Some(Err(err))) => {
                    self.log_connection_error(&err);
                    self.is_closed = true;
                    return Poll::Ready(WorkerExit::ConnectionClosed);
                },
                Poll::Ready(None) => {
                    debug!(
                        target: LOG_TARGET,
                        "{} Incoming peer ({}) substream ended.",
                        self.counter.get(),
                        self.peer_connection_info,
                    );
                    return Poll::Ready(WorkerExit::ConnectionClosed);
                },
                Poll::Pending => break,
            }
        }

        if !self.flush_inbound(cx) {
            return Poll::Ready(WorkerExit::IncomingClosed);
        }

        // poll_new_outbound registers its own waker, which is woken when the remote ACKs a stream
        while let Some(reply) = self.pending_outbound.front() {
            if reply.is_canceled() {
                self.pending_outbound.pop_front();
                continue;
            }
            let Poll::Ready(result) = connection.poll_new_outbound(cx) else {
                break;
            };
            // Poll the connection again: a new stream is only picked up by the connection on its next poll, and
            // an error means the connection is closing
            cx.waker().wake_by_ref();
            if let Some(reply) = self.pending_outbound.pop_front() {
                let result = result.map(|stream| Substream {
                    stream: stream.compat(),
                    _counter_guard: self.counter.new_guard(),
                });
                if reply.send(result).is_err() {
                    warn!(target: LOG_TARGET, "Request to open substream was aborted before reply was sent");
                }
            }
        }

        Poll::Pending
    }

    /// Hands queued inbound streams to the receiver without waiting on the channel. Returns false if the receiver
    /// has been dropped.
    fn flush_inbound(&mut self, cx: &mut Context<'_>) -> bool {
        loop {
            match self.incoming_substreams.poll_send_done(cx) {
                Poll::Ready(Ok(())) => {},
                Poll::Ready(Err(_)) => return false,
                Poll::Pending => return true,
            }
            let Some(stream) = self.pending_inbound.pop_front() else {
                return true;
            };
            if self.incoming_substreams.start_send(stream).is_err() {
                return false;
            }
        }
    }

    fn queue_inbound_stream(&mut self, stream: yamux::Stream) {
        if self.pending_inbound.len() < MAX_QUEUED_INBOUND_STREAMS {
            self.pending_inbound.push_back(stream);
            return;
        }

        // Dropping the stream makes yamux reset it
        drop(stream);
        self.num_inbound_dropped = self.num_inbound_dropped.saturating_add(1);
        #[cfg(feature = "metrics")]
        super::metrics::INBOUND_SUBSTREAMS_DROPPED.inc();

        let should_warn = self
            .last_inbound_drop_warning
            .is_none_or(|last| last.elapsed() >= INBOUND_DROP_WARN_INTERVAL);
        if should_warn {
            self.last_inbound_drop_warning = Some(Instant::now());
            warn!(
                target: LOG_TARGET,
                "Peer ({}) has {} inbound substreams waiting to be accepted. Reset new inbound substream ({} reset in \
                 total)",
                self.peer_connection_info,
                self.pending_inbound.len(),
                self.num_inbound_dropped,
            );
        }
    }

    fn log_connection_error(&self, err: &ConnectionError) {
        match err {
            ConnectionError::Io(io_err)
                if io_err.kind() == io::ErrorKind::ConnectionReset ||
                    io_err.kind() == io::ErrorKind::ConnectionAborted ||
                    io_err.kind() == io::ErrorKind::BrokenPipe =>
            {
                debug!(
                    target: LOG_TARGET,
                    "{} Incoming peer ({}) substream closed by the remote host '{}'",
                    self.counter.get(),
                    self.peer_connection_info,
                    err
                );
            },
            ConnectionError::Decode(FrameDecodeError::Io(io_err))
                if io_err.kind() == io::ErrorKind::ConnectionReset ||
                    io_err.kind() == io::ErrorKind::ConnectionAborted ||
                    io_err.kind() == io::ErrorKind::UnexpectedEof ||
                    io_err.kind() == io::ErrorKind::BrokenPipe =>
            {
                debug!(
                    target: LOG_TARGET,
                    "{} Incoming peer ({}) substream closed by the remote host '{}'",
                    self.counter.get(),
                    self.peer_connection_info,
                    err
                );
            },
            _ => {
                error!(
                    target: LOG_TARGET,
                    "{} Incoming peer ({}) substream task received an error because '{}'",
                    self.counter.get(),
                    self.peer_connection_info,
                    err
                );
            },
        }
    }

    async fn close(&mut self, connection: &mut yamux::Connection<TSocket>) -> yamux::Result<()> {
        if self.is_closed {
            return Ok(());
        }

        self.is_closed = true;
        if let Err(err) = poll_fn(|cx| connection.poll_close(cx)).await {
            match err {
                ConnectionError::Io(ref io_err)
                    if io_err.kind() == io::ErrorKind::ConnectionReset ||
                        io_err.kind() == io::ErrorKind::ConnectionAborted =>
                {
                    debug!(target: LOG_TARGET, "Substream closed by the remote host '{}'", err);
                },
                _ => {
                    error!(target: LOG_TARGET, "Error while closing yamux connection: {}", err);
                    return Err(err);
                },
            }
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

    use crate::{
        connection_manager::{ConnectionDirection, PeerConnectionInfo},
        memsocket::MemorySocket,
        multiplexing::yamux::Yamux,
    };

    #[tokio::test]
    async fn open_substream() -> io::Result<()> {
        let (dialer, listener) = MemorySocket::new_pair();
        let msg = b"The Way of Kings";

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default())?;
        let mut dialer_control = dialer.get_yamux_control();

        tokio::spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

            substream.write_all(msg).await.unwrap();
            substream.shutdown().await.unwrap();
        });

        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default())?;
        let mut substream = listener
            .incoming
            .next()
            .await
            .ok_or_else(|| io::Error::other("no substream"))?;

        let mut buf = Vec::new();
        substream.read_to_end(&mut buf).await?;
        assert_eq!(buf, msg);

        Ok(())
    }

    #[tokio::test]
    async fn substream_count() {
        const NUM_SUBSTREAMS: usize = 10;
        let (dialer, listener) = MemorySocket::new_pair();

        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
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

        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();

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

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default())?;
        let mut dialer_control = dialer.get_yamux_control();

        tokio::spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

            substream.write_all(msg).await.unwrap();
            substream.flush().await.unwrap();

            let mut buf = Vec::new();
            substream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"");
        });

        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default())?;
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
            let incoming =
                Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default())
                    .unwrap()
                    .into_incoming();
            drop(incoming);
            b.wait().await;
        });

        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut substream = dialer_control.open_stream().await.unwrap();
        barrier.wait().await;

        let mut buf = vec![];
        substream.read_to_end(&mut buf).await.unwrap();
        assert!(buf.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn existing_substream_progresses_when_incoming_not_drained() {
        const NUM_UNACCEPTED: usize = 20;
        let (dialer, listener) = MemorySocket::new_pair();

        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();

        let mut dialer_substream = dialer_control.open_stream().await.unwrap();
        dialer_substream.write_all(b"first").await.unwrap();
        let mut listener_substream = listener.incoming.next().await.unwrap();
        let mut buf = [0u8; 5];
        listener_substream.read_exact(&mut buf).await.unwrap();

        // Open more substreams than the incoming channel holds, and never accept them
        let mut unaccepted = Vec::with_capacity(NUM_UNACCEPTED);
        for _ in 0..NUM_UNACCEPTED {
            let mut stream = dialer_control.open_stream().await.unwrap();
            stream.write_all(b"hello").await.unwrap();
            unaccepted.push(stream);
        }

        // The accepted substream must still make progress in both directions
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            dialer_substream.write_all(b"ping").await.unwrap();
            let mut buf = [0u8; 4];
            listener_substream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            listener_substream.write_all(b"pong").await.unwrap();
            dialer_substream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"pong");
        })
        .await;
        assert!(result.is_ok(), "existing substream stalled");
    }

    #[tokio::test]
    async fn open_stream_completes_after_ack_backlog_clears() {
        // yamux::MAX_ACK_BACKLOG, the number of un-ACKed outbound streams at which poll_new_outbound waits
        const MAX_ACK_BACKLOG: usize = 256;
        let (dialer, listener) = MemorySocket::new_pair();

        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();

        // Open the full backlog. The listener accepts each stream straight away, so its inbound queue stays well under
        // the cap, but never writes on them, so none are ACKed.
        let mut outbound = Vec::with_capacity(MAX_ACK_BACKLOG);
        let mut inbound = Vec::with_capacity(MAX_ACK_BACKLOG);
        for _ in 0..MAX_ACK_BACKLOG {
            let mut stream = dialer_control.open_stream().await.unwrap();
            stream.write_all(b"hello").await.unwrap();
            outbound.push(stream);
            let stream = tokio::time::timeout(Duration::from_secs(10), listener.incoming.next())
                .await
                .unwrap()
                .unwrap();
            inbound.push(stream);
        }

        let mut control = dialer_control.clone();
        let mut open_task = tokio::spawn(async move { control.open_stream().await });
        let result = tokio::time::timeout(Duration::from_millis(100), &mut open_task).await;
        assert!(result.is_err(), "open_stream should wait while the ACK backlog is full");

        // The listener's first frame on a stream ACKs it
        inbound.first_mut().unwrap().write_all(b"ack").await.unwrap();

        let result = tokio::time::timeout(Duration::from_secs(10), open_task).await;
        assert!(result.expect("open_stream deadlocked").unwrap().is_ok());

        let result = tokio::time::timeout(Duration::from_secs(10), async {
            let mut buf = [0u8; 3];
            outbound.first_mut().unwrap().read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ack");
        })
        .await;
        assert!(result.is_ok(), "existing substream stalled");
    }

    #[tokio::test]
    async fn excess_inbound_streams_are_reset() {
        // At most this many inbound streams can be held: the worker's queue, the incoming channel and one in flight
        const MAX_HELD: usize = super::MAX_QUEUED_INBOUND_STREAMS + 10 + 1;
        const NUM_EXCESS: usize = 25;
        let (dialer, listener) = MemorySocket::new_pair();

        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();

        let mut dialer_substream = dialer_control.open_stream().await.unwrap();
        dialer_substream.write_all(b"first").await.unwrap();
        let mut listener_substream = listener.incoming.next().await.unwrap();
        let mut buf = [0u8; 5];
        listener_substream.read_exact(&mut buf).await.unwrap();

        // Open streams that are never accepted
        let mut unaccepted = Vec::with_capacity(MAX_HELD + NUM_EXCESS);
        for _ in 0..MAX_HELD + NUM_EXCESS {
            let mut stream = dialer_control.open_stream().await.unwrap();
            stream.write_all(b"hello").await.unwrap();
            unaccepted.push(stream);
        }

        // Streams are held in order, so at least the last NUM_EXCESS streams must have been reset
        for stream in unaccepted.iter_mut().skip(MAX_HELD) {
            let mut buf = [0u8; 1];
            let result = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf))
                .await
                .expect("excess stream was not reset");
            assert!(matches!(result, Ok(0) | Err(_)), "unexpected read result {result:?}");
        }

        let result = tokio::time::timeout(Duration::from_secs(10), async {
            dialer_substream.write_all(b"ping").await.unwrap();
            let mut buf = [0u8; 4];
            listener_substream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            listener_substream.write_all(b"pong").await.unwrap();
            dialer_substream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"pong");
        })
        .await;
        assert!(result.is_ok(), "existing substream stalled");
    }

    #[tokio::test]
    async fn requests_are_served_while_socket_is_backlogged() {
        let (socket, mut raw) = tokio::io::duplex(1 << 20);
        let yamux =
            Yamux::upgrade_connection(socket, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();
        let mut control = yamux.get_yamux_control();

        // A window update for an unknown stream is ignored by yamux, but costs a socket read. Keeping the socket
        // backlogged makes the worker spend its tokio budget on reads in every poll.
        let window_update = [0u8, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        let batch = window_update.repeat(4096);
        let flood = tokio::spawn(async move { while raw.write_all(&batch).await.is_ok() {} });
        tokio::task::yield_now().await;

        let result = tokio::time::timeout(Duration::from_secs(10), control.open_stream()).await;
        assert!(result.expect("open_stream was starved").is_ok());

        let result = tokio::time::timeout(Duration::from_secs(10), control.close()).await;
        assert!(result.expect("close was starved").is_ok());

        flood.abort();
    }

    #[tokio::test]
    async fn send_big_message() -> io::Result<()> {
        #[allow(non_upper_case_globals)]
        static MiB: usize = 1 << 20;
        static MSG_LEN: usize = 16 * MiB;

        let (dialer, listener) = MemorySocket::new_pair();

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default())?;
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

        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default())?;
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
