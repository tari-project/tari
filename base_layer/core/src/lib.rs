// Copyright 2018 The Tari Project
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
#[macro_use]
extern crate bitflags;

pub mod blocks;
#[cfg(feature = "base_node")]
pub mod chain_storage;
pub mod consensus;
#[macro_use]
pub mod covenants;
#[cfg(feature = "base_node")]
pub mod iterators;
pub mod proof_of_work;
#[cfg(feature = "base_node")]
pub mod validation;

#[cfg(any(test, feature = "base_node"))]
#[macro_use]
pub mod test_helpers;

#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
pub mod base_node;
#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
pub mod proto;

#[cfg(any(feature = "base_node", feature = "mempool_proto"))]
pub mod mempool;

#[cfg(feature = "transactions")]
pub mod transactions;

mod common;
pub use common::{borsh, one_sided, ConfidentialOutputHasher};

#[allow(clippy::ptr_offset_with_cast)]
#[allow(clippy::assign_op_pattern)]
#[allow(clippy::manual_range_contains)]
#[allow(clippy::range_plus_one)]
pub mod large_ints {
    uint::construct_uint! {
        /// 256-bit unsigned integer.
        pub struct U256(4);
    }

    uint::construct_uint! {
        /// 512-bit unsigned integer.
        pub struct U512(8);
    }
}

pub use large_ints::{U256, U512};
#[cfg(feature = "base_node")]
mod domain_hashing {
    use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparatedHasher};
    use tari_mmr::{pruned_hashset::PrunedHashSet, BalancedBinaryMerkleTree, Hash, MerkleMountainRange, MutableMmr};

    hash_domain!(KernelMmrHashDomain, "com.tari.base_layer.core.kernel_mmr", 1);

    pub type KernelMmrHasherBlake256 = DomainSeparatedHasher<Blake256, KernelMmrHashDomain>;
    pub type KernelMmr = MerkleMountainRange<KernelMmrHasherBlake256, Vec<Hash>>;
    pub type PrunedKernelMmr = MerkleMountainRange<KernelMmrHasherBlake256, PrunedHashSet>;

    hash_domain!(OutputMmrHashDomain, "com.tari.base_layer.core.output_mmr", 1);
    pub type OutputMmrHasherBlake256 = DomainSeparatedHasher<Blake256, OutputMmrHashDomain>;
    pub type MutableOutputMmr = MutableMmr<OutputMmrHasherBlake256, Vec<Hash>>;
    pub type PrunedOutputMmr = MerkleMountainRange<OutputMmrHasherBlake256, PrunedHashSet>;
    pub type MutablePrunedOutputMmr = MutableMmr<OutputMmrHasherBlake256, PrunedHashSet>;

    hash_domain!(InputMmrHashDomain, "com.tari.base_layer.core.input_mmr", 1);
    pub type InputMmrHasherBlake256 = DomainSeparatedHasher<Blake256, InputMmrHashDomain>;
    pub type PrunedInputMmr = MerkleMountainRange<InputMmrHasherBlake256, PrunedHashSet>;

    hash_domain!(
        ValidatorNodeBmtHashDomain,
        "com.tari.base_layer.core.validator_node_mmr",
        1
    );
    pub type ValidatorNodeBmtHasherBlake256 = DomainSeparatedHasher<Blake256, ValidatorNodeBmtHashDomain>;
    pub type ValidatorNodeBMT = BalancedBinaryMerkleTree<ValidatorNodeBmtHasherBlake256>;
}

#[cfg(feature = "base_node")]
pub use domain_hashing::*;
