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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crate::Hash;
use digest::Digest;

const ALL_ONES: usize = std::usize::MAX;

/// Returns the MMR index of the nth leaf node
pub fn leaf_index(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    2 * n - n.count_ones() as usize
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
pub fn find_peaks(size: usize) -> Vec<usize> {
    if size == 0 {
        return vec![];
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
        return vec![];
    }
    peaks
}

/// Calculates the positions of the parent and sibling of the node at the provided position.
pub fn family(pos: usize) -> (usize, usize) {
    let (peak_map, height) = peak_map_height(pos);
    let peak = 1 << height;
    if (peak_map & peak) != 0 {
        (pos + 1, pos + 1 - 2 * peak)
    } else {
        (pos + 2 * peak, pos + 2 * peak - 1)
    }
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
        if (peak_map & peak) != 0 {
            current += 1;
            sibling = current - 2 * peak;
        } else {
            current += 2 * peak;
            sibling = current - 1;
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
#[inline(always)]
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
    D::new().chain(left).chain(right).result().to_vec()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn leaf_indices() {
        assert_eq!(leaf_index(0), 0);
        assert_eq!(leaf_index(1), 1);
        assert_eq!(leaf_index(2), 3);
        assert_eq!(leaf_index(3), 4);
        assert_eq!(leaf_index(5), 8);
        assert_eq!(leaf_index(6), 10);
        assert_eq!(leaf_index(7), 11);
        assert_eq!(leaf_index(8), 15);
    }
    #[test]
    fn peak_vectors() {
        assert_eq!(find_peaks(0), Vec::<usize>::new());
        assert_eq!(find_peaks(1), vec![0]);
        assert_eq!(find_peaks(3), vec![2]);
        assert_eq!(find_peaks(4), vec![2, 3]);
        assert_eq!(find_peaks(15), vec![14]);
        assert_eq!(find_peaks(23), vec![14, 21, 22]);
    }

    #[test]
    fn peak_map_heights() {
        assert_eq!(peak_map_height(0), (0, 0));
        assert_eq!(peak_map_height(4), (0b11, 0));
        assert_eq!(peak_map_height(9), (0b101, 1));
        assert_eq!(peak_map_height(10), (0b110, 0));
        assert_eq!(peak_map_height(12), (0b111, 1));
        assert_eq!(peak_map_height(33), (0b10001, 1));
        assert_eq!(peak_map_height(34), (0b10010, 0));
    }
    #[test]
    fn is_sibling_left() {
        assert_eq!(is_left_sibling(0), true);
        assert_eq!(is_left_sibling(1), false);
        assert_eq!(is_left_sibling(2), true);
        assert_eq!(is_left_sibling(3), true);
        assert_eq!(is_left_sibling(4), false);
        assert_eq!(is_left_sibling(5), false);
        assert_eq!(is_left_sibling(6), true);
        assert_eq!(is_left_sibling(7), true);
        assert_eq!(is_left_sibling(8), false);
        assert_eq!(is_left_sibling(9), true);
        assert_eq!(is_left_sibling(10), true);
        assert_eq!(is_left_sibling(11), false);
        assert_eq!(is_left_sibling(12), false);
        assert_eq!(is_left_sibling(13), false);
        assert_eq!(is_left_sibling(14), true);
        assert_eq!(is_left_sibling(15), true);
    }

    #[test]
    fn families() {
        assert_eq!(family(1), (2, 0));
        assert_eq!(family(0), (2, 1));
        assert_eq!(family(3), (5, 4));
        assert_eq!(family(9), (13, 12));
        assert_eq!(family(15), (17, 16));
        assert_eq!(family(6), (14, 13));
        assert_eq!(family(13), (14, 6));
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
            (131070, 131069),
            (262142, 262141),
            (524286, 524285),
            (1048574, 1048573),
        ]);
    }
}
