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
    commitment::HomomorphicCommitment,
    ristretto::{constants::RISTRETTO_NUMS_POINTS, RistrettoPublicKey},
};
use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{CompressedRistretto, RistrettoPoint},
};

use crate::{commitment::HomomorphicCommitmentFactory, ristretto::RistrettoSecretKey};
use curve25519_dalek::scalar::Scalar;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Borrow,
    cmp::Ordering,
    iter::Sum,
    ops::{Add, Sub},
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct PedersenBaseOnRistretto255 {
    pub(crate) G: RistrettoPoint,
    pub(crate) H: RistrettoPoint,
}

pub const RISTRETTO_PEDERSEN_G: RistrettoPoint = RISTRETTO_BASEPOINT_POINT;
pub const RISTRETTO_PEDERSEN_H_COMPRESSED: CompressedRistretto = RISTRETTO_NUMS_POINTS[0];

impl Default for PedersenBaseOnRistretto255 {
    fn default() -> Self {
        PedersenBaseOnRistretto255 {
            G: RISTRETTO_PEDERSEN_G,
            H: RISTRETTO_PEDERSEN_H_COMPRESSED.decompress().unwrap(),
        }
    }
}
impl Default for &PedersenBaseOnRistretto255 {
    fn default() -> Self {
        &DEFAULT_RISTRETTO_PEDERSON_BASE
    }
}
lazy_static! {
    pub static ref DEFAULT_RISTRETTO_PEDERSON_BASE: PedersenBaseOnRistretto255 = PedersenBaseOnRistretto255::default();
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct PedersenOnRistretto255 {
    #[serde(skip)]
    base: &'static PedersenBaseOnRistretto255,
    commitment: RistrettoPublicKey,
}

impl PedersenOnRistretto255 {
    pub fn as_public_key(&self) -> &RistrettoPublicKey {
        &self.commitment
    }
}

impl HomomorphicCommitmentFactory for PedersenBaseOnRistretto255 {
    type C = PedersenOnRistretto255;
    type K = RistrettoSecretKey;
    type PK = RistrettoPublicKey;

    fn create(k: &RistrettoSecretKey, v: &RistrettoSecretKey) -> PedersenOnRistretto255 {
        let base = &DEFAULT_RISTRETTO_PEDERSON_BASE;
        let c: RistrettoPoint = k.0 * base.G + v.0 * base.H;
        PedersenOnRistretto255 {
            base,
            commitment: RistrettoPublicKey::new_from_pk(c),
        }
    }

    fn zero() -> PedersenOnRistretto255 {
        let base = &DEFAULT_RISTRETTO_PEDERSON_BASE;
        let zero = Scalar::zero();
        let c: RistrettoPoint = &zero * base.G + &zero * base.H;
        PedersenOnRistretto255 {
            base,
            commitment: RistrettoPublicKey::new_from_pk(c),
        }
    }

    fn from_public_key(k: &RistrettoPublicKey) -> PedersenOnRistretto255 {
        let base = &DEFAULT_RISTRETTO_PEDERSON_BASE;
        PedersenOnRistretto255 {
            base,
            commitment: k.clone(),
        }
    }
}

impl HomomorphicCommitment for PedersenOnRistretto255 {
    type K = RistrettoSecretKey;

    fn open(&self, k: &RistrettoSecretKey, v: &RistrettoSecretKey) -> bool {
        let c: RistrettoPoint = (v.0 * self.base.H) + (k.0 * self.base.G);
        c == self.commitment.point
    }

    fn as_bytes(&self) -> &[u8] {
        self.commitment.compressed.as_bytes()
    }
}

/// Add two commitments together
/// #panics
/// * If the base values are not equal
impl<'b> Add for &'b PedersenOnRistretto255 {
    type Output = PedersenOnRistretto255;

    fn add(self, rhs: &'b PedersenOnRistretto255) -> Self::Output {
        assert_eq!(self.base, rhs.base, "Bases are unequal");
        let lhp = &self.commitment.point;
        let rhp = &rhs.commitment.point;
        let sum = lhp + rhp;
        PedersenOnRistretto255 {
            base: self.base,
            commitment: RistrettoPublicKey::new_from_pk(sum),
        }
    }
}

define_add_variants!(
    LHS = PedersenOnRistretto255,
    RHS = PedersenOnRistretto255,
    Output = PedersenOnRistretto255
);

/// Subtracts the left commitment from the right commitment
/// #panics
/// * If the base values are not equal
impl<'b> Sub for &'b PedersenOnRistretto255 {
    type Output = PedersenOnRistretto255;

    fn sub(self, rhs: &'b PedersenOnRistretto255) -> Self::Output {
        assert_eq!(self.base, rhs.base, "Bases are unequal");
        let lhp = &self.commitment.point;
        let rhp = &rhs.commitment.point;
        let sum = lhp - rhp;
        PedersenOnRistretto255 {
            base: self.base,
            commitment: RistrettoPublicKey::new_from_pk(sum),
        }
    }
}

impl<T> Sum<T> for PedersenOnRistretto255
where T: Borrow<PedersenOnRistretto255>
{
    fn sum<I>(iter: I) -> Self
    where I: Iterator<Item = T> {
        let sum = iter.map(|c| c.borrow().as_public_key().point).sum();
        let sum = RistrettoPublicKey::new_from_pk(sum);
        PedersenBaseOnRistretto255::from_public_key(&sum)
    }
}

impl PartialOrd for PedersenOnRistretto255 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PedersenOnRistretto255 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.commitment.cmp(&other.commitment)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::keys::SecretKey;
    use rand;
    use std::convert::From;

    lazy_static! {
        static ref TEST_RISTRETTO_PEDERSON_BASE: PedersenBaseOnRistretto255 = PedersenBaseOnRistretto255 {
            G: RISTRETTO_NUMS_POINTS[0].decompress().unwrap(),
            H: RISTRETTO_NUMS_POINTS[1].decompress().unwrap(),
        };
    }

    #[test]
    fn check_default_base() {
        let base = PedersenBaseOnRistretto255::default();
        assert_eq!(base.G, RISTRETTO_PEDERSEN_G);
        assert_eq!(base.H.compress(), RISTRETTO_PEDERSEN_H_COMPRESSED)
    }

    #[test]
    fn check_g_ne_h() {
        assert_ne!(RISTRETTO_PEDERSEN_G.compress(), RISTRETTO_PEDERSEN_H_COMPRESSED);
    }

    /// Simple test for open: Generate 100 random sets of scalars and calculate the Pedersen commitment for them.
    /// Then check that the commitment = k.G + v.H, and that `open` returns `true` for `open(&k, &v)`
    #[test]
    fn check_open() {
        let base = &DEFAULT_RISTRETTO_PEDERSON_BASE;
        let mut rng = rand::OsRng::new().unwrap();
        for _ in 0..100 {
            let v = RistrettoSecretKey::random(&mut rng);
            let k = RistrettoSecretKey::random(&mut rng);
            let c = PedersenBaseOnRistretto255::create(&k, &v);
            let c_calc: RistrettoPoint = v.0 * base.H + k.0 * base.G;
            assert_eq!(RistrettoPoint::from(c.as_public_key()), c_calc);
            assert!(c.open(&k, &v));
            // A different value doesn't open the commitment
            assert!(!c.open(&k, &(&v + &v)));
            // A different blinding factor doesn't open the commitment
            assert!(!c.open(&(&k + &v), &v));
        }
    }

    /// Test, for 100 random sets of scalars that the homomorphic property holds. i.e.
    /// $$
    ///   C = C_1 + C_2 = (k_1+k_2).G + (v_1+v_2).H
    /// $$
    /// and
    /// `open(k1+k2, v1+v2)` is true for _C_
    #[test]
    fn check_homomorphism() {
        let mut rng = rand::OsRng::new().unwrap();
        for _ in 0..100 {
            let v1 = RistrettoSecretKey::random(&mut rng);
            let v2 = RistrettoSecretKey::random(&mut rng);
            let v_sum = &v1 + &v2;
            let k1 = RistrettoSecretKey::random(&mut rng);
            let k2 = RistrettoSecretKey::random(&mut rng);
            let k_sum = &k1 + &k2;
            let c1 = PedersenBaseOnRistretto255::create(&k1, &v1);
            let c2 = PedersenBaseOnRistretto255::create(&k2, &v2);
            let c_sum = &c1 + &c2;
            let c_sum2 = PedersenBaseOnRistretto255::create(&k_sum, &v_sum);
            assert!(c1.open(&k1, &v1));
            assert!(c2.open(&k2, &v2));
            assert_eq!(c_sum, c_sum2);
            assert!(c_sum.open(&k_sum, &v_sum));
        }
    }

    #[test]
    #[should_panic]
    fn summing_different_bases_panics() {
        let mut rng = rand::OsRng::new().unwrap();
        let base2 = &TEST_RISTRETTO_PEDERSON_BASE;
        let k = RistrettoSecretKey::random(&mut rng);
        let v = RistrettoSecretKey::random(&mut rng);
        let c1 = PedersenBaseOnRistretto255::create(&k, &v);
        let c: RistrettoPoint = k.0 * base2.G + v.0 * base2.H;
        let c2 = PedersenOnRistretto255 {
            base: base2,
            commitment: RistrettoPublicKey::new_from_pk(c),
        };
        let _ = &c1 + &c2;
    }

    #[test]
    fn sum_commitment_vector() {
        let mut rng = rand::OsRng::new().unwrap();
        let mut v_sum = RistrettoSecretKey::default();
        let mut k_sum = RistrettoSecretKey::default();
        let zero = RistrettoSecretKey::default();
        let mut c_sum = PedersenBaseOnRistretto255::create(&zero, &zero);
        let mut commitments = Vec::with_capacity(100);
        for _ in 0..100 {
            let v = RistrettoSecretKey::random(&mut rng);
            v_sum = &v_sum + &v;
            let k = RistrettoSecretKey::random(&mut rng);
            k_sum = &k_sum + &k;
            let c = PedersenBaseOnRistretto255::create(&k, &v);
            c_sum = &c_sum + &c;
            commitments.push(c);
        }
        assert!(c_sum.open(&k_sum, &v_sum));
        assert_eq!(c_sum, commitments.iter().sum());
    }

}
