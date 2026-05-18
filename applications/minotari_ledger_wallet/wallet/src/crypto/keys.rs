// Copyright 2025. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The Tari-compatible implementation of Ristretto based on the curve25519-dalek implementation
use alloc::{format, string::ToString, vec::Vec};
use core::{
    borrow::Borrow,
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    ops::{Add, Mul, Sub},
};

use blake2::Blake2b;
use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_TABLE,
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
};
use digest::{consts::U64, Digest};
use rand_core::{CryptoRng, RngCore};
use subtle::ConstantTimeEq;
use tari_utilities::{hex::Hex, ByteArray, ByteArrayError, Hashable};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

macro_rules! define_add_variants {
    (LHS = $lhs:ty, RHS = $rhs:ty, Output = $out:ty) => {
        impl<'b> Add<&'b $rhs> for $lhs {
            type Output = $out;

            fn add(self, rhs: &'b $rhs) -> $out {
                &self + rhs
            }
        }

        impl<'a> Add<$rhs> for &'a $lhs {
            type Output = $out;

            fn add(self, rhs: $rhs) -> $out {
                self + &rhs
            }
        }

        impl Add<$rhs> for $lhs {
            type Output = $out;

            fn add(self, rhs: $rhs) -> $out {
                &self + &rhs
            }
        }
    };
}

/// Add variations for `Sub` definitions, similar to those for `Add`
macro_rules! define_sub_variants {
    (LHS = $lhs:ty, RHS = $rhs:ty, Output = $out:ty) => {
        impl<'b> Sub<&'b $rhs> for $lhs {
            type Output = $out;

            fn sub(self, rhs: &'b $rhs) -> $out {
                &self - rhs
            }
        }

        impl<'a> Sub<$rhs> for &'a $lhs {
            type Output = $out;

            fn sub(self, rhs: $rhs) -> $out {
                self - &rhs
            }
        }

        impl Sub<$rhs> for $lhs {
            type Output = $out;

            fn sub(self, rhs: $rhs) -> $out {
                &self - &rhs
            }
        }
    };
}

/// Add variations for `Mul` definitions, similar to those for `Add`
macro_rules! define_mul_variants {
    (LHS = $lhs:ty, RHS = $rhs:ty, Output = $out:ty) => {
        impl<'b> Mul<&'b $rhs> for $lhs {
            type Output = $out;

            fn mul(self, rhs: &'b $rhs) -> $out {
                &self * rhs
            }
        }

        impl<'a> Mul<$rhs> for &'a $lhs {
            type Output = $out;

            fn mul(self, rhs: $rhs) -> $out {
                self * &rhs
            }
        }

        impl Mul<$rhs> for $lhs {
            type Output = $out;

            fn mul(self, rhs: $rhs) -> $out {
                &self * &rhs
            }
        }
    };
}

#[derive(Clone, Default, Zeroize, ZeroizeOnDrop)]
pub struct RistrettoSecretKey(pub(crate) Scalar);

impl RistrettoSecretKey {}

impl borsh::BorshSerialize for RistrettoSecretKey {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.as_bytes(), writer)
    }
}

impl borsh::BorshDeserialize for RistrettoSecretKey {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, borsh::io::Error>
    where R: borsh::io::Read {
        let bytes: Zeroizing<Vec<u8>> = Zeroizing::new(borsh::BorshDeserialize::deserialize_reader(reader)?);
        Self::from_canonical_bytes(bytes.as_slice())
            .map_err(|e| borsh::io::Error::new(borsh::io::ErrorKind::InvalidInput, e.to_string()))
    }
}

//-------------------------------------  Ristretto Secret Key ByteArray  ---------------------------------------------//

impl ByteArray for RistrettoSecretKey {
    /// Return a secret key computed from a canonical byte array
    /// If the byte array is not exactly 32 bytes, returns an error
    /// If the byte array does not represent a canonical encoding, returns an error
    fn from_canonical_bytes(bytes: &[u8]) -> Result<RistrettoSecretKey, ByteArrayError>
    where Self: Sized {
        if bytes.len() != Self::KEY_LEN {
            return Err(ByteArrayError::IncorrectLength {});
        }

        let mut bytes_copied = [0u8; 32];
        bytes_copied.copy_from_slice(bytes);
        let scalar = Option::<Scalar>::from(Scalar::from_canonical_bytes(bytes_copied)).ok_or(
            ByteArrayError::ConversionError {
                reason: ("Invalid canonical scalar byte array".to_string()),
            },
        )?;
        bytes_copied.zeroize();

        Ok(RistrettoSecretKey(scalar))
    }

    /// Return the byte array for the secret key in little-endian order
    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Hash for RistrettoSecretKey {
    /// Require the implementation of the Hash trait for Hashmaps
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl PartialEq for RistrettoSecretKey {
    fn eq(&self, other: &Self) -> bool {
        self.ct_eq(other).into()
    }
}

impl Eq for RistrettoSecretKey {}

impl ConstantTimeEq for RistrettoSecretKey {
    fn ct_eq(&self, other: &Self) -> subtle::Choice {
        self.0.ct_eq(&other.0)
    }
}

//----------------------------------   RistrettoSecretKey Debug --------------------------------------------//
impl fmt::Debug for RistrettoSecretKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RistrettoSecretKey(***)")
    }
}

impl RistrettoSecretKey {
    const KEY_LEN: usize = 32;
    const WIDE_REDUCTION_LEN: usize = 64;

    /// Get the multiplicative inverse of a nonzero secret key
    /// If zero is passed, returns `None`; annoying, but a useful guardrail
    pub fn invert(&self) -> Option<Self> {
        if self.0 == Scalar::ZERO {
            None
        } else {
            Some(RistrettoSecretKey(self.0.invert()))
        }
    }

    /// Return a random secret key on the `ristretto255` curve using the supplied CSPRNG.
    pub fn random<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        RistrettoSecretKey(Scalar::random(rng))
    }

    /// Return a secret key computed from a uniform byte slice using wide reduction
    /// If the byte array is not exactly 64 bytes, returns an error
    pub fn from_uniform_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        if bytes.len() != Self::WIDE_REDUCTION_LEN {
            return Err(ByteArrayError::IncorrectLength {});
        }

        let mut bytes_copied = Zeroizing::new([0u8; Self::WIDE_REDUCTION_LEN]);
        bytes_copied.copy_from_slice(bytes);

        Ok(RistrettoSecretKey(Scalar::from_bytes_mod_order_wide(&bytes_copied)))
    }

    pub fn key_length() -> usize {
        Self::KEY_LEN
    }
}

impl ConstantTimeEq for RistrettoPublicKey {
    fn ct_eq(&self, other: &Self) -> subtle::Choice {
        self.point.ct_eq(&other.point)
    }
}

//----------------------------------   RistrettoSecretKey Mul / Add / Sub --------------------------------------------//

impl<'b> Mul<&'b RistrettoPublicKey> for &RistrettoSecretKey {
    type Output = RistrettoPublicKey;

    fn mul(self, rhs: &'b RistrettoPublicKey) -> RistrettoPublicKey {
        let p = self.0 * rhs.point;
        RistrettoPublicKey::new_from_pk(p)
    }
}

impl<'b> Add<&'b RistrettoSecretKey> for &RistrettoSecretKey {
    type Output = RistrettoSecretKey;

    fn add(self, rhs: &'b RistrettoSecretKey) -> RistrettoSecretKey {
        let k = self.0 + rhs.0;
        RistrettoSecretKey(k)
    }
}

impl<'b> Sub<&'b RistrettoSecretKey> for &RistrettoSecretKey {
    type Output = RistrettoSecretKey;

    fn sub(self, rhs: &'b RistrettoSecretKey) -> RistrettoSecretKey {
        RistrettoSecretKey(self.0 - rhs.0)
    }
}

define_add_variants!(
    LHS = RistrettoSecretKey,
    RHS = RistrettoSecretKey,
    Output = RistrettoSecretKey
);
define_sub_variants!(
    LHS = RistrettoSecretKey,
    RHS = RistrettoSecretKey,
    Output = RistrettoSecretKey
);
define_mul_variants!(
    LHS = RistrettoSecretKey,
    RHS = RistrettoPublicKey,
    Output = RistrettoPublicKey
);

//---------------------------------------------      Conversions     -------------------------------------------------//

impl From<u64> for RistrettoSecretKey {
    fn from(v: u64) -> Self {
        let s = Scalar::from(v);
        RistrettoSecretKey(s)
    }
}

//---------------------------------------------      Borrow impl     -------------------------------------------------//

impl Borrow<Scalar> for &RistrettoSecretKey {
    fn borrow(&self) -> &Scalar {
        &self.0
    }
}

//--------------------------------------------- Ristretto Public Key -------------------------------------------------//

#[derive(Clone)]
pub struct RistrettoPublicKey {
    point: RistrettoPoint,
    compressed: CompressedRistretto,
}

impl RistrettoPublicKey {
    const KEY_LEN: usize = 32;

    // Private constructor
    pub(super) fn new_from_pk(pk: RistrettoPoint) -> Self {
        let compressed = pk.compress();
        Self { point: pk, compressed }
    }

    fn new_from_compressed(compressed: CompressedRistretto) -> Option<Self> {
        compressed.decompress().map(|point| Self {
            compressed: compressed.into(),
            point,
        })
    }

    /// Return the embedded RistrettoPoint representation
    pub fn point(&self) -> RistrettoPoint {
        self.point
    }

    pub(super) fn compressed(&self) -> &CompressedRistretto {
        &self.compressed
    }

    /// Generates a new Public key from the given secret key
    pub fn from_secret_key(k: &RistrettoSecretKey) -> RistrettoPublicKey {
        let pk = &k.0 * RISTRETTO_BASEPOINT_TABLE;
        RistrettoPublicKey::new_from_pk(pk)
    }

    pub fn key_length() -> usize {
        Self::KEY_LEN
    }
}

impl borsh::BorshSerialize for RistrettoPublicKey {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.as_bytes(), writer)
    }
}

impl borsh::BorshDeserialize for RistrettoPublicKey {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, borsh::io::Error>
    where R: borsh::io::Read {
        let bytes: Vec<u8> = borsh::BorshDeserialize::deserialize_reader(reader)?;
        Self::from_canonical_bytes(bytes.as_slice())
            .map_err(|e| borsh::io::Error::new(borsh::io::ErrorKind::InvalidInput, e.to_string()))
    }
}

impl Zeroize for RistrettoPublicKey {
    /// Zeroizes both the point and (if it exists) the compressed point
    fn zeroize(&mut self) {
        self.point.zeroize();
        self.compressed.zeroize();
    }
}

// Requires custom Hashable implementation for RistrettoPublicKey as CompressedRistretto doesnt implement this trait
impl Hashable for RistrettoPublicKey {
    fn hash(&self) -> Vec<u8> {
        Blake2b::<U64>::digest(self.as_bytes()).to_vec()
    }
}

impl Hash for RistrettoPublicKey {
    /// Require the implementation of the Hash trait for Hashmaps
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

//----------------------------------    Ristretto Public Key Default   -----------------------------------------------//

impl Default for RistrettoPublicKey {
    fn default() -> Self {
        RistrettoPublicKey::new_from_pk(RistrettoPoint::default())
    }
}

//------------------------------------ PublicKey Display impl ---------------------------------------------//

impl fmt::Display for RistrettoPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_case(f, false)
    }
}

impl RistrettoPublicKey {
    fn fmt_case(&self, f: &mut fmt::Formatter, uppercase: bool) -> fmt::Result {
        let mut hex = self.to_hex();
        if uppercase {
            hex = hex.to_uppercase();
        }
        if f.alternate() {
            hex = format!("0x{hex}");
        }
        match f.width() {
            None => f.write_str(hex.as_str()),
            Some(w @ 1..=6) => f.write_str(&hex[..w]),
            Some(w @ 7..=63) => {
                let left = (w - 3) / 2;
                let right = hex.len() - (w - left - 3);
                f.write_str(format!("{}...{}", &hex[..left], &hex[right..]).as_str())
            },
            _ => core::fmt::Display::fmt(&hex, f),
        }
    }
}

impl fmt::LowerHex for RistrettoPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_case(f, false)
    }
}

impl fmt::UpperHex for RistrettoPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_case(f, true)
    }
}

impl fmt::Debug for RistrettoPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

//------------------------------------ PublicKey PartialEq, Eq, Ord impl ---------------------------------------------//

impl PartialEq for RistrettoPublicKey {
    fn eq(&self, other: &RistrettoPublicKey) -> bool {
        self.ct_eq(other).into()
    }
}

impl Eq for RistrettoPublicKey {}

impl PartialOrd for RistrettoPublicKey {
    fn partial_cmp(&self, other: &RistrettoPublicKey) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RistrettoPublicKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.compressed().as_bytes().cmp(other.compressed().as_bytes())
    }
}

//---------------------------------- PublicKey ByteArray implementation  ---------------------------------------------//

impl ByteArray for RistrettoPublicKey {
    fn from_canonical_bytes(bytes: &[u8]) -> Result<RistrettoPublicKey, ByteArrayError>
    where Self: Sized {
        // Check the length here, because The Ristretto constructor panics rather than returning an error
        if bytes.len() != 32 {
            return Err(ByteArrayError::IncorrectLength {});
        }
        let compressed = CompressedRistretto::from_slice(bytes).map_err(|_| ByteArrayError::ConversionError {
            reason: "Invalid Public key".to_string(),
        })?;
        match RistrettoPublicKey::new_from_compressed(compressed) {
            Some(p) => Ok(p),
            None => Err(ByteArrayError::ConversionError {
                reason: "Invalid compressed Ristretto point".to_string(),
            }),
        }
    }

    /// Return the little-endian byte array representation of the compressed public key
    fn as_bytes(&self) -> &[u8] {
        self.compressed().as_bytes()
    }
}

//----------------------------------         PublicKey Add / Sub / Mul   ---------------------------------------------//

impl<'a> Add<&'a RistrettoPublicKey> for &RistrettoPublicKey {
    type Output = RistrettoPublicKey;

    fn add(self, rhs: &'a RistrettoPublicKey) -> RistrettoPublicKey {
        let p_sum = self.point + rhs.point;
        RistrettoPublicKey::new_from_pk(p_sum)
    }
}

impl Sub<&RistrettoPublicKey> for &RistrettoPublicKey {
    type Output = RistrettoPublicKey;

    fn sub(self, rhs: &RistrettoPublicKey) -> RistrettoPublicKey {
        let p_sum = self.point - rhs.point;
        RistrettoPublicKey::new_from_pk(p_sum)
    }
}

impl<'a> Mul<&'a RistrettoSecretKey> for &RistrettoPublicKey {
    type Output = RistrettoPublicKey;

    fn mul(self, rhs: &'a RistrettoSecretKey) -> RistrettoPublicKey {
        let p = rhs.0 * self.point;
        RistrettoPublicKey::new_from_pk(p)
    }
}

impl<'a> Mul<&'a RistrettoSecretKey> for &RistrettoSecretKey {
    type Output = RistrettoSecretKey;

    fn mul(self, rhs: &'a RistrettoSecretKey) -> RistrettoSecretKey {
        let p = &rhs.0 * &self.0;
        RistrettoSecretKey(p)
    }
}

define_add_variants!(
    LHS = RistrettoPublicKey,
    RHS = RistrettoPublicKey,
    Output = RistrettoPublicKey
);
define_sub_variants!(
    LHS = RistrettoPublicKey,
    RHS = RistrettoPublicKey,
    Output = RistrettoPublicKey
);
define_mul_variants!(
    LHS = RistrettoPublicKey,
    RHS = RistrettoSecretKey,
    Output = RistrettoPublicKey
);
define_mul_variants!(
    LHS = RistrettoSecretKey,
    RHS = RistrettoSecretKey,
    Output = RistrettoSecretKey
);

//----------------------------------         PublicKey From implementations      -------------------------------------//

impl From<RistrettoSecretKey> for Scalar {
    fn from(k: RistrettoSecretKey) -> Self {
        k.0
    }
}

impl From<RistrettoPublicKey> for RistrettoPoint {
    fn from(pk: RistrettoPublicKey) -> Self {
        pk.point
    }
}

impl From<&RistrettoPublicKey> for RistrettoPoint {
    fn from(pk: &RistrettoPublicKey) -> Self {
        pk.point
    }
}
