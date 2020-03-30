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

use super::{ProtocolError, ProtocolId};
use bitflags::bitflags;
use bytes::{Bytes, BytesMut};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use log::*;
use std::convert::TryInto;

const LOG_TARGET: &str = "comms::connection_manager::protocol";

const BUF_CAPACITY: usize = std::u8::MAX as usize;
const MAX_ROUNDS_ALLOWED: u8 = 5;

pub struct ProtocolNegotiation<'a, TSocket> {
    buf: BytesMut,
    socket: &'a mut TSocket,
}

bitflags! {
    struct Flags: u8 {
        const NONE = 0x00;
        const OPTIMISTIC = 0x01;
        const TERMINATE = 0x02;
        const NOT_SUPPORTED = 0x04;
    }
}

impl<'a, TSocket> ProtocolNegotiation<'a, TSocket>
where TSocket: AsyncRead + AsyncWrite + Unpin
{
    pub fn new(socket: &'a mut TSocket) -> Self {
        Self {
            socket,
            buf: {
                let mut buf = BytesMut::with_capacity(BUF_CAPACITY);
                buf.resize(BUF_CAPACITY, 0);
                buf
            },
        }
    }

    /// Negotiate a protocol to speak. Since this node is initiating this interation, send each protocol this node
    /// wishes to speak until the destination node agrees.
    pub async fn negotiate_protocol_outbound(
        &mut self,
        selected_protocols: &[ProtocolId],
    ) -> Result<ProtocolId, ProtocolError>
    {
        for protocol in selected_protocols {
            self.write_frame_flush(protocol, Flags::NONE).await?;

            let (proto, flags) = self.read_frame().await?;
            // Friendly reply indicating that the maximum number of protocols in one 'session' has been reached
            // This reply cannot be relied upon, so protocol negotiation should be used with a timeout
            if flags.contains(Flags::TERMINATE) {
                return Err(ProtocolError::ProtocolNegotiationTerminatedByPeer);
            }
            if flags.contains(Flags::NOT_SUPPORTED) {
                continue;
            }

            if proto.as_ref() == protocol {
                // Shallow copy
                return Ok(protocol.clone());
            }
        }

        // No more protocols to negotiate - let the peer know
        self.write_frame_flush(&[], Flags::TERMINATE).await?;

        Err(ProtocolError::ProtocolOutboundNegotiationFailed)
    }

    /// Negotiate a protocol to speak. Since this node is initiating this interation, send each protocol this node
    /// wishes to speak until the destination node agrees.
    pub async fn negotiate_protocol_outbound_optimistic(
        &mut self,
        protocol: &ProtocolId,
    ) -> Result<ProtocolId, ProtocolError>
    {
        self.write_frame_flush(protocol, Flags::OPTIMISTIC | Flags::TERMINATE)
            .await?;

        Ok(protocol.clone())
    }

    /// Negotiate a protocol to speak. Since this node is the responder, first we wait for a protocol to be sent and see
    /// if it is in the supported protocol list.
    pub async fn negotiate_protocol_inbound(
        &mut self,
        supported_protocols: &[ProtocolId],
    ) -> Result<ProtocolId, ProtocolError>
    {
        let mut round = 0;
        loop {
            let (proto, flags) = self.read_frame().await?;

            if flags.contains(Flags::OPTIMISTIC) {
                return if supported_protocols.as_ref().iter().any(|p| proto == p) {
                    Ok(proto.clone())
                } else {
                    Err(ProtocolError::ProtocolOptimisticNegotiationFailed)
                };
            }

            // Allow the peer to send a friendly reply saying that it has no more protocols to negotiate.
            // This reply cannot be relied upon, so protocol negotiation should be used with a timeout
            if flags.contains(Flags::TERMINATE) {
                return Err(ProtocolError::ProtocolNegotiationTerminatedByPeer);
            }

            match supported_protocols.as_ref().iter().find(|p| proto == p) {
                Some(proto) => {
                    self.write_frame_flush(proto, Flags::NONE).await?;
                    // Shallow copy
                    return Ok(proto.clone());
                },
                None => {
                    let mut flags = Flags::NOT_SUPPORTED;
                    let terminate = round == MAX_ROUNDS_ALLOWED - 1;
                    if terminate {
                        // Maximum rounds reached - send a friendly reply to let the peer know to give up
                        flags |= Flags::TERMINATE;
                    }
                    self.write_frame_flush(&[], flags).await?;
                    if terminate {
                        break;
                    }
                },
            }

            round += 1;
        }

        Err(ProtocolError::ProtocolInboundNegotiationFailed)
    }

    async fn read_frame(&mut self) -> Result<(Bytes, Flags), ProtocolError> {
        self.socket.read_exact(&mut self.buf[..2]).await?;
        // Len can never overflow the buffer because the buffer len is u8::MAX and the length delimiter
        // is a u8. If that changes, then len should be checked for overflow
        let len = u8::from_be_bytes([self.buf[0]]) as usize;
        let flags = Flags::from_bits_truncate(u8::from_be_bytes([self.buf[1]]));
        self.socket.read_exact(&mut self.buf[0..len]).await?;
        trace!(
            target: LOG_TARGET,
            "Read frame '{}' ({} byte(s) Flags={:?})",
            String::from_utf8_lossy(&self.buf[0..len]),
            len,
            flags,
        );
        Ok((Bytes::copy_from_slice(&self.buf[0..len]), flags))
    }

    async fn write_frame_flush(&mut self, protocol: &[u8], flags: Flags) -> Result<(), ProtocolError> {
        let len_byte = protocol
            .len()
            .try_into()
            .map(|v: u8| v.to_be_bytes())
            .map_err(|_| ProtocolError::ProtocolIdTooLong)?;
        self.socket.write(&len_byte).await?;
        self.socket.write(&flags.bits().to_be_bytes()).await?;
        self.socket.write_all(&protocol).await?;
        self.socket.flush().await?;
        trace!(
            target: LOG_TARGET,
            "Wrote frame '{}' ({} byte(s) Flags={:?})",
            String::from_utf8_lossy(&protocol),
            len_byte[0],
            flags
        );
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::memsocket::MemorySocket;
    use futures::future;
    use tari_test_utils::unpack_enum;

    #[tokio_macros::test_basic]
    async fn negotiate_success() {
        let (mut initiator, mut responder) = MemorySocket::new_pair();
        let mut negotiate_out = ProtocolNegotiation::new(&mut initiator);
        let mut negotiate_in = ProtocolNegotiation::new(&mut responder);

        let supported_protocols = vec![b"B", b"A"]
            .into_iter()
            .map(|p| ProtocolId::from_static(p))
            .collect::<Vec<_>>();
        let selected_protocols = vec![b"C", b"D", b"E", b"F", b"A"]
            .into_iter()
            .map(|p| ProtocolId::from_static(p))
            .collect::<Vec<_>>();

        let (in_proto, out_proto) = future::join(
            negotiate_in.negotiate_protocol_inbound(&supported_protocols),
            negotiate_out.negotiate_protocol_outbound(&selected_protocols),
        )
        .await;

        assert_eq!(in_proto.unwrap(), ProtocolId::from_static(b"A"));
        assert_eq!(out_proto.unwrap(), ProtocolId::from_static(b"A"));
    }

    #[tokio_macros::test_basic]
    async fn negotiate_fail() {
        let (mut initiator, mut responder) = MemorySocket::new_pair();
        let mut negotiate_out = ProtocolNegotiation::new(&mut initiator);
        let mut negotiate_in = ProtocolNegotiation::new(&mut responder);

        let supported_protocols = vec![b"A", b"B"]
            .into_iter()
            .map(|p| ProtocolId::from_static(p))
            .collect::<Vec<_>>();
        let selected_protocols = vec![b"C", b"D", b"E"]
            .into_iter()
            .map(|p| ProtocolId::from_static(p))
            .collect::<Vec<_>>();

        let (in_proto, out_proto) = future::join(
            negotiate_in.negotiate_protocol_inbound(&supported_protocols),
            negotiate_out.negotiate_protocol_outbound(&selected_protocols),
        )
        .await;

        unpack_enum!(ProtocolError::ProtocolNegotiationTerminatedByPeer = in_proto.unwrap_err());
        unpack_enum!(ProtocolError::ProtocolOutboundNegotiationFailed = out_proto.unwrap_err());
    }

    #[tokio_macros::test_basic]
    async fn negotiate_fail_max_rounds() {
        let (mut initiator, mut responder) = MemorySocket::new_pair();
        let mut negotiate_out = ProtocolNegotiation::new(&mut initiator);
        let mut negotiate_in = ProtocolNegotiation::new(&mut responder);

        let supported_protocols = vec![b"A", b"B"]
            .into_iter()
            .map(|p| ProtocolId::from_static(p))
            .collect::<Vec<_>>();
        let selected_protocols = vec![b"C", b"D", b"E", b"F", b"G", b"A"]
            .into_iter()
            .map(|p| ProtocolId::from_static(p))
            .collect::<Vec<_>>();

        let (in_proto, out_proto) = future::join(
            negotiate_in.negotiate_protocol_inbound(&supported_protocols),
            negotiate_out.negotiate_protocol_outbound(&selected_protocols),
        )
        .await;

        unpack_enum!(ProtocolError::ProtocolInboundNegotiationFailed = in_proto.unwrap_err());
        unpack_enum!(ProtocolError::ProtocolNegotiationTerminatedByPeer = out_proto.unwrap_err());
    }

    #[tokio_macros::test_basic]
    async fn negotiate_success_optimistic() {
        let (mut initiator, mut responder) = MemorySocket::new_pair();
        let mut negotiate_out = ProtocolNegotiation::new(&mut initiator);
        let mut negotiate_in = ProtocolNegotiation::new(&mut responder);

        let supported_protocols = vec![b"B", b"A"]
            .into_iter()
            .map(|p| ProtocolId::from_static(p))
            .collect::<Vec<_>>();

        let (in_proto, out_proto) = future::join(
            negotiate_in.negotiate_protocol_inbound(&supported_protocols),
            negotiate_out.negotiate_protocol_outbound_optimistic(&Bytes::from_static(b"A")),
        )
        .await;

        assert_eq!(in_proto.unwrap(), ProtocolId::from_static(b"A"));
        out_proto.unwrap();
    }

    #[tokio_macros::test_basic]
    async fn negotiate_fail_optimistic() {
        let (mut initiator, mut responder) = MemorySocket::new_pair();
        let mut negotiate_out = ProtocolNegotiation::new(&mut initiator);
        let mut negotiate_in = ProtocolNegotiation::new(&mut responder);

        let supported_protocols = vec![b"A", b"B"]
            .into_iter()
            .map(|p| ProtocolId::from_static(p))
            .collect::<Vec<_>>();

        let (in_proto, out_proto) = future::join(
            negotiate_in.negotiate_protocol_inbound(&supported_protocols),
            negotiate_out.negotiate_protocol_outbound_optimistic(&Bytes::from_static(b"C")),
        )
        .await;

        unpack_enum!(ProtocolError::ProtocolOptimisticNegotiationFailed = in_proto.unwrap_err());
        out_proto.unwrap();
    }
}
