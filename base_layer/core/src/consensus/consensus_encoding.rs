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

mod bytes;
mod crypto;
mod generic;
mod integers;
mod micro_tari;
mod script;
mod vec;

use std::io;

pub use vec::MaxSizeVec;

pub use self::bytes::MaxSizeBytes;
use crate::common::byte_counter::ByteCounter;

/// Abstracts the ability of a type to canonically encode itself for the purposes of consensus
pub trait ConsensusEncoding {
    /// Encode to the given writer returning the number of bytes written.
    /// If writing to this Writer is infallible, this implementation MUST always succeed.
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error>;
}

pub trait ConsensusEncodingSized: ConsensusEncoding {
    /// The return value MUST be the exact byte size of the implementing type
    /// and SHOULD be implemented without allocations.
    fn consensus_encode_exact_size(&self) -> usize {
        let mut byte_counter = ByteCounter::new();
        self.consensus_encode(&mut byte_counter)
            .expect("ByteCounter is infallible");
        byte_counter.get()
    }
}

/// Abstracts the ability of a type to be decoded from canonical consensus bytes
pub trait ConsensusDecoding: Sized {
    /// Attempt to decode this type from the given reader
    fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error>;
}

pub trait ToConsensusBytes {
    fn to_consensus_bytes(&self) -> Vec<u8>;
}

impl<T: ConsensusEncoding + ConsensusEncodingSized + ?Sized> ToConsensusBytes for T {
    fn to_consensus_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.consensus_encode_exact_size());
        // Vec's write impl is infallible, as per the ConsensusEncoding contract, consensus_encode is infallible
        self.consensus_encode(&mut buf).expect("unreachable panic");
        buf
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    /// Test utility function that checks the correctness of the ConsensusEncoding, ConsensusEncodingSized,
    /// ConsensusDecoding implementations
    pub fn check_consensus_encoding_correctness<T>(subject: T) -> Result<(), io::Error>
    where T: ConsensusEncoding + ConsensusEncodingSized + ConsensusDecoding + Eq + std::fmt::Debug {
        let mut buf = Vec::new();
        subject.consensus_encode(&mut buf)?;
        assert_eq!(buf.len(), subject.consensus_encode_exact_size());
        let decoded = T::consensus_decode(&mut buf.as_slice())?;
        assert_eq!(decoded, subject);
        Ok(())
    }
}
