//! General definition of public-private key pairs for use in Tari. The traits and structs
//! defined here are used in the Tari domain logic layer exclusively (as opposed to any specific
//! implementation of ECC curve). The idea being that we can swap out the underlying
//! implementation without worrying too much about the impact on upstream code.

use rand::{CryptoRng, Rng};

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
/// Assuming there is a Curve25519 implementation,
/// ```edition2018
/// # use crypto::curve25519::{ Curve25519SecretKey, Curve25519PublicKey };
/// # use crypto::keys::{ SecretKeyFactory, SecretKey, PublicKey };
/// # use rand;
/// let mut rng = rand::OsRng::new().unwrap();
/// let k = Curve25519SecretKey::random(&mut rng);
/// let p = Curve25519PublicKey::from_secret_key(&k);
/// ```
pub trait SecretKey {}

//----------------------------------------   Public Keys  ----------------------------------------//

/// A trait specifying common behaviour for representing `PublicKey`s. Specific elliptic curve
/// implementations need to implement this trait for them to be used in Tari.
///
/// See [SecretKey](trait.SecretKey.html) for an example.
pub trait PublicKey {
    type K: SecretKey;
    /// Calculate the public key associated with the given secret key. This should not fail; if a
    /// failure does occur (implementation error?), the function will panic.
    fn from_secret_key(k: &Self::K) -> Self;
}
