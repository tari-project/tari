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
    task::Context,
    SinkExt,
    Stream,
    StreamExt,
};
use log::*;
use std::{io, pin::Pin, task::Poll};
use tari_shutdown::{Shutdown, ShutdownSignal};
use yamux::Mode;

type IncomingRx = mpsc::Receiver<yamux::Stream>;
type IncomingTx = mpsc::Sender<yamux::Stream>;

pub type Control = yamux::Control;

const LOG_TARGET: &str = "comms::multiplexing::yamux";

pub struct Yamux {
    control: Control,
    incoming: IncomingSubstreams,
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

        let connection = yamux::Connection::new(socket, config, mode);
        let control = connection.control();

        let incoming = Self::spawn_incoming_stream_worker(connection);

        Ok(Self { control, incoming })
    }

    // yamux@0.4 requires the incoming substream stream be polled in order to make progress on requests from it's
    // Control api. Here we spawn off a worker which will do this job
    fn spawn_incoming_stream_worker<TSocket>(connection: yamux::Connection<TSocket>) -> IncomingSubstreams
    where TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static {
        let shutdown = Shutdown::new();
        let (incoming_tx, incoming_rx) = mpsc::channel(10);
        let stream = yamux::into_stream(connection).boxed();
        let incoming = IncomingWorker::new(stream, incoming_tx, shutdown.to_signal());
        runtime::current_executor().spawn(incoming.run());
        IncomingSubstreams::new(incoming_rx, shutdown)
    }

    /// Get the yamux control struct
    pub fn get_yamux_control(&self) -> yamux::Control {
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
}

pub struct IncomingSubstreams {
    inner: IncomingRx,
    shutdown: Shutdown,
}

impl IncomingSubstreams {
    pub fn new(inner: IncomingRx, shutdown: Shutdown) -> Self {
        Self { inner, shutdown }
    }
}

impl Stream for IncomingSubstreams {
    type Item = yamux::Stream;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl Drop for IncomingSubstreams {
    fn drop(&mut self) {
        let _ = self.shutdown.trigger();
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

#[cfg(test)]
mod test {
    use crate::{connection_manager::ConnectionDirection, memsocket::MemorySocket, multiplexing::yamux::Yamux};
    use futures::{
        future,
        io::{AsyncReadExt, AsyncWriteExt},
        StreamExt,
    };
    use std::io;
    use tokio::runtime::Handle;

    #[tokio_macros::test_basic]
    async fn open_substream() -> io::Result<()> {
        let (dialer, listener) = MemorySocket::new_pair();
        let msg = b"The Way of Kings";
        let rt_handle = Handle::current();

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound)
            .await
            .unwrap();
        let mut dialer_control = dialer.get_yamux_control();

        rt_handle.spawn(async move {
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
    async fn close() -> io::Result<()> {
        let (dialer, listener) = MemorySocket::new_pair();
        let msg = b"Words of Radiance";
        let rt_handle = Handle::current();

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound).await?;
        let mut dialer_control = dialer.get_yamux_control();

        rt_handle.spawn(async move {
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
        rt_handle.spawn(async move {
            incoming.next().await;
        });

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
        let rt_handle = Handle::current();
        #[allow(non_upper_case_globals)]
        static MiB: usize = 1 << 20;
        static MSG_LEN: usize = 16 * MiB;

        let (dialer, listener) = MemorySocket::new_pair();

        let dialer = Yamux::upgrade_connection(dialer, ConnectionDirection::Outbound).await?;
        let mut dialer_control = dialer.get_yamux_control();
        // The incoming stream must be polled for the control to work
        rt_handle.spawn(async move {
            dialer.incoming().next().await;
        });

        rt_handle.spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

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
        let mut substream = incoming.next().await.unwrap();
        rt_handle.spawn(async move {
            incoming.next().await;
        });

        let mut buf = vec![0u8; MSG_LEN];
        substream.read_exact(&mut buf).await?;
        assert_eq!(buf, vec![0x55u8; MSG_LEN]);

        let msg = vec![0xAAu8; MSG_LEN];
        substream.write_all(msg.as_slice()).await?;
        substream.close().await?;

        Ok(())
    }
}
