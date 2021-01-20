/// Metadata required for validating the Proof of Work calculation
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProofOfWork {
    /// 0 = Monero
    /// 1 = Blake
    #[prost(uint64, tag = "1")]
    pub pow_algo: u64,
    #[prost(bytes, tag = "4")]
    pub pow_data: std::vec::Vec<u8>,
}
/// The BlockHeader contains all the metadata for the block, including proof of work, a link to the previous block
/// and the transaction kernels.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockHeader {
    /// Version of the block
    #[prost(uint32, tag = "1")]
    pub version: u32,
    /// Height of this block since the genesis block (height 0)
    #[prost(uint64, tag = "2")]
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    #[prost(bytes, tag = "4")]
    pub prev_hash: std::vec::Vec<u8>,
    /// Timestamp at which the block was built.
    #[prost(message, optional, tag = "5")]
    pub timestamp: ::std::option::Option<::prost_types::Timestamp>,
    /// This is the UTXO merkle root of the outputs
    /// This is calculated as Hash (txo MMR root  || roaring bitmap hash of UTXO indices)
    #[prost(bytes, tag = "6")]
    pub output_mr: std::vec::Vec<u8>,
    /// This is the MMR root of the range proofs
    #[prost(bytes, tag = "7")]
    pub range_proof_mr: std::vec::Vec<u8>,
    /// This is the MMR root of the kernels
    #[prost(bytes, tag = "8")]
    pub kernel_mr: std::vec::Vec<u8>,
    /// Total accumulated sum of kernel offsets since genesis block. We can derive the kernel offset sum for *this*
    /// block from the total kernel offset of the previous block header.
    #[prost(bytes, tag = "9")]
    pub total_kernel_offset: std::vec::Vec<u8>,
    /// Nonce increment used to mine this block.
    #[prost(uint64, tag = "10")]
    pub nonce: u64,
    /// Proof of work metadata
    #[prost(message, optional, tag = "11")]
    pub pow: ::std::option::Option<ProofOfWork>,
    /// The size of the kernel MMR
    #[prost(uint64, tag = "12")]
    pub kernel_mmr_size: u64,
    /// The size of the output MMR
    #[prost(uint64, tag = "13")]
    pub output_mmr_size: u64,
}
/// A Tari block. Blocks are linked together into a blockchain.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Block {
    #[prost(message, optional, tag = "1")]
    pub header: ::std::option::Option<BlockHeader>,
    #[prost(message, optional, tag = "2")]
    pub body: ::std::option::Option<super::types::AggregateBody>,
}
/// A new block message. This is the message that is propagated around the network. It contains the
/// minimal information required to identify and optionally request the full block.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewBlock {
    #[prost(bytes, tag = "1")]
    pub block_hash: std::vec::Vec<u8>,
}
/// The representation of a historical block in the blockchain. It is essentially identical to a protocol-defined
/// block but contains some extra metadata that clients such as Block Explorers will find interesting.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HistoricalBlock {
    /// The number of blocks that have been mined since this block, including this one. The current tip will have one
    /// confirmation.
    #[prost(uint64, tag = "1")]
    pub confirmations: u64,
    /// The underlying block
    #[prost(message, optional, tag = "3")]
    pub block: ::std::option::Option<Block>,
    #[prost(message, optional, tag = "4")]
    pub accumulated_data: ::std::option::Option<BlockHeaderAccumulatedData>,
    #[prost(bytes, repeated, tag = "5")]
    pub pruned_output_hashes: ::std::vec::Vec<std::vec::Vec<u8>>,
    #[prost(bytes, repeated, tag = "6")]
    pub pruned_proof_hashes: ::std::vec::Vec<std::vec::Vec<u8>>,
    #[prost(uint64, tag = "7")]
    pub pruned_input_count: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockHeaderAccumulatedData {
    #[prost(uint64, tag = "1")]
    pub achieved_difficulty: u64,
    #[prost(uint64, tag = "2")]
    pub accumulated_monero_difficulty: u64,
    #[prost(uint64, tag = "3")]
    pub accumulated_blake_difficulty: u64,
    #[prost(uint64, tag = "4")]
    pub target_difficulty: u64,
    #[prost(bytes, tag = "5")]
    pub total_kernel_offset: std::vec::Vec<u8>,
    #[prost(bytes, tag = "6")]
    pub hash: std::vec::Vec<u8>,
    #[prost(bytes, tag = "7")]
    pub total_accumulated_difficulty: std::vec::Vec<u8>,
}
/// The NewBlockHeaderTemplate is used for the construction of a new mineable block. It contains all the metadata for
/// the block that the Base Node is able to complete on behalf of a Miner.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewBlockHeaderTemplate {
    /// Version of the block
    #[prost(uint32, tag = "1")]
    pub version: u32,
    /// Height of this block since the genesis block (height 0)
    #[prost(uint64, tag = "2")]
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    #[prost(bytes, tag = "3")]
    pub prev_hash: std::vec::Vec<u8>,
    /// Total accumulated sum of kernel offsets since genesis block. We can derive the kernel offset sum for *this*
    /// block from the total kernel offset of the previous block header.
    #[prost(bytes, tag = "4")]
    pub total_kernel_offset: std::vec::Vec<u8>,
    /// Proof of work metadata
    #[prost(message, optional, tag = "5")]
    pub pow: ::std::option::Option<ProofOfWork>,
}
/// The new block template is used constructing a new partial block, allowing a miner to added the coinbase utxo and as
/// a final step the Base node to add the MMR roots to the header.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewBlockTemplate {
    #[prost(message, optional, tag = "1")]
    pub header: ::std::option::Option<NewBlockHeaderTemplate>,
    #[prost(message, optional, tag = "2")]
    pub body: ::std::option::Option<super::types::AggregateBody>,
    #[prost(uint64, tag = "3")]
    pub target_difficulty: u64,
    #[prost(uint64, tag = "4")]
    pub reward: u64,
    #[prost(uint64, tag = "5")]
    pub total_fees: u64,
}
