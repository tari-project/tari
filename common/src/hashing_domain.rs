// Copyright 2019. The Tari Project
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

use sha2::{Digest, digest::Update};
use tari_common_types::types::{DefaultDomainHasher, MacDomainHasher};
use tari_crypto::hashing::{DomainSeparatedHash, LengthExtensionAttackResistant, Mac};
use thiserror::Error;

pub struct HashingDomain {
    domain_label: &'static str,
}

/// Error type for the pipeline.
#[derive(Debug, Error)]
pub enum HashingDomainError {
    #[error("Two slices have different lengths")]
    CopyFromSlice,
}

impl HashingDomain {
    /// A new constant hashing domain with domain label
    pub const fn new(domain_label: &'static str) -> Self {
        Self { domain_label }
    }

    /// A new generic domain separated hasher for the chosen hashing domain
    pub fn hasher<D: Digest>(&self) -> DefaultDomainHasher<D> {
        DefaultDomainHasher::new(self.domain_label)
    }

    /// Convenience function to compute hash of the data. It will handle hasher creation, data feeding and finalization.
    pub fn digest<D: Digest>(&self, data: &[u8]) -> DomainSeparatedHash<D> {
        self.hasher::<D>().chain(data).finalize()
    }

    /// A new MAC domain separated hasher for the chosen hashing domain - can be used for custom MAC hashing
    pub fn mac_hasher<D: Digest + LengthExtensionAttackResistant>(&self) -> MacDomainHasher<D> {
        MacDomainHasher::new(self.domain_label)
    }

    /// Convenience function to compute hash of the data. It will handle hasher creation, data feeding and finalization.
    pub fn mac_digest<D: Digest + LengthExtensionAttackResistant>(&self, data: &[u8]) -> DomainSeparatedHash<D> {
        self.mac_hasher::<D>().chain(data).finalize()
    }

    /// Generate a finalized domain separated Hash-based Message Authentication Code (HMAC) for the key and message
    pub fn generate_hmac<D: Digest + LengthExtensionAttackResistant + Update>(&self, key: &[u8], msg: &[u8]) -> Mac<D> {
        Mac::generate::<_, _>(key, msg, self.domain_label)
    }
}

pub trait HashToBytes<const I: usize>: AsRef<[u8]> {
    fn hash_to_bytes(&self) -> Result<[u8; I], HashingDomainError> {
        let hash_vec = self.as_ref();
        if hash_vec.is_empty() || hash_vec.len() < I {
            return Err(HashingDomainError::CopyFromSlice);
        }
        let mut buffer: [u8; I] = [0; I];
        buffer.copy_from_slice(&hash_vec[..I]);
        Ok(buffer)
    }
}

impl<const I: usize, D: Digest> HashToBytes<I> for DomainSeparatedHash<D> {}

#[cfg(test)]
mod test {
    use tari_crypto::{hash::blake2::Blake256, tari_utilities::hex::Hex};

    use crate::{common_hash_domain, hashing_domain::HashToBytes};

    #[test]
    fn test_generic_domain_hasher() {
        let mut hasher = common_hash_domain().hasher::<Blake256>();
        hasher.update(b"my 1st secret");
        hasher.update(b"my 2nd secret");
        let hash = hasher.finalize();

        let hash_to_bytes_7: [u8; 7] = hash.hash_to_bytes().unwrap();
        assert_eq!(hash_to_bytes_7, hash.hash_to_bytes().unwrap());
        let hash_to_bytes_23: [u8; 23] = hash.hash_to_bytes().unwrap();
        assert_eq!(hash_to_bytes_23, hash.hash_to_bytes().unwrap());
        let hash_to_bytes_32: [u8; 32] = hash.hash_to_bytes().unwrap();
        assert_eq!(hash_to_bytes_32, hash.hash_to_bytes().unwrap());

        let mut hasher = common_hash_domain().hasher::<Blake256>();
        hasher.update(b"my 3rd secret");
        let hash_1 = hasher.finalize();
        let hash_2 = common_hash_domain().digest::<Blake256>(b"my 3rd secret");
        assert_eq!(hash_1.as_ref(), hash_2.as_ref());
        assert_eq!(hash_1.domain_separation_tag(), hash_2.domain_separation_tag());
        assert_eq!(hash_1.domain_separation_tag(), hash.domain_separation_tag());
    }

    #[test]
    fn test_mac_domain_hasher() {
        // The compiler won't even let you write these tests :), so they're commented out.
        // let mut hasher = COMMON_HASH_DOMAIN.mac_hasher::<Sha256>();
        //
        //     error[E0277]: the trait bound `Sha256: LengthExtensionAttackResistant` is not satisfied
        //         --> common\src\hashing_domain.rs:121:41
        //         |
        //         121 |         let mut hasher = common_hash_domain().mac_hasher::<Sha256>();
        //     |                                            ^^^^^^^^^^ the trait `LengthExtensionAttackResistant` is not
        //     |                                                       implemented for `Sha256`
        //     |
        //     = help: the following other types implement trait `LengthExtensionAttackResistant`:
        //     Blake256
        //     blake2::blake2b::VarBlake2b
        //     sha3::Sha3_256
        //     note: required by a bound in `HashingDomain::mac_hasher`

        let mut hasher = common_hash_domain().mac_hasher::<Blake256>();
        hasher.update(b"my 1st secret");
        hasher.update(b"my 2nd secret");
        let hash = hasher.finalize();

        let hash_to_bytes_7: [u8; 7] = hash.hash_to_bytes().unwrap();
        assert_eq!(hash_to_bytes_7, hash.hash_to_bytes().unwrap());
        let hash_to_bytes_23: [u8; 23] = hash.hash_to_bytes().unwrap();
        assert_eq!(hash_to_bytes_23, hash.hash_to_bytes().unwrap());
        let hash_to_bytes_32: [u8; 32] = hash.hash_to_bytes().unwrap();
        assert_eq!(hash_to_bytes_32, hash.hash_to_bytes().unwrap());

        let mut hasher = common_hash_domain().mac_hasher::<Blake256>();
        hasher.update(b"my 3rd secret");
        let hash_1 = hasher.finalize();
        let hash_2 = common_hash_domain().mac_digest::<Blake256>(b"my 3rd secret");
        assert_eq!(hash_1.as_ref(), hash_2.as_ref());
        assert_eq!(hash_1.domain_separation_tag(), hash_2.domain_separation_tag());
        assert_eq!(hash_1.domain_separation_tag(), hash.domain_separation_tag());

        let hmac = common_hash_domain().generate_hmac::<Blake256>(b"my secret key", b"my message");
        assert_ne!(hmac.domain_separation_tag(), hash_1.domain_separation_tag());
        assert_eq!(
            hmac.into_vec().to_hex(),
            "412767200f4b3bcfbf02bdd556d6fad33be176b06bdcbb00963bd3cb51b5dc79"
        );
    }

    #[test]
    fn test_domain_separation() {
        let secret = b"my secret";
        let hash_generic = common_hash_domain().digest::<Blake256>(secret);
        let hash_mac = common_hash_domain().mac_digest::<Blake256>(secret);
        assert_ne!(hash_generic.as_ref(), hash_mac.as_ref());
        assert_ne!(hash_generic.domain_separation_tag(), hash_mac.domain_separation_tag());
        assert_eq!(
            hash_generic.domain_separation_tag(),
            "com.tari.tari_project.hash_domain.v1.common"
        );
        assert_eq!(
            hash_mac.domain_separation_tag(),
            "com.tari.tari_project.mac_domain.v1.common"
        );
    }
}
