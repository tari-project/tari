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

use std::io;

/// Abstracts the ability of a type to canonically encode itself for the purposes of consensus
pub trait ConsensusEncoding {
    /// Encode to the given writer returning the number of bytes writter.
    /// If writing to this Writer is infallible, this implementation must always succeed.
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error>;
}

pub trait ConsensusEncodingSized: ConsensusEncoding {
    /// The return value MUST be the exact byte size of the implementing type
    /// and SHOULD be implemented without allocations.
    fn consensus_encode_exact_size(&self) -> usize;
}

/// Abstracts the ability of a type to be decoded from canonical consensus bytes
pub trait ConsensusDecoding: Sized {
    /// Attempt to decode this type from the given reader
    fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error>;
}

pub struct ConsensusEncodingWrapper<'a, T> {
    inner: &'a T,
}

impl<'a, T> ConsensusEncodingWrapper<'a, T> {
    pub fn wrap(inner: &'a T) -> Self {
        Self { inner }
    }
}

// TODO: move traits and implement consensus encoding for TariScript
//       for now, this wrapper will do that job
mod tariscript_impl {
    use super::*;
    use crate::common::byte_counter::ByteCounter;
    use tari_crypto::script::TariScript;

    impl<'a> ConsensusEncoding for ConsensusEncodingWrapper<'a, TariScript> {
        fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
            let bytes = self.inner.as_bytes();
            writer.write_all(&bytes)?;
            Ok(bytes.len())
        }
    }

    impl<'a> ConsensusEncodingSized for ConsensusEncodingWrapper<'a, TariScript> {
        fn consensus_encode_exact_size(&self) -> usize {
            let mut counter = ByteCounter::new();
            // TODO: consensus_encode_exact_size must be cheap to run
            // unreachable panic: ByteCounter is infallible
            self.consensus_encode(&mut counter).expect("unreachable");
            counter.get()
        }
    }
}
