//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    convert::{TryFrom, TryInto},
    fmt,
    mem,
};

use super::{node_id::NodeIdError, NodeId};

/// The distance metric used by the [PeerManager](super::PeerManager).
pub type NodeDistance = XorDistance;

/// The XOR distance metric.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct XorDistance(u128);

impl XorDistance {
    /// Construct a new zero distance
    pub fn new() -> Self {
        Self(0)
    }

    /// Calculate the distance between two node ids using the XOR metric.
    pub fn from_node_ids(x: &NodeId, y: &NodeId) -> Self {
        let arr = x ^ y;
        arr[..]
            .try_into()
            .expect("unreachable panic: NodeId::byte_size() <= NodeDistance::byte_size()")
    }

    /// Returns the maximum distance.
    pub const fn max_distance() -> Self {
        Self(u128::MAX)
    }

    /// Returns a zero distance.
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Returns the number of bytes required to represent the `XorDistance`
    pub const fn byte_size() -> usize {
        mem::size_of::<u128>()
    }

    /// Returns the bucket that this distance falls between.
    /// The node distance falls between the `i`th bucket if 2^i <= distance < 2^(i+1).
    pub fn get_bucket_index(&self) -> u8 {
        ((u8::try_from(Self::byte_size()).unwrap() * 8) - u8::try_from(self.0.leading_zeros()).unwrap())
            .saturating_sub(1)
    }

    /// Byte representation of the distance value.
    pub fn to_bytes(&self) -> [u8; Self::byte_size()] {
        self.0.to_be_bytes()
    }

    /// Distance represented as a 128-bit unsigned integer.
    pub fn as_u128(&self) -> u128 {
        self.0
    }
}

impl TryFrom<&[u8]> for XorDistance {
    type Error = NodeIdError;

    /// Construct a node distance from a set of bytes
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() > Self::byte_size() {
            return Err(NodeIdError::IncorrectByteCount);
        }

        let mut buf = [0; Self::byte_size()];
        // Big endian has the MSB at index 0, if size of `bytes` is less than byte_size it must be offset to have
        // leading 0 bytes
        let offset = Self::byte_size() - bytes.len();
        buf[offset..].copy_from_slice(bytes);
        Ok(XorDistance(u128::from_be_bytes(buf)))
    }
}

impl fmt::Display for NodeDistance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for XorDistance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut digits = 0;
        let mut suffix = "";
        loop {
            let prefix = self.0 / u128::pow(10, 3 * (digits + 1));

            if prefix == 0 || digits > 8 {
                return write!(f, "XorDist: {}{}", self.0 / u128::pow(10, 3 * digits), suffix);
            }

            digits += 1;
            suffix = match suffix {
                "" => "thousand",
                "thousand" => "million",
                "million" => "billion",
                "billion" => "trillion",
                "trillion" => "quadrillion",
                "quadrillion" => "quintillion",
                "quintillion" => "sextillion",
                "sextillion" => "septillion",
                "septillion" => "e24",
                _ => suffix,
            }
        }
    }
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_crypto::keys::PublicKey;

    use super::*;
    use crate::types::CommsPublicKey;

    mod ord {
        use super::*;

        #[test]
        fn it_uses_big_endian_ordering() {
            let a = NodeDistance::try_from(&[0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1][..]).unwrap();
            let b = NodeDistance::try_from(&[1u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0][..]).unwrap();
            assert!(a < b);
        }
    }

    mod get_bucket_index {
        use super::*;

        #[test]
        fn it_returns_the_correct_index() {
            fn check_for_dist(lsb_dist: u8, expected: u8) {
                assert_eq!(
                    NodeDistance::try_from(&[0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, lsb_dist][..])
                        .unwrap()
                        .get_bucket_index(),
                    expected,
                    "Failed for dist = {}",
                    lsb_dist
                );
            }

            assert_eq!(NodeDistance::max_distance().get_bucket_index(), 127);
            assert_eq!(NodeDistance::zero().get_bucket_index(), 0);

            check_for_dist(1, 0);
            for i in 2..4 {
                check_for_dist(i, 1);
            }
            for i in 4..8 {
                check_for_dist(i, 2);
            }
            for i in 8..16 {
                check_for_dist(i, 3);
            }
            assert_eq!(
                NodeDistance::try_from(&[0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0b01000001, 0, 0][..])
                    .unwrap()
                    .get_bucket_index(),
                8 * 2 + 7 - 1
            );

            assert_eq!(
                NodeDistance::try_from(&[0b10000000u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0][..])
                    .unwrap()
                    .get_bucket_index(),
                103
            );
        }

        #[test]
        fn correctness_fuzzing() {
            for _ in 0..100 {
                let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
                let a = NodeId::from_public_key(&pk);
                let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
                let b = NodeId::from_public_key(&pk);
                let dist = NodeDistance::from_node_ids(&a, &b);
                let i = u32::from(dist.get_bucket_index());
                let dist = dist.as_u128();
                assert!(2u128.pow(i) <= dist, "Failed for {}, i = {}", dist, i);
                assert!(dist < 2u128.pow(i + 1), "Failed for {}, i = {}", dist, i,);
            }
        }
    }
}
