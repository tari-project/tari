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

use curve25519_dalek::edwards::CompressedEdwardsY;
use crate::commitment::HomomorphicCommitment;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::edwards::EdwardsPoint;
use curve25519_dalek::constants::ED25519_BASEPOINT_COMPRESSED;
use sha2::{Sha256, Digest};
use std::ops::Add;

pub const BASE_POINTS: [CompressedEdwardsY; 10] = [
    CompressedEdwardsY([
        85, 89, 208, 35, 169, 227, 71, 202, 2, 60, 21, 190, 137, 253, 26, 39, 166,
        6, 94, 64, 192, 118, 38, 244, 129, 154, 102, 4, 52, 86, 109, 133]),
    CompressedEdwardsY([
        23, 4, 212, 219, 172, 217, 122, 40, 103, 24, 79, 86, 239, 109, 136, 30, 237,
        100, 7, 229, 188, 77, 230, 102, 198, 133, 219, 169, 18, 194, 97, 105]),
    CompressedEdwardsY([
        169, 90, 226, 164, 67, 1, 198, 99, 28, 162, 143, 71, 1, 139, 95, 6, 55,
        98, 69, 57, 54, 208, 56, 240, 93, 83, 71, 74, 200, 20, 27, 173]),
    CompressedEdwardsY([
        5, 183, 132, 9, 192, 140, 66, 150, 195, 76, 162, 197, 119, 230, 135, 69, 41,
        188, 36, 186, 169, 86, 140, 128, 60, 209, 73, 39, 73, 175, 163, 89]),
    CompressedEdwardsY([
        19, 182, 99, 229, 224, 107, 245, 48, 28, 119, 71, 59, 178, 252, 91, 235, 81,
        228, 4, 110, 155, 126, 254, 242, 246, 209, 163, 36, 203, 139, 16, 148]),
    CompressedEdwardsY([
        192, 8, 29, 78, 244, 209, 61, 50, 2, 121, 234, 239, 168, 181, 30, 185, 28,
        110, 82, 224, 51, 236, 46, 162, 229, 224, 167, 194, 222, 159, 151, 165]),
    CompressedEdwardsY([
        71, 149, 179, 202, 13, 10, 195, 118, 26, 223, 51, 191, 105, 96, 244, 104, 209,
        183, 80, 204, 237, 206, 6, 117, 76, 83, 176, 59, 63, 4, 68, 169]),
    CompressedEdwardsY([
        169, 107, 94, 106, 162, 4, 76, 143, 99, 132, 55, 5, 70, 144, 48, 177, 140,
        93, 251, 33, 8, 44, 74, 148, 190, 198, 213, 55, 162, 155, 60, 147]),
    CompressedEdwardsY([
        240, 223, 115, 125, 65, 250, 119, 113, 67, 34, 207, 175, 223, 141, 236, 100, 88,
        69, 155, 46, 184, 170, 194, 119, 211, 78, 63, 155, 94, 235, 49, 55]),
    CompressedEdwardsY([
        174, 135, 56, 2, 113, 1, 119, 170, 195, 26, 32, 214, 185, 192, 68, 108, 8,
        18, 220, 79, 179, 50, 85, 159, 158, 2, 171, 202, 222, 58, 109, 223]),
];

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(non_snake_case)]
pub struct PedersenBaseOnCurve25519 {
    G: EdwardsPoint,
    H: EdwardsPoint,
}

impl Default for PedersenBaseOnCurve25519 {
    fn default() -> Self {
        PedersenBaseOnCurve25519 {
            G: ED25519_BASEPOINT_COMPRESSED.decompress().unwrap(),
            H: BASE_POINTS[0].decompress().unwrap(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PedersenOnCurve25519<'a> {
    base: &'a PedersenBaseOnCurve25519,
    pub(crate) commitment: CompressedEdwardsY,
}

pub fn nums_generator(n: usize) -> Vec<CompressedEdwardsY> {
    let mut found = 0;
    let mut v = ED25519_BASEPOINT_COMPRESSED.as_bytes().to_vec();
    let mut result = Vec::new();
    while found < n {
        v = Sha256::digest(&v).to_vec();
        let mut a: [u8; 32] = [0; 32];
        a.copy_from_slice(&v[0..32]);
        let y = CompressedEdwardsY(a);
        match y.decompress() {
            None => {}
            Some(_) => {
                result.push(y);
                found += 1;
            }
        }
    }
    result
}


impl<'a> HomomorphicCommitment<'a> for PedersenOnCurve25519<'a> {
    type Base = PedersenBaseOnCurve25519;
    fn new(k: &Scalar, v: &Scalar, base: &'a PedersenBaseOnCurve25519) -> Self {
        let c = (k * base.H + v * base.G).compress();
        PedersenOnCurve25519 {
            base,
            commitment: c,
        }
    }

    fn open(&self, k: &Scalar, v: &Scalar) -> bool {
        let c = (v * self.base.G) + (k * self.base.H);
        c.compress() == self.commitment
    }

    fn commit(&self) -> &[u8] {
        self.commitment.as_bytes()
    }
}

/// Add two commitments together
/// #panics
/// * If the base values are not equal
/// * If either commitment is not a valid point on the curve -- which under normal use cases can't happen
impl<'a, 'b> Add for &'b PedersenOnCurve25519<'a> {
    type Output = PedersenOnCurve25519<'a>;

    fn add(self, rhs: &'b PedersenOnCurve25519) -> Self::Output {
        assert_eq!(self.base, rhs.base, "Bases are unequal");
        let lhp = self.commitment.decompress().expect("Pedersen commitment was not a point on the curve");
        let rhp = rhs.commitment.decompress().expect("Pedersen commitment was not a point on the curve");
        let sum = lhp + rhp;
        PedersenOnCurve25519 {
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
        let base = PedersenBaseOnCurve25519::default();
        assert_eq!(base.G.compress(), ED25519_BASEPOINT_COMPRESSED);
        assert_eq!(base.H.compress(), BASE_POINTS[0])
    }

    /// Calculate the BASE_POINTS array by sequentially hashing the 25519 base point and compare that to the
    /// hard-coded values above
    #[test]
    fn check_nums_values() {
        let v: Vec<EdwardsPoint> = nums_generator(10).iter()
            .map(|x| x.decompress().expect("A BASE_POINT is invalid"))
            .collect();
        for i in 0..10 {
            let calc = v.get(i).unwrap().compress();
            let hard_coded = BASE_POINTS[i];
            assert_eq!(calc, hard_coded);
        }
    }

    #[test]
    fn check_open() {
        let base = PedersenBaseOnCurve25519::default();
        let mut rng = rand::OsRng::new().unwrap();
        for _ in 0..100 {
            let v = Scalar::random(&mut rng);
            let k = Scalar::random(&mut rng);
            let c = PedersenOnCurve25519::new(&k, &v, &base);
            let c_calc = (v * base.G + k * base.H).compress();
            assert_eq!(c.commitment, c_calc);
            assert!(c.open(&k, &v));
            assert!(!c.open(&k, &(&v + &v)));
        }
    }

    #[test]
    fn check_homomorphism() {
        let base = PedersenBaseOnCurve25519::default();
        let mut rng = rand::OsRng::new().unwrap();
        for i in 0..100 {
            let v1 = Scalar::random(&mut rng);
            let v2 = Scalar::random(&mut rng);
            let v_sum = v1 + v2;
            let k1 = Scalar::random(&mut rng);
            let k2 = Scalar::random(&mut rng);
            let k_sum = k1 + k2;
            let c1 = PedersenOnCurve25519::new(&k1, &v1, &base);
            let c2 = PedersenOnCurve25519::new(&k2, &v2, &base);
            let c_sum = &c1 + &c2;
            let c_sum2 = PedersenOnCurve25519::new(&k_sum, &v_sum, &base);
            assert!(c1.open(&k1, &v1));
            assert!(c2.open(&k2, &v2));
            assert_eq!(c_sum, c_sum2);
            println!("{}", i);
            assert!(c_sum.open(&k_sum, &v_sum));
        }
    }

    #[test]
    #[should_panic]
    fn summing_different_bases_panics() {
        let base = PedersenBaseOnCurve25519::default();
        let base2 = PedersenBaseOnCurve25519 {
            G: BASE_POINTS[0].decompress().unwrap(),
            H: BASE_POINTS[1].decompress().unwrap(),
        };
        let v = Scalar::from(100u64);
        let k = Scalar::from(101u64);
        let c1 = PedersenOnCurve25519::new(&k, &v, &base);
        let c2 = PedersenOnCurve25519::new(&k, &v, &base2);
        let _ = &c1 + &c2;
    }
}