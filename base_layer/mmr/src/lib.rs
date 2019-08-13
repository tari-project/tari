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

//! # Merkle Mountain Ranges
//!
//! ## Introduction
//!
//! The Merkle mountain range was invented by Peter Todd more about them can be read at
//! [Open Timestamps](https://github.com/opentimestamps/opentimestamps-server/blob/master/doc/merkle-mountain-range.md)
//! and the [Grin project](https://github.com/mimblewimble/grin/blob/master/doc/mmr.md).
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
//! If we want to calculate the merkle root we simply concatenate and then hash the three peaks.
//!
//! Lets continue the example, by adding a single object. Our MMR now looks as follows
//! ```plaintext
//!       /\
//!      /  \
//!     /\  /\   /\
//!    /\/\/\/\ /\/\ /\ /
//! ```
//! We now have 4 mountains. Calculating the root means hashing the concatenation of the (now) four peaks.
//!
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
//! Now we only have a single binary tree, and the root is now the hash of the single peak's hash. This
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
//! Due to the unique way the MMR is constructed we can easily represent the MMR as a linear list of the nodes. Lets
//! take the following MMR and number the nodes in the order we create them.
//! ```plaintext
//!         6
//!       /  \
//!      /    \
//!     2      5
//!    / \    / \
//!   0   1  3   4
//! ```
//! Looking above at the example of when you create the nodes, you will see the MMR nodes will have been created in the
//! order as they are named. This means we can easily represent them as a list:
//! Height:  0 | 0 | 1 | 0 | 0 | 1 | 2
//! Node:    0 | 1 | 2 | 3 | 4 | 5 | 6
//!
//! Because of the list nature of the MMR we can easily navigate around the MMR using the following formulas:
//!
//! Jump to right sibling : $$ n + 2^{H+1} - 1 $$
//! Jump to left sibling : $$ n - 2^{H+1} - 1 $$
//! peak of binary tree : $$ 2^{ H+1 } - 2 $$
//! left down : $$ n - 2^H $$
//! right down: $$ n-1 $$
//!
//! ## Node numbering
//!
//! There can be some confusion about how nodes are numbered in an MMR. The following conventions are used in this
//! crate:
//!
//! * _All_ indices are numbered starting from zero.
//! * MMR nodes refer to all the nodes in the Merkle Mountain Range and are ordered in the canonical mmr ordering
//! described above.
//! * Leaf nodes are numbered counting from zero and increment by one each time a leaf is added.
//!
//! To illustrate, consider this MMR:
//!
//! //! ```plaintext
//!            14
//!          /     \
//!         /       \
//!        6        13          21          <-- MMR indices
//!      /  \      /  \        /  \
//!     /    \    /    \      /    \
//!     2    5    9    12    17    21
//!    / \  / \  / \  / \   / \   / \
//!    0 1  3 4  7 8 10 11 15 16 18 19 22
//!    ----------------------------------
//!    0 1  2 3  4 5  6  7  8  9 10 11 12  <-- Leaf node indices
//!    ----------------------------------
//! ```

pub type Hash = Vec<u8>;
pub type HashSlice = [u8];

mod backend;
mod merkle_mountain_range;
mod merkle_proof;

// Less commonly used exports
pub mod common;
pub mod error;

// Commonly used exports
/// A vector-based backend for [MerkleMountainRange]
pub use backend::VectorBackend;
/// An immutable, append-only Merkle Mountain range (MMR) data structure
pub use merkle_mountain_range::MerkleMountainRange;
/// A data structure for proving a hash inclusion in an MMR
pub use merkle_proof::{MerkleProof, MerkleProofError};
