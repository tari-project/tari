// Copyright 2019 The Tari Project
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

use crate::{
    challenge::Challenge256Bit,
    keys::PublicKey,
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    signatures::SchnorrSignature,
};
use curve25519_dalek::scalar::Scalar;
use std::ops::Add;

/// # A Schnorr signature implementation on Ristretto
///
/// Find out more about [Schnorr signatures](https://tlu.tarilabs.com/cryptography/digital_signatures/introduction.html).
///
/// `RistrettoSchnorr` utilises the [curve25519-dalek](https://github.com/dalek-cryptography/curve25519-dalek1)
/// implementation of `ristretto255` to provide Schnorr signature functionality.
///
/// In short, a Schnorr sig is made up of the pair _(R, s)_, where _R_ is a public key (of a secret nonce) and _s_ is
/// the signature.
///
/// ## Creating signatures
///
/// You can create a `RisrettoSchnorr` from it's component parts:
///
/// ```edition2018
/// # use crypto::ristretto::*;
/// # use crypto::keys::*;
/// # use crypto::signatures::SchnorrSignature;
/// # use crypto::common::ByteArray;
///
/// let public_r = RistrettoPublicKey::from_hex("6a493210f7499cd17fecb510ae0cea23a110e8d5b901f8acadd3095c73a3b919").unwrap();
/// let s = RistrettoSecretKey::from_bytes(b"10000000000000000000000000000000").unwrap();
/// let sig = RistrettoSchnorr::new(public_r, s);
/// ```
///
/// or you can create a signature by signing a message:
///
/// ```rust
/// # use crypto::ristretto::*;
/// # use crypto::keys::*;
/// # use crypto::signatures::SchnorrSignature;
/// # use crypto::common::*;
/// # use crypto::challenge::*;
///
/// fn get_keypair() -> (RistrettoSecretKey, RistrettoPublicKey) {
///     let mut rng = rand::OsRng::new().unwrap();
///     let k = RistrettoSecretKey::random(&mut rng);
///     let pk = RistrettoPublicKey::from_secret_key(&k);
///     (k, pk)
/// }
///
/// #[allow(non_snake_case)]
/// fn main() {
///     let (k, P) = get_keypair();
///     let (r, R) = get_keypair();
///     let c = Challenge::<Blake256>::new();
///     let e = c.concat(b"Small Gods");
///     let e: Challenge256Bit = e.into();
///     let sig = RistrettoSchnorr::sign(&k.into(), &r.into(), e);
/// }
/// ```
///
/// # Verifying signatures
///
/// Given a signature, (R,s) and a Challenge, e, you can verify that the signature is valid by calling the `verify`
/// method:
///
/// ```edition2018
/// # use crypto::ristretto::*;
/// # use crypto::keys::*;
/// # use crypto::challenge::*;
/// # use crypto::signatures::SchnorrSignature;
/// # use crypto::common::*;
///
/// # #[allow(non_snake_case)]
/// # fn main() {
/// let P = RistrettoPublicKey::from_hex("74896a30c89186b8194e25f8c1382f8d3081c5a182fb8f8a6d34f27fbefbfc70").unwrap();
/// let R = RistrettoPublicKey::from_hex("fa14cb581ce5717248444721242e6b195a482d503a853dea4acb513074d8d803").unwrap();
/// let s = RistrettoSecretKey::from_hex("bd0b253a619310340a4fa2de54cdd212eac7d088ee1dc47e305c3f6cbd020908").unwrap();
/// let sig = RistrettoSchnorr::new(R, s);
/// let e = Challenge::<Blake256>::new()
///     .concat(b"Maskerade");
/// let e: Challenge256Bit = e.into();
/// assert!(sig.verify(&P, &e));
/// # }
/// ```
#[allow(non_snake_case)]
#[derive(PartialEq, Eq, Copy, Debug, Clone)]
pub struct RistrettoSchnorr {
    R: RistrettoPublicKey,
    s: RistrettoSecretKey,
}

impl SchnorrSignature for RistrettoSchnorr {
    type Challenge = Challenge256Bit;
    type Point = RistrettoPublicKey;
    type Scalar = RistrettoSecretKey;

    /// Create a new SchnorrSignature instance given the signature pair (R,s)
    fn new(public_nonce: RistrettoPublicKey, signature: RistrettoSecretKey) -> Self {
        RistrettoSchnorr { R: public_nonce, s: signature }
    }

    /// Create a new SchnorrSignature instance by signing the challenge (which usually comes from a hashed message,
    /// but in general could be any 256bit number) with the secret key and the secret nonce provided.
    fn sign(secret: &RistrettoSecretKey, nonce: &RistrettoSecretKey, challenge: Challenge256Bit) -> RistrettoSchnorr {
        // s = r + e.k
        let e = Scalar::from_bytes_mod_order(challenge);
        let s = &nonce.0 + &(&secret.0 * &e);
        let public_nonce = RistrettoPublicKey::from_secret_key(nonce);
        RistrettoSchnorr { R: public_nonce, s: RistrettoSecretKey(s) }
    }

    /// Verify that this instance is a valid signature for the given public key and challenge
    fn verify(&self, public_key: &RistrettoPublicKey, challenge: &Challenge256Bit) -> bool {
        let lhs = RistrettoPublicKey::from_secret_key(&self.s);
        let e = Scalar::from_bytes_mod_order(challenge.clone());
        let rhs = &self.R.point + &(&e * &public_key.point);
        // This is a constant time comparison
        lhs.point == rhs
    }

    fn get_signature(&self) -> &RistrettoSecretKey {
        &self.s
    }

    fn get_public_nonce(&self) -> &RistrettoPublicKey {
        &self.R
    }
}

impl Add for &RistrettoSchnorr {
    type Output = RistrettoSchnorr;

    fn add(self, rhs: &RistrettoSchnorr) -> Self::Output {
        let r_sum = self.get_public_nonce() + rhs.get_public_nonce();
        let s_sum = self.get_signature() + rhs.get_signature();
        RistrettoSchnorr::new(r_sum, s_sum)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        challenge::{Challenge, Challenge256Bit},
        common::{Blake256, ByteArray},
        keys::{PublicKey, SecretKeyFactory},
        ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
        signatures::SchnorrSignature,
    };
    use rand;

    fn get_keypair() -> (RistrettoSecretKey, RistrettoPublicKey) {
        let mut rng = rand::OsRng::new().unwrap();
        let k = RistrettoSecretKey::random(&mut rng);
        let pk = RistrettoPublicKey::from_secret_key(&k);
        (k, pk)
    }

    /// Create a signature, and then verify it. Also checks that some invalid signatures fail to verify
    #[test]
    #[allow(non_snake_case)]
    fn sign_and_verify_message() {
        let (k, P) = get_keypair();
        let (r, R) = get_keypair();
        let c = Challenge::<Blake256>::new();
        let e = c.concat(P.to_bytes()).concat(R.to_bytes()).concat(b"Small Gods");
        let e: Challenge256Bit = e.into();
        let sig = RistrettoSchnorr::sign(&k.into(), &r.into(), e);
        let R_calc = sig.get_public_nonce();
        assert_eq!(R, *R_calc);
        assert!(sig.verify(&P, &e));
        // Doesn't work for invalid credentials
        assert!(!sig.verify(&R, &e));
        // Doesn't work for different challenge
        let wrong_challenge: Challenge256Bit = Challenge::<Blake256>::new().concat(b"Guards! Guards!").into();
        assert!(!sig.verify(&P, &wrong_challenge));
    }

    /// This test checks that the linearity of Schnorr signatures hold, i.e. that s = s1 + s2 is validated by R1 + R2
    /// and P1 + P2. We do this by hand here rather than using the APIs to guard against regressions
    #[test]
    #[allow(non_snake_case)]
    fn test_signature_addition() {
        // Alice and Bob generate some keys and nonces
        let (k1, P1) = get_keypair();
        let (r1, R1) = get_keypair();
        let (k2, P2) = get_keypair();
        let (r2, R2) = get_keypair();
        // Each of them creates the Challenge = H(R1 || R2 || P1 || P2 || m)
        let challenge = Challenge::<Blake256>::new()
            .concat(R1.to_bytes())
            .concat(R2.to_bytes())
            .concat(P1.to_bytes())
            .concat(P2.to_bytes())
            .concat(b"Moving Pictures");
        let e1: Challenge256Bit = challenge.clone().into();
        let e2: Challenge256Bit = challenge.clone().into();
        // Calculate Alice's signature
        let s1 = RistrettoSchnorr::sign(&k1.into(), &r1.into(), e1);
        // Calculate Bob's signature
        let s2 = RistrettoSchnorr::sign(&k2.into(), &r2.into(), e2);
        // Now add the two sigs together
        let s_agg = &s1 + &s2;
        let e3: Challenge256Bit = challenge.into();
        // Check that the multi-sig verifies
        assert!(s_agg.verify(&(&P1 + &P2), &e3));
    }
}
