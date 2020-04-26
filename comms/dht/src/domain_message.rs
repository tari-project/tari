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

use rand::{rngs::OsRng, RngCore};
use std::cmp;

pub trait ToProtoEnum {
    fn as_i32(&self) -> i32;
}

impl ToProtoEnum for i32 {
    fn as_i32(&self) -> i32 {
        *self
    }
}

#[derive(Debug)]
pub struct OutboundDomainMessage<T> {
    inner: T,
    message_type: i32,
}

impl<T> OutboundDomainMessage<T> {
    pub fn new<M: ToProtoEnum>(message_type: M, message: T) -> Self {
        Self {
            inner: message,
            message_type: message_type.as_i32(),
        }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn to_propagation_header(&self) -> MessageHeader {
        MessageHeader::for_propagation(self.message_type)
    }

    pub fn to_header(&self) -> MessageHeader {
        MessageHeader::new(self.message_type)
    }
}

pub use crate::proto::message_header::MessageHeader;

impl MessageHeader {
    pub fn new(message_type: i32) -> Self {
        Self {
            message_type,
            // In the unimaginably unlikely case that a nonce of 0 chosen,
            // change it to 1 because 0 is exclusively for message propagation
            nonce: cmp::max(1, OsRng.next_u64()),
        }
    }

    pub fn for_propagation(message_type: i32) -> Self {
        const PROPAGATION_NONCE: u64 = 0;
        Self {
            message_type,
            nonce: PROPAGATION_NONCE,
        }
    }
}
