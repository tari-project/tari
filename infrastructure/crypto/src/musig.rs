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
use std::{
    ops::{Add, Mul},
    prelude::v1::Vec,
};

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
        Ok(JointKeyBuilder { pub_keys: Vec::with_capacity(n), num_signers: n })
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
        Ok(JointKey { pub_keys: self.pub_keys, musig_scalars, joint_pub_key, common })
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
        self.pub_keys.iter().map(|p| JointKeyBuilder::calculate_partial_key::<D>(common.as_bytes(), p)).collect()
    }

    /// Calculate the value of the Joint MuSig public key. **NB**: you should usually sort the participant's keys
    /// before calculating the joint key.
    fn calculate_joint_key<D: Digest>(scalars: &[K], pub_keys: &[P]) -> P {
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

//-------------------------------------------         Fixed Set          ---------------------------------------------//

pub struct FixedSet<T> {
    items: Vec<Option<T>>,
}

impl<T: Clone + PartialEq> FixedSet<T> {
    /// Creates a new fixed set of size n.
    pub fn new(n: usize) -> FixedSet<T> {
        FixedSet { items: vec![None; n] }
    }

    /// Returns the size of the fixed set, NOT the number of items that have been set
    pub fn size(&self) -> usize {
        self.items.len()
    }

    /// Set the `index`th item to `val`. Any existing item is overwritten. The set takes ownership of `val`.
    pub fn set_item(&mut self, index: usize, val: T) -> bool {
        if index >= self.items.len() {
            return false;
        }
        self.items[index] = Some(val);
        true
    }

    /// Return a reference to the `index`th item, or `None` if that item has not been set yet.
    pub fn get_item(&self, index: usize) -> Option<&T> {
        match self.items.get(index) {
            None => None,
            Some(option) => option.as_ref(),
        }
    }

    /// Delete an item from the set by setting the `index`th value to None
    pub fn clear_item(&mut self, index: usize) {
        if index < self.items.len() {
            self.items[index] = None;
        }
    }

    /// Returns true if every item in the set has been set. An empty set returns true as well.
    pub fn is_full(&self) -> bool {
        self.items.iter().all(|v| v.is_some())
    }

    /// Return the index of the given item in the set by performing a linear search through the set
    pub fn search(&self, val: &T) -> Option<usize> {
        let key = self.items.iter().enumerate().find(|v| v.1.is_some() && v.1.as_ref().unwrap() == val);
        match key {
            Some(item) => Some(item.0),
            None => None,
        }
    }

    /// Produces the sum of the values in the set, provided the set is full
    pub fn sum(&self) -> Option<T>
    where for<'a> &'a T: Add<&'a T, Output = T> {
        // This function uses HTRB to work: See https://doc.rust-lang.org/nomicon/hrtb.html
        // or here https://users.rust-lang.org/t/lifetimes-for-type-constraint-where-one-reference-is-local/11087
        if self.size() == 0 || !self.is_full() {
            return None;
        }
        let mut iter = self.items.iter().filter_map(|v| v.as_ref());
        // Take the first item
        let mut sum = iter.next().unwrap().clone();
        for v in iter {
            sum = &sum + v;
        }
        Some(sum)
    }
}

//-------------------------------------------         Tests              ---------------------------------------------//

#[cfg(test)]
mod test {
    use super::FixedSet;

    #[derive(Eq, PartialEq, Clone, Debug)]
    struct Foo {
        baz: String,
    }

    #[test]
    fn zero_sized_fixed_set() {
        let mut s = FixedSet::<usize>::new(0);
        assert!(s.is_full(), "Set should be full");
        assert_eq!(s.set_item(1, 1), false, "Should not be able to set item");
        assert_eq!(s.get_item(0), None, "Should not return a value");
    }

    fn data(s: &str) -> Foo {
        match s {
            "patrician" => Foo { baz: "The Patrician".into() },
            "rincewind" => Foo { baz: "Rincewind".into() },
            "vimes" => Foo { baz: "Commander Vimes".into() },
            "librarian" => Foo { baz: "The Librarian".into() },
            "carrot" => Foo { baz: "Captain Carrot".into() },
            _ => Foo { baz: "None".into() },
        }
    }

    #[test]
    fn small_set() {
        let mut s = FixedSet::<Foo>::new(3);
        // Set is empty
        assert_eq!(s.is_full(), false);
        // Add an item
        assert!(s.set_item(1, data("patrician")));
        assert_eq!(s.is_full(), false);
        // Add an item
        assert!(s.set_item(0, data("vimes")));
        assert_eq!(s.is_full(), false);
        // Replace an item
        assert!(s.set_item(1, data("rincewind")));
        assert_eq!(s.is_full(), false);
        // Add item, filling set
        assert!(s.set_item(2, data("carrot")));
        assert_eq!(s.is_full(), true);
        // Try add an invalid item
        assert_eq!(s.set_item(3, data("librarian")), false);
        assert_eq!(s.is_full(), true);
        // Clear an item
        s.clear_item(1);
        assert_eq!(s.is_full(), false);
        // Check contents
        assert_eq!(s.get_item(0).unwrap().baz, "Commander Vimes");
        assert!(s.get_item(1).is_none());
        assert_eq!(s.get_item(2).unwrap().baz, "Captain Carrot");
        // Size is 3
        assert_eq!(s.size(), 3);
        // Slow search
        assert_eq!(s.search(&data("carrot")), Some(2));
        assert_eq!(s.search(&data("vimes")), Some(0));
        assert_eq!(s.search(&data("librarian")), None);
    }

    #[test]
    fn sum_values() {
        let mut s = FixedSet::<usize>::new(4);
        s.set_item(0, 5);
        assert_eq!(s.sum(), None);
        s.set_item(1, 4);
        assert_eq!(s.sum(), None);
        s.set_item(2, 3);
        assert_eq!(s.sum(), None);
        s.set_item(3, 2);
        assert_eq!(s.sum(), Some(14));
        s.set_item(1, 0);
        assert_eq!(s.sum(), Some(10));
    }
}
