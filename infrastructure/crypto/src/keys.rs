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

//! General definition of public-private key pairs for use in Tari. The traits and structs
//! defined here are used in the Tari domain logic layer exclusively (as opposed to any specific
//! implementation of ECC curve). The idea being that we can swap out the underlying
//! implementation without worrying too much about the impact on upstream code.

use rand::{CryptoRng, Rng};
use std::ops::Add;
use tari_utilities::ByteArray;

/// A trait specifying common behaviour for representing `SecretKey`s. Specific elliptic curve
/// implementations need to implement this trait for them to be used in Tari.
///
/// ## Example
///
/// Assuming there is a Ristretto implementation,
/// ```edition2018
/// # use crypto::ristretto::{ RistrettoSecretKey, RistrettoPublicKey };
/// # use crypto::keys::{ SecretKey, PublicKey };
/// # use rand;
/// let mut rng = rand::OsRng::new().unwrap();
/// let k = RistrettoSecretKey::random(&mut rng);
/// let p = RistrettoPublicKey::from_secret_key(&k);
/// ```
pub trait SecretKey: ByteArray + Clone + PartialEq + Eq + Add<Output = Self> + Default {
    fn key_length() -> usize;
    fn random<R: Rng + CryptoRng>(rng: &mut R) -> Self;
}

//----------------------------------------   Public Keys  ----------------------------------------//

/// A trait specifying common behaviour for representing `PublicKey`s. Specific elliptic curve
/// implementations need to implement this trait for them to be used in Tari.
///
/// See [SecretKey](trait.SecretKey.html) for an example.
pub trait PublicKey: ByteArray + Add<Output = Self> + Clone + PartialOrd + Ord + Default {
    type K: SecretKey;
    /// Calculate the public key associated with the given secret key. This should not fail; if a
    /// failure does occur (implementation error?), the function will panic.
    fn from_secret_key(k: &Self::K) -> Self;

    fn key_length() -> usize;

    fn batch_mul (scalars: &[Self::K], points: &[Self]) -> Self;

    fn random_keypair<R: Rng + CryptoRng>(rng: &mut R) -> (Self::K, Self) {
        let k = Self::K::random(rng);
        let pk = Self::from_secret_key(&k);
        (k, pk)
    }
}
