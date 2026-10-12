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
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
    time::{Duration, Instant},
};

use futures::{Stream, channel::oneshot, task::Context};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::mpsc,
    time::Sleep,
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
    noise,
    stream_id,
    stream_id::StreamId,
    utils::atomic_ref_counter::{AtomicRefCounter, AtomicRefCounterGuard},
};

const LOG_TARGET: &str = "comms::multiplexing::yamux";

/// Size of the yamux frame header (not exported by the yamux crate)
const YAMUX_HEADER_LENGTH: usize = 12;

/// The largest yamux data frame payload that, together with its header, fits exactly into one noise frame. Receivers
/// accept frame bodies of up to 1 MiB, so this is compatible with every node running yamux 0.13.
const YAMUX_SPLIT_SEND_SIZE: usize = noise::MAX_WRITE_BUFFER_LENGTH - YAMUX_HEADER_LENGTH; // 65507

/// The maximum number of open streams per connection, in both directions. yamux holds the unread data of every stream,
/// including streams that were dropped but not yet cleaned up. For streams whose window is still the 256 KiB starting
/// window, this caps the unread data a peer can pin on one connection at 128 x 256 KiB = 32 MiB. yamux grows the window
/// of a stream that is read quickly, so streams whose windows grew can pin more, up to the connection receive window,
/// which is left at the yamux default. A peer that opens more streams than this has its connection closed by yamux.
const YAMUX_MAX_NUM_STREAMS: usize = 128;

pub struct Yamux {
    control: Control,
    incoming: IncomingSubstreams,
    substream_counter: AtomicRefCounter,
    write_stall: WriteStall,
}

fn yamux_config() -> yamux::Config {
    // The connection receive window is left at the default: the 256 KiB starting window is fixed by the yamux spec,
    // and auto-tuning grows it up to 1 GiB per connection. The stream limit is lowered from 512, because yamux keeps
    // the unread data of every stream it knows about. The default split send size of 16 KiB sends many small frames
    // for large messages, so frames are sized to fill one noise frame each.
    let mut config = yamux::Config::default();
    config.set_max_num_streams(YAMUX_MAX_NUM_STREAMS);
    config.set_split_send_size(YAMUX_SPLIT_SEND_SIZE);
    config
}

impl Yamux {
    /// Upgrade the underlying socket to use yamux. Write stalls are measured on `socket` itself. A connection whose
    /// socket wraps a transport (e.g. a noise socket) should measure them on the transport instead, with
    /// [Yamux::upgrade_connection_with_write_stall].
    pub fn upgrade_connection<TSocket>(
        socket: TSocket,
        direction: ConnectionDirection,
        peer_connection_info: PeerConnectionInfo,
    ) -> io::Result<Self>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        Self::upgrade_connection_with_stream_limit(socket, direction, peer_connection_info, YAMUX_MAX_NUM_STREAMS)
    }

    /// Upgrade the underlying socket to use yamux, where `write_stall` is measured by a [WriteStallSocket] on the
    /// transport under `socket`, so that the stall clock restarts whenever bytes reach the transport
    pub fn upgrade_connection_with_write_stall<TSocket>(
        socket: TSocket,
        direction: ConnectionDirection,
        peer_connection_info: PeerConnectionInfo,
        write_stall: WriteStall,
    ) -> io::Result<Self>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        Self::upgrade(
            socket,
            direction,
            peer_connection_info,
            YAMUX_MAX_NUM_STREAMS,
            write_stall,
        )
    }

    fn upgrade_connection_with_stream_limit<TSocket>(
        socket: TSocket,
        direction: ConnectionDirection,
        peer_connection_info: PeerConnectionInfo,
        max_num_streams: usize,
    ) -> io::Result<Self>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let socket = WriteStallSocket::new(socket);
        let write_stall = socket.write_stall();
        Self::upgrade(socket, direction, peer_connection_info, max_num_streams, write_stall)
    }

    fn upgrade<TSocket>(
        socket: TSocket,
        direction: ConnectionDirection,
        peer_connection_info: PeerConnectionInfo,
        max_num_streams: usize,
        write_stall: WriteStall,
    ) -> io::Result<Self>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let mode = match direction {
            ConnectionDirection::Inbound => Mode::Server,
            ConnectionDirection::Outbound => Mode::Client,
        };
        let mut config = yamux_config();
        config.set_max_num_streams(max_num_streams);

        let substream_counter = AtomicRefCounter::new();
        let reads_paused = Arc::new(AtomicBool::new(false));
        let socket = ReadPauseSocket {
            inner: socket,
            reads_paused: reads_paused.clone(),
        };
        let connection = yamux::Connection::new(socket.compat(), config, mode);
        let (control, incoming) = Self::spawn_incoming_stream_worker(
            connection,
            substream_counter.clone(),
            peer_connection_info,
            write_stall.clone(),
            reads_paused,
            max_num_streams,
        );

        Ok(Self {
            control,
            incoming,
            substream_counter,
            write_stall,
        })
    }

    // yamux requires the connection to be polled (poll_next_inbound) in order to make progress on any of its
    // substreams or on requests from the Control api. Here we spawn off a worker which will do this job
    fn spawn_incoming_stream_worker<TSocket>(
        connection: yamux::Connection<TSocket>,
        counter: AtomicRefCounter,
        peer_connection_info: PeerConnectionInfo,
        write_stall: WriteStall,
        reads_paused: Arc<AtomicBool>,
        max_num_streams: usize,
    ) -> (Control, IncomingSubstreams)
    where
        TSocket: futures::AsyncRead + futures::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_CHANNEL_SIZE);
        let (request_tx, request_rx) = mpsc::channel(1);
        let incoming = YamuxWorker::new(
            incoming_tx.clone(),
            request_rx,
            counter.clone(),
            peer_connection_info,
            write_stall,
            reads_paused,
            max_num_streams,
        );
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

    /// Returns the write-stall state of this connection's socket
    pub fn write_stall(&self) -> WriteStall {
        self.write_stall.clone()
    }
}

/// A connection is reported as write-stalled once its transport has accepted no bytes for this long while writes were
/// waiting. Shorter waits are normal TCP backpressure.
const WRITE_STALL_REPORT_AFTER: Duration = Duration::from_secs(30);
/// A connection whose transport has accepted no bytes for this long while writes were waiting is closed. A peer that
/// stops reading leaves yamux unable to send anything, and so unable to clean up dropped streams and free their
/// buffers. This is longer than a Tor round trip and shorter than the 240 s sync RPC deadline.
const WRITE_STALL_TIMEOUT: Duration = Duration::from_secs(120);

/// The time since which a connection's transport has been holding back writes without accepting any bytes, shared by
/// the connection's worker and every handle to the connection. It is measured by a [WriteStallSocket] on the transport,
/// under noise: the clock starts when a transport write or flush returns `Pending` and restarts whenever the transport
/// accepts any bytes. So a slow but working link, which keeps accepting bytes, is never stalled, however long a noise
/// frame takes to drain.
#[derive(Debug, Clone, Default)]
pub struct WriteStall {
    since: Arc<Mutex<Option<tokio::time::Instant>>>,
}

impl WriteStall {
    /// Returns how long the transport has been holding back writes without accepting any bytes, or `None` if it is not
    /// holding any back
    pub fn stalled_for(&self) -> Option<Duration> {
        self.since().map(|since| since.elapsed())
    }

    /// Returns true if writes to the transport have been waiting for at least `WRITE_STALL_REPORT_AFTER`
    pub fn is_stalled(&self) -> bool {
        self.stalled_for()
            .is_some_and(|stalled_for| stalled_for >= WRITE_STALL_REPORT_AFTER)
    }

    fn since(&self) -> Option<tokio::time::Instant> {
        match self.since.lock() {
            Ok(since) => *since,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    fn set_waiting(&self, is_waiting: bool) {
        let mut since = match self.since.lock() {
            Ok(since) => since,
            Err(poisoned) => poisoned.into_inner(),
        };
        if is_waiting {
            if since.is_none() {
                *since = Some(tokio::time::Instant::now());
            }
        } else {
            *since = None;
        }
    }

    /// Marks writes as waiting since `since`
    #[cfg(test)]
    pub(crate) fn set_stalled_since(&self, since: tokio::time::Instant) {
        let mut guard = match self.since.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard = Some(since);
    }
}

/// The socket under a yamux connection. It refuses reads while the worker has paused them, and passes writes through.
struct ReadPauseSocket<TSocket> {
    inner: TSocket,
    /// Set by the worker while it cannot queue more inbound streams. Reads then return `Pending` without registering a
    /// waker: the worker is woken by its incoming channel when there is room again, and clears the flag before it
    /// polls the connection.
    reads_paused: Arc<AtomicBool>,
}

impl<TSocket: AsyncRead + Unpin> AsyncRead for ReadPauseSocket<TSocket> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.reads_paused.load(Ordering::Relaxed) {
            return Poll::Pending;
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl<TSocket: AsyncWrite + Unpin> AsyncWrite for ReadPauseSocket<TSocket> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

/// A transport socket that measures how long its writes have been held back since it last accepted bytes (see
/// [WriteStall]). It goes under any framing (e.g. noise), so that the stall clock restarts whenever bytes reach the
/// transport, however large the frames above it are.
pub struct WriteStallSocket<TSocket> {
    inner: TSocket,
    write_stall: WriteStall,
    /// True if the last `poll_write` returned `Pending`
    is_write_blocked: bool,
    /// True if the last `poll_flush` returned `Pending`
    is_flush_blocked: bool,
}

impl<TSocket> WriteStallSocket<TSocket> {
    pub fn new(inner: TSocket) -> Self {
        Self {
            inner,
            write_stall: WriteStall::default(),
            is_write_blocked: false,
            is_flush_blocked: false,
        }
    }

    /// Returns the write-stall state this socket updates
    pub fn write_stall(&self) -> WriteStall {
        self.write_stall.clone()
    }

    fn update_write_stall(&self) {
        self.write_stall
            .set_waiting(self.is_write_blocked || self.is_flush_blocked);
    }
}

impl<TSocket: AsyncRead + Unpin> AsyncRead for WriteStallSocket<TSocket> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl<TSocket: AsyncWrite + Unpin> AsyncWrite for WriteStallSocket<TSocket> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_write(cx, buf);
        match result {
            Poll::Pending => this.is_write_blocked = true,
            Poll::Ready(Ok(n)) if n > 0 => {
                // The transport is accepting bytes, so restart the stall clock. A flush that is still pending starts a
                // fresh clock when it is next polled.
                this.is_write_blocked = false;
                this.is_flush_blocked = false;
            },
            Poll::Ready(_) => {},
        }
        this.update_write_stall();
        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_flush(cx);
        this.is_flush_blocked = result.is_pending();
        this.update_write_stall();
        result
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
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

/// The maximum number of inbound substreams that may wait to be accepted. While the queue is full the worker stops
/// reading from the socket. Inbound substreams that still arrive when it is full are reset.
const MAX_QUEUED_INBOUND_STREAMS: usize = 64;
/// The number of accepted inbound substreams the incoming channel holds
const INCOMING_CHANNEL_SIZE: usize = 10;
/// Local outbound opens are refused this many streams below the yamux stream limit. yamux closes the whole connection
/// when an open would exceed its limit, and it still counts streams that were dropped but not yet cleaned up, which
/// the worker cannot see. The headroom makes that teardown unlikely but cannot rule it out while writes are stalled,
/// since yamux only cleans up dropped streams when it has nothing to write. That case ends with the write-stall
/// watchdog closing the connection anyway.
const LOCAL_OPEN_HEADROOM: usize = 16;
/// The maximum number of outbound substream requests that may wait to open. Further requests fail straight away.
const MAX_PENDING_OUTBOUND_STREAMS: usize = 64;
/// The minimum time between warnings about reset inbound substreams
const INBOUND_DROP_WARN_INTERVAL: Duration = Duration::from_secs(10);
/// The maximum time to wait for the connection to close. The worker exits and drops the socket after this.
const CONNECTION_CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

enum WorkerExit {
    /// The yamux connection closed or failed
    ConnectionClosed,
    /// The receiver of incoming substreams was dropped
    IncomingClosed,
    /// A request to close the connection was received
    CloseRequested(oneshot::Sender<yamux::Result<()>>),
    /// The socket has not accepted a write for [WRITE_STALL_TIMEOUT]
    WriteStalled(Duration),
}

struct YamuxWorker<TSocket> {
    incoming_substreams: PollSender<yamux::Stream>,
    /// The sender behind `incoming_substreams`, used to count the streams in the channel
    incoming_tx: mpsc::Sender<yamux::Stream>,
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
    write_stall: WriteStall,
    reads_paused: Arc<AtomicBool>,
    /// The connection's yamux stream limit
    max_num_streams: usize,
    /// Wakes the worker at the next write-stall deadline
    write_stall_timer: Option<Pin<Box<Sleep>>>,
    /// The start of the current write stall, once it has lasted [WRITE_STALL_REPORT_AFTER]
    reported_write_stall_since: Option<tokio::time::Instant>,
}

impl<TSocket> YamuxWorker<TSocket>
where TSocket: futures::AsyncRead + futures::AsyncWrite + Unpin + Send + Sync + 'static
{
    pub fn new(
        incoming_substreams: mpsc::Sender<yamux::Stream>,
        request_rx: mpsc::Receiver<YamuxRequest>,
        counter: AtomicRefCounter,
        peer_connection_info: PeerConnectionInfo,
        write_stall: WriteStall,
        reads_paused: Arc<AtomicBool>,
        max_num_streams: usize,
    ) -> Self {
        Self {
            incoming_tx: incoming_substreams.clone(),
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
            write_stall,
            reads_paused,
            max_num_streams,
            write_stall_timer: None,
            reported_write_stall_since: None,
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
        self.end_write_stall_report();
        #[cfg(feature = "metrics")]
        super::metrics::inbound_dropped_per_connection().observe(self.num_inbound_dropped as f64);

        match exit {
            WorkerExit::ConnectionClosed => {
                debug!(
                    target: LOG_TARGET,
                    "{} Incoming peer ({}) substream task is stopping because the connection was closed",
                    self.counter.get(),
                    self.peer_connection_info,
                );
                // Hand over streams accepted before the connection closed, their buffered data is still readable.
                // The connection has no more I/O to drive, so waiting on the channel is fine, for a bounded time.
                let deliver = async {
                    if poll_fn(|cx| self.incoming_substreams.poll_send_done(cx)).await.is_err() {
                        return;
                    }
                    while let Some(stream) = self.pop_pending_inbound() {
                        if incoming_tx.send(stream).await.is_err() {
                            return;
                        }
                    }
                };
                if tokio::time::timeout(CONNECTION_CLOSE_TIMEOUT, deliver).await.is_err() {
                    debug!(
                        target: LOG_TARGET,
                        "Peer ({}) timed out handing over inbound substreams after the connection closed, dropping {}",
                        self.peer_connection_info,
                        self.pending_inbound.len(),
                    );
                }
                self.clear_pending_inbound();
            },
            WorkerExit::IncomingClosed => {
                debug!(
                    target: LOG_TARGET,
                    "{} Incoming peer ({}) substream task is stopping because the internal stream sender channel was \
                     closed",
                    self.counter.get(),
                    self.peer_connection_info,
                );
                self.clear_pending_inbound();
                // Ignore: we already log the error variant in self.close
                let _ignore = self.close(&mut connection).await;
            },
            WorkerExit::CloseRequested(reply) => {
                self.clear_pending_inbound();
                if reply.send(self.close(&mut connection).await).is_err() {
                    warn!(target: LOG_TARGET, "Request to close substream was aborted before reply was sent");
                }
            },
            WorkerExit::WriteStalled(stalled_for) => {
                // A graceful close needs the socket to accept writes, so the connection is dropped instead
                warn!(
                    target: LOG_TARGET,
                    "Peer ({}) has not accepted data for {:.0?}. Closing the connection.",
                    self.peer_connection_info,
                    stalled_for,
                );
                self.clear_pending_inbound();
                self.is_closed = true;
            },
        }
    }

    /// Drives the connection and serves the queues. Requests and the inbound queue are served before the connection
    /// is polled, because reading from the socket spends the task's tokio budget, after which channel polls return
    /// `Pending`. The connection is polled on every wake-up that does not exit the worker, because
    /// `poll_next_inbound` performs all reads and writes for every substream on the connection.
    ///
    /// While the inbound queue is full the socket refuses reads, so the connection only writes and yamux flow control
    /// holds the peer back until a queued stream is handed over.
    fn poll_worker<F: Future<Output = ()>>(
        &mut self,
        cx: &mut Context<'_>,
        connection: &mut yamux::Connection<TSocket>,
        incoming_closed: Pin<&mut F>,
    ) -> Poll<WorkerExit> {
        // Move requests to the local queue straight away so that a slow open never blocks request intake
        while !self.is_request_rx_closed {
            match self.request_rx.poll_recv(cx) {
                Poll::Ready(Some(YamuxRequest::OpenStream { reply })) => self.queue_outbound_request(reply),
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
        // When the queue is full, flush_inbound has registered for a wake-up once the channel has room
        self.set_reads_paused(self.is_inbound_queue_full());

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
                    self.set_reads_paused(self.is_inbound_queue_full());
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
        if self.reads_paused.load(Ordering::Relaxed) && !self.is_inbound_queue_full() {
            // The queue has room again: poll the connection once more so that it reads
            self.set_reads_paused(false);
            cx.waker().wake_by_ref();
        }

        // poll_new_outbound registers its own waker, which is woken when the remote ACKs a stream
        while let Some(reply) = self.pending_outbound.front() {
            if reply.is_canceled() {
                self.pending_outbound.pop_front();
                continue;
            }
            // yamux closes the whole connection when an open would exceed its stream limit, so refuse the open here
            if self.num_streams() >= self.max_num_streams.saturating_sub(LOCAL_OPEN_HEADROOM) {
                debug!(
                    target: LOG_TARGET,
                    "Peer ({}) connection has {} open substreams. Rejecting new outbound substream",
                    self.peer_connection_info,
                    self.num_streams(),
                );
                if let Some(reply) = self.pending_outbound.pop_front() {
                    // Ignore: the caller may have given up
                    let _ignore = reply.send(Err(ConnectionError::TooManyStreams));
                }
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

        if let Poll::Ready(stalled_for) = self.poll_write_stall(cx) {
            return Poll::Ready(WorkerExit::WriteStalled(stalled_for));
        }

        Poll::Pending
    }

    /// Checks how long writes to the socket have been waiting. Returns `Ready` once they have waited for
    /// [WRITE_STALL_TIMEOUT], otherwise arranges a wake-up for the next deadline.
    fn poll_write_stall(&mut self, cx: &mut Context<'_>) -> Poll<Duration> {
        let since = self.write_stall.since();
        // The transport may have accepted bytes and blocked again since the last poll, which is a new stall
        if self.reported_write_stall_since.is_some() && self.reported_write_stall_since != since {
            self.end_write_stall_report();
        }
        let Some(since) = since else {
            self.write_stall_timer = None;
            return Poll::Pending;
        };
        let stalled_for = since.elapsed();
        if stalled_for >= WRITE_STALL_TIMEOUT {
            return Poll::Ready(stalled_for);
        }
        if self.reported_write_stall_since.is_none() && stalled_for >= WRITE_STALL_REPORT_AFTER {
            self.reported_write_stall_since = Some(since);
            debug!(
                target: LOG_TARGET,
                "Peer ({}) has not accepted data for {:.0?}",
                self.peer_connection_info,
                stalled_for,
            );
            #[cfg(feature = "metrics")]
            super::metrics::write_stalled_connections().inc();
        }

        let next_check = if self.reported_write_stall_since.is_some() {
            WRITE_STALL_TIMEOUT
        } else {
            WRITE_STALL_REPORT_AFTER
        };
        let Some(deadline) = since.checked_add(next_check) else {
            return Poll::Pending;
        };
        match self.write_stall_timer.as_mut() {
            Some(timer) => timer.as_mut().reset(deadline),
            None => self.write_stall_timer = Some(Box::pin(tokio::time::sleep_until(deadline))),
        }
        if let Some(timer) = self.write_stall_timer.as_mut() &&
            timer.as_mut().poll(cx).is_ready()
        {
            // The deadline passed while this poll ran, so check again
            cx.waker().wake_by_ref();
        }
        Poll::Pending
    }

    /// Ends the report of a write stall that lasted [WRITE_STALL_REPORT_AFTER], if there is one
    fn end_write_stall_report(&mut self) {
        let Some(since) = self.reported_write_stall_since.take() else {
            return;
        };
        debug!(
            target: LOG_TARGET,
            "Peer ({}) write stall ended after {:.0?}",
            self.peer_connection_info,
            since.elapsed(),
        );
        #[cfg(feature = "metrics")]
        {
            super::metrics::write_stalled_connections().dec();
            super::metrics::write_stall_duration().observe(since.elapsed().as_secs_f64());
        }
    }

    /// Returns the number of streams on the connection that this worker knows of: streams handed out and not yet
    /// dropped, plus inbound streams waiting to be accepted, in the incoming channel or being handed to it. yamux may
    /// still hold dropped streams that it has not cleaned up, which happens while the socket is busy writing (see
    /// [LOCAL_OPEN_HEADROOM]).
    fn num_streams(&self) -> usize {
        let num_in_channel = self
            .incoming_tx
            .max_capacity()
            .saturating_sub(self.incoming_tx.capacity());
        // A stream passed to start_send is held by the PollSender until the channel has room for it
        let num_being_handed_over = usize::from(!self.incoming_substreams.is_ready());
        self.counter
            .get()
            .saturating_add(self.pending_inbound.len())
            .saturating_add(num_in_channel)
            .saturating_add(num_being_handed_over)
    }

    fn is_inbound_queue_full(&self) -> bool {
        self.pending_inbound.len() >= MAX_QUEUED_INBOUND_STREAMS
    }

    fn set_reads_paused(&self, is_paused: bool) {
        self.reads_paused.store(is_paused, Ordering::Relaxed);
    }

    fn pop_pending_inbound(&mut self) -> Option<yamux::Stream> {
        let stream = self.pending_inbound.pop_front()?;
        #[cfg(feature = "metrics")]
        super::metrics::QUEUED_INBOUND_SUBSTREAMS.dec();
        Some(stream)
    }

    fn clear_pending_inbound(&mut self) {
        #[cfg(feature = "metrics")]
        super::metrics::QUEUED_INBOUND_SUBSTREAMS.sub(i64::try_from(self.pending_inbound.len()).unwrap_or(i64::MAX));
        self.pending_inbound.clear();
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
            let Some(stream) = self.pop_pending_inbound() else {
                return true;
            };
            if self.incoming_substreams.start_send(stream).is_err() {
                return false;
            }
        }
    }

    fn queue_outbound_request(&mut self, reply: oneshot::Sender<yamux::Result<Substream>>) {
        if self.pending_outbound.len() >= MAX_PENDING_OUTBOUND_STREAMS {
            // Callers that gave up do not count
            self.pending_outbound.retain(|reply| !reply.is_canceled());
        }
        if self.pending_outbound.len() >= MAX_PENDING_OUTBOUND_STREAMS {
            debug!(
                target: LOG_TARGET,
                "Peer ({}) has {} outbound substream requests waiting to open. Rejecting new request",
                self.peer_connection_info,
                self.pending_outbound.len(),
            );
            // Ignore: the caller may have given up
            let _ignore = reply.send(Err(ConnectionError::TooManyStreams));
            return;
        }
        self.pending_outbound.push_back(reply);
    }

    fn queue_inbound_stream(&mut self, stream: yamux::Stream) {
        if self.pending_inbound.len() < MAX_QUEUED_INBOUND_STREAMS {
            self.pending_inbound.push_back(stream);
            #[cfg(feature = "metrics")]
            super::metrics::QUEUED_INBOUND_SUBSTREAMS.inc();
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
        // Closing only writes, so a peer that stops reading would otherwise keep the connection open forever
        let Ok(result) = tokio::time::timeout(CONNECTION_CLOSE_TIMEOUT, poll_fn(|cx| connection.poll_close(cx))).await
        else {
            warn!(
                target: LOG_TARGET,
                "Peer ({}) yamux connection did not close within {:.0?}",
                self.peer_connection_info,
                CONNECTION_CLOSE_TIMEOUT
            );
            return Err(ConnectionError::Io(io::ErrorKind::TimedOut.into()));
        };
        if let Err(err) = result {
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
    use std::{
        io,
        pin::Pin,
        sync::{
            Arc,
            Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        task::{Context, Poll},
        time::Duration,
    };

    use tari_test_utils::collect_stream;
    use tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf},
        sync::Barrier,
    };
    use tokio_stream::StreamExt;

    use crate::{
        connection_manager::{ConnectionDirection, PeerConnectionInfo},
        memsocket::MemorySocket,
        multiplexing::{
            YamuxControlError,
            yamux::{
                INCOMING_CHANNEL_SIZE,
                LOCAL_OPEN_HEADROOM,
                MAX_QUEUED_INBOUND_STREAMS,
                Substream,
                WRITE_STALL_REPORT_AFTER,
                WRITE_STALL_TIMEOUT,
                WriteStall,
                WriteStallSocket,
                YAMUX_MAX_NUM_STREAMS,
                Yamux,
                YamuxWorker,
                yamux_config,
            },
        },
        noise::NoiseConfig,
        peer_manager::PeerFeatures,
        test_utils::node_identity::build_node_identity,
        utils::atomic_ref_counter::AtomicRefCounter,
    };

    /// The yamux starting window: the most unread data yamux holds for one stream
    const STREAM_WINDOW: usize = 256 * 1024;

    /// A socket that counts the bytes read from it
    struct CountingSocket {
        inner: DuplexStream,
        bytes_read: Arc<AtomicUsize>,
    }

    impl AsyncRead for CountingSocket {
        fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            let before = buf.filled().len();
            let result = Pin::new(&mut this.inner).poll_read(cx, buf);
            this.bytes_read
                .fetch_add(buf.filled().len().saturating_sub(before), Ordering::SeqCst);
            result
        }
    }

    impl AsyncWrite for CountingSocket {
        fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
        }
    }

    /// A socket that is never read from, like the socket of a peer that stops reading
    struct NeverReadSocket {
        inner: DuplexStream,
    }

    impl AsyncRead for NeverReadSocket {
        fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, _buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }

    impl AsyncWrite for NeverReadSocket {
        fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
        }
    }

    /// The yamux crate's default stream limit, for a peer that opens more streams than we allow
    const DEFAULT_YAMUX_MAX_NUM_STREAMS: usize = 512;

    #[test]
    fn yamux_frames_fill_one_noise_frame() {
        let config = format!("{:?}", yamux_config());
        assert!(config.contains("split_send_size: 65507"), "{config}");
        // Window defaults are unchanged
        assert!(
            config.contains("max_connection_receive_window: Some(1073741824)"),
            "{config}"
        );
        assert!(config.contains("max_num_streams: 128"), "{config}");
    }

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
    async fn outbound_streams_beyond_the_local_cap_are_refused() {
        const LOCAL_CAP: usize = YAMUX_MAX_NUM_STREAMS - LOCAL_OPEN_HEADROOM;
        let (dialer, listener) = MemorySocket::new_pair();

        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();

        let mut outbound = Vec::with_capacity(LOCAL_CAP);
        let mut inbound = Vec::with_capacity(LOCAL_CAP);
        for _ in 0..LOCAL_CAP {
            let mut stream = dialer_control.open_stream().await.unwrap();
            stream.write_all(b"hello").await.unwrap();
            outbound.push(stream);
            let stream = tokio::time::timeout(Duration::from_secs(10), listener.incoming.next())
                .await
                .unwrap()
                .unwrap();
            inbound.push(stream);
        }

        let err = dialer_control.open_stream().await.unwrap_err();
        assert!(matches!(err, YamuxControlError::TooManyStreams), "{err:?}");
        // The refusal leaves the connection up
        let mut buf = [0u8; 5];
        inbound.first_mut().unwrap().read_exact(&mut buf).await.unwrap();
        assert_stream_carries_data(outbound.first_mut().unwrap(), inbound.first_mut().unwrap()).await;

        // A dropped stream frees its slot once the connection has cleaned it up
        drop(outbound.pop());
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if dialer_control.open_stream().await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(result.is_ok(), "the slot of a dropped stream was not freed");
    }

    #[tokio::test]
    async fn the_local_cap_counts_inbound_streams_that_are_not_accepted() {
        const LOCAL_CAP: usize = YAMUX_MAX_NUM_STREAMS - LOCAL_OPEN_HEADROOM;
        // The worker holds this many unaccepted streams: its queue, the incoming channel, and one passed to the channel
        // sender that is waiting for room
        const NUM_HELD: usize = MAX_QUEUED_INBOUND_STREAMS + INCOMING_CHANNEL_SIZE + 1;
        let (dialer, listener) = MemorySocket::new_pair();

        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut dialer_incoming = dialer.into_incoming();
        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();
        let mut listener_control = listener.get_yamux_control();

        // The listener does not accept these. Once it holds NUM_HELD it stops reading, so the rest stay unread.
        let mut unaccepted = Vec::with_capacity(NUM_HELD + 5);
        for _ in 0..NUM_HELD + 5 {
            let mut stream = dialer_control.open_stream().await.unwrap();
            stream.write_all(b"hello").await.unwrap();
            unaccepted.push(stream);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;

        // The listener may open streams up to its local cap, counting the streams it holds
        let mut outbound = Vec::with_capacity(LOCAL_CAP - NUM_HELD);
        for _ in 0..LOCAL_CAP - NUM_HELD {
            outbound.push(listener_control.open_stream().await.unwrap());
        }
        let err = listener_control.open_stream().await.unwrap_err();
        assert!(matches!(err, YamuxControlError::TooManyStreams), "{err:?}");

        // The refusal leaves the connection up: writes still go through, and the held streams are delivered
        let mut listener_stream = outbound.pop().unwrap();
        listener_stream.write_all(b"still up").await.unwrap();
        let mut dialer_stream = tokio::time::timeout(Duration::from_secs(10), dialer_incoming.next())
            .await
            .unwrap()
            .unwrap();
        let mut buf = [0u8; 8];
        dialer_stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"still up");
        let mut held = tokio::time::timeout(Duration::from_secs(10), listener.incoming.next())
            .await
            .unwrap()
            .unwrap();
        let mut buf = [0u8; 5];
        held.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
    }

    /// Checks that `outbound` and `inbound`, the two ends of one stream, carry data both ways
    async fn assert_stream_carries_data(outbound: &mut Substream, inbound: &mut Substream) {
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            outbound.write_all(b"ping").await.unwrap();
            let mut buf = [0u8; 4];
            inbound.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            inbound.write_all(b"pong").await.unwrap();
            outbound.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"pong");
        })
        .await;
        assert!(result.is_ok(), "the stream stalled");
    }

    #[tokio::test]
    async fn inbound_streams_beyond_the_cap_close_the_connection() {
        let (dialer, listener) = MemorySocket::new_pair();

        // The dialer allows more streams than we do
        let dialer = Yamux::upgrade_connection_with_stream_limit(
            dialer,
            ConnectionDirection::Outbound,
            PeerConnectionInfo::default(),
            DEFAULT_YAMUX_MAX_NUM_STREAMS,
        )
        .unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();

        // Accept and hold every inbound stream
        let accept = tokio::spawn(async move {
            let mut inbound = Vec::new();
            while let Some(stream) = listener.incoming.next().await {
                inbound.push(stream);
            }
            inbound.len()
        });

        let mut outbound = Vec::with_capacity(YAMUX_MAX_NUM_STREAMS + 1);
        for _ in 0..=YAMUX_MAX_NUM_STREAMS {
            let Ok(mut stream) = dialer_control.open_stream().await else {
                break;
            };
            if stream.write_all(b"hello").await.is_err() {
                break;
            }
            outbound.push(stream);
        }

        // yamux refuses the stream past the cap by closing the connection, which ends the incoming stream
        let num_accepted = tokio::time::timeout(Duration::from_secs(10), accept)
            .await
            .expect("the connection was not closed")
            .unwrap();
        assert_eq!(num_accepted, YAMUX_MAX_NUM_STREAMS);
    }

    #[tokio::test]
    async fn reads_pause_while_inbound_streams_are_not_accepted() {
        // More streams than the worker queues and the incoming channel hold together
        const NUM_STREAMS: usize = 100;
        const MAX_HELD_STREAMS: usize = MAX_QUEUED_INBOUND_STREAMS + INCOMING_CHANNEL_SIZE;
        let (dialer, listener) = tokio::io::duplex(1 << 20);
        let bytes_read = Arc::new(AtomicUsize::new(0));
        let listener = CountingSocket {
            inner: listener,
            bytes_read: bytes_read.clone(),
        };

        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut dialer_incoming = dialer.into_incoming();
        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();
        let mut listener_control = listener.get_yamux_control();

        // The listener does not accept any of these. Each carries a full stream window of data.
        let data = vec![0x55u8; STREAM_WINDOW];
        let mut outbound = Vec::with_capacity(NUM_STREAMS);
        for _ in 0..NUM_STREAMS {
            let mut stream = dialer_control.open_stream().await.unwrap();
            stream.write_all(&data).await.unwrap();
            outbound.push(stream);
        }

        // The listener stops reading once its queue is full, so it holds the data of at most the held streams plus
        // the frames that were in flight
        let max_bytes_read = (MAX_HELD_STREAMS + 1) * STREAM_WINDOW + (1 << 20);
        tokio::time::sleep(Duration::from_secs(1)).await;
        let held = bytes_read.load(Ordering::SeqCst);
        assert!(
            held <= max_bytes_read,
            "read {held} bytes, more than the bound of {max_bytes_read}"
        );
        assert!(held >= MAX_HELD_STREAMS * STREAM_WINDOW, "read only {held} bytes");
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(bytes_read.load(Ordering::SeqCst), held, "reads did not stop");

        // Writes still progress while reads are paused
        let mut listener_stream = listener_control.open_stream().await.unwrap();
        let reply = vec![0xAAu8; 100 * 1024];
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            listener_stream.write_all(&reply).await.unwrap();
            let mut dialer_stream = dialer_incoming.next().await.unwrap();
            let mut buf = vec![0u8; reply.len()];
            dialer_stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(buf, reply);
        })
        .await;
        assert!(result.is_ok(), "outbound writes stalled while reads were paused");

        // Once the streams are accepted, reading resumes and none of them was reset
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            for _ in 0..NUM_STREAMS {
                let mut stream = listener.incoming.next().await.unwrap();
                let mut buf = vec![0u8; STREAM_WINDOW];
                stream.read_exact(&mut buf).await.unwrap();
                assert_eq!(buf, data);
            }
        })
        .await;
        assert!(result.is_ok(), "inbound streams were not delivered after reads resumed");
    }

    #[tokio::test(start_paused = true)]
    async fn connection_closes_when_the_peer_stops_reading() {
        // More streams than we allow, so the peer can try to pin more than the stream cap
        const NUM_PEER_STREAMS: usize = YAMUX_MAX_NUM_STREAMS + 50;
        let (local, peer) = tokio::io::duplex(64 * 1024);
        let local =
            Yamux::upgrade_connection(local, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut local_control = local.get_yamux_control();
        let write_stall = local.write_stall();
        let mut local_incoming = local.into_incoming();
        // The peer writes but never reads
        let peer = Yamux::upgrade_connection_with_stream_limit(
            NeverReadSocket { inner: peer },
            ConnectionDirection::Inbound,
            PeerConnectionInfo::default(),
            DEFAULT_YAMUX_MAX_NUM_STREAMS,
        )
        .unwrap();
        let mut peer_control = peer.get_yamux_control();
        let _peer_incoming = peer.into_incoming();

        let start = tokio::time::Instant::now();
        // Fill the socket so that our writes wait
        let mut local_stream = local_control.open_stream().await.unwrap();
        let writer = tokio::spawn(async move {
            let _ignore = local_stream.write_all(&vec![0u8; STREAM_WINDOW]).await;
            local_stream
        });
        while write_stall.stalled_for().is_none() {
            tokio::task::yield_now().await;
        }

        // The peer opens streams which we accept and drop. yamux cannot clean them up without writing, so they are
        // only freed when the connection closes.
        let peer_streams = tokio::spawn(async move {
            let mut streams = Vec::with_capacity(NUM_PEER_STREAMS);
            for _ in 0..NUM_PEER_STREAMS {
                let Ok(mut stream) = peer_control.open_stream().await else {
                    break;
                };
                if stream.write_all(&[1u8; 1024]).await.is_err() {
                    break;
                }
                streams.push(stream);
            }
            streams
        });

        let mut num_accepted = 0usize;
        let result = tokio::time::timeout(WRITE_STALL_TIMEOUT + Duration::from_secs(1), async {
            while let Some(stream) = local_incoming.next().await {
                num_accepted = num_accepted.saturating_add(1);
                drop(stream);
                if num_accepted == 1 {
                    // Time only advances once every task is idle, so the stall is reported after it has lasted long
                    // enough
                    tokio::time::sleep(WRITE_STALL_REPORT_AFTER).await;
                    assert!(write_stall.is_stalled());
                }
            }
        })
        .await;
        assert!(result.is_ok(), "the connection was not closed");
        assert!(
            start.elapsed() >= WRITE_STALL_TIMEOUT,
            "closed after {:.0?}",
            start.elapsed()
        );
        // The streams we dropped stayed in yamux until the connection closed, but no more than the stream cap
        assert!(num_accepted <= YAMUX_MAX_NUM_STREAMS, "accepted {num_accepted} streams");
        assert!(local_control.open_stream().await.is_err());

        writer.abort();
        peer_streams.abort();
    }

    /// A transport that accepts at most `bytes_per_tick` bytes per tick, and nothing at all once `accepts_writes` is
    /// cleared
    struct SlowWriteSocket {
        inner: DuplexStream,
        interval: tokio::time::Interval,
        bytes_per_tick: usize,
        accepts_writes: Arc<AtomicBool>,
    }

    impl SlowWriteSocket {
        fn new(inner: DuplexStream, tick: Duration, bytes_per_tick: usize, accepts_writes: Arc<AtomicBool>) -> Self {
            let mut interval = tokio::time::interval(tick);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            Self {
                inner,
                interval,
                bytes_per_tick,
                accepts_writes,
            }
        }
    }

    impl AsyncRead for SlowWriteSocket {
        fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
        }
    }

    impl AsyncWrite for SlowWriteSocket {
        fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            if !this.accepts_writes.load(Ordering::SeqCst) {
                return Poll::Pending;
            }
            if this.interval.poll_tick(cx).is_pending() {
                return Poll::Pending;
            }
            let len = buf.len().min(this.bytes_per_tick);
            Pin::new(&mut this.inner).poll_write(cx, buf.get(..len).unwrap_or_default())
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
        }
    }

    /// Connects a local yamux over noise over a slow transport, measuring write stalls on the transport as the
    /// connection manager does, to a remote yamux over noise that reads at full speed
    async fn connect_over_slow_noise_transport(
        tick: Duration,
        bytes_per_tick: usize,
        accepts_writes: Arc<AtomicBool>,
    ) -> (Yamux, Yamux) {
        let (local, remote) = tokio::io::duplex(1 << 20);
        let local = WriteStallSocket::new(SlowWriteSocket::new(local, tick, bytes_per_tick, accepts_writes));
        let local_config = NoiseConfig::new(build_node_identity(PeerFeatures::COMMUNICATION_NODE));
        let remote_config = NoiseConfig::new(build_node_identity(PeerFeatures::COMMUNICATION_NODE));
        let (local, remote) = futures::future::join(
            local_config.upgrade_socket(local, ConnectionDirection::Outbound),
            remote_config.upgrade_socket(remote, ConnectionDirection::Inbound),
        )
        .await;
        let local = local.unwrap();
        let write_stall = local.get_ref().write_stall();
        let local = Yamux::upgrade_connection_with_write_stall(
            local,
            ConnectionDirection::Outbound,
            PeerConnectionInfo::default(),
            write_stall,
        )
        .unwrap();
        let remote = Yamux::upgrade_connection(
            remote.unwrap(),
            ConnectionDirection::Inbound,
            PeerConnectionInfo::default(),
        )
        .unwrap();
        (local, remote)
    }

    /// A transport so slow that one noise frame takes longer than the stall timeout to drain is not stalled while it
    /// keeps accepting bytes, because the stall is measured on the transport, under noise
    #[tokio::test(start_paused = true)]
    async fn a_noise_frame_that_drains_slowly_is_not_a_write_stall() {
        const DATA_LEN: usize = 128 * 1024;
        // 100 bytes every 250 ms: 400 B/s, so a 64 KiB noise frame takes about 164 s to drain
        let (local, mut remote) =
            connect_over_slow_noise_transport(Duration::from_millis(250), 100, Arc::new(AtomicBool::new(true))).await;
        let write_stall = local.write_stall();
        let mut local_control = local.get_yamux_control();

        let max_stalled_for = Arc::new(Mutex::new(Duration::ZERO));
        let sampler = tokio::spawn({
            let max_stalled_for = max_stalled_for.clone();
            let write_stall = write_stall.clone();
            async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let stalled_for = write_stall.stalled_for().unwrap_or_default();
                    let mut max = max_stalled_for.lock().unwrap();
                    if stalled_for > *max {
                        *max = stalled_for;
                    }
                }
            }
        });

        let start = tokio::time::Instant::now();
        let mut stream = local_control.open_stream().await.unwrap();
        let writer = tokio::spawn(async move {
            stream.write_all(&vec![0x55u8; DATA_LEN]).await.unwrap();
            stream
        });
        let mut inbound = remote.incoming.next().await.unwrap();
        let mut buf = vec![0u8; DATA_LEN];
        inbound.read_exact(&mut buf).await.unwrap();
        let elapsed = start.elapsed();
        sampler.abort();
        let _stream = writer.await.unwrap();

        assert!(
            elapsed > WRITE_STALL_TIMEOUT,
            "the transport was not slow: {elapsed:.0?}"
        );
        let max_stalled_for = *max_stalled_for.lock().unwrap();
        assert!(
            max_stalled_for < Duration::from_secs(1),
            "stalled for {max_stalled_for:.0?} while the transport kept accepting bytes"
        );
        assert!(!write_stall.is_stalled());
    }

    /// A transport that accepts nothing closes the connection at the stall timeout, with noise in between
    #[tokio::test(start_paused = true)]
    async fn a_noise_transport_that_accepts_nothing_is_closed() {
        let accepts_writes = Arc::new(AtomicBool::new(true));
        let (local, _remote) =
            connect_over_slow_noise_transport(Duration::from_millis(1), 64 * 1024, accepts_writes.clone()).await;
        let mut local_control = local.get_yamux_control();
        let mut local_incoming = local.into_incoming();

        accepts_writes.store(false, Ordering::SeqCst);
        let start = tokio::time::Instant::now();
        let mut stream = local_control.open_stream().await.unwrap();
        let writer = tokio::spawn(async move {
            let _ignore = stream.write_all(&[0u8; 1024]).await;
            stream
        });

        let result = tokio::time::timeout(WRITE_STALL_TIMEOUT + Duration::from_secs(1), local_incoming.next()).await;
        assert!(matches!(result, Ok(None)), "the connection was not closed");
        assert!(
            start.elapsed() >= WRITE_STALL_TIMEOUT,
            "closed after {:.0?}",
            start.elapsed()
        );
        writer.abort();
    }

    async fn poll_write_stall_once<T>(worker: &mut YamuxWorker<T>)
    where T: futures::AsyncRead + futures::AsyncWrite + Unpin + Send + Sync + 'static {
        std::future::poll_fn(|cx| {
            assert!(worker.poll_write_stall(cx).is_pending());
            Poll::Ready(())
        })
        .await;
    }

    /// A transport that accepts bytes and blocks again between two polls starts a new stall, which is reported on its
    /// own once it lasts long enough
    #[tokio::test(start_paused = true)]
    async fn a_new_stall_ends_the_report_of_the_previous_one() {
        let (incoming_tx, _incoming_rx) = tokio::sync::mpsc::channel(INCOMING_CHANNEL_SIZE);
        let (_request_tx, request_rx) = tokio::sync::mpsc::channel(1);
        let write_stall = WriteStall::default();
        let mut worker = YamuxWorker::<tokio_util::compat::Compat<DuplexStream>>::new(
            incoming_tx,
            request_rx,
            AtomicRefCounter::new(),
            PeerConnectionInfo::default(),
            write_stall.clone(),
            Arc::new(AtomicBool::new(false)),
            YAMUX_MAX_NUM_STREAMS,
        );

        let now = tokio::time::Instant::now();
        let first = now
            .checked_sub(WRITE_STALL_REPORT_AFTER + Duration::from_secs(1))
            .unwrap();
        write_stall.set_stalled_since(first);
        poll_write_stall_once(&mut worker).await;
        assert_eq!(worker.reported_write_stall_since, Some(first));

        // The transport accepted bytes and blocked again, all between two polls
        let second = now.checked_sub(Duration::from_secs(1)).unwrap();
        write_stall.set_stalled_since(second);
        poll_write_stall_once(&mut worker).await;
        assert_eq!(worker.reported_write_stall_since, None);

        tokio::time::advance(WRITE_STALL_REPORT_AFTER).await;
        poll_write_stall_once(&mut worker).await;
        assert_eq!(worker.reported_write_stall_since, Some(second));
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_link_that_keeps_accepting_data_is_not_write_stalled() {
        // Several streams keep yamux writing all the time
        const NUM_STREAMS: usize = 4;
        const DATA_LEN: usize = 1024 * 1024;
        // 4 KiB every 10 ms: about 400 KB/s, so the data takes about ten seconds to cross
        const BYTES_PER_TICK: usize = 4 * 1024;
        let (local, local_link) = tokio::io::duplex(64 * 1024);
        let (remote_link, remote) = tokio::io::duplex(64 * 1024);
        let (local_link_read, local_link_write) = tokio::io::split(local_link);
        let (remote_link_read, remote_link_write) = tokio::io::split(remote_link);
        // The link is slow towards the remote and fast back
        let to_remote = tokio::spawn(async move {
            let (mut reader, mut writer) = (local_link_read, remote_link_write);
            let mut buf = vec![0u8; BYTES_PER_TICK];
            loop {
                let n = match reader.read(&mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                if writer.write_all(buf.get(..n).unwrap_or_default()).await.is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        let to_local = tokio::spawn(async move {
            let (mut reader, mut writer) = (remote_link_read, local_link_write);
            let _ignore = tokio::io::copy(&mut reader, &mut writer).await;
        });

        let local_config = NoiseConfig::new(build_node_identity(PeerFeatures::COMMUNICATION_NODE));
        let remote_config = NoiseConfig::new(build_node_identity(PeerFeatures::COMMUNICATION_NODE));
        let (local, remote) = futures::future::join(
            local_config.upgrade_socket(local, ConnectionDirection::Outbound),
            remote_config.upgrade_socket(remote, ConnectionDirection::Inbound),
        )
        .await;
        let local = Yamux::upgrade_connection(
            local.unwrap(),
            ConnectionDirection::Outbound,
            PeerConnectionInfo::default(),
        )
        .unwrap();
        let mut remote = Yamux::upgrade_connection(
            remote.unwrap(),
            ConnectionDirection::Inbound,
            PeerConnectionInfo::default(),
        )
        .unwrap();
        let write_stall = local.write_stall();
        let mut local_control = local.get_yamux_control();

        let reader = tokio::spawn(async move {
            let mut readers = Vec::with_capacity(NUM_STREAMS);
            for _ in 0..NUM_STREAMS {
                let mut stream = remote.incoming.next().await.unwrap();
                readers.push(tokio::spawn(async move {
                    let mut buf = vec![0u8; DATA_LEN];
                    stream.read_exact(&mut buf).await.unwrap();
                }));
            }
            for reader in readers {
                reader.await.unwrap();
            }
        });
        let max_stalled_for = Arc::new(Mutex::new(Duration::ZERO));
        let sampler = tokio::spawn({
            let max_stalled_for = max_stalled_for.clone();
            async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let stalled_for = write_stall.stalled_for().unwrap_or_default();
                    let mut max = max_stalled_for.lock().unwrap();
                    if stalled_for > *max {
                        *max = stalled_for;
                    }
                }
            }
        });

        let start = tokio::time::Instant::now();
        let mut writers = Vec::with_capacity(NUM_STREAMS);
        for _ in 0..NUM_STREAMS {
            let mut stream = local_control.open_stream().await.unwrap();
            writers.push(tokio::spawn(async move {
                stream.write_all(&vec![0x55u8; DATA_LEN]).await.unwrap();
                stream
            }));
        }
        for writer in writers {
            writer.await.unwrap();
        }
        reader.await.unwrap();
        let elapsed = start.elapsed();
        sampler.abort();

        // The link was busy for seconds, but the socket kept accepting data
        assert!(
            elapsed >= Duration::from_secs(2),
            "the link was not slow: {elapsed:.0?}"
        );
        let max_stalled_for = *max_stalled_for.lock().unwrap();
        assert!(
            max_stalled_for < Duration::from_secs(1),
            "stalled for {max_stalled_for:.0?} while data was flowing for {elapsed:.0?}"
        );
        assert!(max_stalled_for < WRITE_STALL_REPORT_AFTER);
        to_remote.abort();
        to_local.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn an_idle_connection_is_not_closed() {
        let (dialer, listener) = MemorySocket::new_pair();
        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();

        let mut stream = dialer_control.open_stream().await.unwrap();
        stream.write_all(b"hello").await.unwrap();
        let mut inbound = listener.incoming.next().await.unwrap();
        let mut buf = [0u8; 5];
        inbound.read_exact(&mut buf).await.unwrap();

        tokio::time::sleep(WRITE_STALL_TIMEOUT * 2).await;
        assert!(dialer.write_stall().stalled_for().is_none());
        assert!(listener.write_stall().stalled_for().is_none());
        stream.write_all(b"again").await.unwrap();
        inbound.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"again");
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

    #[tokio::test(start_paused = true)]
    async fn close_completes_when_peer_stops_reading() {
        // The peer end is never read, so the connection's writes back up once this small buffer is full
        let (socket, _peer) = tokio::io::duplex(64);
        let yamux =
            Yamux::upgrade_connection(socket, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut control = yamux.get_yamux_control();

        let mut substream = control.open_stream().await.unwrap();
        let writer = tokio::spawn(async move {
            let _ignore = substream.write_all(&[0u8; 64 * 1024]).await;
        });
        tokio::task::yield_now().await;

        // Time is paused and advances when every task is idle, so this does not wait in real time
        let result = tokio::time::timeout(super::CONNECTION_CLOSE_TIMEOUT * 2, control.close()).await;
        assert!(result.is_ok(), "close did not return");

        writer.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn queued_inbound_streams_are_delivered_after_connection_closes() {
        // More than the incoming channel holds, so some are still queued in the worker when the connection closes
        const NUM_STREAMS: usize = 20;
        let (dialer, listener) = MemorySocket::new_pair();

        let dialer =
            Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound, PeerConnectionInfo::default()).unwrap();
        let mut dialer_control = dialer.get_yamux_control();
        let mut listener =
            Yamux::upgrade_connection(listener, ConnectionDirection::Inbound, PeerConnectionInfo::default()).unwrap();

        let mut outbound = Vec::with_capacity(NUM_STREAMS);
        for _ in 0..NUM_STREAMS {
            let mut stream = dialer_control.open_stream().await.unwrap();
            stream.write_all(b"hello").await.unwrap();
            outbound.push(stream);
        }
        dialer_control.close().await.unwrap();
        // Time is paused, so this sleep only completes once every other task is idle, i.e. after the listener's worker
        // has seen the connection close
        tokio::time::sleep(Duration::from_secs(1)).await;

        let mut inbound = collect_stream!(
            &mut listener.incoming,
            take = NUM_STREAMS,
            timeout = Duration::from_secs(10)
        );
        assert_eq!(inbound.len(), NUM_STREAMS);
        for stream in &mut inbound {
            let mut buf = [0u8; 5];
            stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"hello");
        }
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
