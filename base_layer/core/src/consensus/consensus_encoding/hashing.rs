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

use borsh::BorshSerialize;
use digest::{consts::U32, Digest};
use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparation};

/// Domain separated consensus encoding hasher.
pub struct DomainSeparatedConsensusHasher<M>(PhantomData<M>);

impl<M: DomainSeparation> DomainSeparatedConsensusHasher<M> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(label: &'static str) -> ConsensusHasher<Blake256> {
        let mut digest = Blake256::new();
        M::add_domain_separation_tag(&mut digest, label);
        ConsensusHasher::from_digest(digest)
    }
}

#[derive(Clone)]
pub struct ConsensusHasher<D> {
    writer: WriteHashWrapper<D>,
}

impl<D: Digest> ConsensusHasher<D> {
    fn from_digest(digest: D) -> Self {
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

    pub fn update_consensus_encode<T: BorshSerialize>(&mut self, data: &T) {
        BorshSerialize::serialize(data, &mut self.writer)
            .expect("Incorrect implementation of BorshSerialize encountered. Implementations MUST be infallible.");
    }

    pub fn chain<T: BorshSerialize>(mut self, data: &T) -> Self {
        self.update_consensus_encode(data);
        self
    }
}

impl Default for ConsensusHasher<Blake256> {
    /// This `default` implementation is provided for convenience, but should not be used as the de-facto consensus
    /// hasher, rather create a new unique hash domain.
    fn default() -> Self {
        hash_domain!(
            DefaultConsensusHashDomain,
            "com.tari.base_layer.core.consensus.consensus_encoding.hashing",
            0
        );
        DomainSeparatedConsensusHasher::<DefaultConsensusHashDomain>::new("default")
    }
}

/// This private struct wraps a Digest and implements the Write trait to satisfy the consensus encoding trait.
/// Do not use the DomainSeparatedHasher with this.
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
    use tari_crypto::hash_domain;
    use tari_script::script;

    use super::*;

    hash_domain!(TestHashDomain, "com.tari.test", 0);

    #[test]
    fn it_hashes_using_the_domain_hasher() {
        let mut hasher = Blake256::new();
        TestHashDomain::add_domain_separation_tag(&mut hasher, "foo");

        let expected_hash = hasher.chain(b"\xff\x00\x00\x00\x00\x00\x00\x00").finalize();
        let hash = DomainSeparatedConsensusHasher::<TestHashDomain>::new("foo")
            .chain(&255u64)
            .finalize();

        assert_eq!(hash, expected_hash.as_ref());
    }

    #[test]
    fn it_adds_to_hash_challenge_in_complete_chunks() {
        // Script is chosen because the consensus encoding impl for TariScript has 2 writes
        let test_subject = script!(Nop);
        let mut hasher = Blake256::new();
        TestHashDomain::add_domain_separation_tag(&mut hasher, "foo");

        let expected_hash = hasher.chain(b"\x01\x73").finalize();
        let hash = DomainSeparatedConsensusHasher::<TestHashDomain>::new("foo")
            .chain(&test_subject)
            .finalize();

        assert_eq!(hash, expected_hash.as_ref());
    }

    #[test]
    fn default_consensus_hash_is_not_blake256_default_hash() {
        let blake256_hasher = Blake256::new();
        let blake256_hash = blake256_hasher.chain(b"").finalize();

        let default_consensus_hasher = ConsensusHasher::default();
        let default_consensus_hash = default_consensus_hasher.chain(b"").finalize();

        assert_ne!(blake256_hash.as_slice(), default_consensus_hash.as_slice());
    }
}
