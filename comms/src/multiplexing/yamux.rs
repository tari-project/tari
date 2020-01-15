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

use crate::connection::ConnectionDirection;
use futures::{
    io::{AsyncRead, AsyncWrite},
    stream::BoxStream,
    StreamExt,
};
use std::{fmt::Debug, io};
use yamux::Mode;

pub type IncomingSubstream<'a> = BoxStream<'a, Result<yamux::Stream, yamux::ConnectionError>>;

#[derive(Debug)]
pub struct Yamux<TSocket> {
    inner: yamux::Connection<TSocket>,
}

const MAX_BUFFER_SIZE: u32 = 8 * 1024 * 1024; // 8MB
const RECEIVE_WINDOW: u32 = 4 * 1024 * 1024; // 4MB

impl<TSocket> Yamux<TSocket>
where TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static
{
    pub fn new(socket: TSocket, mode: Mode) -> Self {
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

        Self {
            inner: yamux::Connection::new(socket, config, mode),
        }
    }

    /// Upgrade the underlying socket to use yamux
    pub async fn upgrade_connection(socket: TSocket, direction: ConnectionDirection) -> io::Result<Self> {
        let mode = match direction {
            ConnectionDirection::Inbound => Mode::Server,
            ConnectionDirection::Outbound => Mode::Client,
        };

        Ok(Self::new(socket, mode))
    }

    /// Get the yamux control struct
    pub fn get_yamux_control(&self) -> yamux::Control {
        self.inner.control()
    }

    /// Returns a `Stream` emitting substreams initiated by the remote
    pub fn incoming(self) -> IncomingSubstream<'static> {
        yamux::into_stream(self.inner).boxed()
    }
}

#[cfg(test)]
mod test {
    use crate::{
        multiplexing::yamux::{Mode, Yamux},
        test_utils::tcp::build_connected_tcp_socket_pair,
    };
    use futures::{
        future,
        io::{AsyncReadExt, AsyncWriteExt},
        StreamExt,
    };
    use std::io;
    use tokio::runtime::Runtime;

    #[test]
    fn open_substream() -> io::Result<()> {
        let mut rt = Runtime::new().unwrap();
        let (dialer, listener) = rt.block_on(build_connected_tcp_socket_pair());
        let msg = b"The Way of Kings";

        let dialer = Yamux::new(dialer, Mode::Client);
        let mut dialer_control = dialer.get_yamux_control();
        // The incoming stream must be polled for the control to work
        rt.spawn(async move {
            dialer.incoming().next().await;
        });

        rt.spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

            substream.write_all(msg).await.unwrap();
            substream.flush().await.unwrap();
            substream.close().await.unwrap();
        });

        let mut listener = Yamux::new(listener, Mode::Server).incoming();
        let mut substream = rt
            .block_on(listener.next())
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no substream"))?
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        let mut buf = Vec::new();
        let _ = rt.block_on(future::select(substream.read_to_end(&mut buf), listener.next()));
        assert_eq!(buf, msg);

        Ok(())
    }

    #[test]
    fn close() -> io::Result<()> {
        let mut rt = Runtime::new().unwrap();
        let (dialer, listener) = rt.block_on(build_connected_tcp_socket_pair());
        let msg = b"Words of Radiance";

        let dialer = Yamux::new(dialer, Mode::Client);
        let mut dialer_control = dialer.get_yamux_control();
        // The incoming stream must be polled for the control to work
        rt.spawn(async move {
            dialer.incoming().next().await;
        });

        rt.spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

            substream.write_all(msg).await.unwrap();
            substream.flush().await.unwrap();

            let mut buf = Vec::new();
            substream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"");
        });

        let mut incoming = Yamux::new(listener, Mode::Server).incoming();
        let mut substream = rt.block_on(incoming.next()).unwrap().unwrap();
        rt.spawn(async move {
            incoming.next().await;
        });

        rt.block_on(async move {
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

            io::Result::Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn send_big_message() -> io::Result<()> {
        let mut rt = Runtime::new().unwrap();
        #[allow(non_upper_case_globals)]
        static MiB: usize = 1 << 20;
        static MSG_LEN: usize = 16 * MiB;

        let (dialer, listener) = rt.block_on(build_connected_tcp_socket_pair());

        let dialer = Yamux::new(dialer, Mode::Client);
        let mut dialer_control = dialer.get_yamux_control();
        // The incoming stream must be polled for the control to work
        rt.spawn(async move {
            dialer.incoming().next().await;
        });

        rt.spawn(async move {
            let mut substream = dialer_control.open_stream().await.unwrap();

            let msg = vec![0x55u8; MSG_LEN];
            substream.write_all(msg.as_slice()).await.unwrap();

            let mut buf = vec![0u8; MSG_LEN];
            substream.read_exact(&mut buf).await.unwrap();
            substream.close().await.unwrap();

            assert_eq!(buf.len(), MSG_LEN);
            assert_eq!(buf, vec![0xAAu8; MSG_LEN]);
        });

        let mut incoming = Yamux::new(listener, Mode::Server).incoming();
        let mut substream = rt.block_on(incoming.next()).unwrap().unwrap();
        rt.spawn(async move {
            incoming.next().await;
        });

        rt.block_on(async move {
            let mut buf = vec![0u8; MSG_LEN];
            substream.read_exact(&mut buf).await?;
            assert_eq!(buf, vec![0x55u8; MSG_LEN]);

            let msg = vec![0xAAu8; MSG_LEN];
            substream.write_all(msg.as_slice()).await?;
            substream.close().await?;

            io::Result::Ok(())
        })?;

        Ok(())
    }
}
