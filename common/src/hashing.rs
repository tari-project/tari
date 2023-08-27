// Copyright 2019. The Taiji Project
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

use sha2::Digest;
use tari_crypto::hashing::{DomainSeparatedHasher, LengthExtensionAttackResistant};

/// A marker trait for tight coupling of DomainSeparatedHasher to Digest; this is useful when a low level hashing
/// function that accepts Digest as type needs to guarantee that domain separated hashing is implemented.
pub trait DomainDigest: Digest {}

impl<D, M> DomainDigest for DomainSeparatedHasher<D, M> where Self: Digest {}

/// A MAC hasher that guarantees implementation of DomainSeparatedHasher and LengthExtensionAttackResistant.
#[allow(dead_code)]
pub fn mac_domain_hasher<D, M>() -> DomainSeparatedHasher<D, M>
where
    D: LengthExtensionAttackResistant,
    DomainSeparatedHasher<D, M>: Digest,
    D: Digest,
{
    DomainSeparatedHasher::<D, M>::new()
}

#[cfg(test)]
mod test {
    use blake2::Blake2b;
    use sha2::{
        digest::{consts::U32, Output},
        Digest,
        Sha256,
    };
    use tari_crypto::{
        hash_domain,
        hashing::{AsFixedBytes, DomainSeparatedHasher},
    };

    use crate::{hashing::mac_domain_hasher, DomainDigest};

    hash_domain!(HashDomain, "com.taiji.test.hash_domain", 1);
    type DomainHasher<D> = DomainSeparatedHasher<D, HashDomain>;

    fn use_as_digest_function<D>(data: &[u8]) -> Output<DomainHasher<D>>
    where D: Digest {
        D::new().chain_update(data).finalize()
    }

    fn use_as_domain_digest_function<D>(data: &[u8]) -> Output<DomainHasher<D>>
    where D: DomainDigest {
        D::new().chain_update(data).finalize()
    }

    #[test]
    fn test_domain_digest() {
        let some_data = b"some data";

        let hashed = use_as_digest_function::<Blake2b<U32>>(some_data);
        let hash_as_digest_1: [u8; 32] = hashed.into();

        let hashed = use_as_digest_function::<DomainHasher<Blake2b<U32>>>(some_data);
        let hash_as_digest_2: [u8; 32] = hashed.into();

        assert_ne!(hash_as_digest_2, hash_as_digest_1);

        let hashed = use_as_domain_digest_function::<DomainHasher<Blake2b<U32>>>(some_data);
        let hash_as_domain_digest: [u8; 32] = hashed.into();

        assert_eq!(hash_as_domain_digest, hash_as_digest_2);

        // The compiler won't even let you write these tests :), so they're commented out.
        // let hashed = use_as_mac_domain_digest_function::<Blake2b<U32>>(some_data);
        //
        // error[E0277]: the trait bound `Blake2b<U32>: MacDomainDigest` is not satisfied
        //     --> common\src\hashing_domain.rs:85:58
        //     |
        //     85 |         let hashed = use_as_mac_domain_digest_function::<Blake2b<U32>>(some_data);
        // |                                                          ^^^^^^^^ the trait `MacDomainDigest` is not
    }

    #[test]
    fn test_mac_domain_digest() {
        hash_domain!(MacDomain, "com.taiji.test.mac_domain.my_function", 1);
        let some_data = b"some data";

        // The 'mac_domain_hasher' introduce specific trait bounds
        let hasher = mac_domain_hasher::<Blake2b<U32>, MacDomain>();
        let hash_from_mac_domain_hasher: [u8; 32] = hasher.digest(some_data).as_fixed_bytes().unwrap();

        // The compiler won't even let you write these tests :), so they're commented out.
        // let hasher = mac_domain_hasher::<Sha256, MacDomain>();
        // error[E0277]: the trait bound `Sha256: LengthExtensionAttackResistant` is not satisfied
        //     --> common\src\hashing.rs:117:42
        //     |
        //     117 |         let hasher = mac_domain_hasher::<Sha256, MacDomain>();
        // |                                          ^^^^^^ the trait `LengthExtensionAttackResistant` is not
        // implemented for `Sha256`

        // A custom domain separated hasher can be created that gives the same results...
        let hasher = DomainSeparatedHasher::<Blake2b<U32>, MacDomain>::new();
        let hash_from_custom_hasher_1: [u8; 32] = hasher.digest(some_data).as_fixed_bytes().unwrap();

        assert_eq!(hash_from_mac_domain_hasher, hash_from_custom_hasher_1);

        // ... but quite easily not adhering to the desired mac properties
        let hasher = DomainSeparatedHasher::<Sha256, MacDomain>::new();
        let hash_from_custom_hasher_2: [u8; 32] = hasher.digest(some_data).as_fixed_bytes().unwrap();

        assert_ne!(hash_from_custom_hasher_2, hash_from_custom_hasher_1);
    }
}
