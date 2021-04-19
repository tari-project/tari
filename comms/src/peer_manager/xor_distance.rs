//  Copyright 2021 The Tari Project
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

use crate::peer_manager::node_id::{NodeIdError, NODE_ID_ARRAY_SIZE};
use std::convert::{TryInto, TryFrom};
use crate::peer_manager::NodeId;
use std::{fmt, cmp};
use tari_crypto::tari_utilities::{
    ByteArray,
};
use nom::lib::std::collections::VecDeque;

const NODE_XOR_DISTANCE_ARRAY_SIZE: usize = 16;

#[derive(Clone, Debug, Eq, PartialOrd, Ord, Default, Copy)]
pub struct XorDistance(u128);

impl XorDistance {
    /// Construct a new zero distance
    pub fn new() -> Self {
        Self(0)
    }

    /// Calculate the distance between two node ids using the Hamming distance.
    pub fn from_node_ids(x: &NodeId, y: &NodeId) -> Self {
        xor(x.as_bytes(), y.as_bytes())
    }

    /// Returns the maximum distance.
    pub const fn max_distance() -> Self {
        Self(u128::MAX >> ((NODE_XOR_DISTANCE_ARRAY_SIZE - NODE_ID_ARRAY_SIZE) * 8))
    }

    /// Returns a zero distance.
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Returns the number of bytes required to represent the `XorDistance`
    pub const fn byte_length() -> usize {
        NODE_XOR_DISTANCE_ARRAY_SIZE
    }

    pub fn get_bucket(&self, num_buckets: u32) -> (XorDistance, XorDistance, u32) {
        // let bits_per_bucket = cmp::max((NODE_XOR_DISTANCE_ARRAY_SIZE * 8) as u32 / num_buckets, 1);

        let mut max:u128 = XorDistance::max_distance().0;
        let mut min = max.checked_shr(1).unwrap_or_default();
        let val:u128  = self.0;
        let mut bucket_no = num_buckets  - 1;
        if bucket_no == 0 {
            return (XorDistance(0), XorDistance(max), 0);
        }
        while min > 0 && val < min {
            max = min;
            min = max.checked_shr(1).unwrap_or_default();
            bucket_no -= 1;
            if bucket_no == 0 {
                return (XorDistance(0), XorDistance(max), 0)
            }
        }

        (XorDistance(min), XorDistance(max), bucket_no)
    }

    // TODO: write unit tests
    pub fn get_buckets(num_buckets: u32) -> Vec<(XorDistance, XorDistance, u32)> {
        // let bits_per_bucket = cmp::max((NODE_XOR_DISTANCE_ARRAY_SIZE * 8) as u32 / num_buckets, 1);

        let mut buckets = VecDeque::new();
        let mut max:u128 = XorDistance::max_distance().0;
        let mut min = max.checked_shr(1).unwrap_or_default();
        let mut bucket_no = num_buckets;
        while min > 0 && bucket_no > 0 {
            buckets.push_front((XorDistance(min), XorDistance(max), bucket_no));
            max = min;
            min = max.checked_shr(1).unwrap_or_default();
            bucket_no -= 1;
        }

        buckets.push_front((XorDistance(0), XorDistance(max), bucket_no));
        buckets.into()
    }


    // fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
    //     bytes
    //         .try_into()
    //         .map_err(|err| ByteArrayError::ConversionError(format!("{:?}", err)))
    // }
}

fn xor(x: &[u8], y: &[u8]) -> XorDistance {
    let mut nd = [0u8; NODE_ID_ARRAY_SIZE];
    for i in 0..nd.len() {
        nd[i] = x[i] ^ y[i];
    }
    nd.as_ref().try_into().unwrap()
}


impl PartialEq for XorDistance {
    fn eq(&self, nd: &XorDistance) -> bool {
        self.0 == nd.0
    }
}

impl TryFrom<&[u8]> for XorDistance {
    type Error = NodeIdError;

    /// Construct a node distance from a set of bytes
    fn try_from(elements: &[u8]) -> Result<Self, Self::Error> {
        let mut bytes = [0; NODE_XOR_DISTANCE_ARRAY_SIZE];
        let start = cmp::max(NODE_XOR_DISTANCE_ARRAY_SIZE - elements.len(), 0);
        for i in start..NODE_XOR_DISTANCE_ARRAY_SIZE {
            bytes[i] = elements[i - start];
        }
        Ok(XorDistance(u128::from_be_bytes(bytes)))
    }
}

impl From<XorDistance> for u128 {

    fn from(value: XorDistance) -> Self {
       value.0
    }
}


// impl ByteArray for XorDistance {
    /// Try and convert the given byte array to a NodeDistance. Any failures (incorrect array length,
    /// implementation-specific checks, etc) return a [ByteArrayError](enum.ByteArrayError.html).
    // fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
    //     bytes
    //         .try_into()
    //         .map_err(|err| ByteArrayError::ConversionError(format!("{:?}", err)))
    // }

//     /// Return the NodeDistance as a byte array
//     fn as_bytes(&self) -> &[u8] {
//         self.0.to_be_bytes().as_ref()
//     }
// }

impl fmt::Display for XorDistance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

#[cfg(test)]
mod tests {

    // use super::*;

    #[test]
    fn get_bucket() {
        let x = XorDistance::new();
        let bucket =  x.get_bucket(1);
        assert_eq!(bucket.0, XorDistance::zero());
        assert_eq!(bucket.1, XorDistance::max_distance());


        let x = XorDistance::new();
        let bucket =  x.get_bucket(20);
        assert_eq!(bucket.0, XorDistance::zero());
        assert_eq!(bucket.1, XorDistance(81129638414606681695789005144063));

        let node_id1 = NodeId::from_bytes(
            &[
                144u8, 28u8, 106u8, 112u8,
                220u8, 197u8, 216u8, 119u8,
                9u8, 217u8, 42u8, 77u8,
                1u8
            ]

        ).unwrap();

        let node_id2 = NodeId::from_bytes(
            &[
                220u8, 197u8, 216u8, 119u8,
                144u8, 28u8, 106u8, 112u8,
                9u8, 217u8, 42u8, 77u8,
                1u8
            ]

        ).unwrap();

        // Intentionally close to node_id2
        let node_id3 =NodeId::from_bytes(
            &[
                220u8, 197u8, 216u8, 119u8,
                144u8, 28u8, 106u8, 112u8,
                9u8, 217u8, 0u8, 0u8,
                1u8
            ]

        ).unwrap();

        // Node 3 but with last bit changed
        let node_id4 =NodeId::from_bytes(
            &[
                220u8, 197u8, 216u8, 119u8,
                144u8, 28u8, 106u8, 112u8,
                9u8, 217u8, 0u8, 0u8,
                0u8
            ]

        ).unwrap();

        let x = node_id1.distance(&node_id2);
        let bucket =  x.get_bucket(2);
        assert_eq!(bucket.0, XorDistance(0));
        assert_eq!(bucket.1, XorDistance(21267647932558653966460912964485513215));

        let bucket =  x.get_bucket(8);
        assert_eq!(bucket.0, XorDistance(0));
        assert_eq!(bucket.1, XorDistance(332306998946228968225951765070086143));

        let bucket =  x.get_bucket(16);
        assert_eq!(bucket.0, XorDistance(0));
        assert_eq!(bucket.1, XorDistance(1298074214633706907132624082305023));

        let bucket =  x.get_bucket(32);
        assert_eq!(bucket.0, XorDistance(5070602400912917605986812821503));
        assert_eq!(bucket.1, XorDistance(10141204801825835211973625643007));


        // test an odd number
        let bucket =  x.get_bucket(33);
        assert_eq!(bucket.0, XorDistance(5070602400912917605986812821503));
        assert_eq!(bucket.1, XorDistance(10141204801825835211973625643007));

        let dist_2_3 = node_id2.distance(&node_id3);
        let bucket =  dist_2_3.get_bucket(12);
        assert_eq!(bucket.0, XorDistance(0));
        assert_eq!(bucket.1,XorDistance(20769187434139310514121985316880383));

        let bucket =  dist_2_3.get_bucket(128);
        assert_eq!(bucket.0, XorDistance(2097151));
        assert_eq!(bucket.1,XorDistance(4194303));

        let dist_3_4 = node_id3.distance(&node_id4);
        let bucket =  dist_3_4.get_bucket(128);
        assert_eq!(bucket.0, XorDistance(1));
        assert_eq!(bucket.1, XorDistance(3));


        let dist_4_4 = node_id4.distance(&node_id4);
        assert_eq!(dist_4_4, XorDistance::zero());
        assert_eq!(dist_4_4.get_bucket(128).0, XorDistance(0));
    }


    #[test]
    fn convert_xor_distance_to_u128() {
        let node_id1 = NodeId::try_from(
            [
                144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159, 211, 53, 207, 0, 157, 5, 55, 235, 247, 160,
                195, 240, 48, 146, 168, 119, 15, 241, 54,
            ]
                .as_bytes(),
        )
            .unwrap();
        let node_id2 = NodeId::try_from(
            [
                186, 43, 62, 14, 60, 214, 9, 180, 145, 122, 55, 160, 83, 83, 45, 185, 219, 206, 226, 128, 5, 26, 20, 0,
                192, 121, 216, 178, 134, 212, 51, 131,
            ]
                .as_bytes(),
        )
            .unwrap();
        let node_id3 = NodeId::try_from(
            [
                60, 32, 246, 39, 108, 201, 214, 91, 30, 230, 3, 126, 31, 46, 66, 203, 27, 51, 240, 177, 230, 22, 118,
                102, 201, 55, 211, 147, 229, 26, 116, 103,
            ]
                .as_bytes(),
        )
            .unwrap();
        let n1_to_n2_dist = node_id1.distance(&node_id2);
        let n1_to_n3_dist = node_id1.distance(&node_id3);
        assert!(n1_to_n2_dist < n1_to_n3_dist);
        let n12_distance = n1_to_n2_dist.0;
        let n13_distance = n1_to_n3_dist.0;
        assert!(n12_distance < n13_distance);
        assert_eq!(n12_distance, 56114865924689668092413877285545836544);
        assert_eq!(n13_distance, 228941924089749863963604860508980641792);
    }
}
