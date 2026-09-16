//  Copyright 2019 The Tari Project
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

//! # Message
//!
//! The message module contains the message types which wrap domain-level messages.

#[macro_use]
mod envelope;

use bytes::BytesMut;
pub use envelope::EnvelopeBody;

mod error;
pub use error::MessageError;

mod inbound;
pub use inbound::InboundMessage;

mod outbound;
pub use outbound::{MessagingReplyRx, MessagingReplyTx, OutboundMessage};

mod tag;
pub use tag::MessageTag;

/// Provides extensions to the prost Message trait.
pub trait MessageExt: prost::Message {
    /// Encodes a message, allocating the buffer on the heap as necessary
    fn to_encoded_bytes(&self) -> Vec<u8>
    where Self: Sized {
        self.encode_to_vec()
    }

    /// Encodes a message into a BytesMut, allocating the buffer on the heap as necessary.
    fn encode_into_bytes_mut(&self) -> BytesMut
    where Self: Sized {
        let mut buf = BytesMut::with_capacity(self.encoded_len());
        self.encode(&mut buf).expect(
            "prost::Message::encode documentation says it is infallible unless the buffer has insufficient capacity. \
             This buffer's capacity was set with encoded_len",
        );
        buf
    }
}
impl<T: prost::Message> MessageExt for T {}
