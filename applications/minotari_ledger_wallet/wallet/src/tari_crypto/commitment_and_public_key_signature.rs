// Copyright 2025. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::vec::Vec;
use core::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    ops::Add,
};

use tari_utilities::ByteArray;

use crate::tari_crypto::{
    commitment::PedersenCommitment,
    commitment_factory::PedersenCommitmentFactory,
    keys::{RistrettoPublicKey, RistrettoSecretKey},
    schnorr::SchnorrSignature,
};

/// An error when creating a commitment signature
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitmentAndPublicKeySignatureError {
    InvalidChallenge,
}

#[derive(Debug, Clone)]
pub struct CommitmentAndPublicKeySignature {
    pub(crate) ephemeral_commitment: PedersenCommitment,
    pub(crate) ephemeral_pubkey: RistrettoPublicKey,
    pub(crate) u_a: RistrettoSecretKey,
    pub(crate) u_x: RistrettoSecretKey,
    pub(crate) u_y: RistrettoSecretKey,
}

impl CommitmentAndPublicKeySignature {
    /// Creates a new [CommitmentSignature]
    pub fn new(
        ephemeral_commitment: PedersenCommitment,
        ephemeral_pubkey: RistrettoPublicKey,
        u_a: RistrettoSecretKey,
        u_x: RistrettoSecretKey,
        u_y: RistrettoSecretKey,
    ) -> Self {
        CommitmentAndPublicKeySignature {
            ephemeral_commitment,
            ephemeral_pubkey,
            u_a,
            u_x,
            u_y,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        a: &RistrettoSecretKey,
        x: &RistrettoSecretKey,
        y: &RistrettoSecretKey,
        r_a: &RistrettoSecretKey,
        r_x: &RistrettoSecretKey,
        r_y: &RistrettoSecretKey,
        challenge: &[u8],
        factory: &PedersenCommitmentFactory,
    ) -> Result<Self, CommitmentAndPublicKeySignatureError> {
        // The challenge is computed by wide reduction
        let e = match RistrettoSecretKey::from_uniform_bytes(challenge) {
            Ok(e) => e,
            Err(_) => return Err(CommitmentAndPublicKeySignatureError::InvalidChallenge),
        };

        // The challenge cannot be zero
        if e == RistrettoSecretKey::default() {
            return Err(CommitmentAndPublicKeySignatureError::InvalidChallenge);
        }

        // Compute the response values
        let ea = &e * a;
        let ex = &e * x;
        let ey = &e * y;

        let u_a = r_a + &ea;
        let u_x = r_x + &ex;
        let u_y = r_y + &ey;

        // Compute the initial values
        let ephemeral_commitment = factory.commit(r_x, r_a);
        let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(r_y);

        Ok(Self::new(ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y))
    }

    /// Get the signature tuple `(ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y)`
    pub fn complete_signature_tuple(
        &self,
    ) -> (
        &PedersenCommitment,
        &RistrettoPublicKey,
        &RistrettoSecretKey,
        &RistrettoSecretKey,
        &RistrettoSecretKey,
    ) {
        (
            &self.ephemeral_commitment,
            &self.ephemeral_pubkey,
            &self.u_a,
            &self.u_x,
            &self.u_y,
        )
    }

    /// Get the response value `u_a`
    pub fn u_a(&self) -> &RistrettoSecretKey {
        &self.u_a
    }

    /// Get the response value `u_x`
    pub fn u_x(&self) -> &RistrettoSecretKey {
        &self.u_x
    }

    /// Get the response value `u_y`
    pub fn u_y(&self) -> &RistrettoSecretKey {
        &self.u_y
    }

    /// Get the ephemeral commitment `ephemeral_commitment`
    pub fn ephemeral_commitment(&self) -> &PedersenCommitment {
        &self.ephemeral_commitment
    }

    /// Get the ephemeral public key `ephemeral_pubkey`
    pub fn ephemeral_pubkey(&self) -> &RistrettoPublicKey {
        &self.ephemeral_pubkey
    }

    /// Produce a canonical byte representation of the commitment signature
    pub fn to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(2 * RistrettoPublicKey::key_length() + 3 * RistrettoSecretKey::key_length());
        buf.extend_from_slice(self.ephemeral_commitment().as_bytes());
        buf.extend_from_slice(self.ephemeral_pubkey().as_bytes());
        buf.extend_from_slice(self.u_a().as_bytes());
        buf.extend_from_slice(self.u_x().as_bytes());
        buf.extend_from_slice(self.u_y().as_bytes());
        buf
    }
}

impl<'a, 'b> Add<&'b CommitmentAndPublicKeySignature> for &'a CommitmentAndPublicKeySignature {
    type Output = CommitmentAndPublicKeySignature;

    fn add(self, rhs: &'b CommitmentAndPublicKeySignature) -> CommitmentAndPublicKeySignature {
        let ephemeral_commitment_sum = self.ephemeral_commitment() + rhs.ephemeral_commitment();
        let ephemeral_pubkey_sum_sum = self.ephemeral_pubkey() + rhs.ephemeral_pubkey();
        let u_a_sum = self.u_a() + rhs.u_a();
        let u_x_sum = self.u_x() + rhs.u_x();
        let u_y_sum = self.u_y() + rhs.u_y();

        CommitmentAndPublicKeySignature::new(
            ephemeral_commitment_sum,
            ephemeral_pubkey_sum_sum,
            u_a_sum,
            u_x_sum,
            u_y_sum,
        )
    }
}

impl<'a> Add<CommitmentAndPublicKeySignature> for &'a CommitmentAndPublicKeySignature {
    type Output = CommitmentAndPublicKeySignature;

    fn add(self, rhs: CommitmentAndPublicKeySignature) -> CommitmentAndPublicKeySignature {
        let ephemeral_commitment_sum = self.ephemeral_commitment() + rhs.ephemeral_commitment();
        let ephemeral_pubkey_sum_sum = self.ephemeral_pubkey() + rhs.ephemeral_pubkey();
        let u_a_sum = self.u_a() + rhs.u_a();
        let u_x_sum = self.u_x() + rhs.u_x();
        let u_y_sum = self.u_y() + rhs.u_y();

        CommitmentAndPublicKeySignature::new(
            ephemeral_commitment_sum,
            ephemeral_pubkey_sum_sum,
            u_a_sum,
            u_x_sum,
            u_y_sum,
        )
    }
}

impl<'a, 'b> Add<&'b SchnorrSignature> for &'a CommitmentAndPublicKeySignature {
    type Output = CommitmentAndPublicKeySignature;

    fn add(self, rhs: &'b SchnorrSignature) -> CommitmentAndPublicKeySignature {
        let ephemeral_commitment_sum = self.ephemeral_commitment().clone();
        let ephemeral_pubkey_sum_sum = self.ephemeral_pubkey() + rhs.get_public_nonce();
        let u_a_sum = self.u_a().clone();
        let u_x_sum = self.u_x().clone();
        let u_y_sum = self.u_y() + rhs.get_signature();

        CommitmentAndPublicKeySignature::new(
            ephemeral_commitment_sum,
            ephemeral_pubkey_sum_sum,
            u_a_sum,
            u_x_sum,
            u_y_sum,
        )
    }
}

impl<'a> Add<SchnorrSignature> for &'a CommitmentAndPublicKeySignature {
    type Output = CommitmentAndPublicKeySignature;

    fn add(self, rhs: SchnorrSignature) -> CommitmentAndPublicKeySignature {
        let ephemeral_commitment_sum = self.ephemeral_commitment().clone();
        let ephemeral_pubkey_sum_sum = self.ephemeral_pubkey() + rhs.get_public_nonce();
        let u_a_sum = self.u_a().clone();
        let u_x_sum = self.u_x().clone();
        let u_y_sum = self.u_y() + rhs.get_signature();

        CommitmentAndPublicKeySignature::new(
            ephemeral_commitment_sum,
            ephemeral_pubkey_sum_sum,
            u_a_sum,
            u_x_sum,
            u_y_sum,
        )
    }
}

impl Default for CommitmentAndPublicKeySignature {
    fn default() -> Self {
        CommitmentAndPublicKeySignature::new(
            PedersenCommitment::default(),
            RistrettoPublicKey::default(),
            RistrettoSecretKey::default(),
            RistrettoSecretKey::default(),
            RistrettoSecretKey::default(),
        )
    }
}

/// Provide a canonical ordering for commitment signatures. We use byte representations of all values in this order:
/// `ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y`
impl Ord for CommitmentAndPublicKeySignature {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut compare = self.ephemeral_commitment().cmp(other.ephemeral_commitment());
        if compare != Ordering::Equal {
            return compare;
        }

        compare = self.ephemeral_pubkey().cmp(other.ephemeral_pubkey());
        if compare != Ordering::Equal {
            return compare;
        }

        compare = self.u_a().as_bytes().cmp(other.u_a().as_bytes());
        if compare != Ordering::Equal {
            return compare;
        }

        compare = self.u_x().as_bytes().cmp(other.u_x().as_bytes());
        if compare != Ordering::Equal {
            return compare;
        }

        self.u_y().as_bytes().cmp(other.u_y().as_bytes())
    }
}

impl PartialOrd for CommitmentAndPublicKeySignature {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for CommitmentAndPublicKeySignature {
    fn eq(&self, other: &Self) -> bool {
        self.ephemeral_commitment().eq(other.ephemeral_commitment()) &&
            self.ephemeral_pubkey().eq(other.ephemeral_pubkey()) &&
            self.u_a().eq(other.u_a()) &&
            self.u_x().eq(other.u_x()) &&
            self.u_y().eq(other.u_y())
    }
}

impl Eq for CommitmentAndPublicKeySignature {}

impl Hash for CommitmentAndPublicKeySignature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.to_vec())
    }
}
