//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, marker::PhantomData};

// Re-export prost public types
pub use ::prost::Message;
use async_trait::async_trait;
use libp2p::futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::codec::Codec;

const MAX_MESSAGE_SIZE: usize = 4 * 1024 * 1024;

pub struct ProstCodec<TMsg>(PhantomData<TMsg>);

impl<TMsg> Default for ProstCodec<TMsg> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

#[async_trait]
impl<TMsg> Codec for ProstCodec<TMsg>
where TMsg: prost::Message + Default
{
    type Message = TMsg;

    async fn decode_from<R>(&mut self, reader: &mut R) -> std::io::Result<(usize, Self::Message)>
    where R: AsyncRead + Unpin + Send {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "message too large"));
        }
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).await?;
        let mut slice = &buf[..];
        let message =
            prost::Message::decode(&mut slice).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        if !slice.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "bytes remaining on buffer",
            ));
        }
        Ok((len, message))
    }

    async fn encode_to<W>(&mut self, writer: &mut W, message: Self::Message) -> std::io::Result<()>
    where W: AsyncWrite + Unpin + Send {
        let mut buf = Vec::new();
        message
            .encode(&mut buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let len = buf.len();
        if len > MAX_MESSAGE_SIZE {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "message too large"));
        }
        writer
            .write_all(
                &u32::try_from(len)
                    .expect("len checked MAX_MESSAGE_SIZE < u32::MAX")
                    .to_be_bytes(),
            )
            .await?;
        writer.write_all(&buf).await?;
        Ok(())
    }
}

impl<TMsg> Clone for ProstCodec<TMsg> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<TMsg> fmt::Debug for ProstCodec<TMsg> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProstCodec").finish()
    }
}
