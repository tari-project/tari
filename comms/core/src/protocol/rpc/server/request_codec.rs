//  Copyright 2026, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{cmp, io};

use bytes::{Buf, Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

/// The length prefix of every frame: a big-endian u32, as `LengthDelimitedCodec` writes it with its defaults
const LENGTH_PREFIX_SIZE: usize = 4;

/// The `RpcRequest` field tag of `request_id` (field 1, varint). prost writes fields in tag order, so it comes first.
const REQUEST_ID_TAG: u8 = 0x08;

/// Enough of the start of an `RpcRequest` to read its `request_id`: the tag and a u32 varint (at most 5 bytes)
const REQUEST_ID_PREFIX_SIZE: usize = 6;

/// A frame read by the RPC server from a session substream
#[derive(Debug)]
pub enum RequestFrame {
    Request(BytesMut),
    /// A request declaring a length over the request limit. Only its `request_id` (0 if it could not be read) and
    /// declared length are kept; the rest of it is skipped as it arrives, without being buffered.
    Oversized {
        request_id: u32,
        len: usize,
    },
    /// In peek mode (see [RequestCodec::set_peek_limit]): the next frame is longer than the peek limit. Nothing of it
    /// was consumed; it is read as usual once peek mode is off.
    Unread,
}

/// The server's framing for an RPC session: the canonical length-delimited framing, except that a request longer than
/// the request limit is turned away from its length prefix alone.
///
/// `LengthDelimitedCodec` reserves buffer space for the length a frame declares as soon as it has read the prefix, so
/// with the 8 MiB response frame limit, four bytes from a client would make the server reserve 8 MiB per session. This
/// reads the prefix itself and reserves nothing for the declared length: the read buffer grows only as bytes arrive
/// (`Framed` makes room for each read), so a request only costs what the client has actually sent of it, up to the
/// request limit for one sent in full. The allocation stays behind after a large request (later frames reuse it)
/// until the session replaces it with a small one (see `release_large_read_buffer`). A longer request
/// (up to the frame limit, which a peer running a version with a larger request cap may honestly send) is reported as
/// [RequestFrame::Oversized] once the start of it, which holds its `request_id`, has arrived, so the server can reply
/// with `BadRequest` and carry on with the session. A length over the frame limit is an error, as it is for
/// `LengthDelimitedCodec`.
///
/// It is only used once the handshake is done: the handshake frames are read with the canonical framing, whose limit is
/// lowered to `MAX_HANDSHAKE_FRAME_SIZE` while they are read (see `Handshake`), so they are capped from their length
/// prefix too. Responses are encoded by `LengthDelimitedCodec` as before.
pub struct RequestCodec {
    encoder: LengthDelimitedCodec,
    max_frame_len: usize,
    max_request_size: usize,
    /// The bytes of an oversized request that are still to be skipped
    skip_remaining: usize,
    /// In peek mode, the longest frame that is decoded; a longer one is reported as [RequestFrame::Unread]
    peek_limit: Option<usize>,
}

impl RequestCodec {
    /// Takes over from `codec` between frames: it must not be part way through decoding one.
    pub fn new(codec: LengthDelimitedCodec, max_request_size: usize) -> Self {
        Self {
            max_frame_len: codec.max_frame_length(),
            encoder: codec,
            max_request_size,
            skip_remaining: 0,
            peek_limit: None,
        }
    }

    /// Turns peek mode on (`Some(limit)`) or off (`None`). In peek mode a frame longer than `limit` is not read: it is
    /// reported as [RequestFrame::Unread] (every time it is asked for) and left where it is, so the session stops
    /// reading the substream and the client's flow control holds the rest of it back.
    pub fn set_peek_limit(&mut self, limit: Option<usize>) {
        self.peek_limit = limit;
    }
}

impl Decoder for RequestCodec {
    type Error = io::Error;
    type Item = RequestFrame;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<RequestFrame>, io::Error> {
        if self.skip_remaining > 0 {
            let num_skipped = cmp::min(self.skip_remaining, src.len());
            src.advance(num_skipped);
            self.skip_remaining = self.skip_remaining.saturating_sub(num_skipped);
            if self.skip_remaining > 0 {
                return Ok(None);
            }
        }

        let Some(mut prefix) = src.get(..LENGTH_PREFIX_SIZE) else {
            return Ok(None);
        };
        let len = usize::try_from(prefix.get_u32()).unwrap_or(usize::MAX);
        if len > self.max_frame_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "frame of {len} bytes is larger than the {} byte limit",
                    self.max_frame_len
                ),
            ));
        }

        if let Some(peek_limit) = self.peek_limit &&
            len > peek_limit
        {
            return Ok(Some(RequestFrame::Unread));
        }

        if len > self.max_request_size {
            let body_start_end = LENGTH_PREFIX_SIZE.saturating_add(cmp::min(len, REQUEST_ID_PREFIX_SIZE));
            let Some(body_start) = src.get(LENGTH_PREFIX_SIZE..body_start_end) else {
                return Ok(None);
            };
            let request_id = read_request_id(body_start);
            // The body (including the part just read) is skipped from the next call on
            src.advance(LENGTH_PREFIX_SIZE);
            self.skip_remaining = len;
            return Ok(Some(RequestFrame::Oversized { request_id, len }));
        }

        // Nothing is reserved for the rest of the frame: `Framed` grows the buffer as bytes arrive
        if src.len() < LENGTH_PREFIX_SIZE.saturating_add(len) {
            return Ok(None);
        }
        src.advance(LENGTH_PREFIX_SIZE);
        Ok(Some(RequestFrame::Request(src.split_to(len))))
    }
}

impl Encoder<Bytes> for RequestCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Bytes, dst: &mut BytesMut) -> Result<(), io::Error> {
        self.encoder.encode(item, dst)
    }
}

/// Reads the `request_id` from the start of an encoded `RpcRequest`, or returns 0 if it is not there
fn read_request_id(body_start: &[u8]) -> u32 {
    match body_start.split_first() {
        Some((&REQUEST_ID_TAG, mut varint)) => prost::encoding::decode_varint(&mut varint)
            .ok()
            .and_then(|id| u32::try_from(id).ok())
            .unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
mod test {
    use prost::Message;

    use super::*;
    use crate::proto;

    const MAX_REQUEST_SIZE: usize = 100;

    fn codec() -> RequestCodec {
        let codec = LengthDelimitedCodec::builder().max_frame_length(1000).new_codec();
        RequestCodec::new(codec, MAX_REQUEST_SIZE)
    }

    fn encoded_request(request_id: u32, payload_len: usize) -> Vec<u8> {
        proto::rpc::RpcRequest {
            request_id,
            method: 1,
            flags: 0,
            deadline: 10,
            payload: vec![1; payload_len],
        }
        .encode_to_vec()
    }

    fn with_prefix(body: &[u8]) -> BytesMut {
        let mut frame = BytesMut::new();
        frame.extend_from_slice(&u32::try_from(body.len()).unwrap().to_be_bytes());
        frame.extend_from_slice(body);
        frame
    }

    #[test]
    fn it_decodes_requests_within_the_limit() {
        let mut codec = codec();
        let body = encoded_request(3, 20);
        let mut src = with_prefix(&body);
        src.extend_from_slice(&with_prefix(&body));
        for _ in 0..2 {
            let RequestFrame::Request(frame) = codec.decode(&mut src).unwrap().unwrap() else {
                panic!("expected a request");
            };
            assert_eq!(frame.as_ref(), body.as_slice());
        }
        assert!(codec.decode(&mut src).unwrap().is_none());
        assert!(src.is_empty());
    }

    #[test]
    fn it_reports_an_oversized_request_from_its_start_without_buffering_it() {
        let mut codec = codec();
        let body = encoded_request(7, MAX_REQUEST_SIZE);
        assert!(body.len() > MAX_REQUEST_SIZE);
        let frame = with_prefix(&body);

        // The length prefix alone is not enough: the request id is not known yet
        let mut src = BytesMut::from(frame.get(..LENGTH_PREFIX_SIZE).unwrap());
        assert!(codec.decode(&mut src).unwrap().is_none());
        // Nothing was reserved for the declared length
        assert!(src.capacity() < MAX_REQUEST_SIZE);

        src.extend_from_slice(
            frame
                .get(LENGTH_PREFIX_SIZE..LENGTH_PREFIX_SIZE + REQUEST_ID_PREFIX_SIZE)
                .unwrap(),
        );
        let RequestFrame::Oversized { request_id, len } = codec.decode(&mut src).unwrap().unwrap() else {
            panic!("expected an oversized request");
        };
        assert_eq!(request_id, 7);
        assert_eq!(len, body.len());

        // The rest of the body is skipped as it arrives, then the next request is decoded as normal
        assert!(codec.decode(&mut src).unwrap().is_none());
        src.extend_from_slice(frame.get(LENGTH_PREFIX_SIZE + REQUEST_ID_PREFIX_SIZE..).unwrap());
        let next = encoded_request(8, 10);
        src.extend_from_slice(&with_prefix(&next));
        let RequestFrame::Request(decoded) = codec.decode(&mut src).unwrap().unwrap() else {
            panic!("expected a request");
        };
        assert_eq!(decoded.as_ref(), next.as_slice());
    }

    #[test]
    fn it_reserves_nothing_for_the_declared_length_of_a_request() {
        let codec = LengthDelimitedCodec::builder().max_frame_length(2_000_000).new_codec();
        let mut codec = RequestCodec::new(codec, 1_000_000);
        let mut src = BytesMut::from(&900_000u32.to_be_bytes()[..]);
        assert!(codec.decode(&mut src).unwrap().is_none());
        assert!(src.capacity() < 1024, "reserved {} bytes", src.capacity());
    }

    #[test]
    fn in_peek_mode_a_long_frame_is_left_unread() {
        let mut codec = codec();
        codec.set_peek_limit(Some(10));
        let short = [1u8; 10];
        let long = encoded_request(9, 20);
        let mut src = with_prefix(&short);
        src.extend_from_slice(&with_prefix(&long));

        let RequestFrame::Request(frame) = codec.decode(&mut src).unwrap().unwrap() else {
            panic!("expected a request");
        };
        assert_eq!(frame.as_ref(), short.as_slice());
        // Reported, but nothing of it consumed, however often it is asked for
        let len_before = src.len();
        for _ in 0..2 {
            assert!(matches!(codec.decode(&mut src).unwrap(), Some(RequestFrame::Unread)));
            assert_eq!(src.len(), len_before);
        }

        codec.set_peek_limit(None);
        let RequestFrame::Request(frame) = codec.decode(&mut src).unwrap().unwrap() else {
            panic!("expected a request");
        };
        assert_eq!(frame.as_ref(), long.as_slice());
    }

    #[test]
    fn it_rejects_a_frame_over_the_frame_limit() {
        let mut codec = codec();
        let mut src = BytesMut::from(&1001u32.to_be_bytes()[..]);
        codec.decode(&mut src).unwrap_err();
    }

    #[test]
    fn it_reads_the_request_id_from_the_start_of_a_request() {
        let body = encoded_request(300, 0);
        assert_eq!(read_request_id(body.get(..REQUEST_ID_PREFIX_SIZE).unwrap()), 300);
        // No request id field (it is 0), or a truncated one
        assert_eq!(read_request_id(&encoded_request(0, 0)), 0);
        assert_eq!(read_request_id(&[REQUEST_ID_TAG, 0x80]), 0);
        assert_eq!(read_request_id(&[]), 0);
    }
}
