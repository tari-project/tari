//  Copyright 2021, The Tari Project
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

use std::cmp;

use bytes::Bytes;
use log::*;

use super::LOG_TARGET;
use crate::{
    proto,
    protocol::{
        rpc,
        rpc::{
            message::{RpcMessageFlags, RpcResponse},
            RpcStatusCode,
            RPC_CHUNKING_SIZE_LIMIT,
            RPC_CHUNKING_THRESHOLD,
        },
    },
};

pub(super) struct ChunkedResponseIter {
    message: RpcResponse,
    initial_payload_size: usize,
    has_emitted_once: bool,
    num_chunks: usize,
    total_chunks: usize,
}

fn calculate_total_chunk_count(payload_len: usize) -> usize {
    let mut total_chunks = payload_len / RPC_CHUNKING_THRESHOLD;
    let excess = (payload_len % RPC_CHUNKING_THRESHOLD) + RPC_CHUNKING_THRESHOLD;
    if total_chunks == 0 || excess > RPC_CHUNKING_SIZE_LIMIT {
        // If the chunk (threshold size) + excess cannot fit in the RPC_CHUNKING_SIZE_LIMIT, then we'll emit another
        // frame smaller than threshold size
        total_chunks += 1;
    }

    total_chunks
}

impl ChunkedResponseIter {
    pub fn new(message: RpcResponse) -> Self {
        let len = message.payload.len();
        Self {
            initial_payload_size: message.payload.len(),
            message,
            has_emitted_once: false,
            num_chunks: 0,
            total_chunks: calculate_total_chunk_count(len),
        }
    }

    fn remaining(&self) -> usize {
        self.message.payload.len()
    }

    fn payload_mut(&mut self) -> &mut Bytes {
        &mut self.message.payload
    }

    fn payload(&self) -> &Bytes {
        &self.message.payload
    }

    fn get_next_chunk(&mut self) -> Option<Bytes> {
        let len = self.payload().len();
        if len == 0 {
            if self.num_chunks > 1 {
                debug!(
                    target: LOG_TARGET,
                    "Emitted {} chunks (Avg.Size: {} bytes, Total: {} bytes)",
                    self.num_chunks,
                    self.initial_payload_size / self.num_chunks,
                    self.initial_payload_size
                );
            }
            return None;
        }

        // If the payload is within the maximum chunk size, simply return the rest of it
        if len <= RPC_CHUNKING_SIZE_LIMIT {
            let chunk = self.payload_mut().split_to(len);
            self.num_chunks += 1;
            trace!(
                target: LOG_TARGET,
                "Emitting chunk {}/{} ({} bytes)",
                self.num_chunks,
                self.total_chunks,
                chunk.len()
            );
            return Some(chunk);
        }

        let chunk_size = cmp::min(len, RPC_CHUNKING_THRESHOLD);
        let chunk = self.payload_mut().split_to(chunk_size);

        self.num_chunks += 1;
        trace!(
            target: LOG_TARGET,
            "Emitting chunk {}/{} ({} bytes)",
            self.num_chunks,
            self.total_chunks,
            chunk.len()
        );
        Some(chunk)
    }

    fn is_last_chunk(&self) -> bool {
        self.num_chunks == self.total_chunks
    }

    fn exceeded_message_size(&self) -> proto::rpc::RpcResponse {
        const BYTES_PER_MB: f32 = 1024.0 * 1024.0;
        // Precision loss is acceptable because this is for display purposes only
        let msg = format!(
            "The response size exceeded the maximum allowed payload size. Max = {:.4} MiB, Got = {:.4} MiB",
            rpc::max_response_payload_size() as f32 / BYTES_PER_MB,
            self.message.payload.len() as f32 / BYTES_PER_MB,
        );
        warn!(target: LOG_TARGET, "{}", msg);
        proto::rpc::RpcResponse {
            request_id: self.message.request_id,
            status: RpcStatusCode::MalformedResponse as u32,
            flags: RpcMessageFlags::FIN.bits().into(),
            payload: msg.into_bytes(),
        }
    }
}

impl Iterator for ChunkedResponseIter {
    type Item = proto::rpc::RpcResponse;

    fn next(&mut self) -> Option<Self::Item> {
        // Edge case: the initial message has an empty payload.
        if self.initial_payload_size == 0 {
            if self.has_emitted_once {
                return None;
            }
            self.has_emitted_once = true;
            return Some(self.message.to_proto());
        }

        // Edge case: the total message size cannot fit into the maximum allowed chunks
        if self.remaining() > rpc::max_response_payload_size() {
            if self.has_emitted_once {
                return None;
            }
            self.has_emitted_once = true;
            return Some(self.exceeded_message_size());
        }

        let request_id = self.message.request_id;
        let chunk = self.get_next_chunk()?;

        // status MUST be set for the first chunked message, all subsequent chunk messages MUST have a status of 0
        let mut status = 0;
        if !self.has_emitted_once {
            status = self.message.status as u32;
        }
        self.has_emitted_once = true;

        let mut flags = self.message.flags;
        if !self.is_last_chunk() {
            // For all chunks except the last the MORE flag MUST be set
            flags |= RpcMessageFlags::MORE;
        }
        let msg = proto::rpc::RpcResponse {
            request_id,
            status,
            flags: flags.bits().into(),
            payload: chunk.to_vec(),
        };

        Some(msg)
    }
}

#[cfg(test)]
mod test {
    use std::{convert::TryFrom, iter};

    use super::*;

    fn create(size: usize) -> ChunkedResponseIter {
        let msg = RpcResponse {
            payload: iter::repeat(0).take(size).collect(),
            ..Default::default()
        };
        ChunkedResponseIter::new(msg)
    }

    #[test]
    fn it_emits_a_zero_size_message() {
        let iter = create(0);
        assert_eq!(iter.total_chunks, 1);
        let msgs = iter.collect::<Vec<_>>();
        assert_eq!(msgs.len(), 1);
        assert!(!RpcMessageFlags::from_bits(u8::try_from(msgs[0].flags).unwrap())
            .unwrap()
            .is_more());
    }

    #[test]
    fn it_emits_one_message_below_threshold() {
        let iter = create(RPC_CHUNKING_THRESHOLD - 1);
        assert_eq!(iter.total_chunks, 1);
        let msgs = iter.collect::<Vec<_>>();
        assert_eq!(msgs.len(), 1);
        assert!(!RpcMessageFlags::from_bits(u8::try_from(msgs[0].flags).unwrap())
            .unwrap()
            .is_more());
    }

    #[test]
    fn it_emits_a_single_message() {
        let iter = create(RPC_CHUNKING_SIZE_LIMIT - 1);
        assert_eq!(iter.count(), 1);

        let iter = create(RPC_CHUNKING_SIZE_LIMIT);
        assert_eq!(iter.count(), 1);
    }

    #[test]
    fn it_emits_an_expected_number_of_chunks() {
        let iter = create(RPC_CHUNKING_THRESHOLD * 2);
        assert_eq!(iter.count(), 2);

        let diff = RPC_CHUNKING_SIZE_LIMIT - RPC_CHUNKING_THRESHOLD;
        let iter = create(RPC_CHUNKING_THRESHOLD * 2 + diff);
        assert_eq!(iter.count(), 2);

        let iter = create(RPC_CHUNKING_THRESHOLD * 2 + diff + 1);
        assert_eq!(iter.count(), 3);
    }

    #[test]
    fn it_sets_the_more_flag_except_last() {
        use std::convert::TryFrom;
        let iter = create(RPC_CHUNKING_THRESHOLD * 3);
        let msgs = iter.collect::<Vec<_>>();
        assert!(RpcMessageFlags::from_bits(u8::try_from(msgs[0].flags).unwrap())
            .unwrap()
            .is_more());
        assert!(RpcMessageFlags::from_bits(u8::try_from(msgs[1].flags).unwrap())
            .unwrap()
            .is_more());
        assert!(!RpcMessageFlags::from_bits(u8::try_from(msgs[2].flags).unwrap())
            .unwrap()
            .is_more());
    }
}
