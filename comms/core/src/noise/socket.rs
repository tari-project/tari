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

use std::{
    cmp,
    convert::TryInto,
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::ready;
use log::*;
use snow::{HandshakeState, TransportState, error::StateProblem};
use tari_utilities::ByteArray;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
    time,
};

use crate::types::CommsPublicKey;

const LOG_TARGET: &str = "comms::noise::socket";

const MAX_PAYLOAD_LENGTH: usize = u16::MAX as usize; // 65535

// The maximum number of bytes that we can buffer is 16 bytes less than u16::max_value() because
// encrypted messages include a tag along with the payload.
const MAX_WRITE_BUFFER_LENGTH: usize = u16::MAX as usize - 16; // 65519

/// Size of the read-ahead buffer that holds raw ciphertext read from the wire. It is large enough to hold
/// at least one maximum-sized frame (2 byte length prefix + 65535 bytes) plus more, so a single socket
/// read can deliver several frames.
const READ_AHEAD_LENGTH: usize = 128 * 1024;

/// Collection of buffers used for buffering data during the various read/write states of a
/// NoiseSocket
struct NoiseBuffers {
    /// Raw ciphertext read ahead from the wire. Only the bytes in `read_start..read_end` are unconsumed.
    /// This only ever holds ciphertext; frames are decrypted with whichever noise state is current when
    /// they are parsed, so bytes read during the handshake can be transport frames.
    read_ahead: Box<[u8]>,
    /// Start of the unconsumed bytes in `read_ahead`
    read_start: usize,
    /// End of the unconsumed bytes in `read_ahead`
    read_end: usize,
    /// Decrypted data read from the wire (produced by having snow decrypt a frame from the
    /// `read_ahead` buffer)
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
            read_ahead: vec![0; READ_AHEAD_LENGTH].into_boxed_slice(),
            read_start: 0,
            read_end: 0,
            read_decrypted: [0; MAX_PAYLOAD_LENGTH],
            write_decrypted: [0; MAX_WRITE_BUFFER_LENGTH],
            write_encrypted: [0; MAX_PAYLOAD_LENGTH],
        }
    }

    /// Number of read-ahead bytes that have not been consumed yet
    fn read_available(&self) -> usize {
        self.read_end.saturating_sub(self.read_start)
    }

    /// The unconsumed read-ahead bytes
    fn read_unconsumed(&self) -> &[u8] {
        self.read_ahead.get(self.read_start..self.read_end).unwrap_or(&[])
    }

    /// Mark `n` read-ahead bytes as consumed
    fn read_consume(&mut self, n: usize) {
        self.read_start = cmp::min(self.read_start.saturating_add(n), self.read_end);
        if self.read_start == self.read_end {
            self.read_start = 0;
            self.read_end = 0;
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
    ReadFrameLen,
    /// Read encrypted frame
    ReadFrame { frame_len: u16 },
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

    /// Get the raw remote static key
    pub fn get_remote_static(&self) -> Option<&[u8]> {
        self.state.get_remote_static()
    }

    /// Get the remote static key as a CommsPublicKey
    pub fn get_remote_public_key(&self) -> Option<CommsPublicKey> {
        self.get_remote_static()
            .and_then(|s| CommsPublicKey::from_canonical_bytes(s).ok())
    }

    /// Returns true if the remote closed the connection, i.e. a read reached the end of the stream.
    fn is_eof(&self) -> bool {
        matches!(self.read_state, ReadState::Eof(_))
    }
}

fn poll_write_all<TSocket>(
    context: &mut Context,
    mut socket: Pin<&mut TSocket>,
    buf: &[u8],
    offset: &mut usize,
) -> Poll<io::Result<()>>
where
    TSocket: AsyncWrite,
{
    loop {
        let bytes = match buf.get(*offset..) {
            Some(bytes) => bytes,
            None => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Offset exceeds buffer length",
                )));
            },
        };
        let n = ready!(socket.as_mut().poll_write(context, bytes))?;
        trace!(
            target: LOG_TARGET,
            "poll_write_all: wrote {}/{} bytes",
            offset.saturating_add(n),
            buf.len()
        );
        if n == 0 {
            return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
        }
        *offset = offset.saturating_add(n);
        assert!(*offset <= buf.len());

        if *offset == buf.len() {
            return Poll::Ready(Ok(()));
        }
    }
}

/// Read more ciphertext from `socket` into the read-ahead buffer so that it can hold at least `needed`
/// unconsumed bytes. A single read fills as much free space as the socket provides.
///
/// Returns the number of bytes read, where 0 means EOF.
fn poll_fill_read_ahead<TSocket>(
    context: &mut Context,
    socket: Pin<&mut TSocket>,
    buffers: &mut NoiseBuffers,
    needed: usize,
) -> Poll<io::Result<usize>>
where
    TSocket: AsyncRead,
{
    // Move the unconsumed bytes to the front if the rest of the frame would not fit after them
    if buffers.read_start.saturating_add(needed) > buffers.read_ahead.len() {
        buffers.read_ahead.copy_within(buffers.read_start..buffers.read_end, 0);
        buffers.read_end = buffers.read_available();
        buffers.read_start = 0;
    }
    let free = match buffers.read_ahead.get_mut(buffers.read_end..) {
        Some(free) if !free.is_empty() => free,
        _ => {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "read-ahead buffer is full",
            )));
        },
    };
    let mut read_buf = ReadBuf::new(free);
    ready!(socket.poll_read(context, &mut read_buf))?;
    let n = read_buf.filled().len();
    buffers.read_end = buffers.read_end.saturating_add(n);
    trace!(
        target: LOG_TARGET,
        "poll_fill_read_ahead: read {} bytes, {} bytes buffered",
        n,
        buffers.read_available()
    );
    Poll::Ready(Ok(n))
}

impl<TSocket> NoiseSocket<TSocket>
where TSocket: AsyncRead + Unpin
{
    #[allow(clippy::too_many_lines)]
    fn poll_read(&mut self, context: &mut Context, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        loop {
            trace!(target: LOG_TARGET, "NoiseSocket ReadState::{:?}", self.read_state);
            match self.read_state {
                ReadState::Init => {
                    self.read_state = ReadState::ReadFrameLen;
                },
                ReadState::ReadFrameLen => {
                    let len_bytes = self.buffers.read_unconsumed().get(..2).and_then(|b| b.try_into().ok());
                    if let Some(len_bytes) = len_bytes {
                        let frame_len = u16::from_be_bytes(len_bytes);
                        self.buffers.read_consume(2);
                        // Empty Frame
                        if frame_len == 0 {
                            self.read_state = ReadState::Init;
                        } else {
                            self.read_state = ReadState::ReadFrame { frame_len };
                        }
                        continue;
                    }
                    let n = ready!(poll_fill_read_ahead(
                        context,
                        Pin::new(&mut self.socket),
                        &mut self.buffers,
                        2
                    ))?;
                    if n == 0 {
                        // EOF at a frame boundary is a graceful shutdown, EOF inside the length prefix is not
                        if self.buffers.read_available() == 0 {
                            self.read_state = ReadState::Eof(Ok(()));
                        } else {
                            self.read_state = ReadState::Eof(Err(()));
                            return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
                        }
                    }
                },
                ReadState::ReadFrame { frame_len } => {
                    let frame_len = usize::from(frame_len);
                    if self.buffers.read_available() >= frame_len {
                        let NoiseBuffers {
                            read_ahead,
                            read_start,
                            read_decrypted,
                            ..
                        } = &mut *self.buffers;
                        let frame = read_ahead
                            .get(*read_start..read_start.saturating_add(frame_len))
                            .expect("this is checked");
                        let result = self.state.read_message(frame, read_decrypted);
                        self.buffers.read_consume(frame_len);
                        match result {
                            Ok(decrypted_len) => {
                                self.read_state = ReadState::CopyDecryptedFrame {
                                    decrypted_len,
                                    offset: 0,
                                };
                            },
                            Err(e) => {
                                warn!(target: LOG_TARGET, "Decryption Error: {e}");
                                self.read_state = ReadState::DecryptionError(e);
                            },
                        }
                        continue;
                    }
                    let n = ready!(poll_fill_read_ahead(
                        context,
                        Pin::new(&mut self.socket),
                        &mut self.buffers,
                        frame_len
                    ))?;
                    if n == 0 {
                        // EOF inside a frame
                        self.read_state = ReadState::Eof(Err(()));
                        return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
                    }
                },
                ReadState::CopyDecryptedFrame {
                    decrypted_len,
                    ref mut offset,
                } => {
                    let num_bytes_to_copy = cmp::min(decrypted_len.saturating_sub(*offset), buf.len());
                    let copy_end = offset.saturating_add(num_bytes_to_copy);
                    let bytes_to_copy = match self.buffers.read_decrypted.get(*offset..copy_end) {
                        Some(bytes) => bytes,
                        None => {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "Offset exceeds buffer length",
                            )));
                        },
                    };
                    buf.get_mut(..num_bytes_to_copy)
                        .expect("this is checked")
                        .copy_from_slice(bytes_to_copy);
                    trace!(
                        target: LOG_TARGET,
                        "CopyDecryptedFrame: copied {}/{} bytes",
                        copy_end,
                        decrypted_len
                    );
                    *offset = copy_end;
                    if *offset == decrypted_len {
                        self.read_state = ReadState::Init;
                    }
                    return Poll::Ready(Ok(num_bytes_to_copy));
                },
                ReadState::Eof(Ok(())) => return Poll::Ready(Ok(0)),
                ReadState::Eof(Err(())) => return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into())),
                ReadState::DecryptionError(ref e) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("DecryptionError: {e}"),
                    )));
                },
            }
        }
    }
}

impl<TSocket> AsyncRead for NoiseSocket<TSocket>
where TSocket: AsyncRead + Unpin
{
    fn poll_read(self: Pin<&mut Self>, context: &mut Context, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let slice = buf.initialize_unfilled();
        let n = futures::ready!(self.get_mut().poll_read(context, slice))?;
        buf.advance(n);
        Poll::Ready(Ok(()))
    }
}

impl<TSocket> NoiseSocket<TSocket>
where TSocket: AsyncWrite + Unpin
{
    #[allow(clippy::too_many_lines)]
    fn poll_write_or_flush(&mut self, context: &mut Context, buf: Option<&[u8]>) -> Poll<io::Result<Option<usize>>> {
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
                        let num_bytes_to_copy =
                            ::std::cmp::min(MAX_WRITE_BUFFER_LENGTH.saturating_sub(*offset), buf.len());
                        let bytes = match buf.get(..num_bytes_to_copy) {
                            Some(bytes) => bytes,
                            None => {
                                return Poll::Ready(Err(io::Error::new(
                                    io::ErrorKind::InvalidInput,
                                    "frame length exceeds buffer length",
                                )));
                            },
                        };
                        self.buffers
                            .write_decrypted
                            .get_mut(*offset..offset.saturating_add(num_bytes_to_copy))
                            .expect("this is checked")
                            .copy_from_slice(bytes);
                        trace!(
                            target: LOG_TARGET,
                            "BufferData: buffered {}/{} bytes",
                            num_bytes_to_copy,
                            buf.len()
                        );
                        *offset = offset.saturating_add(num_bytes_to_copy);
                        Some(num_bytes_to_copy)
                    } else {
                        None
                    };

                    if buf.is_none() || *offset == MAX_WRITE_BUFFER_LENGTH {
                        let bytes = match self.buffers.write_decrypted.get(..*offset) {
                            Some(bytes) => bytes,
                            None => {
                                return Poll::Ready(Err(io::Error::new(
                                    io::ErrorKind::InvalidInput,
                                    "frame length exceeds buffer length",
                                )));
                            },
                        };
                        match self.state.write_message(bytes, &mut self.buffers.write_encrypted) {
                            Ok(encrypted_len) => {
                                let frame_len = encrypted_len
                                    .try_into()
                                    .map_err(|_| io::Error::other("offset should be able to fit in u16"))?;
                                self.write_state = WriteState::WriteFrameLen {
                                    frame_len,
                                    buf: u16::to_be_bytes(frame_len),
                                    offset: 0,
                                };
                            },
                            Err(e) => {
                                warn!(target: LOG_TARGET, "Encryption Error: {e}");
                                let err = io::Error::new(io::ErrorKind::InvalidData, format!("EncryptionError: {e}"));
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
                } => match ready!(poll_write_all(context, Pin::new(&mut self.socket), buf, offset)) {
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
                    let bytes = match self.buffers.write_encrypted.get(..(frame_len as usize)) {
                        Some(bytes) => bytes,
                        None => {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "frame length exceeds buffer length",
                            )));
                        },
                    };
                    match ready!(poll_write_all(context, Pin::new(&mut self.socket), bytes, offset)) {
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
                    ready!(Pin::new(&mut self.socket).poll_flush(context))?;
                    self.write_state = WriteState::Init;
                },
                WriteState::Eof => return Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                WriteState::EncryptionError(ref e) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("EncryptionError: {e}"),
                    )));
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

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.socket).poll_shutdown(cx)
    }
}

pub struct Handshake<TSocket> {
    socket: NoiseSocket<TSocket>,
    recv_timeout: Duration,
}

impl<TSocket> Handshake<TSocket> {
    pub fn new(socket: TSocket, state: HandshakeState, recv_timeout: Duration) -> Self {
        Self {
            socket: NoiseSocket::new(socket, state.into()),
            recv_timeout,
        }
    }
}

impl<TSocket> Handshake<TSocket>
where TSocket: AsyncRead + AsyncWrite + Unpin
{
    /// Perform a Single Round-Trip noise IX handshake returning the underlying [NoiseSocket]
    /// (switched to transport mode) upon success.
    pub async fn perform_handshake(mut self) -> io::Result<NoiseSocket<TSocket>> {
        match self.handshake_1_5rtt().await {
            Ok(_) => self.build(),
            Err(err) => {
                info!(
                    target: LOG_TARGET,
                    "Noise handshake failed because '{err:?}'. Closing socket."
                );
                self.socket.shutdown().await?;
                Err(err)
            },
        }
    }

    /// Performs a 1.5 RTT handshake. For example, the noise XX handshake.
    async fn handshake_1_5rtt(&mut self) -> io::Result<()> {
        if self.socket.state.is_initiator() {
            //   -> e
            self.send().await?;
            self.flush().await?;

            // <- e, ee, s, es
            self.receive().await?;

            //   -> s, se
            self.send().await?;
            self.flush().await?;
        } else {
            //   -> e
            self.receive().await?;

            // <- e, ee, s, es
            self.send().await?;
            self.flush().await?;

            //   -> s, se
            self.receive().await?;
        }

        Ok(())
    }

    async fn send(&mut self) -> io::Result<usize> {
        self.socket.write(&[]).await
    }

    async fn flush(&mut self) -> io::Result<()> {
        self.socket.flush().await
    }

    async fn receive(&mut self) -> io::Result<usize> {
        let num_bytes = time::timeout(self.recv_timeout, self.socket.read(&mut []))
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;

        // Handshake messages carry no payload, so the read above is issued with an empty buffer and
        // returns Ok(0) both when a handshake message was consumed and when the remote hung up
        // before sending one. Left undetected, the EOF case lets the handshake continue and fail on
        // the next `send` with a confusing snow state error ("NotTurnToWrite") rather than
        // reporting that the peer closed the connection.
        if self.socket.is_eof() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "peer closed the connection during the noise handshake",
            ));
        }

        Ok(num_bytes)
    }

    fn build(self) -> io::Result<NoiseSocket<TSocket>> {
        let transport_state = self
            .socket
            .state
            .into_transport_mode()
            .map_err(|err| io::Error::other(format!("Invalid snow state: {err}")))?;

        Ok(NoiseSocket {
            state: transport_state,
            ..self.socket
        })
    }
}

#[derive(Debug)]
enum NoiseState {
    HandshakeState(Box<HandshakeState>),
    TransportState(Box<TransportState>),
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
            NoiseState::HandshakeState(state) => Ok(NoiseState::TransportState(Box::new(state.into_transport_mode()?))),
            _ => Err(snow::Error::State(StateProblem::HandshakeAlreadyFinished)),
        }
    }
}

impl From<HandshakeState> for NoiseState {
    fn from(state: HandshakeState) -> Self {
        NoiseState::HandshakeState(Box::new(state))
    }
}

impl From<TransportState> for NoiseState {
    fn from(state: TransportState) -> Self {
        NoiseState::TransportState(Box::new(state))
    }
}

#[cfg(test)]
mod test {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use futures::future::join;
    use snow::{Builder, Error, Keypair, params::NoiseParams};
    use tokio::sync::mpsc;

    use super::*;
    use crate::{memsocket::MemorySocket, noise::config::NOISE_PARAMETERS};

    async fn build_test_connection()
    -> Result<((Keypair, Handshake<MemorySocket>), (Keypair, Handshake<MemorySocket>)), Error> {
        let parameters: NoiseParams = NOISE_PARAMETERS.parse().expect("Invalid protocol name");

        let dialer_keypair = Builder::new(parameters.clone()).generate_keypair()?;
        let listener_keypair = Builder::new(parameters.clone()).generate_keypair()?;

        let dialer_session = Builder::new(parameters.clone())
            .local_private_key(&dialer_keypair.private)
            .build_initiator()?;
        let listener_session = Builder::new(parameters)
            .local_private_key(&listener_keypair.private)
            .build_responder()?;

        let (dialer_socket, listener_socket) = MemorySocket::new_pair();
        let (dialer, listener) = (
            NoiseSocket::new(dialer_socket, dialer_session.into()),
            NoiseSocket::new(listener_socket, listener_session.into()),
        );

        Ok((
            (dialer_keypair, Handshake {
                socket: dialer,
                recv_timeout: Duration::from_secs(1),
            }),
            (listener_keypair, Handshake {
                socket: listener,
                recv_timeout: Duration::from_secs(1),
            }),
        ))
    }

    async fn perform_handshake(
        dialer: Handshake<MemorySocket>,
        listener: Handshake<MemorySocket>,
    ) -> io::Result<(NoiseSocket<MemorySocket>, NoiseSocket<MemorySocket>)> {
        let (dialer_result, listener_result) = join(dialer.perform_handshake(), listener.perform_handshake()).await;

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
    async fn handshake_reports_eof_when_peer_hangs_up() {
        let ((_dialer_keypair, dialer), (_listener_keypair, mut listener)) = build_test_connection().await.unwrap();

        // The peer reads our first handshake message and then hangs up without replying.
        let listener_task = tokio::spawn(async move {
            listener.receive().await.unwrap();
            drop(listener);
        });

        let err = dialer.perform_handshake().await.unwrap_err();
        listener_task.await.unwrap();

        // Before the EOF was detected explicitly this surfaced as an InvalidData "EncryptionError:
        // state error: NotTurnToWrite" from snow, which says nothing about the peer hanging up.
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "unexpected error: {err}");
    }

    #[tokio::test]
    async fn simple_test() -> io::Result<()> {
        let ((_dialer_keypair, dialer), (_listener_keypair, listener)) = build_test_connection().await.unwrap();

        let (mut dialer_socket, mut listener_socket) = perform_handshake(dialer, listener).await?;

        dialer_socket.write_all(b"stormlight").await?;
        dialer_socket.write_all(b" ").await?;
        dialer_socket.write_all(b"archive").await?;
        dialer_socket.flush().await?;
        dialer_socket.shutdown().await?;

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

    #[tokio::test]
    async fn u16_max_writes() -> io::Result<()> {
        let ((_dialer_keypair, dialer), (_listener_keypair, listener)) = build_test_connection().await.unwrap();

        let (mut a, mut b) = perform_handshake(dialer, listener).await?;

        let buf_send = &[1; MAX_PAYLOAD_LENGTH + 1];
        a.write_all(buf_send).await?;
        a.flush().await?;

        let mut buf_receive = vec![0; MAX_PAYLOAD_LENGTH + 1];
        b.read_exact(&mut buf_receive).await?;
        assert_eq!(&buf_receive[..], &buf_send[..]);

        Ok(())
    }

    #[tokio::test]
    async fn larger_writes() -> io::Result<()> {
        let ((_dialer_keypair, dialer), (_listener_keypair, listener)) = build_test_connection().await.unwrap();

        let (mut a, mut b) = perform_handshake(dialer, listener).await?;

        let buf_send = &[1; MAX_PAYLOAD_LENGTH * 2 + 1024];
        a.write_all(buf_send).await?;
        a.flush().await?;

        let mut buf_receive = vec![0; MAX_PAYLOAD_LENGTH * 2 + 1024];
        b.read_exact(&mut buf_receive).await?;
        assert_eq!(&buf_receive[..], &buf_send[..]);

        Ok(())
    }

    #[tokio::test]
    async fn unexpected_eof() -> io::Result<()> {
        let ((_dialer_keypair, dialer), (_listener_keypair, listener)) = build_test_connection().await.unwrap();

        let (mut a, mut b) = perform_handshake(dialer, listener).await?;

        let buf_send = &[1; MAX_PAYLOAD_LENGTH];
        a.write_all(buf_send).await?;
        a.flush().await?;

        a.socket.shutdown().await.unwrap();
        drop(a);

        let mut buf_receive = vec![0; MAX_PAYLOAD_LENGTH];
        b.read_exact(&mut buf_receive).await.unwrap();
        assert_eq!(&buf_receive[..], &buf_send[..]);

        let err = b.read_exact(&mut buf_receive).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);

        Ok(())
    }

    /// A scripted socket. Each chunk sent on the inbound channel is delivered by exactly one `poll_read` (as long
    /// as the read buffer is large enough), and closing the channel is EOF. Every write is sent on the outbound
    /// channel. `reads` counts the reads that returned data.
    struct MockSocket {
        inbound: mpsc::UnboundedReceiver<Vec<u8>>,
        pending: Vec<u8>,
        outbound: mpsc::UnboundedSender<Vec<u8>>,
        reads: Arc<AtomicUsize>,
    }

    /// The test's end of a [MockSocket]
    struct MockPeer {
        inbound: mpsc::UnboundedSender<Vec<u8>>,
        outbound: mpsc::UnboundedReceiver<Vec<u8>>,
        reads: Arc<AtomicUsize>,
    }

    impl MockSocket {
        fn new() -> (Self, MockPeer) {
            let (in_tx, in_rx) = mpsc::unbounded_channel();
            let (out_tx, out_rx) = mpsc::unbounded_channel();
            let reads = Arc::new(AtomicUsize::new(0));
            let socket = Self {
                inbound: in_rx,
                pending: Vec::new(),
                outbound: out_tx,
                reads: reads.clone(),
            };
            let peer = MockPeer {
                inbound: in_tx,
                outbound: out_rx,
                reads,
            };
            (socket, peer)
        }
    }

    impl AsyncRead for MockSocket {
        fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            if self.pending.is_empty() {
                match ready!(self.inbound.poll_recv(cx)) {
                    Some(chunk) => self.pending = chunk,
                    None => return Poll::Ready(Ok(())),
                }
            }
            let n = cmp::min(self.pending.len(), buf.remaining());
            let chunk: Vec<u8> = self.pending.drain(..n).collect();
            buf.put_slice(&chunk);
            self.reads.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for MockSocket {
        fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            let _ignore = self.outbound.send(buf.to_vec());
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    fn handshake_states() -> (HandshakeState, HandshakeState) {
        let parameters: NoiseParams = NOISE_PARAMETERS.parse().unwrap();
        let initiator_keypair = Builder::new(parameters.clone()).generate_keypair().unwrap();
        let responder_keypair = Builder::new(parameters.clone()).generate_keypair().unwrap();
        let initiator = Builder::new(parameters.clone())
            .local_private_key(&initiator_keypair.private)
            .build_initiator()
            .unwrap();
        let responder = Builder::new(parameters)
            .local_private_key(&responder_keypair.private)
            .build_responder()
            .unwrap();
        (initiator, responder)
    }

    /// Encrypt `message` with `state` and wrap it in a u16 big endian length prefix, as it appears on the wire
    fn wire_frame(state: &mut NoiseState, message: &[u8]) -> Vec<u8> {
        let mut encrypted = vec![0u8; MAX_PAYLOAD_LENGTH];
        let len = state.write_message(message, &mut encrypted).unwrap();
        encrypted.truncate(len);
        let mut frame = u16::try_from(len).unwrap().to_be_bytes().to_vec();
        frame.extend(encrypted);
        frame
    }

    /// Run a complete XX handshake in memory and return the (sender, receiver) transport states
    fn transport_states() -> (NoiseState, NoiseState) {
        let (initiator, responder) = handshake_states();
        let mut initiator = NoiseState::from(initiator);
        let mut responder = NoiseState::from(responder);
        let mut payload = vec![0u8; MAX_PAYLOAD_LENGTH];
        for (sender, receiver) in [(0, 1), (1, 0), (0, 1)] {
            let mut states = [&mut initiator, &mut responder];
            let frame = wire_frame(states.get_mut(sender).unwrap(), &[]);
            let message = frame.get(2..).unwrap();
            states
                .get_mut(receiver)
                .unwrap()
                .read_message(message, &mut payload)
                .unwrap();
        }
        (
            initiator.into_transport_mode().unwrap(),
            responder.into_transport_mode().unwrap(),
        )
    }

    #[tokio::test]
    async fn several_frames_in_one_read_are_decoded_in_order() {
        let (mut sender, receiver) = transport_states();
        let (
            mock,
            MockPeer {
                inbound: in_tx,
                outbound: _out_rx,
                reads,
            },
        ) = MockSocket::new();
        let mut socket = NoiseSocket::new(mock, receiver);

        let mut chunk = Vec::new();
        for message in [&b"first "[..], b"second ", b"third"] {
            chunk.extend(wire_frame(&mut sender, message));
        }
        in_tx.send(chunk).unwrap();
        drop(in_tx);

        let mut buf = Vec::new();
        socket.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"first second third");
        // All three frames came from a single underlying read
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn frames_delivered_one_byte_at_a_time() {
        let (mut sender, receiver) = transport_states();
        let (
            mock,
            MockPeer {
                inbound: in_tx,
                outbound: _out_rx,
                reads,
            },
        ) = MockSocket::new();
        let mut socket = NoiseSocket::new(mock, receiver);

        let mut bytes = wire_frame(&mut sender, b"one byte");
        bytes.extend(wire_frame(&mut sender, b" at a time"));
        // Every byte arrives in its own read, which splits both 2-byte length prefixes
        for byte in &bytes {
            in_tx.send(vec![*byte]).unwrap();
        }
        drop(in_tx);

        let mut buf = Vec::new();
        socket.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"one byte at a time");
        assert_eq!(reads.load(Ordering::SeqCst), bytes.len());
    }

    #[tokio::test]
    async fn max_size_frames_larger_than_the_read_ahead_buffer() {
        let (mut sender, receiver) = transport_states();
        let (
            mock,
            MockPeer {
                inbound: in_tx,
                outbound: _out_rx,
                reads: _reads,
            },
        ) = MockSocket::new();
        let mut socket = NoiseSocket::new(mock, receiver);

        // Three maximum size frames in one chunk do not fit in the read-ahead buffer, so it has to be compacted
        let mut chunk = Vec::new();
        let mut expected = Vec::new();
        for value in 1..=3u8 {
            let message = vec![value; MAX_WRITE_BUFFER_LENGTH];
            chunk.extend(wire_frame(&mut sender, &message));
            expected.extend(message);
        }
        assert!(chunk.len() > READ_AHEAD_LENGTH);
        in_tx.send(chunk).unwrap();
        drop(in_tx);

        let mut buf = Vec::new();
        socket.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, expected);
    }

    #[tokio::test]
    async fn handshake_end_and_transport_frames_in_one_read() {
        let (initiator, responder) = handshake_states();
        let mut initiator = NoiseState::from(initiator);
        let (
            mock,
            MockPeer {
                inbound: in_tx,
                outbound: mut out_rx,
                reads,
            },
        ) = MockSocket::new();
        let handshake = Handshake::new(mock, responder, Duration::from_secs(5));
        let responder_task = tokio::spawn(handshake.perform_handshake());

        // -> e
        in_tx.send(wire_frame(&mut initiator, &[])).unwrap();

        // <- e, ee, s, es
        let mut received = Vec::new();
        loop {
            received.extend(out_rx.recv().await.unwrap());
            let frame_len = received
                .get(..2)
                .map(|len| usize::from(u16::from_be_bytes(len.try_into().unwrap())));
            if let Some(message) = frame_len.and_then(|len| received.get(2..len.saturating_add(2))) {
                let mut payload = vec![0u8; MAX_PAYLOAD_LENGTH];
                initiator.read_message(message, &mut payload).unwrap();
                break;
            }
        }

        // -> s, se followed by the first transport frames, all delivered in one read
        let mut chunk = wire_frame(&mut initiator, &[]);
        let mut initiator = initiator.into_transport_mode().unwrap();
        chunk.extend(wire_frame(&mut initiator, b"hello "));
        chunk.extend(wire_frame(&mut initiator, b"world"));
        in_tx.send(chunk).unwrap();
        drop(in_tx);

        let mut socket = responder_task.await.unwrap().unwrap();
        let mut buf = Vec::new();
        socket.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"hello world");
        // One read for each handshake message the responder received
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn eof_inside_a_buffered_partial_frame_is_unexpected() {
        let (mut sender, receiver) = transport_states();
        let (
            mock,
            MockPeer {
                inbound: in_tx,
                outbound: _out_rx,
                reads: _reads,
            },
        ) = MockSocket::new();
        let mut socket = NoiseSocket::new(mock, receiver);

        let mut chunk = wire_frame(&mut sender, b"complete");
        let truncated = wire_frame(&mut sender, b"truncated");
        chunk.extend(truncated.iter().take(truncated.len().saturating_sub(3)));
        in_tx.send(chunk).unwrap();
        drop(in_tx);

        let mut buf = [0u8; 8];
        socket.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"complete");

        let err = socket.read(&mut buf).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert!(socket.is_eof());
        // The error is sticky; a truncated frame never turns into a clean EOF
        let err = socket.read(&mut buf).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn eof_inside_a_buffered_length_prefix_is_unexpected() {
        let (mut sender, receiver) = transport_states();
        let (
            mock,
            MockPeer {
                inbound: in_tx,
                outbound: _out_rx,
                reads: _reads,
            },
        ) = MockSocket::new();
        let mut socket = NoiseSocket::new(mock, receiver);

        let mut chunk = wire_frame(&mut sender, b"complete");
        chunk.push(0);
        in_tx.send(chunk).unwrap();
        drop(in_tx);

        let mut buf = [0u8; 8];
        socket.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"complete");

        let err = socket.read(&mut buf).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }
}
