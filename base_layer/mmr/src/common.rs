// Copyright 2019. The Tari Project
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

// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::convert::TryInto;

use digest::Digest;

use crate::{error::MerkleMountainRangeError, Hash};

const ALL_ONES: usize = usize::MAX;

#[derive(Copy, Clone)]
pub struct LeafIndex(pub usize);

/// Returns the MMR node index derived from the leaf index.
pub fn node_index(leaf_index: LeafIndex) -> usize {
    if leaf_index.0 == 0 {
        return 0;
    }
    2 * leaf_index.0 - leaf_index.0.count_ones() as usize
}

/// Returns the leaf index derived from the MMR node index.
pub fn leaf_index(node_index: u32) -> u32 {
    let n = checked_n_leaves(node_index as usize)
        .expect("checked_n_leaves can only overflow for `usize::MAX` and that is not possible");
    // Conversion is safe because n < node_index
    n.try_into().unwrap()
}

/// Is this position a leaf in the MMR?
/// We know the positions of all leaves based on the postorder height of an MMR of any size (somewhat unintuitively
/// but this is how the PMMR is "append only").
pub fn is_leaf(pos: usize) -> bool {
    bintree_height(pos) == 0
}

/// Gets the postorder traversal index of all peaks in a MMR given its size.
/// Starts with the top peak, which is always on the left side of the range, and navigates toward lower siblings
/// toward the right  of the range.
pub fn find_peaks(size: usize) -> Option<Vec<usize>> {
    if size == 0 {
        return Some(vec![]);
    }
    let mut peak_size = ALL_ONES >> size.leading_zeros();
    let mut num_left = size;
    let mut sum_prev_peaks = 0;
    let mut peaks = vec![];
    while peak_size != 0 {
        if num_left >= peak_size {
            peaks.push(sum_prev_peaks + peak_size - 1);
            sum_prev_peaks += peak_size;
            num_left -= peak_size;
        }
        peak_size >>= 1;
    }
    if num_left > 0 {
        // This happens, whenever the MMR is not valid, that is, all nodes are not
        // fully spawned. For example, in this case
        //    2
        //   / \
        //  0   1   3   4
        // is invalid, as it can be completed to form
        //       6
        //     /    \
        //    2      5
        //  /  \    /  \
        // 0    1  3    4
        // which is of size 7 (with single peak [6])
        return None;
    }
    Some(peaks)
}

/// Calculates the positions of the (parent, sibling) of the node at the provided position.
/// Returns an error if the pos provided would result in an underflow or overflow.
pub fn family(pos: usize) -> Result<(usize, usize), MerkleMountainRangeError> {
    let (peak_map, height) = peak_map_height(pos);
    let peak = 1 << height;

    // Convert to i128 so that we don't over/underflow, and then we will cast back to usize after
    let pos = pos as i128;
    let peak = i128::from(peak);
    let peak_map = peak_map as i128;

    let res = if (peak_map & peak) == 0 {
        (pos + 2 * peak, pos + 2 * peak - 1)
    } else {
        (pos + 1, pos + 1 - 2 * peak)
    };

    Ok((
        res.0.try_into().map_err(|_| MerkleMountainRangeError::OutOfRange)?,
        res.1.try_into().map_err(|_| MerkleMountainRangeError::OutOfRange)?,
    ))
}

/// For a given starting position calculate the parent and sibling positions
/// for the branch/path from that position to the peak of the tree.
/// We will use the sibling positions to generate the "path" of a Merkle proof.
pub fn family_branch(pos: usize, last_pos: usize) -> Vec<(usize, usize)> {
    // loop going up the tree, from node to parent, as long as we stay inside
    // the tree (as defined by last_pos).
    let (peak_map, height) = peak_map_height(pos);
    let mut peak = 1 << height;
    let mut branch = vec![];
    let mut current = pos;
    let mut sibling;
    while current < last_pos {
        if (peak_map & peak) == 0 {
            current += 2 * peak;
            sibling = current - 1;
        } else {
            current += 1;
            sibling = current - 2 * peak;
        };
        if current > last_pos {
            break;
        }
        branch.push((current, sibling));
        peak <<= 1;
    }
    branch
}

/// The height of a node in a full binary tree from its index.
pub fn bintree_height(num: usize) -> usize {
    if num == 0 {
        return 0;
    }
    peak_map_height(num).1
}

/// return (peak_map, pos_height) of given 0-based node pos prior to its addition
/// Example: on input 4 returns (0b11, 0) as mmr state before adding 4 was
///    2
///   / \
///  0   1   3
/// with 0b11 indicating presence of peaks of height 0 and 1.
/// NOTE:
/// the peak map also encodes the path taken from the root to the added node since the path turns left (resp. right)
/// if-and-only-if a peak at that height is absent (resp. present)
pub fn peak_map_height(mut pos: usize) -> (usize, usize) {
    if pos == 0 {
        return (0, 0);
    }
    let mut peak_size = ALL_ONES >> pos.leading_zeros();
    let mut bitmap = 0;
    while peak_size != 0 {
        bitmap <<= 1;
        if pos >= peak_size {
            pos -= peak_size;
            bitmap |= 1;
        }
        peak_size >>= 1;
    }
    (bitmap, pos)
}

/// Is the node at this pos the "left" sibling of its parent?
pub fn is_left_sibling(pos: usize) -> bool {
    let (peak_map, height) = peak_map_height(pos);
    let peak = 1 << height;
    (peak_map & peak) == 0
}

pub fn hash_together<D: Digest>(left: &[u8], right: &[u8]) -> Hash {
    D::new().chain_update(left).chain_update(right).finalize().to_vec()
}

/// The number of leaves in a MMR of the provided size.
/// Example: on input 5 returns (2 + 1 + 1) as mmr state before adding 5 was
///    2
///   / \
///  0   1   3   4
/// None is returned if the number of leaves exceeds the maximum value of a usize
pub fn checked_n_leaves(size: usize) -> Option<usize> {
    if size == 0 {
        return Some(0);
    }
    if size == usize::MAX {
        return None;
    }

    let mut peak_size = ALL_ONES >> size.leading_zeros();
    let mut nleaves = 0usize;
    let mut size_left = size;
    while peak_size != 0 {
        if size_left >= peak_size {
            nleaves += (peak_size + 1) >> 1;
            size_left -= peak_size;
        }
        peak_size >>= 1;
    }

    if size_left == 0 {
        Some(nleaves)
    } else {
        Some(nleaves + 1)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn leaf_to_node_indices() {
        assert_eq!(node_index(LeafIndex(0)), 0);
        assert_eq!(node_index(LeafIndex(1)), 1);
        assert_eq!(node_index(LeafIndex(2)), 3);
        assert_eq!(node_index(LeafIndex(3)), 4);
        assert_eq!(node_index(LeafIndex(5)), 8);
        assert_eq!(node_index(LeafIndex(6)), 10);
        assert_eq!(node_index(LeafIndex(7)), 11);
        assert_eq!(node_index(LeafIndex(8)), 15);
    }

    #[test]
    fn n_leaf_nodes() {
        assert_eq!(checked_n_leaves(0), Some(0));
        assert_eq!(checked_n_leaves(1), Some(1));
        assert_eq!(checked_n_leaves(3), Some(2));
        assert_eq!(checked_n_leaves(4), Some(3));
        assert_eq!(checked_n_leaves(5), Some(4));
        assert_eq!(checked_n_leaves(8), Some(5));
        assert_eq!(checked_n_leaves(10), Some(6));
        assert_eq!(checked_n_leaves(11), Some(7));
        assert_eq!(checked_n_leaves(15), Some(8));
        assert_eq!(checked_n_leaves(usize::MAX - 1), Some(9223372036854775808));
        // Overflowed
        assert_eq!(checked_n_leaves(usize::MAX), None);
    }

    #[test]
    fn peak_vectors() {
        assert_eq!(find_peaks(0), Some(Vec::<usize>::new()));
        assert_eq!(find_peaks(1), Some(vec![0]));
        assert_eq!(find_peaks(2), None);
        assert_eq!(find_peaks(3), Some(vec![2]));
        assert_eq!(find_peaks(4), Some(vec![2, 3]));
        assert_eq!(find_peaks(5), None);
        assert_eq!(find_peaks(6), None);
        assert_eq!(find_peaks(7), Some(vec![6]));
        assert_eq!(find_peaks(8), Some(vec![6, 7]));
        assert_eq!(find_peaks(9), None);
        assert_eq!(find_peaks(10), Some(vec![6, 9]));
        assert_eq!(find_peaks(11), Some(vec![6, 9, 10]));
        assert_eq!(find_peaks(12), None);
        assert_eq!(find_peaks(13), None);
        assert_eq!(find_peaks(14), None);
        assert_eq!(find_peaks(15), Some(vec![14]));
        assert_eq!(find_peaks(16), Some(vec![14, 15]));
        assert_eq!(find_peaks(17), None);
        assert_eq!(find_peaks(18), Some(vec![14, 17]));
        assert_eq!(find_peaks(19), Some(vec![14, 17, 18]));
        assert_eq!(find_peaks(20), None);
        assert_eq!(find_peaks(21), None);
        assert_eq!(find_peaks(22), Some(vec![14, 21]));
        assert_eq!(find_peaks(23), Some(vec![14, 21, 22]));
        assert_eq!(find_peaks(24), None);
        assert_eq!(find_peaks(25), Some(vec![14, 21, 24]));
        assert_eq!(find_peaks(26), Some(vec![14, 21, 24, 25]));
        assert_eq!(find_peaks(27), None);
        assert_eq!(find_peaks(28), None);
        assert_eq!(find_peaks(56), Some(vec![30, 45, 52, 55]));
        assert_eq!(find_peaks(60), None);
        assert_eq!(find_peaks(123), None);
        assert_eq!(find_peaks(130), Some(vec![126, 129]));
    }

    #[test]
    fn peak_map_heights() {
        assert_eq!(peak_map_height(0), (0, 0));
        assert_eq!(peak_map_height(4), (0b11, 0));
        //      6
        //   2      5
        // 0   1  3  4  7  8
        assert_eq!(peak_map_height(9), (0b101, 1));
        //      6
        //   2      5     9
        // 0   1  3  4  7   8  *
        assert_eq!(peak_map_height(10), (0b110, 0));
        assert_eq!(peak_map_height(12), (0b111, 1));
        assert_eq!(peak_map_height(33), (0b10001, 1));
        assert_eq!(peak_map_height(34), (0b10010, 0));
    }
    #[test]
    fn is_sibling_left() {
        assert!(is_left_sibling(0));
        assert!(!is_left_sibling(1));
        assert!(is_left_sibling(2));
        assert!(is_left_sibling(3));
        assert!(!is_left_sibling(4));
        assert!(!is_left_sibling(5));
        assert!(is_left_sibling(6));
        assert!(is_left_sibling(7));
        assert!(!is_left_sibling(8));
        assert!(is_left_sibling(9));
        assert!(is_left_sibling(10));
        assert!(!is_left_sibling(11));
        assert!(!is_left_sibling(12));
        assert!(!is_left_sibling(13));
        assert!(is_left_sibling(14));
        assert!(is_left_sibling(15));
    }

    #[test]
    fn families() {
        assert_eq!(family(1).unwrap(), (2, 0));
        assert_eq!(family(0).unwrap(), (2, 1));
        assert_eq!(family(3).unwrap(), (5, 4));
        assert_eq!(family(9).unwrap(), (13, 12));
        assert_eq!(family(15).unwrap(), (17, 16));
        assert_eq!(family(6).unwrap(), (14, 13));
        assert_eq!(family(13).unwrap(), (14, 6));
    }

    #[test]
    fn family_branches() {
        // A 3 node tree (height 1)
        assert_eq!(family_branch(0, 2), [(2, 1)]);
        assert_eq!(family_branch(1, 2), [(2, 0)]);
        assert_eq!(family_branch(2, 2), []);

        // leaf node in a larger tree of 7 nodes (height 2)
        assert_eq!(family_branch(0, 6), [(2, 1), (6, 5)]);

        // note these only go as far up as the local peak, not necessarily the single root
        assert_eq!(family_branch(0, 3), [(2, 1)]);
        // pos 4 in a tree of size 4 is a local peak
        assert_eq!(family_branch(3, 3), []);
        // pos 4 in a tree of size 5 is also still a local peak
        assert_eq!(family_branch(3, 4), []);
        // pos 4 in a tree of size 6 has a parent and a sibling
        assert_eq!(family_branch(3, 5), [(5, 4)]);
        // a tree of size 7 is all under a single root
        assert_eq!(family_branch(3, 6), [(5, 4), (6, 2)]);

        // A tree with over a million nodes in it find the "family path" back up the tree from a leaf node at 0.
        // Note: the first two entries in the branch are consistent with a small 7 node tree.
        // Note: each sibling is on the left branch, this is an example of the  largest possible list of peaks
        // before we start combining them into larger peaks.
        assert_eq!(family_branch(0, 1_048_999), [
            (2, 1),
            (6, 5),
            (14, 13),
            (30, 29),
            (62, 61),
            (126, 125),
            (254, 253),
            (510, 509),
            (1022, 1021),
            (2046, 2045),
            (4094, 4093),
            (8190, 8189),
            (16382, 16381),
            (32766, 32765),
            (65534, 65533),
            (131_070, 131_069),
            (262_142, 262_141),
            (524_286, 524_285),
            (1_048_574, 1_048_573),
        ]);
    }

    #[test]
    fn find_peaks_when_num_left_gt_zero() {
        assert!(find_peaks(0).unwrap().is_empty());
        assert_eq!(find_peaks(1).unwrap(), vec![0]);
        assert_eq!(find_peaks(2), None);
        assert_eq!(find_peaks(3).unwrap(), vec![2]);
        assert_eq!(find_peaks(usize::MAX).unwrap(), [18446744073709551614].to_vec());
        assert_eq!(find_peaks(usize::MAX - 1), None);
    }
}
