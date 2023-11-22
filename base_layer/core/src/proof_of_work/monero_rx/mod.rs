//  Copyright 2021, The Tari Project
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
mod error;
pub use error::MergeMineError;

mod helpers;
pub use helpers::{
    construct_monero_data,
    create_blockhashing_blob_from_block,
    create_ordered_transaction_hashes_from_block,
    deserialize_monero_block_from_hex,
    extract_tari_hash_from_block,
    insert_merge_mining_tag_into_block,
    randomx_difficulty,
    serialize_monero_block_to_hex,
    verify_header,
};

mod fixed_array;
pub use fixed_array::FixedByteArray;

mod pow_data;
pub use pow_data::MoneroPowData;

mod merkle_tree;
mod merkle_tree_parameters;
pub use merkle_tree::{create_merkle_proof, tree_hash};
pub use merkle_tree_parameters::MerkleTreeParameters;
// Re-exports
pub use monero::{
    consensus::{deserialize, serialize},
    Block as MoneroBlock,
    BlockHeader as MoneroBlockHeader,
};
