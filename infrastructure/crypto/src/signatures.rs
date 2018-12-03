//! Digital Signature module
//! This module defines generic traits for handling the digital signature operations, agnostic
//! of the underlying elliptic curve implementation

use crate::keys::{PublicKey, SecretKey};

/// Generic definition of Schnorr Signature functionality, agnostic of the elliptic curve used.
/// Schnorr signatures are linear and have the form _s = r + ek_, where _r_ is a nonce (secret key),
/// _k_ is a secret key, and _s_ is the signature.
///
/// ## Example
///
/// ```edition2018
/// # use rand::OsRng;
/// # use crypto::keys::{ PublicKey, SecretKey, SecretKeyFactory };
/// # use crypto::signatures::SchnorrSignature;
/// # use crypto::curve25519::{ Curve25519PublicKey, Curve25519SecretKey, Curve25519EdDSA };
/// # let mut rng = OsRng::new().unwrap();
///  let msg = b"This parrot is dead";
///  let k = Curve25519SecretKey::random(&mut rng);
///  let p = Curve25519PublicKey::from_secret_key(&k);
///  let sig = Curve25519EdDSA::sign(&k, &p, msg);
///  assert!(sig.verify(&p, msg));
/// ```
pub trait SchnorrSignature {
    type K: SecretKey;
    type P: PublicKey;

    /// Return the public nonce R associated with this signature
    #[allow(non_snake_case)]
    fn R(&self) -> Self::P;

    /// Return the signature
    fn s(&self) -> Self::K;

    /// Sign the given message, using the provided secret key. The public key must be the one
    /// associated with the secret key. The message is an arbitrary byte array that will be
    /// hashed as part of the specific digital signature algorithm
    fn sign(secret: &Self::K, public: &Self::P, m: &[u8]) -> Self;

    /// Check whether the given signature is valid for the given message and public key
    fn verify(&self, public: &Self::P, m: &[u8]) -> bool;
}

//* MuSig
//* Schnorr signatures
//* Partial Signatures and Zero-knowledge contingent payments
//* Message signing
//* Aggregate Signatures
