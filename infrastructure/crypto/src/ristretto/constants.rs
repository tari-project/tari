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

use curve25519_dalek::ristretto::CompressedRistretto;

pub const RISTRETTO_NUMS_POINTS: [CompressedRistretto; 10] = [
    CompressedRistretto([144, 202, 17, 205, 108, 98, 39, 203, 10, 188, 57, 226, 113, 12, 68, 74, 230, 97, 126, 168, 24, 152, 231, 22, 53, 63, 52, 16, 217, 101, 102, 5]),
    CompressedRistretto([158, 163, 67, 196, 112, 228, 87, 33, 101, 243, 64, 56, 81, 223, 107, 32, 221, 251, 206, 241, 171, 132, 207, 171, 15, 197, 139, 223, 124, 54, 254, 7]),
    CompressedRistretto([48, 188, 62, 20, 154, 63, 125, 42, 172, 191, 231, 48, 225, 158, 154, 7, 119, 59, 83, 83, 219, 98, 32, 99, 185, 44, 153, 54, 50, 173, 60, 7]),
    CompressedRistretto([142, 16, 136, 206, 212, 150, 29, 136, 213, 177, 113, 189, 154, 52, 40, 68, 84, 120, 154, 69, 95, 70, 236, 55, 82, 145, 49, 33, 36, 183, 30, 108]),
    CompressedRistretto([112, 19, 255, 145, 136, 246, 135, 216, 133, 201, 90, 218, 110, 88, 11, 35, 141, 231, 33, 12, 85, 193, 246, 36, 123, 31, 16, 101, 38, 8, 10, 85]),
    CompressedRistretto([122, 234, 197, 53, 77, 120, 8, 171, 35, 80, 105, 62, 45, 2, 30, 42, 99, 188, 47, 231, 194, 119, 210, 5, 107, 176, 108, 127, 141, 78, 6, 81]),
    CompressedRistretto([228, 224, 63, 227, 33, 214, 87, 20, 172, 223, 193, 247, 88, 37, 111, 121, 204, 69, 49, 213, 30, 143, 121, 244, 15, 194, 105, 198, 196, 117, 160, 65]),
    CompressedRistretto([136, 214, 134, 144, 253, 111, 238, 89, 110, 128, 92, 250, 34, 30, 126, 40, 119, 21, 166, 201, 46, 148, 100, 255, 196, 32, 172, 183, 12, 236, 51, 27]),
    CompressedRistretto([204, 102, 24, 189, 15, 12, 192, 35, 132, 29, 173, 74, 19, 204, 46, 55, 166, 35, 14, 36, 48, 80, 214, 220, 196, 201, 49, 208, 70, 224, 234, 3]),
    CompressedRistretto([96, 230, 255, 101, 87, 7, 198, 66, 73, 210, 250, 146, 78, 49, 146, 182, 149, 220, 88, 44, 180, 246, 214, 140, 180, 43, 155, 49, 24, 147, 237, 64]),
];

pub const RISTRETTO_PEDERSEN_H: CompressedRistretto = RISTRETTO_NUMS_POINTS[0];


#[cfg(test)]
mod test {
    use curve25519_dalek::ristretto::RistrettoPoint;
    use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
    use sha2::{ Digest, Sha512};
    use crate::ristretto::constants::RISTRETTO_NUMS_POINTS;

    /// Generate a set of NUMS points by sequentially hashing the Ristretto255 generator point. By using
    /// `from_uniform_bytes`, the resulting point is a NUMS point if the input bytes are from a uniform distribution.
    fn nums_ristretto(n: usize) -> Vec<RistrettoPoint> {
        let mut v = RISTRETTO_BASEPOINT_POINT.compress().to_bytes();
        let mut result = Vec::new();
        let mut a: [u8; 64] = [0; 64];
        for _ in 0..n {
            let b = Sha512::digest(&v[..]);
            a.copy_from_slice(&b);
            let y = RistrettoPoint::from_uniform_bytes(&a);
            //println!("{:?}", y.compress());
            result.push(y);
            v = y.compress().to_bytes();
        }
        result
    }

    /// Confirm that the [RISTRETTO_NUM_POINTS array](Const.RISTRETTO_NUMS_POINTS.html) is generated with Nothing Up
    /// My Sleeve (NUMS).
    #[test]
    pub fn check_nums_points() {
        let n = RISTRETTO_NUMS_POINTS.len();
        let v_arr = nums_ristretto(n);
        for i in 0..n {
            let nums = RISTRETTO_NUMS_POINTS[i].decompress().unwrap();
            assert_eq!(v_arr[i], nums);
        }
    }
}