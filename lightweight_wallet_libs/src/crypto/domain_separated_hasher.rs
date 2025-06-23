// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Domain separated hashing implementation
//! 
//! This provides domain separation for cryptographic operations to prevent
//! cross-protocol attacks.

extern crate alloc;

use alloc::{format, string::String};
use core::marker::PhantomData;
use digest::{Digest, OutputSizeUser, Update, FixedOutput, FixedOutputReset, Output};
use blake2::Blake2b;
use digest::consts::U64;

/// The `DomainSeparation` trait is used to inject domain separation tags into the [`DomainSeparatedHasher`] in a
/// way that can be applied consistently, but without hard-coding anything into the hasher itself.
pub trait DomainSeparation {
    /// Returns the version number for the metadata tag
    fn version() -> u8;

    /// Returns the category label for the metadata tag. For example, `tari_hmac`
    fn domain() -> &'static str;

    /// The domain separation tag is defined as `{domain}.v{version}.{label}`, where the version and tag are
    /// typically hard-coded into the implementing type, and the label is provided per specific application of the
    /// domain
    fn domain_separation_tag<S: AsRef<str>>(label: S) -> String {
        if !label.as_ref().is_empty() {
            return format!("{}.v{}.{}", Self::domain(), Self::version(), label.as_ref());
        }
        format!("{}.v{}", Self::domain(), Self::version())
    }

    /// Adds the domain separation tag to the given digest. The domain separation tag is defined as
    /// `{domain}.v{version}.{label}`, where the version and tag are typically hard-coded into the implementing
    /// type, and the label is provided per specific application of the domain.
    fn add_domain_separation_tag<S: AsRef<[u8]>, D: Digest>(digest: &mut D, label: S) {
        let label = if label.as_ref().is_empty() { &[] } else { label.as_ref() };
        let domain = Self::domain();
        let (version_offset, version) = byte_to_decimal_ascii_bytes(Self::version());
        let len = if label.is_empty() {
            // 2 additional bytes are 1 x '.' delimiters and 'v' tag for version
            domain.len() + (3 - version_offset) + 2
        } else {
            // 3 additional bytes are 2 x '.' delimiters and 'v' tag for version
            domain.len() + (3 - version_offset) + label.len() + 3
        };
        let len = (len as u64).to_le_bytes();
        digest.update(len);
        digest.update(domain);
        digest.update(b".v");
        digest.update(&version[version_offset..]);
        if !label.is_empty() {
            digest.update(b".");
            digest.update(label);
        }
    }
}

/// Converts a byte value to ASCII bytes that represent its value in big-endian order.
fn byte_to_decimal_ascii_bytes(mut byte: u8) -> (usize, [u8; 3]) {
    const ZERO_ASCII_CHAR: u8 = 48;
    // A u8 can only ever be a 3 char number.
    let mut bytes = [0u8, 0u8, ZERO_ASCII_CHAR];
    let mut pos = 3usize;
    if byte == 0 {
        return (2, bytes);
    }
    while byte > 0 {
        let rem = byte % 10;
        byte /= 10;
        bytes[pos - 1] = ZERO_ASCII_CHAR + rem;
        pos -= 1;
    }
    (pos, bytes)
}

/// A hash value, guaranteed, as far as possible, to have been created using a hash function that has been randomly and
/// independently sampled from an idealized set of hash functions.
pub struct DomainSeparatedHash<D: Digest> {
    output: digest::Output<D>,
}

impl<D: Digest> DomainSeparatedHash<D> {
    fn new(output: digest::Output<D>) -> Self {
        Self { output }
    }
}

impl<D: Digest> AsRef<[u8]> for DomainSeparatedHash<D> {
    fn as_ref(&self) -> &[u8] {
        self.output.as_slice()
    }
}

/// A wrapper for hash digest algorithms that produces [`DomainSeparatedHash`] instances.
#[derive(Debug, Clone, Default)]
pub struct DomainSeparatedHasher<D, M> {
    inner: D,
    label: &'static str,
    _dst: PhantomData<M>,
}

impl<D: Digest, M: DomainSeparation> DomainSeparatedHasher<D, M> {
    /// Create a new instance of [`DomainSeparatedHasher`] without an additional label.
    pub fn new() -> Self {
        Self::new_with_label("")
    }

    /// Create a new instance of [`DomainSeparatedHasher`] for the given label.
    pub fn new_with_label(label: &'static str) -> Self {
        let mut inner = D::new();
        M::add_domain_separation_tag(&mut inner, label);
        Self {
            inner,
            label,
            _dst: PhantomData,
        }
    }

    /// Adds the data to the digest function by first appending the length of the data in the byte array, and then
    /// supplying the data itself.
    pub fn update(&mut self, data: impl AsRef<[u8]>) {
        let bytes = data.as_ref();
        let len = (bytes.len() as u64).to_le_bytes();
        self.inner.update(len);
        self.inner.update(bytes);
    }

    /// Does the same thing as [`Self::update`], but returns the hasher instance to support fluent syntax.
    #[must_use]
    pub fn chain(mut self, data: impl AsRef<[u8]>) -> Self {
        self.update(data);
        self
    }

    /// Finalize the hasher and return the hash result.
    pub fn finalize(self) -> DomainSeparatedHash<D> {
        let output = self.inner.finalize();
        DomainSeparatedHash::new(output)
    }

    /// A convenience function to update, then finalize the hasher and return the hash result.
    pub fn digest(mut self, data: &[u8]) -> DomainSeparatedHash<D> {
        self.update(data);
        self.finalize()
    }
}

impl<D: Digest, M: DomainSeparation> PartialEq for DomainSeparatedHasher<D, M> {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
    }
}

impl<D: Digest, M: DomainSeparation> Eq for DomainSeparatedHasher<D, M> {}

impl<TInnerDigest: Update, TDomain: DomainSeparation> Update for DomainSeparatedHasher<TInnerDigest, TDomain> {
    fn update(&mut self, data: &[u8]) {
        let bytes = data;
        let len = (bytes.len() as u64).to_le_bytes();
        Update::update(&mut self.inner, &len);
        Update::update(&mut self.inner, bytes);
    }
}

impl<TInnerDigest: OutputSizeUser, TDomain: DomainSeparation> OutputSizeUser
    for DomainSeparatedHasher<TInnerDigest, TDomain>
{
    type OutputSize = TInnerDigest::OutputSize;
}

impl<TInnerDigest: FixedOutput, TDomain: DomainSeparation> FixedOutput
    for DomainSeparatedHasher<TInnerDigest, TDomain>
{
    fn finalize_into(self, out: &mut Output<Self>) {
        self.inner.finalize_into(out);
    }
}

impl<D: FixedOutputReset, M: DomainSeparation> DomainSeparatedHasher<D, M> {
    /// Finalize and reset the hasher and return the hash result.
    pub fn finalize_into_reset(&mut self, out: &mut Output<Self>) {
        self.inner.finalize_into_reset(out);
    }
}

// Implements Digest so that it can be used for other crates
impl<TInnerDigest: Digest + FixedOutputReset, TDomain: DomainSeparation> Digest
    for DomainSeparatedHasher<TInnerDigest, TDomain>
{
    fn new() -> Self {
        DomainSeparatedHasher::<TInnerDigest, TDomain>::new()
    }

    // Create new hasher instance which has processed the provided data.
    fn new_with_prefix(data: impl AsRef<[u8]>) -> Self {
        let hasher = DomainSeparatedHasher::<TInnerDigest, TDomain>::new();
        hasher.chain_update(data)
    }

    fn update(&mut self, data: impl AsRef<[u8]>) {
        let bytes = data.as_ref();
        let len = (bytes.len() as u64).to_le_bytes();
        Update::update(&mut self.inner, &len);
        Update::update(&mut self.inner, bytes);
    }

    fn chain_update(self, data: impl AsRef<[u8]>) -> Self
    where Self: Sized {
        self.chain(data)
    }

    fn finalize(self) -> Output<Self> {
        self.finalize().output
    }

    fn finalize_reset(&mut self) -> Output<Self> {
        let value = self.inner.finalize_reset();
        TDomain::add_domain_separation_tag(&mut self.inner, self.label);
        value
    }

    fn finalize_into_reset(&mut self, out: &mut Output<Self>) {
        Digest::finalize_into_reset(&mut self.inner, out);
    }

    // Write result into provided array and consume the hasher instance.
    fn finalize_into(self, out: &mut Output<Self>) {
        Digest::finalize_into(self.inner, out);
    }

    fn reset(&mut self) {
        Digest::reset(&mut self.inner);
        TDomain::add_domain_separation_tag(&mut self.inner, self.label);
    }

    fn output_size() -> usize {
        <TInnerDigest as Digest>::output_size()
    }

    fn digest(data: impl AsRef<[u8]>) -> Output<Self> {
        let mut hasher = Self::new();
        hasher.update(data);
        hasher.finalize().output
    }
}

/// Key manager domain for domain separation
pub struct KeyManagerDomain;

impl DomainSeparation for KeyManagerDomain {
    fn version() -> u8 {
        1
    }

    fn domain() -> &'static str {
        "tari_key_manager"
    }
} 