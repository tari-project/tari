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

use super::MessageError;

// Re-export protos
pub use crate::proto::envelope::*;

/// Wraps a number of `prost::Message`s in a EnvelopeBody
#[macro_export]
macro_rules! wrap_in_envelope_body {
    ($($e:expr),+) => {{
        use $crate::message::MessageExt;
        let mut envelope_body = $crate::message::EnvelopeBody::new();
        let mut error = None;
        $(
            match $e.to_encoded_bytes() {
                Ok(bytes) => envelope_body.push_part(bytes),
                Err(err) => {
                    if error.is_none() {
                        error = Some(err);
                    }
                }
            }
        )*

        match error {
            Some(err) => Err(err),
            None => Ok(envelope_body),
        }
    }}
}

impl EnvelopeBody {
    pub fn new() -> Self {
        Self {
            parts: Default::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.parts.len()
    }

    pub fn total_size(&self) -> usize {
        self.parts.iter().fold(0, |acc, b| acc + b.len())
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Removes and returns the part at the given index. None
    /// is returned if the index is out of bounds
    pub fn take_part(&mut self, index: usize) -> Option<Vec<u8>> {
        Some(index)
            .filter(|i| self.parts.len() > *i)
            .map(|i| self.parts.remove(i))
    }

    pub fn push_part(&mut self, part: Vec<u8>) {
        self.parts.push(part)
    }

    pub fn into_inner(self) -> Vec<Vec<u8>> {
        self.parts
    }

    /// Decodes a part of the message body and returns the result. If the part index is out of range Ok(None) is
    /// returned
    pub fn decode_part<T>(&self, index: usize) -> Result<Option<T>, MessageError>
    where T: prost::Message + Default {
        match self.parts.get(index) {
            Some(part) => T::decode(part.as_slice()).map(Some).map_err(Into::into),
            None => Ok(None),
        }
    }
}
