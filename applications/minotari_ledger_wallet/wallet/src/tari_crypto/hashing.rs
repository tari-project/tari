
use alloc::string::String;

use blake2::{Blake2b, Blake2bVar};
use digest::{
    consts::{U32, U64},
    Digest,
    Update,
};
use digest::OutputSizeUser;
use digest::FixedOutput;
use digest::Output;
use digest::FixedOutputReset;

use core::marker::PhantomData;
use crate::hashing::DomainSeparatedHash;

use alloc::format;

//
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
//
// pub struct DomainSeparatedHash<D: Digest> {
//     output: Output<D>,
// }
//
// impl<D: Digest> DomainSeparatedHash<D> {
//     // This constructor is intentionally private. It should be impossible to create an instance of this struct without
//     // the guarantees that the data represents a hash containing the domain separation label provided in `M`
//     fn new(output: Output<D>) -> Self {
//         Self { output }
//     }
// }
//
// impl<D: Digest> AsRef<[u8]> for DomainSeparatedHash<D> {
//     fn as_ref(&self) -> &[u8] {
//         self.output.as_slice()
//     }
// }
//
#[derive(Debug, Clone, Default)]
pub struct DomainSeparatedHasher<D, M> {
    inner: D,
    label: &'static str,
    _dst: PhantomData<M>,
}

impl<D: Digest, M: DomainSeparation> DomainSeparatedHasher<D, M> {
    /// Create a new instance of [`DomainSeparatedHasher`] without an additional label (to correspond to 'D::new()').
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
        let len = (data.as_ref().len() as u64).to_le_bytes();
        self.inner.update(len);
        self.inner.update(data);
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
//
/// Convert a finalized hash into a fixed size buffer.
pub trait AsFixedBytes<const I: usize>: AsRef<[u8]> {
    /// A convenience function to convert a finalized hash into a fixed size buffer.
    fn as_fixed_bytes(&self) -> Result<[u8; I], SliceError> {
        let hash_vec = self.as_ref();
        if hash_vec.is_empty() || hash_vec.len() < I {
            let hash_vec_length = if hash_vec.is_empty() { 0 } else { hash_vec.len() };
            return Err(SliceError::CopyFromSlice {
                target: I,
                provided: hash_vec_length,
            });
        }
        let mut buffer: [u8; I] = [0; I];
        buffer.copy_from_slice(&hash_vec[..I]);
        Ok(buffer)
    }
}

impl<TInnerDigest: OutputSizeUser, TDomain: DomainSeparation> OutputSizeUser
for DomainSeparatedHasher<TInnerDigest, TDomain>
{
    type OutputSize = TInnerDigest::OutputSize;
}
//
impl<TInnerDigest: Update, TDomain: DomainSeparation> Update for DomainSeparatedHasher<TInnerDigest, TDomain> {
    fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }
}

impl<const I: usize, D: Digest> AsFixedBytes<I> for DomainSeparatedHash<D> {}

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
        self.update(data);
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

//----------------------------------------       Extra marker traits      ----------------------------------------------

/// A marker trait for Digest algorithms that are not susceptible to length-extension attacks.
///
/// Notably, the SHA-2 family does *not* have this trait.
pub trait LengthExtensionAttackResistant {}

impl LengthExtensionAttackResistant for Blake2bVar {}


impl LengthExtensionAttackResistant for Blake2b<U32> {}

impl LengthExtensionAttackResistant for Blake2b<U64> {}

//------------------------------------------------    HMAC  ------------------------------------------------------------
/// A domain separation tag for use in MAC derivation algorithms.
pub struct MacDomain;

impl DomainSeparation for MacDomain {
    fn version() -> u8 {
        1
    }

    fn domain() -> &'static str {
        "com.tari.mac"
    }
}

//
// pub struct Mac<D: Digest> {
//     hmac: DomainSeparatedHash<D>,
// }
//
// impl<D> Mac<D>
// where D: Digest + Update + LengthExtensionAttackResistant
// {
//     /// Generate a MAC with the given (length extension attack resistant) digest function, shared key, message and
//     /// application label.
//     pub fn generate<K, S>(key: K, msg: S, label: &'static str) -> Self
//     where
//         K: AsRef<[u8]>,
//         S: AsRef<[u8]>,
//     {
//         let hmac = DomainSeparatedHasher::<D, MacDomain>::new_with_label(label)
//             .chain(key.as_ref())
//             .chain(msg.as_ref())
//             .finalize();
//         Self { hmac }
//     }
// }
//
// impl<D: Digest> Deref for Mac<D> {
//     type Target = DomainSeparatedHash<D>;
//
//     fn deref(&self) -> &Self::Target {
//         &self.hmac
//     }
// }


// pub trait DerivedKeyDomain: DomainSeparation {
//     /// The associated derived secret key type
//     type DerivedKeyType: SecretKey;
//
//     /// Derive a key from the input key using a suitable domain separation tag and the given application label by wide
//     /// reduction. An error is returned if the supplied primary key isn't at least as long as the derived key.
//     /// If the digest's output size is not sufficient to generate the derived key type, then an error will be thrown.
//     fn generate<D>(primary_key: &[u8], data: &[u8], label: &'static str) -> Result<Self::DerivedKeyType, HashingError>
//     where
//         Self: Sized,
//         D: Digest + Update,
//     {
//         // Ensure the primary key is at least as long as the derived key
//         if primary_key.len() < <Self::DerivedKeyType as SecretKey>::KEY_LEN {
//             return Err(HashingError::InputTooShort {});
//         }
//
//         // Ensure the digest length is suitable for wide reduction
//         if <D as Digest>::output_size() != <Self::DerivedKeyType as SecretKey>::WIDE_REDUCTION_LEN {
//             return Err(HashingError::InputTooShort {});
//         }
//
//         let hash = DomainSeparatedHasher::<D, Self>::new_with_label(label)
//             .chain(primary_key)
//             .chain(data)
//             .finalize();
//         let derived_key = Self::DerivedKeyType::from_uniform_bytes(hash.as_ref())
//             .map_err(|e| HashingError::ConversionFromBytes { reason: e.to_string() })?;
//         Ok(derived_key)
//     }
// }

/// Creates a DomainSeparation struct for a given domain.
#[macro_export]
macro_rules! hash_domain {
    ($name:ident, $domain:expr, $version: expr) => {
        /// A hashing domain instance
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name;

        impl $crate::tari_crypto::hashing::DomainSeparation for $name {
            fn version() -> u8 {
                $version
            }

            fn domain() -> &'static str {
                $domain
            }
        }
    };
    ($name:ident, $domain:expr) => {
        hash_domain!($name, $domain, 1);
    };
}
//
// /// Creates a domain separated hasher type and domain in one
// #[macro_export]
// macro_rules! hasher {
//     ($digest:ty, $name:ident, $domain:expr, $version: expr, $mod_name:ident) => {
//         mod $mod_name {
//             use $crate::hash_domain;
//
//             hash_domain!(__HashDomain, $domain, $version);
//         }
//         pub type $name = $crate::hashing::DomainSeparatedHasher<$digest, $mod_name::__HashDomain>;
//     };
//     ($digest: ty, $name:ident, $domain:expr, $version: expr) => {
//         hasher!($digest, $name, $domain, $version, __inner_hasher_impl);
//     };
//     ($digest: ty, $name:ident, $domain:expr) => {
//         hasher!($digest, $name, $domain, 1, __inner_hasher_impl);
//     };
// }
//
// /// Convenience function for creating a DomainSeparatedHasher with an added label
// pub fn create_hasher_with_label<D: Digest, HD: DomainSeparation>(label: &'static str) -> DomainSeparatedHasher<D, HD> {
//     DomainSeparatedHasher::<D, HD>::new_with_label(label)
// }
//
// /// Convenience function for creating a DomainSeparatedHasher
// pub fn create_hasher<D: Digest, HD: DomainSeparation>() -> DomainSeparatedHasher<D, HD> {
//     DomainSeparatedHasher::<D, HD>::new()
// }


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SliceError {
    /// The requested fixed slice length exceeds the available slice length
    CopyFromSlice {
        /// The requested fixed slice length
        target: usize,
        /// The available slice length
        provided: usize,
    },
}