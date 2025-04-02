// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use core::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    ops::{Add, Mul, Sub},
};

use tari_utilities::{ByteArray, ByteArrayError};

use crate::crypto::keys::{RistrettoPublicKey, RistrettoSecretKey};

#[derive(Debug, Clone, Default)]
pub struct PedersenCommitment(pub(crate) RistrettoPublicKey);

impl PedersenCommitment {
    /// Get this commitment as a public key point
    pub fn as_public_key(&self) -> &RistrettoPublicKey {
        &self.0
    }

    /// Converts a public key into a commitment
    pub fn from_public_key(key: &RistrettoPublicKey) -> PedersenCommitment {
        PedersenCommitment(key.clone())
    }
}

impl ByteArray for PedersenCommitment {
    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        let key = RistrettoPublicKey::from_canonical_bytes(bytes)?;
        Ok(Self(key))
    }

    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl PartialOrd for PedersenCommitment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PedersenCommitment {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

/// Add two commitments together. Note! There is no check that the bases are equal.
impl<'b> Add for &'b PedersenCommitment {
    type Output = PedersenCommitment;

    fn add(self, rhs: &'b PedersenCommitment) -> Self::Output {
        PedersenCommitment(&self.0 + &rhs.0)
    }
}

/// Add a public key to a commitment. Note! There is no check that the bases are equal.
impl<'b> Add<&'b RistrettoPublicKey> for &'b PedersenCommitment {
    type Output = PedersenCommitment;

    fn add(self, rhs: &'b RistrettoPublicKey) -> Self::Output {
        PedersenCommitment(&self.0 + rhs)
    }
}

/// Subtracts the left commitment from the right commitment. Note! There is no check that the bases are equal.
impl<'b> Sub for &'b PedersenCommitment {
    type Output = PedersenCommitment;

    fn sub(self, rhs: &'b PedersenCommitment) -> Self::Output {
        PedersenCommitment(&self.0 - &rhs.0)
    }
}

/// Multiply the commitment with a private key
impl<'a, 'b> Mul<&'b RistrettoSecretKey> for &'a PedersenCommitment {
    type Output = PedersenCommitment;

    fn mul(self, rhs: &'b RistrettoSecretKey) -> PedersenCommitment {
        let p = rhs * &self.0;
        PedersenCommitment::from_public_key(&p)
    }
}

impl Hash for PedersenCommitment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.as_bytes())
    }
}

impl PartialEq for PedersenCommitment {
    fn eq(&self, other: &Self) -> bool {
        self.as_public_key() == other.as_public_key()
    }
}

impl Eq for PedersenCommitment {}

impl borsh::BorshDeserialize for PedersenCommitment {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, borsh::io::Error>
    where R: borsh::io::Read {
        Ok(Self(RistrettoPublicKey::deserialize_reader(reader)?))
    }
}

impl borsh::BorshSerialize for PedersenCommitment {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        self.0.serialize(writer)
    }
}
