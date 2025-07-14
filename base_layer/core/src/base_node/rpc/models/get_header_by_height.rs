// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use tari_common_types::types::{BlockHash, FixedHash, PrivateKey};
use tari_utilities::epoch_time::EpochTime;
use utoipa::{
    openapi::{schema::SchemaType, Object, Schema, Type},
    ToSchema,
};

use crate::{blocks, proof_of_work::ProofOfWork};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct BlockHeader {
    /// Hash of the block header
    pub hash: BlockHash,
    /// Version of the block
    pub version: u16,
    /// Height of this block since the genesis block (height 0)
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    pub prev_hash: BlockHash,
    /// Timestamp at which the block was built.
    #[schema(schema_with = epoch_time_schema)]
    pub timestamp: EpochTime,
    /// This is the Merkle root of the inputs in this block
    pub input_mr: FixedHash,
    /// This is the UTXO merkle root of the outputs on the blockchain
    pub output_mr: FixedHash,
    /// This is the block_output_mr
    pub block_output_mr: FixedHash,
    /// The size (number  of leaves) of the output and range proof MMRs at the time of this header
    pub output_smt_size: u64,
    /// This is the MMR root of the kernels
    pub kernel_mr: FixedHash,
    /// The number of MMR leaves in the kernel MMR
    pub kernel_mmr_size: u64,
    /// Sum of kernel offsets for all kernels in this block.
    #[schema(schema_with = private_key_schema)]
    pub total_kernel_offset: PrivateKey,
    /// Sum of script offsets for all kernels in this block.
    #[schema(schema_with = private_key_schema)]
    pub total_script_offset: PrivateKey,
    /// Merkle root of all active validator node.
    pub validator_node_mr: FixedHash,
    /// The number of validator node hashes
    pub validator_node_size: u64,
    /// Proof of work summary
    #[schema(schema_with = proof_of_work_schema)]
    pub pow: ProofOfWork,
    /// Nonce increment used to mine this block.
    pub nonce: u64,
}

pub fn epoch_time_schema() -> Schema {
    Schema::Object(Object::with_type(SchemaType::Type(Type::Integer)))
}

pub fn private_key_schema() -> Schema {
    Schema::Object(Object::with_type(SchemaType::Type(Type::Array)))
}

pub fn proof_of_work_schema() -> Schema {
    Schema::Object(
        Object::builder()
            .property("pow_algo", Schema::Object(Object::with_type(Type::String)))
            .property("pow_data", Schema::Object(Object::with_type(Type::String)))
            .build(),
    )
}

impl From<BlockHeader> for blocks::BlockHeader {
    fn from(header: BlockHeader) -> Self {
        Self {
            version: header.version,
            height: header.height,
            prev_hash: header.prev_hash,
            timestamp: header.timestamp,
            input_mr: header.input_mr,
            output_mr: header.output_mr,
            block_output_mr: header.block_output_mr,
            output_smt_size: header.output_smt_size,
            kernel_mr: header.kernel_mr,
            kernel_mmr_size: header.kernel_mmr_size,
            total_kernel_offset: header.total_kernel_offset,
            total_script_offset: header.total_script_offset,
            validator_node_mr: header.validator_node_mr,
            validator_node_size: header.validator_node_size,
            pow: header.pow,
            nonce: header.nonce,
        }
    }
}

impl From<blocks::BlockHeader> for BlockHeader {
    fn from(header: blocks::BlockHeader) -> Self {
        Self {
            hash: header.hash(),
            version: header.version,
            height: header.height,
            prev_hash: header.prev_hash,
            timestamp: header.timestamp,
            input_mr: header.input_mr,
            output_mr: header.output_mr,
            block_output_mr: header.block_output_mr,
            output_smt_size: header.output_smt_size,
            kernel_mr: header.kernel_mr,
            kernel_mmr_size: header.kernel_mmr_size,
            total_kernel_offset: header.total_kernel_offset,
            total_script_offset: header.total_script_offset,
            validator_node_mr: header.validator_node_mr,
            validator_node_size: header.validator_node_size,
            pow: header.pow,
            nonce: header.nonce,
        }
    }
}
