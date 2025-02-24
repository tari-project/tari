// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Pedersen commitment types and factories for Ristretto

use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{CompressedRistretto, RistrettoPoint},
    traits::{Identity, MultiscalarMul},
};

use crate::tari_crypto::{
    commitment::PedersenCommitment,
    keys::{RistrettoPublicKey, RistrettoSecretKey},
};

pub const TARI_H: CompressedRistretto = CompressedRistretto([
    206, 56, 152, 65, 192, 200, 105, 138, 185, 91, 112, 36, 42, 238, 166, 72, 64, 177, 234, 197, 246, 68, 183, 208, 8,
    172, 5, 135, 207, 71, 29, 112,
]);
pub const RISTRETTO_PEDERSEN_G: RistrettoPoint = RISTRETTO_BASEPOINT_POINT;

/// Generates Pederson commitments `k.G + v.H` using the provided base
/// [RistrettoPoints](curve25519_dalek::ristretto::RistrettoPoint).
#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(non_snake_case)]
pub struct PedersenCommitmentFactory {
    pub(crate) G: RistrettoPoint,
    pub(crate) H: RistrettoPoint,
}

impl PedersenCommitmentFactory {
    /// Create a new Ristretto Commitment factory with the given points as the bases. It's very cheap to create
    /// factories, since we only hold references to the static generator points.
    #[allow(non_snake_case)]
    pub fn new(G: RistrettoPoint, H: RistrettoPoint) -> PedersenCommitmentFactory {
        PedersenCommitmentFactory { G, H }
    }
}

impl Default for PedersenCommitmentFactory {
    /// The default Ristretto Commitment factory uses the Base point for x25519 and its first Blake256 hash.
    fn default() -> Self {
        PedersenCommitmentFactory::new(RISTRETTO_PEDERSEN_G, ristretto_pedersen_h())
    }
}

impl PedersenCommitmentFactory {
    #[allow(non_snake_case)]
    pub fn commit(&self, k: &RistrettoSecretKey, v: &RistrettoSecretKey) -> PedersenCommitment {
        // If we're using the default generators, speed it up using pre-computation tables
        let c = if (self.G, self.H) == (RISTRETTO_PEDERSEN_G, ristretto_pedersen_h()) {
            RistrettoPoint::multiscalar_mul(&[v.0, k.0], &[self.H, self.G])
        } else {
            RistrettoPoint::multiscalar_mul(&[v.0, k.0], &[self.H, self.G])
        };
        PedersenCommitment(RistrettoPublicKey::new_from_pk(c))
    }

    pub fn zero(&self) -> PedersenCommitment {
        PedersenCommitment(RistrettoPublicKey::new_from_pk(RistrettoPoint::identity()))
    }

    pub fn open(&self, k: &RistrettoSecretKey, v: &RistrettoSecretKey, commitment: &PedersenCommitment) -> bool {
        let c_test = self.commit(k, v);
        commitment.0 == c_test.0
    }

    pub fn commit_value(&self, k: &RistrettoSecretKey, value: u64) -> PedersenCommitment {
        let v = RistrettoSecretKey::from(value);
        self.commit(k, &v)
    }

    pub fn open_value(&self, k: &RistrettoSecretKey, v: u64, commitment: &PedersenCommitment) -> bool {
        let kv = RistrettoSecretKey::from(v);
        self.open(k, &kv, commitment)
    }
}

fn ristretto_pedersen_h() -> RistrettoPoint {
    TARI_H.decompress().expect("Failed to decompress TARI_H")
}
