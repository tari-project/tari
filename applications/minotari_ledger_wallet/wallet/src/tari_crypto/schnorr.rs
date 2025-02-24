// Copyright 2025. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use core::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::Add,
};

use blake2::Blake2b;
use digest::{consts::U64, Digest};
use rand_core::{CryptoRng, RngCore};
use tari_utilities::ByteArray;

use crate::{
    hash_domain,
    hashing::DomainSeparatedHash,
    tari_crypto::{
        hashing::{DomainSeparatedHasher, DomainSeparation},
        keys::{RistrettoPublicKey, RistrettoSecretKey},
    },
};

hash_domain!(SchnorrSigChallenge, "com.tari.schnorr_signature", 1);

/// An error occurred during construction of a SchnorrSignature
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SchnorrSignatureError {
    InvalidChallenge,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone)]
pub struct SchnorrSignature<H = SchnorrSigChallenge> {
    pub(crate) public_nonce: RistrettoPublicKey,
    pub(crate) signature: RistrettoSecretKey,
    _phantom: PhantomData<H>,
}

impl<H> SchnorrSignature<H>
where H: DomainSeparation
{
    /// Create a new `SchnorrSignature`.
    pub fn new(public_nonce: RistrettoPublicKey, signature: RistrettoSecretKey) -> Self {
        SchnorrSignature {
            public_nonce,
            signature,
            _phantom: PhantomData,
        }
    }

    /// Calculates the signature verifier `s.G`. This must be equal to `R + eK`.
    fn calc_signature_verifier(&self) -> RistrettoPublicKey {
        RistrettoPublicKey::from_secret_key(&self.signature)
    }

    pub fn sign_raw_uniform<'a>(
        secret: &'a RistrettoSecretKey,
        nonce: RistrettoSecretKey,
        challenge: &[u8],
    ) -> Result<Self, SchnorrSignatureError> {
        // s = r + e.k
        let e = match RistrettoSecretKey::from_uniform_bytes(challenge) {
            Ok(e) => e,
            Err(_) => return Err(SchnorrSignatureError::InvalidChallenge),
        };
        let public_nonce = RistrettoPublicKey::from_secret_key(&nonce);
        let ek = e * secret;
        let s = ek + nonce;
        Ok(Self::new(public_nonce, s))
    }

    pub fn sign_raw_canonical<'a>(
        secret: &'a RistrettoSecretKey,
        nonce: RistrettoSecretKey,
        challenge: &[u8],
    ) -> Result<Self, SchnorrSignatureError> {
        // s = r + e.k
        let e = match RistrettoSecretKey::from_canonical_bytes(challenge) {
            Ok(e) => e,
            Err(_) => return Err(SchnorrSignatureError::InvalidChallenge),
        };
        let public_nonce = RistrettoPublicKey::from_secret_key(&nonce);
        let ek = e * secret;
        let s = ek + nonce;
        Ok(Self::new(public_nonce, s))
    }

    pub fn sign<'a, B, R: RngCore + CryptoRng>(
        secret: &'a RistrettoSecretKey,
        message: B,
        rng: &mut R,
    ) -> Result<Self, SchnorrSignatureError>
    where
        B: AsRef<[u8]>,
    {
        let nonce = RistrettoSecretKey::random(rng);
        Self::sign_with_nonce_and_message(secret, nonce, message)
    }

    pub fn sign_with_nonce_and_message<'a, B>(
        secret: &'a RistrettoSecretKey,
        nonce: RistrettoSecretKey,
        message: B,
    ) -> Result<Self, SchnorrSignatureError>
    where
        B: AsRef<[u8]>,
    {
        let public_nonce = RistrettoPublicKey::from_secret_key(&nonce);
        let public_key = RistrettoPublicKey::from_secret_key(secret);
        let challenge =
            Self::construct_domain_separated_challenge::<_, Blake2b<U64>>(&public_nonce, &public_key, message);
        Self::sign_raw_uniform(secret, nonce, challenge.as_ref())
    }

    pub fn construct_domain_separated_challenge<B, D>(
        public_nonce: &RistrettoPublicKey,
        public_key: &RistrettoPublicKey,
        message: B,
    ) -> DomainSeparatedHash<D>
    where
        B: AsRef<[u8]>,
        D: Digest,
    {
        DomainSeparatedHasher::<D, H>::new_with_label("challenge")
            .chain(public_nonce.as_bytes())
            .chain(public_key.as_bytes())
            .chain(message.as_ref())
            .finalize()
    }

    pub fn verify<'a, B>(&self, public_key: &'a RistrettoPublicKey, message: B) -> bool
    where B: AsRef<[u8]> {
        let challenge =
            Self::construct_domain_separated_challenge::<_, Blake2b<U64>>(&self.public_nonce, public_key, message);
        self.verify_raw_uniform(public_key, challenge.as_ref())
    }

    pub fn verify_raw_uniform<'a>(&self, public_key: &'a RistrettoPublicKey, challenge: &[u8]) -> bool {
        let e = match RistrettoSecretKey::from_uniform_bytes(challenge) {
            Ok(e) => e,
            Err(_) => return false,
        };
        self.verify_challenge_scalar(public_key, &e)
    }

    pub fn verify_raw_canonical<'a>(&self, public_key: &'a RistrettoPublicKey, challenge: &[u8]) -> bool {
        let e = match RistrettoSecretKey::from_canonical_bytes(challenge) {
            Ok(e) => e,
            Err(_) => return false,
        };
        self.verify_challenge_scalar(public_key, &e)
    }

    pub fn verify_challenge_scalar<'a>(
        &self,
        public_key: &'a RistrettoPublicKey,
        challenge: &RistrettoSecretKey,
    ) -> bool {
        // Reject a zero key
        if public_key == &RistrettoPublicKey::default() {
            return false;
        }

        let lhs = self.calc_signature_verifier();
        let rhs = &self.public_nonce + challenge * public_key;
        // Implementors should make this a constant time comparison
        lhs == rhs
    }

    pub fn get_signature(&self) -> &RistrettoSecretKey {
        &self.signature
    }

    pub fn get_public_nonce(&self) -> &RistrettoPublicKey {
        &self.public_nonce
    }
}

impl<'a, 'b, H> Add<&'b SchnorrSignature<H>> for &'a SchnorrSignature<H>
where H: DomainSeparation
{
    type Output = SchnorrSignature<H>;

    fn add(self, rhs: &'b SchnorrSignature<H>) -> SchnorrSignature<H> {
        let r_sum = self.get_public_nonce() + rhs.get_public_nonce();
        let s_sum = self.get_signature() + rhs.get_signature();
        SchnorrSignature::new(r_sum, s_sum)
    }
}

impl<'a, H> Add<SchnorrSignature<H>> for &'a SchnorrSignature<H>
where H: DomainSeparation
{
    type Output = SchnorrSignature<H>;

    fn add(self, rhs: SchnorrSignature<H>) -> SchnorrSignature<H> {
        let r_sum = self.get_public_nonce() + rhs.get_public_nonce();
        let s_sum = self.get_signature() + rhs.get_signature();
        SchnorrSignature::new(r_sum, s_sum)
    }
}

impl<H> Default for SchnorrSignature<H>
where H: DomainSeparation
{
    fn default() -> Self {
        SchnorrSignature::new(RistrettoPublicKey::default(), RistrettoSecretKey::default())
    }
}

impl<H> Ord for SchnorrSignature<H> {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.public_nonce.cmp(&other.public_nonce) {
            Ordering::Equal => self.signature.as_bytes().cmp(other.signature.as_bytes()),
            v => v,
        }
    }
}

impl<H> PartialOrd for SchnorrSignature<H> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<H> Eq for SchnorrSignature<H> {}

impl<H> PartialEq for SchnorrSignature<H> {
    fn eq(&self, other: &Self) -> bool {
        self.public_nonce.eq(&other.public_nonce) && self.signature.eq(&other.signature)
    }
}

impl<H> Hash for SchnorrSignature<H> {
    fn hash<T: Hasher>(&self, state: &mut T) {
        self.public_nonce.hash(state);
        self.signature.hash(state);
    }
}
