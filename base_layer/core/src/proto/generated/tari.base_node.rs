#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChainMetadata {
    /// The current chain height, or the block number of the longest valid chain, or `None` if there is no chain
    #[prost(message, optional, tag = "1")]
    pub height_of_longest_chain: ::std::option::Option<u64>,
    /// The block hash of the current tip of the longest valid chain, or `None` for an empty chain
    #[prost(message, optional, tag = "2")]
    pub best_block: ::std::option::Option<::std::vec::Vec<u8>>,
    /// The number of blocks back from the tip that this database tracks. A value of 0 indicates that all blocks are
    /// tracked (i.e. the database is in full archival mode).
    #[prost(uint64, tag = "4")]
    pub pruning_horizon: u64,
    /// The current geometric mean of the pow of the chain tip, or `None` if there is no chain
    #[prost(message, optional, tag = "5")]
    pub accumulated_difficulty: ::std::option::Option<u64>,
    /// The effective height of the pruning horizon. This indicates from what height
    /// a full block can be provided (exclusive).
    /// If `effective_pruned_height` is equal to the `height_of_longest_chain` no blocks can be provided.
    /// Archival nodes wil always have an `effective_pruned_height` of zero.
    #[prost(uint64, tag = "6")]
    pub effective_pruned_height: u64,
}
/// Response type for a received BaseNodeService requests
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BaseNodeServiceResponse {
    #[prost(uint64, tag = "1")]
    pub request_key: u64,
    #[prost(bool, tag = "13")]
    pub is_synced: bool,
    #[prost(
        oneof = "base_node_service_response::Response",
        tags = "2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12"
    )]
    pub response: ::std::option::Option<base_node_service_response::Response>,
}
pub mod base_node_service_response {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Response {
        /// Indicates a ChainMetadata response.
        #[prost(message, tag = "2")]
        ChainMetadata(super::ChainMetadata),
        /// Indicates a TransactionKernels response.
        #[prost(message, tag = "3")]
        TransactionKernels(super::TransactionKernels),
        /// Indicates a BlockHeaders response.
        #[prost(message, tag = "4")]
        BlockHeaders(super::BlockHeaders),
        /// Indicates a TransactionOutputs response.
        #[prost(message, tag = "5")]
        TransactionOutputs(super::TransactionOutputs),
        /// Indicates a HistoricalBlocks response.
        #[prost(message, tag = "6")]
        HistoricalBlocks(super::HistoricalBlocks),
        /// Indicates a NewBlockTemplate response.
        #[prost(message, tag = "7")]
        NewBlockTemplate(super::super::core::NewBlockTemplate),
        /// Indicates a NewBlock response.
        #[prost(message, tag = "8")]
        NewBlock(super::super::core::Block),
        /// Indicates a TargetDifficulty response.
        #[prost(uint64, tag = "9")]
        TargetDifficulty(u64),
        /// Block headers in range response
        #[prost(message, tag = "10")]
        FetchHeadersAfterResponse(super::BlockHeaders),
        /// Indicates a MmrNodeCount response
        #[prost(uint32, tag = "11")]
        MmrNodeCount(u32),
        /// Indicates a MmrNodes response
        #[prost(message, tag = "12")]
        MmrNodes(super::MmrNodes),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockHeaders {
    #[prost(message, repeated, tag = "1")]
    pub headers: ::std::vec::Vec<super::core::BlockHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionKernels {
    #[prost(message, repeated, tag = "1")]
    pub kernels: ::std::vec::Vec<super::types::TransactionKernel>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionOutputs {
    #[prost(message, repeated, tag = "1")]
    pub outputs: ::std::vec::Vec<super::types::TransactionOutput>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HistoricalBlocks {
    #[prost(message, repeated, tag = "1")]
    pub blocks: ::std::vec::Vec<super::core::HistoricalBlock>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MmrNodes {
    #[prost(bytes, repeated, tag = "1")]
    pub added: ::std::vec::Vec<std::vec::Vec<u8>>,
    #[prost(bytes, tag = "2")]
    pub deleted: std::vec::Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum MmrTree {
    None = 0,
    Utxo = 1,
    Kernel = 2,
    RangeProof = 3,
}
/// Request type for a received BaseNodeService request.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BaseNodeServiceRequest {
    #[prost(uint64, tag = "1")]
    pub request_key: u64,
    #[prost(
        oneof = "base_node_service_request::Request",
        tags = "2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18"
    )]
    pub request: ::std::option::Option<base_node_service_request::Request>,
}
pub mod base_node_service_request {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Request {
        /// Indicates a GetChainMetadata request. The value of the bool should be ignored.
        #[prost(bool, tag = "2")]
        GetChainMetadata(bool),
        /// Indicates a FetchKernels request.
        #[prost(message, tag = "3")]
        FetchKernels(super::HashOutputs),
        /// Indicates a FetchHeaders request.
        #[prost(message, tag = "4")]
        FetchHeaders(super::BlockHeights),
        /// Indicates a FetchHeadersWithHashes request.
        #[prost(message, tag = "5")]
        FetchHeadersWithHashes(super::HashOutputs),
        /// Indicates a FetchUtxos request.
        #[prost(message, tag = "6")]
        FetchUtxos(super::HashOutputs),
        /// Indicates a FetchBlocks request.
        #[prost(message, tag = "7")]
        FetchBlocks(super::BlockHeights),
        /// Indicates a FetchBlocksWithHashes request.
        #[prost(message, tag = "8")]
        FetchBlocksWithHashes(super::HashOutputs),
        /// Indicates a GetNewBlockTemplate request.
        #[prost(uint64, tag = "9")]
        GetNewBlockTemplate(u64),
        /// Indicates a GetNewBlock request.
        #[prost(message, tag = "10")]
        GetNewBlock(super::super::core::NewBlockTemplate),
        /// Indicates a GetTargetDifficulty request.
        #[prost(uint64, tag = "11")]
        GetTargetDifficulty(u64),
        /// Get headers in best chain following any headers in this list
        #[prost(message, tag = "12")]
        FetchHeadersAfter(super::FetchHeadersAfter),
        /// Indicates a FetchMmrNodeCount request.
        #[prost(message, tag = "13")]
        FetchMmrNodeCount(super::FetchMmrNodeCount),
        /// Indicates a FetchMmrNodes request.
        #[prost(message, tag = "14")]
        FetchMmrNodes(super::FetchMmrNodes),
        /// Indicates a FetchTxos request.
        #[prost(message, tag = "15")]
        FetchTxos(super::HashOutputs),
        /// Indicates a Fetch block with kernels request
        #[prost(message, tag = "16")]
        FetchBlocksWithKernels(super::Signatures),
        /// Indicates a Fetch block with kernels request
        #[prost(message, tag = "17")]
        FetchBlocksWithStxos(super::Commitments),
        /// Indicates a Fetch block with kernels request
        #[prost(message, tag = "18")]
        FetchBlocksWithUtxos(super::Commitments),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockHeights {
    #[prost(uint64, repeated, tag = "1")]
    pub heights: ::std::vec::Vec<u64>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HashOutputs {
    #[prost(bytes, repeated, tag = "1")]
    pub outputs: ::std::vec::Vec<std::vec::Vec<u8>>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Signatures {
    #[prost(message, repeated, tag = "1")]
    pub sigs: ::std::vec::Vec<super::types::Signature>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Commitments {
    #[prost(message, repeated, tag = "1")]
    pub commitments: ::std::vec::Vec<super::types::Commitment>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FetchHeadersAfter {
    #[prost(bytes, repeated, tag = "1")]
    pub hashes: ::std::vec::Vec<std::vec::Vec<u8>>,
    #[prost(bytes, tag = "2")]
    pub stopping_hash: std::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FetchMmrNodeCount {
    #[prost(enumeration = "MmrTree", tag = "1")]
    pub tree: i32,
    #[prost(uint64, tag = "2")]
    pub height: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FetchMmrNodes {
    #[prost(enumeration = "MmrTree", tag = "1")]
    pub tree: i32,
    #[prost(uint32, tag = "2")]
    pub pos: u32,
    #[prost(uint32, tag = "3")]
    pub count: u32,
    #[prost(uint64, tag = "4")]
    pub hist_height: u64,
}
