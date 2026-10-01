// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use digest::{Digest, FixedOutput, Output};
use tari_crypto::hashing::{DomainSeparatedHasher, DomainSeparation};
use zeroize::Zeroizing;

/// Zeroizing finalize for [`DomainSeparatedHasher`], for digests that are key material (KDF uses).
///
/// `DomainSeparatedHasher::finalize` returns a `DomainSeparatedHash` that is not zeroized on drop, so a derived key
/// would linger in memory after use. `finalize_zeroizing` writes the digest straight into a [`Zeroizing`] buffer
/// instead. For a caller-supplied target (e.g. a `SafeArray`) use `FixedOutput::finalize_into`.
///
/// Residual: only the output is zeroized. The internal state of the underlying digest (blake2 0.10 does not zeroize
/// its state) is dropped without being cleared, so the absorbed input may remain in freed memory.
pub trait ZeroizingFinalize {
    type Digest: Digest;

    fn finalize_zeroizing(self) -> Zeroizing<Output<Self::Digest>>;
}

impl<D: Digest + FixedOutput, M: DomainSeparation> ZeroizingFinalize for DomainSeparatedHasher<D, M> {
    type Digest = D;

    fn finalize_zeroizing(self) -> Zeroizing<Output<D>> {
        let mut out = Zeroizing::new(Output::<D>::default());
        FixedOutput::finalize_into(self, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use blake2::Blake2b;
    use digest::consts::U64;
    use tari_crypto::hash_domain;

    use super::*;

    hash_domain!(TestHashDomain, "com.tari.test.zeroizing", 0);

    #[test]
    fn it_matches_finalize() {
        let hasher = DomainSeparatedHasher::<Blake2b<U64>, TestHashDomain>::new_with_label("test").chain(b"data");
        let expected = hasher.clone().finalize();
        let out = hasher.finalize_zeroizing();
        assert_eq!(out.as_slice(), expected.as_ref());
    }
}
