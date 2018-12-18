//! General definition of public-private key pairs for use in Tari. The traits and structs
//! defined here are used in the Tari domain logic layer exclusively (as opposed to any specific
//! implementation of ECC curve). The idea being that we can swap out the underlying
//! implementation without worrying too much about the impact on upstream code.

use crate::hex::HexError;
use derive_error::Error;
use rand::{CryptoRng, Rng};
use std::marker::Sized;

//----------------------------------------   Key Errors   ----------------------------------------//
#[derive(Debug, Error)]
pub enum KeyError {
    // Could not create a valid key when converting from a different format
    #[error(msg_embedded, non_std, no_from)]
    ConversionError(String),
    // Invalid hex representation for key
    HexConversionError(HexError),
}

//----------------------------------------   Secret Keys  ----------------------------------------//

/// A secret key factory trait. The `random` function is pulled out into a separate Trait because
/// we can't know _a priori_ whether the default implementation (uniform random characters over the
/// full 2^256 space) represents legal private keys. Maybe some validation must be done, which
/// must be left up to the respective curve implementations.
pub trait SecretKeyFactory {
    fn random<R: CryptoRng + Rng>(rng: &mut R) -> Self;
}

/// A trait specifying common behaviour for representing `SecretKey`s. Specific elliptic curve
/// implementations need to implement this trait for them to be used in Tari.
///
/// ## Example
///
/// Assuming there is a Curve25519EdDSA implementation,
/// ```edition2018
/// # use crypto::curve25519::{ Curve25519SecretKey, Curve25519PublicKey };
/// # use crypto::keys::{ SecretKeyFactory, SecretKey, PublicKey };
/// # use rand;
/// let mut rng = rand::OsRng::new().unwrap();
/// let k = Curve25519SecretKey::random(&mut rng);
/// let p = Curve25519PublicKey::from_secret_key(&k);
/// ```
pub trait SecretKey {
    /// Return the hexadecimal string representation of the secret key
    fn to_hex(&self) -> String;

    /// Try and convert the given hexadecimal string to a secret key. Any failures (incorrect
    /// string length, non hex characters, etc) return a
    /// [KeyError](enum.KeyError.html) with an explanatory note.
    fn from_hex(hex: &str) -> Result<Self, KeyError>
    where Self: Sized;

    /// Try and convert the given byte array to a secret key. Any failures (incorrect
    /// array length, implementation-specific checks, etc) return a
    /// [KeyError](enum.KeyError.html) with an explanatory note.
    fn from_bytes(bytes: &[u8]) -> Result<Self, KeyError>
    where Self: Sized;

    /// Return the secret key as a byte array
    fn to_bytes(&self) -> &[u8];

    /// Return the secret key as a byte vector
    fn to_vec(&self) -> Vec<u8>;

    /// Try and convert the given byte vector to a secret key. Any failures (incorrect
    /// string length etc) return a [KeyError](enum.KeyError.html) with an explanatory note.
    fn from_vec(v: &[u8]) -> Result<Self, KeyError>
    where Self: Sized;
}

//----------------------------------------   Public Keys  ----------------------------------------//

/// A trait specifying common behaviour for representing `PublicKey`s. Specific elliptic curve
/// implementations need to implement this trait for them to be used in Tari.
///
/// See [SecretKey](trait.SecretKey.html) for an example.
pub trait PublicKey {
    type K: SecretKey;

    /// Calculate the public key associated with the given secret key. This should not fail; if a
    /// failure does occur (implementatio error?), the function will panic.
    fn from_secret_key(k: &Self::K) -> Self;

    /// Return the hexadecimal string representation of the public key
    fn to_hex(&self) -> String;

    /// Try and convert the given byte array to a public key. Any failures (incorrect
    /// array length, implementation-specific checks, etc) return a
    /// [KeyError](enum.KeyError.html) with an explanatory note.
    fn from_hex(hex: &str) -> Result<Self, KeyError>
    where Self: Sized;

    /// Try and convert the given byte array to a public key. Any failures (incorrect
    /// array length, implementation-specific checks, etc) return a
    /// [KeyError](enum.KeyError.html) with an explanatory note.
    fn from_bytes(bytes: &[u8]) -> Result<Self, KeyError>
    where Self: Sized;

    /// Return the public key as a byte array
    fn to_bytes(&self) -> &[u8];

    fn to_vec(&self) -> Vec<u8>;

    fn from_vec(v: &[u8]) -> Result<Self, KeyError>
    where Self: Sized;
}
