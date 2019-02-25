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

// The merkle mountain range was invented by Peter Todd more about them can be ready at:
// https://github.com/opentimestamps/opentimestamps-server/blob/master/doc/merkle-mountain-range.md
// https://github.com/mimblewimble/grin/blob/master/doc/mmr.md
//
// A Merkle mountian range(MMR) is a binary tree in where each parent is the concatenated hash of its two
// children. The leaves at the bottom of the MMR is the hashes of the data. The MMR allows easy to add and proof
// of existence inside of the tree. MMR always tries to have the largest possible single binary tree, so in effect
// it is possible to have more than one binary tree. Every time you have to get the merkle root (the single merkle
// proof of the whole MMR) you have the bag the peaks of the individual trees, or mountain peaks.
//
// Lets take an example of how to construct one. Say you have the following MMR already made:
//
//       /\
//      /  \
//     /\  /\   /\
//    /\/\/\/\ /\/\ /\
//
// From this we can see we have 3 trees or mountains. We have constructed the largest possible tree's we can.
// If we want to calculate the merkle route we will bag each of the mountains in the following way
//
//          /\
//         /\ \
//        /  \ \
//       /\   \ \
//      /  \   \ \
//     /\  /\  /\ \
//    /\/\/\/\/\/\/\
//
// Lets continue thw example, by adding a single object. Our MMR now looks as follows
//
//       /\
//      /  \
//     /\  /\   /\
//    /\/\/\/\ /\/\ /\ /
//
// We now have 4 mountains. Lets bag and calculate the merkle root again
//
//           /\
//          /\ \
//         /\ \ \
//        /  \ \ \
//       /\   \ \ \
//      /  \   \ \ \
//     /\  /\  /\ \ \
//    /\/\/\/\/\/\/\ \
//
//  Lets continue thw example, by adding a single object. Our MMR now looks as follows
//
//           /\
//          /  \
//         /    \
//        /      \
//       /\      /\
//      /  \    /  \
//     /\  /\  /\  /\
//    /\/\/\/\/\/\/\/\
//
// Now we only have a single binary tree, we dont have to bag the mountains to calculate the merkle root. This
// process continues as you add more objects to the MMR.
//
//                 /\
//                /  \
//               /    \
//              /      \
//             /        \
//            /          \
//           /            \
//          /\             \
//         /\ \            /\
//        /  \ \          /  \
//       /\   \ \        /\   \
//      /  \   \ \      /  \   \
//     /\  /\  /\ \    /\  /\  /\
//    /\/\/\/\/\/\/\  /\/\/\/\/\/\
//
// Due to the unique way the MMR is constructed we can easily represent the MMR as a list of the nodes, as when
// adding nodes you only append. Lets take the following MMR and number the nodes in the order we create them.
//
//        7
//       /  \
//      /    \
//     3      6
//    / \    / \
//   1   2  4   5
//
// Looking above at the example of when you create the nodes, you will see the nodes will have been created in the
// order as they are named. This means we can easily represent them as a list:
// Height:  0 | 0 | 1 | 0 | 0 | 1 | 2
// Node:    1 | 2 | 3 | 4 | 5 | 6 | 7
//
// Because of the list nature of the MMR we can easily navigate around the MMR using the following formulas:
// Jump to peer : 2^(H+1) -1
// find peak : 2^(H+1) -2 where < total elements
// left down : 2^H
// right down: -1
// height : log2(I+2) -1
// Pleas note that the formulas are for direct indexs in array's meaning the nodes count from 0 and not 1 as in
// the examples above. H - Height
// I - Index
//
// Pruning the MMR means removing flagging an node as pruned and only removing it if its peer has been removed.
// We do this as we require the peer to prove the hash of the node. Taking the above example, let prune leave 1.
//
//                 /\
//                /  \
//               /    \
//              /      \
//             /        \
//            /          \
//           /            \
//          /\             \
//         /\ \            /\
//        /  \ \          /  \
//       /\   \ \        /\   \
//      /  \   \ \      /  \   \
//     /\  /\  /\ \    /\  /\  /\
//    /\/\/\/\/\/\/\  /\/\/\/\/\/\
//
// Node 1 has now only been marked as pruned but we cannot remove it as of yet because we still require it to
// prove node 2. When we prune node 2, the MMR looks as follows
//
//                 /\
//                /  \
//               /    \
//              /      \
//             /        \
//            /          \
//           /            \
//          /\             \
//         /\ \            /\
//        /  \ \          /  \
//       /\   \ \        /\   \
//      /  \   \ \      /  \   \
//     /\  /\  /\ \    /\  /\  /\
//      /\/\/\/\/\/\  /\/\/\/\/\/\
//
// Although we have no removed node 1 and node 2 from the MMR we cannot yet remove node 3 as we require node 3 for
// the proof of node 6. Lets prune 4 and 5.
//
//                 /\
//                /  \
//               /    \
//              /      \
//             /        \
//            /          \
//           /            \
//          /\             \
//         /\ \            /\
//        /  \ \          /  \
//       /\   \ \        /\   \
//      /  \   \ \      /  \   \
//         /\  /\ \    /\  /\  /\
//        /\/\/\/\/\  /\/\/\/\/\/\
//
// Now we removed 3 from the MMR

use crate::merklenode::{Hashable, MerkleNode, ObjectHash};
use std::collections::HashMap;

pub struct MerkleMountainRange<T>
where T: Hashable
{
    mmr: Vec<MerkleNode>,
    data: HashMap<ObjectHash, T>,
}

impl<T> MerkleMountainRange<T>
where T: Hashable
{
    /// This function creates a new empty Merkle Mountain Range
    pub fn new() -> MerkleMountainRange<T> {
        MerkleMountainRange { mmr: Vec::new(), data: HashMap::new() }
    }

    /// This function adds an vec of leaf nodes to the mmr.
    pub fn add_vec(&mut self, objects: Vec<T>) {
        for object in objects {
            self.add_single(object);
        }
    }

    /// This function adds a new leaf node to the mmr.
    pub fn add_single(&mut self, object: T) {
        let node_hash = object.get_hash();
        let node = MerkleNode::new(node_hash);
        self.mmr.push(node);
        if is_node_right((self.mmr.len() - 1) as u64) {
            self.data.insert(node_hash, object);
            self.add_single_no_leaf((self.mmr.len() - 1) as u64)
        }
    }

    //  //This function adds non leaf nodes, eg nodes that are not directly a hash of data
    // This is iterative and will continue to add up and till it hits the top, will be a future left child
    fn add_single_no_leaf(&mut self, index: u64) {
        let new_hash =
            self.data.get(&self.mmr[index as usize].hash).unwrap().concat(self.mmr[peer_index(index) as usize].hash);

        let new_node = MerkleNode::new(new_hash);
        self.mmr.push(new_node);
        if is_node_right((self.mmr.len() - 1) as u64) {
            self.add_single_no_leaf((self.mmr.len() - 1) as u64)
        }
    }
}
/// This function takes in the index and calculates the index of the peer.
pub fn peer_index(index: u64) -> u64 {
    let index_count = u64::pow(2, index as u32 + 1) - 1;
    if is_node_right(index) {
        index - index_count
    } else {
        index + index_count
    }
}

// This function takes in the index and calculates if the node is right child node or not.
// If the node is the tree root it will still give the answer as if is a child of a node
pub fn is_node_right(_index: u64) -> bool {
    true // todo figure out math formula required to do this
}
