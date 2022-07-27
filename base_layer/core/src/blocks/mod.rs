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

#[cfg(feature = "base_node")]
mod accumulated_data;

#[cfg(feature = "base_node")]
pub use accumulated_data::{
    BlockAccumulatedData,
    BlockHeaderAccumulatedData,
    ChainBlock,
    ChainHeader,
    CompleteDeletedBitmap,
    DeletedBitmap,
    UpdateBlockAccumulatedData,
};
use tari_crypto::{hash::blake2::Blake256, hashing::DomainSeparatedHasher};

mod error;
pub use error::BlockError;

mod block;
pub use block::{Block, BlockBuilder, BlockValidationError, NewBlock};

#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
mod block_header;
#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
pub use block_header::{BlockHeader, BlockHeaderValidationError};

#[cfg(feature = "base_node")]
pub mod genesis_block;

#[cfg(feature = "base_node")]
mod historical_block;
#[cfg(feature = "base_node")]
pub use historical_block::HistoricalBlock;

#[cfg(feature = "base_node")]
mod new_block_template;
#[cfg(feature = "base_node")]
pub use new_block_template::NewBlockTemplate;

#[cfg(feature = "base_node")]
mod new_blockheader_template;
#[cfg(feature = "base_node")]
pub use new_blockheader_template::NewBlockHeaderTemplate;
use tari_common::hashing_domain::HashingDomain;
use tari_crypto::hash_domain;
use tari_mmr::{pruned_hashset::PrunedHashSet, MerkleMountainRange};

/// The base layer core blocks domain separated hashing domain
/// Usage:
///   let hash = core_blocks_hash_domain().digest::<Blake256>(b"my secret");
///   etc.
pub fn core_blocks_hash_domain() -> HashingDomain {
    HashingDomain::new("base_layer.core.blocks")
}

hash_domain!(TariKernelMmrHasher, "com.tari.blocks.mmr.kernels");
pub type KernelMmr = MerkleMountainRange<DomainSeparatedHasher<Blake256, TariKernelMmrHasher>, PrunedHashSet>;

hash_domain!(TariOutputMmrHasher, "com.tari.blocks.mmr.outputs");
pub type OutputMmr = MutableMmr<DomainSeparatedHasher<Blake256, TariOutputMmrHasher>, PrunedHashSet>;

hash_domain!(TariWitnessMmrHasher, "com.tari.blocks.mmr.witnesses");
pub type WitnessMmr = MerkleMountainRange<DomainSeparatedHasher<Blake256, TariWitnessMmrHasher>, PrunedHashSet>;

hash_domain!(TariInputMmrHasher, "com.tari.blocks.mmr.input");
pub type InputMmr = MerkleMountainRange<DomainSeparatedHasher<Blake256, TariInputMmrHasher>, PrunedHashSet>;

hash_domain!(TariMergeMiningHashDomain, "com.tari.blocks.merge_mining");
pub type TariMergeMiningHasher = DomainSeparatedHasher<Blake256, TariMergeMiningHashDomain>;

hash_domain!(TariBlockHashDomain, "com.tari.blocks.header");
pub type TariBlockHeaderHasher = DomainSeparatedHasher<Blake256, TariBlockHashDomain>;
