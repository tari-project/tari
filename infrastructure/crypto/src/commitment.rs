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

use crate::keys::PublicKey;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    ops::{Add, Sub},
};
use tari_utilities::{ByteArray, ByteArrayError};

/// A commitment is like a sealed envelope. You put some information inside the envelope, and then seal (commit) it.
/// You can't change what you've said, but also, no-one knows what you've said until you're ready to open (open) the
/// envelope and reveal its contents. Also it's a special envelope that can only be opened by a special opener that
/// you keep safe in your drawer.
///
/// There are also different types of commitments that vary in their security guarantees, but all of them are
/// represented by binary data; so [HomomorphicCommitment](trait.HomomorphicCommitment.html) implements
/// [ByteArray](trait.ByteArray.html).
///
/// The Homomorphic part means, more or less, that commitments follow some of the standard rules of
/// arithmetic. Adding two commitments is the same as committing to the sum of their parts:
/// $$ \begin{aligned}
///   C_1 &= v_1.G + k_1.H \\\\
///   C_2 &= v_2.G + k_2.H \\\\
///   \therefore C_1 + C_2 &= (v_1 + v_2)G + (k_1 + k_2)H
/// \end{aligned} $$
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "P: PublicKey"))]
pub struct HomomorphicCommitment<P>(pub(crate) P)
where P: PublicKey;

impl<P> HomomorphicCommitment<P>
where P: PublicKey
{
    pub fn as_public_key(&self) -> &P {
        &self.0
    }

    pub fn from_public_key(p: &P) -> HomomorphicCommitment<P> {
        HomomorphicCommitment(p.clone())
    }
}

impl<P> ByteArray for HomomorphicCommitment<P>
where P: PublicKey
{
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        let p = P::from_bytes(bytes)?;
        Ok(Self(p))
    }

    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl<P> PartialOrd for HomomorphicCommitment<P>
where P: PublicKey
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.0.cmp(&other.0))
    }
}

impl<P> Ord for HomomorphicCommitment<P>
where P: PublicKey
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

/// Add two commitments together. Note! There is no check that the bases are equal.
impl<'b, P> Add for &'b HomomorphicCommitment<P>
where
    P: PublicKey,
    &'b P: Add<&'b P, Output = P>,
{
    type Output = HomomorphicCommitment<P>;

    fn add(self, rhs: &'b HomomorphicCommitment<P>) -> Self::Output {
        HomomorphicCommitment(&self.0 + &rhs.0)
    }
}

/// Subtracts the left commitment from the right commitment. Note! There is no check that the bases are equal.
impl<'b, P> Sub for &'b HomomorphicCommitment<P>
where
    P: PublicKey,
    &'b P: Sub<&'b P, Output = P>,
{
    type Output = HomomorphicCommitment<P>;

    fn sub(self, rhs: &'b HomomorphicCommitment<P>) -> Self::Output {
        HomomorphicCommitment(&self.0 - &rhs.0)
    }
}

impl<P: PublicKey> Hash for HomomorphicCommitment<P> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.as_bytes())
    }
}

pub trait HomomorphicCommitmentFactory {
    type P: PublicKey;

    /// Create a new commitment with the value and blinding factor provided. The implementing type will provide the
    /// base values
    fn commit(&self, k: &<Self::P as PublicKey>::K, v: &<Self::P as PublicKey>::K) -> HomomorphicCommitment<Self::P>;
    /// return an identity point for addition using the specified base point. This is a commitment to zero with a zero
    /// blinding factor on the base point
    fn zero(&self) -> HomomorphicCommitment<Self::P>;
    /// Test whether the given keys open the given commitment
    fn open(
        &self,
        k: &<Self::P as PublicKey>::K,
        v: &<Self::P as PublicKey>::K,
        commitment: &HomomorphicCommitment<Self::P>,
    ) -> bool;
    /// Create a commitment from a spending key and a integer value
    fn commit_value(&self, k: &<Self::P as PublicKey>::K, value: u64) -> HomomorphicCommitment<Self::P>;
    /// Test whether the given private key and value open the given commitment
    fn open_value(&self, k: &<Self::P as PublicKey>::K, v: u64, commitment: &HomomorphicCommitment<Self::P>) -> bool;
}
