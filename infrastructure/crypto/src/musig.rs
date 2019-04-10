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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crate::{
    challenge::Challenge,
    keys::{PublicKey, SecretKey},
};
use derive_error::Error;
use digest::Digest;
use std::{ops::Mul, prelude::v1::Vec};

//----------------------------------------------   Constants       ------------------------------------------------//
pub const MAX_SIGNATURES: usize = 32768; // If you need more, call customer support

//----------------------------------------------   Error Codes     ------------------------------------------------//
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MuSigError {
    /// The number of public nonces must match the number of public keys in the joint key
    #[error(no_from, non_std)]
    MismatchedNonces,
    /// The number of partial signatures must match the number of public keys in the joint key
    #[error(no_from, non_std)]
    MismatchedSignatures,
    /// The aggregate signature did not verify
    #[error(no_from, non_std)]
    InvalidAggregateSignature,
    /// A partial signature did not validate
    #[error(no_from, non_std)]
    InvalidPartialSignature(usize),
    /// The participant list must be sorted before making this call
    #[error(no_from, non_std)]
    NotSorted,
    /// The participant key is not in the list
    #[error(no_from, non_std)]
    ParticipantNotFound,
    /// An attempt was made to perform an invalid MuSig state transition
    #[error(no_from, non_std)]
    InvalidStateTransition,
    /// An attempt was made to add a duplicate public key to a MuSig signature
    #[error(no_from, non_std)]
    DuplicatePubKey,
    /// There are too many parties in the MuSig signature
    #[error(no_from, non_std)]
    TooManyParticipants,
    /// There are too few parties in the MuSig signature
    #[error(no_from, non_std)]
    NotEnoughParticipants,
    /// A nonce hash is missing
    #[error(no_from, non_std)]
    MissingHash,
    /// The message to be signed can only be set once
    #[error(no_from, non_std)]
    MessageAlreadySet,
    /// The message to be signed MUST be set before the final nonce is added to the MuSig ceremony
    #[error(no_from, non_std)]
    MissingMessage,
    /// The message to sign is invalid. have you hashed it?
    #[error(no_from, non_std)]
    InvalidMessage,
    /// MuSig requires a hash function with a 32 byte digest
    #[error(no_from, non_std)]
    IncompatibleHashFunction,
}

//----------------------------------------------     Joint Key     ------------------------------------------------//

/// The JointKey is a modified public key used in Signature aggregation schemes like MuSig which is not susceptible
/// to Rogue Key attacks.
///
/// A joint key is calculated from _n_ participants by having each of them calculate:
/// $$
///   L = H(P_1 || P_2 || \dots || P_n)
///   X = \sum H(L || P_i)P_i
///   X_i = k_i H(L || P_i).G
/// $$
/// Concrete implementations of JointKey will also need to implement the MultiScalarMul trait, which allows them to
/// provide implementation-specific optimisations for dot-product operations.
pub struct JointKey<P, K>
where
    K: SecretKey,
    P: PublicKey<K = K>,
{
    pub_keys: Vec<P>,
    musig_scalars: Vec<K>,
    common: K,
    joint_pub_key: P,
}

pub struct JointKeyBuilder<P, K>
where
    K: SecretKey,
    P: PublicKey<K = K>,
{
    num_signers: usize,
    pub_keys: Vec<P>,
}

impl<K, P> JointKeyBuilder<P, K>
where
    K: SecretKey + Mul<P, Output = P>,
    P: PublicKey<K = K>,
{
    /// Create a new JointKey instance containing no participant keys, or return `TooManyParticipants` if n exceeds
    /// `MAX_SIGNATURES`
    pub fn new(n: usize) -> Result<JointKeyBuilder<P, K>, MuSigError> {
        if n > MAX_SIGNATURES {
            return Err(MuSigError::TooManyParticipants);
        }
        if n == 0 {
            return Err(MuSigError::NotEnoughParticipants);
        }
        Ok(JointKeyBuilder {
            pub_keys: Vec::with_capacity(n),
            num_signers: n,
        })
    }

    /// The number of parties in the Joint key
    pub fn num_signers(&self) -> usize {
        self.num_signers
    }

    /// Add a participant signer's public key to the JointKey
    pub fn add_key(&mut self, pub_key: P) -> Result<usize, MuSigError> {
        if self.key_exists(&pub_key) {
            return Err(MuSigError::DuplicatePubKey);
        }
        // push panics on int overflow, so catch this here
        let n = self.pub_keys.len();
        if n >= self.num_signers {
            return Err(MuSigError::TooManyParticipants);
        }
        self.pub_keys.push(pub_key);
        Ok(self.pub_keys.len())
    }

    /// Checks whether the given public key is in the participants list
    pub fn key_exists(&self, key: &P) -> bool {
        self.pub_keys.iter().any(|v| v == key)
    }

    /// Checks whether the number of pub_keys is equal to `num_signers`
    pub fn is_full(&self) -> bool {
        self.pub_keys.len() == self.num_signers
    }

    /// Add all the keys in `keys` to the participant list.
    pub fn add_keys<T: IntoIterator<Item = P>>(&mut self, keys: T) -> Result<usize, MuSigError> {
        for k in keys {
            self.add_key(k)?;
        }
        Ok(self.pub_keys.len())
    }

    /// Produce a sorted, immutable joint Musig public key from the gathered set of conventional public keys
    pub fn build<D: Digest>(mut self) -> Result<JointKey<P, K>, MuSigError> {
        if !self.is_full() {
            return Err(MuSigError::NotEnoughParticipants);
        }
        self.sort_keys();
        let common = self.calculate_common::<D>();
        let musig_scalars = self.calculate_musig_scalars::<D>(&common);
        let joint_pub_key = JointKeyBuilder::calculate_joint_key::<D>(&musig_scalars, &self.pub_keys);
        Ok(JointKey {
            pub_keys: self.pub_keys,
            musig_scalars,
            joint_pub_key,
            common,
        })
    }

    /// Utility function to calculate \\( \ell = H(P_1 || ... || P_n) \mod p \\)
    /// # Panics
    /// If the SecretKey implementation cannot construct a valid key from the given hash, the function will panic.
    /// You should ensure that the SecretKey constructor protects against failures and that the hash digest given
    /// produces a byte array of the correct length.
    fn calculate_common<D: Digest>(&self) -> K {
        let mut common = Challenge::<D>::new();
        for k in self.pub_keys.iter() {
            common = common.concat(k.as_bytes());
        }
        K::from_vec(&common.hash())
            .expect("Could not calculate Scalar from hash value. Your crypto/hash combination might be inconsistent")
    }

    /// Private utility function to calculate \\( H(\ell || P_i) \mod p \\)
    /// # Panics
    /// If the SecretKey implementation cannot construct a valid key from the given hash, the function will panic.
    /// You should ensure that the SecretKey constructor protects against failures and that the hash digest given
    /// produces a byte array of the correct length.
    fn calculate_partial_key<D: Digest>(common: &[u8], pubkey: &P) -> K {
        let k = Challenge::<D>::new().concat(common).concat(pubkey.as_bytes()).hash();
        K::from_vec(&k)
            .expect("Could not calculate Scalar from hash value. Your crypto/hash combination might be inconsistent")
    }

    /// Sort the keys in the participant list. The order is determined by the `Ord` trait of the concrete public key
    /// implementation used to construct the joint key.
    /// **NB:** Sorting the keys will, usually, change the value of the joint key!
    fn sort_keys(&mut self) {
        self.pub_keys.sort_unstable();
    }

    /// Utility function that produces the vector of MuSig private key modifiers, \\( a_i = H(\ell || P_i) \\)
    fn calculate_musig_scalars<D: Digest>(&self, common: &K) -> Vec<K> {
        self.pub_keys
            .iter()
            .map(|p| JointKeyBuilder::calculate_partial_key::<D>(common.as_bytes(), p))
            .collect()
    }

    /// Calculate the value of the Joint MuSig public key. **NB**: you should usually sort the participant's keys
    /// before calculating the joint key.
    fn calculate_joint_key<D: Digest>(scalars: &Vec<K>, pub_keys: &Vec<P>) -> P {
        P::batch_mul(scalars, pub_keys)
    }
}

impl<P, K> JointKey<P, K>
where
    K: SecretKey,
    P: PublicKey<K = K>,
{
    /// Return the index of the given key in the joint key participants list. If the key isn't in the list, returns
    /// `Err(ParticipantNotFound)`
    pub fn index_of(&self, pubkey: &P) -> Result<usize, MuSigError> {
        match self.pub_keys.binary_search(pubkey) {
            Ok(i) => Ok(i),
            Err(_) => Err(MuSigError::ParticipantNotFound),
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.pub_keys.len()
    }

    #[inline]
    pub fn get_pub_keys(&self, index: usize) -> &P {
        &self.pub_keys[index]
    }

    #[inline]
    pub fn get_musig_scalar(&self, index: usize) -> &K {
        &self.musig_scalars[index]
    }

    #[inline]
    pub fn get_common(&self) -> &K {
        &self.common
    }

    #[inline]
    pub fn get_joint_pubkey(&self) -> &P {
        &self.joint_pub_key
    }
}
