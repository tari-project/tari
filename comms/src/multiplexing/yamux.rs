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

use crate::{connection_manager::ConnectionDirection, runtime};
use futures::{
    channel::mpsc,
    future,
    future::Either,
    io::{AsyncRead, AsyncWrite},
    stream::FusedStream,
    task::Context,
    SinkExt,
    Stream,
    StreamExt,
};
use log::*;
use std::{future::Future, io, pin::Pin, sync::Arc, task::Poll};
use tari_shutdown::{Shutdown, ShutdownSignal};
use yamux::Mode;

type IncomingRx = mpsc::Receiver<yamux::Stream>;
type IncomingTx = mpsc::Sender<yamux::Stream>;

// Reexport
pub use yamux::ConnectionError;

const LOG_TARGET: &str = "comms::multiplexing::yamux";

pub struct Yamux {
    control: Control,
    incoming: IncomingSubstreams,
    substream_counter: SubstreamCounter,
}

const MAX_BUFFER_SIZE: u32 = 8 * 1024 * 1024; // 8MB
const RECEIVE_WINDOW: u32 = 4 * 1024 * 1024; // 4MB

impl Yamux {
    /// Upgrade the underlying socket to use yamux
    pub async fn upgrade_connection<TSocket>(socket: TSocket, direction: ConnectionDirection) -> io::Result<Self>
    where TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static {
        let mode = match direction {
            ConnectionDirection::Inbound => Mode::Server,
            ConnectionDirection::Outbound => Mode::Client,
        };

        let mut config = yamux::Config::default();
        // Use OnRead mode instead of OnReceive mode to provide back pressure to the sending side.
        // Caveat: the OnRead mode has the risk of deadlock, where both sides send data larger than
        // receive window and don't read before finishing writes.
        // This should never happen as the window size should be large enough for all protocol messages.
        config.set_window_update_mode(yamux::WindowUpdateMode::OnRead);
        // Because OnRead mode increases the RTT of window update, bigger buffer size and receive
        // window size perform better.
        config.set_max_buffer_size(MAX_BUFFER_SIZE as usize);
        config.set_receive_window(RECEIVE_WINDOW);

        let substream_counter = SubstreamCounter::new();
        let connection = yamux::Connection::new(socket, config, mode);
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
        counter: SubstreamCounter,
    ) -> IncomingSubstreams
    where
        TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let shutdown = Shutdown::new();
        let (incoming_tx, incoming_rx) = mpsc::channel(10);
        let stream = yamux::into_stream(connection).boxed();
        let incoming = IncomingWorker::new(stream, incoming_tx, shutdown.to_signal());
        runtime::current().spawn(incoming.run());
        IncomingSubstreams::new(incoming_rx, counter, shutdown)
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
    pub fn incoming(self) -> IncomingSubstreams {
        self.incoming
    }

    /// Return the number of active substreams
    pub fn substream_count(&self) -> usize {
        self.substream_counter.get()
    }

    /// Return a SubstreamCounter for this connection
    pub(crate) fn substream_counter(&self) -> SubstreamCounter {
        self.substream_counter.clone()
    }

    pub fn is_terminated(&self) -> bool {
        self.incoming.is_terminated()
    }
}

#[derive(Clone)]
pub struct Control {
    inner: yamux::Control,
    substream_counter: SubstreamCounter,
}

impl Control {
    pub fn new(inner: yamux::Control, substream_counter: SubstreamCounter) -> Self {
        Self {
            inner,
            substream_counter,
        }
    }

    /// Open a new stream to the remote.
    pub async fn open_stream(&mut self) -> Result<Substream, ConnectionError> {
        let stream = self.inner.open_stream().await?;
        Ok(Substream {
            stream,
            counter_guard: self.substream_counter.new_guard(),
        })
    }

    /// Close the connection.
    pub fn close(&mut self) -> impl Future<Output = Result<(), ConnectionError>> + '_ {
        self.inner.close()
    }

    pub fn substream_count(&self) -> usize {
        self.substream_counter.get()
    }

    pub(crate) fn substream_counter(&self) -> SubstreamCounter {
        self.substream_counter.clone()
    }
}

pub struct IncomingSubstreams {
    inner: IncomingRx,
    substream_counter: SubstreamCounter,
    shutdown: Shutdown,
}

impl IncomingSubstreams {
    pub fn new(inner: IncomingRx, substream_counter: SubstreamCounter, shutdown: Shutdown) -> Self {
        Self {
            inner,
            substream_counter,
            shutdown,
        }
    }

    pub fn substream_count(&self) -> usize {
        self.substream_counter.get()
    }
}

impl FusedStream for IncomingSubstreams {
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

impl Stream for IncomingSubstreams {
    type Item = Substream;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match futures::ready!(Pin::new(&mut self.inner).poll_next(cx)) {
            Some(stream) => Poll::Ready(Some(Substream {
                stream,
                counter_guard: self.substream_counter.new_guard(),
            })),
            None => Poll::Ready(None),
        }
    }
}

impl Drop for IncomingSubstreams {
    fn drop(&mut self) {
        let _ = self.shutdown.trigger();
    }
}

#[derive(Debug)]
pub struct Substream {
    stream: yamux::Stream,
    counter_guard: CounterGuard,
}

impl AsyncRead for Substream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for Substream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_close(cx)
    }
}

struct IncomingWorker<S> {
    inner: S,
    sender: mpsc::Sender<yamux::Stream>,
    shutdown_signal: Option<ShutdownSignal>,
}

impl<S> IncomingWorker<S>
where S: Stream<Item = Result<yamux::Stream, yamux::ConnectionError>> + Unpin
{
    pub fn new(stream: S, sender: IncomingTx, shutdown_signal: ShutdownSignal) -> Self {
        Self {
            inner: stream,
            sender,
            shutdown_signal: Some(shutdown_signal),
        }
    }

    pub async fn run(mut self) {
        let mut signal = self.shutdown_signal.take();
        loop {
            let either = future::select(self.inner.next(), signal.take().expect("cannot fail")).await;
            match either {
                Either::Left((Some(Err(err)), _)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Incoming peer substream task received an error because '{}'", err
                    );
                    break;
                },
                // Received a substream result
                Either::Left((Some(Ok(stream)), sig)) => {
                    signal = Some(sig);
                    if let Err(err) = self.sender.send(stream).await {
                        if err.is_disconnected() {
                            debug!(
                                target: LOG_TARGET,
                                "Incoming peer substream task is shutting down because the internal stream sender \
                                 channel was closed"
                            );
                            break;
                        }
                    }
                },
                // The substream closed
                Either::Left((None, _)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Incoming peer substream task is shutting down because the stream ended"
                    );
                    break;
                },
                // The shutdown signal was received
                Either::Right((_, _)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Incoming peer substream task is shutting down because the shutdown signal was received"
                    );
                    break;
                },
            }
        }

        self.sender.close_channel();
    }
}

pub type CounterGuard = Arc<()>;
#[derive(Debug, Clone, Default)]
pub struct SubstreamCounter(Arc<CounterGuard>);

impl SubstreamCounter {
    pub fn new() -> Self {
        Default::default()
    }

    /// Create a new CounterGuard. Each of these counts 1 in the substream count
    /// until it is dropped.
    pub fn new_guard(&self) -> CounterGuard {
        Arc::clone(&*self.0)
    }

    /// Get the substream count
    pub fn get(&self) -> usize {
        // Substract one to account for the initial CounterGuard reference
        Arc::strong_count(&*self.0) - 1
    }
}

#[cfg(test)]
mod test {
    use crate::{
        connection_manager::ConnectionDirection,
        memsocket::MemorySocket,
        multiplexing::yamux::Yamux,
        runtime::task,
    };
    use futures::{
        future,
        io::{AsyncReadExt, AsyncWriteExt},
        StreamExt,
    };
    use std::{io, time::Duration};
    use tari_test_utils::collect_stream;

    #[tokio_macros::test_basic]
    async fn open_substream() -> io::Result<()> {
        let (dialer, listener) = MemorySocket::new_pair();
        let msg = b"The Way of Kings";

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound)
            .await
            .unwrap();
        let mut dialer_control = dialer.get_yamux_control();

        task::spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

            substream.write_all(msg).await.unwrap();
            substream.flush().await.unwrap();
            substream.close().await.unwrap();
        });

        let mut listener = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)
            .await?
            .incoming();
        let mut substream = listener
            .next()
            .await
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no substream"))?;

        let mut buf = Vec::new();
        let _ = future::select(substream.read_to_end(&mut buf), listener.next()).await;
        assert_eq!(buf, msg);

        Ok(())
    }

    #[tokio_macros::test_basic]
    async fn substream_count() {
        const NUM_SUBSTREAMS: usize = 10;
        let (dialer, listener) = MemorySocket::new_pair();

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound)
            .await
            .unwrap();
        let mut dialer_control = dialer.get_yamux_control();

        let substreams_out = task::spawn(async move {
            let mut substreams = Vec::with_capacity(NUM_SUBSTREAMS);
            for _ in 0..NUM_SUBSTREAMS {
                substreams.push(dialer_control.open_stream().await.unwrap());
            }
            substreams
        });

        let mut listener = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)
            .await
            .unwrap()
            .incoming();
        let substreams_in = collect_stream!(&mut listener, take = NUM_SUBSTREAMS, timeout = Duration::from_secs(10));

        assert_eq!(dialer.substream_count(), NUM_SUBSTREAMS);
        assert_eq!(listener.substream_count(), NUM_SUBSTREAMS);

        drop(substreams_in);
        drop(substreams_out);

        assert_eq!(dialer.substream_count(), 0);
        assert_eq!(listener.substream_count(), 0);
    }

    #[tokio_macros::test_basic]
    async fn close() -> io::Result<()> {
        let (dialer, listener) = MemorySocket::new_pair();
        let msg = b"Words of Radiance";

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound).await?;
        let mut dialer_control = dialer.get_yamux_control();

        task::spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

            substream.write_all(msg).await.unwrap();
            substream.flush().await.unwrap();

            let mut buf = Vec::new();
            substream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"");
        });

        let mut incoming = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)
            .await?
            .incoming();
        let mut substream = incoming.next().await.unwrap();

        let mut buf = vec![0; msg.len()];
        substream.read_exact(&mut buf).await?;
        assert_eq!(buf, msg);

        // Close the substream and then try to write to it
        substream.close().await?;

        let result = substream.write_all(b"ignored message").await;
        match result {
            Ok(()) => panic!("Write should have failed"),
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::WriteZero),
        }

        Ok(())
    }

    #[tokio_macros::test_basic]
    async fn send_big_message() -> io::Result<()> {
        #[allow(non_upper_case_globals)]
        static MiB: usize = 1 << 20;
        static MSG_LEN: usize = 16 * MiB;

        let (dialer, listener) = MemorySocket::new_pair();

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound).await?;
        let mut dialer_control = dialer.get_yamux_control();

        task::spawn(async move {
            assert_eq!(dialer_control.substream_count(), 0);
            let mut substream = dialer_control.open_stream().await.unwrap();
            assert_eq!(dialer_control.substream_count(), 1);

            let msg = vec![0x55u8; MSG_LEN];
            substream.write_all(msg.as_slice()).await.unwrap();

            let mut buf = vec![0u8; MSG_LEN];
            substream.read_exact(&mut buf).await.unwrap();
            substream.close().await.unwrap();

            assert_eq!(buf.len(), MSG_LEN);
            assert_eq!(buf, vec![0xAAu8; MSG_LEN]);
        });

        let mut incoming = Yamux::upgrade_connection(listener, ConnectionDirection::Inbound)
            .await?
            .incoming();
        assert_eq!(incoming.substream_count(), 0);
        let mut substream = incoming.next().await.unwrap();
        assert_eq!(incoming.substream_count(), 1);

        let mut buf = vec![0u8; MSG_LEN];
        substream.read_exact(&mut buf).await?;
        assert_eq!(buf, vec![0x55u8; MSG_LEN]);

        let msg = vec![0xAAu8; MSG_LEN];
        substream.write_all(msg.as_slice()).await?;
        substream.close().await?;
        drop(substream);

        assert_eq!(incoming.substream_count(), 0);

        Ok(())
    }
}
