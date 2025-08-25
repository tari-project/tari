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

pub mod blocks;
pub mod chain_storage;
pub mod consensus;
pub mod iterators;
pub mod proof_of_work;
pub mod validation;

#[macro_use]
pub mod test_helpers;

pub mod base_node;
pub mod proto;
pub mod mempool;

mod common;

pub use common::AuxChainHashes;

mod domain_hashing {
    use std::convert::TryFrom;

    use blake2::Blake2b;
    use digest::consts::U32;
    use tari_common_types::types::{FixedHash, FixedHashSizeError};
    use tari_crypto::{hash_domain, hashing::DomainSeparatedHasher};
    use tari_hashing::ValidatorNodeMerkleHashDomain;
    use tari_mmr::{
        error::MerkleMountainRangeError,
        pruned_hashset::PrunedHashSet,
        sparse_merkle_tree::SparseMerkleTree,
        Hash,
        MerkleMountainRange,
    };

    hash_domain!(KernelMmrHashDomain, "com.tari.base_layer.core.kernel_mmr", 1);

    pub type KernelMmrHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, KernelMmrHashDomain>;
    pub type KernelMmr = MerkleMountainRange<KernelMmrHasherBlake256, Vec<Hash>>;
    pub type PrunedKernelMmr = MerkleMountainRange<KernelMmrHasherBlake256, PrunedHashSet>;

    hash_domain!(OutputSmtHashDomain, "com.tari.base_layer.core.output_smt", 1);
    pub type OutputSmtHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, OutputSmtHashDomain>;

    hash_domain!(InputMmrHashDomain, "com.tari.base_layer.core.input_mmr", 1);
    pub type InputMmrHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, InputMmrHashDomain>;
    pub type PrunedInputMmr = MerkleMountainRange<InputMmrHasherBlake256, PrunedHashSet>;
    pub type PrunedOutputMmr = MerkleMountainRange<InputMmrHasherBlake256, PrunedHashSet>;

    pub type OutputSmt = SparseMerkleTree<OutputSmtHasherBlake256>;

    pub type ValidatorNodeMerkleHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, ValidatorNodeMerkleHashDomain>;

    #[inline]
    pub fn kernel_mr_hash_from_mmr(kernel_mmr: &KernelMmr) -> Result<FixedHash, MrHashError> {
        Ok(FixedHash::try_from(kernel_mmr.get_merkle_root()?)?)
    }

    #[inline]
    pub fn kernel_mr_hash_from_pruned_mmr(kernel_mmr: &PrunedKernelMmr) -> Result<FixedHash, MrHashError> {
        Ok(FixedHash::try_from(kernel_mmr.get_merkle_root()?)?)
    }

    #[inline]
    pub fn input_mr_hash_from_pruned_mmr(input_mmr: &PrunedInputMmr) -> Result<FixedHash, MrHashError> {
        Ok(FixedHash::try_from(input_mmr.get_merkle_root()?)?)
    }

    #[inline]
    pub fn block_output_mr_hash_from_pruned_mmr(output_mmr: &PrunedOutputMmr) -> Result<FixedHash, MrHashError> {
        Ok(FixedHash::try_from(output_mmr.get_merkle_root()?)?)
    }

    #[derive(Debug, thiserror::Error)]
    pub enum MrHashError {
        #[error("Output SMT conversion error: {0}")]
        FixedHashSizeError(#[from] FixedHashSizeError),
        #[error("Input MR conversion error: {0}")]
        MerkleMountainRangeError(#[from] MerkleMountainRangeError),
    }
}

pub use domain_hashing::*;
