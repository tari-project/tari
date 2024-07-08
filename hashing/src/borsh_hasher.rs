// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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

use core::marker::PhantomData;

use borsh::{io, io::Write, BorshSerialize};
use digest::Digest;
use tari_crypto::hashing::DomainSeparation;

/// A domain-separated hasher that uses Borsh internally to ensure hashing is canonical.
///
/// This assumes that any input type supports `BorshSerialize` canonically; that is, two different values of the same
/// type must serialize distinctly.
pub struct DomainSeparatedBorshHasher<M, D> {
    writer: WriteHashWrapper<D>,
    _m: PhantomData<M>,
}

impl<D: Digest + Default, M: DomainSeparation> DomainSeparatedBorshHasher<M, D> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new_with_label(label: &str) -> Self {
        let mut digest = D::default();
        M::add_domain_separation_tag(&mut digest, label);
        Self {
            writer: WriteHashWrapper(digest),
            _m: PhantomData,
        }
    }

    pub fn finalize(self) -> digest::Output<D> {
        self.writer.0.finalize()
    }

    /// Update the hasher using the Borsh encoding of the input, which is assumed to be canonical.
    pub fn update_consensus_encode<T: BorshSerialize>(&mut self, data: &T) {
        BorshSerialize::serialize(data, &mut self.writer)
            .expect("Incorrect implementation of BorshSerialize encountered. Implementations MUST be infallible.");
    }

    pub fn chain<T: BorshSerialize>(mut self, data: &T) -> Self {
        self.update_consensus_encode(data);
        self
    }
}

/// This private struct wraps a Digest and implements the Write trait to satisfy the consensus encoding trait.
///
/// It's important not to use `DomainSeparatedHasher` with this, since that can inconsistently handle length prepending
/// and render hashing inconsistent. It's fine to use it with `DomainSeparatedBorshHasher`.
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
    extern crate alloc;
    use alloc::vec::Vec;

    use blake2::Blake2b;
    use digest::consts::U32;
    use tari_crypto::hash_domain;

    use super::*;

    #[derive(Debug, BorshSerialize)]
    pub struct TestStruct {
        pub a: u64,
        pub b: u64,
    }

    hash_domain!(TestHashDomain, "com.tari.test.test_hash", 0);

    #[test]
    fn label_yields_distinct_hash() {
        let input = [1u8; 32];

        let hash_label1 = DomainSeparatedBorshHasher::<TestHashDomain, Blake2b<U32>>::new_with_label("label1")
            .chain(&input)
            .finalize();

        let hash_label2 = DomainSeparatedBorshHasher::<TestHashDomain, Blake2b<U32>>::new_with_label("label2")
            .chain(&input)
            .finalize();

        // They should be distinct
        assert_ne!(hash_label1, hash_label2);
    }

    #[test]
    fn it_hashes_using_the_domain_hasher() {
        // Script is chosen because the consensus encoding impl for TariScript has 2 writes
        let mut hasher = Blake2b::<U32>::default();
        TestHashDomain::add_domain_separation_tag(&mut hasher, "foo");

        let expected_hash = hasher.chain_update(b"\xff\x00\x00\x00\x00\x00\x00\x00").finalize();
        let hash = DomainSeparatedBorshHasher::<TestHashDomain, Blake2b<U32>>::new_with_label("foo")
            .chain(&255u64)
            .finalize();

        assert_eq!(hash, expected_hash);
    }

    #[test]
    fn it_adds_to_hash_challenge_in_complete_chunks() {
        // The borsh implementation contains 2 writes, 1 per field. See the macro expansion for details.
        let test_subject1 = TestStruct { a: 1, b: 2 };
        let test_subject2 = TestStruct { a: 3, b: 4 };
        let mut hasher = Blake2b::<U32>::default();
        TestHashDomain::add_domain_separation_tag(&mut hasher, "foo");

        let mut buf = Vec::new();
        BorshSerialize::serialize(&test_subject1, &mut buf).unwrap();
        BorshSerialize::serialize(&test_subject2, &mut buf).unwrap();

        // Write to the test hasher as one chunk
        let expected_hash = hasher.chain_update(&buf).finalize();

        // The domain-separated one must do the same
        let hash = DomainSeparatedBorshHasher::<TestHashDomain, Blake2b<U32>>::new_with_label("foo")
            .chain(&test_subject1)
            .chain(&test_subject2)
            .finalize();

        assert_eq!(hash, expected_hash);
    }

    #[test]
    fn default_consensus_hash_is_not_blake_default_hash() {
        let blake_hasher = Blake2b::<U32>::default();
        let blake_hash = blake_hasher.chain_update(b"").finalize();

        let default_consensus_hasher = DomainSeparatedBorshHasher::<TestHashDomain, Blake2b<U32>>::new_with_label("");
        let default_consensus_hash = default_consensus_hasher.chain(b"").finalize();

        assert_ne!(blake_hash.as_slice(), default_consensus_hash.as_slice());
    }
}
