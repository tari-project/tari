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

use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use crate::ristretto::constants::RISTRETTO_NUMS_POINTS;
use crate::commitment::HomomorphicCommitment;
use curve25519_dalek::scalar::Scalar;
use std::ops::Add;
use curve25519_dalek::ristretto::CompressedRistretto;

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(non_snake_case)]
pub struct PedersenBaseOnRistretto255 {
    G: RistrettoPoint,
    H: RistrettoPoint,
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

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PedersenOnRistretto255<'a> {
    base: &'a PedersenBaseOnRistretto255,
    pub(crate) commitment: CompressedRistretto,
}

impl<'a> HomomorphicCommitment<'a> for PedersenOnRistretto255<'a> {
    type Base = PedersenBaseOnRistretto255;
    fn new(k: &Scalar, v: &Scalar, base: &'a PedersenBaseOnRistretto255) -> Self {
        let c: RistrettoPoint = k * base.H + v * base.G;
        PedersenOnRistretto255 {
            base,
            commitment: c.compress(),
        }
    }

    fn open(&self, k: &Scalar, v: &Scalar) -> bool {
        let c: RistrettoPoint = (v * self.base.G) + (k * self.base.H);
        // TODO: Check that this is constant time
        c == self.commitment.decompress().expect("Pedersen commitment was not a valid Ristretto point")
    }

    fn commit(&self) -> &[u8] {
        self.commitment.as_bytes()
    }
}

/// Add two commitments together
/// #panics
/// * If the base values are not equal
impl<'a, 'b> Add for &'b PedersenOnRistretto255<'a> {
    type Output = PedersenOnRistretto255<'a>;

    fn add(self, rhs: &'b PedersenOnRistretto255) -> Self::Output {
        assert_eq!(self.base, rhs.base, "Bases are unequal");
        let lhp = self.commitment.decompress().expect("Pedersen commitment was not a valid Ristretto point");
        let rhp = rhs.commitment.decompress().expect("Pedersen commitment was not a valid Ristretto point");
        let sum = &lhp + &rhp;
        PedersenOnRistretto255 {
            base: self.base,
            commitment: sum.compress(),
        }
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use curve25519_dalek::scalar::Scalar;
    use rand;

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
    /// Then check that the commitment = v.G + k.H, and that `open` returns `true` for `open(&k, &v)`
    #[test]
    fn check_open() {
        let base = PedersenBaseOnRistretto255::default();
        let mut rng = rand::OsRng::new().unwrap();
        for _ in 0..100 {
            let v = Scalar::random(&mut rng);
            let k = Scalar::random(&mut rng);
            let c = PedersenOnRistretto255::new(&k, &v, &base);
            let c_calc: RistrettoPoint = v * base.G + k * base.H;
            assert_eq!(c.commitment, c_calc.compress());
            assert!(c.open(&k, &v));
            assert!(!c.open(&k, &(&v + &v)));
        }
    }

    /// Test, for 100 random sets of scalars that the homomorphic property holds. i.e.
    /// $$
    ///   C = C_1 + C_2 = (k_1+k_2).H + (v_1+v_2).G
    /// $$
    /// and
    /// `open(k1+k2, v1+v2)` is true for _C_
    #[test]
    fn check_homomorphism() {
        let base = PedersenBaseOnRistretto255::default();
        let mut rng = rand::OsRng::new().unwrap();
        for _ in 0..100 {
            let v1 = Scalar::random(&mut rng);
            let v2 = Scalar::random(&mut rng);
            let v_sum = v1 + v2;
            let k1 = Scalar::random(&mut rng);
            let k2 = Scalar::random(&mut rng);
            let k_sum = k1 + k2;
            let c1 = PedersenOnRistretto255::new(&k1, &v1, &base);
            let c2 = PedersenOnRistretto255::new(&k2, &v2, &base);
            let c_sum = &c1 + &c2;
            let c_sum2 = PedersenOnRistretto255::new(&k_sum, &v_sum, &base);
            assert!(c1.open(&k1, &v1));
            assert!(c2.open(&k2, &v2));
            assert_eq!(c_sum, c_sum2);
            assert!(c_sum.open(&k_sum, &v_sum));
        }
    }

    #[test]
    #[should_panic]
    fn summing_different_bases_panics() {
        let base = PedersenBaseOnRistretto255::default();
        let base2 = PedersenBaseOnRistretto255 {
            G: RISTRETTO_NUMS_POINTS[0].decompress().unwrap(),
            H: RISTRETTO_NUMS_POINTS[1].decompress().unwrap(),
        };
        let v = Scalar::from(100u64);
        let k = Scalar::from(101u64);
        let c1 = PedersenOnRistretto255::new(&k, &v, &base);
        let c2 = PedersenOnRistretto255::new(&k, &v, &base2);
        let _ = &c1 + &c2;
    }
}

