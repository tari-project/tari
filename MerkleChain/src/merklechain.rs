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

/// This is the MerkleChain which is a variant of a merkle mountain range.
/// The merkle mountain range was invented by Peter Todd more about them can be ready at:
/// https://github.com/opentimestamps/opentimestamps-server/blob/master/doc/merkle-mountain-range.md
/// https://github.com/mimblewimble/grin/blob/master/doc/mmr.md
///
/// Merkle chains differ from Merkle Mountain Ranges (MMR) in that they are made final at each block.
/// With each new block you start a new MMR and you bag that one with the previous merkle root. This basically
/// means that a merkle chain is a collections of MMR's. Each time you add to the merkle chain you add a MMR and
/// not a single entry.
///
/// Lets take an example of how to construct one. Say you have the following MMR already made:
///
///          /\
///         /\ \
///        /  \ \
///       /\   \ \
///      /  \   \ \
///     /\  /\  /\ \
///    /\/\/\/\/\/\/\
///
/// This MMR now represents a single block. You get a new block with can be represented as follows:
///
///         /\
///        /  \
///       /\   \
///      /  \   \
///     /\  /\  /\
///    /\/\/\/\/\/\
///
/// You can now construct the merkle chain:
///
///                / \
///               /   \
///              /     \
///             /       \
///            /         \
///           /           \
///          /\            \
///         /\ \           /\
///        /  \ \         /  \
///       /\   \ \       /\   \
///      /  \   \ \     /  \   \
///     /\  /\  /\ \   /\  /\  /\
///    /\/\/\/\/\/\/\ /\/\/\/\/\/\
///
/// When you get a new block of:
///
///     /\
///    /\/\
///
/// You can then construct a new merkle chain of:
///
///                   /\
///                  /  \
///                 /    \
///                / \    \
///               /   \    \
///              /     \    \
///             /       \    \
///            /         \    \
///           /           \    \
///          /\            \    \
///         /\ \           /\    \
///        /  \ \         /  \    \
///       /\   \ \       /\   \    \
///      /  \   \ \     /  \   \    \
///     /\  /\  /\ \   /\  /\  /\   /\
///    /\/\/\/\/\/\/\ /\/\/\/\/\/\ /\/\
///
/// The merkle chain also supports pruning. Pruning will remove a child, if both children have been removed, then
/// the parent can also be removed.
///
/// Going forward with the example, say entry 1 and 11 are pruned, then the merkle chain will look as follows:
///
///                   /\
///                  /  \
///                 /    \
///                / \    \
///               /   \    \
///              /     \    \
///             /       \    \
///            /         \    \
///           /           \    \
///          /\            \    \
///         /\ \           /\    \
///        /  \ \         /  \    \
///       /\   \ \       /\   \    \
///      /  \   \ \     /  \   \    \
///     /\  /\  /\ \   /\  /\  /\   /\
///     \/\/\/\/\ \/\ /\/\/\/\/\/\ /\/\
///
/// If we now prune 2 and 12, the merkle chain looks as follows:
///
///                   /\
///                  /  \
///                 /    \
///                / \    \
///               /   \    \
///              /     \    \
///             /       \    \
///            /         \    \
///           /           \    \
///          /\            \    \
///         /\ \           /\    \
///        /  \ \         /  \    \
///       /\   \ \       /\   \    \
///      /  \   \ \     /  \   \    \
///      \  /\  /  \   /\  /\  /\   /\
///      /\/\/\/\  /\ /\/\/\/\/\/\ /\/\
///
/// If we now prune 3 and 4 the merkle chain looks as follows:
///
///                   /\
///                  /  \
///                 /    \
///                / \    \
///               /   \    \
///              /     \    \
///             /       \    \
///            /         \    \
///           /           \    \
///          /\            \    \
///         /\ \           /\    \
///        /  \ \         /  \    \
///        \   \ \       /\   \    \
///         \   \ \     /  \   \    \
///         /\  /  \   /\  /\  /\   /\
///        /\/\/\  /\ /\/\/\/\/\/\ /\/\
///
/// Pruning 5, 6, 7 and 8 will give a merkle chain as follows:
///
///                   /\
///                  /  \
///                 /    \
///                / \    \
///               /   \    \
///              /     \    \
///             /       \    \
///            /         \    \
///           /           \    \
///          /\            \    \
///          \ \           /\    \
///           \ \         /  \    \
///            \ \       /\   \    \
///             \ \     /  \   \    \
///             /  \   /\  /\  /\   /\
///            /\  /\ /\/\/\/\/\/\ /\/\+
use crate::{merklemountainrange::MerkleMountainRange, merklenode::MerkleNode};

type Hash = [u8; 32];

/// The merkle chain object
pub struct MerkleChain<T> {
    objects: HashMap<Hash, T>,
    merkleroot: MerkleNode,
}
