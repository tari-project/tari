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

//! The Merkle mountain range was invented by Peter Todd more about them can be read at:
//! https://github.com/opentimestamps/opentimestamps-server/blob/master/doc/merkle-mountain-range.md
//! https://github.com/mimblewimble/grin/blob/master/doc/mmr.md
//!
//! A Merkle mountain range(MMR) is a binary tree where each parent is the concatenated hash of its two
//! children. The leaves at the bottom of the MMR is the hashes of the data. The MMR allows easy to add and proof
//! of existence inside of the tree. MMR always tries to have the largest possible single binary tree, so in effect
//! it is possible to have more than one binary tree. Every time you have to get the merkle root (the single merkle
//! proof of the whole MMR) you have the bag the peaks of the individual trees, or mountain peaks.
//!
//! Lets take an example of how to construct one. Say you have the following MMR already made:
//! ```plaintext
//!       /\
//!      /  \
//!     /\  /\   /\
//!    /\/\/\/\ /\/\ /\
//! ```
//! From this we can see we have 3 trees or mountains. We have constructed the largest possible tree's we can.
//! If we want to calculate the merkle route we will bag each of the mountains in the following way
//! ```plaintext
//!          /\
//!         /\ \
//!        /  \ \
//!       /\   \ \
//!      /  \   \ \
//!     /\  /\  /\ \
//!    /\/\/\/\/\/\/\
//! ```
//! Lets continue the example, by adding a single object. Our MMR now looks as follows
//! ```plaintext
//!       /\
//!      /  \
//!     /\  /\   /\
//!    /\/\/\/\ /\/\ /\ /
//! ```
//! We now have 4 mountains. Lets bag and calculate the merkle root again
//! ```plaintext
//!           /\
//!          /\ \
//!         /\ \ \
//!        /  \ \ \
//!       /\   \ \ \
//!      /  \   \ \ \
//!     /\  /\  /\ \ \
//!    /\/\/\/\/\/\/\ \
//! ```
//!  Lets continue thw example, by adding a single object. Our MMR now looks as follows
//! ```plaintext
//!           /\
//!          /  \
//!         /    \
//!        /      \
//!       /\      /\
//!      /  \    /  \
//!     /\  /\  /\  /\
//!    /\/\/\/\/\/\/\/\
//! ```
//! Now we only have a single binary tree, we dont have to bag the mountains to calculate the merkle root. This
//! process continues as you add more objects to the MMR.
//! ```plaintext
//!                 /\
//!                /  \
//!               /    \
//!              /      \
//!             /        \
//!            /          \
//!           /            \
//!          /\             \
//!         /\ \            /\
//!        /  \ \          /  \
//!       /\   \ \        /\   \
//!      /  \   \ \      /  \   \
//!     /\  /\  /\ \    /\  /\  /\
//!    /\/\/\/\/\/\/\  /\/\/\/\/\/\
//! ```
//! Due to the unique way the MMR is constructed we can easily represent the MMR as a list of the nodes, as when
//! adding nodes you only append. Lets take the following MMR and number the nodes in the order we create them.
//! ```plaintext
//!        7
//!       /  \
//!      /    \
//!     3      6
//!    / \    / \
//!   1   2  4   5
//! ```
//! Looking above at the example of when you create the nodes, you will see the nodes will have been created in the
//! order as they are named. This means we can easily represent them as a list:
//! Height:  0 | 0 | 1 | 0 | 0 | 1 | 2
//! Node:    1 | 2 | 3 | 4 | 5 | 6 | 7
//!
//! Because of the list nature of the MMR we can easily navigate around the MMR using the following formulas:
//! Jump to sibling : 2^(H+1) -1
//! find peak : 2^(H+1) -2 where < total elements
//! left down : 2^H
//! right down: -1
//! Note that the formulas are for direct indexes in the array, meaning the nodes count from 0 and not 1 as in
//! the examples above. H - Height
//! I - Index
//!
//! Pruning the MMR means flagging a node as pruned and only removing it if its sibling has been removed.
//! We do this as we require the sibling to prove the hash of the node. Taking the above example, let's prune leaf 1.
//! ```plaintext
//!                 /\
//!                /  \
//!               /    \
//!              /      \
//!             /        \
//!            /          \
//!           /            \
//!          /\             \
//!         /\ \            /\
//!        /  \ \          /  \
//!       /\   \ \        /\   \
//!      /  \   \ \      /  \   \
//!     /\  /\  /\ \    /\  /\  /\
//!    /\/\/\/\/\/\/\  /\/\/\/\/\/\
//! ```
//! Node 1 has now only been marked as pruned but we cannot remove it as of yet because we still require it to
//! prove node 2. When we prune node 2, the MMR looks as follows
//! ```plaintext
//!                 /\
//!                /  \
//!               /    \
//!              /      \
//!             /        \
//!            /          \
//!           /            \
//!          /\             \
//!         /\ \            /\
//!        /  \ \          /  \
//!       /\   \ \        /\   \
//!      /  \   \ \      /  \   \
//!     /\  /\  /\ \    /\  /\  /\
//!      /\/\/\/\/\/\  /\/\/\/\/\/\
//! ```
//! Although we have not removed node 1 and node 2 from the MMR, we cannot yet remove node 3 as we require node 3
//! for the proof of node 6. Let's prune 4 and 5.
//! ```plaintext
//!                 /\
//!                /  \
//!               /    \
//!              /      \
//!             /        \
//!            /          \
//!           /            \
//!          /\             \
//!         /\ \            /\
//!        /  \ \          /  \
//!       /\   \ \        /\   \
//!      /  \   \ \      /  \   \
//!         /\  /\ \    /\  /\  /\
//!        /\/\/\/\/\  /\/\/\/\/\/\
//! ```
//! Now we removed 3 from the MMR

pub mod error;
pub mod merkle_change_tracker;
pub mod merkle_storage;
pub mod merklemountainrange;
pub mod merklenode;
pub mod merkleproof;
pub mod mmr {
    pub use crate::{merklemountainrange::*, merkleproof::*};
}
