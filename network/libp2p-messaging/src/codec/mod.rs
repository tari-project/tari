//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

#[cfg(feature = "prost")]
pub mod prost;

use std::{fmt, io};

use libp2p::futures::{AsyncRead, AsyncWrite};

/// A `Codec` defines the request and response types
/// for a request-response [`Behaviour`](crate::Behaviour) protocol or
/// protocol family and how they are encoded / decoded on an I/O stream.
#[async_trait::async_trait]
pub trait Codec: Default {
    /// The type of inbound and outbound message.
    type Message: fmt::Debug + Send;

    /// Reads a message from the given I/O stream according to the
    /// negotiated protocol.
    async fn decode_from<R>(&self, reader: &mut R) -> io::Result<(usize, Self::Message)>
    where R: AsyncRead + Unpin + Send;

    /// Writes a request to the given I/O stream according to the
    /// negotiated protocol.
    async fn encode_to<W>(&self, writer: &mut W, message: Self::Message) -> io::Result<()>
    where W: AsyncWrite + Unpin + Send;
}
