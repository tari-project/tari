//  Copyright 2022. The Tari Project
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

use std::{io, io::Write, marker::PhantomData};

use digest::{consts::U32, Digest};
use tari_crypto::{
    hash::blake2::Blake256,
    hashing::{DomainSeparatedHasher, DomainSeparation},
};

use crate::consensus::ConsensusEncoding;

/// Domain separated consensus encoding hasher.
pub struct DomainSeparatedConsensusHasher<M: DomainSeparation>(PhantomData<M>);

impl<M: DomainSeparation> DomainSeparatedConsensusHasher<M> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(label: &'static str) -> ConsensusHasher<DomainSeparatedHasher<Blake256, M>> {
        ConsensusHasher::new(DomainSeparatedHasher::new_with_label(label))
    }
}

#[derive(Clone)]
pub struct ConsensusHasher<D> {
    writer: WriteHashWrapper<D>,
}

impl<D: Digest> ConsensusHasher<D> {
    pub fn new(digest: D) -> Self {
        Self {
            writer: WriteHashWrapper(digest),
        }
    }
}

impl<D> ConsensusHasher<D>
where D: Digest<OutputSize = U32>
{
    pub fn finalize(self) -> [u8; 32] {
        self.writer.0.finalize().into()
    }

    pub fn update_consensus_encode<T: ConsensusEncoding + ?Sized>(&mut self, data: &T) {
        // UNWRAP: ConsensusEncode MUST only error if the writer errors, HashWriter::write is infallible
        data.consensus_encode(&mut self.writer)
            .expect("Incorrect implementation of ConsensusEncoding encountered. Implementations MUST be infallible.");
    }

    pub fn chain<T: ConsensusEncoding>(mut self, data: &T) -> Self {
        self.update_consensus_encode(data);
        self
    }
}

impl Default for ConsensusHasher<Blake256> {
    fn default() -> Self {
        ConsensusHasher::new(Blake256::new())
    }
}

/// This private struct wraps a Digest and implements the Write trait to satisfy the consensus encoding trait..
#[derive(Clone)]
struct WriteHashWrapper<D>(D);

impl<D: Digest> Write for WriteHashWrapper<D> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tari_crypto::{hash::blake2::Blake256, hash_domain};

    use super::*;

    #[test]
    fn it_hashes_using_the_domain_hasher() {
        hash_domain!(TestHashDomain, "tari.test", 0);
        let expected_hash = DomainSeparatedHasher::<Blake256, TestHashDomain>::new_with_label("foo")
            .chain(b"\xff\x01")
            .finalize();
        let hash = DomainSeparatedConsensusHasher::<TestHashDomain>::new("foo")
            .chain(&255u64)
            .finalize();

        assert_eq!(hash, expected_hash.as_ref());
    }
}
