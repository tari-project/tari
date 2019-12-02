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

// This file is a slightly modified version of the Libra NoiseSocket implementation.
// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! Noise Socket

use futures::ready;
use log::*;
use snow::{error::StateProblem, HandshakeState, TransportState};
use std::{
    convert::TryInto,
    io,
    pin::Pin,
    task::{Context, Poll},
};
// use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use futures::{io::Error, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const LOG_TARGET: &str = "comms::noise::socket";

const MAX_PAYLOAD_LENGTH: usize = u16::max_value() as usize; // 65535

// The maximum number of bytes that we can buffer is 16 bytes less than u16::max_value() because
// encrypted messages include a tag along with the payload.
const MAX_WRITE_BUFFER_LENGTH: usize = u16::max_value() as usize - 16; // 65519

/// Collection of buffers used for buffering data during the various read/write states of a
/// NoiseSocket
struct NoiseBuffers {
    /// Encrypted frame read from the wire
    read_encrypted: [u8; MAX_PAYLOAD_LENGTH],
    /// Decrypted data read from the wire (produced by having snow decrypt the `read_encrypted`
    /// buffer)
    read_decrypted: [u8; MAX_PAYLOAD_LENGTH],
    /// Unencrypted data intended to be written to the wire
    write_decrypted: [u8; MAX_WRITE_BUFFER_LENGTH],
    /// Encrypted data to write to the wire (produced by having snow encrypt the `write_decrypted`
    /// buffer)
    write_encrypted: [u8; MAX_PAYLOAD_LENGTH],
}

impl NoiseBuffers {
    fn new() -> Self {
        Self {
            read_encrypted: [0; MAX_PAYLOAD_LENGTH],
            read_decrypted: [0; MAX_PAYLOAD_LENGTH],
            write_decrypted: [0; MAX_WRITE_BUFFER_LENGTH],
            write_encrypted: [0; MAX_PAYLOAD_LENGTH],
        }
    }
}

/// Hand written Debug implementation in order to omit the printing of huge buffers of data
impl ::std::fmt::Debug for NoiseBuffers {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct("NoiseBuffers").finish()
    }
}

/// Possible read states for a [NoiseSocket]
#[derive(Debug)]
enum ReadState {
    /// Initial State
    Init,
    /// Read frame length
    ReadFrameLen { buf: [u8; 2], offset: usize },
    /// Read encrypted frame
    ReadFrame { frame_len: u16, offset: usize },
    /// Copy decrypted frame to provided buffer
    CopyDecryptedFrame { decrypted_len: usize, offset: usize },
    /// End of file reached, result indicated if EOF was expected or not
    Eof(Result<(), ()>),
    /// Decryption Error
    DecryptionError(snow::Error),
}

/// Possible write states for a [NoiseSocket]
#[derive(Debug)]
enum WriteState {
    /// Initial State
    Init,
    /// Buffer provided data
    BufferData { offset: usize },
    /// Write frame length to the wire
    WriteFrameLen {
        frame_len: u16,
        buf: [u8; 2],
        offset: usize,
    },
    /// Write encrypted frame to the wire
    WriteEncryptedFrame { frame_len: u16, offset: usize },
    /// Flush the underlying socket
    Flush,
    /// End of file reached
    Eof,
    /// Encryption Error
    EncryptionError(snow::Error),
}

/// A Noise session with a remote
///
/// Encrypts data to be written to and decrypts data that is read from the underlying socket using
/// the noise protocol. This is done by wrapping noise payloads in u16 (big endian) length prefix
/// frames.
#[derive(Debug)]
pub struct NoiseSocket<TSocket> {
    socket: TSocket,
    state: NoiseState,
    buffers: Box<NoiseBuffers>,
    read_state: ReadState,
    write_state: WriteState,
}

impl<TSocket> NoiseSocket<TSocket> {
    fn new(socket: TSocket, session: NoiseState) -> Self {
        Self {
            socket,
            state: session,
            buffers: Box::new(NoiseBuffers::new()),
            read_state: ReadState::Init,
            write_state: WriteState::Init,
        }
    }

    /// Pull out the static public key of the remote
    pub fn get_remote_static(&self) -> Option<&[u8]> {
        self.state.get_remote_static()
    }
}

fn poll_write_all<TSocket>(
    mut context: &mut Context,
    mut socket: Pin<&mut TSocket>,
    buf: &[u8],
    offset: &mut usize,
) -> Poll<io::Result<()>>
where
    TSocket: AsyncWrite,
{
    loop {
        let n = ready!(socket.as_mut().poll_write(&mut context, &buf[*offset..]))?;
        trace!(
            target: LOG_TARGET,
            "poll_write_all: wrote {}/{} bytes",
            *offset + n,
            buf.len()
        );
        if n == 0 {
            return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
        }
        *offset += n;
        assert!(*offset <= buf.len());

        if *offset == buf.len() {
            return Poll::Ready(Ok(()));
        }
    }
}

/// Read a u16 frame length from `socket`.
///
/// Can result in the following output:
/// 1) Ok(None) => EOF; remote graceful shutdown
/// 2) Err(UnexpectedEOF) => read 1 byte then hit EOF; remote died
/// 3) Ok(Some(n)) => new frame of length n
fn poll_read_u16frame_len<TSocket>(
    context: &mut Context,
    socket: Pin<&mut TSocket>,
    buf: &mut [u8; 2],
    offset: &mut usize,
) -> Poll<io::Result<Option<u16>>>
where
    TSocket: AsyncRead,
{
    match ready!(poll_read_exact(context, socket, buf, offset)) {
        Ok(()) => Poll::Ready(Ok(Some(u16::from_be_bytes(*buf)))),
        Err(e) => {
            if *offset == 0 && e.kind() == io::ErrorKind::UnexpectedEof {
                return Poll::Ready(Ok(None));
            }
            Poll::Ready(Err(e))
        },
    }
}

fn poll_read_exact<TSocket>(
    mut context: &mut Context,
    mut socket: Pin<&mut TSocket>,
    buf: &mut [u8],
    offset: &mut usize,
) -> Poll<io::Result<()>>
where
    TSocket: AsyncRead,
{
    loop {
        let n = ready!(socket.as_mut().poll_read(&mut context, &mut buf[*offset..]))?;
        trace!(
            target: LOG_TARGET,
            "poll_read_exact: read {}/{} bytes",
            *offset + n,
            buf.len()
        );
        if n == 0 {
            return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
        }
        *offset += n;
        assert!(*offset <= buf.len());

        if *offset == buf.len() {
            return Poll::Ready(Ok(()));
        }
    }
}

impl<TSocket> NoiseSocket<TSocket>
where TSocket: AsyncRead + Unpin
{
    fn poll_read(&mut self, mut context: &mut Context, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        loop {
            trace!(target: LOG_TARGET, "NoiseSocket ReadState::{:?}", self.read_state);
            match self.read_state {
                ReadState::Init => {
                    self.read_state = ReadState::ReadFrameLen { buf: [0, 0], offset: 0 };
                },
                ReadState::ReadFrameLen {
                    ref mut buf,
                    ref mut offset,
                } => {
                    match ready!(poll_read_u16frame_len(
                        &mut context,
                        Pin::new(&mut self.socket),
                        buf,
                        offset
                    )) {
                        Ok(Some(frame_len)) => {
                            // Empty Frame
                            if frame_len == 0 {
                                self.read_state = ReadState::Init;
                            } else {
                                self.read_state = ReadState::ReadFrame { frame_len, offset: 0 };
                            }
                        },
                        Ok(None) => {
                            self.read_state = ReadState::Eof(Ok(()));
                        },
                        Err(e) => {
                            if e.kind() == io::ErrorKind::UnexpectedEof {
                                self.read_state = ReadState::Eof(Err(()));
                            }
                            return Poll::Ready(Err(e));
                        },
                    }
                },
                ReadState::ReadFrame {
                    frame_len,
                    ref mut offset,
                } => {
                    match ready!(poll_read_exact(
                        &mut context,
                        Pin::new(&mut self.socket),
                        &mut self.buffers.read_encrypted[..(frame_len as usize)],
                        offset
                    )) {
                        Ok(()) => {
                            match self.state.read_message(
                                &self.buffers.read_encrypted[..(frame_len as usize)],
                                &mut self.buffers.read_decrypted,
                            ) {
                                Ok(decrypted_len) => {
                                    self.read_state = ReadState::CopyDecryptedFrame {
                                        decrypted_len,
                                        offset: 0,
                                    };
                                },
                                Err(e) => {
                                    error!(target: LOG_TARGET, "Decryption Error: {}", e);
                                    self.read_state = ReadState::DecryptionError(e);
                                },
                            }
                        },
                        Err(e) => {
                            if e.kind() == io::ErrorKind::UnexpectedEof {
                                self.read_state = ReadState::Eof(Err(()));
                            }
                            return Poll::Ready(Err(e));
                        },
                    }
                },
                ReadState::CopyDecryptedFrame {
                    decrypted_len,
                    ref mut offset,
                } => {
                    let bytes_to_copy = ::std::cmp::min(decrypted_len as usize - *offset, buf.len());
                    buf[..bytes_to_copy]
                        .copy_from_slice(&self.buffers.read_decrypted[*offset..(*offset + bytes_to_copy)]);
                    trace!(
                        target: LOG_TARGET,
                        "CopyDecryptedFrame: copied {}/{} bytes",
                        *offset + bytes_to_copy,
                        decrypted_len
                    );
                    *offset += bytes_to_copy;
                    if *offset == decrypted_len as usize {
                        self.read_state = ReadState::Init;
                    }
                    return Poll::Ready(Ok(bytes_to_copy));
                },
                ReadState::Eof(Ok(())) => return Poll::Ready(Ok(0)),
                ReadState::Eof(Err(())) => return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into())),
                ReadState::DecryptionError(ref e) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("DecryptionError: {}", e),
                    )))
                },
            }
        }
    }
}

impl<TSocket> AsyncRead for NoiseSocket<TSocket>
where TSocket: AsyncRead + Unpin
{
    fn poll_read(self: Pin<&mut Self>, context: &mut Context, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        self.get_mut().poll_read(context, buf)
    }
}

impl<TSocket> NoiseSocket<TSocket>
where TSocket: AsyncWrite + Unpin
{
    fn poll_write_or_flush(
        &mut self,
        mut context: &mut Context,
        buf: Option<&[u8]>,
    ) -> Poll<io::Result<Option<usize>>>
    {
        loop {
            trace!(
                target: LOG_TARGET,
                "NoiseSocket {} WriteState::{:?}",
                if buf.is_some() { "poll_write" } else { "poll_flush" },
                self.write_state,
            );
            match self.write_state {
                WriteState::Init => {
                    if buf.is_some() {
                        self.write_state = WriteState::BufferData { offset: 0 };
                    } else {
                        return Poll::Ready(Ok(None));
                    }
                },
                WriteState::BufferData { ref mut offset } => {
                    let bytes_buffered = if let Some(buf) = buf {
                        let bytes_to_copy = ::std::cmp::min(MAX_WRITE_BUFFER_LENGTH - *offset, buf.len());
                        self.buffers.write_decrypted[*offset..(*offset + bytes_to_copy)]
                            .copy_from_slice(&buf[..bytes_to_copy]);
                        trace!(
                            target: LOG_TARGET,
                            "BufferData: buffered {}/{} bytes",
                            bytes_to_copy,
                            buf.len()
                        );
                        *offset += bytes_to_copy;
                        Some(bytes_to_copy)
                    } else {
                        None
                    };

                    if buf.is_none() || *offset == MAX_WRITE_BUFFER_LENGTH {
                        match self.state.write_message(
                            &self.buffers.write_decrypted[..*offset],
                            &mut self.buffers.write_encrypted,
                        ) {
                            Ok(encrypted_len) => {
                                let frame_len = encrypted_len.try_into().expect("offset should be able to fit in u16");
                                self.write_state = WriteState::WriteFrameLen {
                                    frame_len,
                                    buf: u16::to_be_bytes(frame_len),
                                    offset: 0,
                                };
                            },
                            Err(e) => {
                                error!(target: LOG_TARGET, "Encryption Error: {}", e);
                                let err = io::Error::new(io::ErrorKind::InvalidData, format!("EncryptionError: {}", e));
                                self.write_state = WriteState::EncryptionError(e);
                                return Poll::Ready(Err(err));
                            },
                        }
                    }

                    if let Some(bytes_buffered) = bytes_buffered {
                        return Poll::Ready(Ok(Some(bytes_buffered)));
                    }
                },
                WriteState::WriteFrameLen {
                    frame_len,
                    ref buf,
                    ref mut offset,
                } => match ready!(poll_write_all(&mut context, Pin::new(&mut self.socket), buf, offset)) {
                    Ok(()) => {
                        self.write_state = WriteState::WriteEncryptedFrame { frame_len, offset: 0 };
                    },
                    Err(e) => {
                        if e.kind() == io::ErrorKind::WriteZero {
                            self.write_state = WriteState::Eof;
                        }
                        return Poll::Ready(Err(e));
                    },
                },
                WriteState::WriteEncryptedFrame {
                    frame_len,
                    ref mut offset,
                } => {
                    match ready!(poll_write_all(
                        &mut context,
                        Pin::new(&mut self.socket),
                        &self.buffers.write_encrypted[..(frame_len as usize)],
                        offset
                    )) {
                        Ok(()) => {
                            self.write_state = WriteState::Flush;
                        },
                        Err(e) => {
                            if e.kind() == io::ErrorKind::WriteZero {
                                self.write_state = WriteState::Eof;
                            }
                            return Poll::Ready(Err(e));
                        },
                    }
                },
                WriteState::Flush => {
                    ready!(Pin::new(&mut self.socket).poll_flush(&mut context))?;
                    self.write_state = WriteState::Init;
                },
                WriteState::Eof => return Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                WriteState::EncryptionError(ref e) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("EncryptionError: {}", e),
                    )))
                },
            }
        }
    }

    fn poll_write(&mut self, context: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        if let Some(bytes_written) = ready!(self.poll_write_or_flush(context, Some(buf)))? {
            Poll::Ready(Ok(bytes_written))
        } else {
            unreachable!();
        }
    }

    fn poll_flush(&mut self, context: &mut Context) -> Poll<io::Result<()>> {
        if ready!(self.poll_write_or_flush(context, None))?.is_none() {
            Poll::Ready(Ok(()))
        } else {
            unreachable!();
        }
    }
}

impl<TSocket> AsyncWrite for NoiseSocket<TSocket>
where TSocket: AsyncWrite + Unpin
{
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.get_mut().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.get_mut().poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.socket).poll_close(cx)
    }
}

pub struct Handshake<TSocket> {
    socket: NoiseSocket<TSocket>,
}

impl<TSocket> Handshake<TSocket> {
    pub fn new(socket: TSocket, state: HandshakeState) -> Self {
        Self {
            socket: NoiseSocket::new(socket, state.into()),
        }
    }
}

impl<TSocket> Handshake<TSocket>
where TSocket: AsyncRead + AsyncWrite + Unpin
{
    /// Perform a Single Round-Trip noise IX handshake returning the underlying [NoiseSocket]
    /// (switched to transport mode) upon success.
    pub async fn handshake_1rt(mut self) -> io::Result<NoiseSocket<TSocket>> {
        // The Dialer
        if self.socket.state.is_initiator() {
            // -> e, s
            self.send().await?;
            self.flush().await?;

            // <- e, ee, se, s, es
            self.receive().await?;
        } else {
            // -> e, s
            self.receive().await?;

            // <- e, ee, se, s, es
            self.send().await?;
            self.flush().await?;
        }

        self.finish()
    }

    async fn send(&mut self) -> io::Result<usize> {
        self.socket.write(&[]).await
    }

    async fn flush(&mut self) -> io::Result<()> {
        self.socket.flush().await.map_err(Into::into)
    }

    async fn receive(&mut self) -> io::Result<usize> {
        self.socket.read(&mut []).await
    }

    fn finish(self) -> io::Result<NoiseSocket<TSocket>> {
        let transport_state = self
            .socket
            .state
            .into_transport_mode()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("Invalid snow state: {}", err)))?;

        Ok(NoiseSocket {
            state: transport_state,
            ..self.socket
        })
    }
}

#[derive(Debug)]
enum NoiseState {
    HandshakeState(HandshakeState),
    TransportState(TransportState),
}

macro_rules! proxy_state_method {
    (pub fn $name:ident(&mut self$(,)? $($arg_name:ident : $arg_type:ty),*) -> $ret:ty) => {
        pub fn $name(&mut self, $($arg_name:$arg_type),*) -> $ret {
            match self {
                NoiseState::HandshakeState(state) => state.$name($($arg_name),*),
                NoiseState::TransportState(state) => state.$name($($arg_name),*),
            }
        }
    };
     (pub fn $name:ident(&self$(,)? $($arg_name:ident : $arg_type:ty),*) -> $ret:ty) => {
        pub fn $name(&self, $($arg_name:$arg_type),*) -> $ret {
            match self {
                NoiseState::HandshakeState(state) => state.$name($($arg_name),*),
                NoiseState::TransportState(state) => state.$name($($arg_name),*),
            }
        }
    }
}

impl NoiseState {
    proxy_state_method!(pub fn write_message(&mut self, message: &[u8], payload: &mut [u8]) -> Result<usize, snow::Error>);

    proxy_state_method!(pub fn is_initiator(&self) -> bool);

    proxy_state_method!(pub fn read_message(&mut self, message: &[u8], payload: &mut [u8]) -> Result<usize, snow::Error>);

    proxy_state_method!(pub fn get_remote_static(&self) -> Option<&[u8]>);

    pub fn into_transport_mode(self) -> Result<Self, snow::Error> {
        match self {
            NoiseState::HandshakeState(state) => Ok(NoiseState::TransportState(state.into_transport_mode()?)),
            _ => Err(snow::Error::State(StateProblem::HandshakeAlreadyFinished)),
        }
    }
}

impl From<HandshakeState> for NoiseState {
    fn from(state: HandshakeState) -> Self {
        NoiseState::HandshakeState(state)
    }
}

impl From<TransportState> for NoiseState {
    fn from(state: TransportState) -> Self {
        NoiseState::TransportState(state)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        noise::config::NOISE_IX_PARAMETER,
        test_utils::tcp::build_connected_tcp_socket_pair,
        transports::TcpSocket,
    };
    use futures::future::join;
    use snow::{params::NoiseParams, Builder, Error, Keypair};
    use std::io;
    use tokio::runtime::Runtime;

    async fn build_test_connection() -> Result<((Keypair, Handshake<TcpSocket>), (Keypair, Handshake<TcpSocket>)), Error>
    {
        let parameters: NoiseParams = NOISE_IX_PARAMETER.parse().expect("Invalid protocol name");

        let dialer_keypair = Builder::new(parameters.clone()).generate_keypair()?;
        let listener_keypair = Builder::new(parameters.clone()).generate_keypair()?;

        let dialer_session = Builder::new(parameters.clone())
            .local_private_key(&dialer_keypair.private)
            .build_initiator()?;
        let listener_session = Builder::new(parameters.clone())
            .local_private_key(&listener_keypair.private)
            .build_responder()?;

        let (dialer_socket, listener_socket) = build_connected_tcp_socket_pair().await;
        let (dialer, listener) = (
            NoiseSocket::new(dialer_socket, dialer_session.into()),
            NoiseSocket::new(listener_socket, listener_session.into()),
        );

        Ok((
            (dialer_keypair, Handshake { socket: dialer }),
            (listener_keypair, Handshake { socket: listener }),
        ))
    }

    async fn perform_handshake(
        dialer: Handshake<TcpSocket>,
        listener: Handshake<TcpSocket>,
    ) -> io::Result<(NoiseSocket<TcpSocket>, NoiseSocket<TcpSocket>)>
    {
        let (dialer_result, listener_result) = join(dialer.handshake_1rt(), listener.handshake_1rt()).await;

        Ok((dialer_result?, listener_result?))
    }

    #[tokio::test]
    async fn test_handshake() {
        let ((dialer_keypair, dialer), (listener_keypair, listener)) = build_test_connection().await.unwrap();

        let (dialer_socket, listener_socket) = perform_handshake(dialer, listener).await.unwrap();

        assert_eq!(
            dialer_socket.get_remote_static(),
            Some(listener_keypair.public.as_ref())
        );
        assert_eq!(
            listener_socket.get_remote_static(),
            Some(dialer_keypair.public.as_ref())
        );
    }

    #[tokio::test]
    async fn simple_test() -> io::Result<()> {
        let ((_dialer_keypair, dialer), (_listener_keypair, listener)) = build_test_connection().await.unwrap();

        let (mut dialer_socket, mut listener_socket) = perform_handshake(dialer, listener).await?;

        dialer_socket.write_all(b"stormlight").await?;
        dialer_socket.write_all(b" ").await?;
        dialer_socket.write_all(b"archive").await?;
        dialer_socket.flush().await?;
        dialer_socket.close().await?;

        let mut buf = Vec::new();
        listener_socket.read_to_end(&mut buf).await?;

        assert_eq!(buf, b"stormlight archive");

        Ok(())
    }

    #[tokio::test]
    async fn interleaved_writes() -> io::Result<()> {
        let ((_dialer_keypair, dialer), (_listener_keypair, listener)) = build_test_connection().await.unwrap();

        let (mut a, mut b) = perform_handshake(dialer, listener).await?;

        a.write_all(b"The Name of the Wind").await?;
        a.flush().await?;
        a.write_all(b"The Wise Man's Fear").await?;
        a.flush().await?;

        b.write_all(b"The Doors of Stone").await?;
        b.flush().await?;

        let mut buf = [0; 20];
        b.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"The Name of the Wind");
        let mut buf = [0; 19];
        b.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"The Wise Man's Fear");

        let mut buf = [0; 18];
        a.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"The Doors of Stone");

        Ok(())
    }

    #[test]
    fn u16_max_writes() -> io::Result<()> {
        // Current thread runtime stack overflows, so the full tokio runtime is used here
        let rt = Runtime::new().unwrap();
        rt.block_on(async move {
            let ((_dialer_keypair, dialer), (_listener_keypair, listener)) = build_test_connection().await.unwrap();

            let (mut a, mut b) = perform_handshake(dialer, listener).await?;

            let buf_send = [1; MAX_PAYLOAD_LENGTH];
            a.write_all(&buf_send).await?;
            a.flush().await?;

            let mut buf_receive = [0; MAX_PAYLOAD_LENGTH];
            b.read_exact(&mut buf_receive).await?;
            assert_eq!(&buf_receive[..], &buf_send[..]);

            Ok(())
        })
    }
}
